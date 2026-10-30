#!/usr/bin/env bash
# SMLib for native builds (CLI, cargo test): fetch the fork + patches (smlib/fetch.sh, also
# needed when the libraries exist: build.rs compiles the facade against the SMLib headers) and build
# the static libraries with smlib/build-native.sh unless they already exist.
#
#   scripts/ci/smlib-native.sh [OUT_DIR]      # default: smlib/build-native
#
# Afterwards SMLIB_PREBUILT=<OUT_DIR>.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT="${1:-$ROOT/smlib/build-native}"

"$ROOT/smlib/fetch.sh"

if [ -f "$OUT/lib/libSMLib.a" ] && [ -f "$OUT/lib/libSM_API.a" ]; then
  echo "native SMLib: using existing libraries in $OUT/lib"
else
  JOBS="${JOBS:-$(nproc)}" "$ROOT/smlib/build-native.sh" "$OUT"
fi
OUT="$(cd "$OUT" && pwd)"
ls -l "$OUT/lib"
echo "SMLIB_PREBUILT=$OUT"
