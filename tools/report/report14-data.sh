#!/usr/bin/env bash
# Data of test report 14 ("sketches and namespaces") from the CLI: renderings of the sketch
# examples (views, a highlighted segment line), exact volumes of sketch cases against their
# formulas, STEP statistics (exact circles and lines), outline of the namespace library, errors
# per segment line. Writes reports/tmp/report14/.
#
#   CAD=target/release/mcad tools/report/report14-data.sh
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
CAD="${CAD:-$ROOT/target/release/mcad}"
OUT="$ROOT/reports/tmp/report14"
CLI="$OUT/cli"
CASES="$OUT/cases"
mkdir -p "$OUT" "$CLI" "$CASES"
export CAD_NO_DAEMON=1
r() { "$CAD" render "$@" > /dev/null; }

# renderings of the examples
r tests/models/spec/sketch_profile.mcad -o "$CLI/profil.png" --size 900x560
r tests/models/spec/sketch_profile.mcad -o "$CLI/profil-oben.png" --size 900x560 --view top
r tests/models/spec/sketch_profile.mcad -o "$CLI/profil-zeile12.png" --size 900x560 --line 12 --view top --module holder
r tests/models/spec/sketch_timing_pulley.mcad -o "$CLI/scheibe.png" --size 700x560
r tests/models/spec/sketch_timing_pulley.mcad -o "$CLI/scheibe-oben.png" --size 700x560 --view top
r tests/models/spec/sketch_sweep.mcad -o "$CLI/sweep.png" --size 900x560
r tests/models/spec/sketch_library.mcad -o "$CLI/bibliothek.png" --size 900x560
r tests/models/spec/sketch_profile.mcad -o "$CLI/profil-schnitt.png" --size 900x560 --section z=2.5 --view top

# outline and check output
"$CAD" outline tests/models/spec/sketch_library.mcad > "$OUT/outline.txt" 2>&1 || true
"$CAD" check tests/models/spec/sketch_profile.mcad > "$OUT/check-profil.txt" 2>&1 || true
printf 'linear_extrude(2) sketch {\n    start([0, 0]);\n    line(to = [30, 0]);\n    arc(to = [30, 20], r = 5);\n    line(to = [0, 20]);\n}\nlinear_extrude(2) translate([40, 0]) sketch {\n    start([0, 0]);\n    line(to = [10, 10]);\n    line(to = [10, 0]);\n    line(to = [0, 10]);\n    close();\n}\n' > "$CASES/fehler.scad"
"$CAD" check "$CASES/fehler.scad" > "$OUT/check-fehler.txt" 2>&1 || true
"$CAD" check tests/models/spec/sketch_library.mcad --module parts.plate.hole_plate > "$OUT/check-modul.txt" 2>&1 || true

# exact volumes: case name, source, formula (python expression)
cat > "$CASES/list.txt" <<'EOF'
rechteck_halbkreis|linear_extrude(5) sketch { start([0, 0]); line(to = [40, 0]); line(to = [40, 20]); arc(to = [0, 20], r = 20); close(); }|(40*20 + pi*20**2/2)*5
tangentialbogen|linear_extrude(5) sketch { start([0, 0]); line(to = [40, 0]); line(by = [0, 20]); arc(to = [0, 20]); close(); }|(40*20 + pi*20**2/2)*5
mittelpunkt_winkel|linear_extrude(5) sketch { start([0, 0]); line(to = [40, 0]); line(by = [0, 20]); arc(center = [20, 20], angle = 180); close(); }|(40*20 + pi*20**2/2)*5
kugel|rotate_extrude() sketch { start([0, -7]); arc(to = [0, 7], r = 7); close(); }|4/3*pi*7**3
torus|rotate_extrude() sketch { start([24, 0]); arc(to = [16, 0], r = 4); arc(to = [24, 0], r = 4); }|2*pi**2*20*4**2
ring_windung|linear_extrude(2) sketch { start([0, 0]); line(to = [20, 0]); line(to = [20, 20]); line(to = [0, 20]); close(); start([15, 10]); arc(to = [5, 10], r = 5, cw = true); arc(to = [15, 10], r = 5, cw = true); }|(400 - pi*25)*2
ring_hole|linear_extrude(2) sketch { start([0, 0]); line(to = [20, 0]); line(to = [20, 20]); line(to = [0, 20]); close(); hole { start([15, 10]); arc(to = [5, 10], r = 5); arc(to = [15, 10], r = 5); } }|(400 - pi*25)*2
bezier_quadratisch|linear_extrude(1) sketch { start([0, 0]); line(to = [4, 0]); line(to = [4, 4]); bezier(c1 = [2, 6], to = [0, 4]); close(); }|16 + 8/3
stern_schleife|module stern(n = 6) sketch { start([10, 0]); for (i = [1 : n]) { line(to = [5*cos(360*(i-0.5)/n), 5*sin(360*(i-0.5)/n)]); line(to = [10*cos(360*i/n), 10*sin(360*i/n)]); } } linear_extrude(3) stern();|12*0.5*10*5*sin(pi/6)*3
rohr_pfad|sweep() { sketch(closed = false) { start([0, 0]); line(to = [20, 0]); arc(to = [30, 10]); line(by = [0, 15]); } circle(2); }|pi*4*(20 + pi*10/2 + 15)
gehrung_pfad|sweep() { sketch(closed = false) { start([0, 0]); line(to = [20, 0]); line(to = [20, 15]); } square(2, center = true); }|4*35
twist_bogen|linear_extrude(10, twist = 90) sketch { start([-5, -3]); line(to = [5, -3]); arc(to = [5, 3], r = 3); line(to = [-5, 3]); arc(to = [-5, -3], r = 3); }|(60 + pi*9)*10
EOF
: > "$OUT/volumes.tsv"
while IFS='|' read -r name src formula; do
  [ -z "$name" ] && continue
  printf '%s\n' "$src" > "$CASES/$name.scad"
  start=$(date +%s%N)
  v=$("$CAD" measure "$CASES/$name.scad" --volume --json 2>/dev/null | python3 -c 'import json,sys; d=json.load(sys.stdin); print(repr(d["volume"]["volume"]))' || echo nan)
  ms=$(( ($(date +%s%N) - start) / 1000000 ))
  want=$(python3 -c "from math import *; print(repr($formula))")
  printf '%s\t%s\t%s\t%s\t%s\n' "$name" "$v" "$want" "$ms" "$src" >> "$OUT/volumes.tsv"
done < "$CASES/list.txt"

# STEP: curves and surfaces written by the exporter
: > "$OUT/step.tsv"
for f in tests/models/spec/sketch_profile.mcad tests/models/spec/sketch_timing_pulley.mcad tests/models/spec/sketch_sweep.mcad "$CASES/rechteck_halbkreis.scad" "$CASES/torus.scad"; do
  n=$(basename "$f" .scad)
  "$CAD" export "$f" -o "$OUT/$n.step" --json > "$OUT/$n.export.json" 2>/dev/null || true
  python3 - "$n" "$OUT/$n.export.json" "$OUT/$n.step" >> "$OUT/step.tsv" <<'EOF'
import json, re, sys
name, js, step = sys.argv[1:4]
d = json.load(open(js))
t = open(step).read()
cnt = {k: len(re.findall(r"=%s\(" % k, t)) for k in ["CIRCLE", "LINE", "B_SPLINE_CURVE_WITH_KNOTS", "PLANE", "CYLINDRICAL_SURFACE", "TOROIDAL_SURFACE", "SPHERICAL_SURFACE", "SURFACE_OF_LINEAR_EXTRUSION", "SURFACE_OF_REVOLUTION", "B_SPLINE_SURFACE_WITH_KNOTS"]}
st = d.get("step") or {}
info = {k: st.get(k) for k in ["solids", "faces", "edges", "approximated_edges", "approximated_faces", "curves", "surfaces"]}
print("\t".join([name, json.dumps(cnt), json.dumps(info), repr(d.get("volume"))]))
EOF
done
echo "report14 data -> $OUT"
