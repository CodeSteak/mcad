#!/usr/bin/env bash
# Rust checks as run by .github/workflows/rust.yml.
#
#   scripts/ci/rust.sh fmt | clippy | clippy-wasm | test | wasm-check | all
#
# clippy and test link the native SMLib: set SMLIB_PREBUILT (scripts/ci/smlib-native.sh);
# without it crates/kernel-smlib/build.rs builds SMLib itself (several minutes).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

need_smlib() {
  if [ -z "${SMLIB_PREBUILT:-}" ]; then
    echo "note: SMLIB_PREBUILT is not set; build.rs builds SMLib with CMake" >&2
  fi
}

step() {
  case "$1" in
    fmt) cargo fmt --all --check ;;
    clippy) need_smlib; cargo clippy --workspace --all-targets --locked -- -D warnings ;;
    clippy-wasm) cargo clippy -p cad-wasm --target wasm32-unknown-unknown --locked -- -D warnings ;;
    test) need_smlib; cargo test --workspace --locked ;;
    wasm-check) cargo check -p cad-wasm --target wasm32-unknown-unknown --locked ;;
    *) echo "unknown step: $1 (fmt|clippy|clippy-wasm|test|wasm-check|all)" >&2; exit 2 ;;
  esac
}

if [ "${1:-all}" = all ]; then
  for s in fmt clippy clippy-wasm test wasm-check; do
    echo "== rust.sh $s"
    step "$s"
  done
else
  for s in "$@"; do step "$s"; done
fi
