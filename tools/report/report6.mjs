#!/usr/bin/env node
// Test report 6 ("Projects, parts, include/use"): collects the results of the Phase D tests
// and writes
//
//   docs/reports/06-projekte-bauteile/report.html, report.pdf, img/*.jpg
//
// Inputs (written by the test runs, gitignored under reports/tmp/):
//   report2/playwright.json   Playwright JSON reporter (web/playwright.config.ts); web/e2e/projects.spec.ts
//                             is reported here, ui.spec.ts / ui-b.spec.ts / ui-c.spec.ts as regression
//   report6/vitest.json       Vitest JSON reporter
//   report6/cargo.txt         output of `cargo test --workspace`
//   report6/compare.txt       output of tests/openscad-compat/compare.py (optional)
//   report6/scenarios.json    title/caption/screenshots/code/checks per scenario (web/e2e/helpers.ts)
//   report6/img/*.jpg         screenshots
//
// The include/use comparison with OpenSCAD 2021.01 is computed here: for every project in
// tests/openscad-compat/projects the ECHO lines of `cad eval --json` (target/debug/mcad or
// $CAD) against expected.echo and, if `openscad` is installed, against a fresh OpenSCAD run.
//
//   node tools/report/report6.mjs [--run]
//
// `--run` reruns Vitest and the Phase D Playwright scenarios. The PDF is printed with Chromium.

import { execFileSync, spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import os from "node:os";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "../..");
const web = join(root, "web");
const input = join(root, "reports/tmp/report6");
const pwFile = join(root, "reports/tmp/report2/playwright.json");
const out = join(root, "docs/reports/06-projekte-bauteile");
const require = createRequire(join(web, "package.json"));

const readJson = (f, d = null) => (existsSync(f) ? JSON.parse(readFileSync(f, "utf8")) : d);
const esc = (s) => String(s ?? "").replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
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
const hasPhaseD = (j) => JSON.stringify(j?.suites ?? []).includes('"projects.spec.ts"');
mkdirSync(input, { recursive: true });
if (rerun || !existsSync(join(input, "vitest.json"))) {
  console.log("report6: Vitest …");
  sh(["vitest", "run", "--reporter=default", "--reporter=json", `--outputFile.json=${join(input, "vitest.json")}`]);
}
if (rerun || !hasPhaseD(readJson(pwFile))) {
  console.log("report6: Playwright (web/e2e/projects.spec.ts) …");
  sh(["playwright", "test", "e2e/projects.spec.ts"]);
}

const pw = readJson(pwFile);
const vt = readJson(join(input, "vitest.json"));
const scenarios = readJson(join(input, "scenarios.json"), []);
const cargo = existsSync(join(input, "cargo.txt")) ? readFileSync(join(input, "cargo.txt"), "utf8") : null;
const compare = existsSync(join(input, "compare.txt")) ? readFileSync(join(input, "compare.txt"), "utf8") : null;
if (!pw || !hasPhaseD(pw)) {
  console.error(`missing: results of web/e2e/projects.spec.ts in ${pwFile} (cd web && pnpm e2e)`);
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
const e2e = flatten(pw, "projects.spec.ts");
const regression = [...flatten(pw, "ui.spec.ts"), ...flatten(pw, "ui-b.spec.ts"), ...flatten(pw, "ui-c.spec.ts")];
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
const NEW_UNIT = new Set(["web/src/logic/files.test.ts", "web/src/files/zip.test.ts"]);

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
const NEW_RUST_BINS = ["tests/project.rs", "tests/project_openscad.rs", "tests/projects.rs"];
const NEW_RUST = new Set(["indexed_named_instance", "file_regions", "include_path_span", "file_names", "objects_xml_has_one_object_per_mesh"]);
const stepVerify = existsSync(join(input, "step-verify.txt")) ? readFileSync(join(input, "step-verify.txt"), "utf8") : null;
const stepLine = stepVerify?.split("\n").find((l) => /\d+\/\d+ passed/.test(l)) ?? null;
const compareLine = compare?.split("\n").find((l) => /\d+\s*\/\s*\d+/.test(l) && /(ok|match)/i.test(l)) ?? compare?.trim().split("\n").at(-1) ?? null;

// ---------------------------------------------------------------- include/use against OpenSCAD

const cadBin = process.env.CAD ?? [join(root, "target/release/mcad"), join(root, "target/debug/mcad")].find(existsSync);
const projDir = join(root, "tests/openscad-compat/projects");
const echoLines = (t) => t.split("\n").filter((l) => l.startsWith("ECHO:"));
const osc = [];
for (const name of existsSync(projDir) ? readdirSync(projDir).sort() : []) {
  const dir = join(projDir, name);
  if (!statSync(dir).isDirectory()) continue;
  const files = [];
  const walk = (d, pre = "") => {
    for (const f of readdirSync(d).sort()) {
      const p = join(d, f);
      if (statSync(p).isDirectory()) walk(p, `${pre}${f}/`);
      else if (f.endsWith(".scad")) files.push(`${pre}${f}`);
    }
  };
  walk(dir);
  const expected = echoLines(readFileSync(join(dir, "expected.echo"), "utf8"));
  let ours = null;
  let err = null;
  try {
    const j = JSON.parse(execFileSync(cadBin, ["eval", join(dir, "main.scad"), "--json"], { encoding: "utf8", maxBuffer: 64 << 20 }));
    ours = (j.scene?.echoes ?? []).map((e) => `ECHO: ${e.text}`);
  } catch (e) {
    err = String(e.message ?? e).split("\n")[0];
  }
  let live = null;
  try {
    const tmp = join(os.tmpdir(), `report6-${name}.echo`);
    execFileSync("openscad", ["-o", tmp, "main.scad"], { cwd: dir, stdio: "ignore" });
    live = echoLines(readFileSync(tmp, "utf8"));
  } catch {
    live = null;
  }
  const same = (a, b) => a && b && a.length === b.length && a.every((x, i) => x === b[i]);
  osc.push({ name, files, expected, ours, live, ok: same(ours, expected), liveOk: live ? same(ours, live) : null, err });
}
const oscOk = osc.filter((o) => o.ok && o.liveOk !== false).length;

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
const kib = (b) => `${(b / 1024).toFixed(0)} KiB`;

// ---------------------------------------------------------------- environment

const { chromium } = require("@playwright/test");
const browser = await chromium.launch({ args: ["--use-angle=swiftshader", "--enable-unsafe-swiftshader"] });
// `openscad --version` prints to stderr.
const ov = spawnSync("openscad", ["--version"], { encoding: "utf8" });
const oscVersion = ov.error ? "not installed" : `${ov.stdout ?? ""}${ov.stderr ?? ""}`.trim() || "–";
const env = [
  ["Operating system", `${os.type()} ${os.release()} (${os.arch()})`],
  ["CPU", `${os.cpus()[0]?.model ?? "–"} · ${os.cpus().length} cores`],
  ["Node.js", process.version],
  ["Browser", `Chromium ${browser.version()} (headless, Playwright ${pkgVersion("@playwright/test")}), WebGL via SwiftShader`],
  ["Window", "1440 × 900 px, device pixel ratio 1"],
  ["Build", `Vite ${pkgVersion("vite")} production build; React ${pkgVersion("react")}, three.js ${pkgVersion("three")}, CodeMirror ${pkgVersion("@codemirror/view")}`],
  ["OpenSCAD", oscVersion],
  ["Source", `Branch ${git("rev-parse", "--abbrev-ref", "HEAD")}, commit ${git("rev-parse", "--short", "HEAD")}`],
];

// ---------------------------------------------------------------- html

const date = new Date().toISOString().slice(0, 10);
const scen = (id) => scenarios.find((s) => s.id === id);
const statusLabel = (s) => (s === "passed" ? "passed" : s === "skipped" ? "skipped" : "failed");
const code = (s) => (s ? `<pre>${esc(s)}</pre>` : "");

const scenarioRows = e2e
  .map((t) => {
    const c = scen(t.id);
    const checks = c?.checks?.length ? `<ul class="checks">${c.checks.map((x) => `<li>${esc(x)}</li>`).join("")}</ul>` : "";
    return `<tr class="${t.status === "passed" ? "ok" : "bad"}"><td>${esc(t.id)}</td><td>${esc(t.title)}</td><td class="res">${statusLabel(t.status)}</td><td class="num">${(t.ms / 1000).toFixed(1)} s</td><td>${checks || esc(c?.caption ?? "")}${t.error ? `<div class="err">${esc(t.error.split("\n")[0])}</div>` : ""}</td></tr>`;
  })
  .join("\n");

const unitRows = unit
  .map((f) => `<tr class="${f.failed.length ? "bad" : "ok"}"><td>${esc(f.file)}${NEW_UNIT.has(f.file) ? ' <span class="tag">new</span>' : ""}</td><td class="num">${f.passed} / ${f.total}</td><td>${f.failed.map(esc).join("<br>") || "–"}</td></tr>`)
  .join("\n");

const rustRows = rustBins
  .map((b) => {
    const isNew = NEW_RUST_BINS.some((n) => b.bin.endsWith(n));
    const fresh = isNew ? [] : b.tests.filter((t) => NEW_RUST.has(t.name.replace(/^tests::/, ""))).map((t) => t.name);
    const bad = b.tests.filter((t) => t.status === "FAILED").map((t) => t.name);
    const name = b.bin.replace(/^.*\/deps\//, "").replace(/-[0-9a-f]{16}$/, "");
    return `<tr class="${bad.length ? "bad" : "ok"}"><td>${esc(name)}${isNew ? ' <span class="tag">new</span>' : ""}</td><td class="num">${b.tests.filter((t) => t.status === "ok").length} / ${b.tests.length}</td><td>${isNew ? b.tests.map((t) => `<code>${esc(t.name)}</code>`).join(", ") : fresh.map((n) => `<code>${esc(n)}</code>`).join(", ") || "–"}${bad.length ? `<div class="err">${bad.map(esc).join(", ")}</div>` : ""}</td></tr>`;
  })
  .join("\n");

const oscRows = osc
  .map(
    (o) =>
      `<tr class="${o.ok && o.liveOk !== false ? "ok" : "bad"}"><td>${esc(o.name)}</td><td>${o.files.map((f) => `<code>${esc(f)}</code>`).join(" ")}</td><td class="num">${o.expected.length}</td><td class="res">${o.ok ? "equal" : "different"}</td><td class="res">${o.liveOk === null ? "–" : o.liveOk ? "equal" : "different"}</td></tr>`,
  )
  .join("\n");
const oscBlocks = osc
  .map((o) => {
    const main = readFileSync(join(projDir, o.name, "main.scad"), "utf8").trim();
    return `<div class="osc"><b>${esc(o.name)}</b> – main.scad and echo (identical for OpenSCAD and us)<div class="pair"><div>${code(main)}</div><div>${code((o.ours ?? []).join("\n"))}</div></div>${o.err ? `<div class="err">${esc(o.err)}</div>` : ""}</div>`;
  })
  .join("\n");

const blocks = scenarios
  .filter((s) => (s.images ?? []).length)
  .sort((a, b) => a.id.localeCompare(b.id, "en", { numeric: true }))
  .map((s) => {
    const img = (i) => `<div class="lbl">${esc(i.label)}</div><img src="img/${esc(i.name)}.jpg" alt="${esc(s.title)}: ${esc(i.label)}">`;
    const text = `${s.code ? `<div class="lbl">Code</div>${code(s.code)}` : ""}<p class="cap">${esc(s.caption)}</p>`;
    return `<section class="sc">
<h3>${esc(s.id)} ${esc(s.title)}</h3>
${s.images.map((i) => `<div class="shot">${img(i)}</div>`).join("")}
${text}
</section>`;
  })
  .join("\n");

const html = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Test report 6 – Projects, parts, include/use</title>
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
  ul { margin: 1mm 0 2mm 5mm; padding: 0; }
  li { margin: 0.6mm 0; }
  ul.checks { margin: 0 0 0 3.5mm; }
  ul.checks li { margin: 0; }
  code, pre { font-family: ui-monospace, "SFMono-Regular", Menlo, Consolas, monospace; font-size: 8pt; }
  pre { margin: 0; padding: 1.5mm 2mm; background: #f6f8fa; border: 1px solid var(--border); border-radius: 1mm; white-space: pre-wrap; word-break: break-word; }
  .sc { break-inside: avoid; page-break-inside: avoid; margin: 0 0 7mm; }
  .shot img { max-width: 100%; max-height: 115mm; width: auto; height: auto; border: 1px solid var(--border); display: block; }
  .pair { display: flex; gap: 2mm; }
  .pair > div { flex: 1; min-width: 0; }
  .osc { break-inside: avoid; margin: 0 0 4mm; font-size: 9pt; }
  .lbl { font-size: 7.5pt; color: var(--muted); text-transform: uppercase; letter-spacing: 0.04em; margin: 1.5mm 0 0.5mm; }
  .cap { font-size: 8.8pt; color: var(--muted); margin: 1.5mm 0 0; }
  .small { font-size: 8.8pt; color: var(--muted); }
  tr, td { break-inside: avoid; page-break-inside: avoid; }
</style>
</head>
<body>
<main>
<h1>Test report 6 – Projects, parts, include/use</h1>
<div class="meta">Language, engine, CLI and browser UI · ${esc(date)} · Commit ${esc(git("rev-parse", "--short", "HEAD"))}</div>

<div class="summary">
  <div>Phase D (Playwright)<b>${passed(e2e)} / ${e2e.length} passed</b></div>
  <div>Phase A–C (regression)<b>${regression.length ? `${passed(regression)} / ${regression.length}` : "–"}</b></div>
  <div>Vitest<b>${vt ? `${unitPassed} / ${unitTotal}` : "–"}</b></div>
  <div>cargo test --workspace<b>${cargo ? `${rustOk} / ${rustTotal}` : "–"}</b></div>
  <div>include/use against OpenSCAD<b>${oscOk} / ${osc.length}</b></div>
</div>

<h2>1 Scope</h2>
<ul>
  <li><b><code>include &lt;file&gt;</code> / <code>use &lt;file&gt;</code></b> with OpenSCAD semantics: include inserts the statements of the file at its position (definitions, variables, geometry; the last assignment wins, with a warning across the file boundary); use only makes modules and functions visible, which run in their own file (they see that file's variables and its own use libraries, not transitively); own definitions take precedence over used ones, among several use the last one wins. Paths relative to the including file (or absolute). Cycles and missing files are errors at the include/use line.</li>
  <li><b>File IDs in source positions:</b> each loaded file occupies its own range of positions (file <i>f</i> from <i>f</i>·2<sup>24</sup>, statement IDs from <i>f</i>·2<sup>22</sup>); diagnostics, hints, handles, spans and text edits thus know their file. The protocol outputs positions local to the file (<code>SrcRange.file</code>, <code>Diagnostic.file</code>, <code>Edit.file</code>, <code>Scene.files</code>); writing back hits the correct file.</li>
  <li><b>File resolver:</b> <code>cad-eval</code> reads project files via the trait <code>FileResolver</code> (file system for the CLI, map for session/Wasm). The session receives the files with <code>SetFiles</code>/<code>SetFile</code>. The engine cache is unchanged (keys from the content).</li>
  <li><b>Projects in the UI:</b> files as tabs above the editor (separate undo history per file), selectable main file (★), new files, removal; open a folder (File System Access API, saving writes back), otherwise upload and download a ZIP; autosave of the whole project to IndexedDB; example projects in the examples menu. Ctrl+click on the path in include/use opens the file; a double-click on an instance of a module from another file opens that file at the definition; handles write into the file of the literal (also files that are not displayed, one undo step there); face from another file: the status bar names file and line, a click opens it.</li>
  <li><b>Multiple parts</b> (open item 2, plan 12.9): named instances at the top level of the main file are parts. Scene with bounding box and volume per part; parts bar with visibility and export STL/3MF/STEP per part, all as ZIP (STL or STEP) or one 3MF with one object per part. STEP (exact B-rep, AP242/AP214) goes through the same writer as the whole model: <code>Engine::export_step</code> is <code>Engine::export_step_nodes</code> of the root, parts are passed with their placement (<code>display_world</code>) via <code>Kernel::transform</code> and <code>Kernel::brep</code> to <code>cad_step::write</code>. CLI: <code>cad export --part name</code> (also <code>.step</code>, <code>--schema</code>), <code>--all-parts -o parts/ [--format step]</code>.</li>
  <li><b>Named instances in loops</b> (plan 12 item 2): <code>for (i = [0:3]) k[i]: halter();</code> → <code>k[2].anker</code>; integer index, visibility as for named instances, snapping writes <code>k[2].ecke</code>. <code>k: …</code> in a loop remains an error.</li>
  <li><b>Example project</b> <code>tests/models/project-housing/</code>: housing.mcad (main file), constants.mcad (include), lib/parts.mcad (use, with screw bosses <code>boss[i]</code>), parts housing and lid.</li>
</ul>

<h2>2 Test environment</h2>
<table>${env.map(([a, b]) => `<tr><th style="width:32mm">${esc(a)}</th><td>${esc(b)}</td></tr>`).join("")}</table>

<h2>3 include/use against OpenSCAD 2021.01</h2>
<p class="small">For each project in <code>tests/openscad-compat/projects/</code>, the ECHO lines of <code>cad eval --json main.scad</code> are compared against <code>expected.echo</code> (generated with <code>openscad -o x.echo main.scad</code>) and against a fresh OpenSCAD run. The same comparison runs as the Rust test <code>cad-eval/tests/project_openscad.rs</code>.</p>
<table><tr><th>Project</th><th>Files</th><th>Echo lines</th><th>expected.echo</th><th>OpenSCAD now</th></tr>${oscRows}</table>
${oscBlocks}

<h2>4 Scenarios (Playwright, web/e2e/projects.spec.ts)</h2>
<table>
<tr><th>ID</th><th>Scenario</th><th>Result</th><th>Duration</th><th>Checked</th></tr>
${scenarioRows}
</table>
<p class="small">Regression in the same run (phase A–C: ui.spec.ts, ui-b.spec.ts, ui-c.spec.ts): ${passed(regression)} / ${regression.length} passed.</p>

<h2>5 Unit, Rust and CLI tests</h2>
${vt ? `<table><tr><th>Vitest file</th><th>passed</th><th>Failed</th></tr>${unitRows}</table>` : "<p>No Vitest results found.</p>"}
${cargo ? `<table><tr><th>Rust test program (cargo test --workspace)</th><th>passed</th><th>new tests</th></tr>${rustRows}</table>${rustBad.length ? `<p class="err">Failed: ${rustBad.map(esc).join(", ")}</p>` : ""}` : ""}
<p class="small">New in Rust: projects (resolver, include/use, cycle, missing file, use without geometry, writing back into an included file, context module from a library, parts, <code>k[i]</code> with error cases), OpenSCAD comparison, session (SetFiles/SetFile, file in ranges and edits, part export), CLI (<code>check</code>/<code>eval</code> with file system, diagnostics with path, <code>export --part</code>, <code>--all-parts</code>). ${compareLine ? `OpenSCAD comparison of the single files (compare.py): <code>${esc(compareLine.trim())}</code>.` : ""} ${stepLine ? `STEP check with OpenCASCADE over all examples (<code>tests/step/verify.py</code>, venv-step): <code>${esc(stepLine.replace(/; report.*$/, "").trim())}</code>.` : ""} STEP per part, as ZIP and with <code>--all-parts --format step</code>: Rust tests <code>cad-session/tests/project.rs</code> (<code>export_parts</code>) and <code>cad-cli/tests/projects.rs</code>; in the browser P07.</p>


<h2>6 Deviations and limitations</h2>
<ul>
  <li><b>Search path:</b> only relative to the including file or absolute; no <code>OPENSCADPATH</code>/library folder. Missing files and include cycles are errors (OpenSCAD: warning, the file is skipped).</li>
  <li><b>use:</b> variables of a library are evaluated once per evaluation (lazily), not anew on every call as in OpenSCAD; <code>$</code> variables at the top level of a library apply in its modules. The geometry of used files is not generated.</li>
  <li><b>A file included twice</b> (via two include paths) is loaded twice (two file IDs, same path); at most 255 files, at most 16 MiB per file.</li>
  <li><b>Snap visibility</b> of a grid/instance named in an included file extends to the end of that file (not into the including file); evaluating <code>k.anker</code> is not affected by this.</li>
  <li><b>Parts</b> only in file context; a hidden part inside a boolean operation cannot be hidden separately (the subtree stays whole). Assemblies are open.</li>
  <li><b>STEP of several parts in one file:</b> one product (name: part names joined with " + ") with all bodies, no assembly structure per part; a part whose module is an implicit union consists of several bodies (e.g. an enclosure with screw bosses).</li>
  <li><b>solidcore</b> is now opt-in (<code>--features solidcore</code>); <code>cad diff-kernels</code> without the feature reports "build with <code>--features solidcore</code>" (exit 2).</li>

  <li><b>Module names</b> that are the same in the main file and a library: the context (parameter bar) takes the module of the main file.</li>
</ul>

<h2>7 Screenshots and code excerpts</h2>
<p class="small">Largest image file: ${kib(maxImg)} (JPEG, quality 80).</p>
${blocks}
</main>
</body>
</html>
`;

mkdirSync(out, { recursive: true });
writeFileSync(join(out, "report.html"), html);

const page = await browser.newPage();
await page.goto(pathToFileURL(join(out, "report.html")).href, { waitUntil: "load" });
await page.evaluate(() => Promise.all([...document.images].map((i) => (i.complete ? null : new Promise((r) => (i.onload = i.onerror = r))))));
await page.pdf({
  path: join(out, "report.pdf"),
  format: "A4",
  printBackground: true,
  displayHeaderFooter: true,
  headerTemplate: "<span></span>",
  footerTemplate: '<div style="font-size:7pt;color:#777;width:100%;text-align:center">Test report 6 – Projects, parts, include/use · Page <span class="pageNumber"></span> of <span class="totalPages"></span></div>',
  margin: { top: "14mm", bottom: "16mm", left: "12mm", right: "12mm" },
});
await browser.close();
console.log(`report6: ${passed(e2e)}/${e2e.length} phase D, ${passed(regression)}/${regression.length} regression, ${unitPassed}/${unitTotal} Vitest, ${rustOk}/${rustTotal} Rust, OpenSCAD ${oscOk}/${osc.length}, ${used.size} images (max ${kib(maxImg)})`);
console.log(`-> ${join(out, "report.html")}`);
console.log(`-> ${join(out, "report.pdf")}`);
