#!/usr/bin/env node
// Test report 2 ("UI Phase A"): collects the results of the UI tests and writes
//
//   docs/reports/02-ui-phase-a/report.html, report.pdf, img/*.jpg
//
// Inputs (written by the test runs, gitignored under reports/tmp/report2/):
//   playwright.json   Playwright JSON reporter (web/playwright.config.ts)
//   vitest.json       Vitest JSON reporter (pnpm test --reporter=json --outputFile=…)
//   scenarios.json    title/caption/screenshot per e2e scenario (web/e2e/helpers.ts)
//   metrics.json      load time, edit → scene, fps (web/e2e/ui.spec.ts)
//   img/*.jpg         screenshots
//
//   cd web && pnpm test:report      (runs Vitest, Playwright, then this script)
//   node tools/report/report2-ui.mjs [--run]
//
// Without inputs the script runs what is missing itself: Vitest (vitest.json) and the Phase A
// scenarios (web/e2e/ui.spec.ts) when playwright.json has none of them – it is shared with the
// other reports and holds only the last Playwright run. `--run` reruns both.
//
// The PDF is printed with Chromium (page.pdf, A4, backgrounds on).

import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import os from "node:os";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "../..");
const web = join(root, "web");
const input = join(root, "reports/tmp/report2");
const out = join(root, "docs/reports/02-ui-phase-a");
const require = createRequire(join(web, "package.json"));

const readJson = (f, d = null) => (existsSync(f) ? JSON.parse(readFileSync(f, "utf8")) : d);
const esc = (s) => String(s ?? "").replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
const fmtMs = (v) => (typeof v === "number" ? `${Math.round(v)} ms` : "–");
const median = (a) => (a && a.length ? [...a].sort((x, y) => x - y)[Math.floor(a.length / 2)] : null);
const git = (...a) => {
  try {
    return execFileSync("git", a, { cwd: root, encoding: "utf8" }).trim();
  } catch {
    return "–";
  }
};
const pkgVersion = (name) => {
  try {
    return JSON.parse(readFileSync(join(web, "node_modules", name, "package.json"), "utf8")).version;
  } catch {
    return "–";
  }
};

// ---------------------------------------------------------------- inputs

const rerun = process.argv.includes("--run");
const sh = (args) => {
  try {
    execFileSync("pnpm", ["exec", ...args], { cwd: web, stdio: "inherit" });
  } catch {
    /* failed tests still write their results */
  }
};
const hasPhaseA = (j) => JSON.stringify(j?.suites ?? []).includes('"ui.spec.ts"');
mkdirSync(input, { recursive: true });
if (rerun || !existsSync(join(input, "vitest.json"))) {
  console.log("report2: Vitest …");
  sh(["vitest", "run", "--reporter=default", "--reporter=json", `--outputFile.json=${join(input, "vitest.json")}`]);
}
if (rerun || !hasPhaseA(readJson(join(input, "playwright.json")))) {
  console.log("report2: Playwright (web/e2e/ui.spec.ts) …");
  sh(["playwright", "test", "e2e/ui.spec.ts"]);
}
const pw = readJson(join(input, "playwright.json"));
const vt = readJson(join(input, "vitest.json"));
const scenarios = readJson(join(input, "scenarios.json"), []);
const metrics = readJson(join(input, "metrics.json"), {});
if (!pw || !hasPhaseA(pw)) {
  console.error(`missing: results of web/e2e/ui.spec.ts in ${join(input, "playwright.json")} (cd web && pnpm e2e)`);
  process.exit(1);
}

/** Playwright tests flattened: { id, title, status, ms }. */
function flatten(suite, acc = []) {
  // Only the Phase A scenarios (web/e2e/ui.spec.ts); Phase B has its own report.
  if (suite.file && !suite.file.endsWith("ui.spec.ts")) return acc;
  for (const s of suite.suites ?? []) flatten(s, acc);
  for (const spec of suite.specs ?? []) {
    for (const t of spec.tests ?? []) {
      const r = t.results?.at(-1);
      const id = spec.title.split(" ")[0];
      acc.push({ id, title: spec.title.slice(id.length + 1), status: r?.status ?? t.status ?? "unknown", ms: r?.duration ?? 0, error: r?.error?.message ?? null });
    }
  }
  return acc;
}
const e2e = flatten(pw);
const e2ePassed = e2e.filter((t) => t.status === "passed").length;

const unit = [];
if (vt) {
  for (const f of vt.testResults ?? []) {
    const name = f.name.replace(/^.*\/web\//, "web/");
    const res = f.assertionResults ?? [];
    unit.push({ file: name, total: res.length, passed: res.filter((r) => r.status === "passed").length, failed: res.filter((r) => r.status === "failed").map((r) => r.fullName) });
  }
}
const unitTotal = unit.reduce((s, f) => s + f.total, 0);
const unitPassed = unit.reduce((s, f) => s + f.passed, 0);

// ---------------------------------------------------------------- images

rmSync(join(out, "img"), { recursive: true, force: true });
mkdirSync(join(out, "img"), { recursive: true });
const images = [];
for (const s of scenarios) {
  if (!s.image) continue;
  const src = join(input, "img", `${s.image}.jpg`);
  if (!existsSync(src)) continue;
  copyFileSync(src, join(out, "img", `${s.image}.jpg`));
  images.push(s);
}
// Further screenshots of the scenarios (a, b, c variants) that are not the main image.
const extra = existsSync(join(input, "img"))
  ? readdirSync(join(input, "img"))
      .filter((f) => f.endsWith(".jpg") && !images.some((s) => `${s.image}.jpg` === f))
      .sort()
  : [];
for (const f of extra) copyFileSync(join(input, "img", f), join(out, "img", f));
const sizes = readdirSync(join(out, "img")).map((f) => statSync(join(out, "img", f)).size);
const maxImg = Math.max(0, ...sizes);

// ---------------------------------------------------------------- environment

const { chromium } = require("@playwright/test");
const browser = await chromium.launch({ args: ["--use-angle=swiftshader", "--enable-unsafe-swiftshader"] });
const chromiumVersion = browser.version();

const distAssets = existsSync(join(web, "dist/assets")) ? readdirSync(join(web, "dist/assets")) : [];
const asset = (re) => {
  const f = distAssets.find((x) => re.test(x));
  return f ? statSync(join(web, "dist/assets", f)).size : null;
};
const kib = (b) => (b === null ? "–" : `${(b / 1024).toFixed(0)} KiB`);
const mib = (b) => (b === null ? "–" : `${(b / 1024 / 1024).toFixed(2)} MiB`);

const env = [
  ["Operating system", `${os.type()} ${os.release()} (${os.arch()})`],
  ["CPU", `${os.cpus()[0]?.model ?? "–"} · ${os.cpus().length} cores`],
  ["Node.js", process.version],
  ["Browser", `Chromium ${chromiumVersion} (headless, Playwright ${pkgVersion("@playwright/test")})`],
  ["Graphics", "WebGL via SwiftShader (software rendering, no GPU)"],
  ["Window", "1440 × 900 px, device pixel ratio 1"],
  ["Build", `Vite ${pkgVersion("vite")} production build, preview server (vite preview)`],
  ["Libraries", `React ${pkgVersion("react")}, three.js ${pkgVersion("three")}, CodeMirror ${pkgVersion("@codemirror/view")}, Zustand ${pkgVersion("zustand")}, Vitest ${pkgVersion("vitest")}`],
  ["Engine", `cad_wasm_bg.wasm ${mib(asset(/^cad_wasm_bg-.*\.wasm$/))}, smlib.wasm ${mib(asset(/^smlib-.*\.wasm$/))} (each once in the build)`],
  ["Source", `Branch ${git("rev-parse", "--abbrev-ref", "HEAD")}, commit ${git("rev-parse", "--short", "HEAD")}`],
];

// ---------------------------------------------------------------- html

const date = new Date().toISOString().slice(0, 10);
const caption = (id) => scenarios.find((s) => s.id === id);

const statusLabel = (s) => (s === "passed" ? "passed" : s === "skipped" ? "skipped" : "failed");
const scenarioRows = e2e
  .map((t) => {
    const c = caption(t.id);
    return `<tr class="${t.status === "passed" ? "ok" : "bad"}"><td>${esc(t.id)}</td><td>${esc(t.title)}</td><td class="res">${statusLabel(t.status)}</td><td class="num">${(t.ms / 1000).toFixed(1)} s</td><td>${esc(c?.caption ?? "")}${t.error ? `<div class="err">${esc(t.error.split("\n")[0])}</div>` : ""}</td></tr>`;
  })
  .join("\n");

const unitRows = unit
  .map((f) => `<tr class="${f.failed.length ? "bad" : "ok"}"><td>${esc(f.file)}</td><td class="num">${f.passed} / ${f.total}</td><td>${f.failed.map(esc).join("<br>") || "–"}</td></tr>`)
  .join("\n");

const m = metrics;
const metricRows = [
  ["Load time until first scene", fmtMs(m.firstSceneMs), "Navigation until the first draw of the complete scene of housing.mcad (loading Wasm, evaluation, tessellation, rendering)"],
  ["First evaluation", fmtMs(m.firstEvalMs), "SetSource until complete scene (cold cache)"],
  ["Keystroke → new scene (median)", fmtMs(m.editToSceneMedianMs), `In-app measurement: last change until the new scene is drawn; individual values ${(m.editToSceneMs ?? []).map((v) => Math.round(v)).join(", ")} ms`],
  ["Keystroke → new scene, test view (median)", fmtMs(m.keyToSceneE2eMedianMs), "Playwright: keystroke until the scene is visible in the test hook (incl. 50 ms polling interval)"],
  ["Evaluation (median)", fmtMs(m.evalMsMedian), "SetSource until complete scene during the changes (warm cache)"],
  ["Frame rate while rotating", typeof m.orbitFps === "number" ? `${m.orbitFps.toFixed(0)} fps` : "–", "120 frames of camera rotation around the z axis, one full render per frame (SwiftShader)"],
  ["Largest image file in the report", kib(maxImg), "JPEG, quality 82"],
]
  .map(([a, b, c]) => `<tr><td>${esc(a)}</td><td class="num">${esc(b)}</td><td>${esc(c)}</td></tr>`)
  .join("\n");

const figure = (s) => `<figure><img src="img/${esc(s.image)}.jpg" alt="${esc(s.title)}"><figcaption><b>${esc(s.id)} ${esc(s.title)}.</b> ${esc(s.caption)}</figcaption></figure>`;
const extraCaptions = {
  "s03a-cursor-deckel": "Cursor on deckel() in line 22: module call full, rest dimmed.",
  "s07b-doppelklick": "After a double-click on the enclosure: cursor in module gehaeuse, view in module context.",
  "s08b-datei": "After a click on \"File\" in the breadcrumb bar: cursor on k: gehaeuse(w = 60), whole file.",
  "s09b-root-modifier": "25_root_modifier.scad: only the ! subtree is shown, hint in the status bar.",
  "s13a-raster-zeile": "Grid with only the planes of line 12 (box between grid points).",
  "s13c-raster-hover": "Hover over g[loch[i], mitte, boden]: expression highlighted, grid point highlighted (in loops, the point of the first iteration).",
  "s20b-profil": "27_spacers_2d.scad, top view: cursor on square([30, 10]) – 2D shape as outline with a light fill, rest dimmed.",
};
const extraFigure = (n) => `<figure><img src="img/${esc(n)}.jpg" alt="${esc(n)}"><figcaption>${esc(extraCaptions[n] ?? n)}</figcaption></figure>`;
/** Main screenshot of each scenario followed by its further screenshots (same sNN prefix). */
const figures = images
  .map((s) => {
    const prefix = s.image.match(/^s\d+/)?.[0];
    const more = extra.map((f) => f.replace(/\.jpg$/, "")).filter((n) => n.match(/^s\d+/)?.[0] === prefix);
    return [figure(s), ...more.map(extraFigure)].join("\n");
  })
  .join("\n");

const html = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Test report 2 – UI phase A</title>
<style>
  @page { size: A4; margin: 16mm 14mm 16mm 14mm; }
  :root { --text: #1f2328; --muted: #5d646e; --border: #d5d9df; --ok: #1a7f37; --bad: #c9302c; --head: #f2f4f7; }
  * { box-sizing: border-box; }
  body { font: 10pt/1.45 system-ui, -apple-system, "Segoe UI", Roboto, sans-serif; color: var(--text); margin: 0; }
  main { max-width: 182mm; margin: 0 auto; }
  h1 { font-size: 20pt; margin: 0 0 2mm; }
  h2 { font-size: 13pt; margin: 8mm 0 3mm; border-bottom: 1px solid var(--border); padding-bottom: 1mm; }
  .meta { color: var(--muted); margin-bottom: 5mm; }
  .summary { display: flex; gap: 4mm; margin: 4mm 0; }
  .summary div { flex: 1; border: 1px solid var(--border); border-radius: 2mm; padding: 3mm; }
  .summary b { display: block; font-size: 15pt; }
  table { width: 100%; border-collapse: collapse; margin: 2mm 0; font-size: 8.8pt; }
  th, td { border: 1px solid var(--border); padding: 1.4mm 2mm; text-align: left; vertical-align: top; }
  th { background: var(--head); font-weight: 600; }
  td.num { text-align: right; white-space: nowrap; }
  tr.ok td.res { color: var(--ok); font-weight: 600; }
  tr.bad td.res, tr.bad td:first-child { color: var(--bad); font-weight: 600; }
  .err { color: var(--bad); font-size: 8pt; margin-top: 1mm; }
  ul { margin: 1mm 0 2mm 5mm; padding: 0; }
  li { margin: 0.6mm 0; }
  figure { margin: 0 0 6mm; break-inside: avoid; page-break-inside: avoid; }
  figure img { width: 100%; border: 1px solid var(--border); display: block; }
  figcaption { font-size: 8.8pt; color: var(--muted); margin-top: 1.5mm; }
  figure img { max-height: 112mm; object-fit: contain; }
  .small { font-size: 8.8pt; color: var(--muted); }
  tr, td { break-inside: avoid; page-break-inside: avoid; }
</style>
</head>
<body>
<main>
<h1>Test report 2 – UI phase A</h1>
<div class="meta">Browser UI of the parametric CAD (TypeScript/React, CodeMirror 6, three.js) · ${esc(date)} · Commit ${esc(git("rev-parse", "--short", "HEAD"))}</div>

<div class="summary">
  <div>End-to-end (Playwright)<b>${e2ePassed} / ${e2e.length} passed</b></div>
  <div>Unit tests (Vitest)<b>${vt ? `${unitPassed} / ${unitTotal} passed` : "not run"}</b></div>
  <div>First scene<b>${fmtMs(m.firstSceneMs)}</b></div>
  <div>Keystroke → scene<b>${fmtMs(m.editToSceneMedianMs)}</b></div>
</div>

<h2>1 Scope</h2>
<p>This report covers the browser UI after phase A: the editor on the left, the 3D view with breadcrumb bar on the right, the status bar at the bottom. The engine (Rust, Wasm, SMLib kernel) runs in a web worker; the UI only interprets the scene description and loads meshes per node on demand.</p>
<ul>
  <li><b>Editor:</b> Lezer grammar (OpenSCAD with <code>grid</code>, <code>anchor</code>, <code>k:</code>), diagnostics (red underline, line red or yellow, message at the end of the line), inline hints (values, "4 edges"), <code>*</code> lines grayed out, positions of old scenes via <code>ChangeSet.mapPos</code>.</li>
  <li><b>3D view:</b> meshes per node with a cache keyed by <code>mesh_key</code>, placement via <code>display_world</code>, edges as lines, rotate/pan/zoom, "Show all", standard views, axis triad, ground.</li>
  <li><b>Code → view:</b> cursor (node full, rest dimmed), hover over a line, difference/intersection/fillet/chamfer/shell, modifiers <code>#</code> <code>%</code> <code>!</code> <code>*</code>, transformations with coordinate triads before/after and an outline at the original position, 2D shapes as outline with a light fill.</li>
  <li><b>View → code:</b> hovering over a face highlights the line, a click sets the cursor, a double-click on a module instance jumps to the definition.</li>
  <li><b>Module context:</b> <code>SetContext</code> when the cursor is in a module definition, breadcrumb bar with a jump one level up.</li>
  <li><b>Grids and anchors (read-only):</b> planes at the edge of the bounding box, intersection points, literal solid / expression dashed, toggle "all grids", hover over a grid expression, anchors as diamonds with an arrow.</li>
  <li><b>Files and errors:</b> autosave to IndexedDB, open/save, examples menu, export STL/3MF, syntax errors and watchdog.</li>
</ul>

<h2>2 Test environment</h2>
<table>${env.map(([a, b]) => `<tr><th style="width:32mm">${esc(a)}</th><td>${esc(b)}</td></tr>`).join("")}</table>
<p class="small">The end-to-end tests run against the production build. They wait for scene versions and the display state via the test hook <code>window.__cad</code>, not for fixed times. Hover and click in the view are real mouse events on the canvas; the hook only supplies a pixel under which a face of the requested line lies.</p>

<h2>3 Scenarios (Playwright)</h2>
<table>
<tr><th>ID</th><th>Scenario</th><th>Result</th><th>Duration</th><th>Check</th></tr>
${scenarioRows}
</table>

<h2>4 Unit tests (Vitest)</h2>
${vt ? `<table><tr><th>File</th><th>passed</th><th>Failed</th></tr>${unitRows}</table>
<p class="small">Logic without a browser: Lezer grammar on all examples, mapping code ↔ node (cursor, face → line, ambiguity, module context), display states, grid display, breadcrumb bar and jump to definition, editor decorations, mapping of old positions. Scenes as fixtures from the real engine (<code>web/scripts/dump-scene.mjs</code>).</p>` : "<p>No Vitest results found.</p>"}

<h2>5 Measurements</h2>
<table><tr><th>Quantity</th><th>Value</th><th>Measurement</th></tr>${metricRows}</table>
<p class="small">Software rendering without GPU; on a desktop with a GPU the frame rates are considerably higher. The times include the evaluation in the worker (language + kernel) and loading the meshes.</p>

<h2>6 Known limitations and deviations</h2>
<ul>
  <li>Handles, dragging, snapping and the parameter bar are not part of phase A; grids and anchors are only displayed.</li>
  <li>Hover over edges is resolved via the adjacent face (edges carry no node tags). Union children get a thin outline on hover, but not with the cursor.</li>
  <li>When the module context changes, the UI resends the source with a new version so that each scene version denotes exactly one evaluation (meshes are requested per version). This costs one additional language evaluation of a few milliseconds.</li>
  <li>In case of ambiguity (same content at several places, e.g. two lids), the location of the hit point within the node bounding boxes is used to disambiguate; if it remains ambiguous, all lines are highlighted.</li>
  <li>Grid expressions in loops (e.g. <code>g[loch[i], mitte, boden]</code>) are reported once by the scene; on hover the point of the first iteration is highlighted.</li>
  <li>The editor wraps long lines; hints at the end of a line can therefore move to a second screen line.</li>
  <li>The watchdog test blocks the worker via a test message; a real hanging kernel call was not reproduced.</li>
  <li>The measurements come from a headless Chromium with SwiftShader and are only meaningful relative to each other.</li>
</ul>

<div class="figs">
<h2>7 Screenshots</h2>
${figures}
</div>
</main>
</body>
</html>
`;

mkdirSync(out, { recursive: true });
writeFileSync(join(out, "report.html"), html);

const page = await browser.newPage();
await page.goto(pathToFileURL(join(out, "report.html")).href, { waitUntil: "load" });
await page.pdf({
  path: join(out, "report.pdf"),
  format: "A4",
  printBackground: true,
  displayHeaderFooter: true,
  headerTemplate: "<span></span>",
  footerTemplate: '<div style="font-size:7pt;color:#777;width:100%;text-align:center">Test report 2 – UI phase A · Page <span class="pageNumber"></span> of <span class="totalPages"></span></div>',
  margin: { top: "14mm", bottom: "16mm", left: "12mm", right: "12mm" },
});
await browser.close();
console.log(`report2: ${e2ePassed}/${e2e.length} e2e, ${unitPassed}/${unitTotal} unit, ${images.length + extra.length} images (max ${kib(maxImg)})`);
console.log(`-> ${join(out, "report.html")}`);
console.log(`-> ${join(out, "report.pdf")}`);
