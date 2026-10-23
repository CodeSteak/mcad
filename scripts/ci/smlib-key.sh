#!/usr/bin/env bash
# Cache key for the prebuilt SMLib libraries (GitHub Actions cache, see .github/actions/).
#
#   scripts/ci/smlib-key.sh native     # smlib/build-native/lib/{libSMLib,libSM_API}.a
#   scripts/ci/smlib-key.sh wasm       # web/public/smlib/{smlib.js,smlib.wasm}
#
# Output: smlib-<kind>-<fork commit>-<hash>, the hash over the patches, smlib/CMakeLists.txt,
# the build script, the compiler (c++ or emcc) and CMake; for wasm also the facade sources
# (they are linked into smlib.wasm; natively the facade is compiled by build.rs, not cached).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
kind="${1:?usage: smlib-key.sh native|wasm}"

commit=$(sed -n 's/^COMMIT=\([0-9a-f]*\)$/\1/p' "$ROOT/smlib/fetch.sh")
[ -n "$commit" ] || { echo "COMMIT not found in smlib/fetch.sh" >&2; exit 2; }

files=("$ROOT"/smlib/patches/*.patch "$ROOT/smlib/CMakeLists.txt")
case "$kind" in
  native)
    files+=("$ROOT/smlib/build-native.sh")
    compiler=$("${CXX:-c++}" --version | head -n 1)
    ;;
  wasm)
    files+=("$ROOT/smlib/build-wasm.sh" "$ROOT"/smlib/facade/*)
    emcc_bin=$(command -v emcc || echo "${EMSDK:-}/upstream/emscripten/emcc")
    [ -x "$emcc_bin" ] || { echo "emcc not found (scripts/ci/emsdk.sh, or set EMSDK)" >&2; exit 2; }
    compiler=$("$emcc_bin" --version | head -n 1)
    ;;
  *) echo "unknown kind: $kind" >&2; exit 2 ;;
esac

hash=$(
  {
    for f in "${files[@]}"; do
      echo "== ${f#"$ROOT"/}"
      cat "$f"
    done
    echo "== compiler: $compiler"
    echo "== $(cmake --version | head -n 1)"
  } | sha256sum | cut -c1-16
)
echo "smlib-$kind-${commit:0:12}-$hash"
