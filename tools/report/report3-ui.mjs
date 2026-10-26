#!/usr/bin/env node
// Test report 3 ("UI phase B: handles, snapping, parameter bar"): collects the results of the
// Phase B tests and writes
//
//   docs/reports/03-ui-phase-b/report.html, report.pdf, img/*.jpg
//
// Inputs (written by the test runs, gitignored under reports/tmp/):
//   report2/playwright.json   Playwright JSON reporter (web/playwright.config.ts); only
//                             web/e2e/ui-b.spec.ts is reported here, ui.spec.ts as regression
//   report3/vitest.json       Vitest JSON reporter
//   report3/cargo.txt         output of the Rust tests of the changed crates
//   report3/scenarios.json    title/caption/screenshots/code per scenario (web/e2e/helpers.ts)
//   report3/metrics.json      drag latency and frame rate (web/e2e/ui-b.spec.ts, B14)
//   report3/img/*.jpg         screenshots
//
//   cd web && pnpm test:report3     (Rust tests, Vitest, Playwright, then this script)
//   node tools/report/report3-ui.mjs
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
const input = join(root, "reports/tmp/report3");
const pwFile = join(root, "reports/tmp/report2/playwright.json");
const out = join(root, "docs/reports/03-ui-phase-b");
const require = createRequire(join(web, "package.json"));

const readJson = (f, d = null) => (existsSync(f) ? JSON.parse(readFileSync(f, "utf8")) : d);
const esc = (s) => String(s ?? "").replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
const fmtMs = (v) => (typeof v === "number" ? `${Math.round(v)} ms` : "–");
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

const pw = readJson(pwFile);
const vt = readJson(join(input, "vitest.json"));
const scenarios = readJson(join(input, "scenarios.json"), []);
const metrics = readJson(join(input, "metrics.json"), {});
const cargo = existsSync(join(input, "cargo.txt")) ? readFileSync(join(input, "cargo.txt"), "utf8") : null;
if (!pw) {
  console.error(`missing: ${pwFile} (first: cd web && pnpm e2e)`);
  process.exit(1);
}

/** Playwright tests of one spec file flattened: { id, title, status, ms }. */
function flatten(suite, file, acc = []) {
  if (suite.file && !suite.file.endsWith(file)) return acc;
  for (const s of suite.suites ?? []) flatten(s, file, acc);
  for (const spec of suite.specs ?? []) {
    if (spec.file && !spec.file.endsWith(file)) continue;
    for (const t of spec.tests ?? []) {
      const r = t.results?.at(-1);
      const id = spec.title.split(" ")[0];
      acc.push({ id, title: spec.title.slice(id.length + 1), status: r?.status ?? t.status ?? "unknown", ms: r?.duration ?? 0, error: r?.error?.message ?? null });
    }
  }
  return acc;
}
const e2e = flatten(pw, "ui-b.spec.ts");
const phaseA = flatten(pw, "ui.spec.ts");
const e2ePassed = e2e.filter((t) => t.status === "passed").length;
const aPassed = phaseA.filter((t) => t.status === "passed").length;
if (!e2e.length) {
  console.error("no results of web/e2e/ui-b.spec.ts in playwright.json");
  process.exit(1);
}

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
const NEW_UNIT = new Set(["web/src/logic/format.test.ts", "web/src/logic/handles.test.ts", "web/src/logic/drag.test.ts", "web/src/logic/params.test.ts", "web/src/editor/editGroup.test.ts"]);

/** Rust tests: `test name ... ok` lines grouped by test binary. */
const rust = [];
if (cargo) {
  let cur = null;
  for (const l of cargo.split("\n")) {
    const run = l.match(/Running (?:unittests )?(\S+)/);
    if (run) {
      cur = { bin: run[1].replace(/\s*\(.*$/, ""), tests: [] };
      rust.push(cur);
      continue;
    }
    const t = l.match(/^test (\S+) \.\.\. (ok|FAILED|ignored)/);
    if (t && cur) cur.tests.push({ name: t[1], status: t[2] });
  }
}
const rustBins = rust.filter((b) => b.tests.length);
const rustTotal = rustBins.reduce((s, b) => s + b.tests.length, 0);
const rustOk = rustBins.reduce((s, b) => s + b.tests.filter((t) => t.status === "ok").length, 0);
const NEW_RUST = new Set([
  "computed_values_have_locked_handles",
  "one_d_handles_snap_to_grid_planes",
  "plane_snap_respects_visibility",
  "zone_center_handle_through_translate",
  "drag_always_answers_and_plane_snap",
]);

// ---------------------------------------------------------------- images

rmSync(join(out, "img"), { recursive: true, force: true });
mkdirSync(join(out, "img"), { recursive: true });
const used = new Set();
for (const s of scenarios) for (const k of [s.before, s.after]) if (k) used.add(k);
for (const n of used) {
  const src = join(input, "img", `${n}.jpg`);
  if (existsSync(src)) copyFileSync(src, join(out, "img", `${n}.jpg`));
}
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
  ["Engine", `cad_wasm_bg.wasm ${mib(asset(/^cad_wasm_bg-.*\.wasm$/))}, smlib.wasm ${mib(asset(/^smlib-.*\.wasm$/))}`],
  ["Source", `Branch ${git("rev-parse", "--abbrev-ref", "HEAD")}, commit ${git("rev-parse", "--short", "HEAD")}`],
];

// ---------------------------------------------------------------- html

const date = new Date().toISOString().slice(0, 10);
const scen = (id) => scenarios.find((s) => s.id === id);
const statusLabel = (s) => (s === "passed" ? "passed" : s === "skipped" ? "skipped" : "failed");

const scenarioRows = e2e
  .map((t) => {
    const c = scen(t.id);
    return `<tr class="${t.status === "passed" ? "ok" : "bad"}"><td>${esc(t.id)}</td><td>${esc(t.title)}</td><td class="res">${statusLabel(t.status)}</td><td class="num">${(t.ms / 1000).toFixed(1)} s</td><td>${esc(c?.caption ?? "")}${t.error ? `<div class="err">${esc(t.error.split("\n")[0])}</div>` : ""}</td></tr>`;
  })
  .join("\n");

const unitRows = unit
  .map((f) => `<tr class="${f.failed.length ? "bad" : "ok"}"><td>${esc(f.file)}${NEW_UNIT.has(f.file) ? " <span class=\"tag\">new</span>" : ""}</td><td class="num">${f.passed} / ${f.total}</td><td>${f.failed.map(esc).join("<br>") || "–"}</td></tr>`)
  .join("\n");

const rustRows = rustBins
  .map((b) => {
    const fresh = b.tests.filter((t) => NEW_RUST.has(t.name)).map((t) => t.name);
    const bad = b.tests.filter((t) => t.status === "FAILED").map((t) => t.name);
    const name = b.bin.replace(/^.*\/deps\//, "").replace(/-[0-9a-f]{16}$/, "");
    return `<tr class="${bad.length ? "bad" : "ok"}"><td>${esc(name)}</td><td class="num">${b.tests.filter((t) => t.status === "ok").length} / ${b.tests.length}</td><td>${fresh.map((n) => `<code>${esc(n)}</code>`).join(", ") || "–"}${bad.length ? `<div class="err">${bad.map(esc).join(", ")}</div>` : ""}</td></tr>`;
  })
  .join("\n");

const d = metrics.drag ?? {};
const c = metrics.dragCube ?? {};

const metricRows = [
  ["Drag → scene, coarse (median)", fmtMs(d.coarse?.median), `Drag message until the scene of the source written from it is drawn, coarse tessellation (10 × tolerance); ${d.coarse?.n ?? "–"} scenes`],
  ["Drag → scene, coarse (p95 / max)", `${fmtMs(d.coarse?.p95)} / ${fmtMs(d.coarse?.max)}`, "95 % quantile and maximum of the same measurement"],
  ["Release → fine scene", fmtMs(d.fine?.last), "pointerup until the scene with the fine root mesh is drawn (DragEnd, closing the undo group, re-evaluation from the cache)"],
  ["Frame rate while dragging", typeof d.fps === "number" ? `${d.fps.toFixed(0)} fps` : "–", `requestAnimationFrame counter while dragging (${d.durationMs ? (d.durationMs / 1000).toFixed(1) : "–"} s); longest gap ${fmtMs(d.maxGapMs)}`],
  ["Mouse moves / drag messages", `${d.moves ?? "–"} / ${d.sent ?? "–"}`, `at most one message in flight, ${d.dropped ?? "–"} intermediate states skipped (last position wins)`],
  ["Light model: drag → coarse scene (median / p95)", `${fmtMs(c.coarse?.median)} / ${fmtMs(c.coarse?.p95)}`, `B01, ${c.model ?? "–"}: ${c.coarse?.n ?? "–"} scenes; cost of UI, worker and protocol without significant kernel work`],
  ["Light model: release → fine / fps", `${fmtMs(c.fine?.last)} / ${typeof c.fps === "number" ? c.fps.toFixed(0) : "–"} fps`, `longest frame gap ${fmtMs(c.maxGapMs)}, ${c.sent ?? "–"} drag messages`],
  ["Largest image file in the report", kib(maxImg), "JPEG, quality 82"],
]
  .map(([a, b, c]) => `<tr><td>${esc(a)}</td><td class="num">${esc(b)}</td><td>${esc(c)}</td></tr>`)
  .join("\n");

const code = (s) => (s ? `<pre>${esc(s)}</pre>` : "<pre class=\"none\">–</pre>");
const shotsFor = (s) => {
  const imgs = [s.before && { n: s.before, l: s.before.includes("waehrend") ? "during" : "before" }, s.after && { n: s.after, l: s.after.includes("waehrend") ? "during" : "after" }].filter(Boolean);
  if (!imgs.length) return "";
  return `<div class="pair">${imgs.map((i) => `<div><div class="lbl">${esc(i.l)}</div><img src="img/${esc(i.n)}.jpg" alt="${esc(s.title)} ${esc(i.l)}"></div>`).join("")}</div>`;
};
const blocks = e2e
  .map((t) => scen(t.id))
  .filter(Boolean)
  .map(
    (s) => `<section class="sc">
<h3>${esc(s.id)} ${esc(s.title)}</h3>
${shotsFor(s)}
${s.codeBefore || s.codeAfter ? `<div class="codes"><div><div class="lbl">Code before</div>${code(s.codeBefore)}</div><div><div class="lbl">Code after</div>${code(s.codeAfter)}</div></div>` : ""}
<p class="cap">${esc(s.caption)}</p>
</section>`,
  )
  .join("\n");

const html = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Test report 3 – UI phase B</title>
<style>
  @page { size: A4; margin: 16mm 14mm 16mm 14mm; }
  :root { --text: #1f2328; --muted: #5d646e; --border: #d5d9df; --ok: #1a7f37; --bad: #c9302c; --head: #f2f4f7; }
  * { box-sizing: border-box; }
  body { font: 10pt/1.45 system-ui, -apple-system, "Segoe UI", Roboto, sans-serif; color: var(--text); margin: 0; }
  main { max-width: 182mm; margin: 0 auto; }
  h1 { font-size: 20pt; margin: 0 0 2mm; }
  h2 { font-size: 13pt; margin: 8mm 0 3mm; border-bottom: 1px solid var(--border); padding-bottom: 1mm; }
  h3 { font-size: 11pt; margin: 0 0 2mm; }
  .meta { color: var(--muted); margin-bottom: 5mm; }
  .summary { display: flex; gap: 3mm; margin: 4mm 0; }
  .summary div { flex: 1; border: 1px solid var(--border); border-radius: 2mm; padding: 2.5mm; font-size: 9pt; }
  .summary b { display: block; font-size: 13pt; }
  table { width: 100%; border-collapse: collapse; margin: 2mm 0; font-size: 8.6pt; }
  th, td { border: 1px solid var(--border); padding: 1.3mm 2mm; text-align: left; vertical-align: top; }
  th { background: var(--head); font-weight: 600; }
  td.num { text-align: right; white-space: nowrap; }
  tr.ok td.res { color: var(--ok); font-weight: 600; }
  tr.bad td.res, tr.bad td:first-child { color: var(--bad); font-weight: 600; }
  .err { color: var(--bad); font-size: 8pt; margin-top: 1mm; }
  .tag { font-size: 7pt; color: #fff; background: #0c8599; border-radius: 1mm; padding: 0 1mm; }
  ul { margin: 1mm 0 2mm 5mm; padding: 0; }
  li { margin: 0.6mm 0; }
  code, pre { font-family: ui-monospace, "SFMono-Regular", Menlo, Consolas, monospace; font-size: 8pt; }
  pre { margin: 0; padding: 1.5mm 2mm; background: #f6f8fa; border: 1px solid var(--border); border-radius: 1mm; white-space: pre-wrap; word-break: break-word; }
  pre.none { color: var(--muted); }
  .sc { break-inside: avoid; page-break-inside: avoid; margin: 0 0 7mm; }
  .pair { display: flex; gap: 2mm; }
  .pair > div { flex: 1; min-width: 0; }
  .pair img { max-width: 100%; max-height: 76mm; width: auto; height: auto; border: 1px solid var(--border); display: block; }
  .lbl { font-size: 7.5pt; color: var(--muted); text-transform: uppercase; letter-spacing: 0.04em; margin: 1mm 0 0.5mm; }
  .codes { display: flex; gap: 2mm; margin-top: 1.5mm; }
  .codes > div { flex: 1; min-width: 0; }
  .cap { font-size: 8.8pt; color: var(--muted); margin: 1.5mm 0 0; }
  .small { font-size: 8.8pt; color: var(--muted); }
  tr, td { break-inside: avoid; page-break-inside: avoid; }
</style>
</head>
<body>
<main>
<h1>Test report 3 – UI phase B: handles, snapping, parameter bar</h1>
<div class="meta">Editing in the view · ${esc(date)} · Commit ${esc(git("rev-parse", "--short", "HEAD"))}</div>

<div class="summary">
  <div>Phase B (Playwright)<b>${e2ePassed} / ${e2e.length} passed</b></div>
  <div>Phase A (regression)<b>${phaseA.length ? `${aPassed} / ${phaseA.length}` : "–"}</b></div>
  <div>Vitest<b>${vt ? `${unitPassed} / ${unitTotal}` : "–"}</b></div>
  <div>Rust (changed crates)<b>${cargo ? `${rustOk} / ${rustTotal}` : "–"}</b></div>
  <div>Drag → coarse scene (cube / perforated plate)<b>${fmtMs(c.coarse?.median)} / ${fmtMs(d.coarse?.median)}</b></div>
</div>

<h2>1 Scope</h2>
<p>Phase B adds editing in the 3D view to the browser UI. The code remains the single source of truth: handles change literals or write grid references, the parameter bar only changes the preview.</p>
<ul>
  <li><b>Handles</b> of the line under the cursor, only for a literal or a variable with a literal; computed values are shown as a locked handle (gray ring). Kinds: cube/square corner, box/rect corner points, radius (sphere, circle, cylinder, torus), height (cylinder, linear_extrude), cylinder between two points, translate target (arrow old → new, handle at the target), rotation ring per axis (rotate), scaling at the bounding box corner, grid planes with a literal (handle on an edge of the grid box, drag along the axis), point variables, polygon/spline points, zone center via translate. Colors: literal orange, default value of a module parameter violet, pure reference turquoise, locked gray.</li>
  <li><b>Dragging:</b> <code>DragStart</code> → <code>Drag</code> (at most one message in flight, the last position wins) → <code>Edits</code> → the editor applies them as a live change and sends the source immediately; the engine tessellates coarsely, and finely after <code>DragEnd</code>. One drag is one undo step (live changes without history, at the end as one isolated event). Esc cancels.</li>
  <li><b>Snapping</b> in screen space (snap radius configurable, default 14 px) to grid points of the module and visible anchors (<code>SnapTargets</code>), target highlighted and labeled; the reference is written. 1D handles (height, radius, plane) snap to grid planes of the same axis and write <code>g.name</code> (new in cad-eval). <b>Alt:</b> no snapping, numbers rounded to the step size (settings, default 1 mm / 15°).</li>
  <li><b>Parameter bar</b> next to the view in module context: number (input field + slider), bool (switch), vector (three fields); reset per parameter, marking of changed values, "Reset all", values per module until the end of the session, "Apply as default" (<code>SetDefault</code>). Preview via <code>SetContext { overrides }</code>.</li>
  <li><b>Inline controls</b> for number and bool literals of assignments at the end of the line (slider, switch); one slider drag is one undo step.</li>
  <li><b>Ctrl+click</b> on a grid or anchor expression jumps to the declaration.</li>
</ul>
<p class="small">Changes to Rust crates: <code>cad-eval</code> (locked handles as <code>locked_handles</code>, <code>SnapRef::Plane</code> with a check that after writing, the handle lies exactly on the plane and the plane is visible at that location), <code>cad-session</code> (<code>Drag</code> always answers, <code>SetDragOptions</code>, scene with <code>locked_handles</code>), protocol types regenerated.</p>

<h2>2 Test environment</h2>
<table>${env.map(([a, b]) => `<tr><th style="width:32mm">${esc(a)}</th><td>${esc(b)}</td></tr>`).join("")}</table>
<p class="small">The end-to-end tests run against the production build with real mouse events on the canvas (Playwright <code>mouse.down/move/up</code>, one step per animation frame). They wait for scene versions, the drag state and the editor text via the test hook <code>window.__cad</code>, never for fixed times. The hook only supplies screen positions (handles, grid points, points on a ring or axis).</p>

<h2>3 Scenarios (Playwright, web/e2e/ui-b.spec.ts)</h2>
<table>
<tr><th>ID</th><th>Scenario</th><th>Result</th><th>Duration</th><th>Check</th></tr>
${scenarioRows}
</table>
<p class="small">Regression: the ${phaseA.length} scenarios from phase A (web/e2e/ui.spec.ts) ran in the same run, ${aPassed} passed.</p>

<h2>4 Unit tests</h2>
${vt ? `<table><tr><th>Vitest file</th><th>passed</th><th>Failed</th></tr>${unitRows}</table>
<p class="small">New: snap search in screen space, ray → value (plane, axis, ring), preview rounding, "at most one drag in flight", rounding and number formatting, undo grouping with the real CodeMirror history, mapping of the engine edits, state of the parameter bar, inline controls. Fixture <code>griffe.scad</code> from the real engine.</p>` : "<p>No Vitest results found.</p>"}
${cargo ? `<table><tr><th>Rust test program</th><th>passed</th><th>new tests</th></tr>${rustRows}</table>` : ""}

<h2>5 Measurements</h2>
<table><tr><th>Quantity</th><th>Value</th><th>Measurement</th></tr>${metricRows}</table>
<p class="small">Upper part: ${esc(d.model ?? "–")}. Each change of t fully re-evaluates the difference with fillet and four holes (no cache hit); the latency here is determined by the kernel time (SetSource → complete scene ${esc((d.evalMs ?? []).length ? `median ${Math.round([...d.evalMs].sort((x, y) => x - y)[Math.floor(d.evalMs.length / 2)])} ms` : "–")}). Equal rounded values produce no change; each change waits for its scene before the next position is sent (otherwise every new source would cancel the running kernel job). The frame rate is limited by software rendering (SwiftShader); phase A measured 14 fps while rotating.</p>

<h2>6 Known limitations and deviations</h2>
<ul>
  <li><b>Jump to the grid declaration with Ctrl+click</b> instead of a plain click: a plain click must still set the cursor, otherwise grid expressions would not be editable. The UI only knows the references of a module in module context (cursor in the module).</li>
  <li><b>Without Alt</b>, the handle snaps if a target is within the snap radius, otherwise numbers are written (rounded). A handle that consists only of references (e.g. <code>box(g[…], g[…])</code>) never turns into numbers without Alt; it only switches between targets.</li>
  <li><b>Snapping of 1D handles</b> only to planes on which the handle lands exactly after writing <code>g.name</code> (same origin and scale along the axis). Under a translated subtree or with <code>center = true</code>/<code>d =</code> (factor 2) there are therefore no plane targets; otherwise an expression such as <code>g.deckel - 5</code> would be written, which locks the handle.</li>
  <li>Point handles are dragged in the plane through the handle perpendicular to the view direction (2D points in their xy plane); nothing changes along the view direction. For a specific axis, a standard view helps.</li>
  <li>Undo group: at the end of the drag the total change is undone once and re-applied as one history entry; this causes one additional re-evaluation from the cache.</li>
  <li>Inline controls only appear for assignments with a number or bool literal (spec, value types); the range depends on the value at the start of the drag (0 … 2 × value, at least 10).</li>
  <li>Parameters without a default value are set to 0 / false / [0, 0, 0] on reset (spec table); strings and lists are read-only.</li>
  <li>The measurements come from headless Chromium with SwiftShader.</li>
</ul>

<h2>7 Screenshots and code excerpts</h2>
${blocks}
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
  footerTemplate: '<div style="font-size:7pt;color:#777;width:100%;text-align:center">Test report 3 – UI phase B · Page <span class="pageNumber"></span> of <span class="totalPages"></span></div>',
  margin: { top: "14mm", bottom: "16mm", left: "12mm", right: "12mm" },
});
await browser.close();
console.log(`report3: ${e2ePassed}/${e2e.length} phase B, ${aPassed}/${phaseA.length} phase A, ${unitPassed}/${unitTotal} Vitest, ${rustOk}/${rustTotal} Rust, ${used.size} images (max ${kib(maxImg)})`);
console.log(`-> ${join(out, "report.html")}`);
console.log(`-> ${join(out, "report.pdf")}`);
