#!/usr/bin/env bash
# Data of test report 16 ("Text und SVG-Import") from the CLI and OpenSCAD 2021.01: alphabet
# tables of every bundled font, renderings of the SVG examples (the sources are shown by the
# browser itself), the examples text_schild/svg_logo, kernel times (Hello World, 40
# characters), and the comparison with OpenSCAD (tools/report/report16-openscad.py).
# Writes reports/tmp/report16/.
#
#   CAD=target/release/mcad tools/report/report16-data.sh
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
CAD="${CAD:-$ROOT/target/release/mcad}"
OUT="$ROOT/reports/tmp/report16"
CLI="$OUT/cli"
CASES="$OUT/cases"
mkdir -p "$OUT" "$CLI" "$CASES"
export CAD_NO_DAEMON=1
r() { "$CAD" render "$@" > /dev/null; }

# alphabet tables: one file per font, rows of the required characters (cad_font::REQUIRED_CHARS)
python3 - "$CASES" <<'EOF'
import sys
out = sys.argv[1]
rows = ["ABCDEFGHIJKLMNOPQRSTUVWXYZ", "abcdefghijklmnopqrstuvwxyz", "0123456789 ÄÖÜ äöü ß ẞ",
        ".,;:!?'\"„“‚‘»«()[]{}-–—_/\\", "&%#+*=<>°²³€@§~^|·•…±×÷µØø"]
fonts = [("jetbrains", "JetBrains Mono"), ("jetbrains-bold", "JetBrains Mono:style=Bold"),
         ("inter", "Inter"), ("inter-bold", "Inter:style=Bold"), ("allerta", "Allerta Stencil")]
esc = lambda s: s.replace("\\", "\\\\").replace('"', '\\"')
for name, font in fonts:
    with open(f"{out}/alphabet-{name}.scad", "w") as f:
        f.write(f"// Alphabet-Tafel {font} (Bericht 16)\n")
        for i, row in enumerate(rows):
            f.write(f'translate([0, {-14 * i}, 0]) linear_extrude(0.6) text("{esc(row)}", size = 8, font = "{font}");\n')
EOF
for f in "$CASES"/alphabet-*.scad; do
  n=$(basename "$f" .scad)
  "$CAD" check "$f" > "$OUT/$n.check.txt" 2>&1 || true
  r "$f" -o "$CLI/$n.png" --view top --size 1400x560 --no-axes
done

# SVG examples: rendering of each file alone (top view) and the example model
for s in logo blatt haus welle; do
  printf 'linear_extrude(2) import("../../../../tests/models/svg/%s.svg");\n' "$s" > "$CASES/svg-$s.scad"
  r "$CASES/svg-$s.scad" -o "$CLI/svg-$s.png" --view top --size 700x560 --no-axes
  "$CAD" check "$CASES/svg-$s.scad" > "$OUT/svg-$s.check.txt" 2>&1 || true
done
r tests/models/spec/svg_logo.scad -o "$CLI/svg_logo.png" --size 1000x640
r tests/models/spec/svg_logo.scad -o "$CLI/svg_logo-oben.png" --size 1000x640 --view top
r tests/models/spec/text_schild.scad -o "$CLI/text_schild.png" --size 1000x640
r tests/models/spec/text_schild.scad -o "$CLI/text_schild-oben.png" --size 1000x640 --view top
"$CAD" measure tests/models/spec/text_schild.scad --colors > "$OUT/schild-colors.txt" 2>&1 || true
"$CAD" export tests/models/spec/text_schild.scad -o "$OUT/text_schild.3mf" > "$OUT/schild-3mf.txt" 2>&1 || true
"$CAD" export tests/models/spec/svg_logo.scad -o "$OUT/svg_logo.step" --json > "$OUT/svg_logo.export.json" 2>/dev/null || true
python3 - "$OUT/svg_logo.step" > "$OUT/svg_logo.step-counts.json" <<'EOF'
import json, re, sys
t = open(sys.argv[1]).read()
print(json.dumps({k: len(re.findall(r"=%s\(" % k, t)) for k in ["CIRCLE", "LINE", "ELLIPSE", "B_SPLINE_CURVE_WITH_KNOTS", "PLANE", "CYLINDRICAL_SURFACE", "SURFACE_OF_LINEAR_EXTRUSION", "B_SPLINE_SURFACE_WITH_KNOTS"]}))
EOF

# warnings of unsupported SVG content
cat > "$CASES/warn.svg" <<'EOF'
<svg xmlns="http://www.w3.org/2000/svg" width="40mm" height="20mm" viewBox="0 0 40 20">
  <style>.a { fill: red }</style>
  <rect class="a" width="20" height="20"/>
  <text x="22" y="12">Hallo</text>
  <image href="foto.png" width="10" height="10"/>
  <path d="M 22 2 L 38 2 L 38 8" stroke="black" fill="none"/>
  <use href="#r"/>
</svg>
EOF
printf 'linear_extrude(1) import("warn.svg");\n' > "$CASES/warn.scad"
"$CAD" check "$CASES/warn.scad" > "$OUT/warn.check.txt" 2>&1 || true
printf 'linear_extrude(1) text("Maß: 20°C ± 2", font = "Allerta Stencil");\n' > "$CASES/fallback.scad"
"$CAD" check "$CASES/fallback.scad" > "$OUT/fallback.check.txt" 2>&1 || true

# kernel times (native, release): eval_ms / kernel_ms of `cad eval --json`, 3 runs each
: > "$OUT/times.tsv"
t() { # name, source
  printf '%s\n' "$2" > "$CASES/time-$1.scad"
  for i in 1 2 3; do
    "$CAD" eval "$CASES/time-$1.scad" --json 2>/dev/null | python3 -c "import json,sys; s=json.load(sys.stdin)['scene']['stats']; print('$1', s['eval_ms'], s['kernel_ms'], s['kernel_calls'], s['nodes'], sep='\t')" >> "$OUT/times.tsv"
  done
}
t hello 'linear_extrude(2) text("Hello World", font = "Inter");'
t vierzig 'linear_extrude(2) text("The quick brown fox jumps over the lazy d", font = "Inter");'
t vierzig_mono 'linear_extrude(2) text("The quick brown fox jumps over the lazy d");'
t platte 'cube([100, 20, 3]); translate([2, 5, 3]) linear_extrude(1) text("Hello World", font = "Inter");'
t svg_logo "$(cat tests/models/spec/svg_logo.scad | sed 's#\.\./svg/#../../../../tests/models/svg/#g')"
t text_schild "$(cat tests/models/spec/text_schild.scad)"

# OpenSCAD comparison
python3 tools/report/report16-openscad.py "$OUT" "$CAD"
echo "report16 data -> $OUT"
