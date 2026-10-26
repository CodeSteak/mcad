#!/usr/bin/env bash
# Data of test report 15 ("Sketches: corners, offset, stroke") from the CLI: renderings of the
# examples (views, highlighted lines with the guide lines of offset/stroke/fillet2d), areas of 2D
# operations against closed formulas, the OpenSCAD comparison of offset case by case, STEP
# statistics (exact circles and lines), error messages, timings of the 2D operations. Writes
# reports/tmp/report15/.
#
#   CAD=target/release/mcad tools/report/report15-data.sh
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
CAD="${CAD:-$ROOT/target/release/mcad}"
OUT="$ROOT/reports/tmp/report15"
CLI="$OUT/cli"
CASES="$OUT/cases"
mkdir -p "$OUT" "$CLI" "$CASES"
export CAD_NO_DAEMON=1
r() { "$CAD" render "$@" > /dev/null; }

# renderings of the examples
r tests/models/spec/sketch_lid.mcad -o "$CLI/deckel.png" --size 900x560
r tests/models/spec/sketch_lid.mcad -o "$CLI/deckel-offset.png" --size 900x560 --view top --line 30
r tests/models/spec/sketch_holder.mcad -o "$CLI/halter.png" --size 900x560
r tests/models/spec/sketch_holder.mcad -o "$CLI/halter-fillet.png" --size 900x560 --view top --line 18
r tests/models/spec/sketch_cable_duct.mcad -o "$CLI/kabelkanal.png" --size 900x560
r tests/models/spec/sketch_cable_duct.mcad -o "$CLI/kabelkanal-stroke.png" --size 900x560 --view top --line 22
r crates/cad-cli/tests/data/skizze_ecken.scad -o "$CLI/ecken-handles.png" --size 900x560 --view top --line 5 --show handles
r crates/cad-cli/tests/data/skizze_ecken.scad -o "$CLI/ecken-offset.png" --size 900x560 --view top --line 13 --show handles

# areas against formulas: case name, 2D source (extruded by 1), formula (python expression)
cat > "$CASES/list.txt" <<'EOF'
rechteck_fillet2d|fillet2d(4) square([30, 20]);|30*20 - (4 - pi)*16
rechteck_corner|sketch { start([0, 0]); corner(r = 4); line(to = [30, 0]); corner(r = 4); line(to = [30, 20]); corner(r = 4); line(to = [0, 20]); corner(r = 4); close(); }|30*20 - (4 - pi)*16
quadrat_kreis|fillet2d(5) square(10);|pi*25
rechteck_chamfer2d|chamfer2d(2) square([30, 20]);|600 - 4*2
l_konkav|fillet2d(2) polygon([[0, 0], [20, 0], [20, 8], [8, 8], [8, 20], [0, 20]]);|256 - 4*(1 - pi/4)*4
l_zone|fillet2d(2) { polygon([[0, 0], [20, 0], [20, 8], [8, 8], [8, 20], [0, 20]]); translate([8, 8]) circle(1); }|256 + (1 - pi/4)*4
offset_quadrat|offset(r = 2) square(10);|(10 + 2*2)**2 - (4 - pi)*4
offset_quadrat_fein|offset(r = 0.5) square(20);|(20 + 2*0.5)**2 - (4 - pi)*0.25
offset_ring|offset(r = 1) difference() { circle(10); circle(4); }|pi*(11**2 - 3**2)
offset_delta_fase|offset(delta = 2, chamfer = true) square(10);|196 - 4*(2*sqrt(2) - 2)**2
stroke_bogen_flach|stroke(2, cap = "flat") sketch(closed = false) { start([10, 0]); arc(center = [0, 0], angle = 90); }|pi/4*(11**2 - 9**2)
stroke_bogen_rund|stroke(2) sketch(closed = false) { start([10, 0]); arc(center = [0, 0], angle = 90); }|pi/4*(11**2 - 9**2) + pi
stroke_bogen_eckig|stroke(2, cap = "square") sketch(closed = false) { start([10, 0]); arc(center = [0, 0], angle = 90); }|pi/4*(11**2 - 9**2) + 4
stroke_ecke_gehrung|stroke(2, cap = "flat", join = "miter") sketch(closed = false) { start([0, 0]); line(to = [10, 0]); line(to = [10, 10]); }|39 + 1
stroke_ecke_rund|stroke(2, cap = "flat") sketch(closed = false) { start([0, 0]); line(to = [10, 0]); line(to = [10, 10]); }|39 + pi/4
stroke_ecke_fase|stroke(2, cap = "flat", join = "bevel") sketch(closed = false) { start([0, 0]); line(to = [10, 0]); line(to = [10, 10]); }|39 + 0.5
stroke_kreisrand|stroke(2) circle(10);|pi*(11**2 - 9**2)
corner_bogen|sketch { start([0, 0]); line(to = [10, 0]); corner(r = 1); arc(center = [0, 0], angle = 90); close(); }|pi*25 - (0.5*(10 - sqrt(80))*10/9 + 50*(asin(1/9) - 1/9) - 0.5*(acos(-1/9) - sin(acos(-1/9))))
EOF
: > "$OUT/areas.tsv"
while IFS='|' read -r name src formula; do
  [ -z "$name" ] && continue
  printf 'linear_extrude(1) %s\n' "$src" > "$CASES/$name.scad"
  start=$(date +%s%N)
  v=$("$CAD" measure "$CASES/$name.scad" --volume --json 2>/dev/null | python3 -c 'import json,sys; d=json.load(sys.stdin); print(repr(d["volume"]["volume"]))' || echo nan)
  ms=$(( ($(date +%s%N) - start) / 1000000 ))
  want=$(python3 -c "from math import *; print(repr($formula))")
  printf '%s\t%s\t%s\t%s\t%s\n' "$name" "$v" "$want" "$ms" "$src" >> "$OUT/areas.tsv"
done < "$CASES/list.txt"

# OpenSCAD comparison of offset, case by case
CAD="$CAD" python3 tools/report/report15-openscad.py "$OUT"

# errors at the lines
cat > "$CASES/fehler.scad" <<'EOF'
linear_extrude(1) sketch {
    start([0, 0]);
    line(to = [20, 0]);
    corner(r = 12);
    line(to = [20, 8]);
    corner(r = 1);
    arc(to = [10, 18]);
    line(to = [0, 8]);
    corner(r = 2);
    corner(r = 3);
    close();
}
translate([30, 0, 0]) linear_extrude(1) fillet2d(1.5) square([10, 2]);
translate([50, 0, 0]) linear_extrude(1) offset(r = -6) square(10);
translate([70, 0, 0]) linear_extrude(1) offset(1) sketch(closed = false) { start([0, 0]); line(to = [5, 0]); }
EOF
"$CAD" check "$CASES/fehler.scad" > "$OUT/check-fehler.txt" 2>&1 || true

# inspect: hints of fillet2d
"$CAD" inspect tests/models/spec/sketch_holder.mcad --line 18 > "$OUT/inspect-halter.txt" 2>&1 || true

# STEP: curves and surfaces written by the exporter
: > "$OUT/step.tsv"
for f in tests/models/spec/sketch_lid.mcad tests/models/spec/sketch_holder.mcad tests/models/spec/sketch_cable_duct.mcad tests/models/openscad-compat/34_offset_round.scad tests/models/openscad-compat/36_offset_holes.scad; do
  n=$(basename "$f" .scad)
  "$CAD" export "$f" -o "$OUT/$n.step" --json > "$OUT/$n.export.json" 2>/dev/null || true
  python3 - "$n" "$OUT/$n.export.json" "$OUT/$n.step" >> "$OUT/step.tsv" <<'EOF'
import json, re, sys
name, js, step = sys.argv[1:4]
d = json.load(open(js))
t = open(step).read()
cnt = {k: len(re.findall(r"=%s\(" % k, t)) for k in ["CIRCLE", "LINE", "B_SPLINE_CURVE_WITH_KNOTS", "PLANE", "CYLINDRICAL_SURFACE", "SURFACE_OF_LINEAR_EXTRUSION", "B_SPLINE_SURFACE_WITH_KNOTS"]}
st = d.get("step") or {}
info = {k: st.get(k) for k in ["solids", "faces", "edges", "approximated_edges", "approximated_faces", "curves", "surfaces"]}
print("\t".join([name, json.dumps(cnt), json.dumps(info), repr(d.get("volume"))]))
EOF
done

# timings: 2D operations alone (no kernel) and with extrusion
cat > "$CASES/zeit2d.scad" <<'EOF'
n = 40;
module zahnrad() difference() {
    polygon([for (i = [0 : 4 * n - 1]) let(a = i * 360 / (4 * n), r = (floor(i / 2) % 2 == 0) ? 20 : 17) [r * cos(a), r * sin(a)]]);
    circle(5);
}
offset(r = 0.5) zahnrad();
translate([50, 0, 0]) fillet2d(0.4) zahnrad();
translate([0, 50, 0]) stroke(1.5) sketch(closed = false) {
    start([0, 0]);
    spline([[10, 10], [20, -5], [30, 10], [40, 0], [50, 12]]);
}
EOF
sed 's/^offset(r = 0.5) zahnrad();/linear_extrude(3) offset(r = 0.5) zahnrad();/; s/fillet2d(0.4) zahnrad();/linear_extrude(3) fillet2d(0.4) zahnrad();/; s/translate(\[0, 50, 0\]) stroke/translate([0, 50, 0]) linear_extrude(2) stroke/' "$CASES/zeit2d.scad" > "$CASES/zeit3d.scad"
: > "$OUT/timings.tsv"
for f in zeit2d zeit3d; do
  "$CAD" check "$CASES/$f.scad" > /dev/null 2>&1 || true
  best=999999
  for i in 1 2 3; do
    s=$(date +%s%N); "$CAD" check "$CASES/$f.scad" > /dev/null 2>&1 || true; e=$(date +%s%N)
    ms=$(( (e - s) / 1000000 )); [ "$ms" -lt "$best" ] && best=$ms
  done
  printf '%s\t%s\n' "$f" "$best" >> "$OUT/timings.tsv"
done
for f in tests/models/spec/sketch_lid.mcad tests/models/spec/sketch_holder.mcad tests/models/spec/sketch_cable_duct.mcad; do
  s=$(date +%s%N); "$CAD" check "$f" > /dev/null 2>&1 || true; e=$(date +%s%N)
  printf '%s\t%s\n' "$(basename "$f" .scad)" "$(( (e - s) / 1000000 ))" >> "$OUT/timings.tsv"
done
echo "report15 data -> $OUT"
