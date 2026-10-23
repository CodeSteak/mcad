#!/usr/bin/env bash
# Inputs of test report 13 ("Korrekturen Geometrie") in reports/tmp/report13/:
#   fehler/<name>.{vorher,nachher}.json   cad measure --volume --json of each example in
#                                          tests/alltag/fehler (before/after)
#   img/<name>.{vorher,nachher}.png        cad render of the examples and of S1/S4 views
#   zeit.{vorher,nachher}.json             timings of cad render schluesselbrett (3 runs)
#   fehler.txt                             tests/alltag/check.sh --fehler (after)
#
#   CAD_OLD=<cad before the corrections> tools/report/report13-data.sh
#
# CAD_OLD: a release build of the commit before the corrections (cba3ab4), e.g. from
# `git archive cba3ab4 crates Cargo.toml Cargo.lock smlib | tar -x -C OLD` with
# OLD/third_party/smlib -> the SMLib checkout with patch 0001 only, then
# `SMLIB_PREBUILT=<libs with 0001> cargo build --release -p cad-cli` there.
# CAD: the current build (default target/release/cad). Both run without daemon.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
out="$root/reports/tmp/report13"
CAD=${CAD:-$root/target/release/cad}
: "${CAD_OLD:?CAD_OLD=<cad before the corrections> setzen}"
export CAD_NO_DAEMON=1
mkdir -p "$out/fehler" "$out/img"
fe="$root/tests/alltag/fehler"
al="$root/examples/alltag"

for f in "$fe"/*.scad; do
  n=$(basename "$f" .scad)
  for v in vorher nachher; do
    bin=$CAD; [ $v = vorher ] && bin=$CAD_OLD
    (cd "$fe" && timeout 300 "$bin" measure "$f" --volume --json > "$out/fehler/$n.$v.json" 2>/dev/null) || true
    [ -s "$out/fehler/$n.$v.json" ] || echo '{"crash": true}' > "$out/fehler/$n.$v.json"
  done
done

# images: examples with a visible result (iso), sections where the inside matters
render() {  # name file dir args...
  local n=$1 f=$2 d=$3; shift 3
  for v in vorher nachher; do
    bin=$CAD; [ $v = vorher ] && bin=$CAD_OLD
    (cd "$d" && timeout 300 "$bin" render "$f" -o "$out/img/$n.$v.png" --size 800x600 "$@" > /dev/null 2>&1) || true
  done
}
render f01 "$fe/f01_sweep_spline_profil.scad" "$fe" --view iso
render f05 "$fe/f05_fase_volumen_null.scad" "$fe" --view iso
render f07 "$fe/f07_torus_zone.scad" "$fe" --view iso --line 6
render f08a "$fe/f08a_union_torus.scad" "$fe" --section y=0 --view back --target 20,0,1 --zoom 6
render f08b "$fe/f08b_union_ring_verloren.scad" "$fe" --section y=0 --view back --target 20,0,1 --zoom 6
render f08c "$fe/f08c_differenz_torus_tangential.scad" "$fe" --section y=0 --view back --target 20,0,1 --zoom 6
render f10 "$fe/f10_union_fillet_schale_verloren.scad" "$fe" --view iso
render s1 "$al/becher.scad" "$al" --module becher --show grids --view front
render s4 "$al/gehaeuse/gehaeuse.scad" "$al/gehaeuse" --section x=30 --view right
render s4b "$al/kuehlkoerper.scad" "$al" --module kuehlkoerper --show grids

# run time of the Schlüsselbrett: 3 renders each
for v in vorher nachher; do
  bin=$CAD; [ $v = vorher ] && bin=$CAD_OLD
  echo "[" > "$out/zeit.$v.json"
  for i in 1 2 3; do
    (cd "$al/schluesselbrett" && "$bin" render schluesselbrett.scad -o "$out/img/sb.$v.png" --json 2>/dev/null) \
      | python3 -c 'import json,sys; print(json.dumps(json.load(sys.stdin)["timings"]))' >> "$out/zeit.$v.json"
    [ $i -lt 3 ] && echo "," >> "$out/zeit.$v.json"
  done
  echo "]" >> "$out/zeit.$v.json"
done

CAD="$CAD" "$root/tests/alltag/check.sh" --fehler nichts > "$out/fehler.txt" 2>&1 || true
echo "inputs in $out"
