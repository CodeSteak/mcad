#!/usr/bin/env node
// Test report 15 ("Sketches: corners, offset, stroke", plan 17.7 step 2): corner() in sketches,
// fillet2d/chamfer2d with zones, offset (OpenSCAD-compatible, exact arcs), stroke of open paths,
// exact 2D regions in the kernel, display and handles in the UI. Writes
// docs/reports/15-skizzen-ecken-offset/report.html (images in img/).
//
// Inputs (gitignored, reports/tmp/report15/):
//   cli/*.png, areas.tsv, openscad.tsv, step.tsv, timings.tsv,
//   check-fehler.txt, inspect-halter.txt                       tools/report/report15-data.sh
//   scenarios.json, img/*.jpg                                   cd web && pnpm exec playwright test e2e/sketch-corners.spec.ts
//   playwright.json                                             whole suite (pnpm e2e; copy of reports/tmp/report2/playwright.json)
//   vitest.txt, typecheck.txt                                   cd web && pnpm test, pnpm typecheck
//   cargo.txt, clippy.txt, fmt.txt                              cargo test --workspace --no-fail-fast, clippy -D warnings, fmt --check
//   compare.txt, step.txt, alltag.txt, node-test.txt            compare.py --eval, verify.py, tests/everyday/check.sh, test-node.mjs
//
//   node tools/report/report15.mjs
//   node tools/report/print-pdf.mjs docs/reports/15-skizzen-ecken-offset/report.html docs/reports/15-skizzen-ecken-offset/report.pdf "Test report 15: Sketches – corners, offset, stroke"

import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import os from "node:os";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "../..");
const input = join(root, "reports/tmp/report15");
const out = join(root, "docs/reports/15-skizzen-ecken-offset");
const imgDir = join(out, "img");
mkdirSync(imgDir, { recursive: true });

const esc = (s) => String(s ?? "").replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
const readText = (f) => (existsSync(f) ? readFileSync(f, "utf8") : null);
const readJson = (f) => (existsSync(f) ? JSON.parse(readFileSync(f, "utf8")) : null);
const git = (...a) => {
  try {
    return execFileSync("git", a, { cwd: root, encoding: "utf8" }).trim();
  } catch {
    return "–";
  }
};
const num = (x, d = 0) => (x === null || x === undefined || Number.isNaN(Number(x)) ? "–" : Number(x).toLocaleString("en-US", { minimumFractionDigits: d, maximumFractionDigits: d }));
const sci = (x) => (x === null || x === undefined || !Number.isFinite(x) ? "–" : x === 0 ? "0" : x.toExponential(1).replace("e", "·10^").replace(/\^(-?)\+?(\d+)/, (_, s, n) => `<sup>${s === "-" ? "−" : ""}${n}</sup>`));
const pct = (x, d = 3) => (x === null || x === undefined || !Number.isFinite(x) ? "–" : `${num(x * 100, d)} %`);
const md = (t) => esc(t).replace(/`([^`]+)`/g, "<code>$1</code>");
const T = (name) => readText(join(input, name))?.replaceAll(`${input}/`, "").replaceAll(`${root}/`, "") ?? "(missing)";
const pre = (t) => `<pre>${esc(t.trimEnd())}</pre>`;
const src = (p) => readText(join(root, p)) ?? "(missing)";

function img(srcPath, dst) {
  if (!existsSync(srcPath)) return null;
  if (srcPath.endsWith(".png") && dst.endsWith(".jpg")) {
    try {
      execFileSync("convert", [srcPath, "-resize", "1440x>", "-quality", "86", join(imgDir, dst)]);
      return `img/${dst}`;
    } catch {
      dst = dst.replace(/\.jpg$/, ".png");
    }
  }
  copyFileSync(srcPath, join(imgDir, dst));
  return `img/${dst}`;
}
const cli = (n) => img(join(input, "cli", `${n}.png`), `cli-${n}.jpg`);
const pw = (n) => img(join(input, "img", `${n}.jpg`), `ui-${n}.jpg`);
const fig = (s, cap) => (s ? `<figure><img src="${s}" alt=""><figcaption>${md(cap)}</figcaption></figure>` : `<p class="small">(image missing: ${esc(cap)})</p>`);
const pair = (a, ca, b, cb) => `<div class="two">${fig(a, ca)}${fig(b, cb)}</div>`;

// ------------------------------------------------------------------------------- test summaries

const cargo = readText(join(input, "cargo.txt"));
const cargoSum = (() => {
  if (!cargo) return null;
  let ok = 0;
  let failed = 0;
  for (const m of cargo.matchAll(/test result: \w+\. (\d+) passed; (\d+) failed/g)) {
    ok += +m[1];
    failed += +m[2];
  }
  return { ok, failed };
})();
const NEW = /region2::|op2d::|biarcs|curve2::|sketch_ops|corner_(rules|errors|radius)|concave_corners|arguments_of_2d|sketch_corners_offset_stroke|spec_models|tree_bounds|openscad_models|ring_of_long|step_has_exact/;
const newTests = cargo ? [...cargo.matchAll(/^test (\S+) \.\.\. (ok|FAILED)$/gm)].map((m) => ({ name: m[1], res: m[2] })).filter((t) => NEW.test(t.name)) : [];
const failedTests = cargo ? [...cargo.matchAll(/^test (\S+) \.\.\. FAILED$/gm)].map((m) => m[1]) : [];
const clippy = readText(join(input, "clippy.txt"));
const fmt = readText(join(input, "fmt.txt"));
const clippyOk = clippy != null && !/^(error|warning)/m.test(clippy);
const fmtOk = fmt != null && fmt.trim() === "";
const playwright = readJson(join(input, "playwright.json"));
const pwStats = (() => {
  if (!playwright) return null;
  const all = [];
  const walk = (s) => {
    for (const sp of s.specs ?? []) for (const t of sp.tests ?? []) all.push({ title: sp.title, file: sp.file, status: t.results?.at(-1)?.status ?? t.status });
    for (const c of s.suites ?? []) walk(c);
  };
  for (const s of playwright.suites ?? []) walk(s);
  return { total: all.length, passed: all.filter((t) => t.status === "passed").length, failed: all.filter((t) => t.status !== "passed" && t.status !== "skipped") };
})();
const vitest = readText(join(input, "vitest.txt"));
const vitestSum = vitest ? (/Tests\s+(\d+) passed/.exec(vitest)?.[1] ?? null) : null;
const compare = readText(join(input, "compare.txt"));
const compareSum = compare ? /(\d+)\/(\d+)/.exec(compare.split("\n").reverse().find((l) => /\d+\/\d+ files/.test(l)) ?? "")?.slice(1, 3) : null;
const compareNew = compare ? compare.split("\n").filter((l) => /^(file|3[456]_|-{10})/.test(l)) : [];
const step = readText(join(input, "step.txt"));
const stepSum = step ? (/\d+\/\d+ passed/.exec(step)?.[0] ?? null) : null;
const stepNew = step ? step.split("\n").filter((l) => /^(model|skizze_(deckel|halter|kabelkanal)|3[456]_)/.test(l)) : [];
const alltag = readText(join(input, "alltag.txt"));
const alltagLine = alltag ? alltag.trim().split("\n").at(-1) : null;
// tests/everyday/check.sh summary line: "N models, 0 with problems, …" (older runs: "0 mit Problemen")
const alltagOk = alltag != null && /\b0 (with problems|mit Problemen)/.test(alltag);
const nodeTest = readText(join(input, "node-test.txt"));
const scenarios = (readJson(join(input, "scenarios.json")) ?? []).sort((a, b) => a.id.localeCompare(b.id));
const flaky = readText(join(input, "flaky.txt"));

const tsv = (name) =>
  (readText(join(input, name)) ?? "")
    .trim()
    .split("\n")
    .filter(Boolean)
    .map((l) => l.split("\t"));
const areas = tsv("areas.tsv").map(([name, got, want, ms, code]) => {
  const g = Number(got);
  const w = Number(want);
  return { name, got: g, want: w, rel: Math.abs(g - w) / Math.abs(w), ms: Number(ms), code };
});
const pyEval = (e) => {
  if (!e) return null;
  try {
    return Number(execFileSync("python3", ["-c", `from math import *; print(repr(${e}))`], { encoding: "utf8" }).trim());
  } catch {
    return null;
  }
};
const osc = tsv("openscad.tsv").map(([name, code, vo, vu, exact, bo, bu, cls]) => {
  const o = Number(vo);
  const u = Number(vu);
  const x = pyEval(exact);
  const bO = JSON.parse(bo || "{}");
  const bU = JSON.parse(bu || "{}");
  let dbox = null;
  if (bO.min && bU.min) dbox = Math.max(...[0, 1].flatMap((i) => [Math.abs(bO.min[i] - bU.min[i]), Math.abs(bO.max[i] - bU.max[i])]));
  return { name, code, o, u, x, rel: (u - o) / o, relx: x ? (u - x) / x : null, relox: x ? (o - x) / x : null, dbox, cls };
});
const steps = tsv("step.tsv").map(([name, cnt, info, vol]) => ({ name, cnt: JSON.parse(cnt), info: JSON.parse(info), vol: Number(vol) }));
const timings = Object.fromEntries(tsv("timings.tsv").map(([n, ms]) => [n, Number(ms)]));

const date = new Date().toISOString().slice(0, 10);
const commit = git("rev-parse", "--short", "HEAD");
const branch = git("rev-parse", "--abbrev-ref", "HEAD");
const rustc = (() => {
  try {
    return execFileSync("rustc", ["--version"], { encoding: "utf8" }).trim();
  } catch {
    return "–";
  }
})();

const ANAMES = {
  rechteck_fillet2d: "`fillet2d(4) square([30, 20])`: ab − (4 − π) r²",
  rechteck_corner: "same as a sketch with four `corner(r = 4)` (start corner directly after `start`)",
  quadrat_kreis: "`fillet2d(5) square(10)`: the lines vanish, a circle",
  rechteck_chamfer2d: "`chamfer2d(2) square([30, 20])`: ab − 4 · d²/2",
  l_konkav: "L profile, all corners r 2: 5 convex (−), 1 concave (+), each (1 − π/4) r²",
  l_zone: "same, only the concave corner (zone)",
  offset_quadrat: "`offset(r = 2) square(10)`: (a + 2r)² − (4 − π) r²",
  offset_quadrat_fein: "`offset(r = 0.5) square(20)`",
  offset_ring: "`offset(r = 1)` of a ring 10/4: π (11² − 3²)",
  offset_delta_fase: "`offset(delta = 2, chamfer = true) square(10)`: cut at distance 2",
  stroke_bogen_flach: "`stroke(2, cap = \"flat\")` of a quarter circle r 10: annular sector",
  stroke_bogen_rund: "same with round caps: annular sector + π (w/2)²",
  stroke_bogen_eckig: "same with square caps: annular sector + w²",
  stroke_ecke_gehrung: "Polyline with a right angle, w 2, miter: bands + 1",
  stroke_ecke_rund: "same, round join: bands + π/4",
  stroke_ecke_fase: "same, bevel: bands + 1/2",
  stroke_kreisrand: "`stroke(2) circle(10)`: ring π (11² − 9²)",
  corner_bogen: "Quarter circle r 10, `corner(r = 1)` between line and arc (closed form)",
};
const ONAMES = {
  quadrat_r: "offset(r = 2) square(10)",
  quadrat_r_innen: "offset(r = −2) square(10)",
  kreis_r: "offset(3) circle(r = 5) – positional = r",
  l_r: "offset(r = 1.5) L profile (concave corner stays sharp)",
  l_r_innen: "offset(r = −1.5) L profile (concave corner becomes round)",
  ohne_argument: "offset() square(10) – r = 1",
  r_vor_delta: "offset(r = 2, delta = 1) – r wins",
  quadrat_delta: "offset(delta = 2) square(10)",
  quadrat_fase: "offset(delta = 2, chamfer = true) square(10)",
  quadrat_delta_innen: "offset(delta = −2) square(10)",
  stern_delta: "offset(delta = 1) star (10 points)",
  stern_fase: "offset(delta = 1, chamfer = true) star",
  spitzes_dreieck: "offset(delta = 1) acute triangle (5.7°): miter without limit",
  l_delta: "offset(delta = 1.5) L profile",
  l_delta_innen: "offset(delta = −1.5) L profile",
  l_fase_innen: "offset(delta = −1.5, chamfer = true) L profile",
  platte: "Plate 30 × 20 with circular and square hole (without offset)",
  platte_r: "offset(r = 1) plate: holes smaller",
  platte_r_innen: "offset(r = −1) plate: holes larger",
  loch_schliesst: "offset(r = 4.5) plate: both holes closed",
  platte_delta: "offset(delta = 1) plate",
  zusammenwachsen: "offset(r = 1.5) of two squares 2 apart: they grow together",
  steg_verschwindet: "offset(r = −1): 1.5 mm web disappears, two parts",
  oeffnen: "offset(r = 2) offset(r = −2): opening – corners round, narrow parts gone",
};
const oscMax = osc.length ? Math.max(...osc.map((r) => Math.abs(r.rel))) : null;
const oscPoly = osc.filter((r) => r.cls === "polygonal");
const oscPolyMax = oscPoly.length ? Math.max(...oscPoly.map((r) => Math.abs(r.rel))) : null;
const oscBox = osc.length ? Math.max(...osc.map((r) => r.dbox ?? 0)) : null;
const areaMax = areas.length ? Math.max(...areas.map((a) => a.rel)) : null;

const html = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Test report 15 – Sketches: corners, offset, stroke</title>
<style>
  @page { size: A4; margin: 15mm 13mm 16mm 13mm; }
  :root { --text: #1f2328; --muted: #5d646e; --border: #d5d9df; --ok: #1a7f37; --bad: #c9302c; --head: #f2f4f7; }
  * { box-sizing: border-box; }
  body { font: 9.6pt/1.45 system-ui, -apple-system, "Segoe UI", Roboto, sans-serif; color: var(--text); margin: 0; }
  main { max-width: 184mm; margin: 0 auto; }
  h1 { font-size: 18pt; margin: 0 0 2mm; }
  h2 { font-size: 13pt; margin: 7mm 0 3mm; border-bottom: 1px solid var(--border); padding-bottom: 1mm; break-after: avoid; }
  h3 { font-size: 10.5pt; margin: 4mm 0 1.5mm; break-after: avoid; }
  p { margin: 1.5mm 0; }
  .meta { color: var(--muted); margin-bottom: 4mm; }
  .summary { display: flex; flex-wrap: wrap; gap: 2.5mm; margin: 3mm 0; }
  .summary div { flex: 1 1 28mm; border: 1px solid var(--border); border-radius: 2mm; padding: 2mm; font-size: 8.4pt; }
  .summary b { display: block; font-size: 12pt; }
  table { width: 100%; border-collapse: collapse; margin: 2mm 0; font-size: 8.2pt; }
  th, td { border: 1px solid var(--border); padding: 1.1mm 1.8mm; text-align: left; vertical-align: top; }
  th { background: var(--head); font-weight: 600; }
  td.n { text-align: right; white-space: nowrap; }
  code, pre { font-family: ui-monospace, "SFMono-Regular", Menlo, Consolas, monospace; font-size: 7.6pt; }
  pre { margin: 0 0 1.5mm; padding: 1.4mm 2mm; background: #f6f8fa; border: 1px solid var(--border); border-radius: 1mm; white-space: pre-wrap; word-break: break-word; }
  .small { font-size: 7.6pt; color: var(--muted); font-weight: 400; }
  ul { margin: 1mm 0 2mm 5mm; padding: 0; }
  li { margin: 0.5mm 0; }
  tr, td, figure, pre { break-inside: avoid; }
  .ok { color: var(--ok); font-weight: 600; } .bad { color: var(--bad); font-weight: 600; }
  figure { margin: 2mm 0; }
  figure img { max-width: 100%; max-height: 92mm; border: 1px solid var(--border); border-radius: 1mm; }
  figcaption { font-size: 7.8pt; color: var(--muted); }
  .two { display: grid; grid-template-columns: 1fr 1fr; gap: 3mm; align-items: start; }
  .codefig { display: grid; grid-template-columns: 1fr 1fr; gap: 3mm; align-items: start; }
  .note { border-left: 1mm solid #e8b84a; padding: 1mm 2.5mm; background: #fff8e6; margin: 2mm 0; }
</style>
</head>
<body>
<main>
<h1>Test report 15 – Sketches: corners, offset, stroke</h1>
<div class="meta">Plan 17.7, step 2: <code>corner</code>, <code>fillet2d</code>/<code>chamfer2d</code>, <code>offset</code>, <code>stroke</code> · cad-kernel (<code>region2.rs</code>, biarcs in <code>curve2.rs</code>), cad-ir, cad-eval, cad-engine (<code>op2d.rs</code>), cad-session, cad-cli, web · ${esc(date)} · branch ${esc(branch)} · commit ${esc(commit)} · ${esc(rustc)} · ${os.cpus().length} cores</div>

<div class="summary">
  <div>cargo test --workspace<b class="${cargoSum && cargoSum.failed === 0 ? "ok" : "bad"}">${cargoSum ? `${cargoSum.ok} / ${cargoSum.ok + cargoSum.failed}` : "–"}</b>${newTests.length} of them new/extended</div>
  <div>clippy · fmt<b class="${clippyOk && fmtOk ? "ok" : "bad"}">${clippy == null ? "–" : clippyOk ? "clean" : "finding"} · ${fmt == null ? "–" : fmtOk ? "clean" : "diff"}</b>-D warnings</div>
  <div>Vitest · Playwright<b class="${pwStats && pwStats.failed.length === 0 ? "ok" : "bad"}">${vitestSum ?? "–"} · ${pwStats ? `${pwStats.passed} / ${pwStats.total}` : "–"}</b>new: E01–E05</div>
  <div>OpenSCAD · STEP<b class="${compareSum && compareSum[0] === compareSum[1] ? "ok" : "bad"}">${compareSum ? `${compareSum[0]}/${compareSum[1]}` : "–"} · ${esc(stepSum?.match(/\d+\/\d+/)?.[0] ?? "–")}</b>compare.py · verify.py (OCCT)</div>
  <div>offset per case<b class="${oscMax !== null && oscMax < 0.01 ? "ok" : "bad"}">≤ ${pct(oscMax, 3)}</b>${osc.length} cases against OpenSCAD</div>
  <div>Everyday · Wasm (Node)<b class="${alltagOk && nodeTest && /all node tests passed/.test(nodeTest) ? "ok" : "bad"}">${alltagOk ? "unchanged" : "–"} · ${nodeTest && /all node tests passed/.test(nodeTest) ? "ok" : "–"}</b>check.sh · test-node.mjs</div>
</div>

<h2>1 Task and result</h2>
<p>Step 2 of plan 17: <b>corners</b> in sketches (<code>corner(r = …)</code>, <code>corner(chamfer = …)</code>) and on arbitrary 2D profiles (<code>fillet2d</code>, <code>chamfer2d</code>, with zones as in 3D), <b><code>offset</code></b> compatible with OpenSCAD and exact for lines and arcs, <b><code>stroke</code></b> for open paths; display and handles in UI and CLI. The basis is a new module <code>cad_kernel::region2</code>: exact Booleans of regions made of lines and circular arcs, from which offset, stroke and the evaluation of arbitrary 2D subtrees are built. Text, SVG import (step 3) and draw mode (step 4) are not part of this.</p>
<ul>
<li>Areas against formulas (section 7): all ${areas.length} cases within the volume integration of SMLib (≤ ${sci(areaMax)} relative) – rectangle with filleted corners = ab − (4 − π) r², offset of a square = (a + 2r)² − (4 − π) r², stroke of a circular arc = annular sector + caps.</li>
<li>OpenSCAD 2021.01 (section 4): ${osc.length} individual cases, area deviation ≤ ${pct(oscMax, 3)} (polygonal, i.e. <code>delta</code> on polygons: ≤ ${pct(oscPolyMax, 4)}), bounding box ≤ ${num(oscBox, 4)} mm; compare.py with the new models 34–36: ${compareSum ? `${compareSum[0]}/${compareSum[1]}` : "–"}. The remaining deviation for <code>r</code> is OpenSCAD's faceting (<code>$fn = 128</code>): we compute exact arcs.</li>
<li>STEP: arcs as <code>CIRCLE</code>, extruded arcs as cylinders, no B-spline curves and no approximated edges in the new examples; OpenCASCADE reads them with the same volume (verify.py).</li>
</ul>

<h2>2 corner() in sketches</h2>
<p><b>Rules</b> (plan 17.9): <code>corner()</code> between two segments rounds the corner between them – the closing line of <code>close()</code> counts as a segment, so <code>corner()</code> before <code>close()</code> rounds the corner to the closing line; <code>corner()</code> <b>directly after <code>start(p)</code></b> rounds the start corner (between the last and the first segment); likewise <code>corner()</code> after the last segment of a contour that ends at the start (no closing line needed). The sketch is first built without corners (tangential arcs and <code>line(length)</code> refer to the sharp corner), then all corners of a contour are computed on the original segments; if two corners need overlapping parts of one segment, both fail with the largest common value – independent of the order. The fillet is a segment of the corner line (highlighting); a start corner is placed at the end of the contour. Chamfer: intersection points at distance <code>d</code> from the corner point.</p>
<table>
<tr><th>Segments</th><th>Solution</th></tr>
<tr><td>line – line, line – arc, arc – arc</td><td>exact: center = intersection of the curves offset by r (line/line, line/circle, circle/circle with R ± r depending on the side), foot points on the segments, shortest candidate</td></tr>
<tr><td>with Bézier or spline</td><td>Newton on <code>a(u) + r·n<sub>a</sub>(u) = b(w) + r·n<sub>b</sub>(w)</code> (start value from the tangents), stop at residual 10<sup>−13</sup> of the size, warning above 10<sup>−6</sup>; the curve is trimmed at the parameter (stays a B-spline). Test: line–Bézier, residual &lt; 10<sup>−9</sup>, distance center–curve = r to 10<sup>−6</sup></td></tr>
</table>
<p><b>Errors at the corner line:</b> radius does not fit ("at most ≈ r", via bisection), tangential corner ("no corner"), cusp (180°), after <code>close()</code>, before <code>start</code>, twice at one corner, start/end of an open contour, value ≤ 0. <b>Handle:</b> radius at the middle of the fillet, base at the corner point, linear in r (UI test E01).</p>
<div class="codefig">${pre(src("crates/cad-cli/tests/data/sketch_corners.mcad"))}<div>${fig(cli("ecken-handles"), "`cad render --line 5 --view top --show handles`: the sketch highlighted, handles (radius on the fillets)")}${fig(cli("ecken-offset"), "`--line 13`: offset – source contour, offset contour dashed")}</div></div>
${pre(T("cases/fehler.scad"))}
${pre(T("check-fehler.txt"))}

<h2>3 fillet2d and chamfer2d</h2>
<p><code>fillet2d(r) { profil; zone; … }</code> / <code>chamfer2d(d)</code>: the first child is the profile (any origin: <code>square</code>, <code>polygon</code>, sketches, 2D Booleans), further children are zones (2D shapes and <code>zone(linie | ebene, d)</code>). Corners are joints with a kink, convex and concave; the fillet lies on the inner side of the kink (convex: material removed, concave: material added). <b>Selection</b> as in 3D: <code>select = "inside"</code> (default) – the corner point lies in a zone; <code>"touch"</code> – also when only the replaced piece of the profile touches a zone. Inline hint "n corners", an empty selection is a warning, radii that are too large are errors per corner (the remaining corners are rounded). Nested blocks act from the inside out; the transitions of a fillet are tangential and do not count. A single closed contour keeps its segments (splines stay splines); everything else goes through the exact region.</p>
<div class="codefig">${pre(src("tests/models/spec/sketch_holder.mcad"))}<div>${fig(cli("halter"), "sketch_holder.mcad: inner corner r 5 (zone), all other corners r 1, slots from `stroke`")}${fig(cli("halter-fillet"), "`--line 18` from above: the five new corner arcs of the outer block")}</div></div>
${pre(T("inspect-halter.txt").split("\n").slice(0, 14).join("\n"))}

<h2>4 offset: semantics and comparison with OpenSCAD</h2>
<p>As in OpenSCAD 2021.01: first positional argument <code>r</code>, without arguments <code>r = 1</code>, <code>r</code> wins over <code>delta</code>, <code>chamfer</code> only with <code>delta</code>. <code>r &gt; 0</code>: convex corners become <b>exact arcs</b>, concave ones stay sharp; <code>r &lt; 0</code>: convex ones stay sharp, concave ones become round, narrow parts disappear, holes grow. <code>delta</code>: corners extended to the intersection of the offset edges (without a miter limit – OpenSCAD sets <code>miter_limit</code> 10<sup>6</sup>, as the acute triangle below confirms), <code>chamfer = true</code> cuts like Clipper's <code>jtSquare</code> perpendicular to the angle bisector at distance <code>delta</code> from the corner point (measured in OpenSCAD: 196 − 4·(2√2 − 2)² for the square). Exact: region ∪ (bands ± r along the segments ∪ circles at the corners), or region ∖ (…) for r &lt; 0 – the Minkowski sum with the circle, made of lines and arcs; <code>delta</code>: one-sided bands and corner pieces (miter/chamfer) at diverging corners.</p>
<p>One file <code>linear_extrude(1) …</code> per case (volume = area, <code>reports/tmp/report15/openscad/</code>); OpenSCAD exports STL (<code>$fn = 128</code> for roundings, as in <code>make-reference.py</code>), we measure exactly (<code>cad measure</code>). "exact": closed-form formula where one exists.</p>
<table>
<tr><th>Case</th><th>OpenSCAD mm²</th><th>ours mm²</th><th>dev.</th><th>exact</th><th>OpenSCAD − exact</th><th>bbox Δ mm</th></tr>
${osc.map((r) => `<tr><td>${esc(ONAMES[r.name] ?? r.name)}</td><td class="n">${num(r.o, 4)}</td><td class="n">${num(r.u, 4)}</td><td class="n">${pct(r.rel, 4)}</td><td class="n">${r.x ? num(r.x, 4) : "–"}</td><td class="n">${r.relox !== null ? pct(r.relox, 4) : "–"}</td><td class="n">${num(r.dbox, 4)}</td></tr>`).join("\n")}
</table>
<p class="small">Where a formula exists, the deviation of our values from it is the integration accuracy of SMLib (section 7); that of OpenSCAD is the faceting (circular arcs as 128-gons: chords inside, slightly less area) or Clipper's integer coordinates (chamfer of the square: 3·10<sup>−7</sup>).</p>
<p><b>compare.py</b> with the new models (<code>tests/models/openscad-compat/34_offset_round.scad</code>, <code>35_offset_delta.scad</code>, <code>36_offset_holes.scad</code>; reference generated with <code>make-reference.py</code> only for these files, <code>stl_stats.classify</code> counts <code>offset(r)</code> as rounded):</p>
${pre(compareNew.join("\n"))}
<div class="codefig">${pre(src("tests/models/spec/sketch_lid.mcad"))}<div>${fig(cli("deckel"), "sketch_lid.mcad: outline with four `corner(r = 6)`, plug-in rim as a ring of two inward offsets")}${fig(cli("deckel-offset"), "`--line 30`: source contour and offset contour (dashed); the corner radii 6 → 3.8 stay arcs")}</div></div>

<h2>5 stroke</h2>
<p><code>stroke(w, cap = "round" | "flat" | "square", join = "round" | "miter" | "bevel")</code> (default round/round): children are open sketches (paths; a contour that ends at its start is closed) and arbitrary 2D profiles (their boundary as closed paths: <code>stroke(1) circle(5)</code> is a ring). Bands of half the width along the segments (line: rectangle, arc: annular sector), joins round, mitered (up to 4 half-widths like SVG's <code>stroke-miterlimit</code>, otherwise bevel) or beveled, ends round, flat or extended by w/2; crossing paths are united.</p>
<div class="codefig">${pre(src("tests/models/spec/sketch_cable_duct.mcad"))}<div>${fig(cli("kabelkanal"), "sketch_cable_duct.mcad: floor and walls from `stroke` of a path with a tangential arc and `corner(r = 12)`, ribs as a zigzag with miter joins")}${fig(cli("kabelkanal-stroke"), "`--line 22`: center line of the floor")}</div></div>

<h2>6 Kernel: exact 2D regions</h2>
<ul>
<li><b><code>region2::combine(operanden, prädikat)</code></b>: all segments are split at their exact intersection points (line–line, line–circle, circle–circle including tangency, overlap of collinear lines and identical circles; points within 10<sup>−9</sup> of the size are merged), duplicate pieces are dropped. Each piece is kept where the predicate differs on its left and right (test points next to the middle of the piece; winding number per fill rule via the ray towards +x, arcs split into y-monotone pieces), with the region on the left. Joined into loops with the tightest turn at shared vertices; pieces of the same line or circle are merged. Result: outer loops counterclockwise, holes clockwise, only lines and arcs.</li>
<li>2D subtrees: the predicate is the Boolean expression of the leaves (fill rule per leaf: polygons even-odd, otherwise nonzero); a randomized test compares 60 Booleans of circles, rectangles, polygons and sketches with the polygon clipping of the display (≤ 10<sup>−3</sup>).</li>
<li><b>B-splines</b> enter as G1 biarcs (two tangential arcs per piece, pieces halved until the deviation is ≤ 1 µm, at most 10<sup>−5</sup> of the model size). Rationale: the offset of a B-spline is not a B-spline, the offset of an arc is an arc – the offset is exact with respect to the approximation and tangent-continuous, intersections stay solvable in closed form, STEP contains lines and circles; 1 µm is far below printing and manufacturing tolerances and above the computational accuracy.</li>
<li>Engine: <code>Op::Offset</code>, <code>Op::Blend2</code>, <code>Op::Stroke</code> are 2D nodes; <code>profile::lift</code> computes the region (cached by subtree content) and passes it on as sketch leaves – extrude, revolve, loft, sweep and STEP see exact lines and arcs. <code>cad_engine::region_2d</code> returns the region of a 2D node (bounding boxes for the compatibility tests).</li>
</ul>
<p class="note"><b>Fix from step 1 (arcs over 90°).</b> The facade builds an arc as a rational quadratic B-spline with spans ≤ 90°; a longer arc is a curve of several spans. Booleans with extrusions of such curves failed in SMLib depending on position (ring of two 180° arcs ∪ box, rotated by 157.5°: "volume 924.12 impossible for operands 976.79, 43.2"; correct with 4 × 90°). The engine now splits arcs over 90° into equal pieces ≤ 90° before the kernel (<code>profile::split_arcs</code>; extrude, revolve, sweep – not loft); a half circle becomes two cylindrical faces. Regression test <code>ring_of_long_arcs_unites_with_a_box</code>: arcs of 180°, 120°, 90° and 360°, each with eight box positions, volume = 3 × exact 2D area of the union (≤ 10<sup>−5</sup>). Found along the way: the winding number of the region took arc end points from the angles (rounding ~10<sup>−15</sup>), so a ray exactly through a vertex counted wrongly (ring of three 120° arcs without a hole) – now the shared vertices of the loop are used (test <code>rings_of_arcs_of_any_sweep</code>).</p>
<table>
<tr><th>Model</th><th>Faces</th><th>CIRCLE</th><th>LINE</th><th>B-spline curves</th><th>Surface types</th><th>approximated</th></tr>
${steps.map((s) => `<tr><td>${esc(s.name)}</td><td class="n">${esc(s.info.faces)}</td><td class="n">${s.cnt.CIRCLE}</td><td class="n">${s.cnt.LINE}</td><td class="n">${s.cnt.B_SPLINE_CURVE_WITH_KNOTS}</td><td>${esc(Object.entries(s.info.surfaces ?? {}).map(([k, v]) => `${k} ${v}`).join(", "))}</td><td class="n">${esc(s.info.approximated_edges)} / ${esc(s.info.approximated_faces)}</td></tr>`).join("\n")}
</table>
<p>OpenCASCADE (<code>tests/step/verify.py</code>) for the new models (columns: bodies, faces, V kernel, V OCCT, dV, dA, dBox, surface types):</p>
${pre(stepNew.join("\n"))}

<h2>7 Measurements: areas against formulas, run time</h2>
<p><code>linear_extrude(1) …</code> per case (<code>reports/tmp/report15/cases/</code>), <code>cad measure --volume</code> with the release build (time: whole CLI call without daemon). The geometry is exact; the deviation is that of the volume integration in SMLib.</p>
<table>
<tr><th>Case</th><th>Kernel mm²</th><th>Formula mm²</th><th>rel. dev.</th><th>ms</th></tr>
${areas.map((v) => `<tr><td>${md(ANAMES[v.name] ?? v.name)}</td><td class="n">${num(v.got, 4)}</td><td class="n">${num(v.want, 4)}</td><td class="n">${sci(v.rel)}</td><td class="n">${num(v.ms)}</td></tr>`).join("\n")}
</table>
<p><b>Run time</b> (release, <code>cad check</code>, best of three runs, whole call): gear with 160 corners and a bore – <code>offset(r = 0.5)</code>, <code>fillet2d(0.4)</code> of all 160 corners – and <code>stroke</code> of a spline, 2D only: <b>${num(timings.zeit2d)} ms</b>; the same extruded (kernel): ${num(timings.zeit3d)} ms. Examples: lid ${num(timings.skizze_deckel)} ms, bracket ${num(timings.skizze_halter)} ms, cable duct ${num(timings.skizze_kabelkanal)} ms (mostly kernel Booleans).</p>

<h2>8 UI: display and handles</h2>
<p>Cursor on <code>offset</code>: the children as an outline (source contour), the offset contour dashed in the accent color; on <code>stroke</code>: the path as an outline, the center line dashed; on <code>fillet2d</code>/<code>chamfer2d</code>: zones blue, the profile before filleting as an outline, the new corner pieces like selected edges, "n corners" at the line; on <code>corner</code>: the fillet as the active segment of the sketch with a radius handle. The helper lines come from the engine (<code>SceneNode.guides</code>: <code>source</code>, <code>result</code>, <code>centerline</code>). Checks via the UI state (<code>window.__cad</code>) and pixels of the layers (<code>guide:offset</code>, <code>guide:centerline</code>, <code>sketch:active</code>).</p>
${scenarios.map((s) => `<h3>${esc(s.id)} ${esc(s.title)}</h3><p>${md(s.caption)}</p>${(s.images ?? []).length === 2 ? pair(pw(s.images[0].name), s.images[0].label, pw(s.images[1].name), s.images[1].label) : (s.images ?? []).map((i) => fig(pw(i.name), i.label)).join("")}${s.code && s.code.split("\n").length <= 16 ? pre(s.code) : ""}<ul>${(s.checks ?? []).map((c) => `<li>${md(c)}</li>`).join("")}</ul>`).join("\n")}

<h2>9 Test runs</h2>
<table>
<tr><th>Run</th><th>Result</th></tr>
<tr><td><code>cargo test --workspace --no-fail-fast</code></td><td>${cargoSum ? `${cargoSum.ok} passed, ${cargoSum.failed} failed${failedTests.length ? ` (${failedTests.map(esc).join(", ")})` : ""}` : "–"}</td></tr>
<tr><td><code>cargo clippy --workspace --all-targets -- -D warnings</code> · <code>cargo fmt --check</code></td><td>${clippyOk ? "clean" : "finding"} · ${fmtOk ? "clean" : "diff"}</td></tr>
<tr><td><code>pnpm typecheck</code> · <code>pnpm test</code> (Vitest)</td><td>${readText(join(input, "typecheck.txt")) !== null ? (/error TS/i.test(readText(join(input, "typecheck.txt"))) ? "errors" : "clean") : "–"} · ${vitestSum ?? "–"} passed</td></tr>
<tr><td><code>pnpm e2e</code> (Playwright, Chromium)</td><td>${pwStats ? `${pwStats.passed} / ${pwStats.total}${pwStats.failed.length ? ` – failed: ${pwStats.failed.map((f) => esc(f.title)).join(", ")}` : ""}` : "–"}${flaky ? ` · ${md(flaky.trim())}` : ""}</td></tr>
<tr><td><code>tests/openscad-compat/compare.py --eval</code></td><td>${compareSum ? `${compareSum[0]}/${compareSum[1]}` : "–"}</td></tr>
<tr><td><code>tests/step/verify.py</code> (OpenCASCADE)</td><td>${esc(stepSum ?? "–")}</td></tr>
<tr><td><code>tests/everyday/check.sh</code></td><td>${esc(alltagLine ?? "–")}</td></tr>
<tr><td><code>node crates/kernel-smlib/js/test-node.mjs</code></td><td>${nodeTest && /all node tests passed/.test(nodeTest) ? "passed" : "–"}</td></tr>
</table>
<p>New or extended Rust tests (${newTests.length}):</p>
<p class="small">${newTests.map((t) => `<code>${esc(t.name)}</code> ${t.res === "ok" ? '<span class="ok">ok</span>' : '<span class="bad">FAILED</span>'}`).join(" · ")}</p>
<p class="small">Vitest: <code>web/src/logic/sketch2d.test.ts</code> (offset contour, center line, zones and corner arcs of fillet2d, radius handle of corner on scenes of the examples). CLI: <code>sketch_corners_offset_stroke</code> (volume of all four operations, hint "4 corners", radius handle, helper lines in <code>render</code>, errors at the corner line). Compatibility: <code>openscad_compat</code>, <code>tree_bounds</code> (bounding boxes of the new models exactly via <code>region_2d</code>), <code>spec_models</code> (three new examples).</p>

<h2>10 Limitations</h2>
<ul>
<li><code>offset(delta)</code> on arcs extends the tangents (miter of the tangent lines) – OpenSCAD only has polygons there. B-splines enter <code>offset</code>, <code>stroke</code> and 2D Booleans as biarcs (1 µm); the result contains arcs instead of the spline; <code>corner</code> and <code>fillet2d</code> of a single contour keep the spline.</li>
<li><code>fillet2d</code>/<code>chamfer2d</code> have no radius handle (only <code>corner</code> does); zones are 2D shapes and <code>zone(linie | ebene)</code>; 3D zones are ignored with a warning.</li>
<li>The region classifies pieces via test points at a distance of ~10<sup>−6</sup> of the size: boundaries that come closer than that without intersecting or overlapping can be classified wrongly. Candidate pairs via an x sort (quadratic in the worst case).</li>
<li>2D Booleans <i>between</i> results (<code>difference() { stroke(…); stroke(…); }</code>) remain 3D Booleans of the extruded parts as before; <code>offset</code>, <code>fillet2d</code>, <code>stroke</code> compute exactly in 2D (over their whole subtree).</li>
<li><code>stroke</code>: miter limit fixed at 4 half-widths; an arc with a radius below half the width is swept as a sector plus a mirrored sector. Errors of a 2D operation under an extrusion appear at both lines (as before for 2D errors).</li>
<li>Lofts join sketches segment by segment and still receive arcs over 90° as curves of several spans; whether SMLib Booleans with such loft faces have the same weakness has not been checked.</li>
<li><code>cad render --line N</code> of a corner line highlights the whole sketch (highlighting of the single segment only in the UI, as in report 14).</li>
</ul>
</main>
</body>
</html>
`;
writeFileSync(join(out, "report.html"), html);
console.log(`${join(out, "report.html")}: ${html.length} characters, ${scenarios.length} scenarios, ${areas.length} area cases, ${osc.length} OpenSCAD cases`);
