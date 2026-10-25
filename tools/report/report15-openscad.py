#!/usr/bin/env python3
"""OpenSCAD comparison of `offset` for test report 15, case by case.

Every case is a 2D expression; both sides extrude it by 1 (volume = area). OpenSCAD 2021.01
exports an STL (with `-D $fn=128` for cases with round joins or circles, like
tests/openscad-compat/make-reference.py), our CLI measures volume and bounds exactly
(`cad measure --volume --bounds`). Writes a TSV: name, code, V OpenSCAD, V ours, exact formula
(if any), bounds of both.

    CAD=target/release/mcad python3 tools/report/report15-openscad.py OUT_DIR
"""
import json
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tests/openscad-compat"))
import stl_stats  # noqa: E402

L = "polygon([[0, 0], [20, 0], [20, 8], [8, 8], [8, 20], [0, 20]])"
STERN = "polygon([for (i = [0 : 9]) let(a = i * 36, r = i % 2 == 0 ? 10 : 5) [r * cos(a), r * sin(a)]])"
PLATTE = "difference() { square([30, 20]); translate([8, 10]) circle(r = 4); translate([18, 6]) square([8, 8]); }"
PI = "3.141592653589793"

# name, 2D code, exact area (Python expression or None)
CASES = [
    ("quadrat_r", "offset(r = 2) square(10);", f"14**2 - (4 - {PI}) * 4"),
    ("quadrat_r_innen", "offset(r = -2) square(10);", "36"),
    ("kreis_r", "offset(3) circle(r = 5);", f"{PI} * 64"),
    ("l_r", f"offset(r = 1.5) {L};", None),
    ("l_r_innen", f"offset(r = -1.5) {L};", None),
    ("ohne_argument", "offset() square(10);", f"144 - (4 - {PI})"),
    ("r_vor_delta", "offset(r = 2, delta = 1) square(10);", f"14**2 - (4 - {PI}) * 4"),
    ("quadrat_delta", "offset(delta = 2) square(10);", "196"),
    ("quadrat_fase", "offset(delta = 2, chamfer = true) square(10);", "196 - 4 * (2 * 2**0.5 - 2)**2"),
    ("quadrat_delta_innen", "offset(delta = -2) square(10);", "36"),
    ("stern_delta", f"offset(delta = 1) {STERN};", None),
    ("stern_fase", f"offset(delta = 1, chamfer = true) {STERN};", None),
    ("spitzes_dreieck", "offset(delta = 1) polygon([[0, 0], [10, 0], [0, 1]]);", None),
    ("l_delta", f"offset(delta = 1.5) {L};", None),
    ("l_delta_innen", f"offset(delta = -1.5) {L};", None),
    ("l_fase_innen", f"offset(delta = -1.5, chamfer = true) {L};", None),
    ("platte", PLATTE, None),
    ("platte_r", f"offset(r = 1) {PLATTE}", None),
    ("platte_r_innen", f"offset(r = -1) {PLATTE}", None),
    ("loch_schliesst", f"offset(r = 4.5) {PLATTE}", None),
    ("platte_delta", f"offset(delta = 1) {PLATTE}", None),
    ("zusammenwachsen", "offset(r = 1.5) { square([10, 10]); translate([12, 0]) square([10, 10]); }", None),
    ("steg_verschwindet", "offset(r = -1) union() { square([10, 10]); translate([10, 4]) square([15, 1.5]); translate([25, 0]) square([10, 10]); }", None),
    ("oeffnen", "offset(r = 2) offset(r = -2) union() { square([30, 12]); translate([12, 0]) square([3, 30]); translate([0, 20]) square([1.5, 10]); }", None),
]


def main():
    out = Path(sys.argv[1])
    cases = out / "openscad"
    cases.mkdir(parents=True, exist_ok=True)
    cad = os.environ.get("CAD", str(ROOT / "target/release/mcad"))
    env = dict(os.environ, CAD_NO_DAEMON="1")
    rows = []
    for name, code, exact in CASES:
        src = f"linear_extrude(1) {code}\n"
        f = cases / f"{name}.scad"
        f.write_text(src)
        cls = stl_stats.classify(src)
        defines = [] if cls["class"] == "polygonal" else ["-D", "$fn=128"]
        stl = cases / f"{name}.stl"
        r = subprocess.run(["openscad", *defines, "-o", str(stl), str(f)], capture_output=True, text=True)
        if r.returncode != 0:
            rows.append([name, code, "nan", "nan", exact or "", "{}", "{}"])
            continue
        st = stl_stats.mesh_stats(stl_stats.read_stl(stl))
        m = subprocess.run([cad, "measure", str(f), "--volume", "--bounds", "--json"], capture_output=True, text=True, env=env)
        try:
            d = json.loads(m.stdout)
            v = d["volume"]["volume"]
            b = d["bounds"]["bounds"]
        except Exception:  # noqa: BLE001
            v, b = float("nan"), {}
        rows.append([name, code, repr(st["volume"]), repr(v), exact or "", json.dumps(st["bounds"]), json.dumps(b), cls["class"]])
    with open(out / "openscad.tsv", "w") as fh:
        for row in rows:
            fh.write("\t".join(str(x) for x in row) + "\n")
    print(f"{len(rows)} cases -> {out / 'openscad.tsv'}")


if __name__ == "__main__":
    main()
