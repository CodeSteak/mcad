#!/usr/bin/env python3
"""Generate test report 5 "STEP export and kernel comparison".

    python3 tools/report/report5.py --run     # measure everything (tests, verify.py, diff-kernels, images), HTML + PDF
    python3 tools/report/report5.py           # only HTML/PDF from target/step/ (previous --run)

Output: docs/reports/05-step-export/{report.html, report.pdf, img/*.png}.
Intermediate state: target/step/ (verify-report.json, diff-kernels.json, report5-tests.json, readback/).

Requirements:
  * SMLIB_PREBUILT (default /home/user/controller-cad/smlib/build-native) or an own SMLib build,
    third_party/smlib (smlib/fetch.sh), emsdk for smlib/build-wasm.sh (Node test),
  * a venv with cadquery-ocp (OCCT Python), default /opt/tools/venv-step (STEP_PY),
  * Node 22 and playwright in tools/report (``npm i playwright@1.56.1 --no-save``) for the PDF.
"""

from __future__ import annotations

import argparse
import datetime as dt
import html
import json
import os
import platform
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "docs" / "reports" / "05-step-export"
IMG = OUT / "img"
WORK = ROOT / "target" / "step"
CAD = ROOT / "target" / "release" / "cad"
STEP_PY = os.environ.get("STEP_PY", "/opt/tools/venv-step/bin/python")
RENDERS = [
    ("14_pipe_bend", "pipe bend: surfaces of revolution and cylinders"),
    ("22_gear", "gear: 82 planes, 5 cylinders"),
    ("dose_shell", "can (shell, fillets): cylinders and spheres"),
    ("freiformdeckel", "freeform lid: B-spline surfaces"),
    ("knauf_mirror", "knob (mirrored): revolution, B-spline"),
    ("flasche", "bottle: surfaces of revolution, cone"),
]
TITLE = "Test report 5 – STEP export and kernel comparison"
# Environment keys of target/step/report5-tests.json written by older (German) runs.
LEGACY_ENV_KEYS = {"Datum": "Date", "Betriebssystem": "Operating system", "STEP-Leser": "STEP reader"}


def sh(cmd, env: dict | None = None) -> subprocess.CompletedProcess:
    env = dict(env or os.environ)
    env.setdefault("SMLIB_PREBUILT", "/home/user/controller-cad/smlib/build-native")
    print("$", " ".join(map(str, cmd)), flush=True)
    return subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True, env=env)


def run_all() -> None:
    WORK.mkdir(parents=True, exist_ok=True)
    IMG.mkdir(parents=True, exist_ok=True)
    tests: dict = {}
    r = sh(["cargo", "build", "--release", "-p", "cad-cli", "--bins", "--examples"])
    if r.returncode:
        sys.exit(r.stderr)
    r = sh(["cargo", "test", "--workspace"])
    passed = sum(int(m) for m in re.findall(r"test result: \w+\. (\d+) passed", r.stdout))
    failed = sum(int(m) for m in re.findall(r"(\d+) failed;", r.stdout))
    tests["cargo test --workspace"] = {"ok": r.returncode == 0, "text": f"{passed} passed, {failed} failed"}
    r = sh(["cargo", "clippy", "--workspace", "--all-targets"])
    warns = re.findall(r"^warning: (?!`)(.*)\n\s*--> ([^:]+)", r.stderr, re.M)
    files = sorted({f for _, f in warns})
    tests["cargo clippy --workspace --all-targets"] = {
        "ok": r.returncode == 0,
        "text": f"{len(warns)} warning{'' if len(warns) == 1 else 's'}" + (f" in {', '.join(files)}" if files else ""),
    }
    r = sh(["node", "crates/kernel-smlib/js/test-node.mjs"])
    last = (r.stdout.strip().splitlines() or [""])[-1]
    n_ok = r.stdout.count("\nok ")
    tests["node crates/kernel-smlib/js/test-node.mjs"] = {"ok": r.returncode == 0, "text": f"{n_ok} tests, {last}"}
    r = sh(["python3", "tests/openscad-compat/compare.py"], env={**os.environ, "CAD": str(CAD)})
    m = re.search(r"(\d+/\d+) files match", r.stdout)
    tests["tests/openscad-compat/compare.py"] = {"ok": r.returncode == 0, "text": (m.group(0) if m else r.stdout[-200:])}
    r = sh([STEP_PY, "tests/step/verify.py", "--cad", str(CAD), "--out", str(WORK), "--stl-dir", str(WORK / "readback")])
    m = re.search(r"(\d+/\d+) passed", r.stdout)
    tests["tests/step/verify.py"] = {"ok": r.returncode == 0, "text": (m.group(0) if m else r.stdout[-200:])}
    models = sorted((ROOT / "tests" / "models" / "openscad-compat").glob("*.scad")) + sorted((ROOT / "tests" / "models" / "spec").glob("*.scad"))
    r = sh([str(CAD), "diff-kernels", *map(str, models), "--json"])
    (WORK / "diff-kernels.json").write_text(r.stdout)
    tests["cad diff-kernels (all examples)"] = {"ok": r.returncode in (0, 1), "text": f"Exit {r.returncode} (1 = not all nodes equal)"}
    for stem, _ in RENDERS:
        sh([str(ROOT / "target" / "release" / "examples" / "render_stl"), str(WORK / "readback" / f"{stem}.stl"),
            str(IMG / f"readback-{stem}.png"), str(WORK / "readback" / f"{stem}.edges.json"), "iso"])
    env = {}
    env["Date"] = dt.datetime.now().strftime("%Y-%m-%d %H:%M")
    env["Commit"] = sh(["git", "rev-parse", "--short", "HEAD"]).stdout.strip() + " (" + sh(["git", "rev-parse", "--abbrev-ref", "HEAD"]).stdout.strip() + ")"
    env["Operating system"] = f"{platform.system()} {platform.release()}, {os.cpu_count()} cores"
    env["Rust"] = sh(["rustc", "--version"]).stdout.strip()
    env["C++"] = (sh(["g++", "--version"]).stdout.splitlines() or [""])[0]
    env["SMLib"] = "tangency-labs/smlib capi-tess @ 7eba8aa, facade version 3 (sc_brep)"
    env["solidcore"] = "tangency-labs/solidcore main @ 3a6a50b (git dependency, feature `solidcore`)"
    occ = sh([STEP_PY, "-c", "import importlib.metadata as m, sys; print(m.version('cadquery-ocp'), sys.version.split()[0])"]).stdout.split()
    env["STEP reader"] = f"OpenCASCADE via cadquery-ocp {occ[0] if occ else '?'} (Python {occ[1] if len(occ) > 1 else '?'}, venv {Path(STEP_PY).parent.parent})"
    env["Node"] = sh(["node", "--version"]).stdout.strip()
    (WORK / "report5-tests.json").write_text(json.dumps({"tests": tests, "env": env}, indent=1))


CSS = (Path(__file__).with_name("report1.py").read_text().split('CSS = """', 1)[1].split('"""', 1)[0])


def esc(x) -> str:
    return html.escape(str(x))


def fmt_e(x) -> str:
    return "–" if x is None else f"{x:.1e}"


def build_html() -> str:
    ver = json.loads((WORK / "verify-report.json").read_text())
    dk = json.loads((WORK / "diff-kernels.json").read_text())
    meta = json.loads((WORK / "report5-tests.json").read_text())
    meta["env"] = {LEGACY_ENV_KEYS.get(k, k): v for k, v in meta["env"].items()}
    res = ver["results"]
    n_ok = sum(r["ok"] for r in res)
    surf: dict[str, int] = {}
    curv: dict[str, int] = {}
    for r in res:
        for k, v in (r.get("step_surfaces") or {}).items():
            surf[k] = surf.get(k, 0) + v
        for k, v in (r.get("step_curves") or {}).items():
            curv[k] = curv.get(k, 0) + v
    max_dv = max(r.get("volume_rel", 0) or 0 for r in res)
    max_da = max(r.get("area_rel", 0) or 0 for r in res)
    max_db = max(r.get("bounds_rel", 0) or 0 for r in res)
    approx = sum(r.get("approximated", 0) for r in res)
    s = dk["summary"]
    out: list[str] = []
    a = out.append
    a(f"<!doctype html><html lang='en'><head><meta charset='utf-8'><title>{esc(TITLE)}</title>")
    a(f"<style>{CSS}</style></head><body>")
    a(f"<h1>{esc(TITLE)}</h1>")
    a(f"<div class='sub'>{esc(meta['env']['Date'])} · Commit {esc(meta['env']['Commit'])}</div>")
    a("<div class='kpis'>")
    a(f"<div class='kpi'><b class='{'ok' if n_ok == len(res) else 'bad'}'>{n_ok}/{len(res)}</b><span>Examples: STEP valid, closed, volume/area/bounding box as in the kernel</span></div>")
    a(f"<div class='kpi'><b>{fmt_e(max_dv)}</b><span>largest rel. volume deviation OCCT ↔ SMLib</span></div>")
    a(f"<div class='kpi'><b>{approx}</b><span>approximated faces/edges (everything written exactly)</span></div>")
    a(f"<div class='kpi'><b>{s['same']}/{s['nodes']}</b><span>nodes equal with solidcore (rest unsupported/blocked)</span></div>")
    a("</div>")

    a("<h2>1. Scope</h2><ul>")
    a("<li><b>Facade</b> (<code>smlib/facade</code>, ABI version 2 → 3): the new query <code>sc_brep(h)</code> returns topology "
      "(bodies → shells → faces → loops → oriented edges, vertices) and exact geometry: every face and edge as "
      "NURBS (degree, knots, weights, control points), analytic surfaces (plane, cylinder, cone, sphere, torus, surface of "
      "revolution, extrusion surface) additionally with their parameters. Non-B-spline geometry (blends, offsets) would be "
      "approximated to the model tolerance and flagged. Native side (FFI) and Wasm bridge (<code>smlib-bridge.js</code>, "
      "<code>exports.txt</code>).</li>")
    a("<li><b>Neutral B-rep</b> <code>cad_kernel::brep</code> with <code>Kernel::brep</code> (default: Unsupported). "
      "<code>kernel-smlib</code> checks the analytic parameters against the NURBS (9×9 samples), trims edge curves "
      "to their interval (knot insertion), detects exact lines and circles and determines <code>same_sense</code> from "
      "SMLib's normal at an interior face point.</li>")
    a("<li><b>Writer</b> <code>cad-step</code> (kernel-independent): AP242 (default) or AP214, product structure "
      "(<code>PRODUCT</code> … <code>SHAPE_DEFINITION_REPRESENTATION</code>), units mm and degrees, "
      "<code>MANIFOLD_SOLID_BREP</code>/<code>BREP_WITH_VOIDS</code> per body (several shells → several solids), open "
      "surfaces as <code>SHELL_BASED_SURFACE_MODEL</code>, colors from <code>color()</code> as <code>STYLED_ITEM</code> per face.</li>")
    a("<li><b>Integration</b>: <code>Engine::export_step</code>, <code>cad export -o x.step [--schema ap214]</code> (JSON with "
      "volume, area, bounding box and statistics), session <code>Export{format:\"step\"}</code> (additive, <code>protocol.ts</code> regenerated), Wasm.</li>")
    a("<li><b>Verification</b> <code>tests/step/verify.py</code>: reads every file with OpenCASCADE (not our code), "
      "<code>BRepCheck_Analyzer</code>, closed shells, number of solids/faces, volume and bounding box against the kernel "
      f"(relative {ver['tolerance_rel']:g}), area ({ver.get('area_tolerance_rel', 1e-4):g}, see section 7). Rust tests of the writer "
      "(syntax, references, numbers read back with our own Part 21 reader) run without Python.</li>")
    a("<li><b>solidcore</b>: crate <code>kernel-solidcore</code>, CLI <code>cad diff-kernels</code>, documentation <code>docs/solidcore-migration.md</code>.</li></ul>")
    a("<p><b>Why AP242:</b> AP242 is the current edition that merges and replaces AP203 and AP214; the B-rep entities "
      "(Part 42) are the same in both, the choice only changes the header and the application context. AP214 remains available via "
      "<code>--schema ap214</code> for older readers that do not know the AP242 schema name.</p>")

    a("<h2>2. Environment</h2><table>")
    for k, v in meta["env"].items():
        a(f"<tr><th style='width:20%'>{esc(k)}</th><td>{esc(v)}</td></tr>")
    a("</table>")

    a("<h2>3. Check results</h2><table>")
    for k, v in meta["tests"].items():
        a(f"<tr><th style='width:42%'><code>{esc(k)}</code></th><td class='{'ok' if v['ok'] else 'bad'}'>{'ok' if v['ok'] else 'FAILED'}</td><td>{esc(v['text'])}</td></tr>")
    a("</table>")

    a("<h2 class='pagebreak'>4. All examples: kernel ↔ STEP file read back</h2>")
    a("<p class='small'>V: volume in mm³ (SMLib <code>mass_props</code> or OCCT, adaptive integration 1e-9). dV, dA: relative deviation "
      "of volume and area; dBox: largest deviation of a bounding box coordinate relative to the diagonal. \"valid\": BRepCheck valid, all "
      "shells closed, number of solids and faces as written. Face types: as written (OCCT reads the same types).</p>")
    a("<table><tr><th>Model</th><th class='n'>Solids</th><th class='n'>Faces</th><th class='n'>V kernel</th><th class='n'>V STEP (OCCT)</th>"
      "<th class='n'>dV</th><th class='n'>dA</th><th class='n'>dBox</th><th>valid</th><th>Face types</th><th>Curve types</th></tr>")
    for r in res:
        c = r.get("checks", {})
        valid = all(c.get(k, False) for k in ("read", "valid", "closed", "solids", "faces"))
        name = Path(r["model"]).stem
        st = ", ".join(f"{k} {v}" for k, v in sorted((r.get("step_surfaces") or {}).items()))
        ct = ", ".join(f"{k} {v}" for k, v in sorted((r.get("step_curves") or {}).items()))
        cls = "ok" if r["ok"] else "bad"
        a(f"<tr><td>{esc(name)}</td><td class='n'>{r.get('occ_solids', '–')}</td><td class='n'>{r.get('occ_faces', '–')}</td>"
          f"<td class='n'>{r.get('kernel_volume') or 0:.3f}</td><td class='n'>{r.get('occ_volume') or 0:.3f}</td>"
          f"<td class='n'>{fmt_e(r.get('volume_rel'))}</td><td class='n'>{fmt_e(r.get('area_rel'))}</td><td class='n'>{fmt_e(r.get('bounds_rel'))}</td>"
          f"<td class='{cls}'>{'yes' if valid else 'no'}</td><td class='small'>{esc(st)}</td><td class='small'>{esc(ct)}</td></tr>")
    a("</table>")
    a(f"<p>Largest deviations: volume {fmt_e(max_dv)}, area {fmt_e(max_da)}, bounding box {fmt_e(max_db)}. "
      "The volume deviations above 1e-7 concern B-spline surfaces and surfaces of revolution, where both sides integrate numerically.</p>")
    a("<h3>Totals over all examples</h3><table><tr><th>Face type (STEP entity)</th><th class='n'>Count</th><th>Curve type</th><th class='n'>Count</th></tr>")
    sn = {"plane": "PLANE", "cylinder": "CYLINDRICAL_SURFACE", "cone": "CONICAL_SURFACE", "sphere": "SPHERICAL_SURFACE",
          "torus": "TOROIDAL_SURFACE", "revolution": "SURFACE_OF_REVOLUTION", "extrusion": "SURFACE_OF_LINEAR_EXTRUSION",
          "bspline": "B_SPLINE_SURFACE_WITH_KNOTS", "nurbs": "… + RATIONAL_B_SPLINE_SURFACE"}
    cn = {"line": "LINE", "circle": "CIRCLE", "ellipse": "ELLIPSE", "bspline": "B_SPLINE_CURVE_WITH_KNOTS", "nurbs": "… + RATIONAL_B_SPLINE_CURVE"}
    sl, cl = sorted(surf.items(), key=lambda x: -x[1]), sorted(curv.items(), key=lambda x: -x[1])
    for i in range(max(len(sl), len(cl))):
        s1 = sl[i] if i < len(sl) else ("", "")
        c1 = cl[i] if i < len(cl) else ("", "")
        a(f"<tr><td>{esc(s1[0])} <span class='small'>{esc(sn.get(s1[0], ''))}</span></td><td class='n'>{s1[1]}</td>"
          f"<td>{esc(c1[0])} <span class='small'>{esc(cn.get(c1[0], ''))}</span></td><td class='n'>{c1[1]}</td></tr>")
    a("</table>")

    a("<h2>5. STEP geometry read back</h2>")
    a("<p class='small'>OpenCASCADE reads the STEP file, meshes it (chord error 0.2 % of the diagonal) and writes STL; the edge lines "
      "are the B-rep edges of the shape read in. Rendered with the CLI's software rasterizer (<code>cad-render</code>, example "
      "<code>render_stl</code>). The images therefore show the geometry as seen by a different kernel.</p><div class='grid2'>")
    for stem, cap in RENDERS:
        if (IMG / f"readback-{stem}.png").exists():
            a(f"<figure><img src='img/readback-{stem}.png' alt='{esc(stem)}'><figcaption><b>{esc(stem)}</b> – {esc(cap)}</figcaption></figure>")
    a("</div>")

    a("<h2>6. Kernel comparison <code>cad diff-kernels</code> (SMLib ↔ solidcore)</h2>")
    a(f"<p>solidcore @ <code>{esc(dk['solidcore_rev'][:7])}</code>, all {len(dk['files'])} examples, {s['nodes']} evaluated nodes: "
      f"<b>{s['same']}</b> equal, <b>{s['unsupported']}</b> unsupported, <b>{s['blocked']}</b> blocked (input unsupported), "
      f"<b>{s['differ']}</b> different, <b>{s['failed']}</b> failed. Only 2D nodes are equal (profiles are computed in Rust without the "
      "kernel). Every primitive aborts in solidcore with <code>Unported: SmSurface::DropCurve (intersect.surface_drop_lib_p3)</code>; "
      "booleans, queries and mass properties can therefore not be checked yet. The comparison runs over all files without crashing.</p>")
    cov = dk["coverage"]
    kernel_ops = ["box", "sphere", "cylinder", "torus", "polyhedron", "linear_extrude", "rotate_extrude", "loft", "sweep", "surface",
                  "difference", "union", "intersection", "translate", "rotate", "fillet", "chamfer", "shell", "zone"]
    a("<table><tr><th>Operation (node)</th><th class='n'>Nodes</th><th class='n'>equal</th><th class='n'>unsupported</th><th class='n'>blocked</th><th class='n'>different</th></tr>")
    for op in kernel_ops + sorted(k for k in cov if k not in kernel_ops and cov[k].get("same", 0) and cov[k].get("nodes", 0) > 3):
        if op not in cov:
            continue
        c = cov[op]
        a(f"<tr><td><code>{esc(op)}</code></td><td class='n'>{c.get('nodes', 0)}</td><td class='n'>{c.get('same', 0)}</td><td class='n'>{c.get('unsupported', 0)}</td>"
          f"<td class='n'>{c.get('blocked', 0)}</td><td class='n'>{c.get('differ', 0) + c.get('failed', 0)}</td></tr>")
    a("</table><p class='small'>Further labels (module calls, groups, 2D primitives) in the JSON <code>target/step/diff-kernels.json</code>. "
      "Acceptance and missing parts: <code>docs/solidcore-migration.md</code>.</p>")

    a("<h2>7. Known limitations</h2><ul>")
    a("<li><b>Area:</b> SMLib integrates areas only to about 1e-4 (<code>SmApiBrepComputeArea</code> clamps the accuracy to [1e-4, 1e-1]). "
      "For <code>freiformdeckel</code> the kernel area deviates by 1.9e-4; OCCT adaptive (6658.641 mm²) and a fine OCCT mesh "
      "(6658.656 mm²) agree with each other, so the error lies in the kernel reference value. The area check therefore uses 5e-4, "
      "volume and bounding box 1e-4.</li>")
    a("<li>OCCT's default integration (fixed Gauss rule) deviates by up to 1e-4 for B-spline surfaces and surfaces of revolution; verify.py integrates adaptively (1e-9).</li>")
    a("<li>No p-curves (2D curves in the surface parameter space): readers project the 3D edges. OCCT does this without errors for all examples; "
      "other systems might read B-spline surfaces with a seam more slowly.</li>")
    a("<li>Edge curves of booleans are B-splines (intersection curves); exact circles/lines are detected (tolerance 1e-9 of the model size), "
      "ellipses only if SMLib holds them as such (none in the examples).</li>")
    a("<li>Surfaces without a B-spline form (blend, offset surfaces) would be approximated; in all examples SMLib supplies exact B-splines or analytic surfaces (0 approximated).</li>")
    a("<li>Voids (<code>BREP_WITH_VOIDS</code>) are checked in the Rust test and on the kernel (sphere minus sphere); none occur in the examples.</li>")
    a("<li>Colors: per face from <code>color()</code> as <code>STYLED_ITEM</code>; OCCT reads them only via XCAF, verify.py does not check them.</li>")
    a("<li>The root is exported in its own coordinate system (like STL/3MF); 2D results are rejected.</li>")
    a("<li>In the browser the session writes a fixed timestamp (<code>1970-01-01T00:00:00</code>), the CLI the current UTC time.</li>")
    a("<li>solidcore: see section 6; <code>kernel-solidcore</code> only covers the API entry points available today.</li></ul>")
    a("</body></html>")
    return "\n".join(out)


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--run", action="store_true", help="measure everything again")
    ap.add_argument("--no-pdf", action="store_true")
    a = ap.parse_args()
    if a.run:
        run_all()
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / "report.html").write_text(build_html())
    print(f"{OUT / 'report.html'}")
    if not a.no_pdf:
        r = sh(["node", "tools/report/print-pdf.mjs", str(OUT / "report.html"), str(OUT / "report.pdf"), TITLE])
        print(r.stdout, r.stderr)


if __name__ == "__main__":
    main()
