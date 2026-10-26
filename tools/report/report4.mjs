#!/usr/bin/env node
// Test report 4 ("Fillet, chamfer, shell and profile operations"): collects the results of the
// Phase C tests and writes
//
//   docs/reports/04-fillet-profile/report.html, report.pdf, img/*.jpg
//
// Inputs (written by the test runs, gitignored under reports/tmp/):
//   report2/playwright.json   Playwright JSON reporter (web/playwright.config.ts); web/e2e/ui-c.spec.ts
//                             is reported here, ui.spec.ts / ui-b.spec.ts as regression
//   report4/vitest.json       Vitest JSON reporter
//   report4/cargo.txt         output of `cargo test --workspace`
//   report4/scenarios.json    title/caption/screenshots/code/checks per scenario (web/e2e/helpers.ts)
//   report4/metrics.json      evaluation times (web/e2e/ui-c.spec.ts, C18)
//   report4/img/*.jpg         screenshots
//
//   cd web && pnpm test:report4     (Rust tests, Vitest, Playwright, then this script)
//   node tools/report/report4.mjs [--run]
//
// Missing inputs are produced by the script (Vitest; the Phase C scenarios when playwright.json
// has none of them). `--run` reruns Vitest and all Playwright scenarios. The PDF is printed with
// Chromium (page.pdf, A4, backgrounds on).

import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import os from "node:os";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "../..");
const web = join(root, "web");
const input = join(root, "reports/tmp/report4");
const pwFile = join(root, "reports/tmp/report2/playwright.json");
const out = join(root, "docs/reports/04-fillet-profile");
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

const rerun = process.argv.includes("--run");
const sh = (args) => {
  try {
    execFileSync("pnpm", ["exec", ...args], { cwd: web, stdio: "inherit" });
  } catch {
    /* failed tests still write their results */
  }
};
const hasPhaseC = (j) => JSON.stringify(j?.suites ?? []).includes('"ui-c.spec.ts"');
mkdirSync(input, { recursive: true });
if (rerun || !existsSync(join(input, "vitest.json"))) {
  console.log("report4: Vitest …");
  sh(["vitest", "run", "--reporter=default", "--reporter=json", `--outputFile.json=${join(input, "vitest.json")}`]);
}
if (rerun) {
  console.log("report4: Playwright (all scenarios) …");
  sh(["playwright", "test"]);
} else if (!hasPhaseC(readJson(pwFile))) {
  console.log("report4: Playwright (web/e2e/ui-c.spec.ts) …");
  sh(["playwright", "test", "e2e/ui-c.spec.ts"]);
}

const pw = readJson(pwFile);
const vt = readJson(join(input, "vitest.json"));
const scenarios = readJson(join(input, "scenarios.json"), []);
const metrics = readJson(join(input, "metrics.json"), {});
const cargo = existsSync(join(input, "cargo.txt")) ? readFileSync(join(input, "cargo.txt"), "utf8") : null;
if (!pw || !hasPhaseC(pw)) {
  console.error(`missing: results of web/e2e/ui-c.spec.ts in ${pwFile} (cd web && pnpm e2e)`);
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
const e2e = flatten(pw, "ui-c.spec.ts");
// "ui-b.spec.ts" / "ui-c.spec.ts" do not end with "ui.spec.ts".
const phaseAUnique = flatten(pw, "ui.spec.ts");
const phaseB = flatten(pw, "ui-b.spec.ts");
const passed = (l) => l.filter((t) => t.status === "passed").length;

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
const NEW_UNIT = new Set(["web/src/logic/curves.test.ts", "web/src/logic/zones.test.ts", "web/src/engine/transport.test.ts"]);
const CHANGED_UNIT = new Set(["web/src/logic/display.test.ts", "web/src/logic/drag.test.ts"]);

/** Rust tests: `test name ... ok` lines grouped by test binary. */
const rust = [];
if (cargo) {
  let cur = null;
  for (const l of cargo.split("\n")) {
    const run = l.match(/Running (?:unittests )?(\S+)/) ?? l.match(/Doc-tests (\S+)/);
    if (run) {
      cur = { bin: run[1].replace(/\s*\(.*$/, ""), tests: [] };
      rust.push(cur);
      continue;
    }
    const t = l.match(/^test (.+?) \.\.\. (ok|FAILED|ignored)/);
    if (t && cur) cur.tests.push({ name: t[1], status: t[2] });
  }
}
const rustBins = rust.filter((b) => b.tests.length);
const rustTotal = rustBins.reduce((s, b) => s + b.tests.length, 0);
const rustOk = rustBins.reduce((s, b) => s + b.tests.filter((t) => t.status === "ok").length, 0);
const rustBad = rustBins.flatMap((b) => b.tests.filter((t) => t.status === "FAILED").map((t) => `${b.bin}: ${t.name}`));
const NEW_RUST = new Set(["unchanged_literals_keep_their_text", "profile_operation_handles", "spec_models_stage1"]);

// ---------------------------------------------------------------- images

rmSync(join(out, "img"), { recursive: true, force: true });
mkdirSync(join(out, "img"), { recursive: true });
const used = new Set();
for (const s of scenarios) for (const i of s.images ?? []) used.add(i.name);
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
  ["Engine", `cad_wasm_bg.wasm ${mib(asset(/^cad_wasm_bg-.*\.wasm$/))}, smlib.wasm ${mib(asset(/^smlib-.*\.wasm$/))} (SMLib kernel, Emscripten)`],
  ["Source", `Branch ${git("rev-parse", "--abbrev-ref", "HEAD")}, commit ${git("rev-parse", "--short", "HEAD")}`],
];

// ---------------------------------------------------------------- html

const date = new Date().toISOString().slice(0, 10);
const scen = (id) => scenarios.find((s) => s.id === id);
const statusLabel = (s) => (s === "passed" ? "passed" : s === "skipped" ? "skipped" : "failed");

const scenarioRows = e2e
  .map((t) => {
    const c = scen(t.id);
    const checks = c?.checks?.length ? `<ul class="checks">${c.checks.map((x) => `<li>${esc(x)}</li>`).join("")}</ul>` : "";
    return `<tr class="${t.status === "passed" ? "ok" : "bad"}"><td>${esc(t.id)}</td><td>${esc(t.title)}</td><td class="res">${statusLabel(t.status)}</td><td class="num">${(t.ms / 1000).toFixed(1)} s</td><td>${checks || esc(c?.caption ?? "")}${t.error ? `<div class="err">${esc(t.error.split("\n")[0])}</div>` : ""}</td></tr>`;
  })
  .join("\n");

const unitRows = unit
  .map((f) => `<tr class="${f.failed.length ? "bad" : "ok"}"><td>${esc(f.file)}${NEW_UNIT.has(f.file) ? ' <span class="tag">new</span>' : CHANGED_UNIT.has(f.file) ? ' <span class="tag alt">extended</span>' : ""}</td><td class="num">${f.passed} / ${f.total}</td><td>${f.failed.map(esc).join("<br>") || "–"}</td></tr>`)
  .join("\n");

const rustRows = rustBins
  .map((b) => {
    const fresh = b.tests.filter((t) => NEW_RUST.has(t.name)).map((t) => t.name);
    const bad = b.tests.filter((t) => t.status === "FAILED").map((t) => t.name);
    const name = b.bin.replace(/^.*\/deps\//, "").replace(/-[0-9a-f]{16}$/, "");
    return `<tr class="${bad.length ? "bad" : "ok"}"><td>${esc(name)}</td><td class="num">${b.tests.filter((t) => t.status === "ok").length} / ${b.tests.length}</td><td>${fresh.map((n) => `<code>${esc(n)}</code>`).join(", ") || "–"}${bad.length ? `<div class="err">${bad.map(esc).join(", ")}</div>` : ""}</td></tr>`;
  })
  .join("\n");

const ev = metrics.evalTimes ?? {};
const metricRows = Object.entries(ev)
  .map(([, o]) => [o.label, `${fmtMs(o.median)} / ${fmtMs(o.medianTotal)}`, `Kernel time of the complete scene / SetSource → scene in the app, median of 5 variants (individual kernel values ${o.kernel.map((v) => Math.round(v)).join(", ")} ms)${o.failed ? `; ${o.failed} of 5 variants failed in the kernel (see section 7)` : ""}`])
  .concat([["Largest image file in the report", kib(maxImg), "JPEG, quality 82"]])
  .map(([a, b, c]) => `<tr><td>${esc(a)}</td><td class="num">${esc(b)}</td><td>${esc(c)}</td></tr>`)
  .join("\n");

const code = (s) => (s ? `<pre>${esc(s)}</pre>` : "");
const blocks = scenarios
  .filter((s) => (s.images ?? []).length)
  .sort((a, b) => a.id.localeCompare(b.id, "en", { numeric: true }))
  .map((s) => {
    const img = (i) => `<div class="lbl">${esc(i.label)}</div><img src="img/${esc(i.name)}.jpg" alt="${esc(s.title)}: ${esc(i.label)}">`;
    const text = `${s.code ? `<div class="lbl">Code</div>${code(s.code)}` : ""}<p class="cap">${esc(s.caption)}</p>`;
    if (s.images.length === 1) {
      return `<section class="sc">
<h3>${esc(s.id)} ${esc(s.title)}</h3>
<div class="side"><div class="img">${img(s.images[0])}</div><div class="txt">${text}</div></div>
</section>`;
    }
    return `<section class="sc">
<h3>${esc(s.id)} ${esc(s.title)}</h3>
<div class="pair">${s.images.map((i) => `<div>${img(i)}</div>`).join("")}</div>
${text}
</section>`;
  })
  .join("\n");

const BUGS = [
  {
    crate: "kernel-smlib / smlib (sc_shell)",
    title: "shell of a filleted body fails; message depends on the platform",
    code: `grid g { z: o = 18; }
shell(1.6) { box([0, 0, 0], [90, 34, 18], $fillet = 3); zone(g.o, d = 1); }
shell(1.6) { box([0, 0, 0], [90, 34, 18], $fillet = 5); zone(g.o, d = 1); }
shell(1.6) { box([0, 0, 0], [90, 34, 18], $fillet = 6); zone(g.o, d = 1); }
// dimensions of tests/models/spec/dose_shell.scad (there with grid g { z: o = 30; }):
shell(1.5) { box([0, 0, 0], [50, 35, 30], $fillet = 4); zone(g.o, d = 1); }`,
    text: "With the SMLib fork (tangency-labs/smlib, capi-tess @ 7eba8aa) each of these shells fails, in the browser (Wasm) as well as natively (cad check): $fillet = 3 with \"SmApiShellBrepFull failed: SM_ERR_NULL_POINTER (1006)\", $fillet = 5 and 6 with \"wall thickness 1.6 leaves no cavity (SMLib returned no closed solid)\" and \"self-overlapping\" respectively, the dimensions of dose_shell.scad with \"self-overlapping solid at 10 face(s)\" – dose_shell.scad is therefore red. For the same input the messages differ between Wasm and native ($fillet = 8: Wasm \"self-overlapping\", native SM_ERR (1001)). With the previous kernel state, $fillet = 5 reported \"ok\" with an empty result and $fillet = 6 \"ok\" without an outer wall; these silent errors no longer occur. Sharp-edged and chamfered bodies ($chamfer) give correct hollow bodies (griffschale.scad, measurement C18).",
  },
  {
    crate: "kernel-smlib (nurbs_surface)",
    title: "surface(points) with 2 × 2 or 3 × 3 control points fails",
    code: `surface(points = [[[0, 0, 0], [20, 0, 6]], [[0, 20, 6], [20, 20, 0]]]);`,
    text: "\"surface gescheitert: SmApiCreateSurfaceFromOrderedPoints failed: SM_ERR (1001)\" (surface failed), likewise for 3 × 3. cad-eval accepts 2 × 2 points and up, the spec names no minimum; 4 × 4 works. Suggestion: adapt the degree to the number of points (like interp_curve for three path points).",
  },
  {
    crate: "cad-engine ($fillet / $chamfer)",
    title: "Seam edges count as separate edges of a primitive",
    code: `cylinder(h = 10, r = 5, $chamfer = 1);   // "3 edges"
sphere(5, $fillet = 1);                    // error`,
    text: "For the cylinder, the top, bottom and seam edges are selected (\"3 edges\", 3 drawn); for the sphere only the seam (\"1 edge\"), and the fillet fails with \"fillet: none of the edges can be blended (smooth, seam or open edges)\" – line red instead of \"0 edges\" or no effect. Smooth seam and pole edges should not be selected.",
  },
];
const bugBlocks = BUGS.map((b, i) => `<section class="bug"><h3>F${i + 1} ${esc(b.title)}</h3><div class="small"><b>Crate:</b> ${esc(b.crate)}</div>${code(b.code)}<p>${esc(b.text)}</p></section>`).join("\n");

const html = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Test report 4 – Fillet, chamfer, shell and profile operations</title>
<style>
  @page { size: A4; margin: 16mm 14mm 16mm 14mm; }
  :root { --text: #1f2328; --muted: #5d646e; --border: #d5d9df; --ok: #1a7f37; --bad: #c9302c; --head: #f2f4f7; }
  * { box-sizing: border-box; }
  body { font: 10pt/1.45 system-ui, -apple-system, "Segoe UI", Roboto, sans-serif; color: var(--text); margin: 0; }
  main { max-width: 182mm; margin: 0 auto; }
  h1 { font-size: 19pt; margin: 0 0 2mm; }
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
  .tag.alt { background: #6741d9; }
  ul { margin: 1mm 0 2mm 5mm; padding: 0; }
  li { margin: 0.6mm 0; }
  ul.checks { margin: 0 0 0 3.5mm; }
  ul.checks li { margin: 0; }
  code, pre { font-family: ui-monospace, "SFMono-Regular", Menlo, Consolas, monospace; font-size: 8pt; }
  pre { margin: 0; padding: 1.5mm 2mm; background: #f6f8fa; border: 1px solid var(--border); border-radius: 1mm; white-space: pre-wrap; word-break: break-word; }
  .sc, .bug { break-inside: avoid; page-break-inside: avoid; margin: 0 0 7mm; }
  .pair { display: flex; gap: 2mm; }
  .pair > div { flex: 1; min-width: 0; }
  .pair img { max-width: 100%; max-height: 74mm; width: auto; height: auto; border: 1px solid var(--border); display: block; }
  .side { display: flex; gap: 3mm; align-items: flex-start; }
  .side > .img { flex: 0 0 auto; max-width: 58%; }
  .side > .img img { max-width: 100%; max-height: 88mm; width: auto; height: auto; border: 1px solid var(--border); display: block; }
  .side > .txt { flex: 1; min-width: 0; }
  .lbl { font-size: 7.5pt; color: var(--muted); text-transform: uppercase; letter-spacing: 0.04em; margin: 1mm 0 0.5mm; }
  .cap { font-size: 8.8pt; color: var(--muted); margin: 1.5mm 0 0; }
  .small { font-size: 8.8pt; color: var(--muted); }
  .bug p { font-size: 9pt; margin: 1.5mm 0 0; }
  tr, td { break-inside: avoid; page-break-inside: avoid; }
</style>
</head>
<body>
<main>
<h1>Test report 4 – Fillet, chamfer, shell and profile operations</h1>
<div class="meta">Display in the browser UI · ${esc(date)} · Commit ${esc(git("rev-parse", "--short", "HEAD"))}</div>

<div class="summary">
  <div>Phase C (Playwright)<b>${passed(e2e)} / ${e2e.length} passed</b></div>
  <div>Phase A/B (regression)<b>${phaseAUnique.length + phaseB.length ? `${passed(phaseAUnique) + passed(phaseB)} / ${phaseAUnique.length + phaseB.length}` : "–"}</b></div>
  <div>Vitest<b>${vt ? `${unitPassed} / ${unitTotal}` : "–"}</b></div>
  <div>cargo test --workspace<b>${cargo ? `${rustOk} / ${rustTotal}` : "–"}</b></div>
  <div>Bugs in other crates<b>${BUGS.length}</b></div>
</div>

<h2>1 Scope</h2>
<p>Tested and completed: the display of the spec sections "Fillet and chamfer", "Boolean operations" (parameters <code>fillet =</code>/<code>chamfer =</code>), "Profile operations and surfaces", "Primitives/2D" and "Errors and warnings", plus two open items from phase A/B.</p>
<ul>
  <li><b>fillet/chamfer line:</b> zones blue and semi-transparent, selected edges orange, "n zones · m edges" at the end of the line (existing, now with a pixel check). <b>New: cursor on a zone</b> shows only that zone and only the edges of the block that lie in it (checked in the view against the zone meshes, <code>inside</code>: all sample points, <code>touch</code>: one); a zone via <code>translate</code> without the outline/coordinate triads of the transformation. Empty selection yellow with "0 edges", failure red with a red edge and the body without fillet (engine); <code>difference(fillet = r)</code>, <code>$fillet</code>/<code>$chamfer</code>, <code>select = "touch"</code> and nested blocks checked.</li>
  <li><b>shell:</b> like the zone block; <b>new</b>: the opened faces of the body are filled orange (faces of the body mesh that lie completely in a zone; count checked against the engine's <code>opened_faces</code>), also with the cursor on a zone.</li>
  <li><b>Profile operations (new):</b> <code>linear_extrude</code> – profile as outline at the bottom (at −h/2 with <code>center</code>), height handle at the top; <code>rotate_extrude</code> – profile in the xz plane, axis of rotation as a dashed line, angle handle on the ring; <code>loft</code> – all profiles as outlines at their location, result semi-transparent; <code>sweep</code> – path as a curve (cubic interpolation as in the kernel), handles at literal points, profile at the start point perpendicular to the start tangent; <code>surface</code> – control net as lines with handles, surface rendered double-sided; <code>thicken</code> – surface as outline, result full. cad-eval already supplied the handles (height, angle, path points, control net); new are a test for them and their display/check in the browser.</li>
  <li><b>2D shapes alone:</b> outline with a light fill in the xy plane; the warning for a z deviation of grid points appears yellow at the end of the line.</li>
  <li><b>Refit:</b> after a significant size change (diagonal × 1.6 or center moved by more than half the diagonal) the view refits itself as long as the user has not moved the camera (and not dragged a handle); otherwise the button "Refit" (German UI: „Neu einpassen“) appears.</li>
  <li><b>Axis-constrained dragging</b> of point handles: Shift key – axis of the local coordinate system from the mouse movement; keys X/Y/Z – axis directly (press again: free), also in the middle of a drag. Guide line in the axis color, snapping only to targets on the axis, the other components keep their text.</li>
  <li><b>Examples (new, in the examples menu):</b> griffschale.scad (shell), flasche.scad (rotate_extrude + fillet), lueftungsgitter.scad (linear_extrude + grid, chamfer =), kabelfuehrung.scad (sweep), freiformdeckel.scad (surface + thicken).</li>
  <li><b>Draft mode while dragging (engine state 58735e2):</b> fillets that take longer than 25 ms are suspended by the engine while dragging (<code>NodeReport.blend_suspended</code>). The status bar then shows "fillet suspended while dragging (line …)" (German UI: „Verrundung beim Ziehen ausgesetzt (Zeile …)“), the same hint appears at the end of the line; after release the engine evaluates fully and the hint disappears (C20).</li>
  <li><b>Watchdog block (plan 7.3):</b> the UI remembers the key of <code>current</code> on every <code>Progress</code>. If the watchdog terminates the worker, the new worker receives <code>{type: "BlockKeys", keys: […]}</code> with all keys that have hung so far before <code>SetSource</code> (the list survives further restarts), and then the last source again; the blocked node reports "timeout, node blocked until the next change" (German UI: „Zeitüberschreitung, Knoten gesperrt bis zur nächsten Änderung“), its line is red (C19, unit test transport.test.ts). Without a newly blocked node the source is not resent, so that the same input does not hang again.</li>
  <li><b>tools/report/report2-ui.mjs</b> runs standalone again: if Vitest results are missing or the shared playwright.json contains no phase A scenarios (it only holds the last run), it runs these tests itself.</li>
</ul>
<p class="small">Changes to Rust crates: <code>cad-session</code> – scene nodes with <code>op_info</code> (resolved parameters of fillet/chamfer/shell incl. <code>select</code>, zone, linear_extrude, rotate_extrude, loft, sweep path, surface net, thicken), <code>blend_suspended</code> per node and <code>suspended_blends</code> in the statistics (from the engine's draft mode), protocol types regenerated; <code>cad-eval</code> – during free dragging, literals whose value does not change are not rewritten (otherwise axis-constrained dragging would round e.g. 5.5 to 6), tests for profile handles; <code>compat-tests</code> – expectations for the five new examples (the test requires one entry per file in tests/models/spec).</p>

<h2>2 Test environment</h2>
<table>${env.map(([a, b]) => `<tr><th style="width:32mm">${esc(a)}</th><td>${esc(b)}</td></tr>`).join("")}</table>
<p class="small">The end-to-end tests run against the production build with real mouse and keyboard events. Checks go through the test hook <code>window.__cad</code> (display plan, drawn edges by kind, zone/face check, camera, handles, editor text) and through pixels: for a layer (e.g. <code>edge:selected</code>, <code>open-faces</code>, <code>mode:profile</code>, <code>guide:path</code>) the pixels that change when the layer is hidden are counted.</p>

<h2>3 Scenarios (Playwright, web/e2e/ui-c.spec.ts)</h2>
<table>
<tr><th>ID</th><th>Scenario</th><th>Result</th><th>Duration</th><th>Checked</th></tr>
${scenarioRows}
</table>
<p class="small">Regression in the same run: phase A (web/e2e/ui.spec.ts) ${passed(phaseAUnique)} / ${phaseAUnique.length}, phase B (web/e2e/ui-b.spec.ts) ${passed(phaseB)} / ${phaseB.length} passed.</p>

<h2>4 Unit and Rust tests</h2>
${vt ? `<table><tr><th>Vitest file</th><th>passed</th><th>Failed</th></tr>${unitRows}</table>
<p class="small">New: cubic path interpolation, start frame of the profile, point-in-mesh with tolerance, edges in a zone (inside/touch), opened faces, criterion for refitting. Extended: display plan for profile operations, zones and shell (fixture <code>profile.scad</code> from the real engine), axis-constrained dragging.</p>` : "<p>No Vitest results found.</p>"}
${cargo ? `<table><tr><th>Rust test program (cargo test --workspace)</th><th>passed</th><th>new/changed tests</th></tr>${rustRows}</table>${rustBad.length ? `<p class="err">Failed: ${rustBad.map(esc).join(", ")}</p>` : ""}` : ""}

<h2>5 Measurements</h2>
<table><tr><th>Model</th><th>Kernel / total</th><th>Measurement</th></tr>${metricRows}</table>
<p class="small">Each variant has different parameters (no cache hit in the affected node). "Total" is SetSource until the complete scene in the worker, including language evaluation and tessellation; the times come from headless Chromium with SwiftShader on ${esc(os.cpus().length)} cores.</p>

<h2>6 Known limitations and deviations</h2>
<ul>
  <li><b>Edges of a zone, opened faces:</b> the view checks against the tessellated zone meshes (tolerance 1.5 × chord error); the engine selects with the exact kernel classification. In borderline cases (edge exactly on a curved zone surface) the view can deviate; in the test, the count matches the engine.</li>
  <li><b>sweep path:</b> the UI interpolates the curve with the same method as the SMLib facade (cubic, chord lengths, two extra points for three points); if the method in the kernel changes, the display deviates. The start frame follows sc_sweep (x = global X projected into the normal plane).</li>
  <li><b>Profile of linear_extrude/rotate_extrude:</b> the mesh of the direct children is shown (2D, flat in their plane); z offsets of 2D transformations are lost, as in the engine.</li>
  <li><b>Zone via translate:</b> the cursor on the zone shows only the zone and its edges, not the coordinate triad of the transformation (the spec line "Cursor on a zone" takes precedence); the handle at the zone center remains.</li>
  <li><b>Refit:</b> automatic only as long as the camera has not been rotated/panned/zoomed since the last fit and no handle has been dragged; "Show all" and the standard views restore this state.</li>
  <li><b>Axis-constrained:</b> axes of the handle's local coordinate system (only x/y for 2D points); with Shift the handle waits until 6 px of movement determine the axis. If the axis is only chosen during the drag, the axis line goes through the current position.</li>
  <li><b>An open surface next to bodies</b> at the top level is an implicit union and thus an error ("thicken first") – as per spec, loaded separately in the test.</li>
  <li><b>Watchdog block:</b> the node from the last <code>Progress.current</code> before the hang is blocked; what is blocked is its content key, so identical nodes elsewhere are blocked as well. The list applies until the page is reloaded; a change to the node changes the key and thereby lifts the block.</li>
  <li><b>Draft mode:</b> the hint shows the last drawn state; the threshold (25 ms) and the selection of suspended fillets are in the engine.</li>
  <li>Some results depend on the kernel: dose_shell.scad is currently red because of F1 (line red, body without shell); griffschale.scad therefore uses $chamfer = 4 (a correct hollow body on both platforms); tests C08, C17 and C18 additionally check that an "ok" node has geometry.</li>
</ul>

<h2>7 Bugs found in other crates</h2>
<p class="small">Not fixed (crates outside this task); minimal examples to reproduce in the browser or with <code>cad check file.scad</code> (cad-cli, native kernel).</p>
${bugBlocks}

<h2>8 Screenshots and code excerpts</h2>
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
  footerTemplate: '<div style="font-size:7pt;color:#777;width:100%;text-align:center">Test report 4 – Fillet, chamfer, shell and profile operations · Page <span class="pageNumber"></span> of <span class="totalPages"></span></div>',
  margin: { top: "14mm", bottom: "16mm", left: "12mm", right: "12mm" },
});
await browser.close();
console.log(`report4: ${passed(e2e)}/${e2e.length} phase C, ${passed(phaseAUnique)}/${phaseAUnique.length} phase A, ${passed(phaseB)}/${phaseB.length} phase B, ${unitPassed}/${unitTotal} Vitest, ${rustOk}/${rustTotal} Rust, ${used.size} images (max ${kib(maxImg)})`);
console.log(`-> ${join(out, "report.html")}`);
console.log(`-> ${join(out, "report.pdf")}`);
