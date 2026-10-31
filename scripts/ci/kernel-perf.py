#!/usr/bin/env python3
"""Eval time and peak memory per model file, SMLib against solidcore (plan 0.5).

Runs `mcad eval --json --no-daemon --kernel K FILE` once per kernel and file (a fresh process,
so the peak RSS is that of one evaluation) and prints a Markdown report: totals, ratio
solidcore / SMLib, the slowest files. Wall time includes process start and parsing; `kernel_ms`
is the engine's time in kernel calls (`scene.stats.kernel_ms`).

    scripts/ci/kernel-perf.py MCAD [--json OUT.json] [FILES…]

MCAD: a build with `--features solidcore` (release for meaningful times). Without FILES: the
corpus of scripts/ci/solidcore.sh. Files where solidcore fails or stops early (unsupported
operations) still count; their time is what solidcore spent until then.
"""

import json
import os
import subprocess
import sys
import time

ROOT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))


def corpus():
    out = []
    for d, ext in [("tests/models/openscad-compat", ".scad"), ("tests/models/spec", ".mcad"),
                   ("tests/models/everyday", ".mcad")]:
        out += sorted(os.path.join(d, f) for f in os.listdir(os.path.join(ROOT, d)) if f.endswith(ext))
    every = "tests/models/everyday"
    for n in sorted(os.listdir(os.path.join(ROOT, every))):
        main = os.path.join(every, n, n + ".mcad")
        if os.path.isfile(os.path.join(ROOT, main)):
            out.append(main)
    out.append("tests/models/project-housing/housing.mcad")
    bugs = "tests/everyday/bugs"
    out += sorted(os.path.join(bugs, f) for f in os.listdir(os.path.join(ROOT, bugs)) if f.endswith(".mcad"))
    return out


def run(mcad, kernel, f):
    env = dict(os.environ, MCAD_NO_DAEMON="1", MCAD_LANG="en")
    t0 = time.monotonic()
    p = subprocess.Popen([mcad, "eval", "--json", "--no-daemon", "--kernel", kernel, f], cwd=ROOT, env=env,
                         stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    out = p.stdout.read()
    _, status, ru = os.wait4(p.pid, 0)
    p.returncode = os.waitstatus_to_exitcode(status)
    wall = (time.monotonic() - t0) * 1e3
    try:
        stats = json.loads(out).get("scene", {}).get("stats", {})
    except ValueError:
        stats = {}
    return {"wall_ms": wall, "kernel_ms": stats.get("kernel_ms"), "rss_mb": ru.ru_maxrss / 1024,
            "exit": p.returncode}


def main():
    args = sys.argv[1:]
    if not args:
        print(__doc__, file=sys.stderr)
        sys.exit(2)
    mcad, args = os.path.abspath(args[0]), args[1:]
    out_json = None
    if args[:1] == ["--json"]:
        out_json, args = args[1], args[2:]
    files = args or corpus()
    rows = []
    for f in files:
        a, b = run(mcad, "smlib", f), run(mcad, "solidcore", f)
        rows.append({"file": f, "smlib": a, "solidcore": b})
        print(f"{f}: SMLib {a['wall_ms']:.0f} ms {a['rss_mb']:.0f} MB, "
              f"solidcore {b['wall_ms']:.0f} ms {b['rss_mb']:.0f} MB", file=sys.stderr)
    if out_json:
        json.dump(rows, open(out_json, "w"), indent=1)

    def tot(k, key):
        return sum((r[k][key] or 0) for r in rows)

    def ratio(x, y):
        return f"{y / x:.2f}" if x else "–"

    print(f"`mcad eval` per file, {len(rows)} files, one process per file and kernel.\n")
    print("| | SMLib | solidcore | solidcore / SMLib |")
    print("| --- | ---: | ---: | ---: |")
    for key, label in [("wall_ms", "wall time, sum (ms)"), ("kernel_ms", "kernel calls, sum (ms)")]:
        print(f"| {label} | {tot('smlib', key):.0f} | {tot('solidcore', key):.0f} | "
              f"{ratio(tot('smlib', key), tot('solidcore', key))} |")
    pa = max(r["smlib"]["rss_mb"] for r in rows)
    pb = max(r["solidcore"]["rss_mb"] for r in rows)
    print(f"| peak RSS, largest file (MB) | {pa:.0f} | {pb:.0f} | {ratio(pa, pb)} |")
    ma = sorted(r["smlib"]["rss_mb"] for r in rows)[len(rows) // 2]
    mb = sorted(r["solidcore"]["rss_mb"] for r in rows)[len(rows) // 2]
    print(f"| peak RSS, median file (MB) | {ma:.0f} | {mb:.0f} | {ratio(ma, mb)} |")
    print("\nSlowest files (solidcore wall time):\n")
    print("| file | SMLib ms | solidcore ms | ratio | SMLib MB | solidcore MB |")
    print("| --- | ---: | ---: | ---: | ---: | ---: |")
    for r in sorted(rows, key=lambda r: -r["solidcore"]["wall_ms"])[:8]:
        a, b = r["smlib"], r["solidcore"]
        print(f"| `{r['file']}` | {a['wall_ms']:.0f} | {b['wall_ms']:.0f} | {ratio(a['wall_ms'], b['wall_ms'])} | "
              f"{a['rss_mb']:.0f} | {b['rss_mb']:.0f} |")


if __name__ == "__main__":
    main()
