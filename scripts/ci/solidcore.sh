#!/usr/bin/env bash
# solidcore coverage as run weekly by .github/workflows/solidcore.yml: builds `cad` with
# `--features solidcore` against the newest solidcore `main` (instead of the commit pinned in
# crates/kernel-solidcore/Cargo.toml) and runs `cad diff-kernels` over the examples
# (examples/openscad-compat, examples/spec). Reports the numbers only: unsupported, blocked
# or differing nodes do not fail the run (the pin is guarded by `diff_kernels_on_examples`);
# a build failure or a crash of diff-kernels does.
#
#   scripts/ci/solidcore.sh [out-dir]   # default target/solidcore-coverage:
#                                       # diff-kernels.json, summary.md
#   SOLIDCORE_REV=<commit>|main         # default main
#
# Writes summary.md to $GITHUB_STEP_SUMMARY when set. Changes the pin in
# crates/kernel-solidcore (Cargo.toml, src/lib.rs) and Cargo.lock; locally restore them with
# `git checkout -- crates/kernel-solidcore Cargo.lock`.
# Locally: scripts/ci/smlib-native.sh && SMLIB_PREBUILT=smlib/build-native scripts/ci/solidcore.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

OUT="${1:-target/solidcore-coverage}"
REPO="https://github.com/tangency-labs/solidcore"
WANT="${SOLIDCORE_REV:-main}"
MANIFEST="crates/kernel-solidcore/Cargo.toml"
LIB="crates/kernel-solidcore/src/lib.rs"

if [ "$WANT" = main ]; then
  REV="$(git ls-remote "$REPO" refs/heads/main | cut -f1)"
elif [[ "$WANT" =~ ^[0-9a-f]{40}$ ]]; then
  REV="$WANT"
else
  echo "SOLIDCORE_REV: full commit hash or main, not '$WANT'" >&2
  exit 2
fi
PINNED="$(sed -n 's/^solidcore = { git = "[^"]*", rev = "\([0-9a-f]*\)".*/\1/p' "$MANIFEST")"
if [ -z "$REV" ] || [ -z "$PINNED" ]; then
  echo "solidcore commit not found (main: '$REV', pin: '$PINNED')" >&2
  exit 1
fi
echo "solidcore: pinned ${PINNED:0:7}, measuring ${REV:0:7} ($WANT)"
sed -i "s/rev = \"$PINNED\"/rev = \"$REV\"/" "$MANIFEST"
sed -i "s/SOLIDCORE_REV: &str = \"$PINNED\"/SOLIDCORE_REV: \&str = \"$REV\"/" "$LIB"

# headless CLI: no Tauri/WebKitGTK needed; Cargo.lock follows the changed pin
cargo build -p cad-cli --no-default-features --features solidcore

mkdir -p "$OUT"
files=(examples/openscad-compat/*.scad examples/spec/*.scad)
code=0
CAD_NO_DAEMON=1 "${CARGO_TARGET_DIR:-target}/debug/cad" diff-kernels "${files[@]}" --json >"$OUT/diff-kernels.json" || code=$?
# 0: every node the same, 1: not (expected for now), anything else: usage error or crash
if [ "$code" -gt 1 ]; then
  echo "cad diff-kernels ended with exit code $code" >&2
  exit 1
fi

python3 -I - "$OUT/diff-kernels.json" "$REV" "$PINNED" >"$OUT/summary.md" <<'PY'
import json
import sys

path, rev, pinned = sys.argv[1:4]
v = json.load(open(path))
s = v["summary"]
keys = ["same", "unsupported", "blocked", "differ", "failed"]
print(f"## solidcore coverage @ `{rev[:7]}` (pinned: `{pinned[:7]}`)\n")
print(f"`cad diff-kernels` over {len(v['files'])} examples, {s['nodes']} nodes.\n")
print("| " + " | ".join(keys) + " |")
print("|" + " ---: |" * len(keys))
print("| " + " | ".join(str(s[k]) for k in keys) + " |\n")
print("<details><summary>per operation</summary>\n")
print("| operation | nodes | " + " | ".join(keys) + " |")
print("| --- | ---: |" + " ---: |" * len(keys))
for label, c in sorted(v["coverage"].items()):
    print(f"| `{label}` | {c.get('nodes', 0)} | " + " | ".join(str(c.get(k, 0)) for k in keys) + " |")
print("\n</details>")
PY
cat "$OUT/summary.md"
if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
  cat "$OUT/summary.md" >>"$GITHUB_STEP_SUMMARY"
fi
