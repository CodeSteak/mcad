#!/usr/bin/env bash
# Test runs of report 22 (studio look) into reports/tmp/report22/final/: one log per step and
# times.txt with "<step> <exit code> <seconds>". Without WebKitGTK the workspace is tested
# without the desktop app (cad-app, cad-cli feature `gui`); the rest as in scripts/ci/.
#
#   SMLIB_PREBUILT=… SMLIB_SRC=… tools/report/report22-tests.sh
#   PW_CONFIG=playwright.local.config.ts tools/report/report22-tests.sh   # local Chromium
#   ONLY=playwright PW_CHUNKS=1 tools/report/report22-tests.sh            # resumable chunks
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT="$ROOT/reports/tmp/report22/final"
mkdir -p "$OUT"
[ -n "${ONLY:-}" ] || : > "$OUT/times.txt"
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

# ONLY="playwright" (space separated) runs only these steps
want() { [ -z "${ONLY:-}" ] || [[ " $ONLY " == *" $1 "* ]]; }

# PW_CHUNKS=1: the Playwright suite in four chunks (pw-a…d.{txt,json}); a chunk whose JSON
# exists is skipped, so an interrupted run can be resumed
pw_chunk() {
  local name=$1
  shift
  [ -f "$OUT/pw-$name.json" ] && { echo "pw-$name: done before"; return; }
  step "pw-$name" bash -c "cd web && timeout 3000 npx playwright test -c ${PW_CONFIG:-playwright.config.ts} $*"
  cp "$ROOT/reports/tmp/report2/playwright.json" "$OUT/pw-$name.json.tmp" && mv "$OUT/pw-$name.json.tmp" "$OUT/pw-$name.json"
}

want fmt && step fmt cargo fmt --all --check
want clippy && step clippy cargo clippy --workspace --exclude cad-app --no-default-features --all-targets --locked -- -D warnings
want cargo && step cargo cargo test --workspace --exclude cad-app --no-default-features --locked --no-fail-fast
want usage && step usage cargo test -p cad-cli --no-default-features --locked usage
want typecheck && step typecheck bash -c "cd web && pnpm typecheck"
want vitest && step vitest bash -c "cd web && pnpm test"
if want playwright; then
  if [ -n "${PW_CHUNKS:-}" ]; then
    pw_chunk a e2e/asserts.spec.ts e2e/color-fixes.spec.ts e2e/colors.spec.ts e2e/language.spec.ts e2e/measure.spec.ts e2e/phase-d.spec.ts
    pw_chunk b e2e/projects.spec.ts e2e/section.spec.ts e2e/sketch-corners.spec.ts e2e/sketch-editor.spec.ts e2e/sketches.spec.ts e2e/stencil.spec.ts
    pw_chunk c e2e/studio.spec.ts e2e/text-svg.spec.ts e2e/ui-b.spec.ts e2e/ui-e.spec.ts
    pw_chunk d e2e/ui-c.spec.ts e2e/ui-overhaul.spec.ts e2e/ui.spec.ts
  else
    step playwright bash -c "cd web && npx playwright test -c ${PW_CONFIG:-playwright.config.ts}"
    cp "$ROOT/reports/tmp/report2/playwright.json" "$OUT/playwright.json" 2>/dev/null || true
  fi
fi
