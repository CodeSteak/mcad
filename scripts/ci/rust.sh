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
CARGO="${CARGO:-cargo}"   # overridable, as in the Makefile (make CARGO=/path/to/cargo)

need_smlib() {
  if [ -z "${SMLIB_PREBUILT:-}" ]; then
    echo "note: SMLIB_PREBUILT is not set; build.rs builds SMLib with CMake" >&2
  fi
}

step() {
  case "$1" in
    fmt) "$CARGO" fmt --all --check ;;
    clippy) need_smlib; "$CARGO" clippy --workspace --all-targets --locked -- -D warnings ;;
    clippy-wasm) "$CARGO" clippy -p cad-wasm --target wasm32-unknown-unknown --locked -- -D warnings ;;
    test)
      need_smlib
      "$CARGO" test --workspace --locked
      # tests/daemon.rs stops its daemons: none may be left (checked in CI only, a local
      # daemon of the developer would count too)
      sleep 1
      if [ -n "${CI:-}" ] && pgrep -f 'cad daemon serve' >/dev/null; then
        echo "CLI daemon left behind by the tests:" >&2
        pgrep -af 'cad daemon serve' >&2
        pkill -f 'cad daemon serve' || true
        exit 1
      fi
      ;;
    wasm-check) "$CARGO" check -p cad-wasm --target wasm32-unknown-unknown --locked ;;
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
