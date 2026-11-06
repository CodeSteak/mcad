#!/usr/bin/env python3
"""Timings for test report 23: the last SMLib build against solidcore with --jobs 1/2/4.

  python3 tools/report/report23-data.py --smlib OLD_MCAD [--sc0 OLD_MCAD_WITH_SOLIDCORE] \
      [--rounds 5] [--corpus-rounds 2] [--stl]

OLD_MCAD: release build of f4f8586 (`smlib/fetch.sh`, `scripts/ci/smlib-native.sh`,
`cargo build --release -p cad-cli --no-default-features`); OLD_MCAD_WITH_SOLIDCORE: the same with
`--features solidcore` (run with `--kernel solidcore`). New binary: target/release/mcad.
Every run is a fresh process without daemon (`MCAD_NO_DAEMON=1`), `mcad eval FILE --json`;
variants alternate per round and file so that drift of the machine hits all alike.
Writes reports/tmp/report23/{cmp.jsonl, cmp2.jsonl, corpus.jsonl}. Run on an idle machine.
"""
import argparse
import json
import os
import re
import subprocess
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "reports/tmp/report23"
NEW = str(ROOT / "target/release/mcad")
MODELS = [f"examples/0{i}-{n}.mcad" for i, n in enumerate(
    ["plate", "bracket", "tray", "hook", "box-lid", "name-tag", "mug", "macropad", "flashlight"], 1)] + [
    "tests/models/spec/text_sign.mcad", "tests/models/everyday/key_rack/key_rack.mcad",
    "tests/models/everyday/trivet.mcad", "tests/models/everyday/mug.mcad",
    "tests/models/everyday/gear.mcad", "tests/models/everyday/housing/housing.mcad",
    "tests/models/project-housing/housing.mcad"]
ENV = dict(os.environ, MCAD_NO_DAEMON="1")


def run(argv, timeout=900):
    t0 = time.perf_counter()
    r = subprocess.run(argv, env=ENV, capture_output=True, timeout=timeout)
    t = time.perf_counter() - t0
    try:
        st = json.loads(r.stdout)["scene"]["stats"]
    except Exception:
        st = None
    return t, r.returncode, st


def rounds(files, variants, n, out, prefix=""):
    with open(out, "a") as o:
        for rnd in range(n):
            for i, f in enumerate(files):
                k = (rnd + i) % len(variants)
                for name, base in variants[k:] + variants[:k]:
                    t, rc, st = run([base[0], "eval", str(ROOT / prefix / f), "--json"] + base[1:])
                    o.write(json.dumps({"round": rnd, "file": f, "v": name, "wall": t, "engine": st and st["kernel_ms"] / 1000,
                                        "nodes": st and st["nodes"], "rc": rc}) + "\n")
                    o.flush()
            print("round", rnd, "done", flush=True)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--smlib", required=True)
    ap.add_argument("--sc0")
    ap.add_argument("--rounds", type=int, default=5)
    ap.add_argument("--corpus-rounds", type=int, default=2)
    ap.add_argument("--stl", action="store_true", help="also one STL export per model (cmp.jsonl)")
    a = ap.parse_args()
    OUT.mkdir(parents=True, exist_ok=True)
    v4 = [("smlib", [a.smlib]), ("j1", [NEW, "--jobs", "1"]), ("j2", [NEW, "--jobs", "2"]), ("j4", [NEW, "--jobs", "4"])]
    for fn in ("cmp.jsonl", "cmp2.jsonl", "corpus.jsonl"):
        (OUT / fn).unlink(missing_ok=True)
    if a.stl:
        with open(OUT / "cmp.jsonl", "w") as o:
            for f in MODELS:
                row = {"file": f}
                for name, base in [v4[0], v4[1], v4[3]]:
                    t, rc, st = run([base[0], "export", str(ROOT / f), "-o", str(OUT / "x.stl")] + base[1:])
                    row["stl_" + name], row["stl_" + name + "_rc"] = t, rc
                    row["nodes"] = row.get("nodes") or (st and st.get("nodes"))
                o.write(json.dumps(row) + "\n")
    rounds(MODELS, v4, a.rounds, OUT / "cmp2.jsonl")
    ref = (ROOT / "tests/kernel-reference.json").read_text()
    corpus = re.findall(r'\{"file": "([^"]+)"', ref)
    vc = [("smlib", [a.smlib])] + ([("sc0", [a.sc0, "--kernel", "solidcore"])] if a.sc0 else []) + [v4[1], v4[3]]
    rounds(corpus, vc, a.corpus_rounds, OUT / "corpus.jsonl", "tests")
    print("done:", OUT)


if __name__ == "__main__":
    main()
