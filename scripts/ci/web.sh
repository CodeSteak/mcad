#!/usr/bin/env bash
# Browser build and tests (from the repository root).
#
#   scripts/ci/web.sh wasm        # cad-wasm + wasm-bindgen + wasm-opt -> web/src/wasm-pkg/
#                                 # (needs web/public/smlib/smlib.{js,wasm}: scripts/ci/smlib-wasm.sh)
#   scripts/ci/web.sh install     # pnpm install --frozen-lockfile
#   scripts/ci/web.sh typecheck   # tsc --noEmit
#   scripts/ci/web.sh unit        # vitest
#   scripts/ci/web.sh node        # Node tests: smlib.wasm via the bridge, engine smoke test, transport
#   scripts/ci/web.sh build       # vite build -> web/dist
#   scripts/ci/web.sh e2e         # Playwright (Chromium from PLAYWRIGHT_BROWSERS_PATH or `playwright install`)
#
# Several steps can be given at once: scripts/ci/web.sh install typecheck unit build
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WEB="$ROOT/web"
PNPM="${PNPM:-pnpm}"   # overridable, as in the Makefile

step() {
  case "$1" in
    wasm) "$WEB/scripts/build-wasm.sh" ;;
    install) (cd "$WEB" && "$PNPM" install --frozen-lockfile) ;;
    typecheck) (cd "$WEB" && "$PNPM" typecheck) ;;
    unit) (cd "$WEB" && "$PNPM" test) ;;
    node)
      node "$ROOT/crates/kernel-smlib/js/test-node.mjs" "$WEB/public/smlib"
      node "$WEB/scripts/smoke-node.mjs"
      node "$WEB/scripts/test-transport.mjs"
      ;;
    build) (cd "$WEB" && "$PNPM" build) ;;
    e2e) (cd "$WEB" && "$PNPM" e2e) ;;
    *) echo "unknown step: $1 (wasm|install|typecheck|unit|node|build|e2e)" >&2; exit 2 ;;
  esac
}

[ $# -gt 0 ] || { sed -n '2,14p' "$0"; exit 2; }
for s in "$@"; do
  echo "== web.sh $s"
  step "$s"
done
