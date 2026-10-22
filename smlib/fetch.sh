#!/usr/bin/env bash
# Fetch NVIDIA usd-brep (SMLib + SM_API, Apache-2.0) at the pinned commit into
# third_party/usd-brep. The sources are not part of this repository.
#
#   smlib/fetch.sh            # clone https://github.com/NVIDIA-Omniverse/usd-brep
#   USD_BREP_URL=/path/or/url smlib/fetch.sh
#
# Afterwards: smlib/build-native.sh (optional), cargo test -p kernel-smlib,
# smlib/build-wasm.sh.
set -euo pipefail

COMMIT=c9c5979
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEST="$HERE/../third_party/usd-brep"
URL="${USD_BREP_URL:-https://github.com/NVIDIA-Omniverse/usd-brep.git}"

mkdir -p "$(dirname "$DEST")"
if [ -L "$DEST" ]; then
  echo "third_party/usd-brep is a symlink to $(readlink "$DEST"); leaving it alone"
  exit 0
fi
if [ ! -d "$DEST/.git" ]; then
  git clone --filter=blob:none --no-checkout "$URL" "$DEST"
fi
git -C "$DEST" fetch --quiet origin || true
git -C "$DEST" checkout --quiet --detach "$COMMIT"
echo "usd-brep at $(git -C "$DEST" rev-parse --short HEAD) in $DEST"
test -f "$DEST/source/SMLib/inc/SmSmlibAll.h"
