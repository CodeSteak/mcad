#!/usr/bin/env bash
# wasm-bindgen-cli in the version of the `wasm-bindgen` crate in Cargo.lock (they must match).
# Installed into ~/.cargo/bin: prebuilt release binary if available, else cargo install.
# Nothing happens if the right version is already on PATH.
#
#   scripts/ci/wasm-bindgen.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
want=$(awk '/^name = "wasm-bindgen"$/ { getline; gsub(/version = |"/, ""); print; exit }' "$ROOT/Cargo.lock")
[ -n "$want" ] || { echo "wasm-bindgen not found in Cargo.lock" >&2; exit 2; }
have=$(wasm-bindgen --version 2>/dev/null | awk '{ print $2 }' || true)
if [ "$have" = "$want" ]; then
  echo "wasm-bindgen $want: already installed ($(command -v wasm-bindgen))"
  exit 0
fi

BIN="${CARGO_HOME:-$HOME/.cargo}/bin"
mkdir -p "$BIN"
name="wasm-bindgen-$want-x86_64-unknown-linux-musl"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
if [ "$(uname -sm)" = "Linux x86_64" ] &&
  curl -fsSL --retry 3 -o "$tmp/wb.tar.gz" \
    "https://github.com/wasm-bindgen/wasm-bindgen/releases/download/$want/$name.tar.gz"; then
  tar -xzf "$tmp/wb.tar.gz" -C "$tmp"
  install -m 755 "$tmp/$name/wasm-bindgen" "$BIN/wasm-bindgen"
else
  echo "no prebuilt wasm-bindgen $want; cargo install"
  cargo install wasm-bindgen-cli --version "$want" --locked
fi
"$BIN/wasm-bindgen" --version
