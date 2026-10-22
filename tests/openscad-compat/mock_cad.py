#!/usr/bin/env python3
"""Stand-in for our CLI (`cad`) to test compare.py before the CLI exists.

  mock_cad.py export <file.scad> -o <out.stl>   copies OpenSCAD's reference STL
  mock_cad.py eval <file.scad> --json           prints echoes (and metrics) from reference.json

Distortions (environment):
  MOCK_CAD_SCALE=f   scale all vertices by f about the origin
  MOCK_CAD_SHIFT=d   move all vertices by d along x
  MOCK_CAD_ONLY=stem apply the distortion only to this model
  MOCK_CAD_ECHO=s    append an echo line s
  MOCK_CAD_FAIL=1    exit with an error
"""

from __future__ import annotations

import json
import os
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import stl_stats  # noqa: E402

ROOT = HERE.parent.parent
REF_STL = ROOT / "target" / "openscad-compat"


def distorted(stem: str) -> bool:
    only = os.environ.get("MOCK_CAD_ONLY")
    return not only or only == stem


def main(argv: list[str]) -> int:
    if os.environ.get("MOCK_CAD_FAIL"):
        print("mock: failing on purpose", file=sys.stderr)
        return 3
    if len(argv) >= 4 and argv[0] == "export" and argv[2] == "-o":
        src, out = Path(argv[1]), Path(argv[3])
        tris = stl_stats.read_stl(REF_STL / f"{src.stem}.stl")
        if distorted(src.stem):
            s = float(os.environ.get("MOCK_CAD_SCALE", "1"))
            d = float(os.environ.get("MOCK_CAD_SHIFT", "0"))
            tris = [tuple((v[0] * s + d, v[1] * s, v[2] * s) for v in t) for t in tris]
        stl_stats.write_ascii_stl(out, tris)
        return 0
    if len(argv) >= 3 and argv[0] == "eval" and "--json" in argv:
        src = Path(argv[1])
        ref = json.loads((HERE / "reference.json").read_text())["files"][src.name]
        echoes = list(ref["echo"])
        if os.environ.get("MOCK_CAD_ECHO"):
            echoes.append(os.environ["MOCK_CAD_ECHO"])
        geo = {k: ref[k] for k in ("volume", "area", "bounds")}
        print(json.dumps({"diagnostics": [], "echoes": [{"text": e} for e in echoes], "geometry": geo}))
        return 0
    print(f"mock: unsupported arguments {argv}", file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
