#!/usr/bin/env bash
# Data of test report 17 ("Stencil font cad stencil mono"): character charts of the font
# (SVG from the font itself, PNG through Chromium), alphabet tables through the CLI (stencil
# regular/bold, Allerta Stencil for comparison), parameter variations, the example stencil
# (render, STL, PrusaSlicer G-code and a picture of its first layer), the same sign in both
# stencil fonts, kernel times (3 runs each) and the warnings of the parameters.
# Writes reports/tmp/report17/.
#
#   CAD=target/release/mcad tools/report/report17-data.sh
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
CAD="${CAD:-$ROOT/target/release/mcad}"
CHROME="${CHROME:-$(ls -d /opt/pw-browsers/chromium_headless_shell-*/chrome-linux/headless_shell 2>/dev/null | head -n 1)}"
OUT="$ROOT/reports/tmp/report17"
CLI="$OUT/cli"
CASES="$OUT/cases"
CHART="$OUT/chart"
mkdir -p "$OUT" "$CLI" "$CASES" "$CHART"
export CAD_NO_DAEMON=1
r() { "$CAD" render "$@" > /dev/null; }
shot() { # svg png width height
  "$CHROME" --no-sandbox --disable-gpu --hide-scrollbars --screenshot="$2" --window-size="$3,$4" "file://$1" > /dev/null 2>&1
}

# 1. character charts straight from the font (cell, base line, x-height, cap height)
cargo build -q --release -p cad-font --example stencil_chart
EX="$ROOT/target/release/examples/stencil_chart"
"$EX" "$CHART/regular.svg" 0.14 0.1 16 > "$OUT/glyphs-regular.tsv"
"$EX" "$CHART/bold.svg" 0.2 0.1 16 > "$OUT/glyphs-bold.tsv"
shot "$CHART/regular.svg" "$CHART/regular.png" 2720 2450
shot "$CHART/bold.svg" "$CHART/bold.png" 2720 2450
# parameter variations (stroke, bridge as fractions of size)
for v in "0.06 0.04" "0.14 0.1" "0.2 0.2" "0.14 0"; do
  n="var-${v// /-}"
  "$EX" "$CHART/$n.svg" $v 8 "AB08@eäß" > "$OUT/$n.tsv"
  shot "$CHART/$n.svg" "$CHART/$n.png" 1360 350
done

# 2. alphabet tables through the CLI (rows of cad_font::REQUIRED_CHARS)
python3 - "$CASES" <<'EOF'
import sys
out = sys.argv[1]
rows = ["ABCDEFGHIJKLMNOPQRSTUVWXYZ", "abcdefghijklmnopqrstuvwxyz", "0123456789 ÄÖÜ äöü ß ẞ",
        ".,;:!?'\"„“‚‘»«()[]{}-–—_/\\", "&%#+*=<>°²³€@§~^|·•…±×÷µØø"]
fonts = [("stencil", ""), ("stencil-bold", ', font = "cad stencil mono:style=Bold"'), ("allerta", ', font = "Allerta Stencil"')]
esc = lambda s: s.replace("\\", "\\\\").replace('"', '\\"')
for name, font in fonts:
    with open(f"{out}/alphabet-{name}.scad", "w") as f:
        f.write(f"// Alphabet chart {name} (report 17)\n")
        for i, row in enumerate(rows):
            f.write(f'translate([0, {-14 * i}, 0]) linear_extrude(0.6) text("{esc(row)}", size = 8{font});\n')
EOF
for f in "$CASES"/alphabet-*.scad; do
  n=$(basename "$f" .scad)
  "$CAD" check "$f" > "$OUT/$n.check.txt" 2>&1 || true
  r "$f" -o "$CLI/$n.png" --view top --size 1400x560 --no-axes
done

# 3. the example stencil: render, export, slice
r tests/models/spec/stencil.mcad -o "$CLI/schablone.png" --size 1000x560
r tests/models/spec/stencil.mcad -o "$CLI/schablone-oben.png" --size 1000x420 --view top --no-axes
"$CAD" measure tests/models/spec/stencil.mcad --volume --bounds > "$OUT/schablone-measure.txt" 2>&1 || true
"$CAD" export tests/models/spec/stencil.mcad -o "$OUT/schablone.stl" > "$OUT/schablone-export.txt" 2>&1 || true
"$CAD" export tests/models/spec/stencil.mcad -o "$OUT/schablone.3mf" > /dev/null 2>&1 || true
if command -v prusa-slicer > /dev/null; then
  prusa-slicer --export-gcode --layer-height 0.2 --first-layer-height 0.2 --nozzle-diameter 0.4 \
    --output "$OUT/schablone.gcode" "$OUT/schablone.stl" > "$OUT/slicer.txt" 2>&1 || true
  python3 tools/report/report17-gcode.py "$OUT/schablone.gcode" "$CLI/schablone-gcode.png" > "$OUT/gcode.json" || true
fi
# raised sign: the same text in both stencil fonts
for f in stencil allerta; do
  font=""
  [ "$f" = allerta ] && font=', font = "Allerta Stencil"'
  cat > "$CASES/schild-$f.scad" <<EOF
// Sign with raised text (report 17): $f
color("white") cube([100, 30, 2]);
color("black") translate([50, 17, 2]) linear_extrude(1) text("WERKSTATT 3", size = 8, halign = "center"$font);
color("black") translate([50, 5, 2]) linear_extrude(1) text("ZUTRITT NUR MIT BRILLE", size = 4.2, halign = "center"$font);
EOF
  r "$CASES/schild-$f.scad" -o "$CLI/schild-$f.png" --size 1000x420 --view top --no-axes
  "$CAD" measure "$CASES/schild-$f.scad" --volume > "$OUT/schild-$f.measure.txt" 2>&1 || true
done

# 4. warnings of the parameters
printf 'text("A", size = 10, stroke_width = 3, bridge_width = 0.1);\ntext("B", font = "Inter", bridge_width = 1);\ntext("C", font = "Comic Sans");\n' > "$CASES/warn.scad"
"$CAD" check "$CASES/warn.scad" > "$OUT/warn.check.txt" 2>&1 || true

# 5. kernel times (native, release): eval_ms / kernel_ms of `cad eval --json`, 3 runs each
: > "$OUT/times.tsv"
t() { # name, source
  printf '%s\n' "$2" > "$CASES/time-$1.scad"
  for i in 1 2 3; do
    "$CAD" eval "$CASES/time-$1.scad" --json 2>/dev/null | python3 -c "import json,sys; s=json.load(sys.stdin)['scene']['stats']; print('$1', s['eval_ms'], s['kernel_ms'], s['kernel_calls'], s['nodes'], sep='\t')" >> "$OUT/times.tsv"
  done
}
for f in stencil allerta jetbrains; do
  font=""
  [ "$f" = allerta ] && font=', font = "Allerta Stencil"'
  [ "$f" = jetbrains ] && font=', font = "JetBrains Mono"'
  t "hello-$f" "linear_extrude(2) text(\"Hello World\"$font);"
  t "vierzig-$f" "linear_extrude(2) text(\"The quick brown fox jumps over the lazy d\"$font);"
  t "schild-$f" "cube([100, 14, 2]); translate([50, 4, 2]) linear_extrude(1.2) text(\"ZUTRITT NUR MIT BRILLE\", size = 4.2, halign = \"center\"$font);"
done
t schablone "$(cat tests/models/spec/stencil.mcad)"

# 6. the OpenSCAD comparison models use no text() (independent of the default font)
grep -l 'text(' tests/models/openscad-compat/*.scad tests/openscad-compat/projects -r > "$OUT/compat-text.txt" 2>/dev/null || true
echo "report17 data -> $OUT"
