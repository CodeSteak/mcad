#!/usr/bin/env bash
# Pinned Emscripten SDK (the version of the test reports: emcc 6.0.13).
#
#   EMSDK=~/emsdk scripts/ci/emsdk.sh          # default EMSDK: $HOME/emsdk
#   EMSDK_VERSION=6.0.13                        # default
#
# If $EMSDK already holds an activated SDK of this version (a local install),
# nothing is downloaded. Otherwise emsdk is cloned and the version installed and activated.
set -euo pipefail

VERSION="${EMSDK_VERSION:-6.0.13}"
DIR="${EMSDK:-$HOME/emsdk}"

installed() {
  [ -x "$DIR/upstream/emscripten/emcc" ] &&
    "$DIR/upstream/emscripten/emcc" --version 2>/dev/null | head -n 1 | grep -q " $VERSION "
}

if installed; then
  echo "emsdk $VERSION: already installed in $DIR"
else
  if [ ! -x "$DIR/emsdk" ]; then
    git clone --depth 1 https://github.com/emscripten-core/emsdk.git "$DIR"
  fi
  "$DIR/emsdk" install "$VERSION"
  "$DIR/emsdk" activate "$VERSION"
  installed || { echo "emsdk: emcc $VERSION not found after install" >&2; exit 1; }
fi

"$DIR/upstream/emscripten/emcc" --version | head -n 1