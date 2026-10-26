#!/usr/bin/env node
// Test report 18 ("Sketch editor", plan 17.6/17.12, step 4): drawing mode with grid in the
// browser and in Tauri – every tool with a screenshot and the code it wrote, the snap display,
// undo, dragging handles, deleting segments, sketches under transformations, the dark theme,
// response times, the Tauri scenario, test runs, limits.
// Writes docs/reports/18-skizzen-editor/report.html (images in img/).
//
// Inputs (gitignored, reports/tmp/report18/):
//   scenarios.json, img/*.jpg, metrics.json     cd web && pnpm exec playwright test e2e/skizzen-editor.spec.ts
//   playwright.txt                              whole suite (cd web && pnpm e2e > …)
//   vitest.txt, typecheck.txt                   cd web && pnpm test, pnpm typecheck
//   cargo.txt, clippy.txt, fmt.txt              cargo test --workspace, clippy -D warnings, fmt --check
//   tauri/tauri-e2e.json, tauri/img/*.png       node web/e2e-tauri/desktop.e2e.mjs --only T16 --out reports/tmp/report18/tauri
//   wasm.json (optional)                        {"base", "before": {raw, gzip}, "after": {raw, gzip}}
//
//   node tools/report/report18.mjs
//   node tools/report/print-pdf.mjs docs/reports/18-skizzen-editor/report.html docs/reports/18-skizzen-editor/report.pdf "Test report 18: Sketch editor"

import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import os from "node:os";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "../..");
const input = join(root, "reports/tmp/report18");
const out = join(root, "docs/reports/18-skizzen-editor");
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
const md = (t) => esc(t).replace(/`([^`]+)`/g, "<code>$1</code>");
const pre = (t) => `<pre>${esc(String(t).trimEnd())}</pre>`;

function img(srcPath, dst, width = 1200) {
  if (!existsSync(srcPath)) return null;
  if (dst.endsWith(".jpg")) {
    try {
      execFileSync("convert", [srcPath, "-resize", `${width}x>`, "-quality", "78", join(imgDir, dst)]);
      return `img/${dst}`;
    } catch {
      dst = dst.replace(/\.jpg$/, ".png");
    }
  }
  copyFileSync(srcPath, join(imgDir, dst));
  return `img/${dst}`;
}
const pw = (n) => img(join(input, "img", `${n}.jpg`), `ui-${n}.jpg`);
const tauriImg = (n) => img(join(input, "tauri", "img", n), `tauri-${n.replace(/\.png$/, ".jpg")}`);
const fig = (s, cap) => (s ? `<figure><img src="${s}" alt=""><figcaption>${md(cap)}</figcaption></figure>` : `<p class="small">(image missing: ${esc(cap)})</p>`);

// ------------------------------------------------------------------------------- data

const scenarios = readJson(join(input, "scenarios.json")) ?? [];
const byId = Object.fromEntries(scenarios.map((s) => [s.id, s]));
const metrics = readJson(join(input, "metrics.json")) ?? {};
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
const NEW = /sketch_edit::|sketch_editor|sweep_continues|intersections_of_lines|step_rounding|dragging_a_sketch_point|sketch_op_info/;
const newTests = cargo ? [...cargo.matchAll(/^test (\S+) \.\.\. (ok|FAILED)$/gm)].map((m) => ({ name: m[1], res: m[2] })).filter((t) => NEW.test(t.name) || /^(new_sketch|line_|grid_points|snaps_by|tangent_line|arcs_bezier|center_arc|corner_tool|close_and|hole_blocks|plane_of|anchors_are|segment_under|draws_lines|sketch_under|syntax_errors_stop)/.test(t.name)) : [];
const clippy = readText(join(input, "clippy.txt"));
const fmt = readText(join(input, "fmt.txt"));
const clippyOk = clippy != null && !/^(error|warning)/m.test(clippy);
const fmtOk = fmt != null && fmt.trim() === "";
const pwText = readText(join(input, "playwright.txt"));
const pwSum = pwText ? { passed: +(/(\d+) passed/.exec(pwText)?.[1] ?? 0), failed: +(/(\d+) failed/.exec(pwText)?.[1] ?? 0), flaky: +(/(\d+) flaky/.exec(pwText)?.[1] ?? 0), skipped: +(/(\d+) skipped/.exec(pwText)?.[1] ?? 0) } : null;
const pwFailed = pwText ? [...pwText.matchAll(/^\s+\d+\) \[chromium\] › (.+)$/gm)].map((m) => m[1].trim()) : [];
const vitest = readText(join(input, "vitest.txt"));
const vitestSum = vitest ? (/Tests\s+(\d+) passed/.exec(vitest)?.[1] ?? null) : null;
const typecheck = readText(join(input, "typecheck.txt"));
const tauri = readJson(join(input, "tauri", "tauri-e2e.json"));
const t16 = tauri?.results?.find((r) => r.id === "T16") ?? null;
const wasm = readJson(join(input, "wasm.json"));
const hover = metrics.hover ?? null;

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

/** One scenario: caption, code next to the screenshots, checks. */
function scen(id, opts = {}) {
  const s = byId[id];
  if (!s) return `<p class="small">(scenario ${esc(id)} missing)</p>`;
  const images = (s.images ?? []).map((i) => fig(pw(i.name), i.label)).join("");
  const code = s.code ? pre(s.code) : "";
  const body = code && !opts.wide ? `<div class="codefig"><div>${code}</div><div>${images}</div></div>` : `${images}${code}`;
  return `<h3>${esc(s.id)} ${esc(s.title)}</h3><p>${md(s.caption)}</p>${body}<ul class="checks">${(s.checks ?? []).map((c) => `<li>${md(c)}</li>`).join("")}</ul>`;
}

const html = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Test report 18 – Sketch editor</title>
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
  ul.checks li { font-size: 8.2pt; }
  tr, td, figure, pre { break-inside: avoid; }
  .ok { color: var(--ok); font-weight: 600; } .bad { color: var(--bad); font-weight: 600; }
  figure { margin: 0 0 2mm; }
  figure img { max-width: 100%; max-height: 110mm; border: 1px solid var(--border); border-radius: 1mm; background: #fff; }
  figcaption { font-size: 7.8pt; color: var(--muted); }
  .codefig { display: grid; grid-template-columns: 0.85fr 1.15fr; gap: 3mm; align-items: start; break-inside: avoid; }
  .note { border-left: 1mm solid #e8b84a; padding: 1mm 2.5mm; background: #fff8e6; margin: 2mm 0; }
</style>
</head>
<body>
<main>
<h1>Test report 18 – Sketch editor</h1>
<div class="meta">Plan 17.7, step 4: drawing mode with grid in browser and Tauri · cad-eval (<code>sketch_edit.rs</code>, drawing state of sketches), cad-session (protocol <code>Sketch</code>), web UI (<code>app/sketch.ts</code>, <code>sketchUi.tsx</code>, <code>logic/sketchEditor.ts</code>, viewer) · ${esc(date)} · Branch ${esc(branch)} · Commit ${esc(commit)} · ${esc(rustc)} · ${os.cpus().length} cores</div>

<div class="summary">
  <div>Tools<b class="ok">9 + 2</b>line, 3 arcs, Bézier, rectangle, circle, round corner, select; close, delete</div>
  <div>cargo test --workspace<b class="${cargoSum && cargoSum.failed === 0 ? "ok" : "bad"}">${cargoSum ? `${cargoSum.ok} / ${cargoSum.ok + cargoSum.failed}` : "–"}</b>${newTests.length} of them new</div>
  <div>clippy · fmt<b class="${clippyOk && fmtOk ? "ok" : "bad"}">${clippy == null ? "–" : clippyOk ? "clean" : "finding"} · ${fmt == null ? "–" : fmtOk ? "clean" : "diff"}</b>-D warnings</div>
  <div>Vitest · Playwright<b class="${pwSum && pwSum.failed === 0 ? "ok" : "bad"}">${vitestSum ?? "–"} · ${pwSum ? `${pwSum.passed} / ${pwSum.passed + pwSum.failed}` : "–"}</b>new: Z01–Z17</div>
  <div>Tauri T16<b class="${t16?.ok ? "ok" : "bad"}">${t16 ? (t16.ok ? "passed" : "failed") : "–"}</b>${t16 ? `${t16.checks.length} checks, native kernel` : ""}</div>
  <div>Snap + preview<b>${hover ? `${num(hover.medianMs, 1)} ms` : "–"}</b>median per mouse move (browser, ${hover ? hover.segments : "–"} segments)</div>
</div>

<h2>1 Task and result</h2>
<p>Plan 17.6: tools in the view, drawing in the sketch plane with a grid, snap with indicator, every action writes code into the <code>sketch</code> block at the cursor or creates one, undo as with dragging, editing via handles, deleting segments; the code remains the single source. Implemented:</p>
<ul>
<li><b>Drawing mode</b> (toggle "Sketch"): camera perpendicular to the sketch plane – also under transformations, in <code>rotate_extrude</code> (xz plane) and in centered <code>linear_extrude</code> (bottom face); step grid, grid lines and grid points of the module or the sketch in the plane; toolbar, status line, Esc exits.</li>
<li><b>Tools</b> line (chain, a click on the start closes), arc through three points, arc around a center, tangent arc, Bézier (cubic), rectangle, circle, round corner (<code>corner(r)</code>), close, delete, select. Every action is a text edit in the existing language and one undo step.</li>
<li><b>Snap</b> to grid points and anchors (references such as <code>g[b, u]</code>, <code>k.ecke</code>), start, corners, end points, midpoints and intersections, arc midpoints, tangent (<code>line(length = …)</code>), horizontal/vertical, grid lines (<code>[g.b, 40]</code>), step size; Alt = free with step size. The marker shows the kind, the label shows the text that would be written.</li>
<li><b>Editing:</b> with "Select" the handles of all segments (existing handle logic: change a literal, rewrite a grid reference), a click on a segment sets the cursor, Del deletes.</li>
<li><b>Logic in Rust:</b> sketch and insertion point, drawing state, snap, code, preview, deletion, corners, grid, plane and ray intersection in <code>cad-eval</code>/<code>cad-session</code>; the UI forwards mouse and keys and applies the response. The same via <code>WorkerTransport</code> (Wasm) and <code>TauriTransport</code> (native engine process).</li>
</ul>

<h2>2 Decisions</h2>
<table>
<tr><th>Question</th><th>Decision</th></tr>
<tr><td>Where the code goes</td><td>Sketch = the <code>sketch</code> statement that contains the cursor or starts on its line. Insertion after the statement on the cursor line (or the last one before it), in the innermost <code>hole</code> block; on the <code>sketch {</code> line at the end; empty blocks are expanded. Without a sketch: new <code>sketch { … }</code> block at the end of the context module or the file. After every action the cursor is placed after the new code.</td></tr>
<tr><td>Current point</td><td>From the evaluation: <code>SketchInfo::steps</code> holds every segment statement in evaluation order with the state after it (contour, start, point, direction; geometry before <code>corner()</code>) – also for loops and module calls in the block. Line, arcs and Bézier continue the open contour if their first point is its end; otherwise <code>start(p)</code>. A new contour in the middle of a continuing one goes after its end.</td></tr>
<tr><td>Tool state</td><td>The engine is stateless: the chosen points (position, text, kind) go out with every request and come back with the response. At most one request in flight, the last mouse position wins, clicks in order.</td></tr>
<tr><td>Undo</td><td>One transaction per action with <code>isolateHistory</code> (also <code>start</code> + first line together); the source goes to the engine immediately, which processes messages in order – the next request sees the new text. Ctrl+Z also works with focus in the view.</td></tr>
<tr><td>References instead of numbers</td><td>Grid points and anchors write their reference; a rectangle from two grid points also gets the other corners as grid points. Free points: numbers rounded to the step size; exact points (end points, intersections …): shortest number with 6 digits. 2D points write <code>g[a, b]</code> (also with handle snap, instead of <code>g[a, b, _]</code>).</td></tr>
<tr><td>Plane</td><td><code>sketch_frame</code>: node coordinate system, in <code>rotate_extrude</code> the xz plane, in centered <code>linear_extrude</code> the bottom face – for editor, handles (<code>Handle.frame</code>) and rendering (<code>OpInfo::Sketch.plane</code>).</td></tr>
<tr><td>Handles and tools</td><td>Drawing tools also draw on existing points (no handles there to catch the click); handles of all segments with "Select".</td></tr>
<tr><td>Tolerance</td><td>Snap radius in pixels (settings, 14 px) times pixel size at the ray hit point; the UI sends only ray, pixel size and radius.</td></tr>
</table>

<h2>3 Flow</h2>
${pre(`Mouse moved     → Sketch { cursor, tool, points, ray, pixel, snap_px, free }        (≤ 1 in flight)
                 ← Sketch { plane, snap {point, text, kind, label}, guides, preview, prompt }
Click           → Sketch { …, click: true }
                 ← points (point stored)  or  edits + select (action done)
                   UI: one transaction (undo step), cursor after the code, SetSource immediately
Close/Del       → Sketch { tool: close | delete, cursor }  ← edits

Engine (cad-eval::sketch_edit, Session):  place → sketch, insertion point, drawing state, grid/anchors
                                          snap → point + text + kind     click → code   preview → polylines
UI (app/sketch.ts, viewer):               ray, keys, apply response, draw, camera`)}

<h2>4 Tools</h2>
<p>Each scenario draws with the mouse (Playwright, real pointer events) and checks the written code verbatim; left the code after the action, right the view.</p>
${scen("Z02")}
${scen("Z03")}
${scen("Z04")}
${scen("Z05")}
${scen("Z06")}
${scen("Z07")}
${scen("Z08")}
${scen("Z09")}
${scen("Z10")}

<h2>5 Snap</h2>
${scen("Z11")}

<h2>6 Editing: undo, handles, delete</h2>
${scen("Z12")}
${scen("Z13")}
${scen("Z14")}

<h2>7 Drawing mode, planes, theme</h2>
${scen("Z01")}
${scen("Z15")}
${scen("Z16", { wide: true })}

<h2>8 Desktop app (Tauri)</h2>
<p>Scenario T16 in <code>web/e2e-tauri/desktop.e2e.mjs</code> (tauri-driver, WebKitWebDriver, Xvfb): the real app <code>cad</code> with the native engine process, messages via <code>TauriTransport</code>. The view is operated with pointer events on the canvas (WebDriver has no mouse over WebGL content that can be moved reliably).</p>
${t16 ? `<p>Result: <span class="${t16.ok ? "ok" : "bad"}">${t16.ok ? "passed" : "failed"}</span> in ${num(t16.ms / 1000, 1)} s${tauri?.metrics?.sketchTauri?.hoverMedianMs != null ? `, snap + preview median ${num(tauri.metrics.sketchTauri.hoverMedianMs, 1)} ms per move` : ""}.</p><ul class="checks">${t16.checks.map((c) => `<li class="${c.ok ? "" : "bad"}">${md(c.text)}</li>`).join("")}</ul>${(t16.images ?? []).map((i) => fig(tauriImg(i.name), i.label)).join("")}${t16.error ? pre(t16.error) : ""}` : "<p>(missing)</p>"}

<h2>9 Measurements</h2>
${scen("Z17", { wide: true })}
${wasm ? `<h3>Wasm module</h3><table><tr><th></th><th>raw</th><th>gzip -9</th></tr><tr><td><code>cad_wasm_bg.wasm</code> before (commit ${esc(wasm.base)})</td><td class="n">${num(wasm.before.raw)} B</td><td class="n">${num(wasm.before.gzip)} B</td></tr><tr><td>with sketch editor</td><td class="n">${num(wasm.after.raw)} B</td><td class="n">${num(wasm.after.gzip)} B</td></tr><tr><td>growth</td><td class="n">${num(wasm.after.raw - wasm.before.raw)} B</td><td class="n">${num(wasm.after.gzip - wasm.before.gzip)} B</td></tr></table>` : ""}

<h2>10 Test runs</h2>
<table>
<tr><th>Run</th><th>Result</th></tr>
<tr><td><code>cargo test --workspace -- --test-threads=2</code></td><td>${cargoSum ? `${cargoSum.ok} passed, ${cargoSum.failed} failed` : "–"}</td></tr>
<tr><td><code>cargo clippy --workspace --all-targets -- -D warnings</code> · <code>cargo fmt --check</code></td><td>${clippyOk ? "clean" : "finding"} · ${fmtOk ? "clean" : "diff"}</td></tr>
<tr><td><code>pnpm typecheck</code> · <code>pnpm test</code> (Vitest)</td><td>${typecheck !== null ? (/error TS/i.test(typecheck) ? "errors" : "clean") : "–"} · ${vitestSum ?? "–"} passed</td></tr>
<tr><td><code>pnpm e2e</code> (Playwright, Chromium, full suite)</td><td>${pwSum ? `${pwSum.passed} passed, ${pwSum.failed} failed${pwSum.skipped ? `, ${pwSum.skipped} skipped` : ""}${pwFailed.length ? `: ${pwFailed.map(esc).join("; ")}` : ""}` : "–"}</td></tr>
<tr><td>Tauri E2E T16</td><td>${t16 ? (t16.ok ? "passed" : "failed") : "–"}</td></tr>
</table>
<p>New Rust tests (${newTests.length}):</p>
<p class="small">${newTests.map((x) => `<code>${esc(x.name)}</code> ${x.res === "ok" ? '<span class="ok">ok</span>' : '<span class="bad">FAILED</span>'}`).join(" · ")}</p>
<p class="small">cad-eval <code>tests/sketch_edit.rs</code>: insertion point, new sketch in file and context module, chains, new contour after a continuing one, snap kinds, intersections, tangent, rectangle with grid references, arcs, Bézier, circle, center arc on grid points, inserting and changing corners, close, delete, <code>hole</code> and empty blocks, planes, anchors, segment under the mouse, handle snap in short form; unit tests in the module (arc angle over 180°, intersections, rounding). cad-session <code>tests/sketch_editor.rs</code>: protocol with ray (new sketch, chain, close, delete), plane under transformation, syntax errors, selection, <code>OpInfo.plane</code>. Vitest <code>logic/sketchEditor.test.ts</code>: grid, camera, markers, undo step with cursor.</p>

<h2>11 Limitations</h2>
<ul>
<li>Main file only: in another project file the mode does not start; sketches from included files are not a target.</li>
<li>Statements in loops or modules are not deleted, and their corner does not get a <code>corner()</code> (message). The drawing state in the middle of a loop is the one after its last iteration.</li>
<li>Snap only to the sketch, visible grids and anchors – not to body edges or other sketches; horizontal/vertical only relative to the previous point.</li>
<li>Bézier only cubic, no spline tool; <code>line(by = …)</code> and <code>length</code> + <code>angle</code> are not generated (except tangent <code>line(length = …)</code>).</li>
<li>A new sketch is a bare 2D sketch (to get a body: e.g. write <code>linear_extrude</code> in front of it).</li>
<li>The camera keeps the orbit control's "z up": in rotated planes the sketch's y axis is not necessarily vertical on screen. With a drawing tool the left mouse button does not rotate the camera (right pans, wheel zooms, "Select" to rotate).</li>
<li>Tauri: T16 operates the view with synthetic pointer events.</li>
</ul>
</main>
</body>
</html>
`;
writeFileSync(join(out, "report.html"), html);
console.log(`${join(out, "report.html")}: ${html.length} characters, ${scenarios.length} scenarios`);
