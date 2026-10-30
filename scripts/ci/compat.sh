#!/usr/bin/env bash
# OpenSCAD comparison and STEP check.
#
#   scripts/ci/compat.sh cli         # cargo build --release -p cad-cli --no-default-features (headless, needs SMLIB_PREBUILT)
#   scripts/ci/compat.sh reference   # OpenSCAD 2021.01: regenerate the reference into target/,
#                                    # compare with the checked-in reference.json, self-test of compare.py
#   scripts/ci/compat.sh projects    # include/use projects against a live OpenSCAD run (Rust test)
#   scripts/ci/compat.sh compare     # compare.py --eval with the release CLI (33 models)
#   scripts/ci/compat.sh step        # tests/step/verify.py with OpenCASCADE (STEP_PYTHON, see step-venv.sh)
#
# Env: MCAD (old name CAD; default target/release/mcad), OPENSCAD (default openscad), STEP_PYTHON (default python3).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
MCAD="${MCAD:-${CAD:-$ROOT/target/release/mcad}}"
OPENSCAD="${OPENSCAD:-openscad}"
REF="$ROOT/tests/openscad-compat/reference.json"
TMP="$ROOT/target/openscad-compat"

need_openscad() {
  local v
  v=$("$OPENSCAD" --version 2>&1 || true)
  case "$v" in
    *2021.01*) echo "$v" ;;
    *) echo "OpenSCAD 2021.01 required, found: ${v:-none}" >&2; exit 2 ;;
  esac
}

# Runs a command whose `mcad` calls go through the CLI daemon (as for users), with a socket
# directory of its own; the daemon is stopped afterwards, also on failure.
with_daemon() {
  local ipc="$TMP/ipc-$$"
  mkdir -p "$ipc"
  local rc=0
  MCAD_IPC_DIR="$ipc" "$@" || rc=$?
  MCAD_IPC_DIR="$ipc" "$MCAD" daemon status || true
  MCAD_IPC_DIR="$ipc" "$MCAD" daemon stop || true
  rm -rf "$ipc"
  return $rc
}

step() {
  case "$1" in
    cli)
      cargo build --release -p cad-cli --no-default-features --locked
      ls -l "$ROOT/target/release/mcad"
      ;;
    reference)
      need_openscad
      mkdir -p "$TMP"
      cp "$REF" "$TMP/reference.checked-in.json"
      # make-reference.py writes reference.json in place; put the checked-in file back in any case.
      trap 'cp "$TMP/reference.checked-in.json" "$REF"' EXIT
      python3 tests/openscad-compat/make-reference.py --openscad "$OPENSCAD"
      cp "$REF" "$TMP/reference.fresh.json"
      cp "$TMP/reference.checked-in.json" "$REF"
      trap - EXIT
      python3 scripts/ci/compare-reference.py "$TMP/reference.checked-in.json" "$TMP/reference.fresh.json"
      python3 tests/openscad-compat/compare.py --self-test
      ;;
    projects)
      need_openscad
      cargo test -p cad-eval --test project_openscad --locked
      ;;
    compare)
      [ -x "$MCAD" ] || { echo "CLI not found at $MCAD (scripts/ci/compat.sh cli)" >&2; exit 2; }
      with_daemon env MCAD="$MCAD" python3 tests/openscad-compat/compare.py --eval
      ;;
    step)
      [ -x "$MCAD" ] || { echo "CLI not found at $MCAD (scripts/ci/compat.sh cli)" >&2; exit 2; }
      with_daemon "${STEP_PYTHON:-python3}" tests/step/verify.py --cad "$MCAD"
      ;;
    *) echo "unknown step: $1 (cli|reference|projects|compare|step)" >&2; exit 2 ;;
  esac
}

[ $# -gt 0 ] || { sed -n '2,12p' "$0"; exit 2; }
for s in "$@"; do
  echo "== compat.sh $s"
  step "$s"
done
