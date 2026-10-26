#!/usr/bin/env bash
# Data of test report 12 ("fixes for colors and daemon") that come from the CLI and the
# checking scripts: outputs and images per finding of tests/color-review/ (after the fix;
# the images before are the reviewer's in tests/color-review/img/), the 3MF check, the
# daemon with a long CAD_IPC_DIR and the slicer sources. Writes reports/tmp/report12/.
#
#   CAD=target/debug/mcad PY3MF=/path/venv/bin/python tools/report/report12-data.sh
#
# PY3MF: Python with lib3mf and trimesh (pip install lib3mf trimesh networkx lxml).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
CAD="${CAD:-$ROOT/target/release/mcad}"
PY3MF="${PY3MF:-python3}"
OUT="$ROOT/reports/tmp/report12"
CLI="$OUT/cli"
C="tests/color-review/cases"
mkdir -p "$OUT" "$CLI"
export CAD_NO_DAEMON=1
r() { "$CAD" render "$@" > /dev/null; }

# F1: equal geometry, different colours (view: regions and faces; STEP colours)
r "$C/pal2.scad" -o "$CLI/f1-pal2.png" --size 700x420
r "$C/pal2.scad" -o "$CLI/f1-pal2-flaechen.png" --size 700x420 --show faces
r "$C/palette.scad" -o "$CLI/f1-palette.png" --size 700x520
"$CAD" export "$C/pal2.scad" -o "$OUT/pal2.step" > /dev/null
python3 - "$OUT/pal2.step" > "$OUT/f1-step.txt" <<'EOF'
import re, sys, collections
t = open(sys.argv[1]).read()
ents = dict(re.findall(r"#(\d+)=(.*?);\n", t))
def col(i, d=0):
    e = ents[i]
    if e.startswith("COLOUR_RGB"):
        v = [float(x) for x in e[len("COLOUR_RGB('',"):-1].split(",")]
        return "#%02x%02x%02x" % tuple(round(x * 255) for x in v)
    if d > 8:
        return None
    for r in re.findall(r"#(\d+)", e):
        c = col(r, d + 1)
        if c:
            return c
cnt = collections.Counter(col(re.search(r"\(#(\d+)\)", v).group(1)) for v in ents.values() if v.startswith("STYLED_ITEM('color'"))
for c, n in sorted(cnt.items()):
    print(f"{c}: {n} faces")
EOF

# F2: slot table (the CLI shows the result of the UI edit: the table in the code)
printf 'module bar(c) color(c) cube([10, 2, 2]);\nfilament("red", 4);\nbar("red");\ntranslate([0, 5, 0]) bar("blue");\ntranslate([0, 10, 0]) bar("green");\n' > "$OUT/f2-tabelle.scad"
"$CAD" outline "$OUT/f2-tabelle.scad" > "$OUT/f2-outline.txt" 2>&1 || true

# F3: overlap, fillet at a colour boundary, colour in a 2D profile
r "$C/r_overlap2.scad" -o "$CLI/f3-overlap2.png" --size 520x360
r "$C/r_overlap3.scad" -o "$CLI/f3-overlap3.png" --size 520x360
r "$C/r_fillet2.scad" -o "$CLI/f3-fillet2.png" --size 520x360
r "$C/r_fillet2.scad" -o "$CLI/f3-fillet2-front.png" --size 520x360 --view front
r "$C/r_extrude.scad" -o "$CLI/f3-extrude.png" --size 620x360
"$CAD" measure "$C/r_extrude.scad" --colors > "$OUT/f3-extrude.txt" 2>&1 || true

# F4: material outside parts
"$CAD" export "$C/inc/main_use.scad" -o "$OUT/main_use.3mf" > "$OUT/f4-export.txt" 2>&1 || true

# F5: random colours
"$CAD" outline "$C/randnames.scad" --json > "$OUT/f5-randnames.json" 2>/dev/null || true
r "$C/randnames.scad" -o "$CLI/f5-randnames.png" --size 700x560 --view top

# F6, F7, F8: volumes and messages
for f in r_touch r_touch3 r_touch4 values unknown f1; do
  "$CAD" measure "$C/$f.scad" --colors > "$OUT/m-$f.txt" 2>&1 || true
done
"$CAD" export "$C/plain.scad" -o "$OUT/plain.3mf" > /dev/null
unzip -p "$OUT/plain.3mf" 3D/3dmodel.model | head -c 600 > "$OUT/f8-plain-model.txt"

# F9: per-colour export, colour list of a part, file names
"$CAD" export "$C/inc/main_use.scad" --per-color -o "$OUT/x.stl" > "$OUT/f9-stl.txt" 2>&1 || true
"$CAD" export "$C/inc/main_use.scad" --per-color --part teil -o "$OUT/teil.zip" > "$OUT/f9-part.txt" 2>&1 || true

# 3MF of the example for the slicers (no colour group) and with --color-group
EX="tests/models/spec/multicolor.mcad"
"$CAD" measure "$EX" --colors --json > "$OUT/measure.json"
"$CAD" export "$EX" -o "$OUT/mehrfarbig.3mf" > "$OUT/export.txt"
"$CAD" export "$EX" -o "$OUT/mehrfarbig-cg.3mf" --color-group > "$OUT/export-cg.txt"
"$PY3MF" tools/report/verify3mf.py "$OUT/mehrfarbig.3mf" --expect "$OUT/measure.json" --json "$OUT/verify.json" > "$OUT/verify.txt" 2>&1 || true
"$PY3MF" tools/report/verify3mf.py "$OUT/mehrfarbig-cg.3mf" --expect "$OUT/measure.json" --json "$OUT/verify-cg.json" > "$OUT/verify-cg.txt" 2>&1 || true
"$PY3MF" tests/color-review/check3mf.py "$OUT/main_use.3mf" > "$OUT/check3mf-main_use.txt" 2>&1 || true

# daemon with a long CAD_IPC_DIR (socket path > 107 bytes)
LONG="$(mktemp -d)/a-very-long-directory-name-for-the-socket-of-the-background-process-so-that-it-is-long-enough/ipc"
mkdir -p "$LONG"
(
  unset CAD_NO_DAEMON
  export CAD_IPC_DIR="$LONG"
  echo "CAD_IPC_DIR: $LONG ($(( ${#LONG} + 12 )) bytes up to daemon.sock)"
  "$CAD" check "$EX" --verbose 2>&1 | grep -i daemon || true
  "$CAD" daemon status
  "$CAD" daemon stop
) > "$OUT/daemon-long.txt" 2>&1 || true
rm -rf "$(dirname "$(dirname "$LONG")")"

# slicer sources (Bambu colour dialog)
SRC="$OUT/sources"
mkdir -p "$SRC"
for u in \
  "bambulab/BambuStudio/master/src/slic3r/GUI/Plater.cpp" \
  "bambulab/BambuStudio/master/src/slic3r/GUI/ObjColorDialog.cpp" \
  "bambulab/BambuStudio/master/src/libslic3r/Format/bbs_3mf.cpp" \
  "SoftFever/OrcaSlicer/main/src/libslic3r/Format/bbs_3mf.cpp" \
  "SoftFever/OrcaSlicer/main/src/slic3r/GUI/Plater.cpp"; do
  f="$SRC/$(echo "$u" | tr / _)"
  [ -s "$f" ] || curl -sSL -o "$f" "https://raw.githubusercontent.com/$u" || true
done
( cd "$SRC" && sha256sum ./* > ../sources.sha256 )
{
  echo "== BambuStudio Plater.cpp: colour dialog for foreign 3MF"
  grep -n "color_group_map.empty() && !volume_color_data.empty()" "$SRC/bambulab_BambuStudio_master_src_slic3r_GUI_Plater.cpp" || true
  echo "== BambuStudio bbs_3mf.cpp: triangle colours only from pid/pindex"
  grep -n "triangle_colors.size() == triangles_count" "$SRC/bambulab_BambuStudio_master_src_libslic3r_Format_bbs_3mf.cpp" || true
  grep -n 'boost::starts_with(m_curr_characters, "BambuStudio-")' "$SRC/bambulab_BambuStudio_master_src_libslic3r_Format_bbs_3mf.cpp" || true
  echo "== BambuStudio bbs_3mf.cpp: model_settings.config for all files"
  grep -n "iequals(name, BBS_MODEL_CONFIG_FILE)" "$SRC/bambulab_BambuStudio_master_src_libslic3r_Format_bbs_3mf.cpp" || true
  echo "== BambuStudio ObjColorDialog.cpp: OK overwrites extruder"
  grep -n "apply_volume_filament_ids\|volume->config.set(\"extruder\"" "$SRC/bambulab_BambuStudio_master_src_slic3r_GUI_ObjColorDialog.cpp" || true
  echo "== OrcaSlicer bbs_3mf.cpp: colour group only without volume metadata"
  grep -n "color_group_id_to_extruder_id_map.find" "$SRC/SoftFever_OrcaSlicer_main_src_libslic3r_Format_bbs_3mf.cpp" || true
  echo "== OrcaSlicer Plater.cpp: occurrences of color_group_map (0: no colour dialog for 3MF)"
  grep -c "color_group_map" "$SRC/SoftFever_OrcaSlicer_main_src_slic3r_GUI_Plater.cpp" || true
} > "$OUT/sources.txt"
echo "report12 data: $OUT"
