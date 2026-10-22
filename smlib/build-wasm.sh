#!/usr/bin/env bash
# Build SMLib + SM_API + the controller-cad facade with Emscripten into
# smlib.js (ES module factory `createSmlib`) and smlib.wasm.
#
#   smlib/build-wasm.sh [OUT_DIR] [--libs-only] [--debug]
#
# OUT_DIR defaults to web/public/smlib/. Requires emsdk (emcc on PATH, or EMSDK set,
# or /opt/tools/emsdk). Sources come from SMLIB_SRC or third_party/usd-brep
# (see smlib/fetch.sh). The CMake build tree is smlib/build-wasm (cached).
#
# Flags (see smlib/CMakeLists.txt): -O3 -fwasm-exceptions, MODULARIZE, EXPORT_ES6,
# ALLOW_MEMORY_GROWTH, no threads, ENVIRONMENT=worker,web,node, 8 MiB stack.
# Env: SMLIB_WASM_OPT (default -O3, e.g. -Oz), SMLIB_WASM_BUILD (build dir), JOBS (default 4).
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
OUT="$ROOT/web/public/smlib"
LIBS_ONLY=0
BUILD_TYPE=Release
for a in "$@"; do
  case "$a" in
    --libs-only) LIBS_ONLY=1 ;;
    --debug) BUILD_TYPE=RelWithDebInfo ;;
    -*) echo "unknown flag: $a" >&2; exit 2 ;;
    *) OUT="$a" ;;
  esac
done
SRC="${SMLIB_SRC:-$ROOT/third_party/usd-brep}"
JOBS="${JOBS:-4}"
BUILD="${SMLIB_WASM_BUILD:-$HERE/build-wasm}"

if ! command -v emcc >/dev/null 2>&1; then
  EMSDK_DIR="${EMSDK:-/opt/tools/emsdk}"
  # shellcheck disable=SC1091
  . "$EMSDK_DIR/emsdk_env.sh" >/dev/null 2>&1
fi
command -v emcc >/dev/null || { echo "emcc not found (install emsdk)" >&2; exit 1; }

GEN=()
if command -v ninja >/dev/null 2>&1; then GEN=(-G Ninja); fi

start=$(date +%s)
emcmake cmake -S "$HERE" -B "$BUILD" "${GEN[@]}" \
  -DCMAKE_BUILD_TYPE="$BUILD_TYPE" \
  -DCMAKE_CXX_FLAGS_RELEASE="${SMLIB_WASM_OPT:--O3} -DNDEBUG" \
  -DCMAKE_CXX_FLAGS_RELWITHDEBINFO="-O2 -g -DNDEBUG" \
  -DSMLIB_SRC="$SRC" \
  -DSC_BUILD_FACADE=ON \
  -DSC_WASM=ON
if [ "$LIBS_ONLY" = 1 ]; then
  cmake --build "$BUILD" --parallel "$JOBS" --target SMLib SM_API
  echo "wasm libs built in $(( $(date +%s) - start )) s"
  exit 0
fi
cmake --build "$BUILD" --parallel "$JOBS" --target smlib

mkdir -p "$OUT"
cp -f "$BUILD/smlib.js" "$BUILD/smlib.wasm" "$OUT/"
end=$(date +%s)
echo "wasm build finished in $((end - start)) s -> $OUT"
for f in "$OUT/smlib.wasm" "$OUT/smlib.js"; do
  raw=$(stat -c %s "$f")
  gz=$(gzip -9 -c "$f" | wc -c)
  line="$(basename "$f"): raw $raw B, gzip -9 $gz B"
  if command -v brotli >/dev/null 2>&1; then
    br=$(brotli -q 11 -c "$f" | wc -c)
    line="$line, brotli -q 11 $br B"
  elif command -v node >/dev/null 2>&1; then
    br=$(node -e 'const z=require("zlib"),b=require("fs").readFileSync(process.argv[1]);
      console.log(z.brotliCompressSync(b,{params:{[z.constants.BROTLI_PARAM_QUALITY]:11,
      [z.constants.BROTLI_PARAM_SIZE_HINT]:b.length}}).length)' "$f")
    line="$line, brotli q11 (node zlib) $br B"
  fi
  echo "$line"
done
