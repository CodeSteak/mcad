#!/usr/bin/env python3
"""Builds the OpenSCAD reference data for examples/openscad-compat/*.scad.

For every file:
  openscad -o target/openscad-compat/<name>.stl  [-D '$fn=128'] <file>
  openscad -o target/openscad-compat/<name>.echo [-D '$fn=128'] <file>
then volume (divergence theorem), surface area, bounding box and triangle count of the
STL plus the ECHO lines go into tests/openscad-compat/reference.json. Only the JSON is
checked in; STLs and echo files stay under target/ (gitignored).

`$fn` rule: `-D '$fn=128'` is passed when the file has round features (sphere, cylinder,
circle without `$fn` = 3…12 at the call, rotate_extrude, twisted linear_extrude) and
does NOT assign `$fn`/`$fa`/`$fs` at top level. A `$fn` argument at a call
(`cylinder(d = 9, $fn = 6)`) overrides the global value in OpenSCAD, so hexagons stay
hexagons while plain holes become smooth. A file that sets `$fn` globally is exported
as written (the -D value would override its own assignment).

Usage: python3 tests/openscad-compat/make-reference.py [--openscad PATH] [--fn 128] [files…]
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import stl_stats  # noqa: E402

ROOT = HERE.parent.parent
MODELS = ROOT / "examples" / "openscad-compat"
OUT = ROOT / "target" / "openscad-compat"
REF = HERE / "reference.json"


def run_openscad(openscad: str, src: Path, out: Path, defines: list[str]) -> str:
    cmd = [openscad, "-o", str(out)]
    for d in defines:
        cmd += ["-D", d]
    cmd.append(str(src))
    env = dict(os.environ, QT_QPA_PLATFORM=os.environ.get("QT_QPA_PLATFORM", "offscreen"))
    p = subprocess.run(cmd, capture_output=True, text=True, env=env, timeout=600)
    if p.returncode != 0 or not out.exists():
        raise RuntimeError(f"{' '.join(cmd)} failed ({p.returncode}):\n{p.stderr}")
    return p.stderr


def openscad_version(openscad: str) -> str:
    p = subprocess.run([openscad, "--version"], capture_output=True, text=True)
    return (p.stdout + p.stderr).strip()


def build(openscad: str, src: Path, fn: int) -> dict:
    text = src.read_text()
    cls = stl_stats.classify(text)
    defines = []
    if cls["round_features"] and not cls["global_fn"]:
        defines.append(f"$fn={fn}")
    OUT.mkdir(parents=True, exist_ok=True)
    stl = OUT / (src.stem + ".stl")
    echo = OUT / (src.stem + ".echo")
    log = run_openscad(openscad, src, stl, defines)
    run_openscad(openscad, src, echo, defines)
    stats = stl_stats.mesh_stats(stl_stats.read_stl(stl))
    echo_lines = [l[len("ECHO: ") :] for l in echo.read_text().splitlines() if l.startswith("ECHO: ")]
    warnings = [l for l in log.splitlines() if l.startswith(("WARNING", "ERROR"))]
    return {
        "source_fnv1a64": stl_stats.fnv1a64(src.read_bytes()),
        "class": cls["class"],
        "round_features": cls["round_features"],
        "defines": defines,
        **{k: stats[k] for k in ("volume", "area", "bounds", "triangles")},
        "echo": echo_lines,
        "openscad_warnings": warnings,
    }


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--openscad", default=os.environ.get("OPENSCAD", "openscad"))
    ap.add_argument("--fn", type=int, default=128, help="$fn for files with round features (default 128)")
    ap.add_argument("files", nargs="*", type=Path, help="only these files (default: all)")
    args = ap.parse_args()

    files = [f.resolve() for f in args.files] or sorted(MODELS.glob("*.scad"))
    ref = json.loads(REF.read_text()) if REF.exists() and args.files else {"files": {}}
    ref["openscad_version"] = openscad_version(args.openscad)
    ref["fn_rule"] = (
        f"-D $fn={args.fn} if the file has round features and no top-level $fn/$fa/$fs assignment; "
        "$fn at a call always wins in OpenSCAD"
    )
    ref["units"] = "mm, mm^2, mm^3"
    failed = 0
    for f in files:
        try:
            ref["files"][f.name] = build(args.openscad, f, args.fn)
            r = ref["files"][f.name]
            print(f"{f.name:32} {r['class']:9} V={r['volume']:12.3f} A={r['area']:11.3f} tris={r['triangles']:6}"
                  f" echo={len(r['echo'])} {' '.join(r['defines'])}")
        except Exception as e:  # noqa: BLE001
            failed += 1
            print(f"{f.name}: {e}", file=sys.stderr)
    ref["files"] = dict(sorted(ref["files"].items()))
    REF.write_text(json.dumps(ref, indent=1) + "\n")
    print(f"wrote {REF.relative_to(ROOT)} ({len(ref['files'])} files, {failed} failed)")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
