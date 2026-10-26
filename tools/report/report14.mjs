#!/usr/bin/env node
// Test report 14 ("sketches and namespaces", plan 17.7 step 1): sketches with mixed
// contours (lines, arcs, Bézier curves, splines) through the kernel, modules as namespaces,
// display and handles in the UI. Writes docs/reports/14-skizzen/report.html (images in img/).
//
// Inputs (gitignored, reports/tmp/report14/):
//   cli/*.png, volumes.tsv, step.tsv, outline.txt, check-*.txt   tools/report/report14-data.sh
//   scenarios.json, img/*.jpg                                   cd web && pnpm exec playwright test e2e/skizzen.spec.ts
//   playwright.json, playwright.txt                             whole suite (pnpm e2e; copy of reports/tmp/report2/playwright.json)
//   vitest.txt, typecheck.txt                                   cd web && pnpm test, pnpm typecheck
//   cargo.txt, clippy.txt, fmt.txt                              cargo test --workspace --no-fail-fast, clippy -D warnings, fmt --check
//   compare.txt, step.txt, alltag.txt, node-test.txt            compare.py --eval, verify.py, tests/alltag/check.sh, test-node.mjs
//
//   node tools/report/report14.mjs
//   node tools/report/print-pdf.mjs docs/reports/14-skizzen/report.html docs/reports/14-skizzen/report.pdf "Test report 14: sketches and namespaces"

import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import os from "node:os";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "../..");
const input = join(root, "reports/tmp/report14");
const out = join(root, "docs/reports/14-skizzen");
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
const newTests = cargo
  ? [...cargo.matchAll(/^test (\S+) \.\.\. (ok|FAILED)$/gm)]
      .map((m) => ({ name: m[1], res: m[2] }))
      .filter((t) => /sketch|namespace|qualified|curve2::|arc_forms|segments_of|errors_per_segment|rectangle_with|revolve_arc|holes_by|bezier_and|loft_between|step_has_exact|sweep_along|several_contours|handles_of_segments|loops_variables|openscad_files_are|module_without|spec_models/.test(t.name))
  : [];
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
  return { total: all.length, passed: all.filter((t) => t.status === "passed").length, failed: all.filter((t) => t.status !== "passed" && t.status !== "skipped"), neu: all.filter((t) => t.file?.includes("skizzen")) };
})();
const vitest = readText(join(input, "vitest.txt"));
const vitestSum = vitest ? (/Tests\s+(\d+) passed/.exec(vitest)?.[1] ?? null) : null;
const compare = readText(join(input, "compare.txt"));
const compareSum = compare ? /(\d+)\/(\d+)/.exec(compare.split("\n").reverse().find((l) => /\d+\/\d+ files/.test(l)) ?? "")?.slice(1, 3) : null;
const step = readText(join(input, "step.txt"));
const stepSum = step ? (/\d+\/\d+ passed/.exec(step)?.[0] ?? null) : null;
const stepSketch = step ? step.split("\n").filter((l) => /^skizze/.test(l)) : [];
const alltag = readText(join(input, "alltag.txt"));
const alltagLine = alltag ? alltag.trim().split("\n").at(-1) : null;
const nodeTest = readText(join(input, "node-test.txt"));
const scenarios = (readJson(join(input, "scenarios.json")) ?? []).sort((a, b) => a.id.localeCompare(b.id));
const flaky = readText(join(input, "flaky.txt"));

const volumes = (readText(join(input, "volumes.tsv")) ?? "")
  .trim()
  .split("\n")
  .filter(Boolean)
  .map((l) => {
    const [name, got, want, ms, code] = l.split("\t");
    const g = Number(got);
    const w = Number(want);
    return { name, got: g, want: w, rel: Math.abs(g - w) / Math.abs(w), ms: Number(ms), code };
  });
const steps = (readText(join(input, "step.tsv")) ?? "")
  .trim()
  .split("\n")
  .filter(Boolean)
  .map((l) => {
    const [name, cnt, info, vol] = l.split("\t");
    return { name, cnt: JSON.parse(cnt), info: JSON.parse(info), vol: Number(vol) };
  });

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

const VNAMES = {
  rechteck_halbkreis: "Rectangle 40 × 20 + half circle r 20 (end point + radius), h 5",
  tangentialbogen: "same with tangential arc",
  mittelpunkt_winkel: "same with center + angle",
  kugel: "rotate_extrude of a half circle: sphere r 7",
  torus: "rotate_extrude of two arcs: torus R 20, r 4",
  ring_windung: "Square 20 minus circle r 5 (reverse direction, nonzero)",
  ring_hole: "same with `hole { … }`",
  bezier_quadratisch: "Square 4 + quadratic Bézier cap (parabola 2/3 · b · h)",
  stern_schleife: "Star of 12 lines via a loop in a sketch module",
  rohr_pfad: "sweep of a circle r 2 along line–arc–line (Pappus)",
  gehrung_pfad: "sweep of a square 2 along a corner (miter)",
  twist_bogen: "linear_extrude(twist = 90) of a slot (volume without twist)",
};
const noProblems = (t) => /\b0 (mit Problemen|with problems)/.test(t ?? "");

const html = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Test report 14 – Sketches and namespaces</title>
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
<h1>Test report 14 – Sketches and namespaces</h1>
<div class="meta">Plan 17.7, step 1: <code>sketch { … }</code> with mixed contours, modules as namespaces · cad-syntax, cad-eval, cad-ir, cad-kernel, cad-engine, kernel-smlib (facade <code>sc_sketch.cpp</code>), cad-step, cad-session, cad-cli, web · ${esc(date)} · branch ${esc(branch)} · commit ${esc(commit)} · ${esc(rustc)} · ${os.cpus().length} cores</div>

<div class="summary">
  <div>cargo test --workspace<b class="${cargoSum && cargoSum.failed === 0 ? "ok" : "bad"}">${cargoSum ? `${cargoSum.ok} / ${cargoSum.ok + cargoSum.failed}` : "–"}</b>${newTests.length} of them new/extended</div>
  <div>clippy · fmt<b class="${clippyOk && fmtOk ? "ok" : "bad"}">${clippy == null ? "–" : clippyOk ? "clean" : "finding"} · ${fmt == null ? "–" : fmtOk ? "clean" : "diff"}</b>-D warnings</div>
  <div>Vitest · Playwright<b class="${pwStats && pwStats.failed.length === 0 ? "ok" : "bad"}">${vitestSum ?? "–"} · ${pwStats ? `${pwStats.passed} / ${pwStats.total}` : "–"}</b>new: S01–S05</div>
  <div>OpenSCAD · STEP<b class="${compareSum && compareSum[0] === compareSum[1] ? "ok" : "bad"}">${compareSum ? `${compareSum[0]}/${compareSum[1]}` : "–"} · ${esc(stepSum?.match(/\d+\/\d+/)?.[0] ?? "–")}</b>compare.py · verify.py (OCCT)</div>
  <div>Everyday models<b class="${noProblems(alltag) ? "ok" : "bad"}">${noProblems(alltag) ? "unchanged" : "–"}</b>tests/alltag/check.sh</div>
  <div>Wasm kernel (Node)<b class="${nodeTest && /all node tests passed/.test(nodeTest) ? "ok" : "bad"}">${nodeTest && /all node tests passed/.test(nodeTest) ? "passed" : "–"}</b>test-node.mjs</div>
</div>

<h2>1 Task and result</h2>
<p>Step 1 of plan 17: <b>sketches</b> – a 2D block of segments (lines, circular arcs, Bézier curves, splines), each line of code one segment with its own display, its own handles and its own errors – as a profile for <code>linear_extrude</code>, <code>rotate_extrude</code>, <code>loft</code> and <code>sweep</code> (also as a path), exact through the kernel all the way to STEP; and <b>modules as namespaces</b> (<code>teile.schraube(d = 3)</code>). Draw mode (step 4) and <code>corner</code>/<code>offset</code>/<code>stroke</code> (step 2) are not part of this; <code>corner()</code> is reserved (warning).</p>
<ul>
<li>Volumes against formulas (section 5): all cases within the integration accuracy of SMLib (≤ ${sci(Math.max(...volumes.filter((v) => v.name !== "twist_bogen").map((v) => v.rel)))} relative; the twist extrusion is a cubic loft approximation as before).</li>
<li>STEP: arcs as <code>CIRCLE</code>, lines as <code>LINE</code>, extruded arcs as <code>CYLINDRICAL_SURFACE</code>, no approximated edges; OpenCASCADE reads all four new examples with the same volume (≤ 10<sup>−6</sup>).</li>
<li>OpenSCAD files unchanged: compare.py ${compareSum ? `${compareSum[0]}/${compareSum[1]}` : "–"}; user modules named <code>line</code>/<code>arc</code> and <code>spline(points)</code> outside sketches behave as before (tests).</li>
</ul>

<h2>2 Sketches: language</h2>
<p>Decisions (plan 17.8): <code>sketch</code> is a built-in 2D module; <code>sketch {</code> and <code>hole {</code> work without parentheses; segments are ordinary statements that append to the sketch currently being evaluated – also from within modules and loops. Inside a sketch, segment names take precedence over user modules of the same name; outside, everything stays as in OpenSCAD. Other geometry and transformations inside a sketch are an error. Each sketch is <b>one</b> node <code>Profile::Sketch</code>; the segment statements are stored in <code>EvalOutput::sketches</code> so that the cache key does not depend on source positions.</p>
<table>
<tr><th>Segment</th><th>Forms</th></tr>
<tr><td><code>start(p)</code></td><td>starts a contour; points may be grid points and anchors (z ≠ 0: warning, projected)</td></tr>
<tr><td><code>line</code></td><td><code>to = p</code>, <code>by = v</code> (relative), <code>length = l, angle = a</code>; <code>length</code> alone continues the current direction</td></tr>
<tr><td><code>arc</code></td><td><code>to, r [, large, cw]</code> as in SVG · <code>center, angle</code> (degrees, sign = direction) · <code>center, to</code> · <code>through, to</code> · <code>to</code> alone = tangential</td></tr>
<tr><td><code>bezier</code></td><td><code>c1, to</code> (quadratic) · <code>c1, c2, to</code> (cubic)</td></tr>
<tr><td><code>spline</code></td><td>points after the current point, <code>tangent = true | v</code>, <code>end_tangent = v</code>; cubic global interpolation (chord length, averaged knots)</td></tr>
<tr><td><code>close()</code></td><td>line back to the start; if the contour ends at the start, it is closed even without <code>close()</code></td></tr>
<tr><td><code>hole { … }</code></td><td>contours inside are holes (in either direction)</td></tr>
</table>
<p><b>Holes and multiple contours:</b> winding rule <i>nonzero</i>; a contour is a boundary where the winding number is ≠ 0 on one side only; islands in holes via nesting. <b>Sweep:</b> without <code>path</code>, an open sketch (<code>sketch(closed = false)</code>) among the children is the path. <b>Loft:</b> equal segment count → segment by segment (each segment one face, matched via <code>start</code>); otherwise each section becomes a closed B-spline starting at the +x ray from the centroid (as for circles/polygons), so nothing twists.</p>
<h3>Examples (<code>tests/models/spec/skizze_*.scad</code>)</h3>
<div class="codefig">${pre(src("tests/models/spec/skizze_profil.scad"))}<div>${fig(cli("profil"), "skizze_profil.scad: bracket (arcs, bore in reverse direction, slot via `hole`), knob (tangential arc, cubic Bézier)")}${fig(cli("profil-oben"), "top view")}</div></div>
<div class="codefig">${pre(src("tests/models/spec/skizze_zahnriemenscheibe.scad"))}<div>${fig(cli("scheibe"), "skizze_zahnriemenscheibe.scad: 20 tooth gaps via a loop (arc through a point + arc around the center), hub as a second contour")}${fig(cli("scheibe-oben"), "top view")}</div></div>
<div class="codefig">${pre(src("tests/models/spec/skizze_sweep.scad"))}${fig(cli("sweep"), "skizze_sweep.scad: pipe bend (smooth path: one curve chain, annulus as profile), frame along a path with corners (miter)")}</div>

<h2>3 Modules as namespaces</h2>
<p><code>module name { … }</code> without parentheses; calls <code>a.b.c(…)</code>, also named (<code>k: teile.schraube();</code>). The first part is looked up lexically, each further part only directly in the body of the previous module. The body is the namespace, with the parameters of the outer module at their default values; it belongs to no instance – the inner module does not see the grids, named instances and anchors of the outer one, and the geometry of the outer module is not created. <b>Decision on functions/constants:</b> reachable (<code>teile.kopf(d)</code>, <code>teile.M3</code>) – libraries bundle helper functions and dimensions with their modules without a separate file; OpenSCAD has no dot after module names (no conflict); a variable of the same name shadows the module. Context: <code>cad --module teile.platte.lochplatte</code>, UI breadcrumb "teile › rohr" (S05).</p>
<div class="codefig">${pre(src("tests/models/spec/skizze_bibliothek.scad"))}<div>${fig(cli("bibliothek"), "skizze_bibliothek.scad: profile.stern(), profile.langloch(), teile.platte.lochplatte(n = 4), teile.schraube(), disc with d = teile.kopf(teile.M3)")}${pre(T("outline.txt").split("\n").slice(0, 12).join("\n"))}${pre(T("check-modul.txt"))}</div></div>

<h2>4 Kernel integration</h2>
<ul>
<li><code>cad_kernel::Segment::{Line, Arc, Nurbs}</code> (<code>curve2.rs</code>): NURBS form, affine mapping (arcs stay arcs under similarity transforms, otherwise exact rational elliptical arcs), sampling, global interpolation, joining into one B-spline (Bézier decomposition, degree elevation); <code>Loop::Composite</code>, <code>PlacedPath</code>, <code>Kernel::sweep_path</code>.</li>
<li>Facade <code>smlib/facade/sc_sketch.cpp</code>, included at the end of <code>sc_facade.cpp</code> (uses its internal helpers; compiled on its own it is empty). Loop kind 3: lines → <code>SmLine</code>, arcs → rational quadratic B-splines (<code>SM_CF_CIRCULAR_ARC</code>), Bézier/splines → B-splines; joints welded exactly. Changes to existing functions: one field in <code>Loop</code>, one branch each in <code>parse_loops</code>/<code>loop_curves</code>, the condition in the twist branch of <code>sc_extrude</code>, <code>loft_segmentwise</code> in <code>sc_loft</code>. New: <code>sc_sketch_sweep</code> (smooth path: curve sweep with profile reference point from patch 0003; corners: miter).</li>
<li>STEP: SMLib stores extruded arcs as <code>SmSurfOfExtrusion</code> of the exact B-spline; on such surfaces (without p-curves) OpenCASCADE computed volumes that were wrong by up to 2.5·10<sup>−3</sup>. The writer therefore writes an extrusion of a circular arc along its axis as <code>CYLINDRICAL_SURFACE</code> (orientation adjusted) – after that OCCT agrees to 10<sup>−9</sup> or to the kernel accuracy, respectively.</li>
</ul>
<table>
<tr><th>Model</th><th>Faces</th><th>CIRCLE</th><th>LINE</th><th>B-spline curves</th><th>Surface types</th><th>approximated</th></tr>
${steps.map((s) => `<tr><td>${esc(s.name)}</td><td class="n">${esc(s.info.faces)}</td><td class="n">${s.cnt.CIRCLE}</td><td class="n">${s.cnt.LINE}</td><td class="n">${s.cnt.B_SPLINE_CURVE_WITH_KNOTS}</td><td>${esc(Object.entries(s.info.surfaces ?? {}).map(([k, v]) => `${k} ${v}`).join(", "))}</td><td class="n">${esc(s.info.approximated_edges)} / ${esc(s.info.approximated_faces)}</td></tr>`).join("\n")}
</table>
<p>OpenCASCADE (<code>tests/step/verify.py</code>) for the new examples (columns: bodies, faces, V kernel, V OCCT, dV, dA, dBox):</p>
${pre(stepSketch.join("\n"))}

<h2>5 Measurements: volumes against formulas</h2>
<p>One small file per case (<code>reports/tmp/report14/cases/</code>), <code>cad measure --volume</code> with the release build (time: whole CLI call without daemon). The geometry is exact; the deviation is that of the volume integration in SMLib.</p>
<table>
<tr><th>Case</th><th>Kernel mm³</th><th>Formula mm³</th><th>rel. dev.</th><th>ms</th></tr>
${volumes.map((v) => `<tr><td>${md(VNAMES[v.name] ?? v.name)}</td><td class="n">${num(v.got, 4)}</td><td class="n">${num(v.want, 4)}</td><td class="n">${sci(v.rel)}</td><td class="n">${num(v.ms)}</td></tr>`).join("\n")}
</table>
<p class="small">twist: the extrusion with twist is (as before for all profiles) a cubic loft approximation through sections every 7.5°.</p>

<h2>6 Errors per segment line</h2>
${pre(T("cases/fehler.scad"))}
${pre(T("check-fehler.txt"))}
<p>Checked are: contour not closed (the sketch stays visible with a red dashed closing line), self-intersection (reported at the later line, with a reference), impossible arc (radius &lt; half chord, three collinear points, tangential along the tangent, angle 0 or &gt; 360°), contour without area, segment before <code>start</code>/after <code>close()</code>. A self-intersecting contour is additionally reported by the kernel at the extrusion.</p>

<h2>7 UI: display and handles</h2>
<p>When the cursor is in a sketch (segment line, loop, <code>hole</code>, call of a sketch module), the view draws it in its plane: mesh as an outline with a light fill (visible through the body), segments as lines, the segment of the cursor line thick in the accent color, errors red, control polygons and arc centers dashed, the start as a ring with a direction arrow, the end of open contours as a square. Handles: end/start points (literal or grid snap; <code>line(by = …)</code> relative, without snap), arc radius, center, through point, Bézier control points, spline points. Checks via the UI state (<code>window.__cad</code>) and pixels of the layers.</p>
${scenarios.map((s) => `<h3>${esc(s.id)} ${esc(s.title)}</h3><p>${md(s.caption)}</p>${(s.images ?? []).length === 2 ? pair(pw(s.images[0].name), s.images[0].label, pw(s.images[1].name), s.images[1].label) : (s.images ?? []).map((i) => fig(pw(i.name), i.label)).join("")}${s.code && s.code.split("\n").length <= 16 ? pre(s.code) : ""}<ul>${(s.checks ?? []).map((c) => `<li>${md(c)}</li>`).join("")}</ul>`).join("\n")}

<h2>8 Test runs</h2>
<table>
<tr><th>Run</th><th>Result</th></tr>
<tr><td><code>cargo test --workspace --no-fail-fast</code></td><td>${cargoSum ? `${cargoSum.ok} passed, ${cargoSum.failed} failed` : "–"}</td></tr>
<tr><td><code>cargo clippy --workspace --all-targets -- -D warnings</code> · <code>cargo fmt --check</code></td><td>${clippyOk ? "clean" : "finding"} · ${fmtOk ? "clean" : "difference"}</td></tr>
<tr><td><code>pnpm typecheck</code> · <code>pnpm test</code> (Vitest)</td><td>${readText(join(input, "typecheck.txt")) !== null ? (/error TS/i.test(readText(join(input, "typecheck.txt"))) ? "error" : "clean") : "–"} · ${vitestSum ?? "–"} passed</td></tr>
<tr><td><code>pnpm e2e</code> (Playwright, Chromium)</td><td>${pwStats ? `${pwStats.passed} / ${pwStats.total}${pwStats.failed.length ? ` – failed: ${pwStats.failed.map((f) => esc(f.title)).join(", ")}` : ""}` : "–"}${flaky ? ` · ${md(flaky.trim())}` : ""}</td></tr>
<tr><td><code>tests/openscad-compat/compare.py --eval</code></td><td>${compareSum ? `${compareSum[0]}/${compareSum[1]}` : "–"}</td></tr>
<tr><td><code>tests/step/verify.py</code> (OpenCASCADE)</td><td>${esc(stepSum ?? "–")}</td></tr>
<tr><td><code>tests/alltag/check.sh</code></td><td>${esc(alltagLine ?? "–")}</td></tr>
<tr><td><code>node crates/kernel-smlib/js/test-node.mjs</code></td><td>${nodeTest && /all node tests passed/.test(nodeTest) ? "passed" : "–"}</td></tr>
</table>
<p>New or extended Rust tests (${newTests.length}):</p>
<p class="small">${newTests.map((t) => `<code>${esc(t.name)}</code> ${t.res === "ok" ? '<span class="ok">ok</span>' : '<span class="bad">FAILED</span>'}`).join(" · ")}</p>
<p class="small">Vitest: <code>web/src/logic/sketch.test.ts</code> (sketch under the cursor, helper lines, segment handles, module with path); Lezer grammar for <code>sketch {</code>, modules without parentheses and <code>a.b()</code>. CLI: <code>sketch_and_namespaces</code> (volume, function in a namespace, <code>outline</code>, <code>--module teile.platte</code>, errors per line).</p>

<h2>9 Limitations</h2>
<ul>
<li><code>corner</code>, <code>fillet2d</code>, <code>offset</code>, <code>stroke</code> are missing (step 2); no transformations inside a sketch (a module called in a sketch draws in sketch coordinates).</li>
<li>Self-intersection is checked on sampled polylines (tolerance relative to the sketch size); intersections between contours are allowed (nonzero); the nesting of holes and islands assumes non-intersecting contours.</li>
<li>Sweep along a sketch with corners only as a miter and with the profile centered on the origin; closed paths are not supported.</li>
<li>Loft with different segment counts joins each section into one curve (C0 at the joints): the faces no longer follow the segments.</li>
<li>STEP: only extrusions of circular arcs are written as cylinders; surfaces of revolution of arcs remain <code>SURFACE_OF_REVOLUTION</code> of the exact curve (sphere/torus not recognized; OCCT reads them correctly).</li>
<li><code>cad render --line N</code> of a segment line highlights the whole sketch; highlighting of the single segment exists only in the UI.</li>
<li>Namespaces: constants are evaluated in a new namespace on each access (no caching); <code>--module name</code> without a path takes the first module of that name.</li>
</ul>
</main>
</body>
</html>
`;
writeFileSync(join(out, "report.html"), html);
console.log(`${join(out, "report.html")}: ${html.length} characters, ${scenarios.length} scenarios, ${volumes.length} volume cases`);
