#!/usr/bin/env python3
"""Testbericht 1 „Kernel, Sprache, Engine, CLI“ erzeugen.

    python3 tools/report/report1.py                 # alles messen, HTML + Bilder + PDF
    python3 tools/report/report1.py --repeat 30     # zusätzlich Wiederholungsläufe kernel-smlib
    python3 tools/report/report1.py --reuse         # nur HTML/PDF aus target/report1/data.json

Ausgabe: docs/reports/01-kernel-sprache-engine-cli/{report.html, report.pdf, *.png}.
Zwischenstände (Messdaten, Wasm-Paket, Logs): target/report1/.

Voraussetzungen (siehe Bericht, Abschnitt Testumgebung):
  * SMLIB_PREBUILT (Standard: /home/user/controller-cad/smlib/build-native) oder eigener Build,
    third_party/usd-brep (smlib/fetch.sh),
  * emsdk (/opt/tools/emsdk) für smlib/build-wasm.sh, wasm-bindgen-cli in der Cargo.lock-Version,
  * openscad 2021.01 und xvfb-run für die Vergleichsbilder,
  * Node 22 und das npm-Paket playwright in tools/report (``npm i playwright@1.56.1 --no-save``)
    mit Chromium unter PLAYWRIGHT_BROWSERS_PATH für das PDF.
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

# Models with renders: one per examples/spec file, plus typical OpenSCAD models.
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
REPEAT_NOTE = ("Vor den Korrekturen: 4 von 40 Läufen mit 8 Threads fehlgeschlagen (zu dicker shell). Nach der "
               "rand()-Korrektur bei 100 Läufen mit 8 Threads noch je einmal <code>invalid_profiles_fail_cleanly</code> "
               "(Speicherfehler in SMLibs shell) und <code>handles_are_released</code> (paralleler Zähler); beides seither behoben.")

CRATE_ROLE = {
    "cad-syntax": "Lexer, Parser, AST",
    "cad-eval": "Sprachauswertung, Raster, Anker, Griffe",
    "cad-ir": "Geometriebaum, Cache-Schlüssel",
    "cad-kernel": "Kernel-Trait, Mesh, Export",
    "kernel-mock": "Testkernel",
    "kernel-smlib": "SMLib-Fassade (nativ)",
    "cad-engine": "Auswertung, Cache, Provenienz, Zonen",
    "cad-session": "Sitzung, Protokoll",
    "cad-render": "Software-Rasterizer",
    "cad-cli": "CLI",
    "cad-wasm": "Wasm-Einstieg (Worker)",
    "compat-tests": "OpenSCAD-Vergleich Stufe 1",
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
        return f"nicht verfügbar ({ex.__class__.__name__})"


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
        "cpu": f"{cpu}, {os.cpu_count()} Kerne",
        "memory": mem,
        "rustc": first_line(["rustc", "--version"]),
        "cargo": first_line(["cargo", "--version"]),
        "emcc": first_line([emcc, "--version"]),
        "node": first_line(["node", "--version"]),
        "openscad": first_line(["openscad", "--version"]),
        "wasm_bindgen": first_line(["wasm-bindgen", "--version"]),
        "playwright": pw,
        "python": platform.python_version(),
        "smlib": "usd-brep @ c9c5979 (SMLib, SM_API), native vorgebaut: " + os.environ.get("SMLIB_PREBUILT", DEFAULT_PREBUILT),
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
    for label, args in (("seriell (--test-threads=1)", ["--test-threads=1"]),
                        ("parallel (Standard, 4 Threads)", []),
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
                failed_tests += names or ["(Abbruch)"]
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
    for src in sorted((ROOT / "examples/spec").glob("*.scad")):
        name = src.stem
        dst = OUT / f"spec-{name}.png"
        _, t = run([str(CAD), "render", str(src), "--size", size, "-o", str(dst)], check=False)
        out.append({"group": "spec", "name": name, "file": f"examples/spec/{src.name}", "ours": dst.name,
                    "seconds": round(t, 2), "purpose": purpose(src)})
    # highlighted subtracted body (cad-render transparency)
    dst = OUT / "spec-gehaeuse-zeile12.png"
    run([str(CAD), "render", "examples/spec/gehaeuse.scad", "--line", "12", "--size", size, "-o", str(dst)], check=False)
    out.append({"group": "highlight", "name": "gehaeuse --line 12", "file": "examples/spec/gehaeuse.scad",
                "ours": dst.name, "purpose": "Zeile 12: abgezogener Körper rot transparent, Rest gedimmt"})
    for name in COMPAT_RENDERS:
        src = ROOT / "examples/openscad-compat" / f"{name}.scad"
        dst = OUT / f"compat-{name}.png"
        _, t = run([str(CAD), "render", str(src), "--size", size, "-o", str(dst)], check=False)
        item = {"group": "compat", "name": name, "file": f"examples/openscad-compat/{src.name}", "ours": dst.name,
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
    """Text of the `// Zweck:` header comment, including its continuation lines."""
    lines = src.read_text(encoding="utf-8").splitlines()
    for i, line in enumerate(lines[:3]):
        m = re.match(r"//\s*Zweck:\s*(.*)", line)
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


def de(x, nd=1):
    """German number format."""
    if x is None:
        return "–"
    if isinstance(x, int):
        return f"{x:,}".replace(",", " ")
    s = f"{x:,.{nd}f}"
    return s.replace(",", " ").replace(".", ",")


def pct(x, nd=2):
    return "–" if x is None else de(100.0 * x, nd) + " %"


def kib(n):
    return "–" if n is None else de(n / 1024.0, 0) + " KiB"


def mib(n):
    return "–" if n is None else de(n / 1024.0 / 1024.0, 2) + " MiB"


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
    return '<span class="ok">ok</span>' if ok else '<span class="bad">Fehler</span>'


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
    a("<!doctype html><html lang='de'><head><meta charset='utf-8'>")
    a("<meta name='viewport' content='width=device-width, initial-scale=1'>")
    a("<title>Testbericht 1: Kernel, Sprache, Engine, CLI</title>")
    a(f"<style>{CSS}</style></head><body>")
    a("<h1>Testbericht 1: Kernel, Sprache, Engine, CLI</h1>")
    a(f"<p class='sub'>controller-cad · Stand {esc(env['date'])} · Commit {esc(env['commit'])} "
      f"(Branch <code>{esc(env['branch'])}</code>)</p>")
    a("<div class='kpis'>")
    a(f"<div class='kpi'><b>{de(total_pass)}</b><span>Rust-Tests bestanden ({total_fail} Fehler, {total_ign} ignoriert)</span></div>")
    a(f"<div class='kpi'><b>{n_ok}/{n_files}</b><span>OpenSCAD-Modelle innerhalb der Toleranz</span></div>")
    sm = w["sizes"]["smlib.wasm"]
    cw = w["sizes"]["cad_wasm_bg.wasm"]
    a(f"<div class='kpi'><b>{mib((sm['brotli'] or sm['gzip']) + (cw['brotli'] or cw['gzip']))}</b>"
      f"<span>Wasm gesamt, Brotli (SMLib {mib(sm['brotli'])}, Engine {mib(cw['brotli'])})</span></div>")
    st = smoke_t.get("set_source_to_root_mesh_ms")
    a(f"<div class='kpi'><b>{de(st, 0)} ms</b><span>Quelltext → Wurzel-Mesh (Beispiel gehaeuse, Node, kalt)</span></div>")
    a("</div>")

    # ---- scope
    a("<h2>1 Umfang</h2>")
    a("<p>Geprüft werden die vier unteren Schichten der Anwendung (UI nicht Teil dieses Berichts):</p><ul>")
    a("<li><b>Kernel:</b> SMLib/SM_API (NVIDIA usd-brep @ c9c5979) hinter einer C++-Fassade mit flacher C-ABI "
      "(Handles, Fehlercodes, Flächen-Tags für die Provenienz), nativ gelinkt und als Emscripten-Wasm "
      "(<code>smlib.wasm</code>); Rust-Anbindung <code>kernel-smlib</code>. Operationen: Primitive, Transformationen, "
      "Booleans, Fillet/Fase, Extrusion (mit twist/scale), Rotation, Loft, Sweep, NURBS-Fläche, thicken, shell, "
      "Kanten/Flächen-Abfragen, Klassifizieren, Tessellierung, Masseeigenschaften.</li>")
    a("<li><b>Sprache:</b> OpenSCAD-Dialekt (<code>cad-syntax</code>, <code>cad-eval</code>): Variablen, Funktionen, "
      "Module, <code>children()</code>, <code>for</code>/<code>if</code>/<code>let</code>/<code>each</code>, "
      "Listenausdrücke, <code>echo</code>/<code>assert</code>, Modifier, 2D/3D-Primitive, Extrusionen; dazu die "
      "Erweiterungen der Spec: Raster (<code>grid</code>), Anker, benannte Instanzen, <code>box()</code> zwischen "
      "Rasterpunkten, Fillets/Fasen mit Zonen, Loft, Sweep, Griffe.</li>")
    a("<li><b>Engine:</b> <code>cad-engine</code> wertet den Geometriebaum schrittweise mit Cache (inhaltsbasierte "
      "Schlüssel) aus, ordnet Flächen und Kanten Zeilen zu, wählt Kanten über Zonen und exportiert STL/3MF; "
      "<code>cad-session</code> stellt das Protokoll für Worker und CLI.</li>")
    a("<li><b>CLI:</b> <code>cad check | eval | inspect | render | export</code> mit <code>--json</code>; "
      "<code>render</code> nutzt den Software-Rasterizer <code>cad-render</code>.</li></ul>")

    # ---- environment
    a("<h2>2 Testumgebung</h2><table>")
    rows = [
        ("System", f"{env['os']} (Linux {env['kernel']}), {env['cpu']}, {env['memory']} RAM, Container ohne GPU"),
        ("Rust", f"{env['rustc']}; {env['cargo']}"),
        ("Kernel", env["smlib"]),
        ("Emscripten", env["emcc"]),
        ("wasm-bindgen", env["wasm_bindgen"]),
        ("Node", env["node"]),
        ("OpenSCAD (Referenz)", env["openscad"] + "; Bilder mit xvfb-run, orthografisch"),
        ("PDF", f"Chromium aus Playwright {env['playwright']}"),
        ("Python", env["python"]),
    ]
    for k, v in rows:
        a(f"<tr><th style='width:22%'>{esc(k)}</th><td>{esc(v)}</td></tr>")
    a("</table>")
    a(f"<p class='small'>Release-Build der CLI: {de(d['release_build_seconds'])} s (inkrementell). Alle Zahlen stammen "
      "aus einem Lauf von <code>tools/report/report1.py</code>; Zeiten sind Wanduhrzeiten auf dieser Maschine.</p>")

    # ---- tests
    a("<h2>3 Testergebnisse</h2>")
    a("<h3>3.1 Rust-Tests pro Crate</h3>")
    a("<table><tr><th>Crate</th><th>Inhalt</th><th class='n'>bestanden</th><th class='n'>Fehler</th>"
      "<th class='n'>ignoriert</th><th class='n'>Testzeit</th><th class='n'>Laufzeit cargo</th><th>Status</th></tr>")
    for t in tests:
        a(f"<tr><td><code>{esc(t['crate'])}</code></td><td>{esc(CRATE_ROLE.get(t['crate'], ''))}</td>"
          f"<td class='n'>{t['passed']}</td><td class='n'>{t['failed']}</td><td class='n'>{t['ignored']}</td>"
          f"<td class='n'>{de(t['test_seconds'], 2)} s</td><td class='n'>{de(t['wall_seconds'], 1)} s</td>"
          f"<td>{status_cell(t['ok'])}</td></tr>")
    a(f"<tr><th>Summe</th><th></th><th class='n'>{total_pass}</th><th class='n'>{total_fail}</th>"
      f"<th class='n'>{total_ign}</th><th class='n'>{de(total_time, 2)} s</th>"
      f"<th class='n'>{de(sum(t['wall_seconds'] for t in tests), 1)} s</th><th></th></tr></table>")
    a("<p class='small'>Testzeit: Summe der von den Test-Binaries gemeldeten Zeiten (Debug-Build, SMLib und Fassade "
      "optimiert). Laufzeit cargo: <code>cargo test -p &lt;crate&gt;</code> nach vorherigem Build.</p>")
    rep = d.get("repeat")
    if rep:
        a("<h3>3.2 Wiederholungsläufe <code>kernel-smlib/tests/kernel.rs</code></h3><table>"
          "<tr><th>Modus</th><th class='n'>Läufe</th><th class='n'>fehlgeschlagen</th><th>Test</th>"
          "<th class='n'>Dauer</th></tr>")
        for k, v in rep.items():
            ft = ", ".join(f"<code>{esc(t)}</code>" for t in v.get("failed_tests", [])) or "–"
            a(f"<tr><td>{esc(k)}</td><td class='n'>{v['runs']}</td><td class='n'>{v['failed']}</td>"
              f"<td>{ft}</td><td class='n'>{de(v['seconds'], 0)} s</td></tr>")
        a("</table>")
        if REPEAT_NOTE:
            a(f"<p class='small'>{REPEAT_NOTE}</p>")
    a("<h3>3.3 Kernel als Wasm (Node)</h3>")
    nt = w.get("node_tests") or []
    a(f"<p><code>node crates/kernel-smlib/js/test-node.mjs</code>: {sum(1 for x in nt if x[0] == 'ok')} von {len(nt)} "
      f"Tests bestanden ({'ok' if w.get('node_test_ok') else 'Fehler'}).</p>")
    a("<table><tr><th>Test</th><th class='n'>Zeit</th><th>Status</th></tr>")
    for s, name, ms in nt:
        a(f"<tr><td>{esc(name)}</td><td class='n'>{de(float(ms), 1)} ms</td><td>{status_cell(s == 'ok')}</td></tr>")
    a("</table>")

    # ---- fixed defects
    a("<h3>3.4 In diesem Durchgang behobene Fehler</h3><table>"
      "<tr><th style='width:20%'>Fehler</th><th style='width:40%'>Ursache</th><th style='width:40%'>Behebung, Nachweis</th></tr>")
    fixes = [
        ("Union dreier orthogonaler Zylinder: 3237 statt 3800 mm³ (05_nested_booleans)",
         "SMLib meldet bei (Z ∪ Y) ∪ X Erfolg, verwirft aber den −x-Arm des X-Zylinders (Hülle x ≥ −4); "
         "(Z ∪ X) ∪ Y ist richtig, (Y ∪ X) ∪ Z endet mit SM_ERR. Nur bei gleichen Radien (Schnittkurven mit "
         "Berührpunkten); mit r = 3,9 oder 4,1 für den dritten Zylinder stimmt es. <code>mass_props</code> und "
         "Mesh sind gleich falsch: kein Tessellierungs- oder Exportfehler. Paarweise rechnete die Fassade schon.",
         "Die Fassade prüft jedes paarweise Ergebnis (Volumen-Ungleichungen, Lage von Proben je 5 pro Fläche "
         "dicht innen und außen an den Operandenflächen) und versucht sonst: Operanden tauschen, andere "
         "Reihenfolge, zuletzt zweiten Operanden um 10⁻⁶…10⁻⁴ der Modellgröße verschieben; besteht nichts, "
         "bleibt SMLibs Ergebnis wie bisher. "
         "Tests: <code>kernel.rs</code> und <code>engine.rs::union_of_three_orthogonal_cylinders</code> "
         "(alle 6 Reihenfolgen, verschachtelt, als Bohrung), Node-Test."),
        ("linear_extrude mit twist: Hülle 1,25 mm daneben (12_linear_extrude_twist)",
         "Schnitt t wurde erst skaliert, dann gedreht; OpenSCAD dreht erst und skaliert dann entlang x/y "
         "(<code>Scaling(s)·Rotation2D(−twist·t)</code>). Bei <code>scale = [1, 0.5]</code> entstehen andere "
         "Querschnitte (y bis 5,08 statt 3,83 mm). Drehsinn und Startwinkel waren richtig. Zusätzlich: Schnitte "
         "alle 22,5° (Hüllenfehler 3·10⁻³ mm), und das Mesh lag 2,9 % über dem Volumen, weil SMLib die "
         "Sehnenhöhe nur an Dreieckskanten begrenzt.",
         "Reihenfolge wie OpenSCAD; Schnitte alle 7,5° (Hülle auf 10⁻⁴ mm, Volumen 540,000 von 540). Die "
         "Tessellierung misst auf Freiformflächen die Abweichung der Dreiecksmitten und verfeinert bis ≤ 2·tol "
         "(höchstens 3 Durchgänge, Dreiecksbudget). Tests: <code>twisted_extrude_with_nonuniform_scale_matches_openscad</code>, "
         "<code>twisted_extrude_mesh_volume</code>, <code>engine.rs::linear_extrude_twist_nonuniform_scale_like_openscad</code>."),
        ("Wackliger Test <code>invalid_profiles_fail_cleanly</code>",
         "Zwei Ursachen, beide im zu dicken <code>shell</code> (Würfel 10 mm, Wand 6 mm, muss scheitern). "
         "(1) SMLib ruft <code>srand(time(NULL))</code> in <code>SmFace::GetPointsInFace</code> und "
         "<code>SmShell::IsInnerShellOf</code> auf: vorher 4 von 40 Läufen (8 Threads) fehlgeschlagen, mit fester "
         "Uhrzeit (<code>time()</code> per LD_PRELOAD) 0 von 40. Ein <code>srand</code> vor jeder Operation "
         "reicht nicht, SMLib setzt den Startwert mitten in der Operation neu. (2) Danach noch 1 von 100: "
         "SMLibs Shell liest bei zu dicker Wand freigegebenen Speicher (valgrind: <i>Invalid read</i> in "
         "<code>SmProtoEdge::RemoveProtoVertex</code>; solidcore UB-239/242); je nach Heap scheitert sie, "
         "liefert einen Körper oder stürzt ab (Segfault in einer Schleife nach 9 Aufrufen).",
         "(1) Die Fassade definiert <code>rand()</code>/<code>srand()</code> selbst (statisch gelinkt, nativ und "
         "Emscripten): fester Startwert, Neustart bei jedem <code>sc_*</code>-Aufruf. (2) <code>sc_shell</code> "
         "lehnt Wände ab, die zwischen zwei gegenüberliegenden ebenen Flächen keinen Platz haben (Abstand ≤ 2·t), "
         "bevor SMLib gerufen wird. Dazu lief <code>handles_are_released</code> parallel falsch (prozessweiter "
         "Zähler): eigenes Test-Binary. Abschnitt 3.2; Tests <code>determinism.rs</code>, <code>handles.rs</code>."),
        ("Diagonalbänder bei transparent gerenderten abgezogenen Körpern",
         "<code>cad-render</code> sortierte transparente Dreiecke nach dem Schwerpunkt. Wo Dreiecke in der Tiefe "
         "überlappen, vor allem bei koplanaren Flächen (abgezogener Körper und sein Hohlraum im gedimmten "
         "Ergebnis), wechselt die Reihenfolge entlang der Dreiecksdiagonalen.",
         "Fragmente werden pro Abtastpunkt nach Tiefe sortiert, koplanare in Szenenreihenfolge (Abb. "
         "<i>gehaeuse --line 12</i> in Abschnitt 5). Test: "
         "<code>render.rs::transparent_coplanar_faces_have_no_diagonal_bands</code>."),
    ]
    for f, c, r in fixes:
        a(f"<tr><td>{f}</td><td>{c}</td><td>{r}</td></tr>")
    a("</table>")

    # ---- compare table
    a("<h2>4 Vergleich mit OpenSCAD</h2>")
    a(f"<p><code>tests/openscad-compat/compare.py</code> exportiert jedes Modell als STL "
      f"(<code>cad export</code>, Sehnentoleranz 0,01 mm) und vergleicht Volumen, Fläche und Hülle mit OpenSCAD 2021.01 "
      f"(<code>$fn = 128</code> bei Rundungen, siehe <code>tests/openscad-compat/README.md</code>). Toleranzen: Hülle "
      f"0,01 mm; Volumen und Fläche 1 % (<i>polygonal</i>) bzw. 2 % (<i>rounded</i>). Ergebnis: "
      f"<b>{n_ok} von {n_files}</b> innerhalb der Toleranz, Laufzeit {de(comp['seconds'])} s.</p>")
    a("<table><tr><th>Modell</th><th>Klasse</th><th class='n'>V OpenSCAD [mm³]</th><th class='n'>V unseres [mm³]</th>"
      "<th class='n'>ΔV</th><th class='n'>ΔA</th><th class='n'>ΔHülle [mm]</th><th class='n'>Dreiecke</th>"
      "<th class='n'>Zeit</th><th>Status</th></tr>")
    for r in comp["results"]:
        dv = r.get("deviation", {})
        ours = r.get("ours") or {}
        refv = r.get("reference") or {}
        a(f"<tr><td><code>{esc(r['file'].removesuffix('.scad'))}</code></td><td>{esc(r.get('class', ''))}</td>"
          f"<td class='n'>{de(refv.get('volume'), 1)}</td><td class='n'>{de(ours.get('volume'), 1)}</td>"
          f"<td class='n'>{pct(dv.get('volume_rel'))}</td><td class='n'>{pct(dv.get('area_rel'))}</td>"
          f"<td class='n'>{de(dv.get('bounds_abs'), 4)}</td><td class='n'>{de(ours.get('triangles'))}</td>"
          f"<td class='n'>{de(r.get('seconds'), 2)} s</td><td>{status_cell(r.get('status') == 'ok')}</td></tr>")
    a("</table>")
    r12 = next((r for r in comp["results"] if r["file"].startswith("12_")), None)
    if r12 and r12.get("ours"):
        a("<div class='note'><b>Zu 12_linear_extrude_twist:</b> Hier weicht OpenSCADs eigenes Netz vom exakten "
          "Körper ab. Exakt sind 1280 + 840 + 540 = 2660 mm³ (verdrillte Querschnitte behalten ihre Fläche); "
          f"OpenSCAD liefert {de(r12['reference']['volume'], 1)} mm³, weil es jede verdrillte Seitenfläche in zwei "
          "ebene Dreiecke mit fester Diagonale teilt. Unser STL hat "
          f"{de(r12['ours']['volume'], 1)} mm³. Die Abweichung von {pct(r12['deviation']['volume_rel'])} in der "
          "Tabelle ist also OpenSCADs Facettierung.</div>")

    # ---- renders
    a("<h2 class='pagebreak'>5 Renderings</h2>")
    a(f"<p>Alle Bilder {IMG[0]}×{IMG[1]} px, isometrische Ansicht (Blick entlang (−1, 1, −1), orthografisch), "
      "<code>cad render &lt;datei&gt; --size 600x450</code>. OpenSCAD-Bilder: <code>openscad -o x.png "
      "--render --imgsize=600,450 --viewall --autocenter --projection=ortho --camera=100,-100,100,0,0,0</code> mit denselben "
      "<code>-D</code>-Werten wie die Referenz.</p>")
    a("<h3>5.1 Beispiele der Spec (<code>examples/spec/</code>)</h3><p class='small'>Nur mit unserer CLI: "
      "Raster, Anker, Fillets und Zonen gibt es in OpenSCAD nicht.</p><div class='grid2'>")
    for it in d["renders"]:
        if it["group"] in ("spec", "highlight"):
            cap = f"<b>{esc(it['name'])}</b> – {esc(it.get('purpose', ''))}"
            a(f"<figure><img src='{esc(it['ours'])}' alt='{esc(it['name'])}'><figcaption>{cap}</figcaption></figure>")
    a("</div>")
    a("<h3>5.2 OpenSCAD-Modelle: unseres (links) und OpenSCAD (rechts)</h3>")
    for it in d["renders"]:
        if it["group"] != "compat":
            continue
        a(f"<div class='pair'><h3><code>{esc(it['name'])}</code> <span class='small'>{esc(it.get('purpose', ''))}</span></h3>"
          "<div class='grid2'>")
        a(f"<figure><img src='{esc(it['ours'])}' alt='{esc(it['name'])} cad render'><figcaption>cad render</figcaption></figure>")
        if it.get("openscad"):
            a(f"<figure><img src='{esc(it['openscad'])}' alt='{esc(it['name'])} OpenSCAD'><figcaption>OpenSCAD 2021.01</figcaption></figure>")
        else:
            a("<figure><figcaption>OpenSCAD-Bild nicht verfügbar</figcaption></figure>")
        a("</div></div>")

    # ---- measurements
    a("<h2>6 Messwerte</h2>")
    a("<h3>6.1 Wasm-Größen</h3><table><tr><th>Datei</th><th class='n'>roh</th><th class='n'>gzip −9</th>"
      "<th class='n'>Brotli q11</th></tr>")
    for name, s in w["sizes"].items():
        a(f"<tr><td><code>{esc(name)}</code></td><td class='n'>{kib(s['raw'])}</td><td class='n'>{kib(s['gzip'])}</td>"
          f"<td class='n'>{kib(s['brotli'])}</td></tr>")
    a("</table><p class='small'><code>smlib.wasm</code>: SMLib + SM_API + Fassade, Emscripten <code>-O3 -fwasm-exceptions</code>; "
      "<code>cad_wasm_bg.wasm</code>: Sprache, Engine, Sitzung (wasm-bindgen, wasm-opt −O3).</p>")
    a("<h3>6.2 Ladezeit und Auswertung (Node 22, ein Prozess wie im Worker)</h3>")
    labels = {
        "load_smlib_ms": "smlib.wasm laden, kompilieren, initialisieren",
        "load_cad_wasm_ms": "cad_wasm laden und initialisieren",
        "set_source_to_preliminary_scene_ms": "SetSource → vorläufige Szene (ohne Kernel)",
        "set_source_to_scene_ms": "SetSource → vollständige Szene",
        "set_source_to_root_mesh_ms": "SetSource → Wurzel-Mesh",
        "request_all_meshes_ms": "Meshes aller Knoten anfordern",
        "edit_to_scene_ms": "Griff ziehen → TextEdit → neue Szene",
        "edit_to_coarse_root_mesh_ms": "… → grobes Wurzel-Mesh",
        "warm_set_source_to_preliminary_scene_ms": "erneutes SetSource (Cache warm) → vorläufige Szene",
        "warm_set_source_to_scene_ms": "erneutes SetSource (Cache warm) → Szene",
        "warm_set_source_to_root_mesh_ms": "erneutes SetSource (Cache warm) → Wurzel-Mesh",
    }
    if smoke_t:
        a(f"<p><code>web/scripts/smoke-node.mjs --json</code> mit dem Beispiel <code>gehaeuse</code>, "
          f"Median aus {len(smoke)} Läufen, Schrittbudget 16 ms.</p>")
        a("<table><tr><th>Schritt</th><th class='n'>Zeit</th></tr>")
        for k, v in smoke_t.items():
            if k == "steps":
                continue
            a(f"<tr><td>{esc(labels.get(k, k))}</td><td class='n'>{de(v, 1)} ms</td></tr>")
        if "steps" in smoke_t:
            a(f"<tr><td>Auswertschritte bis zur vollständigen Szene</td><td class='n'>{de(int(smoke_t['steps']))}</td></tr>")
        a(f"<tr><td>Dreiecke Wurzel-Mesh / Knoten der Szene</td><td class='n'>{de(smoke[0].get('triangles_root'))} / {de(smoke[0].get('nodes'))}</td></tr>")
        a("</table>")
    elif w.get("smoke_error"):
        a(f"<p class='bad'>smoke-node fehlgeschlagen.</p><pre>{esc(w['smoke_error'])}</pre>")
    if "smlib_init_ms" in w:
        a(f"<p>Nur Kernel (<code>test-node.mjs</code>): Datei lesen {de(w['smlib_read_ms'])} ms, kompilieren, "
          f"instanziieren und initialisieren {de(w['smlib_init_ms'])} ms. Szenario: {esc(w.get('smlib_scenario', ''))}.</p>")
    a("<h3>6.3 Auswertung nativ (CLI)</h3>")
    secs = [r.get("seconds") for r in comp["results"] if r.get("seconds") is not None]
    if secs:
        secs_sorted = sorted(secs)
        slow = sorted(comp["results"], key=lambda r: -(r.get("seconds") or 0))[:3]
        a(f"<p><code>cad export</code> je Modell (Prozessstart, Auswertung, Tessellierung, STL): Median "
          f"{de(secs_sorted[len(secs) // 2], 2)} s, Summe {de(sum(secs), 1)} s; am längsten "
          + ", ".join(f"<code>{esc(r['file'].removesuffix('.scad'))}</code> {de(r['seconds'], 2)} s" for r in slow)
          + ".</p>")
    spec = [it for it in d["renders"] if it["group"] == "spec"]
    if spec:
        a("<p><code>cad render</code> der Spec-Beispiele (600×450, 2×2-Überabtastung): "
          + ", ".join(f"{esc(it['name'])} {de(it['seconds'], 2)} s" for it in spec) + ".</p>")

    # ---- limits
    a("<h2>7 Bekannte Grenzen</h2><ul>")
    tri12 = (r12 or {}).get("ours", {}).get("triangles")
    limits = [
        "SMLib-Booleans sind in Sonderlagen unzuverlässig. Die Fassade erkennt falsche Ergebnisse über "
        "Stichproben, nicht vollständig: Ein Fehler zwischen allen Probepunkten bleibt unentdeckt. Die Prüfung "
        "nutzt SMLibs Punktklassifizierung, die bei gedrehten Splinekörpern selbst irrt (knauf_mirror); besteht "
        "keine Variante, bleibt deshalb SMLibs eigenes Ergebnis. Im Verschiebe-Rückfall weicht der zweite "
        "Operand um bis zu 10⁻⁴ der Modellgröße ab. Die Prüfung kostet Zeit: "
        "<code>cad export</code> aller 41 Beispiele beim Beheben gemessen 13,7 s → 18,9 s (+38 %), am meisten "
        "bei Vereinigungen vieler Teile (07_crown_ring ×2,1) und bei knauf_mirror (×1,9, Rückfälle).",
        "Offene Ergebnisse: Berühren sich Körper an einer Kante, ist die Vereinigung in SMLib kein geschlossener "
        "Körper; <code>cad</code> meldet dann Volumen 0 (winkel_anker; das STL ist in Ordnung). knauf_mirror: "
        "Der gedrehte Splinekörper minus Bohrung ist offen, Torus und Stift fehlen im Ergebnis, das Fillet an "
        "der Bohrung scheitert (SM_ERR) – alles wie vor diesem Durchgang, noch offen.",
        "Tessellierung von Freiformflächen: Die Verfeinerung setzt eine globale maximale Kantenlänge, damit werden "
        f"auch ebene Flächen desselben Körpers feiner (12_linear_extrude_twist: {de(tri12)} Dreiecke, OpenSCAD 452).",
        "Twist-Extrusion ist eine kubische Loft-Annäherung durch Schnitte alle 7,5°. Kreisprofile mit "
        "ungleichmäßigem <code>scale</code> und <code>scale = 0</code> (Spitze) werden nicht unterstützt.",
        "OpenSCAD facettiert, wir rechnen exakt: Volumen runder Teile weichen systematisch um 0,01–0,3 % ab; bei "
        "twist liegt OpenSCAD selbst um ~1 % daneben (Abschnitt 4).",
        "Determinismus: <code>rand()</code> ist fest und prozessweit ersetzt. Speicherfehler in SMLib machen "
        "Ergebnisse vom Heap abhängig: Die Shell-Vorprüfung deckt nur ebene, gegenüberliegende Wände ab; schon "
        "knapp passende Wände (10-mm-Würfel, t = 4,99) zeigen unter valgrind ungültige Lesezugriffe. Abhilfe "
        "bringen erst die Upstream-Korrekturen (tangency-labs/smlib 93efd6d, af8db37) bzw. solidcore.",
        "<code>cad render</code> ist ein Software-Rasterizer (orthografisch, Lambert, 2×2-Überabtastung) ohne "
        "Schatten und Perspektive. Transparenz pro Pixel braucht Speicher proportional zur Zahl der Schichten.",
        "Nicht Teil dieses Berichts: Browser-UI, STEP-Export, Umstieg auf solidcore (M8).",
    ]
    for l in limits:
        a(f"<li>{l}</li>")
    a("</ul>")
    a("<p class='small'>Erzeugt mit <code>python3 tools/report/report1.py</code> "
      "(PDF: <code>tools/report/print-pdf.mjs</code>, Chromium aus Playwright).</p>")
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
