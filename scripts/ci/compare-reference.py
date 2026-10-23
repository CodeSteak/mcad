#!/usr/bin/env python3
"""Compares two OpenSCAD reference files (tests/openscad-compat/reference.json format).

    python3 scripts/ci/compare-reference.py CHECKED_IN.json FRESH.json

CI regenerates the reference with the installed OpenSCAD (make-reference.py) and checks
that the checked-in file still describes what OpenSCAD 2021.01 produces: same files, source
hashes, classes, defines and echo lines; volume and area within 1e-6 relative, bounds within
1e-6 mm, triangle counts equal. Exit code 0 if equal, 1 otherwise.
"""

from __future__ import annotations

import json
import sys

REL = 1e-6
ABS = 1e-6
EXACT = ("source_fnv1a64", "class", "defines", "echo", "triangles")


def close(a: float, b: float) -> bool:
    return abs(a - b) <= max(REL * max(abs(a), abs(b)), ABS)


def main() -> int:
    if len(sys.argv) != 3:
        print(__doc__, file=sys.stderr)
        return 2
    old = json.load(open(sys.argv[1], encoding="utf-8"))
    new = json.load(open(sys.argv[2], encoding="utf-8"))
    diffs: list[str] = []
    if old.get("openscad_version") != new.get("openscad_version"):
        diffs.append(f"openscad_version: {old.get('openscad_version')!r} != {new.get('openscad_version')!r}")
    a, b = old["files"], new["files"]
    for name in sorted(set(a) ^ set(b)):
        diffs.append(f"{name}: only in {'checked-in' if name in a else 'fresh'} reference")
    for name in sorted(set(a) & set(b)):
        x, y = a[name], b[name]
        for k in EXACT:
            if x.get(k) != y.get(k):
                diffs.append(f"{name}: {k} {x.get(k)!r} != {y.get(k)!r}")
        for k in ("volume", "area"):
            if not close(x[k], y[k]):
                diffs.append(f"{name}: {k} {x[k]} != {y[k]}")
        for side in ("min", "max"):
            for i, (p, q) in enumerate(zip(x["bounds"][side], y["bounds"][side])):
                if abs(p - q) > ABS:
                    diffs.append(f"{name}: bounds.{side}[{i}] {p} != {q}")
    for d in diffs:
        print(d)
    print(f"reference: {len(a)} files checked in, {len(b)} regenerated, {len(diffs)} difference(s)")
    return 1 if diffs else 0


if __name__ == "__main__":
    sys.exit(main())
