#!/usr/bin/env bash
# solidcore against the frozen reference (tests/kernel-reference.json): builds `mcad` against
# the newest solidcore `main` (instead of the commit pinned in crates/kernel-solidcore/Cargo.toml)
# and runs `mcad diff-reference` over the reference's 101 files with `--mesh`. Reports the
# numbers only: unsupported, blocked or differing nodes do not fail the run (the pin is guarded
# by `solidcore_against_the_reference` in crates/cad-cli/tests/cli.rs); a build failure or a
# crash of diff-reference does.
#
#   scripts/ci/solidcore.sh [out-dir]   # default target/solidcore-coverage:
#                                       # diff-reference.json, summary.md
#   SOLIDCORE_REV=<commit>|main         # default main
#
# Changes the pin in crates/kernel-solidcore (Cargo.toml, src/lib.rs: SOLIDCORE_REV, KERNEL_ID)
# and Cargo.lock; locally restore them with `git checkout -- crates/kernel-solidcore Cargo.lock`
# (pin routine: docs/solidcore-migration.md).
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
sed -i "s/rev = \"$PINNED\"/rev = \"$REV\"/g" "$MANIFEST"
sed -i "s/SOLIDCORE_REV: &str = \"$PINNED\"/SOLIDCORE_REV: \&str = \"$REV\"/" "$LIB"
sed -i "s/KERNEL_ID: &str = \"solidcore-${PINNED:0:7}\"/KERNEL_ID: \&str = \"solidcore-${REV:0:7}\"/" "$LIB"
# (solidcore-record and solidcore-convert come from the same repository and commit)

# headless CLI: no Tauri/WebKitGTK needed; Cargo.lock follows the changed pin
cargo build -p cad-cli --no-default-features

mkdir -p "$OUT"
code=0
MCAD_NO_DAEMON=1 "${CARGO_TARGET_DIR:-target}/debug/mcad" diff-reference tests/kernel-reference.json --mesh --json >"$OUT/diff-reference.json" || code=$?
# 0: every node the same, 1: not (expected: the known exceptions), anything else: usage error or crash
if [ "$code" -gt 1 ]; then
  echo "mcad diff-reference ended with exit code $code" >&2
  exit 1
fi

python3 -I - "$OUT/diff-reference.json" "$REV" "$PINNED" >"$OUT/summary.md" <<'PY'
import json
import sys

path, rev, pinned = sys.argv[1:4]
v = json.load(open(path))
s = v["summary"]
keys = ["same", "unsupported", "blocked", "differ", "failed"]
print(f"## solidcore coverage @ `{rev[:7]}` (pinned: `{pinned[:7]}`)\n")
print(f"`mcad diff-reference` over {len(v['files'])} examples, {s['nodes']} nodes.\n")
print("| " + " | ".join(keys) + " |")
print("|" + " ---: |" * len(keys))
print("| " + " | ".join(str(s[k]) for k in keys) + " |\n")
m = s.get("mesh")
if m:
    print(f"Meshes (`--mesh`, tol 0.05) of {m['files']} results: verdicts {m['verdicts']}; watertight "
          f"{m['watertight']}; chord error max {m['chord_max']:.4f} mm; triangles {m['triangles']}; "
          f"time {m['ms']:.0f} ms (B-rep {m['brep_ms']:.0f} ms of it)\n")
print("<details><summary>per operation</summary>\n")
print("| operation | nodes | " + " | ".join(keys) + " |")
print("| --- | ---: |" + " ---: |" * len(keys))
for label, c in sorted(v["coverage"].items()):
    print(f"| `{label}` | {c.get('nodes', 0)} | " + " | ".join(str(c.get(k, 0)) for k in keys) + " |")
print("\n</details>")
PY
cat "$OUT/summary.md"