#!/usr/bin/env bash
# Inputs of test report 13 ("geometry fixes") in reports/tmp/report13/:
#   fehler/<name>.{vorher,nachher}.json   cad measure --volume --json of each example in
#                                          tests/everyday/bugs (before/after)
#   img/<name>.{vorher,nachher}.png        cad render of the examples and of S1/S4 views
#   zeit.{vorher,nachher}.json             timings of cad render schluesselbrett (3 runs)
#   fehler.txt                             tests/everyday/check.sh --bugs (after)
#
#   CAD_OLD=<cad before the corrections> tools/report/report13-data.sh
#
# CAD_OLD: a release build of the commit before the corrections (cba3ab4), e.g. from
# `git archive cba3ab4 crates Cargo.toml Cargo.lock smlib | tar -x -C OLD` with
# OLD/third_party/smlib -> the SMLib checkout with patch 0001 only, then
# `SMLIB_PREBUILT=<libs with 0001> cargo build --release -p cad-cli` there.
# CAD: the current build (default target/release/mcad). Both run without daemon.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
out="$root/reports/tmp/report13"
CAD=${CAD:-$root/target/release/mcad}
: "${CAD_OLD:?CAD_OLD=<cad before the corrections> must be set}"
export CAD_NO_DAEMON=1
mkdir -p "$out/fehler" "$out/img"
fe="$root/tests/everyday/bugs"
al="$root/tests/models/everyday"

for f in "$fe"/*.mcad; do
  n=$(basename "$f" .mcad)
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
render f01 "$fe/f01_sweep_spline_profile.mcad" "$fe" --view iso
render f05 "$fe/f05_chamfer_volume_zero.mcad" "$fe" --view iso
render f07 "$fe/f07_torus_zone.mcad" "$fe" --view iso --line 6
render f08a "$fe/f08a_union_torus.mcad" "$fe" --section y=0 --view back --target 20,0,1 --zoom 6
render f08b "$fe/f08b_union_ring_lost.mcad" "$fe" --section y=0 --view back --target 20,0,1 --zoom 6
render f08c "$fe/f08c_difference_torus_tangential.mcad" "$fe" --section y=0 --view back --target 20,0,1 --zoom 6
render f10 "$fe/f10_union_fillet_shell_lost.mcad" "$fe" --view iso
render s1 "$al/mug.mcad" "$al" --module mug --show grids --view front
render s4 "$al/housing/housing.mcad" "$al/housing" --section x=30 --view right
render s4b "$al/heat_sink.mcad" "$al" --module heat_sink --show grids

# run time of the key rack (schluesselbrett): 3 renders each
for v in vorher nachher; do
  bin=$CAD; [ $v = vorher ] && bin=$CAD_OLD
  echo "[" > "$out/zeit.$v.json"
  for i in 1 2 3; do
    (cd "$al/key_rack" && "$bin" render key_rack.mcad -o "$out/img/sb.$v.png" --json 2>/dev/null) \
      | python3 -c 'import json,sys; print(json.dumps(json.load(sys.stdin)["timings"]))' >> "$out/zeit.$v.json"
    [ $i -lt 3 ] && echo "," >> "$out/zeit.$v.json"
  done
  echo "]" >> "$out/zeit.$v.json"
done

CAD="$CAD" "$root/tests/everyday/check.sh" --fehler nichts > "$out/fehler.txt" 2>&1 || true
echo "inputs in $out"
