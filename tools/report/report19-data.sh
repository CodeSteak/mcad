#!/usr/bin/env bash
# Data of test report 19 ("Measuring"): `cad measure` outputs (text and JSON) for every kind on the
# spec examples, the section summaries, analytic test models (cylinder, cone, sphere, torus,
# fillets, chamfer, bores) measured through the CLI against their formulas, a rendered section
# next to its contour summary, and CLI times (3 runs each, without daemon).
# Writes reports/tmp/report19/cli/ and reports/tmp/report19/analytic.json.
#
#   CAD=target/release/mcad tools/report/report19-data.sh
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
CAD="${CAD:-$ROOT/target/release/mcad}"
OUT="$ROOT/reports/tmp/report19"
CLI="$OUT/cli"
CASES="$OUT/cases"
mkdir -p "$OUT" "$CLI" "$CASES"
export CAD_NO_DAEMON=1
S=tests/models/spec

# runs a command, keeps its text (with the command line) and its time
run() { # name args…
  local n="$1"
  shift
  {
    printf '$ cad'
    printf " %q" "$@"
    printf '\n'
    "$CAD" "$@" 2>&1 || true
  } > "$CLI/$n.txt"
  local t0 t1
  t0=$(date +%s%N)
  for _ in 1 2 3; do "$CAD" "$@" > /dev/null 2>&1 || true; done
  t1=$(date +%s%N)
  echo "$n $(( (t1 - t0) / 3000000 ))" >> "$CLI/times.txt"
  "$CAD" "$@" --json > "$CLI/$n.json" 2> /dev/null || true
}
: > "$CLI/times.txt"

# 1. every kind on the spec examples
run housing-section measure "$S/housing.mcad" --section z=10 --wall "[0,15,10]" --distance "edge@[3,15,10]" "edge@[57,15,10]"
run housing-3d measure "$S/housing.mcad" --distance "face@[3,15,10]" "face@[57,15,10]" --body "face@[0,15,10]" --length "edge@[60,15,20]"
run plate-radius measure "$S/hole_plate.mcad" --radius "face@[8,6,2]" --radius "face@[8.292893,6,3.707107]" --distance "center@[6,6,0]" "center@[18,6,0]"
run plate-at measure "$S/hole_plate.mcad" --at "[8,6,2]"
run plate-section measure "$S/hole_plate.mcad" --section z=2 --radius "edge@[8,6,2]" --wall "[8,6,2]"
run chamfer-angle measure "$S/chamfers.mcad" --angle "face@[39.5,12,9.5]" "face@[20,12,10]" --angle "face@[20,12,10]" "face@[40,12,5]"
"$CAD" render "$S/hole_plate.mcad" -o "$CLI/plate-section.png" --section z=2 --view top --size 900x420 --no-axes > /dev/null 2>&1 || true
"$CAD" render "$S/housing.mcad" -o "$CLI/housing-section.png" --section z=10 --view top --size 900x520 --no-axes > /dev/null 2>&1 || true

# 2. analytic models: name | source | cad measure arguments | key | expected (python expression)
cat > "$CASES/list.tsv" <<'EOF'
Cylinder r 5, radius	cylinder(h = 10, r = 5);	--radius face@[5,0,5]	radius	5
Cylinder r 5, lateral area	cylinder(h = 10, r = 5);	--area face@[5,0,5]	area	2*pi*5*10
Cylinder r 5, circular edge length	cylinder(h = 10, r = 5);	--length edge@[0,5,10]	length	2*pi*5
Cylinder, cap faces normal distance	cylinder(h = 10, r = 5);	--distance face@[1,1,10] face@[1,1,0]	distance	10
Cone r 4→2, radius at h 5	cylinder(h = 10, r1 = 4, r2 = 2);	--radius face@[3,0,5]	radius	3
Cone r 4→2, half angle	cylinder(h = 10, r1 = 4, r2 = 2);	--radius face@[3,0,5]	semi_angle	degrees(atan(0.2))
Cone r 4→2, lateral area	cylinder(h = 10, r1 = 4, r2 = 2);	--area face@[3,0,5]	area	pi*(4+2)*sqrt(100+4)
Sphere r 5, radius	sphere(r = 5);	--radius face@[0,0,5]	radius	5
Sphere r 5, volume (kernel)	sphere(r = 5);	--body body@[0,0,5]	volume	4/3*pi*125
Torus R 10 r 2, tube radius	rotate_extrude() translate([10, 0, 0]) circle(r = 2);	--radius face@[12,0,0]	radius	2
Torus R 10 r 2, ring radius	rotate_extrude() translate([10, 0, 0]) circle(r = 2);	--radius face@[12,0,0]	major	10
Fillet r 2 (straight edge)	cube([20, 10, 8], $fillet = 2);	--radius face@[19.414214,5,7.414214]	radius	2
Fillet r 1 (circular edge)	difference(fillet = 1) { cube([20, 20, 6]); translate([10, 10, -1]) cylinder(h = 8, r = 3); }	--radius face@[12.292893,10,5.292893]	radius	1
Chamfer 2 mm, angle to top face	cube([20, 10, 8], $chamfer = 2);	--angle face@[19,5,7] face@[10,5,8]	angle	45
Chamfer 2 mm, opening angle	cube([20, 10, 8], $chamfer = 2);	--angle face@[19,5,7] face@[10,5,8]	opening	135
Section through bore r 3, radius	difference() { cube([20, 20, 6]); translate([10, 10, -1]) cylinder(h = 8, r = 3); }	--section z=3 --radius edge@[13,10,3]	radius	3
Section, area of the region	difference() { cube([20, 20, 6]); translate([10, 10, -1]) cylinder(h = 8, r = 3); }	--section z=3 --area region@[1,1,3]	area	400-9*pi
Section, wall thickness bore–edge	difference() { cube([20, 20, 6]); translate([10, 10, -1]) cylinder(h = 8, r = 3); }	--section z=3 --wall [7,10,3]	wall	7
Oblique section (45°) through bore r 3: semi-major axis	difference() { cube([20, 20, 10]); translate([10, 10, -1]) cylinder(h = 12, r = 3); }	--section-plane 10,10,5,1,0,1 --radius edge@[10,13,5]	radius	3*sqrt(2)
Oblique section (45°): semi-minor axis	difference() { cube([20, 20, 10]); translate([10, 10, -1]) cylinder(h = 12, r = 3); }	--section-plane 10,10,5,1,0,1 --radius edge@[10,13,5]	minor	3
Housing: wall thickness in section z 10	include <../../../../tests/models/spec/housing.mcad>	--section z=10 --wall [0,15,10]	wall	3
EOF
python3 - "$CAD" "$CASES" "$OUT/analytic.json" <<'EOF'
import json, math, subprocess, sys
cad, cases, out = sys.argv[1:4]
env = {"CAD_NO_DAEMON": "1", "PATH": "/usr/bin:/bin"}
rows = []
for i, line in enumerate(open(f"{cases}/list.tsv")):
    name, src, args, key, expect = line.rstrip("\n").split("\t")
    f = f"{cases}/case{i:02d}.scad"
    open(f, "w").write(src + "\n")
    r = subprocess.run([cad, "measure", f, *args.split(" "), "--json"], capture_output=True, text=True, env=env)
    want = eval(expect, {k: getattr(math, k) for k in ("pi", "sqrt", "atan", "degrees")})
    got, exact, tol, label = None, None, None, None
    try:
        j = json.loads(r.stdout)
        m = j["measurements"][-1]
        exact, tol, label = m["exact"], m.get("tol"), m["label"]
        got = next((v["value"] for v in m["values"] if v["key"] == key), None)
    except Exception as e:  # noqa: BLE001
        label = f"error: {e}: {r.stderr.strip()[:200]}"
    rows.append({"name": name, "source": src, "args": args, "key": key, "expect": want, "got": got,
                 "err": None if got is None else got - want, "exact": exact, "tol": tol, "label": label})
json.dump(rows, open(out, "w"), indent=1)
def limit(r):
    return 1e-3 if "kernel" in r["name"].lower() else 1e-6
bad = [r for r in rows if r["err"] is None or abs(r["err"]) > max(limit(r) * abs(r["expect"]), 1e-9)]
print(f"{len(rows)} analytic cases, {len(rows) - len(bad)} within the limit (1e-6 relative, kernel volume 1e-3)")
for b in bad:
    print("  deviation:", b["name"], b["got"], b["expect"], b["label"])
EOF
echo "-> $OUT"
