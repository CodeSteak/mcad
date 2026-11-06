#!/usr/bin/env bash
# Builds the engine for the browser worker (pure Rust, kernel solidcore included):
#   crates/cad-wasm (wasm32-unknown-unknown, profile `wasm`) → wasm-bindgen --target web
#   → web/src/wasm-pkg/
#
#   web/scripts/build-wasm.sh [--debug] [--no-opt]
#
# * Profile `wasm` (Cargo.toml: release with opt-level "z", fat LTO, one codegen unit);
#   --debug: the dev profile.
# * wasm-bindgen-cli is installed in the version of the `wasm-bindgen` crate in Cargo.lock
#   if the installed one differs (scripts/ci/wasm-bindgen.sh).
# * wasm-opt (PATH) runs with -Oz unless --no-opt; without it the module stays as built.
#
# Output (gitignored): web/src/wasm-pkg/{cad_wasm.js, cad_wasm_bg.wasm, cad_wasm.d.ts}.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WEB="$(cd "$HERE/.." && pwd)"
ROOT="$(cd "$WEB/.." && pwd)"
OUT="$WEB/src/wasm-pkg"
PROFILE=wasm
OPT=1
for a in "$@"; do
  case "$a" in
    --debug) PROFILE=debug ;;
    --no-opt) OPT=0 ;;
    *) echo "unknown argument: $a" >&2; exit 2 ;;
  esac
done

# 1. Rust → wasm32
cargo_flags=(-p cad-wasm --target wasm32-unknown-unknown)
[ "$PROFILE" = wasm ] && cargo_flags+=(--profile wasm)
(cd "$ROOT" && "${CARGO:-cargo}" build "${cargo_flags[@]}")
WASM="${CARGO_TARGET_DIR:-$ROOT/target}/wasm32-unknown-unknown/$PROFILE/cad_wasm.wasm"

# 2. wasm-bindgen-cli in the version of the crate
"$ROOT/scripts/ci/wasm-bindgen.sh"

rm -rf "$OUT"
mkdir -p "$OUT"
wasm-bindgen --target web --out-dir "$OUT" "$WASM"

# 3. wasm-opt
if [ "$OPT" = 1 ]; then
  WASM_OPT="$(command -v wasm-opt || true)"
  if [ -n "$WASM_OPT" ]; then
    "$WASM_OPT" -Oz --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext \
      --enable-mutable-globals --enable-reference-types --enable-multivalue \
      "$OUT/cad_wasm_bg.wasm" -o "$OUT/cad_wasm_bg.opt.wasm" \
      && mv "$OUT/cad_wasm_bg.opt.wasm" "$OUT/cad_wasm_bg.wasm" \
      || echo "wasm-opt failed; keeping the unoptimised module" >&2
  else
    echo "wasm-opt not found; skipping" >&2
  fi
fi

f="$OUT/cad_wasm_bg.wasm"
raw=$(stat -c %s "$f")
gz=$(gzip -9 -c "$f" | wc -c)
br=$(command -v brotli >/dev/null && brotli -c -q 11 "$f" | wc -c || echo "-")
echo "$(basename "$f"): $raw B, gzip -9 $gz B, brotli $br B"
echo "-> $OUT"
