#!/usr/bin/env python3
"""Timings of test report 10 (CLI daemon): for three models and the commands `check`,
`render` and `export -o x.step` each

  cold      without daemon (`--no-daemon`, a fresh process as before)
  start     first call with daemon (no daemon running: includes starting it)
  warm      second call (same file)
  changed   second call after a small change of the file (one parameter)

Every measurement starts from a stopped daemon and its own socket directory, is repeated
(--repeat, default 5) and reported as median, min and max (wall time of the `cad` process).
Writes reports/tmp/report10/timings.json.

  python3 tools/report/report10-measure.py [--cad target/release/mcad] [--repeat 5]
"""

import argparse
import json
import os
import shutil
import statistics
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

# model, directory to copy, (old, new) for the small change
MODELS = [
    ("spec/variabler_radius.scad", None, ("translate([5, 8, 4]) cube([30, 14, 10]);", "translate([5, 8, 4]) cube([30, 15, 10]);")),
    ("projekt-gehaeuse/gehaeuse.scad", "projekt-gehaeuse", ("translate([0, tiefe + abstand, 0])", "translate([0, tiefe + abstand + 1, 0])")),
    ("spec/lochplatte.scad", None, ("cylinder(h = 6, d = 3.5)", "cylinder(h = 6, d = 3.6)")),
]

COMMANDS = {
    "check": lambda f, out: ["check", f],
    "render": lambda f, out: ["render", f, "-o", str(out / "x.png")],
    "export_step": lambda f, out: ["export", f, "-o", str(out / "x.step")],
}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--cad", default=str(ROOT / "target/release/mcad"))
    ap.add_argument("--repeat", type=int, default=5)
    a = ap.parse_args()
    cad = a.cad
    tmp = Path(tempfile.mkdtemp(prefix="report10-"))
    ipc = tmp / "ipc"
    env = dict(os.environ, CAD_IPC_DIR=str(ipc), CAD_CONFIG=str(tmp / "keine.toml"))
    env.pop("CAD_NO_DAEMON", None)

    def run(args, check=True):
        t0 = time.perf_counter()
        p = subprocess.run([cad, *args], env=env, capture_output=True, text=True)
        dt = (time.perf_counter() - t0) * 1000
        if check and p.returncode not in (0, 1):
            sys.exit(f"cad {' '.join(args)}: exit {p.returncode}\n{p.stderr}")
        return dt, p

    def stop():
        run(["daemon", "stop"], check=False)

    def stat(xs):
        return {"median": statistics.median(xs), "min": min(xs), "max": max(xs), "n": len(xs)}

    results = []
    try:
        for rel, copy_dir, (old, new) in MODELS:
            work = tmp / "m"
            if work.exists():
                shutil.rmtree(work)
            if copy_dir:
                shutil.copytree(ROOT / "examples" / copy_dir, work)
                f = work / Path(rel).name
            else:
                work.mkdir()
                f = work / Path(rel).name
                shutil.copy(ROOT / "examples" / rel, f)
            original = f.read_text()
            assert old in original, (rel, old)
            out = tmp / "out"
            out.mkdir(exist_ok=True)
            for name, mk in COMMANDS.items():
                args = mk(str(f), out)
                series = {"cold": [], "start": [], "warm": [], "changed": []}
                info = None
                for _ in range(a.repeat):
                    stop()
                    f.write_text(original)
                    series["cold"].append(run(["--no-daemon", *args])[0])
                    series["start"].append(run(args)[0])
                    series["warm"].append(run(args)[0])
                    f.write_text(original.replace(old, new))
                    dt, p = run([*args, "--verbose"])
                    series["changed"].append(dt)
                    info = p.stderr.strip()
                    f.write_text(original)
                st = json.loads(run(["daemon", "status", "--json"])[1].stdout)
                stop()
                row = {"model": rel, "command": name, "args": " ".join(x if not x.startswith(str(tmp)) else Path(x).name for x in args),
                       **{k: stat(v) for k, v in series.items()}, "verbose_changed": info,
                       "daemon_rss_bytes": st.get("rss_bytes"), "cache_bytes": st.get("cache", {}).get("bytes"),
                       "cache_entries": st.get("cache", {}).get("entries"), "hit_rate": st.get("cache", {}).get("hit_rate")}
                results.append(row)
                print(f"{rel:34} {name:12} cold {row['cold']['median']:7.1f}  start {row['start']['median']:7.1f}  "
                      f"warm {row['warm']['median']:7.1f}  changed {row['changed']['median']:7.1f} ms", flush=True)
        # client round trip with a trivial model
        tiny = tmp / "tiny.scad"
        tiny.write_text("cube(1);\n")
        stop()
        run(["check", str(tiny)])
        rt = [run(["check", str(tiny)])[0] for _ in range(20)]
        local = [run(["--no-daemon", "check", str(tiny)])[0] for _ in range(20)]
        stop()
        extra = {"tiny_check_daemon": stat(rt), "tiny_check_local": stat(local)}
        print(f"cube(1) check: daemon {extra['tiny_check_daemon']['median']:.1f} ms, local {extra['tiny_check_local']['median']:.1f} ms")
        # a short session as shown in the report: verbose output, status, log
        demo = []
        ex = lambda r: str(ROOT / "examples" / r)
        for args in (["check", ex("spec/variabler_radius.scad"), "--verbose"],
                     ["render", ex("spec/variabler_radius.scad"), "-o", str(tmp / "v.png"), "--verbose"],
                     ["export", ex("spec/lochplatte.scad"), "-o", str(tmp / "l.step"), "--verbose"],
                     ["check", ex("spec/lochplatte.scad"), "--json"],
                     ["daemon", "status"]):
            dt, p = run(args)
            shown = " ".join(x.replace(str(ROOT) + "/", "").replace(str(tmp) + "/", "") for x in args)
            out = p.stdout.replace(str(ROOT) + "/", "").replace(str(tmp), "$TMP")
            if "--json" in args:
                j = json.loads(p.stdout)
                out = json.dumps({"command": j["command"], "ok": j["ok"], "daemon": j["daemon"]}, ensure_ascii=False) + "\n"
            demo.append({"cmd": "cad " + shown, "ms": dt, "stdout": out, "stderr": p.stderr.replace(str(tmp), "$TMP")})
        log = (ipc / "daemon.log").read_text().replace(str(ROOT) + "/", "").replace(str(tmp), "$TMP")
        stop()
        extra["demo"] = demo
        extra["log"] = log
    finally:
        stop()
        shutil.rmtree(tmp, ignore_errors=True)
    dst = ROOT / "reports/tmp/report10"
    dst.mkdir(parents=True, exist_ok=True)
    meta = {"cad": cad, "repeat": a.repeat, "cpus": os.cpu_count(), "date": time.strftime("%Y-%m-%d %H:%M")}
    (dst / "timings.json").write_text(json.dumps({"meta": meta, "rows": results, **extra}, indent=1))
    print(f"-> {dst / 'timings.json'}")


if __name__ == "__main__":
    main()
