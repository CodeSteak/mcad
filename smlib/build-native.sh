#!/usr/bin/env bash
# Build SMLib + SM_API natively as static libraries, for use via SMLIB_PREBUILT:
#
#   smlib/build-native.sh [OUT_DIR]          # default: smlib/build-native
#   SMLIB_PREBUILT=smlib/build-native cargo test -p kernel-smlib
#
# Without SMLIB_PREBUILT, crates/kernel-smlib/build.rs builds the same libraries itself
# (in OUT_DIR, cached by cargo). This script only exists to share one build between
# worktrees/profiles and to speed up iteration.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUT="${1:-$HERE/build-native}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"
SRC="${SMLIB_SRC:-$HERE/../third_party/smlib}"
JOBS="${JOBS:-4}"

GEN=()
if command -v ninja >/dev/null 2>&1; then GEN=(-G Ninja); fi

start=$(date +%s)
cmake -S "$HERE" -B "$OUT/build" "${GEN[@]}" \
  -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_CXX_FLAGS_RELEASE="-O2 -DNDEBUG" \
  -DSMLIB_SRC="$SRC" \
  -DSC_BUILD_FACADE=OFF
cmake --build "$OUT/build" --parallel "$JOBS" --target SMLib SM_API
mkdir -p "$OUT/lib"
cp -f "$OUT/build/libSMLib.a" "$OUT/build/libSM_API.a" "$OUT/lib/"
end=$(date +%s)
echo "native SMLib build finished in $((end - start)) s; libs in $OUT/lib"
