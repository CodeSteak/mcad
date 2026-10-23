#!/usr/bin/env bash
# SMLib as Wasm (smlib.js + smlib.wasm in web/public/smlib/, input of web/scripts/build-wasm.sh):
# fetch the fork + patches and run smlib/build-wasm.sh unless both files already exist
# (CI cache). Needs emcc (scripts/ci/emsdk.sh).
#
#   scripts/ci/smlib-wasm.sh [--force]
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT="$ROOT/web/public/smlib"

if [ "${1:-}" != "--force" ] && [ -f "$OUT/smlib.js" ] && [ -f "$OUT/smlib.wasm" ]; then
  echo "smlib.wasm: using existing build in $OUT"
else
  "$ROOT/smlib/fetch.sh"
  JOBS="${JOBS:-$(nproc)}" "$ROOT/smlib/build-wasm.sh" "$OUT"
fi
ls -l "$OUT"/smlib.js "$OUT"/smlib.wasm
