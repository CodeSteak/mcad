#!/usr/bin/env python3
"""Stage 2 of the OpenSCAD comparison: runs our CLI on every model in
examples/openscad-compat/ and compares volume, surface area and bounding box with
tests/openscad-compat/reference.json (built by make-reference.py from OpenSCAD 2021.01).

Commands (templates; placeholders {cad} {src} {out} {name} {stem} {ref_stl}):
  export:  $CAD_EXPORT_CMD, default "{cad} export {src} -o {out}"   -> STL
  eval:    $CAD_EVAL_CMD,   default "{cad} eval {src} --json"       -> JSON (optional, --eval)
  {cad} is $MCAD (old name $CAD; default "mcad"), or --cad.

With --eval the JSON of `mcad eval --json` is read as well:
  - echo lines: key "echoes", a list of strings or of objects with "text"; compared
    with OpenSCAD's ECHO lines.
  - with --metrics eval: volume/area/bounds come from the first object in the JSON that
    has a "volume" key ("area", "bounds": {"min": [..], "max": [..]} or [[..], [..]]),
    instead of from the exported STL.

Tolerances (OpenSCAD facets, we are exact):
  bounds   0.01 mm per coordinate.
  volume and area, relative:
    1 % for class "polygonal" (no round features, or only `$fn` = 3…12 directly at the
        call: both sides build the same polygons, differences are numerical only),
    2 % for class "rounded" (OpenSCAD with $fn = 128 against exact B-rep geometry).
  At $fn = 128 OpenSCAD's facetting error is small: inscribed polygons lose
  1 - sin(2π/n)/(2π/n) ≈ 0.04 % of a cylinder's volume, spheres about 0.1 % (measured
  on 02/03/13 against the analytic volumes). It grows where a difference removes most
  of the volume, and our own STL export is tessellated as well; 1 %/2 % leave a margin
  of more than 10x, while a wrong dimension of a main feature still shows. Bounds move
  by at most r·(1 - cos(π/128)) ≈ 3e-4·r (0.006 mm at r = 20), below 0.01 mm.
  The class is stored per file in reference.json (see stl_stats.classify).

Output: a table on stdout and a JSON report (--report, default
target/openscad-compat/compare-report.json). Exit code 0 if every file matches, 1 if
any file deviates or a command fails, 2 for setup errors.

Self-test with the mock CLI (copies OpenSCAD's own STL, optionally distorted):
  python3 tests/openscad-compat/compare.py --self-test
"""

from __future__ import annotations

import argparse
import json
import os
import shlex
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import stl_stats  # noqa: E402

ROOT = HERE.parent.parent
MODELS = ROOT / "examples" / "openscad-compat"
REF = HERE / "reference.json"
REF_STL = ROOT / "target" / "openscad-compat"
OUT = ROOT / "target" / "openscad-compat" / "ours"

BOUNDS_TOL = 0.01
REL_TOL = {"polygonal": 0.01, "rounded": 0.02}

DEFAULT_EXPORT = "{cad} export {src} -o {out}"
DEFAULT_EVAL = "{cad} eval {src} --json"


def fill(template: str, **kw: str) -> list[str]:
    q = {k: shlex.quote(v) if k != "cad" else v for k, v in kw.items()}
    return shlex.split(template.format(**q))


def run(cmd: list[str], timeout: float) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, capture_output=True, text=True, timeout=timeout, cwd=ROOT)


def find_metrics(obj):
    if isinstance(obj, dict):
        if "volume" in obj:
            return obj
        for v in obj.values():
            r = find_metrics(v)
            if r is not None:
                return r
    elif isinstance(obj, list):
        for v in obj:
            r = find_metrics(v)
            if r is not None:
                return r
    return None


def norm_bounds(b) -> dict:
    if isinstance(b, dict):
        return {"min": list(b["min"]), "max": list(b["max"])}
    return {"min": list(b[0]), "max": list(b[1])}


def echo_lines(data) -> list[str] | None:
    e = data.get("echoes") if isinstance(data, dict) else None
    if e is None:
        return None
    return [x["text"] if isinstance(x, dict) else str(x) for x in e]


def rel(a: float, b: float) -> float:
    return abs(a - b) / max(abs(b), 1e-9)


def compare_file(name: str, ref: dict, args) -> dict:
    src = MODELS / name
    stem = src.stem
    out = OUT / f"{stem}.stl"
    res = {"file": name, "class": ref["class"], "status": "ok", "problems": []}
    kw = dict(cad=args.cad, src=str(src), out=str(out), name=name, stem=stem, ref_stl=str(REF_STL / f"{stem}.stl"))
    metrics = None
    t0 = time.time()

    if args.eval:
        cmd = fill(args.eval_cmd, **kw)
        try:
            p = run(cmd, args.timeout)
        except Exception as e:  # noqa: BLE001
            p = None
            res["problems"].append(f"eval: {e}")
        if p is not None:
            if p.returncode != 0:
                res["problems"].append(f"eval exit {p.returncode}: {p.stderr.strip()[:300]}")
            else:
                try:
                    data = json.loads(p.stdout)
                    ours = echo_lines(data)
                    if ours is not None and ours != ref["echo"]:
                        res["problems"].append(f"echo differs: ours {ours}, openscad {ref['echo']}")
                    if args.metrics == "eval":
                        metrics = find_metrics(data)
                        if metrics is None:
                            res["problems"].append("eval JSON has no object with `volume`")
                except json.JSONDecodeError as e:
                    res["problems"].append(f"eval output is not JSON: {e}")

    if args.metrics == "export":
        out.parent.mkdir(parents=True, exist_ok=True)
        if out.exists():
            out.unlink()
        cmd = fill(args.export_cmd, **kw)
        try:
            p = run(cmd, args.timeout)
            if p.returncode != 0:
                res["problems"].append(f"export exit {p.returncode}: {p.stderr.strip()[:300]}")
            elif not out.exists():
                res["problems"].append("export wrote no file")
            else:
                metrics = stl_stats.mesh_stats(stl_stats.read_stl(out))
        except Exception as e:  # noqa: BLE001
            res["problems"].append(f"export: {e}")
    res["seconds"] = round(time.time() - t0, 3)

    if metrics is not None:
        ours_b = norm_bounds(metrics["bounds"])
        dev = max(
            max(abs(a - b) for a, b in zip(ours_b["min"], ref["bounds"]["min"])),
            max(abs(a - b) for a, b in zip(ours_b["max"], ref["bounds"]["max"])),
        )
        tol = REL_TOL[ref["class"]]
        dv = rel(metrics["volume"], ref["volume"])
        da = rel(metrics["area"], ref["area"]) if "area" in metrics else None
        res["ours"] = {"volume": metrics["volume"], "area": metrics.get("area"), "bounds": ours_b,
                       "triangles": metrics.get("triangles")}
        res["reference"] = {k: ref[k] for k in ("volume", "area", "bounds", "triangles")}
        res["deviation"] = {"volume_rel": dv, "area_rel": da, "bounds_abs": dev}
        res["tolerance"] = {"volume_rel": tol, "area_rel": tol, "bounds_abs": BOUNDS_TOL}
        if dev > BOUNDS_TOL:
            res["problems"].append(f"bounds off by {dev:.4f} mm (> {BOUNDS_TOL})")
        if dv > tol:
            res["problems"].append(f"volume off by {dv * 100:.3f} % (> {tol * 100:g} %)")
        if da is not None and da > tol:
            res["problems"].append(f"area off by {da * 100:.3f} % (> {tol * 100:g} %)")
    elif not res["problems"]:
        res["problems"].append("no metrics")
    if res["problems"]:
        res["status"] = "FAIL"
    return res


def table(results: list[dict]) -> str:
    head = f"{'file':30} {'class':9} {'V openscad':>12} {'V ours':>12} {'dV %':>7} {'dA %':>7} {'dBox mm':>8}  status"
    lines = [head, "-" * len(head)]
    for r in results:
        d = r.get("deviation")
        if d:
            da = f"{d['area_rel'] * 100:7.3f}" if d["area_rel"] is not None else "      -"
            lines.append(
                f"{r['file']:30} {r['class']:9} {r['reference']['volume']:12.3f} {r['ours']['volume']:12.3f} "
                f"{d['volume_rel'] * 100:7.3f} {da} {d['bounds_abs']:8.4f}  {r['status']}"
            )
        else:
            lines.append(f"{r['file']:30} {r['class']:9} {'':12} {'':12} {'':7} {'':7} {'':8}  {r['status']}")
        for p in r["problems"]:
            lines.append(f"    {p}")
    ok = sum(r["status"] == "ok" for r in results)
    lines.append(f"{ok}/{len(results)} files match")
    return "\n".join(lines)


def parse_args(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--cad", default=os.environ.get("MCAD", os.environ.get("CAD", "mcad")), help="CLI command (env MCAD or CAD, default `mcad`)")
    ap.add_argument("--export-cmd", default=os.environ.get("CAD_EXPORT_CMD", DEFAULT_EXPORT))
    ap.add_argument("--eval-cmd", default=os.environ.get("CAD_EVAL_CMD", DEFAULT_EVAL))
    ap.add_argument("--eval", action="store_true", help="also run the eval command (echo comparison)")
    ap.add_argument("--metrics", choices=["export", "eval"], default="export",
                    help="take volume/area/bounds from the exported STL (default) or from eval --json")
    ap.add_argument("--report", type=Path, default=ROOT / "target" / "openscad-compat" / "compare-report.json")
    ap.add_argument("--timeout", type=float, default=300.0)
    ap.add_argument("--self-test", action="store_true", help="check this script against the mock CLI")
    ap.add_argument("files", nargs="*", help="only these model names (default: all in reference.json)")
    return ap.parse_args(argv)


def main(argv=None) -> int:
    args = parse_args(argv)
    if args.self_test:
        return self_test()
    if not REF.exists():
        print(f"{REF} missing; run make-reference.py", file=sys.stderr)
        return 2
    ref = json.loads(REF.read_text())
    names = args.files or list(ref["files"])
    unknown = [n for n in names if n not in ref["files"]]
    if unknown:
        print(f"not in reference.json: {unknown}", file=sys.stderr)
        return 2
    for n in names:
        if stl_stats.fnv1a64((MODELS / n).read_bytes()) != ref["files"][n]["source_fnv1a64"]:
            print(f"{n}: reference is stale, run make-reference.py", file=sys.stderr)
            return 2
    results = [compare_file(n, ref["files"][n], args) for n in names]
    print(table(results))
    report = {
        "settings": {
            "export_cmd": args.export_cmd if args.metrics == "export" else None,
            "eval_cmd": args.eval_cmd if args.eval else None,
            "cad": args.cad,
            "metrics": args.metrics,
            "bounds_tol_mm": BOUNDS_TOL,
            "rel_tol": REL_TOL,
            "openscad": ref.get("openscad_version"),
        },
        "summary": {"files": len(results), "ok": sum(r["status"] == "ok" for r in results),
                    "failed": [r["file"] for r in results if r["status"] != "ok"]},
        "results": results,
    }
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(report, indent=1) + "\n")
    print(f"report: {args.report}")
    return 0 if not report["summary"]["failed"] else 1


def self_test() -> int:
    """Runs the comparison against mock_cad.py: identical STLs must pass, distorted fail."""
    mock = f"{shlex.quote(sys.executable)} {shlex.quote(str(HERE / 'mock_cad.py'))}"
    missing = [n for n in json.loads(REF.read_text())["files"] if not (REF_STL / (Path(n).stem + ".stl")).exists()]
    if missing:
        print(f"reference STLs missing under {REF_STL} (run make-reference.py): {missing[:3]}…", file=sys.stderr)
        return 2
    rep = ROOT / "target" / "openscad-compat" / "selftest-report.json"
    cases = [
        ("identical STL, echo from reference", {}, ["--eval"], 0),
        ("volume +1.5 % (scale 1.005)", {"MOCK_CAD_SCALE": "1.005"}, [], 1),
        ("one file shifted by 0.02 mm", {"MOCK_CAD_SHIFT": "0.02", "MOCK_CAD_ONLY": "19_hex_nut"}, [], 1),
        ("wrong echo", {"MOCK_CAD_ECHO": "x"}, ["--eval", "06_hole_grid.scad"], 1),
        ("export fails", {"MOCK_CAD_FAIL": "1"}, ["01_cube_variants.scad"], 1),
        ("metrics from eval --json", {}, ["--eval", "--metrics", "eval"], 0),
    ]
    ok = True
    for title, env, extra, want in cases:
        e = dict(os.environ, MCAD=mock, **env)
        p = subprocess.run([sys.executable, __file__, "--report", str(rep), *extra], env=e, capture_output=True,
                           text=True, cwd=ROOT)
        good = p.returncode == want
        ok &= good
        last = p.stdout.strip().splitlines()[-2:] if p.stdout.strip() else [p.stderr.strip()]
        print(f"{'ok  ' if good else 'FAIL'} {title}: exit {p.returncode} (want {want}) | {' | '.join(last)}")
    print("self-test passed" if ok else "self-test FAILED")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
