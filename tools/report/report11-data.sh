#!/usr/bin/env bash
# Data of test report 11 ("Farben, Multi-Filament-3MF, dunkles Thema") that come from the CLI
# and the checking scripts; writes reports/tmp/report11/ (gitignored) and the CLI images to
# docs/reports/11-farben-filamente/img/.
#
#   CAD=target/release/mcad PY3MF=/pfad/venv/bin/python tools/report/report11-data.sh
#
# PY3MF: Python with lib3mf and trimesh (pip install lib3mf trimesh networkx lxml).
# The test runs (cargo, Playwright, Tauri-E2E, compare.py, verify.py) are listed in
# tools/report/report11.mjs.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
CAD="${CAD:-$ROOT/target/release/mcad}"
PY3MF="${PY3MF:-python3}"
OUT="$ROOT/reports/tmp/report11"
IMG="$ROOT/docs/reports/11-farben-filamente/img"
EX="tests/models/spec/mehrfarbig.scad"
mkdir -p "$OUT" "$IMG"
export CAD_NO_DAEMON=1

python3 tools/palette/palette.py --json "$OUT/palette.json" > "$OUT/palette.txt"

"$CAD" outline "$EX" > "$OUT/outline.txt"
"$CAD" outline "$EX" --json > "$OUT/outline.json"
"$CAD" measure "$EX" --colors --json > "$OUT/measure.json"
"$CAD" measure "$EX" --colors > "$OUT/measure.txt"
"$CAD" export "$EX" --part schild --part figur -o "$OUT/mehrfarbig.3mf" > "$OUT/export.txt"
"$CAD" export "$EX" --part schild --part figur -o "$OUT/mehrfarbig.3mf" --json > "$OUT/export.json"
"$CAD" export "$EX" --per-color --part schild --part figur -o "$OUT/mehrfarbig-farben.zip" > "$OUT/export-zip.txt"
"$CAD" export "$EX" -o "$OUT/modell.3mf" > "$OUT/export-root.txt"
unzip -p "$OUT/mehrfarbig.3mf" 3D/3dmodel.model > "$OUT/3dmodel.model"
unzip -p "$OUT/mehrfarbig.3mf" Metadata/model_settings.config > "$OUT/model_settings.config"
unzip -p "$OUT/mehrfarbig.3mf" "\[Content_Types\].xml" > "$OUT/content_types.xml"
unzip -l "$OUT/mehrfarbig.3mf" > "$OUT/3mf-list.txt"
unzip -l "$OUT/mehrfarbig-farben.zip" > "$OUT/zip-list.txt"
"$PY3MF" tools/report/verify3mf.py "$OUT/mehrfarbig.3mf" --expect "$OUT/measure.json" --json "$OUT/verify.json" > "$OUT/verify.txt" 2>&1 || true

# unknown names: random colours (stable)
printf 'color("lightblue") cube(1);\ncolor("Rot") translate([2, 0, 0]) cube(1);\ncolor("salmon") translate([4, 0, 0]) cube(1);\ncolor("navy") translate([6, 0, 0]) cube(1);\ncolor("gold") translate([8, 0, 0]) cube(1);\ncolor("olive") translate([10, 0, 0]) cube(1);\n' > "$OUT/zufall.scad"
"$CAD" outline "$OUT/zufall.scad" --no-kernel --json > "$OUT/zufall.json" || true
"$CAD" check "$OUT/zufall.scad" > "$OUT/zufall-check.txt" 2>&1 || true

# CLI images (light, dark, regions, views)
"$CAD" render "$EX" -o "$IMG/cli-hell.png" --size 900x560 > /dev/null
"$CAD" render "$EX" -o "$IMG/cli-dunkel.png" --size 900x560 --theme dark > /dev/null
"$CAD" render "$EX" -o "$IMG/cli-regionen.png" --size 900x560 --show regions --theme dark > /dev/null
"$CAD" render "$EX" -o "$IMG/cli-ansichten.png" --views iso,front,top,right --size 1400x900 > /dev/null
"$CAD" render "$EX" -o "$IMG/cli-schnitt.png" --size 900x560 --section x=80 --part figur --show regions --view right > /dev/null

# sources of the 3MF assumptions (fetched files, hashes)
SRC="$OUT/sources"
mkdir -p "$SRC"
for u in \
  "bambulab/BambuStudio/master/src/libslic3r/Format/bbs_3mf.cpp" \
  "bambulab/BambuStudio/master/src/slic3r/GUI/Plater.cpp" \
  "bambulab/BambuStudio/master/src/slic3r/GUI/ObjColorDialog.cpp" \
  "bambulab/BambuStudio/master/src/libslic3r/Model.cpp" \
  "bambulab/BambuStudio/master/src/libslic3r/Format/3mf.cpp" \
  "SoftFever/OrcaSlicer/main/src/libslic3r/Format/bbs_3mf.cpp" \
  "SoftFever/OrcaSlicer/main/src/slic3r/GUI/Plater.cpp"; do
  f="$SRC/$(echo "$u" | tr / _)"
  [ -s "$f" ] || curl -sSL -o "$f" "https://raw.githubusercontent.com/$u" || true
done
( cd "$SRC" && sha256sum ./* > ../sources.sha256 )
echo "report11 data: $OUT"
