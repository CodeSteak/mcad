#!/usr/bin/env bash
# Fetch SMLib + SM_API (Apache-2.0) at the pinned commit into third_party/smlib and apply the
# patches in smlib/patches/. The sources are not part of this repository.
#
# Source: tangency-labs/smlib, branch capi-tess @ 7eba8aa — NVIDIA usd-brep c9c5979 (the
# release solidcore ports) plus memory-safety fixes found with ASan/UBSan (use-after-free in
# proto-topology teardown, shell and boolean imprint: 93efd6d, af8db37, f5019a1, 1a6cc54,
# da8d05e, a05e288, 5fe723c, 594f54a, f619b0d), repeatable sampling (SM_RAND_SEED) and a
# C API (capi/, not used here). The OpenCASCADE translators of upstream are removed.
#
# Patches (smlib/patches/*.patch, applied in order with `git apply`, working tree only):
#   0001  1ed8669 of the fork's capi-solidcraft branch, source part: pointer maps iterate in
#         insertion order, not address order (shell/offset/fillet/Boolean results no longer
#         depend on the heap layout; native and Wasm take the same path).
#
#   smlib/fetch.sh            # clone https://github.com/tangency-labs/smlib
#   SMLIB_URL=/path/or/url smlib/fetch.sh
#
# Afterwards: smlib/build-native.sh (optional), cargo test -p kernel-smlib,
# smlib/build-wasm.sh. A third_party/smlib symlink is left alone; the script only checks
# that it is at the pinned commit with the patches applied.
set -euo pipefail

COMMIT=7eba8aa8086777c9cb220c0eb2726112a2a5a2b4
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEST="$HERE/../third_party/smlib"
URL="${SMLIB_URL:-https://github.com/tangency-labs/smlib.git}"
PATCHES=("$HERE"/patches/*.patch)

applied() { git -C "$DEST" apply --reverse --check "$1" >/dev/null 2>&1; }

mkdir -p "$(dirname "$DEST")"
if [ -L "$DEST" ]; then
  echo "third_party/smlib is a symlink to $(readlink "$DEST"); leaving it alone"
  have=$(git -C "$DEST" rev-parse HEAD 2>/dev/null || echo "?")
  [ "$have" = "$COMMIT" ] || echo "warning: it is at $have, not at $COMMIT" >&2
  for p in "${PATCHES[@]}"; do
    applied "$p" || echo "warning: $(basename "$p") is not applied there" >&2
  done
  exit 0
fi
if [ ! -d "$DEST/.git" ]; then
  git clone --filter=blob:none --no-checkout "$URL" "$DEST"
fi
git -C "$DEST" fetch --quiet origin || true
git -C "$DEST" checkout --quiet --force --detach "$COMMIT"
for p in "${PATCHES[@]}"; do
  if applied "$p"; then
    echo "$(basename "$p"): already applied"
  else
    git -C "$DEST" apply "$p"
    echo "$(basename "$p"): applied"
  fi
done
echo "smlib at $(git -C "$DEST" rev-parse --short HEAD) + ${#PATCHES[@]} patch(es) in $DEST"
test -f "$DEST/source/SMLib/inc/SmSmlibAll.h"
