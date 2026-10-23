#!/usr/bin/env bash
# Desktop app (Tauri 2) as run by .github/workflows/desktop.yml and the Linux deps of the
# rust workflow (the workspace contains crates/cad-app, so `cargo test --workspace` links
# WebKitGTK).
#
#   scripts/ci/desktop.sh deps      # apt: WebKitGTK 4.1, libsoup 3, … (+ ninja); with `e2e-deps` also Xvfb, WebKitWebDriver
#   scripts/ci/desktop.sh e2e-deps  # apt: xvfb, webkit2gtk-driver; cargo install tauri-driver
#   scripts/ci/desktop.sh ui        # pnpm build:tauri -> web/dist-tauri (embedded into cad)
#   scripts/ci/desktop.sh build     # cargo build --release -p cad-cli (feature gui) -> target/release/cad
#   scripts/ci/desktop.sh e2e       # node web/e2e-tauri/desktop.e2e.mjs (tauri-driver, Xvfb when no display)
#
# Locally: scripts/ci/smlib-native.sh && SMLIB_PREBUILT=smlib/build-native scripts/ci/desktop.sh ui build e2e
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

SUDO=""
[ "$(id -u)" = 0 ] || SUDO="sudo"

step() {
  case "$1" in
    deps)
      $SUDO apt-get update
      $SUDO apt-get install -y --no-install-recommends ninja-build pkg-config \
        libwebkit2gtk-4.1-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev \
        libayatana-appindicator3-dev librsvg2-dev libxdo-dev
      ;;
    e2e-deps)
      $SUDO apt-get install -y --no-install-recommends xvfb webkit2gtk-driver imagemagick
      command -v tauri-driver >/dev/null || cargo install tauri-driver --version 2.1.0 --locked
      ;;
    ui) (cd web && pnpm build:tauri) ;;
    build) cargo build --release --locked -p cad-cli ;;
    e2e) node web/e2e-tauri/desktop.e2e.mjs --bin "$ROOT/target/release/cad" ;;
    *) echo "unknown step: $1 (deps|e2e-deps|ui|build|e2e)" >&2; exit 2 ;;
  esac
}

[ $# -gt 0 ] || { sed -n '2,13p' "$0"; exit 2; }
for s in "$@"; do
  echo "== desktop.sh $s"
  step "$s"
done
