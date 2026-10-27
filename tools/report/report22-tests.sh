#!/usr/bin/env bash
# Test runs of report 22 (studio look) into reports/tmp/report22/final/: one log per step and
# times.txt with "<step> <exit code> <seconds>". Without WebKitGTK the workspace is tested
# without the desktop app (cad-app, cad-cli feature `gui`); the rest as in scripts/ci/.
#
#   SMLIB_PREBUILT=… SMLIB_SRC=… tools/report/report22-tests.sh
#   PW_CONFIG=playwright.local.config.ts tools/report/report22-tests.sh   # local Chromium
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT="$ROOT/reports/tmp/report22/final"
mkdir -p "$OUT"
: > "$OUT/times.txt"
cd "$ROOT"
export MCAD_NO_DAEMON=1

step() {
  local name=$1
  shift
  local t0
  t0=$(date +%s)
  ("$@") > "$OUT/$name.txt" 2>&1
  local rc=$?
  echo "$name $rc $(($(date +%s) - t0))" >> "$OUT/times.txt"
  echo "$name: exit $rc"
}

step fmt cargo fmt --all --check
step clippy cargo clippy --workspace --exclude cad-app --no-default-features --all-targets --locked -- -D warnings
step cargo cargo test --workspace --exclude cad-app --no-default-features --locked --no-fail-fast
step usage cargo test -p cad-cli --no-default-features --locked usage
step typecheck bash -c "cd web && pnpm typecheck"
step vitest bash -c "cd web && pnpm test"
step playwright bash -c "cd web && npx playwright test -c ${PW_CONFIG:-playwright.config.ts}"
cp "$ROOT/reports/tmp/report2/playwright.json" "$OUT/playwright.json" 2>/dev/null || true
