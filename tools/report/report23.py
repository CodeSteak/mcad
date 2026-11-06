#!/usr/bin/env python3
"""Test report 23: pure Rust (SMLib removed, solidcore the only kernel) and parallel evaluation of
CSG branches (`--jobs N`). Writes docs/reports/23-pure-rust-threads/report.html (images in img/).

Inputs (gitignored, reports/tmp/report23/):
  before/NAME.png, before/times.txt   renders with the last SMLib build (debug, studio, 900x675)
  after/NAME.png, after/times.txt     the same with this branch
  after4/NAME.png                     the same with --jobs 4 (release)
  cmp.jsonl, cmp2.jsonl, corpus.jsonl  python3 tools/report/report23-data.py --smlib OLD --sc0 OLD_SC --stl
                                      (STL export per model; interleaved eval rounds of 16 models; corpus)
  tests/tests.raw, tests/*.log        final test runs: name|exit code|seconds|command per line
  ui/*.png                            optional UI screenshots

  python3 tools/report/report23.py
  node tools/report/print-pdf.mjs docs/reports/23-pure-rust-threads/report.html \
      docs/reports/23-pure-rust-threads/report.pdf "Test report 23: pure Rust, parallel CSG"
"""
import html
import json
import re
import os
import shutil
import statistics
import subprocess
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
INP = ROOT / "reports/tmp/report23"
OUT = ROOT / "docs/reports/23-pure-rust-threads"
IMG = OUT / "img"
shutil.rmtree(IMG, ignore_errors=True)
IMG.mkdir(parents=True, exist_ok=True)

esc = html.escape


def md(t):
    """`code` in a plain string."""
    parts = esc(t).split("`")
    return "".join(f"<code>{p}</code>" if i % 2 else p for i, p in enumerate(parts))


def git(*a):
    try:
        return subprocess.check_output(["git", *a], cwd=ROOT, text=True).strip()
    except Exception:
        return "–"


def jsonl(name):
    p = INP / name
    return [json.loads(l) for l in p.read_text().splitlines() if l.strip()] if p.exists() else []


def jpg(src, dst, width=900):
    if not src.exists():
        return None
    try:
        subprocess.check_call(["convert", str(src), "-resize", f"{width}x>", "-quality", "82", str(IMG / dst)])
        return f"img/{dst}"
    except Exception:
        d = dst.replace(".jpg", ".png")
        shutil.copy(src, IMG / d)
        return f"img/{d}"


def fig(src, cap):
    return f'<figure><img src="{src}" alt=""><figcaption>{md(cap)}</figcaption></figure>' if src else f'<p class="small">(missing: {esc(cap)})</p>'


def f(x, d=2):
    return "–" if x is None else f"{x:,.{d}f}"


def short(p):
    return p.replace("tests/models/", "").replace("examples/", "").replace(".mcad", "")


# ---------------------------------------------------------------- data
commit = git("rev-parse", "--short", "HEAD")
branch = git("rev-parse", "--abbrev-ref", "HEAD")
date = git("log", "-1", "--format=%cs")
commits = [l.split(" ", 1) for l in git("log", "--format=%h %s", "f4f8586..HEAD").splitlines()]
rustc = git("-c", "core.quotepath=off", "--version")
try:
    rustc = subprocess.check_output(["rustc", "--version"], text=True).strip()
except Exception:
    pass

names = ["01-plate", "02-bracket", "03-tray", "04-hook", "05-box-lid", "06-name-tag", "07-mug", "08-macropad", "09-flashlight"]


def times(p):
    t = {}
    if p.exists():
        for l in p.read_text().splitlines():
            w = l.split()
            if len(w) >= 3:
                t[w[0]] = float(w[-1])
    return t


tb, ta = times(INP / "before/times.txt"), times(INP / "after/times.txt")


def pixdiff(a, b):
    try:
        from PIL import Image, ImageChops

        x, y = Image.open(a).convert("RGB"), Image.open(b).convert("RGB")
        d = ImageChops.difference(x, y).convert("L")
        hist = d.histogram()
        return sum(hist[41:]) / (x.size[0] * x.size[1])
    except Exception:
        return None


def pixident(a, b):
    try:
        from PIL import Image, ImageChops

        return ImageChops.difference(Image.open(a).convert("RGB"), Image.open(b).convert("RGB")).getbbox() is None
    except Exception:
        return None


# eval rounds: per (file, variant) median of wall and engine
def medians(rows):
    acc = defaultdict(lambda: defaultdict(list))
    for r in rows:
        if r.get("engine") is not None:
            acc[r["file"]][r["v"]].append((r["wall"], r["engine"]))
    res = {}
    for fl, vs in acc.items():
        res[fl] = {v: (statistics.median(w for w, _ in xs), statistics.median(e for _, e in xs), len(xs), min(w for w, _ in xs)) for v, xs in vs.items()}
    return res


R = medians(jsonl("cmp2.jsonl"))
C = medians(jsonl("corpus.jsonl"))
S1 = {r["file"]: r for r in jsonl("cmp.jsonl")}
order = [r["file"] for r in jsonl("cmp.jsonl")] or sorted(R)
rounds = max([r["round"] for r in jsonl("cmp2.jsonl")] or [-1]) + 1
crounds = max([r["round"] for r in jsonl("corpus.jsonl")] or [-1]) + 1
V = ["smlib", "j1", "j2", "j4"]
VL = {"smlib": "SMLib (C++, 1 thread)", "j1": "solidcore --jobs 1", "j2": "--jobs 2", "j4": "--jobs 4", "sc0": "solidcore at f4f8586 (before this branch)"}


def total(res, v, idx=1, files=None):
    xs = [res[fl][v][idx] for fl in (files or res) if v in res[fl]]
    return sum(xs) if xs else None


heavy = [fl for fl in order if fl in R and R[fl].get("smlib", (0, 0))[0] >= 1.0]
light = [fl for fl in order if fl in R and fl not in heavy]

def read_tests():
    """tests/tests.raw (name|rc|seconds|command, from the test script) and the logs next to it."""
    raw = INP / "tests/tests.raw"
    out = []
    if not raw.exists():
        return out
    for line in raw.read_text().splitlines():
        w = line.split("|", 3)
        if len(w) < 4:
            continue
        name, rc, secs, cmd = w[0], int(w[1]), int(w[2]), w[3]
        log = (INP / f"tests/{name}.log").read_text(errors="replace") if (INP / f"tests/{name}.log").exists() else ""
        passed = sum(int(m) for m in re.findall(r"test result: \w+\. (\d+) passed", log))
        failed = sum(int(m) for m in re.findall(r"(\d+) failed", " ".join(re.findall(r"test result: .*", log))))
        if cmd.startswith("cargo test"):
            res = f"{passed} passed, {failed} failed"
        elif name == "diffref":
            tail = [l for l in log.splitlines() if l.startswith(("Total:", "Meshes"))]
            res = " / ".join(tail) + " (exit 1 = differences present: the 8 known ones)"
            rc = 0 if "same 2596, unsupported 3, blocked 3, differ 2, failed 0" in log else rc
        elif name == "tree":
            users = sorted(set(re.findall(r"^\S*── ([\w-]+) v", log, re.M)))
            res = f"`cc` only as build dependency of {', '.join(users)} (blake3 with `pure`: compiles no C); crates named cmake/smlib: {(log.strip().splitlines() or ['?'])[-1]}"
        else:
            res = "clean" if rc == 0 else (log.strip().splitlines() or ["?"])[-1]
        if name == "tree":
            cmd = "cargo tree -p cad-cli --no-default-features -e build,normal -i cc"
        out.append({"cmd": cmd, "ok": rc == 0, "result": res, "time": f"{secs // 60} min {secs % 60} s" if secs >= 60 else f"{secs} s"})
    return out


tests = read_tests()

# ---------------------------------------------------------------- chart (ratio to SMLib)
def ratio_chart(files):
    rows = [(fl, R[fl]["j1"][1] / R[fl]["smlib"][1], R[fl]["j4"][1] / R[fl]["smlib"][1]) for fl in files if all(v in R[fl] for v in ("smlib", "j1", "j4")) and R[fl]["smlib"][1] > 0]
    if not rows:
        return ""
    W, lab, right = 640, 150, 50
    bh, gap, top = 9, 2, 40
    rowh = 2 * bh + gap + 9
    H = top + rowh * len(rows) + 22
    xmax = max(2.0, max(max(a, b) for _, a, b in rows) * 1.08)
    sx = lambda v: lab + (W - lab - right) * v / xmax
    s = [f'<svg viewBox="0 0 {W} {H}" width="100%" role="img" aria-label="engine time relative to SMLib">']
    s.append('<style>.t{font:9px system-ui,sans-serif;fill:#52514e}.v{font:8.5px system-ui,sans-serif;fill:#0b0b0b}.g{stroke:#e4e4e1;stroke-width:1}</style>')
    tick = 0.5
    v = 0.0
    while v <= xmax + 1e-9:
        x = sx(v)
        s.append(f'<line class="g" x1="{x:.1f}" y1="{top - 6}" x2="{x:.1f}" y2="{H - 18}"/><text class="t" x="{x:.1f}" y="{H - 6}" text-anchor="middle">{v:g}×</text>')
        v += tick
    x1 = sx(1.0)
    s.append(f'<line x1="{x1:.1f}" y1="{top - 10}" x2="{x1:.1f}" y2="{H - 18}" stroke="#0b0b0b" stroke-width="1.2" stroke-dasharray="3 2"/>')
    s.append(f'<text class="t" x="{x1 + 3:.1f}" y="{top - 12}">SMLib = 1 (shorter bar = faster)</text>')
    # legend
    s.append(f'<rect x="{lab}" y="2" width="10" height="8" rx="2" fill="#2a78d6"/><text class="t" x="{lab + 14}" y="9">solidcore --jobs 1</text>')
    s.append(f'<rect x="{lab + 110}" y="2" width="10" height="8" rx="2" fill="#eb6834"/><text class="t" x="{lab + 124}" y="9">solidcore --jobs 4</text>')
    for i, (fl, a, b) in enumerate(rows):
        y = top + i * rowh
        s.append(f'<text class="t" x="{lab - 6}" y="{y + bh + 3}" text-anchor="end">{esc(short(fl))}</text>')
        for j, (val, col) in enumerate(((a, "#2a78d6"), (b, "#eb6834"))):
            yy = y + j * (bh + gap)
            s.append(f'<rect x="{lab}" y="{yy}" width="{max(sx(val) - lab, 1):.1f}" height="{bh}" rx="2" fill="{col}"><title>{esc(short(fl))}: {val:.2f}× SMLib</title></rect>')
            s.append(f'<text class="v" x="{sx(val) + 3:.1f}" y="{yy + bh - 1.5}">{val:.2f}×</text>')
    s.append("</svg>")
    return "".join(s)


# ---------------------------------------------------------------- sections
pairs = []
for n in names:
    a = jpg(INP / f"before/{n}.png", f"before-{n}.jpg", 560)
    b = jpg(INP / f"after/{n}.png", f"after-{n}.jpg", 560)
    d = pixdiff(INP / f"before/{n}.png", INP / f"after/{n}.png")
    d4 = pixident(INP / f"after1/{n}.png", INP / f"after4/{n}.png")
    pairs.append(f'<div class="two">{fig(a, f"`{n}` – SMLib ({tb.get(n, 0):.1f} s debug render)")}'
                 f'{fig(b, f"`{n}` – solidcore ({ta.get(n, 0):.1f} s); {100 * d:.2f} % of the pixels differ by more than 40/255" + (f"; release `--jobs 4` vs `--jobs 1`: {'pixel-identical' if d4 else 'DIFFERENT'}" if d4 is not None else "") if d is not None else n)}</div>')


def evalrow(fl):
    r = R.get(fl, {})
    sm = r.get("smlib")
    cells = []
    for v in V:
        x = r.get(v)
        cells.append(f'<td class="n">{f(x[0])}</td>' if x else "<td></td>")
    eng = lambda v: r[v][1] if v in r else None
    rat = lambda v: (eng(v) / eng("smlib")) if sm and eng(v) and eng("smlib") > 0 else None
    s1 = S1.get(fl, {})
    stl = lambda v: s1.get("stl_" + v)
    return (f'<tr><td><code>{esc(short(fl))}</code></td><td class="n">{s1.get("nodes") or ""}</td>{"".join(cells)}'
            f'<td class="n">{f(rat("j1"))}×</td><td class="n"><b>{f(rat("j4"))}×</b></td>'
            f'<td class="n">{f(stl("smlib"))}</td><td class="n">{f(stl("j1"))}</td><td class="n">{f(stl("j4"))}</td></tr>')


def sumrow(label, files):
    t = {v: total(R, v, 0, files) for v in V}
    e = {v: total(R, v, 1, files) for v in V}
    st = {v: sum(S1[fl].get("stl_" + v, 0) for fl in files if fl in S1) for v in ("smlib", "j1", "j4")}
    return (f'<tr class="sum"><td>{esc(label)}</td><td></td>{"".join(f"<td class=n>{f(t[v])}</td>" for v in V)}'
            f'<td class="n">{f(e["j1"] / e["smlib"]) if e["smlib"] else "–"}×</td><td class="n"><b>{f(e["j4"] / e["smlib"]) if e["smlib"] else "–"}×</b></td>'
            f'<td class="n">{f(st["smlib"])}</td><td class="n">{f(st["j1"])}</td><td class="n">{f(st["j4"])}</td></tr>')


evalrows = "".join(evalrow(fl) for fl in order if fl in R)
allfiles = [fl for fl in order if fl in R]

# corpus
cv = [v for v in ("smlib", "sc0", "j1", "j2", "j4") if any(v in C[fl] for fl in C)]
ctot = {v: total(C, v, 1) for v in cv}
cwall = {v: total(C, v, 0) for v in cv}
cfiles = len(C)
cwins = sum(1 for fl in C if "smlib" in C[fl] and "j4" in C[fl] and C[fl]["j4"][1] < C[fl]["smlib"][1])
csmall = sum(1 for fl in C if "smlib" in C[fl] and C[fl]["smlib"][1] < 0.5)
ctop = sorted((fl for fl in C if "smlib" in C[fl] and "j4" in C[fl]), key=lambda fl: -C[fl]["smlib"][1])[:12]
corpus_rows = "".join(
    f'<tr><td><code>{esc(fl.replace("models/", ""))}</code></td>' + "".join(f'<td class="n">{f(C[fl][v][1]) if v in C[fl] else "–"}</td>' for v in cv)
    + f'<td class="n"><b>{f(C[fl]["j4"][1] / C[fl]["smlib"][1])}×</b></td></tr>' for fl in ctop)

headline_e = (total(R, "j4", 1, allfiles) / total(R, "smlib", 1, allfiles)) if R else None
headline_c = (ctot.get("j4") / ctot.get("smlib")) if ctot.get("smlib") and ctot.get("j4") else None

testrows = "".join(f'<tr><td><code>{esc(t["cmd"])}</code></td><td><span class="{"ok" if t["ok"] else "bad"}">{"passed" if t["ok"] else "failed"}</span> – {md(t["result"])}</td><td class="n">{esc(t.get("time", ""))}</td></tr>' for t in tests)
ui = sorted((INP / "ui").glob("*.png")) if (INP / "ui").exists() else []
uit = json.loads((INP / "ui/times.json").read_text()) if (INP / "ui/times.json").exists() else {}
uifigs = ("<p>Browser build (one Wasm module with solidcore, sequential) in Chromium with software WebGL, window 1440×900, German UI as in all Playwright specs; <code>web/e2e/report23.spec.ts</code> (<code>REPORT23=1</code>). Taken while the Rust tests ran on the same 4 cores, so the times only show that the models evaluate, not the speed.</p>"
          + "".join(fig(jpg(p, f"ui-{p.stem}.jpg", 1000), f"`{p.stem.split('_', 1)[-1]}`: no errors; evaluation in the browser {uit.get(p.stem.split('_', 1)[-1], 0) / 1000:.1f} s") for p in ui))

CSS = (ROOT / "tools/report/report23.css").read_text() if (ROOT / "tools/report/report23.css").exists() else ""

page = f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Test report 23 – pure Rust and parallel CSG evaluation</title>
<style>
{CSS}
</style>
</head>
<body>
<main>
<h1>Test report 23 – pure Rust kernel and parallel CSG evaluation</h1>
<div class="meta">{esc(date)} · Branch {esc(branch)} · Commit {esc(commit)} · {esc(rustc)} · {os.cpu_count()} cores (VM). Comparison against the last SMLib state <code>f4f8586</code> (<code>main</code>), built as release in a separate worktree. Timings: release binaries, one process per run, no daemon (<code>MCAD_NO_DAEMON=1</code>), <code>mcad eval FILE --json</code>; medians of {rounds} interleaved rounds (variants alternate per round, so drift of the VM hits all of them alike).</div>

<div class="summary">
  <div>C/C++ in the build<b>none</b>SMLib, facade, Emscripten, cmake removed; blake3 pure Rust</div>
  <div>Reference (101 files, 2604 nodes)<b>2596 same</b>3 unsupported, 3 blocked (variable radius), 2 differ</div>
  <div>--jobs 4 vs --jobs 1<b>identical results</b>whole corpus, mesh hashes, diagnostics</div>
  <div>Engine time vs SMLib (16 models)<b>{f(headline_e)}× with 4 threads</b>{f(total(R, "j1", 1, allfiles) / total(R, "smlib", 1, allfiles)) if R else "–"}× with 1 thread</div>
  <div>Engine time vs SMLib (corpus)<b>{f(headline_c)}× with 4 threads</b>faster than SMLib in {cwins} of {cfiles} files</div>
</div>

<h2>1 Result</h2>
<ul>
<li><b>Pure Rust.</b> <code>kernel-smlib</code>, <code>smlib/</code>, the facade, the Emscripten build and the cmake/ninja/libstdc++ requirements are removed; solidcore is the only kernel in CLI, daemon, desktop engine and browser (one Wasm module). <code>cargo tree</code> lists <code>cc</code> only as a build dependency that compiles nothing (blake3 with <code>pure</code>) and for Tauri's Windows resources.</li>
<li><b>Same geometry.</b> SMLib's per-node results were frozen before the removal (<code>tests/kernel-reference.json</code>); solidcore matches 2596 of 2604 nodes, the 8 others are listed exactly in a test. The nine examples render with under 0.21 % of the pixels changed.</li>
<li><b>Parallel CSG.</b> <code>--jobs N</code> / <code>MCAD_JOBS</code> evaluates independent nodes on N worker threads, each with its own kernel. Results are bit-identical to the sequential run.</li>
<li><b>Faster than the old single-threaded C++?</b> {"<b>No, not in general.</b>" if headline_e and headline_e > 1.05 else "<b>About even.</b>" if headline_e and headline_e > 0.95 else "<b>Yes.</b>"} Over the 16 models solidcore with 4 threads needs {f(headline_e)}× SMLib's engine time{f", over the corpus {f(headline_c)}×" if headline_c else ""}. solidcore alone is slower than SMLib (1 thread: {f(total(R, "j1", 1, allfiles) / total(R, "smlib", 1, allfiles)) if R else "–"}×); threads win back only 10–30 %, because the time of most models sits in one chain of dependent heavy operations (section 5). Models with several independent heavy bodies (<code>housing</code>) or where solidcore is faster per operation (<code>09-flashlight</code>, <code>text_sign</code>) beat SMLib. Meshing (STL export) is faster in solidcore and narrows the gap for full exports.</li>
</ul>

<h2>2 Commits</h2>
<table><tr><th>Commit</th><th>Title</th></tr>{"".join(f"<tr><td><code>{esc(h)}</code></td><td>{esc(s)}</td></tr>" for h, s in commits)}</table>

<h2>3 Phase A: SMLib removed</h2>
<table><tr><th>Part</th><th>Change</th></tr>
<tr><td>Reference data</td><td>While SMLib still built: <code>diff-kernels --write-reference</code> froze SMLib's results of all 101 corpus files (2604 nodes; volume, faces, edges, selected edges, B-rep positions of the 260 blended nodes) into <code>tests/kernel-reference.json</code> (6.2 MB, 690 kB gzip). Checked against the live comparison before removal: every node the same verdict, rule and notes.</td></tr>
<tr><td>CLI</td><td><code>mcad diff-kernels</code> → <code>mcad diff-reference REF.json [FILE…] [--mesh]</code> (same rules). <code>--kernel</code>, <code>MCAD_KERNEL</code> and the cargo feature <code>solidcore</code> are gone.</td></tr>
<tr><td>Variable radius</td><td>Edges of one fillet with different radius laws: error “variable radius is not supported: fillet of N edges with different radius laws (A–B mm); use a constant radius” (German and English), body stays unfilleted. One law for all edges and constant radii per edge work.</td></tr>
<tr><td>Kernel fixes found by the switch</td><td>(1) Union of bodies that do not overlap could return one body a second time inside out as a void – wrong volume without any warning (stencil glyph “c”: 2.02 instead of 7.30). Now detected (more shells than both operands, two alike) and merged. (2) Failures at tangential contacts had no place (no operands in the message, no mark in <code>render</code>): geometric fallback over the contact edges.</td></tr>
<tr><td>Determinism bug (found in phase B)</td><td>STL import started each face loop at the first key of a <code>HashMap</code>: volumes of <code>spec/stl_import.mcad</code> changed in the 10th digit from run to run, also sequentially. Now the smallest vertex.</td></tr>
<tr><td>Browser</td><td>one Wasm module, profile <code>wasm</code> (opt-level 3, fat LTO): 34.3 MB raw, 10.8 MB gzip, 6.66 MB Brotli (before with SMLib: 39.3 MiB, 6.2 MiB Brotli). opt-level “z” would give 4.97 MB Brotli but is 2.4× slower (macropad 85 s vs 36 s in Node; e2e S04/S06 time out).</td></tr>
</table>
<h3>Reference comparison (<code>mcad diff-reference tests/kernel-reference.json --mesh</code>)</h3>
<table><tr><th>Nodes</th><th class="n">same</th><th class="n">unsupported</th><th class="n">blocked</th><th class="n">differ</th><th class="n">failed</th></tr>
<tr><td>2604 in 101 files</td><td class="n">2596</td><td class="n">3</td><td class="n">3</td><td class="n">2</td><td class="n">0</td></tr></table>
<p>Unsupported/blocked: the six variable-radius nodes of <code>variable_radius</code>. Differ: <code>sketch_cable_duct</code> (open point of the handover) and <code>f11</code> (SMLib fails, solidcore succeeds). Volume rules used: relative 2293, exact 129, operands 67, curved 25, blend 70. Meshes at 0.05 mm: 100 ok and watertight, 1 skipped, largest chord error 0.0500 mm.</p>

<h3>Renderings: SMLib (left) and solidcore (right)</h3>
<p><code>mcad render FILE --style studio --size 900x675</code>, debug builds (render times include the studio pass). Pixel counts: share of pixels whose grey difference exceeds 40/255.</p>
{"".join(pairs)}

<h2>4 Phase B: parallel evaluation of CSG branches</h2>
<table><tr><th>Part</th><th>Design</th></tr>
<tr><td>Thread pool</td><td><code>cad-engine/src/pool.rs</code>: N worker threads (stack 256 MB, solidcore recurses deeply), each with its own kernel built in its thread; tasks are single nodes, results come back over a channel. A worker panic is re-raised on the dispatching thread; tasks of a dropped job are skipped.</td></tr>
<tr><td>Scheduling</td><td><code>Job::step_parallel</code>: a node is ready when its children are done (a <code>zone</code> also waits for its block's body). Ready nodes go out in post-order. Cached nodes, 2D, empty and plain primitives run on the dispatching thread. A node whose cache key is already running waits and counts as a cache hit, so nodes, cache hits and kernel calls equal the sequential run.</td></tr>
<tr><td>Determinism</td><td>Every node, sequential or parallel, starts with <code>Kernel::begin_node(seed)</code>, the seed derived from its cache key: solidcore gets a fresh <code>solidcore::Kernel</code> (rng, fillet step, size estimate) and its copy key domains from the seed. A node's result depends only on its inputs, not on what ran before. Tags are interned before, diagnostics collected in tree order after.</td></tr>
<tr><td>Shapes</td><td>solidcore shapes are <code>Arc&lt;ShapeData&gt;</code>, B-rep in a <code>ShapeCell</code>, memos behind mutexes (Send + Sync).</td></tr>
<tr><td>UI step budget</td><td>A step dispatches, then waits until a node finished and the deadline passed (or all done); running nodes continue in the background. <code>--jobs 1</code> and wasm32 use the previous loop.</td></tr>
<tr><td>Interface</td><td><code>--jobs N</code> (1–256) on evaluating commands, default <code>MCAD_JOBS</code>, else the number of CPUs; invalid <code>MCAD_JOBS</code> → exit 2. Daemon (per call) and desktop engine process take it; the browser stays sequential.</td></tr>
</table>
<h3>Determinism and parallelism tests</h3>
<table><tr><th>Test</th><th>What</th></tr>
<tr><td><code>cad-cli/tests/cli.rs</code> <code>parallel_evaluation_is_deterministic</code></td><td><code>diff-reference --mesh --json</code> over all 101 reference files with <code>--jobs 1</code> and twice <code>--jobs 4</code>: identical per-node status, message, volume, faces, edges, selected edges, mesh hashes; <code>eval --json</code> of macropad, flashlight, mug, key_rack, hull_minkowski, project-housing with 1, 2, 4 threads identical</td></tr>
<tr><td><code>cad-engine/tests/parallel.rs</code> (3)</td><td>at least two Booleans run at the same time; identical subtrees are cache hits; all examples and spec models with 1 vs 4 threads (twice, plus a shared engine run): same reports, keys, volumes to the bit, statistics</td></tr>
<tr><td>Renders</td><td>the nine examples rendered with <code>--jobs 4</code> against <code>--jobs 1</code> (captions in section 3)</td></tr>
</table>

<h2>5 Performance: solidcore with threads against single-threaded SMLib</h2>
<p>Columns SMLib … <code>--jobs 4</code>: wall time of <code>mcad eval</code> in seconds (process start to exit; median of {rounds} rounds). Ratios: engine time (<code>stats.kernel_ms</code>, the evaluation itself) relative to SMLib; below 1 = faster than SMLib. STL: <code>mcad export FILE -o x.stl</code>, one run (evaluation + meshing + writing).</p>
<table><tr><th>Model</th><th class="n">nodes</th><th class="n">SMLib</th><th class="n">jobs 1</th><th class="n">jobs 2</th><th class="n">jobs 4</th><th class="n">jobs 1 / SMLib</th><th class="n">jobs 4 / SMLib</th><th class="n">STL SMLib</th><th class="n">STL jobs 1</th><th class="n">STL jobs 4</th></tr>
{evalrows}
{sumrow(f"heavy models (SMLib ≥ 1 s, {len(heavy)})", heavy)}
{sumrow(f"all {len(allfiles)}", allfiles)}
</table>
{ratio_chart(heavy)}
<p class="small">Engine time relative to SMLib for the models where SMLib needs at least 1 s.</p>

<h3>Corpus: the 101 reference files</h3>
<p>Sums over the files of the per-file medians ({crounds} rounds), seconds; engine = evaluation only, wall = whole processes:</p>
<table><tr><th>Variant</th><th class="n">engine</th><th class="n">engine / SMLib</th><th class="n">wall</th></tr>{"".join(f'<tr><td>{esc(VL.get(v, v))}</td><td class="n">{f(ctot[v], 1)}</td><td class="n"><b>{f(ctot[v] / ctot["smlib"])}×</b></td><td class="n">{f(cwall[v], 1)}</td></tr>' for v in cv)}</table>
<p>solidcore before this branch and <code>--jobs 1</code> after it are equal ({f(ctot["j1"] / ctot["sc0"]) if ctot.get("sc0") else "–"}×): phases A and B cost nothing sequentially. {csmall} of the files take SMLib under 0.5 s; there process start and language dominate. The twelve most expensive files (SMLib engine time; <code>f11_pin_coincident</code>: SMLib fails there, solidcore succeeds):</p>
<table><tr><th>File</th>{"".join(f'<th class="n">{esc(VL.get(v, v))}</th>' for v in cv)}<th class="n">jobs 4 / SMLib</th></tr>{corpus_rows}</table>

<h3>Why threads do not close the gap</h3>
<p>Node-level parallelism overlaps only nodes that do not depend on each other; the evaluation cannot be shorter than its longest chain of dependent nodes. Kernel calls timed per node (temporary hook, sequential):</p>
<table><tr><th>Model</th><th>Heavy calls</th><th>Chain</th></tr>
<tr><td><code>08-macropad</code></td><td>Boolean 8.5 s, fillet 6.0 s, Boolean 2.1 s, four Booleans ~1 s, shell 0.6 s</td><td>case → fillet → cut one after the other: floor ≈ 16.6 s, reached 17.2 s with threads</td></tr>
<tr><td><code>07-mug</code>, everyday <code>mug</code></td><td>fillet 5.1 s, shell 3.0 s, union 1.6 s, sweep 0.9 s</td><td>loft → shell → union(fillet) is a chain; only the sweep runs alongside</td></tr>
<tr><td><code>text_sign</code></td><td>one Boolean 13.3 s (62 %)</td><td>a single node over plate, rim and both texts</td></tr>
<tr><td><code>trivet</code>, <code>gear</code>, <code>key_rack</code></td><td>union → chamfer/fillet → difference</td><td>chains; key_rack's identical hooks are one cached subtree</td></tr>
<tr><td>everyday <code>housing</code></td><td>several independent sub-bodies</td><td>best case: 1.48× engine speed-up with 4 threads</td></tr>
</table>
<p><code>--jobs 2</code> brings almost all of the gain; none of the models has more than two heavy independent branches at once. To beat SMLib across the board, solidcore's single operations must get faster or parallel inside (face pairs of a Boolean, faces of a fillet). Interactive use profits from the cache in both cases: after an edit only the changed path is evaluated again, which is again a chain.</p>

<h2>6 Test runs on the final state</h2>
<table><tr><th>Command</th><th>Result</th><th class="n">Duration</th></tr>{testrows}</table>
<p class="small">Container without WebKitGTK: <code>cad-app</code> and the <code>gui</code> feature of <code>cad-cli</code> are not built. Rust tests crate by crate (debug test binaries of about 500 MB each). Playwright 1.56.1 with the preinstalled Chromium through a browser-path shim.</p>
{f"<h2>7 Browser</h2>{uifigs}" if uifigs else ""}

<h2>{8 if uifigs else 7} Limitations and open points</h2>
<table><tr><th>Item</th><th>Status</th></tr>
<tr><td>Speed against SMLib</td><td>solidcore is slower per operation on most models (open point 10 of the handover; <code>07-mug</code>: fillet and shell); threads do not compensate where one chain dominates. Next step: profile solidcore's Boolean and fillet, parallelism inside a kernel call.</td></tr>
<tr><td>Variable radius</td><td>not supported (different radius laws on the edges of one fillet); clear error.</td></tr>
<tr><td><code>sketch_cable_duct</code></td><td>Boolean differs from SMLib's (handover open point).</td></tr>
<tr><td>Browser</td><td>sequential (no threads in the Wasm build); Wasm 6.66 MB Brotli.</td></tr>
<tr><td>Desktop GUI</td><td>takes <code>MCAD_JOBS</code>, no <code>--jobs</code> flag; not built here (no WebKitGTK).</td></tr>
<tr><td><code>Job::current()</code></td><td>with threads the first running node in evaluation order (spinner/watchdog text), an approximation.</td></tr>
<tr><td>Library default</td><td><code>Engine</code> stays sequential unless <code>set_workers</code> is called (CLI, daemon, desktop engine do).</td></tr>
</table>
</main>
</body>
</html>
"""
page = re.sub(r"(<table>)(<tr><th.*?</tr>)", r"\1<thead>\2</thead>", page, flags=re.S)
(OUT / "report.html").write_text(page)
print(OUT / "report.html", len(page), "characters")
