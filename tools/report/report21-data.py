#!/usr/bin/env python3
"""Data of test report 21 (Plan 21: offset/hull/minkowski, STL import, collision asserts, user
feedback, rename cad → mcad). Runs the release CLI without daemon on small models and writes
values, timings, CLI outputs and images to reports/tmp/report21/data/:

  a/hull.json, a/*.png        hull()/minkowski() cases: volume (2D: extruded by 1 = area) against
                              the formula, special case or warning, time of the whole call
  b/bench.json, b/*.png       STL import: spheres with 250 … 5000 triangles as B-rep (build, build
                              + one Boolean) and as mesh, realistic exports, combined model
  c/asserts.json, c/*.png     assert cases against formulas, export exit codes, renders
  d/*.txt, d/*.png            user feedback, before ("vorher": VORHER binary, build before phase D) /
                              after ("nachher")
  e/*.txt                     rename: --version, --help, .mcad and .scad side by side

  python3 tools/report/report21-data.py [--mcad target/release/mcad] [--vorher <cad 0.2.1>]
      [--only a,b,c,d,e] [--repeat 3]
"""
import argparse
import json
import math
import os
import re
import shutil
import statistics
import struct
import subprocess
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
ap = argparse.ArgumentParser()
ap.add_argument("--mcad", default=str(ROOT / "target/release/mcad"))
ap.add_argument("--vorher", default=os.environ.get("VORHER"))
ap.add_argument("--only", default="a,b,c,d,e")
ap.add_argument("--repeat", type=int, default=3)
args = ap.parse_args()
MCAD = args.mcad
OUT = ROOT / "reports/tmp/report21/data"
ENV = dict(os.environ, MCAD_NO_DAEMON="1", CAD_NO_DAEMON="1")
pi = math.pi


def run(cmd, cwd=None):
    t0 = time.perf_counter()
    p = subprocess.run(cmd, capture_output=True, text=True, env=ENV, cwd=cwd)
    return p, (time.perf_counter() - t0) * 1000


def shown(cmd, name="mcad"):
    """Command line as shown in the report (program name, paths relative to the work dir)."""
    parts = [name] + [str(c) for c in cmd[1:]]
    return "$ " + " ".join(f"'{p}'" if re.search(r"[ ()\[\]*,]", p) else p for p in parts)


def transcript(cmd, cwd, name="mcad", show_exit=True):
    p, _ = run(cmd, cwd)
    text = shown(cmd, name) + "\n" + (p.stdout + p.stderr).rstrip()
    if show_exit:
        text += f"\n(Exit {p.returncode})"
    return text + "\n"


# ------------------------------------------------------------------------------ A: hull/minkowski
def part_a():
    out = OUT / "a"
    out.mkdir(parents=True, exist_ok=True)

    def box_ball(a, b, c, r):
        return a * b * c + 2 * r * (a * b + b * c + c * a) + pi * r * r * (a + b + c) + 4 / 3 * pi * r**3

    def belt(r1, r2, d):
        a = math.acos((r1 - r2) / d)
        t = math.sqrt(d * d - (r1 - r2) ** 2)
        return r1 * r1 * (pi - a) + r2 * r2 * a + t * (r1 + r2)

    def spheres(r1, r2, d):
        s = (r1 - r2) / d
        c = math.sqrt(1 - s * s)
        z1, z2 = r1 * s, d + r2 * s
        p1, p2 = r1 * c, r2 * c

        def cap(r, h):
            return pi * h * h * (3 * r - h) / 3

        return 4 / 3 * pi * r1**3 - cap(r1, r1 - z1) + pi * (z2 - z1) / 3 * (p1 * p1 + p1 * p2 + p2 * p2) + cap(r2, r2 - r2 * s)

    l_area = 175 + 80 * 2 + (5 * pi / 4 - 1) * 4
    tetra = "polyhedron([[0, 0, 0], [10, 0, 0], [0, 10, 0], [0, 0, 10]], [[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]])"
    cases = [
        ("2d_stadion", "hull", "Stadium r 3, L 10", "πr² + 2rL", "linear_extrude(1) hull() { circle(3); translate([10, 0]) circle(3); }", pi * 9 + 60),
        ("2d_riemen", "hull", "Circles r 5 and r 2, distance 12", "belt area", "linear_extrude(1) hull() { circle(5); translate([12, 0]) circle(2); }", belt(5, 2, 12)),
        ("2d_quadrat_kreis", "hull", "Square 10 + circle r 5 on the edge", "100 + 12.5π", "linear_extrude(1) hull() { square(10); translate([10, 5]) circle(5); }", 100 + pi * 12.5),
        ("2d_vier_kreise", "hull", "4 circles r 2 on rectangle 20×10", "24·14 − (4 − π)·4", "linear_extrude(1) hull() { for (x = [0, 20], y = [0, 10]) translate([x, y]) circle(2); }", 24 * 14 - (4 - pi) * 4),
        ("2d_text_G", "hull", "Hull of the glyph \"G\" (size 20)", "–", 'linear_extrude(1) hull() text("G", size = 20);', None),
        ("2d_mink_kreis", "minkowski", "Rectangle 10×20 ⊕ circle r 2", "A + Pr + πr²", "linear_extrude(1) minkowski() { square([10, 20]); circle(2); }", 200 + 120 + 4 * pi),
        ("2d_mink_L", "minkowski", "L profile ⊕ circle r 2 (concave corner)", "A + Pr + (5π/4 − 1)r²", "linear_extrude(1) minkowski() { polygon([[0, 0], [20, 0], [20, 5], [5, 5], [5, 20], [0, 20]]); circle(2); }", l_area),
        ("2d_mink_konvex", "minkowski", "Square 10 ⊕ triangle 4×3", "176", "linear_extrude(1) minkowski() { square(10); polygon([[0, 0], [4, 0], [0, 3]]); }", 176.0),
        ("3d_kapsel", "hull", "Capsule r 3, L 10", "πr²L + 4/3πr³", "hull() { sphere(3); translate([0, 0, 10]) sphere(3); }", pi * 9 * 10 + 36 * pi),
        ("3d_kapsel_gedreht", "hull", "Capsule rotated [30, 40, 0]", "πr²L + 4/3πr³", "rotate([30, 40, 0]) hull() { sphere(3); translate([0, 0, 10]) sphere(3); }", pi * 9 * 10 + 36 * pi),
        ("3d_kugeln_r5_r2", "hull", "Spheres r 5 and r 2, distance 10", "cap + truncated cone + cap", "hull() { sphere(5); translate([0, 0, 10]) sphere(2); }", spheres(5, 2, 10)),
        ("3d_zylinder_kuppel", "hull", "Cylinder r 3 h 10 + sphere r 3 on top", "πr²h + 2/3πr³", "hull() { cylinder(r = 3, h = 10); translate([0, 0, 10]) sphere(3); }", pi * 9 * 10 + 2 / 3 * pi * 27),
        ("3d_zwei_wuerfel", "hull", "two cubes 10 at distance 20", "30·10·10", "hull() { cube(10); translate([20, 0, 0]) cube(10); }", 3000.0),
        ("3d_wuerfel_pyramide", "hull", "Cube 10 + \"point\" 5 above the top face", "1000 + 100·5/3", "hull() { cube(10, center = true); translate([0, 0, 10]) cube(0.0001, center = true); }", 1000 + 100 * 5 / 3),
        ("3d_stadion_platte", "hull", "3 cylinders r 3 h 2", "h·(triangle + Pr + πr²)", "hull() { cylinder(r = 3, h = 2); translate([20, 0, 0]) cylinder(r = 3, h = 2); translate([0, 15, 0]) cylinder(r = 3, h = 2); }", 2 * (150 + 60 * 3 + 9 * pi)),
        ("3d_quader_zylinder", "hull", "Cuboid 10×10×5 + cylinder r 5 h 5", "h·(200 + 12.5π)", "hull() { cube([10, 10, 5]); translate([20, 5, 0]) cylinder(r = 5, h = 5); }", 5 * (100 + 100 + pi * 12.5)),
        ("3d_kugeln_quader", "hull", "8 spheres r 2 at the corners of 20×10×5", "cuboid ⊕ sphere (Steiner)", "hull() { for (x = [0, 20], y = [0, 10], z = [0, 5]) translate([x, y, z]) sphere(2); }", box_ball(20, 10, 5, 2)),
        ("3d_mink_wuerfel", "minkowski", "Cuboid 10×20×30 ⊕ sphere r 2", "Steiner", "minkowski() { cube([10, 20, 30]); sphere(2); }", box_ball(10, 20, 30, 2)),
        ("3d_mink_wuerfel_gedreht", "minkowski", "rotated cuboid ⊕ sphere r 2", "Steiner", "minkowski() { rotate([10, 20, 30]) cube([10, 20, 30]); sphere(2); }", box_ball(10, 20, 30, 2)),
        ("3d_mink_extrude_zyl", "minkowski", "Prism 10×20×5 ⊕ cylinder r 2 h 1", "6·(A + Pr + πr²)", "minkowski() { linear_extrude(5) square([10, 20]); cylinder(r = 2, h = 1); }", 6 * (200 + 120 + 4 * pi)),
        ("3d_mink_zwei_quader", "minkowski", "Cube 10 ⊕ cuboid 1×2×3", "11·12·13", "minkowski() { cube(10); cube([1, 2, 3]); }", 11 * 12 * 13.0),
        ("3d_mink_tetraeder", "minkowski", "Tetrahedron ⊕ sphere r 1", "–", f"minkowski() {{ {tetra}; sphere(1); }}", None),
        ("3d_kugeln_ungleich", "hull", "3 spheres r 3/2/1 not on one axis", "–", "hull() { sphere(3); translate([10, 0, 0]) sphere(2); translate([0, 10, 0]) sphere(1); }", None),
        ("3d_kugeln_ebene", "hull", "4 equal spheres r 2 in one plane", "–", "hull() { for (x = [0, 20], y = [0, 10]) translate([x, y, 0]) sphere(2); }", None),
    ]
    rows = []
    for name, op, what, formula, src, want in cases:
        f = out / f"{name}.mcad"
        f.write_text(src + "\n")
        times, vol, warns = [], None, []
        for _ in range(args.repeat):
            p, ms = run([MCAD, "measure", str(f), "--volume", "--json"])
            times.append(ms)
        v = json.loads(p.stdout)
        vol = (v.get("volume") or {}).get("volume")
        warns = [d["message"] for d in v.get("diagnostics", []) if d.get("severity") in ("warning", "Warning")]
        ins, _ = run([MCAD, "inspect", str(f), "--line", "1"])
        # `mcad inspect` (English): "hint: exact: solid of revolution"
        hint = next((l.split("hint:", 1)[1].strip() for l in ins.stdout.splitlines() if "hint: exact" in l), "")
        rel = (vol - want) / want if (want and vol is not None) else None
        rows.append(dict(name=name, op=op, what=what, formula=formula, src=src, want=want, got=vol, rel=rel, ms=statistics.median(times), hint=hint, warnings=warns))
        run([MCAD, "render", str(f), "-o", str(out / f"{name}.png"), "--size", "480x360", "--no-axes"])
        print(f"A {name:26} {vol!s:>20} rel={rel if rel is None else f'{rel:+.1e}'} {statistics.median(times):6.0f} ms {hint or warns[:1]}")
    (out / "hull.json").write_text(json.dumps(rows, indent=1, ensure_ascii=False))
    ex = ROOT / "tests/models/spec/hull_minkowski.mcad"
    run([MCAD, "render", str(ex), "-o", str(out / "beispiel.png"), "--views", "all"])
    run([MCAD, "render", str(ex), "-o", str(out / "beispiel_iso.png"), "--size", "1200x800", "--no-axes"])
    (out / "beispiel_check.txt").write_text(transcript([MCAD, "check", "tests/models/spec/hull_minkowski.mcad"], ROOT))


# ------------------------------------------------------------------------------ B: STL import
def sphere_tris(n, r=20):
    segs = max(6, int(math.sqrt(n / 2)))
    rings = max(3, n // (2 * segs) + 1)
    pts = [(0, 0, r)]
    for i in range(1, rings):
        th = pi * i / rings
        for j in range(segs):
            ph = 2 * pi * j / segs
            pts.append((r * math.sin(th) * math.cos(ph), r * math.sin(th) * math.sin(ph), r * math.cos(th)))
    pts.append((0, 0, -r))
    s = len(pts) - 1

    def idx(i, j):
        return 1 + (i - 1) * segs + (j % segs)

    faces = [(0, idx(1, j), idx(1, j + 1)) for j in range(segs)]
    for i in range(1, rings - 1):
        for j in range(segs):
            a, b, c, d = idx(i, j), idx(i, j + 1), idx(i + 1, j + 1), idx(i + 1, j)
            faces += [(a, d, c), (a, c, b)]
    faces += [(s, idx(rings - 1, j + 1), idx(rings - 1, j)) for j in range(segs)]
    return [[pts[i] for i in f] for f in faces]


def write_stl(fn, tris):
    with open(fn, "wb") as f:
        f.write(b"solid exported by a test".ljust(80, b" "))
        f.write(struct.pack("<I", len(tris)))
        for t in tris:
            f.write(struct.pack("<3f", 0, 0, 0))
            for v in t:
                f.write(struct.pack("<3f", *v))
            f.write(b"\0\0")


def part_b():
    out = OUT / "b"
    out.mkdir(parents=True, exist_ok=True)

    def timings(src, name):
        f = out / f"{name}.mcad"
        f.write_text(src)
        p, _ = run([MCAD, "render", "--json", str(f), "-o", str(out / f"{name}.png")], out)
        return json.loads(p.stdout)["timings"]

    def faces_of(stl):
        f = out / "hint.mcad"
        f.write_text(f'import("{stl}");\n')
        p, _ = run([MCAD, "inspect", str(f), "--line", "1"], out)
        # `mcad inspect` (English): "hint: B-rep: 8 faces from 20 triangles" or "hint: mesh: 20 triangles, 8 faces"
        h = next(l for l in p.stdout.splitlines() if "hint:" in l and "triangles" in l)
        tri = int(re.search(r"(\d+) triangles", h).group(1))
        fl = int(re.search(r"(\d+) faces", h).group(1))
        return tri, fl, ("mesh" if "mesh:" in h else "solid")

    rows = []
    for n in [250, 500, 750, 1000, 1500, 2000, 3000, 5000, 10000, 20000]:
        stl = f"kugel{n}.stl"
        write_stl(out / stl, sphere_tris(n))
        tri, fl, auto = faces_of(stl)
        row = dict(n=n, triangles=tri, faces=fl, auto=auto)
        for mode in ["solid", "mesh"]:
            if mode == "solid" and n > 5000:
                continue
            rep = args.repeat if (mode == "mesh" or fl < 2500) else 1
            a = [timings(f'import("{stl}", as = "{mode}");\n', f"bs_{n}_{mode}") for _ in range(rep)]
            b = [timings(f'difference() {{\n  import("{stl}", as = "{mode}");\n  cylinder(h = 60, r = 5, center = true);\n}}\n', f"bd_{n}_{mode}") for _ in range(rep)]
            med = lambda xs, k: statistics.median(x[k] for x in xs)  # noqa: E731
            row[mode] = dict(eval_ms=med(a, "eval_ms"), kernel_ms=med(a, "kernel_ms"), bool_ms=med(b, "kernel_ms"), render_ms=med(a, "render_ms"), runs=rep)
        rows.append(row)
        print(f"B {n:6} tri={tri} faces={fl} auto={auto} " + " ".join(f"{m}: {row[m]['kernel_ms']:.0f}/{row[m]['bool_ms']:.0f} ms" for m in ("solid", "mesh") if m in row))
    real = []
    for m, src in [("platte", "difference() {\n  cube([60, 40, 5]);\n  for (x = [8, 22, 38, 52], y = [8, 32]) translate([x, y, -1]) cylinder(h = 7, r = 2);\n}\n"), ("gehaeuse", (ROOT / "reports/tmp/report21/phase-b/gehaeuse.scad").read_text())]:
        (out / f"{m}_quelle.mcad").write_text(src)
        run([MCAD, "export", f"{m}_quelle.mcad", "-o", f"{m}.stl"], out)
        tri, fl, auto = faces_of(f"{m}.stl")
        a = timings(f'import("{m}.stl", as = "solid");\n', f"r_{m}")
        b = timings(f'difference() {{\n  import("{m}.stl", as = "solid");\n  translate([30, 15, -5]) cylinder(h = 40, r = 6);\n}}\n', f"boolean_{m}")
        real.append(dict(model=m, triangles=tri, faces=fl, auto=auto, kernel_ms=a["kernel_ms"], bool_ms=b["kernel_ms"]))
    # combined model: housing from STL (B-rep) with bores, "board" as a 20000-triangle mesh, lid
    shutil.copy(ROOT / "reports/tmp/report21/phase-b/files/gehaeuse.stl", out / "gehaeuse_kombi.stl")
    kombi = (ROOT / "reports/tmp/report21/phase-b/kombi.scad").read_text().replace('"gehaeuse.stl"', '"gehaeuse_kombi.stl"')
    (out / "kombi.mcad").write_text(kombi)
    run([MCAD, "render", "kombi.mcad", "-o", "kombi_schnitt.png", "--section", "y=10", "--section-flip", "--size", "1000x700", "--no-axes"], out)
    run([MCAD, "render", "kombi.mcad", "-o", "kombi_xray.png", "--xray", "--size", "1000x700", "--no-axes"], out)
    (out / "kombi_measure.txt").write_text(transcript([MCAD, "measure", "kombi.mcad", "--volume", "--interference", "--clearance", "platine", "gehaeuse"], out))
    shutil.copy(ROOT / "tests/models/stl/winkel.stl", out / "winkel.stl")
    ex = (ROOT / "tests/models/spec/stl_import.mcad").read_text().replace("../stl/winkel.stl", "winkel.stl")
    (out / "stl_import.mcad").write_text(ex)
    run([MCAD, "render", "stl_import.mcad", "-o", "beispiel.png", "--size", "1000x700", "--no-axes"], out)
    (out / "stl_import_inspect.txt").write_text(transcript([MCAD, "inspect", "stl_import.mcad", "--line", "8"], out, show_exit=False) + transcript([MCAD, "inspect", "stl_import.mcad", "--line", "13"], out, show_exit=False))
    (out / "netz.mcad").write_text('color("orange") import("kugel20000.stl");\n')
    run([MCAD, "render", "netz.mcad", "-o", "netz_kugel20000.png", "--size", "700x600", "--no-axes"], out)
    (out / "solid_bool.mcad").write_text('color("blue") difference() { import("kugel1000.stl", as = "solid"); cylinder(h = 60, r = 5, center = true); }\n')
    run([MCAD, "render", "solid_bool.mcad", "-o", "solid_kugel1000_boolean.png", "--size", "700x600", "--no-axes"], out)
    (out / "bench.json").write_text(json.dumps(dict(spheres=rows, real=real), indent=1))
    for f in out.glob("kugel*.stl"):
        f.unlink()


# ------------------------------------------------------------------------------ C: asserts
def part_c():
    out = OUT / "c"
    out.mkdir(parents=True, exist_ok=True)
    shutil.copy(ROOT / "tests/models/stl/winkel.stl", out / "winkel.stl")
    plate = "cube([40, 30, 3]);\n"
    cases = [
        ("K1", "keepout: cuboid protrudes into the plate", f"{plate}assert_keepout() translate([10, 10, 2]) cube([10, 10, 5]);", "overlap", "volume", 100.0, "10·10·1"),
        ("K2", "keepout: clearance 0.5, distance 0.3", f"{plate}assert_keepout(clearance = 0.5) translate([10, 10, 3.3]) cube(5);", "too_close", "distance", 0.3, "3.3 − 3"),
        ("K3", "keepout: clearance 0.5, distance 1", f"{plate}assert_keepout(clearance = 0.5) translate([10, 10, 4]) cube(5);", None, None, None, "distance 1 > 0.5"),
        ("K4", "keepout: touching at clearance 0", f"{plate}assert_keepout() translate([10, 10, 3]) cube(5);", None, None, None, "distance 0"),
        ("K5", "keepout: sphere r 2 half in the plate", f"{plate}assert_keepout() translate([20, 15, 3]) sphere(2);", "overlap", "volume", 2 / 3 * pi * 8, "2/3·π·r³"),
        ("K6", "keepout: STL as mesh in the plate", f'{plate}assert_keepout() translate([5, 5, 2]) import("winkel.stl", as = "mesh");', "overlap", "volume", None, "mesh: yes/no only"),
        ("K7", "keepout: two parts, only b in the keep-out volume", "module t(x) translate([x, 0, 0]) cube(10);\na: t(0);\nb: t(20);\nassert_keepout() translate([25, 5, 5]) cube(10);", "overlap", "volume", 125.0, "5·5·5"),
        ("D1", "disjoint: overlapping", "assert_disjoint() { cube(10); translate([8, 0, 0]) cube(10); }", "overlap", "volume", 200.0, "2·10·10"),
        ("D2", "disjoint: touching", "assert_disjoint() { cube(10); translate([10, 0, 0]) cube(10); }", None, None, None, "touching allowed"),
        ("D3", "disjoint: cylinder half in the cuboid", "assert_disjoint() { cube(10); translate([10, 5, 0]) cylinder(h = 10, r = 2); }", "overlap", "volume", pi * 4 * 10 / 2, "π·r²·h/2"),
        ("I1", "intersect: chain a–b–c", "assert_intersect() { cube(10); translate([8, 0, 0]) cube(10); translate([16, 0, 0]) cube(10); }", None, None, None, "connected"),
        ("I2", "intersect: fourth child 14 mm away", "assert_intersect() { cube(10); translate([8, 0, 0]) cube(10); translate([16, 0, 0]) cube(10); translate([40, 0, 0]) cube(10); }", "apart", "distance", 14.0, "40 − 26"),
        ("I3", "intersect: touching only", "assert_intersect() { cube(10); translate([10, 0, 0]) cube(10); }", "apart", "distance", 0.0, "touching ≠ intersection"),
        ("T1", "touches: face 5×5", "assert_touches() { cube(10); translate([10, 2, 0]) cube([10, 5, 5]); }", None, "contact_area", 25.0, "5·5"),
        ("T2", "touches: cylinder r 3 on face", "assert_touches() { cube(10); translate([5, 5, 10]) cylinder(h = 5, r = 3); }", None, "contact_area", pi * 9, "π·r²"),
        ("T3", "touches: lid on housing rim", "module g() difference() { cube([60, 40, 25]); translate([2, 2, 2]) cube([56, 36, 25]); }\nassert_touches() { g(); translate([0, 0, 25]) cube([60, 40, 2]); }", None, "contact_area", 60 * 40 - 56 * 36, "60·40 − 56·36"),
        ("T4", "touches: lying cylinder (line)", "assert_touches() { cube([30, 30, 3]); translate([5, 15, 8]) rotate([0, 90, 0]) cylinder(h = 20, r = 5); }", "line_contact", None, None, "line contact"),
        ("T5", 'touches: the same, contact = "any"', 'assert_touches(contact = "any") { cube([30, 30, 3]); translate([5, 15, 8]) rotate([0, 90, 0]) cylinder(h = 20, r = 5); }', None, None, None, "line allowed"),
        ("T6", "touches: sphere on plate (point)", "assert_touches() { cube([30, 30, 3]); translate([15, 15, 8]) sphere(5); }", "point_contact", None, None, "point contact"),
        ("T7", "touches: gap 0.2", "assert_touches() { cube(10); translate([10.2, 0, 0]) cube(10); }", "gap", "distance", 0.2, "10.2 − 10"),
        ("T8", "touches: gap 0.2 at tol 0.3", "assert_touches(tol = 0.3) { cube(10); translate([10.2, 0, 0]) cube(10); }", None, None, None, "within tol"),
        ("T9", "touches: overlap 1 mm", "assert_touches() { cube(10); translate([9, 0, 0]) cube(10); }", "overlap", "volume", 100.0, "1·10·10"),
        ("T10", "touches: cuboids edge to edge", "assert_touches() { cube(10); translate([10, 10, 0]) cube(10); }", "line_contact", None, None, "edge to edge"),
    ]
    rows = []
    for cid, title, src, prob, key, want, formula in cases:
        f = out / f"{cid}.mcad"
        f.write_text(src + "\n")
        ms_all, ms_assert = [], []
        for _ in range(args.repeat):
            p, ms = run([MCAD, "check", str(f), "--json"])
            j = json.loads(p.stdout)
            ms_all.append(ms)
            ms_assert.append(j["asserts"][0]["ms"])
        a = j["asserts"][0]
        bad = [q for q in a["pairs"] if not q["ok"]]
        q = (bad or a["pairs"] or [None])[0]
        got = q["problem"] if q and not q["ok"] else None
        val = q.get(key) if (key and q) else None
        ok = got == prob and (want is None or (val is not None and abs(val - want) <= max(1e-5, 1e-6 * abs(want))))
        rows.append(dict(id=cid, title=title, src=src, expected=prob, problem=got, key=key, value=val, want=want, formula=formula, rel=(None if (val is None or not want) else (val - want) / want),
                         ms_assert=statistics.median(ms_assert), ms_total=statistics.median(ms_all), ok=ok, message=q["message"] if q else "", contact=q.get("contact") if q else None, exact=q.get("contact_exact") if q else None))
        print(f"C {cid:4} {'ok ' if ok else 'BAD'} {title:40} {got} {key}={val}")
    (out / "asserts.json").write_text(json.dumps(rows, indent=1, ensure_ascii=False))
    # example model, violation, export exit codes, renders
    spec = (ROOT / "tests/models/spec/asserts.mcad").read_text().replace("../stl/winkel.stl", "winkel.stl")
    (out / "asserts.mcad").write_text(spec)
    bad = spec.replace("luft = 1;", "luft = 3;").replace("cube([10, 10, 12])", "cube([10, 10, 25])")
    (out / "asserts_bad.mcad").write_text(bad)
    shutil.copy(ROOT / "reports/tmp/report21/phase-c/cli/faelle.scad", out / "faelle.mcad")
    txt = transcript([MCAD, "check", "asserts.mcad"], out) + "\n" + transcript([MCAD, "check", "asserts_bad.mcad"], out) + "\n"
    txt += transcript([MCAD, "export", "asserts_bad.mcad", "-o", "x.stl"], out) + "\n" + transcript([MCAD, "export", "asserts_bad.mcad", "-o", "x.stl", "--ignore-asserts"], out)
    (out / "export.txt").write_text(txt)
    (out / "outline.txt").write_text(transcript([MCAD, "outline", "asserts.mcad"], out, show_exit=False))
    for theme in ["hell", "dunkel"]:
        extra = ["--theme", "dark"] if theme == "dunkel" else []
        run([MCAD, "render", "asserts.mcad", "-o", f"asserts-ok-{theme}.png", "--show", "asserts", "--size", "1000x750", "--no-axes", *extra], out)
        run([MCAD, "render", "asserts_bad.mcad", "-o", f"asserts-verletzt-{theme}.png", "--xray", "--size", "1000x750", "--no-axes", *extra], out)
        run([MCAD, "render", "faelle.mcad", "-o", f"faelle-{theme}.png", "--show", "asserts", "--size", "1200x800", "--no-axes", *extra], out)
    # the larger plate from phase C: timings cold and through the daemon
    t = []
    for _ in range(args.repeat):
        p, ms = run([MCAD, "check", "asserts.mcad", "--json"], out)
        t.append(dict(total=ms, asserts={a["kind"] if "kind" in a else str(i): a["ms"] for i, a in enumerate(json.loads(p.stdout)["asserts"])}))
    (out / "asserts_times.json").write_text(json.dumps(t, indent=1))


# ------------------------------------------------------------------------------ D: user feedback
def part_d():
    out = OUT / "d"
    out.mkdir(parents=True, exist_ok=True)
    src = ROOT / "reports/tmp/report21/phase-d"
    for n in ["dom_rippe", "teil", "halter"]:
        shutil.copy(src / f"{n}.scad", out / f"{n}.scad")
    (out / "pair.scad").write_text("module block(w = 10) cube([w, 10, 10]);\na: block();\ntranslate([10, 0, 0]) b: block();\n")
    (out / "slot.scad").write_text("difference() {\n    cube([40, 20, 4]);\n    translate([0, 0, -1]) linear_extrude(6) slot([10, 10], [30, 10], 3);\n}\n")
    shutil.copy(ROOT / "tests/models/spec/fillet_zonen.mcad", out / "fillet_zonen.scad")
    for n in ["f09a_absturz_tangentialer_dom", "f08c_differenz_torus_tangential"]:
        shutil.copy(ROOT / f"tests/alltag/fehler/{n}.mcad", out / f"{n}.scad")
    cmds = {
        "d1": [["check", "dom_rippe.scad"], ["check", "f09a_absturz_tangentialer_dom.scad"], ["check", "f08c_differenz_torus_tangential.scad"]],
        "d2": [["measure", "teil.scad", "-D", "w=40", "--volume", "all"], ["check", "teil.scad", "-D", "breite=3"]],
        "d3": [["export", "pair.scad", "-o", "pair.step"]],
        "d3m": [["export", "pair.scad", "-o", "pair_merge.step", "--merge"]],
        "d5": [["measure", "slot.scad", "--volume", "all", "--section", "z=2"]],
        "d7": [["export", "halter.scad", "-m", "halter(30, h = 5)", "-o", "halter.stl"], ["export", "halter.scad", "-m", "teile.schraube(d = 3)", "-o", "schraube.stl"], ["check", "halter.scad", "-m", "halter(x = 1)"]],
    }
    bins = [("nachher", MCAD, "mcad")] + ([("vorher", args.vorher, "cad")] if args.vorher else [])
    for label, binary, name in bins:
        for key, lst in cmds.items():
            if label == "vorher" and key == "d3m":
                continue
            txt = ""
            for c in lst:
                txt += transcript([binary, *c], out, name=name)
                if "-o" in c and c[c.index("-o") + 1].endswith(".step"):
                    f = out / c[c.index("-o") + 1]
                    if f.exists():
                        names = re.findall(r"MANIFOLD_SOLID_BREP\('([^']*)'", f.read_text(errors="replace"))
                        txt += f"# STEP: {len(names)} MANIFOLD_SOLID_BREP {names}\n"
            (out / f"{key}-{label}.txt").write_text(txt)
        p, _ = run([binary, "check", "dom_rippe.scad", "--json"], out)
        (out / f"d1-{label}.json").write_text(p.stdout)
        p, _ = run([binary, "inspect", "fillet_zonen.scad", "--line", "12", "--json"], out)
        try:
            j = json.loads(p.stdout)
            n = j["nodes"][0]
            se = n.get("selected_edges")
            summary = dict(schema_version=j.get("schema_version"), selected_edges=(f"list of {len(se)} polylines with {sum(len(x) for x in se)} points" if isinstance(se, list) else se), selected_edge_count=n.get("selected_edge_count"))
        except Exception as e:  # noqa: BLE001
            summary = dict(error=str(e))
        (out / f"d6-{label}.json").write_text(json.dumps(summary, ensure_ascii=False))
        run([binary, "render", "dom_rippe.scad", "-o", f"dom_rippe-{label}.png", "--size", "900x650", "--no-axes"], out)
        run([binary, "render", "f08c_differenz_torus_tangential.scad", "-o", f"f08c-{label}.png", "--size", "900x650", "--no-axes"], out)
        print(f"D {label} done")


# ------------------------------------------------------------------------------ E: rename
def part_e():
    out = OUT / "e"
    out.mkdir(parents=True, exist_ok=True)
    (out / "teil.mcad").write_text("use <lib/halter.scad>\nhalter(20);\n")
    (out / "lib").mkdir(exist_ok=True)
    (out / "lib/halter.scad").write_text("module halter(w = 10) cube([w, 5, 2]);\n")
    txt = transcript([MCAD, "--version"], out, show_exit=False) + "\n"
    txt += transcript([MCAD, "check", "teil.mcad"], out) + "\n"
    txt += transcript([MCAD, "measure", "lib/halter.scad", "-m", "halter(30)", "--volume", "all"], out)
    (out / "dateien.txt").write_text(txt)
    p, _ = run([MCAD, "--help"])
    (out / "help.txt").write_text(p.stdout)


only = set(args.only.split(","))
for k, fn in [("a", part_a), ("b", part_b), ("c", part_c), ("d", part_d), ("e", part_e)]:
    if k in only:
        fn()
print("->", OUT)
