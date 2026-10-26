#!/usr/bin/env python3
"""Generate test report 1 "Kernel, language, engine, CLI".

    python3 tools/report/report1.py                 # measure everything, HTML + images + PDF
    python3 tools/report/report1.py --repeat 30     # additionally repeated runs of kernel-smlib
    python3 tools/report/report1.py --reuse         # only HTML/PDF from target/report1/data.json

Output: docs/reports/01-kernel-sprache-engine-cli/{report.html, report.pdf, *.png}.
Intermediate state (measurements, Wasm package, logs): target/report1/.

Requirements (see the report, section test environment):
  * SMLIB_PREBUILT (default: /home/user/controller-cad/smlib/build-native) or an own build,
    third_party/usd-brep (smlib/fetch.sh),
  * emsdk (/opt/tools/emsdk) for smlib/build-wasm.sh, wasm-bindgen-cli in the Cargo.lock version,
  * openscad 2021.01 and xvfb-run for the comparison images,
  * Node 22 and the npm package playwright in tools/report (``npm i playwright@1.56.1 --no-save``)
    with Chromium under PLAYWRIGHT_BROWSERS_PATH for the PDF.
"""

from __future__ import annotations

import argparse
import datetime as dt
import gzip
import html
import json
import os
import platform
import re
import shutil
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "docs" / "reports" / "01-kernel-sprache-engine-cli"
WORK = ROOT / "target" / "report1"
CAD = ROOT / "target" / "release" / "cad"
SMLIB_WASM = ROOT / "target" / "smlib-wasm"
PKG_ROOT = WORK / "wasm-web"  # same layout as web/ (scripts/, src/wasm-pkg/), outside web/
DEFAULT_PREBUILT = "/home/user/controller-cad/smlib/build-native"

# Models with renders: one per tests/models/spec file, plus typical OpenSCAD models.
COMPAT_RENDERS = [
    "05_nested_booleans",
    "06_hole_grid",
    "07_crown_ring",
    "12_linear_extrude_twist",
    "13_rotate_extrude_ring",
    "14_pipe_bend",
    "15_vase",
    "19_hex_nut",
    "22_gear",
    "31_phone_stand",
]
IMG = (600, 450)
# Our iso view looks along (-1, 1, -1); OpenSCAD gets the same direction, orthographic.
OPENSCAD_CAMERA = "100,-100,100,0,0,0"
# Remark under the repetition table (findings that the counts alone do not show).
REPEAT_NOTE = ("Before the fixes: 4 of 40 runs with 8 threads failed (shell too thick). After the rand() fix, 100 runs "
               "with 8 threads still failed once each in <code>invalid_profiles_fail_cleanly</code> "
               "(memory error in SMLib's shell) and <code>handles_are_released</code> (parallel counter); both have been fixed since.")

CRATE_ROLE = {
    "cad-syntax": "Lexer, parser, AST",
    "cad-eval": "Language evaluation, grids, anchors, handles",
    "cad-ir": "Geometry tree, cache keys",
    "cad-kernel": "Kernel trait, mesh, export",
    "kernel-mock": "Test kernel",
    "kernel-smlib": "SMLib facade (native)",
    "cad-engine": "Evaluation, cache, provenance, zones",
    "cad-session": "Session, protocol",
    "cad-render": "Software rasterizer",
    "cad-cli": "CLI",
    "cad-wasm": "Wasm entry point (worker)",
    "compat-tests": "OpenSCAD comparison, stage 1",
}


def log(*a):
    print("[report1]", *a, file=sys.stderr, flush=True)


def run(cmd, *, cwd=ROOT, env=None, check=True, timeout=3600, capture=True):
    e = dict(os.environ)
    e.setdefault("SMLIB_PREBUILT", DEFAULT_PREBUILT)
    if env:
        e.update(env)
    t0 = time.monotonic()
    r = subprocess.run(cmd, cwd=cwd, env=e, text=True, timeout=timeout,
                       stdout=subprocess.PIPE if capture else None,
                       stderr=subprocess.STDOUT if capture else None)
    dtime = time.monotonic() - t0
    if check and r.returncode != 0:
        tail = (r.stdout or "")[-4000:]
        raise SystemExit(f"command failed ({r.returncode}): {' '.join(map(str, cmd))}\n{tail}")
    return r, dtime


def first_line(cmd):
    try:
        r = subprocess.run(cmd, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=60)
        return (r.stdout or "").strip().splitlines()[0]
    except Exception as ex:  # noqa: BLE001
        return f"not available ({ex.__class__.__name__})"


# ---------------------------------------------------------------- measurements


def environment():
    cpu = "?"
    try:
        for line in Path("/proc/cpuinfo").read_text().splitlines():
            if line.startswith("model name"):
                cpu = line.split(":", 1)[1].strip()
                break
    except OSError:
        pass
    mem = "?"
    try:
        kb = int(re.search(r"MemTotal:\s+(\d+)", Path("/proc/meminfo").read_text()).group(1))
        mem = f"{kb / 1024 / 1024:.0f} GiB"
    except (OSError, AttributeError):
        pass
    osname = platform.platform()
    try:
        osname = re.search(r'PRETTY_NAME="([^"]+)"', Path("/etc/os-release").read_text()).group(1)
    except (OSError, AttributeError):
        pass
    emcc = os.environ.get("EMSDK", "/opt/tools/emsdk") + "/upstream/emscripten/emcc"
    git = first_line(["git", "-C", str(ROOT), "log", "-1", "--format=%h %cd %s", "--date=format:%Y-%m-%d %H:%M"])
    branch = first_line(["git", "-C", str(ROOT), "rev-parse", "--abbrev-ref", "HEAD"])
    pw = "?"
    try:
        pw = json.loads((ROOT / "tools/report/node_modules/playwright/package.json").read_text())["version"]
    except (OSError, KeyError, ValueError):
        pass
    return {
        "date": dt.datetime.now().strftime("%Y-%m-%d %H:%M"),
        "os": osname,
        "kernel": platform.release(),
        "cpu": f"{cpu}, {os.cpu_count()} cores",
        "memory": mem,
        "rustc": first_line(["rustc", "--version"]),
        "cargo": first_line(["cargo", "--version"]),
        "emcc": first_line([emcc, "--version"]),
        "node": first_line(["node", "--version"]),
        "openscad": first_line(["openscad", "--version"]),
        "wasm_bindgen": first_line(["wasm-bindgen", "--version"]),
        "playwright": pw,
        "python": platform.python_version(),
        "smlib": "usd-brep @ c9c5979 (SMLib, SM_API), prebuilt natively: " + os.environ.get("SMLIB_PREBUILT", DEFAULT_PREBUILT),
        "commit": git,
        "branch": branch,
    }


def workspace_packages():
    r, _ = run(["cargo", "metadata", "--no-deps", "--format-version", "1"])
    d = json.loads(r.stdout)
    return [p["name"] for p in d["packages"]]


RE_RUNNING = re.compile(r"^\s+Running (.+?) \((\S+)\)\s*$")
RE_DOC = re.compile(r"^\s+Doc-tests (\S+)")
RE_RESULT = re.compile(r"test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out; finished in ([\d.]+)s")


def parse_cargo_test(out):
    bins = []
    cur = None
    for line in out.splitlines():
        m = RE_RUNNING.match(line)
        if m:
            cur = m.group(1)
            continue
        m = RE_DOC.match(line)
        if m:
            cur = "doc-tests"
            continue
        m = RE_RESULT.search(line)
        if m and cur:
            bins.append({
                "target": cur,
                "ok": m.group(1) == "ok",
                "passed": int(m.group(2)),
                "failed": int(m.group(3)),
                "ignored": int(m.group(4)),
                "seconds": float(m.group(7)),
            })
            cur = None
    return bins


def cargo_tests():
    log("cargo test: build")
    run(["cargo", "test", "--workspace", "--no-run", "-q"], timeout=7200)
    crates = []
    for pkg in workspace_packages():
        log("cargo test -p", pkg)
        r, wall = run(["cargo", "test", "-p", pkg], check=False, timeout=3600)
        bins = [b for b in parse_cargo_test(r.stdout) if b["passed"] + b["failed"] + b["ignored"] > 0]
        crates.append({
            "crate": pkg,
            "ok": r.returncode == 0,
            "passed": sum(b["passed"] for b in bins),
            "failed": sum(b["failed"] for b in bins),
            "ignored": sum(b["ignored"] for b in bins),
            "test_seconds": round(sum(b["seconds"] for b in bins), 2),
            "wall_seconds": round(wall, 2),
            "binaries": bins,
        })
    return crates


def kernel_test_binary():
    r, _ = run(["cargo", "test", "-p", "kernel-smlib", "--test", "kernel", "--no-run"])
    m = re.search(r"\((target/debug/deps/kernel-[0-9a-f]+)\)", r.stdout)
    if not m:
        raise SystemExit("kernel test binary not found:\n" + r.stdout[-2000:])
    return ROOT / m.group(1)


def repeat_runs(n):
    if n <= 0:
        return None
    exe = kernel_test_binary()
    out = {}
    for label, args in (("serial (--test-threads=1)", ["--test-threads=1"]),
                        ("parallel (default, 4 threads)", []),
                        ("parallel (--test-threads=8)", ["--test-threads=8"])):
        fails = 0
        failed_tests = []
        t0 = time.monotonic()
        for i in range(n):
            log(f"repeat {label} {i + 1}/{n}")
            r = subprocess.run([str(exe)] + args, cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
            if r.returncode != 0:
                fails += 1
                names = re.findall(r"^test (\S+) \.\.\. FAILED", r.stdout, re.M)
                failed_tests += names or ["(aborted)"]
                (WORK / f"repeat-failure-{len(failed_tests)}.log").write_text(r.stdout)
        out[label] = {"runs": n, "failed": fails, "seconds": round(time.monotonic() - t0, 1),
                      "failed_tests": failed_tests}
    return out


def build_release():
    log("cargo build --release -p cad-cli")
    _, t = run(["cargo", "build", "--release", "-p", "cad-cli"], timeout=7200)
    return round(t, 1)


def compressed_sizes(path: Path):
    b = path.read_bytes()
    gz = len(gzip.compress(b, 9))
    br = None
    try:
        r = subprocess.run(["node", "-e",
                            "const z=require('zlib'),b=require('fs').readFileSync(process.argv[1]);"
                            "console.log(z.brotliCompressSync(b,{params:{[z.constants.BROTLI_PARAM_QUALITY]:11,"
                            "[z.constants.BROTLI_PARAM_SIZE_HINT]:b.length}}).length)", str(path)],
                           text=True, stdout=subprocess.PIPE, timeout=120)
        br = int(r.stdout.strip())
    except Exception:  # noqa: BLE001
        pass
    return {"raw": len(b), "gzip": gz, "brotli": br}


def wasm_measurements(rebuild):
    data = {}
    if rebuild or not (SMLIB_WASM / "smlib.wasm").exists():
        log("smlib/build-wasm.sh")
        r, t = run(["bash", str(ROOT / "smlib/build-wasm.sh"), str(SMLIB_WASM)], timeout=7200)
        data["smlib_build_seconds"] = round(t, 1)
    # kernel test in Node
    log("node crates/kernel-smlib/js/test-node.mjs")
    r, t = run(["node", "crates/kernel-smlib/js/test-node.mjs", str(SMLIB_WASM)], check=False, timeout=1200)
    data["node_test_ok"] = r.returncode == 0
    data["node_test_log"] = r.stdout.strip().splitlines()
    m = re.search(r"read ([\d.]+) ms, compile\+instantiate\+init ([\d.]+) ms", r.stdout)
    if m:
        data["smlib_read_ms"] = float(m.group(1))
        data["smlib_init_ms"] = float(m.group(2))
    m = re.search(r"scenario (.*)$", r.stdout, re.M)
    if m:
        data["smlib_scenario"] = m.group(1)
    data["node_tests"] = re.findall(r"^(ok|FAILED)\s+(.+?) \(([\d.]+) ms\)$", r.stdout, re.M)
    # browser engine package (cad-wasm + wasm-bindgen + wasm-opt), built outside web/
    log("cad-wasm → wasm-bindgen")
    run(["cargo", "build", "-q", "-p", "cad-wasm", "--target", "wasm32-unknown-unknown", "--release"], timeout=7200)
    pkg = PKG_ROOT / "src" / "wasm-pkg"
    if pkg.exists():
        shutil.rmtree(pkg)
    pkg.mkdir(parents=True)
    run(["wasm-bindgen", "--target", "web", "--out-dir", str(pkg),
         str(ROOT / "target/wasm32-unknown-unknown/release/cad_wasm.wasm")])
    wasm_opt = shutil.which("wasm-opt") or os.environ.get("EMSDK", "/opt/tools/emsdk") + "/upstream/bin/wasm-opt"
    if Path(wasm_opt).exists():
        run([wasm_opt, "-O3", "--enable-bulk-memory", "--enable-nontrapping-float-to-int", "--enable-sign-ext",
             "--enable-mutable-globals", "--enable-reference-types", "--enable-multivalue",
             str(pkg / "cad_wasm_bg.wasm"), "-o", str(pkg / "cad_wasm_bg.opt.wasm")])
        (pkg / "cad_wasm_bg.opt.wasm").replace(pkg / "cad_wasm_bg.wasm")
        data["wasm_opt"] = True
    bridge = next(pkg.glob("snippets/**/smlib-bridge.js"))
    (pkg / "smlib-bridge.js").write_text(f'export * from "./{bridge.relative_to(pkg).as_posix()}";\n')
    for f in ("smlib.js", "smlib.wasm"):
        shutil.copy2(SMLIB_WASM / f, pkg / f)
    scripts = PKG_ROOT / "scripts"
    scripts.mkdir(parents=True, exist_ok=True)
    shutil.copy2(ROOT / "web/scripts/smoke-node.mjs", scripts / "smoke-node.mjs")
    log("node smoke-node.mjs --json")
    runs = []
    for _ in range(3):
        r, _t = run(["node", str(scripts / "smoke-node.mjs"), "--json"], check=False, timeout=1200)
        try:
            runs.append(json.loads(r.stdout[r.stdout.index("{"):]))
        except ValueError:
            data["smoke_error"] = r.stdout[-3000:]
            break
    data["smoke_runs"] = runs
    data["sizes"] = {
        "smlib.wasm": compressed_sizes(pkg / "smlib.wasm"),
        "smlib.js": compressed_sizes(pkg / "smlib.js"),
        "cad_wasm_bg.wasm": compressed_sizes(pkg / "cad_wasm_bg.wasm"),
        "cad_wasm.js": compressed_sizes(pkg / "cad_wasm.js"),
    }
    return data


def compare():
    log("tests/openscad-compat/compare.py")
    rep = WORK / "compare-report.json"
    r, t = run([sys.executable, "tests/openscad-compat/compare.py", "--report", str(rep)],
               env={"CAD": str(CAD)}, check=False, timeout=3600)
    d = json.loads(rep.read_text())
    d["stdout"] = r.stdout.splitlines()[-3:]
    d["exit_code"] = r.returncode
    d["seconds"] = round(t, 1)
    return d


def reference():
    return json.loads((ROOT / "tests/openscad-compat/reference.json").read_text())["files"]


def png_size_ok(p: Path):
    return p.exists() and p.stat().st_size <= 300 * 1024


def renders(ref):
    OUT.mkdir(parents=True, exist_ok=True)
    out = []
    size = f"{IMG[0]}x{IMG[1]}"
    for src in sorted((ROOT / "tests/models/spec").glob("*.mcad")):
        name = src.stem
        dst = OUT / f"spec-{name}.png"
        _, t = run([str(CAD), "render", str(src), "--size", size, "-o", str(dst)], check=False)
        out.append({"group": "spec", "name": name, "file": f"tests/models/spec/{src.name}", "ours": dst.name,
                    "seconds": round(t, 2), "purpose": purpose(src)})
    # highlighted subtracted body (cad-render transparency)
    dst = OUT / "spec-gehaeuse-zeile12.png"
    run([str(CAD), "render", "tests/models/spec/housing.mcad", "--line", "12", "--size", size, "-o", str(dst)], check=False)
    out.append({"group": "highlight", "name": "housing --line 12", "file": "tests/models/spec/housing.mcad",
                "ours": dst.name, "purpose": "line 12: subtracted body red and transparent, the rest dimmed"})
    for name in COMPAT_RENDERS:
        src = ROOT / "tests/models/openscad-compat" / f"{name}.scad"
        dst = OUT / f"compat-{name}.png"
        _, t = run([str(CAD), "render", str(src), "--size", size, "-o", str(dst)], check=False)
        item = {"group": "compat", "name": name, "file": f"tests/models/openscad-compat/{src.name}", "ours": dst.name,
                "seconds": round(t, 2), "purpose": purpose(src)}
        osc = OUT / f"compat-{name}-openscad.png"
        defines = []
        for d in ref.get(src.name, {}).get("defines", []):
            defines += ["-D", d]
        if shutil.which("xvfb-run"):
            run(["xvfb-run", "-a", "openscad", "-q", "--render", *defines, "-o", str(osc), f"--imgsize={IMG[0]},{IMG[1]}",
                 "--viewall", "--autocenter", "--projection=ortho", f"--camera={OPENSCAD_CAMERA}", str(src)],
                check=False, timeout=1200)
        if osc.exists() and osc.stat().st_size > 0:
            item["openscad"] = osc.name
        out.append(item)
    for it in out:
        for k in ("ours", "openscad"):
            if k in it:
                p = OUT / it[k]
                it[k + "_bytes"] = p.stat().st_size if p.exists() else 0
                it[k + "_size_ok"] = png_size_ok(p)
    return out


def purpose(src: Path):
    """Text of the `// Purpose:` (formerly `// Zweck:`) header comment, including its continuation lines."""
    lines = src.read_text(encoding="utf-8").splitlines()
    for i, line in enumerate(lines[:3]):
        m = re.match(r"//\s*(?:Purpose|Zweck):\s*(.*)", line)
        if m:
            text = [m.group(1).strip()]
            for cont in lines[i + 1:]:
                c = re.match(r"//\s*(.*)", cont)
                if not c or not c.group(1).strip():
                    break
                text.append(c.group(1).strip())
            return " ".join(text)
    return ""


def collect(args):
    WORK.mkdir(parents=True, exist_ok=True)
    data = {"environment": environment()}
    data["release_build_seconds"] = build_release()
    data["tests"] = cargo_tests()
    data["repeat"] = repeat_runs(args.repeat)
    data["wasm"] = wasm_measurements(args.rebuild_wasm)
    data["compare"] = compare()
    ref = reference()
    data["reference_meta"] = {k: {"class": v.get("class"), "defines": v.get("defines", [])} for k, v in ref.items()}
    data["renders"] = renders(ref)
    (WORK / "data.json").write_text(json.dumps(data, indent=1, ensure_ascii=False))
    return data


# ---------------------------------------------------------------- HTML

def esc(s):
    return html.escape(str(s), quote=True)


def num(x, nd=1):
    """Number format of the reports: decimal point, space as thousands separator."""
    if x is None:
        return "–"
    if isinstance(x, int):
        return f"{x:,}".replace(",", " ")
    s = f"{x:,.{nd}f}"
    return s.replace(",", " ")


def pct(x, nd=2):
    return "–" if x is None else num(100.0 * x, nd) + " %"


def kib(n):
    return "–" if n is None else num(n / 1024.0, 0) + " KiB"


def mib(n):
    return "–" if n is None else num(n / 1024.0 / 1024.0, 2) + " MiB"


CSS = """
:root { --ink:#1d2129; --muted:#5b6270; --line:#d6d9df; --soft:#f3f4f6; --ok:#1f7a3a; --bad:#b3261e; --acc:#2f5fb3; }
* { box-sizing: border-box; }
html { background:#fff; }
body { margin:0 auto; max-width: 960px; padding: 24px 28px 48px; color: var(--ink);
  font: 10.5pt/1.45 "Inter", "Segoe UI", "Helvetica Neue", Arial, sans-serif; background:#fff; }
h1 { font-size: 20pt; margin: 0 0 4px; letter-spacing: -0.01em; }
h2 { font-size: 13.5pt; margin: 26px 0 8px; padding-bottom: 3px; border-bottom: 1.5px solid var(--ink); break-after: avoid; }
h3 { font-size: 11pt; margin: 16px 0 6px; break-after: avoid; }
p { margin: 6px 0; }
.sub { color: var(--muted); margin-bottom: 14px; }
.kpis { display: grid; grid-template-columns: repeat(4, 1fr); gap: 8px; margin: 14px 0 6px; }
.kpi { border: 1px solid var(--line); border-radius: 6px; padding: 8px 10px; }
.kpi b { display: block; font-size: 15pt; line-height: 1.2; }
.kpi span { color: var(--muted); font-size: 9pt; }
table { border-collapse: collapse; width: 100%; margin: 6px 0 10px; font-size: 9pt; font-variant-numeric: tabular-nums; }
th, td { border-bottom: 1px solid var(--line); padding: 3px 6px; text-align: left; vertical-align: top; }
th { background: var(--soft); font-weight: 600; }
td.n, th.n { text-align: right; white-space: nowrap; }
tr { break-inside: avoid; }
.ok { color: var(--ok); font-weight: 600; }
.bad { color: var(--bad); font-weight: 600; }
code { font-family: "JetBrains Mono", "DejaVu Sans Mono", monospace; font-size: 8.8pt; background: var(--soft); padding: 0 3px; border-radius: 3px; }
.small { font-size: 8.8pt; color: var(--muted); }
ul { margin: 4px 0 8px 18px; padding: 0; }
li { margin: 2px 0; }
.grid2 { display: grid; grid-template-columns: 1fr 1fr; gap: 10px 12px; }
.grid3 { display: grid; grid-template-columns: 1fr 1fr 1fr; gap: 10px 10px; }
figure { margin: 0; break-inside: avoid; }
figure img { width: 100%; height: auto; border: 1px solid var(--line); border-radius: 4px; display: block; }
figcaption { font-size: 8.5pt; color: var(--muted); margin-top: 3px; }
figcaption b { color: var(--ink); }
.pair { break-inside: avoid; margin-bottom: 12px; }
.pair h3 { margin: 10px 0 4px; }
.note { border-left: 3px solid var(--acc); padding: 4px 10px; background: #f5f8fd; margin: 8px 0; }
@page { size: A4; margin: 14mm 13mm 16mm; }
@media print {
  body { max-width: none; padding: 0; font-size: 9.5pt; }
  h2 { break-before: auto; }
  .pagebreak { break-before: page; }
  a { color: inherit; text-decoration: none; }
}
"""


def status_cell(ok):
    return '<span class="ok">ok</span>' if ok else '<span class="bad">failed</span>'


def html_report(d):
    env = d["environment"]
    tests = d["tests"]
    comp = d["compare"]
    w = d["wasm"]
    total_pass = sum(t["passed"] for t in tests)
    total_fail = sum(t["failed"] for t in tests)
    total_ign = sum(t["ignored"] for t in tests)
    total_time = sum(t["test_seconds"] for t in tests)
    n_ok = comp["summary"]["ok"]
    n_files = comp["summary"]["files"]
    smoke = w.get("smoke_runs") or []
    smoke_t = {}
    if smoke:
        keys = [k for k in smoke[0]["timings"] if isinstance(smoke[0]["timings"][k], (int, float))]
        for k in keys:
            vals = sorted(r["timings"][k] for r in smoke if k in r["timings"])
            smoke_t[k] = vals[len(vals) // 2]
    parts = []
    a = parts.append
    a("<!doctype html><html lang='en'><head><meta charset='utf-8'>")
    a("<meta name='viewport' content='width=device-width, initial-scale=1'>")
    a("<title>Test report 1: Kernel, language, engine, CLI</title>")
    a(f"<style>{CSS}</style></head><body>")
    a("<h1>Test report 1: Kernel, language, engine, CLI</h1>")
    a(f"<p class='sub'>controller-cad · As of {esc(env['date'])} · Commit {esc(env['commit'])} "
      f"(branch <code>{esc(env['branch'])}</code>)</p>")
    a("<div class='kpis'>")
    a(f"<div class='kpi'><b>{num(total_pass)}</b><span>Rust tests passed ({total_fail} failed, {total_ign} ignored)</span></div>")
    a(f"<div class='kpi'><b>{n_ok}/{n_files}</b><span>OpenSCAD models within tolerance</span></div>")
    sm = w["sizes"]["smlib.wasm"]
    cw = w["sizes"]["cad_wasm_bg.wasm"]
    a(f"<div class='kpi'><b>{mib((sm['brotli'] or sm['gzip']) + (cw['brotli'] or cw['gzip']))}</b>"
      f"<span>Wasm total, Brotli (SMLib {mib(sm['brotli'])}, engine {mib(cw['brotli'])})</span></div>")
    st = smoke_t.get("set_source_to_root_mesh_ms")
    a(f"<div class='kpi'><b>{num(st, 0)} ms</b><span>Source → root mesh (example gehaeuse, Node, cold)</span></div>")
    a("</div>")

    # ---- scope
    a("<h2>1 Scope</h2>")
    a("<p>This report covers the four lower layers of the application (the UI is not part of this report):</p><ul>")
    a("<li><b>Kernel:</b> SMLib/SM_API (NVIDIA usd-brep @ c9c5979) behind a C++ facade with a flat C ABI "
      "(handles, error codes, face tags for provenance), linked natively and as Emscripten Wasm "
      "(<code>smlib.wasm</code>); Rust binding <code>kernel-smlib</code>. Operations: primitives, transformations, "
      "booleans, fillet/chamfer, extrusion (with twist/scale), revolve, loft, sweep, NURBS surface, thicken, shell, "
      "edge/face queries, classification, tessellation, mass properties.</li>")
    a("<li><b>Language:</b> OpenSCAD dialect (<code>cad-syntax</code>, <code>cad-eval</code>): variables, functions, "
      "modules, <code>children()</code>, <code>for</code>/<code>if</code>/<code>let</code>/<code>each</code>, "
      "list comprehensions, <code>echo</code>/<code>assert</code>, modifiers, 2D/3D primitives, extrusions; plus the "
      "extensions from the spec: grids (<code>grid</code>), anchors, named instances, <code>box()</code> between "
      "grid points, fillets/chamfers with zones, loft, sweep, handles.</li>")
    a("<li><b>Engine:</b> <code>cad-engine</code> evaluates the geometry tree incrementally with a cache (content-based "
      "keys), maps faces and edges to source lines, selects edges via zones and exports STL/3MF; "
      "<code>cad-session</code> provides the protocol for the worker and the CLI.</li>")
    a("<li><b>CLI:</b> <code>cad check | eval | inspect | render | export</code> with <code>--json</code>; "
      "<code>render</code> uses the software rasterizer <code>cad-render</code>.</li></ul>")

    # ---- environment
    a("<h2>2 Test environment</h2><table>")
    rows = [
        ("System", f"{env['os']} (Linux {env['kernel']}), {env['cpu']}, {env['memory']} RAM, container without GPU"),
        ("Rust", f"{env['rustc']}; {env['cargo']}"),
        ("Kernel", env["smlib"]),
        ("Emscripten", env["emcc"]),
        ("wasm-bindgen", env["wasm_bindgen"]),
        ("Node", env["node"]),
        ("OpenSCAD (reference)", env["openscad"] + "; images with xvfb-run, orthographic"),
        ("PDF", f"Chromium from Playwright {env['playwright']}"),
        ("Python", env["python"]),
    ]
    for k, v in rows:
        a(f"<tr><th style='width:22%'>{esc(k)}</th><td>{esc(v)}</td></tr>")
    a("</table>")
    a(f"<p class='small'>Release build of the CLI: {num(d['release_build_seconds'])} s (incremental). All numbers come "
      "from one run of <code>tools/report/report1.py</code>; times are wall-clock times on this machine.</p>")

    # ---- tests
    a("<h2>3 Test results</h2>")
    a("<h3>3.1 Rust tests per crate</h3>")
    a("<table><tr><th>Crate</th><th>Content</th><th class='n'>passed</th><th class='n'>failed</th>"
      "<th class='n'>ignored</th><th class='n'>Test time</th><th class='n'>cargo run time</th><th>Status</th></tr>")
    for t in tests:
        a(f"<tr><td><code>{esc(t['crate'])}</code></td><td>{esc(CRATE_ROLE.get(t['crate'], ''))}</td>"
          f"<td class='n'>{t['passed']}</td><td class='n'>{t['failed']}</td><td class='n'>{t['ignored']}</td>"
          f"<td class='n'>{num(t['test_seconds'], 2)} s</td><td class='n'>{num(t['wall_seconds'], 1)} s</td>"
          f"<td>{status_cell(t['ok'])}</td></tr>")
    a(f"<tr><th>Total</th><th></th><th class='n'>{total_pass}</th><th class='n'>{total_fail}</th>"
      f"<th class='n'>{total_ign}</th><th class='n'>{num(total_time, 2)} s</th>"
      f"<th class='n'>{num(sum(t['wall_seconds'] for t in tests), 1)} s</th><th></th></tr></table>")
    a("<p class='small'>Test time: sum of the times reported by the test binaries (debug build, SMLib and facade "
      "optimized). cargo run time: <code>cargo test -p &lt;crate&gt;</code> after a prior build.</p>")
    rep = d.get("repeat")
    if rep:
        a("<h3>3.2 Repeated runs of <code>kernel-smlib/tests/kernel.rs</code></h3><table>"
          "<tr><th>Mode</th><th class='n'>Runs</th><th class='n'>failed</th><th>Test</th>"
          "<th class='n'>Duration</th></tr>")
        for k, v in rep.items():
            ft = ", ".join(f"<code>{esc(t)}</code>" for t in v.get("failed_tests", [])) or "–"
            a(f"<tr><td>{esc(k)}</td><td class='n'>{v['runs']}</td><td class='n'>{v['failed']}</td>"
              f"<td>{ft}</td><td class='n'>{num(v['seconds'], 0)} s</td></tr>")
        a("</table>")
        if REPEAT_NOTE:
            a(f"<p class='small'>{REPEAT_NOTE}</p>")
    a("<h3>3.3 Kernel as Wasm (Node)</h3>")
    nt = w.get("node_tests") or []
    a(f"<p><code>node crates/kernel-smlib/js/test-node.mjs</code>: {sum(1 for x in nt if x[0] == 'ok')} of {len(nt)} "
      f"tests passed ({'ok' if w.get('node_test_ok') else 'failed'}).</p>")
    a("<table><tr><th>Test</th><th class='n'>Time</th><th>Status</th></tr>")
    for s, name, ms in nt:
        a(f"<tr><td>{esc(name)}</td><td class='n'>{num(float(ms), 1)} ms</td><td>{status_cell(s == 'ok')}</td></tr>")
    a("</table>")

    # ---- fixed defects
    a("<h3>3.4 Bugs fixed in this round</h3><table>"
      "<tr><th style='width:20%'>Bug</th><th style='width:40%'>Cause</th><th style='width:40%'>Fix, evidence</th></tr>")
    fixes = [
        ("Union of three orthogonal cylinders: 3237 instead of 3800 mm³ (05_nested_booleans)",
         "For (Z ∪ Y) ∪ X, SMLib reports success but drops the −x arm of the X cylinder (bounding box x ≥ −4); "
         "(Z ∪ X) ∪ Y is correct, (Y ∪ X) ∪ Z ends with SM_ERR. Only with equal radii (intersection curves with "
         "touching points); with r = 3.9 or 4.1 for the third cylinder the result is correct. <code>mass_props</code> "
         "and the mesh are equally wrong: not a tessellation or export bug. Pairwise, the facade already computed "
         "correctly.",
         "The facade checks every pairwise result (volume inequalities, location of probes, 5 per face, just "
         "inside and outside the operand faces) and otherwise tries: swap operands, different order, and finally "
         "shift the second operand by 10⁻⁶…10⁻⁴ of the model size; if nothing passes, SMLib's result is kept as "
         "before. "
         "Tests: <code>kernel.rs</code> and <code>engine.rs::union_of_three_orthogonal_cylinders</code> "
         "(all 6 orders, nested, as a hole), Node test."),
        ("linear_extrude with twist: bounding box off by 1.25 mm (12_linear_extrude_twist)",
         "Section t was scaled first and rotated afterwards; OpenSCAD rotates first and then scales along x/y "
         "(<code>Scaling(s)·Rotation2D(−twist·t)</code>). With <code>scale = [1, 0.5]</code> this gives different "
         "cross-sections (y up to 5.08 instead of 3.83 mm). Direction of rotation and start angle were correct. In "
         "addition: sections every 22.5° (bounding box error 3·10⁻³ mm), and the mesh was 2.9 % above the volume "
         "because SMLib limits the chord height only at triangle edges.",
         "Order as in OpenSCAD; sections every 7.5° (bounding box within 10⁻⁴ mm, volume 540.000 of 540). On "
         "freeform surfaces the tessellation measures the deviation of the triangle centers and refines until "
         "≤ 2·tol (at most 3 passes, triangle budget). Tests: "
         "<code>twisted_extrude_with_nonuniform_scale_matches_openscad</code>, "
         "<code>twisted_extrude_mesh_volume</code>, <code>engine.rs::linear_extrude_twist_nonuniform_scale_like_openscad</code>."),
        ("Flaky test <code>invalid_profiles_fail_cleanly</code>",
         "Two causes, both in the too-thick <code>shell</code> (10 mm cube, 6 mm wall, must fail). "
         "(1) SMLib calls <code>srand(time(NULL))</code> in <code>SmFace::GetPointsInFace</code> and "
         "<code>SmShell::IsInnerShellOf</code>: before, 4 of 40 runs (8 threads) failed; with a fixed clock "
         "(<code>time()</code> via LD_PRELOAD), 0 of 40. An <code>srand</code> before each operation is not "
         "enough, because SMLib reseeds in the middle of the operation. (2) After that, still 1 of 100: with a "
         "too-thick wall, SMLib's shell reads freed memory (valgrind: <i>Invalid read</i> in "
         "<code>SmProtoEdge::RemoveProtoVertex</code>; solidcore UB-239/242); depending on the heap it fails, "
         "returns a body, or crashes (segfault in a loop after 9 calls).",
         "(1) The facade defines <code>rand()</code>/<code>srand()</code> itself (statically linked, native and "
         "Emscripten): fixed seed, reset on every <code>sc_*</code> call. (2) <code>sc_shell</code> rejects walls "
         "that do not fit between two opposite planar faces (distance ≤ 2·t) before SMLib is called. In addition, "
         "<code>handles_are_released</code> failed when run in parallel (process-wide counter): separate test "
         "binary. Section 3.2; tests <code>determinism.rs</code>, <code>handles.rs</code>."),
        ("Diagonal bands on subtracted bodies rendered transparent",
         "<code>cad-render</code> sorted transparent triangles by centroid. Where triangles overlap in depth, "
         "especially on coplanar faces (subtracted body and its cavity in the dimmed result), the order flips "
         "along the triangle diagonals.",
         "Fragments are sorted by depth per sample, coplanar ones in scene order (figure "
         "<i>gehaeuse --line 12</i> in section 5). Test: "
         "<code>render.rs::transparent_coplanar_faces_have_no_diagonal_bands</code>."),
    ]
    for f, c, r in fixes:
        a(f"<tr><td>{f}</td><td>{c}</td><td>{r}</td></tr>")
    a("</table>")

    # ---- compare table
    a("<h2>4 Comparison with OpenSCAD</h2>")
    a(f"<p><code>tests/openscad-compat/compare.py</code> exports each model as STL "
      f"(<code>cad export</code>, chord tolerance 0.01 mm) and compares volume, area and bounding box with OpenSCAD "
      f"2021.01 (<code>$fn = 128</code> for round shapes, see <code>tests/openscad-compat/README.md</code>). "
      f"Tolerances: bounding box 0.01 mm; volume and area 1 % (<i>polygonal</i>) or 2 % (<i>rounded</i>). Result: "
      f"<b>{n_ok} of {n_files}</b> within tolerance, run time {num(comp['seconds'])} s.</p>")
    a("<table><tr><th>Model</th><th>Class</th><th class='n'>V OpenSCAD [mm³]</th><th class='n'>V ours [mm³]</th>"
      "<th class='n'>ΔV</th><th class='n'>ΔA</th><th class='n'>ΔBBox [mm]</th><th class='n'>Triangles</th>"
      "<th class='n'>Time</th><th>Status</th></tr>")
    for r in comp["results"]:
        dv = r.get("deviation", {})
        ours = r.get("ours") or {}
        refv = r.get("reference") or {}
        a(f"<tr><td><code>{esc(r['file'].removesuffix('.scad'))}</code></td><td>{esc(r.get('class', ''))}</td>"
          f"<td class='n'>{num(refv.get('volume'), 1)}</td><td class='n'>{num(ours.get('volume'), 1)}</td>"
          f"<td class='n'>{pct(dv.get('volume_rel'))}</td><td class='n'>{pct(dv.get('area_rel'))}</td>"
          f"<td class='n'>{num(dv.get('bounds_abs'), 4)}</td><td class='n'>{num(ours.get('triangles'))}</td>"
          f"<td class='n'>{num(r.get('seconds'), 2)} s</td><td>{status_cell(r.get('status') == 'ok')}</td></tr>")
    a("</table>")
    r12 = next((r for r in comp["results"] if r["file"].startswith("12_")), None)
    if r12 and r12.get("ours"):
        a("<div class='note'><b>On 12_linear_extrude_twist:</b> here OpenSCAD's own mesh deviates from the exact "
          "body. The exact value is 1280 + 840 + 540 = 2660 mm³ (twisted cross-sections keep their area); "
          f"OpenSCAD gives {num(r12['reference']['volume'], 1)} mm³ because it splits every twisted side face into "
          "two planar triangles with a fixed diagonal. Our STL has "
          f"{num(r12['ours']['volume'], 1)} mm³. The {pct(r12['deviation']['volume_rel'])} deviation in the "
          "table is therefore OpenSCAD's faceting.</div>")

    # ---- renders
    a("<h2 class='pagebreak'>5 Renderings</h2>")
    a(f"<p>All images {IMG[0]}×{IMG[1]} px, isometric view (looking along (−1, 1, −1), orthographic), "
      "<code>cad render &lt;file&gt; --size 600x450</code>. OpenSCAD images: <code>openscad -o x.png "
      "--render --imgsize=600,450 --viewall --autocenter --projection=ortho --camera=100,-100,100,0,0,0</code> with "
      "the same <code>-D</code> values as the reference.</p>")
    a("<h3>5.1 Examples from the spec (<code>tests/models/spec/</code>)</h3><p class='small'>Only with our CLI: "
      "grids, anchors, fillets and zones do not exist in OpenSCAD.</p><div class='grid2'>")
    for it in d["renders"]:
        if it["group"] in ("spec", "highlight"):
            cap = f"<b>{esc(it['name'])}</b> – {esc(it.get('purpose', ''))}"
            a(f"<figure><img src='{esc(it['ours'])}' alt='{esc(it['name'])}'><figcaption>{cap}</figcaption></figure>")
    a("</div>")
    a("<h3>5.2 OpenSCAD models: ours (left) and OpenSCAD (right)</h3>")
    for it in d["renders"]:
        if it["group"] != "compat":
            continue
        a(f"<div class='pair'><h3><code>{esc(it['name'])}</code> <span class='small'>{esc(it.get('purpose', ''))}</span></h3>"
          "<div class='grid2'>")
        a(f"<figure><img src='{esc(it['ours'])}' alt='{esc(it['name'])} cad render'><figcaption>cad render</figcaption></figure>")
        if it.get("openscad"):
            a(f"<figure><img src='{esc(it['openscad'])}' alt='{esc(it['name'])} OpenSCAD'><figcaption>OpenSCAD 2021.01</figcaption></figure>")
        else:
            a("<figure><figcaption>OpenSCAD image not available</figcaption></figure>")
        a("</div></div>")

    # ---- measurements
    a("<h2>6 Measurements</h2>")
    a("<h3>6.1 Wasm sizes</h3><table><tr><th>File</th><th class='n'>raw</th><th class='n'>gzip −9</th>"
      "<th class='n'>Brotli q11</th></tr>")
    for name, s in w["sizes"].items():
        a(f"<tr><td><code>{esc(name)}</code></td><td class='n'>{kib(s['raw'])}</td><td class='n'>{kib(s['gzip'])}</td>"
          f"<td class='n'>{kib(s['brotli'])}</td></tr>")
    a("</table><p class='small'><code>smlib.wasm</code>: SMLib + SM_API + facade, Emscripten <code>-O3 -fwasm-exceptions</code>; "
      "<code>cad_wasm_bg.wasm</code>: language, engine, session (wasm-bindgen, wasm-opt −O3).</p>")
    a("<h3>6.2 Load time and evaluation (Node 22, one process as in the worker)</h3>")
    labels = {
        "load_smlib_ms": "Load, compile, initialize smlib.wasm",
        "load_cad_wasm_ms": "Load and initialize cad_wasm",
        "set_source_to_preliminary_scene_ms": "SetSource → preliminary scene (without kernel)",
        "set_source_to_scene_ms": "SetSource → complete scene",
        "set_source_to_root_mesh_ms": "SetSource → root mesh",
        "request_all_meshes_ms": "Request meshes of all nodes",
        "edit_to_scene_ms": "Drag handle → TextEdit → new scene",
        "edit_to_coarse_root_mesh_ms": "… → coarse root mesh",
        "warm_set_source_to_preliminary_scene_ms": "Repeated SetSource (warm cache) → preliminary scene",
        "warm_set_source_to_scene_ms": "Repeated SetSource (warm cache) → scene",
        "warm_set_source_to_root_mesh_ms": "Repeated SetSource (warm cache) → root mesh",
    }
    if smoke_t:
        a(f"<p><code>web/scripts/smoke-node.mjs --json</code> with the example <code>gehaeuse</code>, "
          f"median of {len(smoke)} runs, step budget 16 ms.</p>")
        a("<table><tr><th>Step</th><th class='n'>Time</th></tr>")
        for k, v in smoke_t.items():
            if k == "steps":
                continue
            a(f"<tr><td>{esc(labels.get(k, k))}</td><td class='n'>{num(v, 1)} ms</td></tr>")
        if "steps" in smoke_t:
            a(f"<tr><td>Evaluation steps until the complete scene</td><td class='n'>{num(int(smoke_t['steps']))}</td></tr>")
        a(f"<tr><td>Triangles of root mesh / nodes of the scene</td><td class='n'>{num(smoke[0].get('triangles_root'))} / {num(smoke[0].get('nodes'))}</td></tr>")
        a("</table>")
    elif w.get("smoke_error"):
        a(f"<p class='bad'>smoke-node failed.</p><pre>{esc(w['smoke_error'])}</pre>")
    if "smlib_init_ms" in w:
        a(f"<p>Kernel only (<code>test-node.mjs</code>): read file {num(w['smlib_read_ms'])} ms, compile, "
          f"instantiate and initialize {num(w['smlib_init_ms'])} ms. Scenario: {esc(w.get('smlib_scenario', ''))}.</p>")
    a("<h3>6.3 Native evaluation (CLI)</h3>")
    secs = [r.get("seconds") for r in comp["results"] if r.get("seconds") is not None]
    if secs:
        secs_sorted = sorted(secs)
        slow = sorted(comp["results"], key=lambda r: -(r.get("seconds") or 0))[:3]
        a(f"<p><code>cad export</code> per model (process start, evaluation, tessellation, STL): median "
          f"{num(secs_sorted[len(secs) // 2], 2)} s, total {num(sum(secs), 1)} s; longest "
          + ", ".join(f"<code>{esc(r['file'].removesuffix('.scad'))}</code> {num(r['seconds'], 2)} s" for r in slow)
          + ".</p>")
    spec = [it for it in d["renders"] if it["group"] == "spec"]
    if spec:
        a("<p><code>cad render</code> of the spec examples (600×450, 2×2 supersampling): "
          + ", ".join(f"{esc(it['name'])} {num(it['seconds'], 2)} s" for it in spec) + ".</p>")

    # ---- limits
    a("<h2>7 Known limitations</h2><ul>")
    tri12 = (r12 or {}).get("ours", {}).get("triangles")
    limits = [
        "SMLib booleans are unreliable in special configurations. The facade detects wrong results by sampling, "
        "not completely: an error between all probe points goes undetected. The check uses SMLib's point "
        "classification, which itself is wrong for rotated spline bodies (knauf_mirror); if no variant passes, "
        "SMLib's own result is therefore kept. In the shift fallback, the second operand deviates by up to 10⁻⁴ "
        "of the model size. The check costs time: "
        "<code>cad export</code> of all 41 examples, measured during the fix, 13.7 s → 18.9 s (+38 %), most for "
        "unions of many parts (07_crown_ring ×2.1) and for knauf_mirror (×1.9, fallbacks).",
        "Open results: if bodies touch along an edge, the union in SMLib is not a closed body; <code>cad</code> "
        "then reports volume 0 (winkel_anker; the STL is fine). knauf_mirror: the rotated spline body minus the "
        "hole is open, torus and pin are missing from the result, the fillet at the hole fails (SM_ERR) – all as "
        "before this round, still open.",
        "Tessellation of freeform surfaces: the refinement sets a global maximum edge length, so planar faces of "
        f"the same body also become finer (12_linear_extrude_twist: {num(tri12)} triangles, OpenSCAD 452).",
        "Twist extrusion is a cubic loft approximation through sections every 7.5°. Circular profiles with "
        "non-uniform <code>scale</code> and <code>scale = 0</code> (apex) are not supported.",
        "OpenSCAD facets, we compute exactly: volumes of round parts deviate systematically by 0.01–0.3 %; with "
        "twist, OpenSCAD itself is off by ~1 % (section 4).",
        "Determinism: <code>rand()</code> is replaced with a fixed, process-wide implementation. Memory errors in "
        "SMLib make results depend on the heap: the shell pre-check only covers planar, opposite walls; walls that "
        "barely fit (10 mm cube, t = 4.99) already show invalid reads under valgrind. Only the upstream fixes "
        "(tangency-labs/smlib 93efd6d, af8db37) or solidcore will resolve this.",
        "<code>cad render</code> is a software rasterizer (orthographic, Lambert, 2×2 supersampling) without "
        "shadows or perspective. Per-pixel transparency needs memory proportional to the number of layers.",
        "Not part of this report: browser UI, STEP export, switch to solidcore (M8).",
    ]
    for l in limits:
        a(f"<li>{l}</li>")
    a("</ul>")
    a("<p class='small'>Generated with <code>python3 tools/report/report1.py</code> "
      "(PDF: <code>tools/report/print-pdf.mjs</code>, Chromium from Playwright).</p>")
    a("</body></html>")
    return "\n".join(parts)


def print_pdf():
    script = ROOT / "tools/report/print-pdf.mjs"
    r, _ = run(["node", str(script), str(OUT / "report.html"), str(OUT / "report.pdf")], cwd=ROOT / "tools/report",
               check=False)
    if r.returncode != 0:
        log("PDF failed:\n" + r.stdout[-3000:])
        return False
    log(r.stdout.strip())
    return True


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--reuse", action="store_true", help="use target/report1/data.json, only write HTML/PDF")
    ap.add_argument("--repeat", type=int, default=0, help="repeat the kernel-smlib kernel tests N times per mode")
    ap.add_argument("--rebuild-wasm", action="store_true", help="always run smlib/build-wasm.sh")
    ap.add_argument("--no-pdf", action="store_true")
    args = ap.parse_args()
    if args.reuse:
        data = json.loads((WORK / "data.json").read_text())
    else:
        data = collect(args)
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / "report.html").write_text(html_report(data), encoding="utf-8")
    log("wrote", OUT / "report.html")
    big = [it for it in data["renders"] for k in ("ours", "openscad") if k in it and not it.get(k + "_size_ok", True)]
    if big:
        log("WARNING: PNG > 300 KB:", big)
    if not args.no_pdf:
        print_pdf()


if __name__ == "__main__":
    main()
