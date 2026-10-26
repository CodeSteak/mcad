#!/usr/bin/env node
// Test report 19 ("Measuring", plan 18): measure mode in the browser and in Tauri – every kind with
// a screenshot, measuring in the section, the list (copy, delete), provenance, the dark theme,
// response times; `cad measure` with the same kinds (text and JSON), analytic test models
// against their formulas, test runs, limits.
// Writes docs/reports/19-messen/report.html (images in img/).
//
// Inputs (gitignored, reports/tmp/report19/):
//   scenarios.json, img/*.jpg, metrics.json     cd web && pnpm exec playwright test e2e/messen.spec.ts
//   cli/*.txt, cli/*.json, cli/*.png,           CAD=target/release/mcad tools/report/report19-data.sh
//   analytic.json
//   playwright.txt                              whole suite (cd web && pnpm e2e > …)
//   vitest.txt, typecheck.txt                   cd web && pnpm test, pnpm typecheck
//   cargo.txt, clippy.txt, fmt.txt              cargo test --workspace, clippy -D warnings, fmt --check
//   tauri/tauri-e2e.json, tauri/img/*.png       node web/e2e-tauri/desktop.e2e.mjs --only T17 --out reports/tmp/report19/tauri
//   wasm.json (optional)                        {"base", "before": {raw, gzip}, "after": {raw, gzip}}
//
//   node tools/report/report19.mjs
//   node tools/report/print-pdf.mjs docs/reports/19-messen/report.html docs/reports/19-messen/report.pdf "Test report 19: Measuring"

import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import os from "node:os";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "../..");
const input = join(root, "reports/tmp/report19");
const out = join(root, "docs/reports/19-messen");
const imgDir = join(out, "img");
mkdirSync(imgDir, { recursive: true });

const esc = (s) => String(s ?? "").replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
const readText = (f) => (existsSync(f) ? readFileSync(f, "utf8") : null);
const readJson = (f) => (existsSync(f) ? JSON.parse(readFileSync(f, "utf8")) : null);
const gitOut = (...a) => {
  try {
    return execFileSync("git", a, { cwd: root, encoding: "utf8" }).trim();
  } catch {
    return "–";
  }
};
// English number format without thousands separators (as in the translated report 19)
const num = (x, d = 0) => (x === null || x === undefined || Number.isNaN(Number(x)) ? "–" : Number(x).toLocaleString("en-US", { minimumFractionDigits: d, maximumFractionDigits: d, useGrouping: false }));
const md = (t) => esc(t).replace(/`([^`]+)`/g, "<code>$1</code>");
const pre = (t) => `<pre>${esc(String(t).trimEnd())}</pre>`;
const term = (t) => `<pre class="term">${esc(String(t).trimEnd())}</pre>`;

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
const cliImg = (n) => img(join(input, "cli", n), `cli-${n.replace(/\.png$/, ".jpg")}`, 900);
const fig = (s, cap) => (s ? `<figure><img src="${s}" alt=""><figcaption>${md(cap)}</figcaption></figure>` : `<p class="small">(image missing: ${esc(cap)})</p>`);

// ------------------------------------------------------------------------------- data

const scenarios = readJson(join(input, "scenarios.json")) ?? [];
const byId = Object.fromEntries(scenarios.map((s) => [s.id, s]));
const metrics = readJson(join(input, "metrics.json")) ?? {};
const analytic = readJson(join(input, "analytic.json")) ?? [];
const cliTxt = (n) => readText(join(input, "cli", `${n}.txt`));
const cliJson = (n) => readJson(join(input, "cli", `${n}.json`));
const times = Object.fromEntries((readText(join(input, "cli", "times.txt")) ?? "").trim().split("\n").filter(Boolean).map((l) => l.split(" ")).map(([k, v]) => [k, +v]));
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
// new tests: cad-measure (unit + analytic), cad-session measure, cad-cli messen
const NEW = /^(curve::tests|surface::tests|num::tests|dist::tests|cylinder_radius|cone_sphere|fillet_and_chamfer|distances$|section_through|snaps_vertices|distance_by_clicks|radius_of_bores|housing_wall|angle_of_a_chamfer|section_mode_over|housing_section_wall|hole_plate_radii|chamfer_angle_and_at|usage_errors)/;
const newTests = cargo ? [...cargo.matchAll(/^test (\S+) \.\.\. (ok|FAILED)$/gm)].map((m) => ({ name: m[1], res: m[2] })).filter((t) => NEW.test(t.name)) : [];
const clippy = readText(join(input, "clippy.txt"));
const fmt = readText(join(input, "fmt.txt"));
// Exit-code lines ("fmt 0", "clippy 0", "clippy-wasm 0") may follow the tool output.
const exitOk = (t) => !/^(fmt|clippy|clippy-wasm) [1-9]\d*$/m.test(t);
const clippyOk = clippy != null && !/^(error|warning)/m.test(clippy) && exitOk(clippy);
const fmtOk = fmt != null && fmt.replace(/^fmt \d+$/gm, "").trim() === "" && exitOk(fmt);
const pwText = readText(join(input, "playwright.txt"));
const pwSum = pwText ? { passed: +(/(\d+) passed/.exec(pwText)?.[1] ?? 0), failed: +(/(\d+) failed/.exec(pwText)?.[1] ?? 0), flaky: +(/(\d+) flaky/.exec(pwText)?.[1] ?? 0), skipped: +(/(\d+) skipped/.exec(pwText)?.[1] ?? 0) } : null;
const pwFailed = pwText ? [...pwText.matchAll(/^\s+\d+\) \[chromium\] › (.+)$/gm)].map((m) => m[1].trim()) : [];
const vitest = readText(join(input, "vitest.txt"));
const vitestSum = vitest ? (/Tests\s+(\d+) passed/.exec(vitest)?.[1] ?? null) : null;
const typecheck = readText(join(input, "typecheck.txt"));
const tauri = readJson(join(input, "tauri", "tauri-e2e.json"));
const t17 = tauri?.results?.find((r) => r.id === "T17") ?? null;
const wasm = readJson(join(input, "wasm.json"));
const hover = metrics.hoverRtt ?? null;
const limitOf = (r) => (/kernel/i.test(r.name) ? 1e-3 : 1e-6);
const analyticOk = analytic.filter((r) => r.err !== null && Math.abs(r.err) <= Math.max(limitOf(r) * Math.abs(r.expect), 1e-9)).length;

const date = new Date().toISOString().slice(0, 10);
const commit = gitOut("rev-parse", "--short", "HEAD");
const branch = gitOut("rev-parse", "--abbrev-ref", "HEAD");
const rustc = (() => {
  try {
    return execFileSync("rustc", ["--version"], { encoding: "utf8" }).trim();
  } catch {
    return "–";
  }
})();

function scen(id, opts = {}) {
  const s = byId[id];
  if (!s) return `<p class="small">(scenario ${esc(id)} missing)</p>`;
  const images = (s.images ?? []).map((i) => fig(pw(i.name), i.label)).join("");
  const code = s.code ? pre(s.code) : "";
  const body = code && !opts.wide && s.code.includes("\n") ? `<div class="codefig"><div>${code}</div><div>${images}</div></div>` : `${images}${code}`;
  return `<h3>${esc(s.id)} ${esc(s.title)}</h3><p>${md(s.caption)}</p>${body}<ul class="checks">${(s.checks ?? []).map((c) => `<li>${md(c)}</li>`).join("")}</ul>`;
}

const sci = (x) => (x === null || x === undefined ? "–" : x === 0 ? "0" : Math.abs(x) < 1e-3 ? x.toExponential(1) : num(x, 6));
const analyticTable = analytic.length
  ? `<table><tr><th>Case</th><th>Call</th><th class="n">expected</th><th class="n">measured</th><th class="n">deviation</th><th>exact</th></tr>${analytic
      .map((r) => {
        const ok = r.err !== null && Math.abs(r.err) <= Math.max(limitOf(r) * Math.abs(r.expect), 1e-9);
        return `<tr><td>${esc(r.name)}</td><td><code>${esc(r.args)}</code></td><td class="n">${num(r.expect, 6)}</td><td class="n">${r.got === null ? esc(r.label) : num(r.got, 6)}</td><td class="n ${ok ? "ok" : "bad"}">${sci(r.err)}</td><td>${r.exact ? "yes" : r.exact === false ? `no${r.tol ? ` (± ${sci(r.tol)})` : ""}` : "–"}</td></tr>`;
      })
      .join("")}</table>`
  : "<p>(missing: tools/report/report19-data.sh)</p>";

const jsonExcerpt = (() => {
  const j = cliJson("plate-radius");
  if (!j?.measurements) return null;
  const m = j.measurements[0];
  const short = { kind: m.kind, title: m.title, value: m.value, unit: m.unit, label: m.label, values: m.values, exact: m.exact, details: m.details, picks: m.picks.map((p) => ({ kind: p.kind, reference: p.reference, label: p.label, origin: p.origin })), cli: m.cli, text: m.text };
  return JSON.stringify(short, null, 1);
})();
const sectionExcerpt = (() => {
  const j = cliJson("plate-section");
  if (!j?.section) return null;
  const s = j.section;
  return JSON.stringify({ plane: s.plane, exact: s.exact, max_dev: s.max_dev, area: s.area, regions: s.regions.map((r) => ({ area: r.area, holes: r.holes, bounds: r.bounds, contours: r.contours.length })), circles: s.circles, min_wall: s.min_wall }, null, 1);
})();

const html = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Test report 19 – Measuring</title>
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
  td.n, th.n { text-align: right; white-space: nowrap; }
  code, pre { font-family: ui-monospace, "SFMono-Regular", Menlo, Consolas, monospace; font-size: 7.6pt; }
  pre { margin: 0 0 1.5mm; padding: 1.4mm 2mm; background: #f6f8fa; border: 1px solid var(--border); border-radius: 1mm; white-space: pre-wrap; word-break: break-word; }
  pre.term { background: #16191d; color: #e6e9ed; border-color: #32373e; }
  .small { font-size: 7.6pt; color: var(--muted); font-weight: 400; }
  ul { margin: 1mm 0 2mm 5mm; padding: 0; }
  li { margin: 0.5mm 0; }
  ul.checks li { font-size: 8.2pt; }
  tr, td, figure, pre { break-inside: avoid; }
  .ok { color: var(--ok); font-weight: 600; } .bad { color: var(--bad); font-weight: 600; }
  figure { margin: 0 0 2mm; }
  figure img { max-width: 100%; max-height: 110mm; border: 1px solid var(--border); border-radius: 1mm; background: #fff; }
  figcaption { font-size: 7.8pt; color: var(--muted); }
  .codefig { display: grid; grid-template-columns: 0.8fr 1.2fr; gap: 3mm; align-items: start; break-inside: avoid; }
  .two { display: grid; grid-template-columns: 1fr 1fr; gap: 3mm; align-items: start; }
</style>
</head>
<body>
<main>
<h1>Test report 19 – Measuring</h1>
<div class="meta">Plan 18: measurement tool in UI (browser, Tauri) and CLI with shared logic in Rust · cad-measure (new), cad-engine (<code>measure_parts</code>, <code>section_parts</code>), cad-session (<code>measure.rs</code>, protocol <code>Measure</code>), cad-cli (<code>measure.rs</code>), web UI (<code>app/measure.ts</code>, <code>measureUi.tsx</code>, viewer) · ${esc(date)} · Branch ${esc(branch)} · Commit ${esc(commit)} · ${esc(rustc)} · ${os.cpus().length} cores</div>

<div class="summary">
  <div>Measurement kinds<b class="ok">7</b>distance, radius, angle, length, area, body, wall thickness – also in the section</div>
  <div>Analytic cases (CLI)<b class="${analyticOk === analytic.length && analytic.length ? "ok" : "bad"}">${analytic.length ? `${analyticOk} / ${analytic.length}` : "–"}</b>within 10⁻⁶ relative (kernel volume 10⁻³)</div>
  <div>cargo test --workspace<b class="${cargoSum && cargoSum.failed === 0 ? "ok" : "bad"}">${cargoSum ? `${cargoSum.ok} / ${cargoSum.ok + cargoSum.failed}` : "–"}</b>${newTests.length} of them new</div>
  <div>clippy · fmt<b class="${clippyOk && fmtOk ? "ok" : "bad"}">${clippy == null ? "–" : clippyOk ? "clean" : "finding"} · ${fmt == null ? "–" : fmtOk ? "clean" : "diff"}</b>-D warnings</div>
  <div>Vitest · Playwright<b class="${pwSum && pwSum.failed === 0 ? "ok" : "bad"}">${vitestSum ?? "–"} · ${pwSum ? `${pwSum.passed} / ${pwSum.passed + pwSum.failed}` : "–"}</b>new: M01–M11</div>
  <div>Tauri T17<b class="${t17?.ok ? "ok" : "bad"}">${t17 ? (t17.ok ? "passed" : "failed") : "–"}</b>${t17 ? `${t17.checks.length} checks, native kernel` : ""}</div>
  <div>Response under the mouse<b>${hover ? `${num(hover.median, 1)} ms` : "–"}</b>median (browser, Wasm, ${hover ? hover.n : "–"} responses)</div>
</div>

<h2>1 Task and result</h2>
<p>Task: extend the measurement tool in the UI (browser and Tauri) and in the CLI – distance point–point and between points, edges, faces (minimal, with foot points, parallel planes as normal distance), radius/diameter of circular edges, arcs, cylinder, cone, sphere and torus faces exactly from the B-rep with axis/center, angle, length, area, bounding box/volume; measuring in the active section (snap to contours, distances in the plane, radius of section circles, wall thickness, contour area) with exact section curves; snap, dimension lines and labels, multiple measurements, delete, copy as text/comment/<code>cad measure</code>; link to the language; the same measurement kinds in <code>cad measure</code> with JSON. Implemented:</p>
<ul>
<li><b>Shared logic in Rust:</b> new crate <code>cad-measure</code> (exact edges and faces, distance, angle, radius, area, section, wall thickness), in <code>cad-session</code> the measurement model per node, snap, provenance, texts and CLI arguments – the same functions serve the protocol (UI) and the CLI references.</li>
<li><b>Exactness:</b> analytic geometry in closed form; section curves as plane ∩ B-rep from the kernel, their segments on analytic surfaces as line, circle or ellipse (with the deviation of the kernel curve); otherwise with a stated tolerance. Every measurement carries <code>exact</code> and <code>tol</code>.</li>
<li><b>UI:</b> mode "Measure" with context bar (measurement kind, "In section"), snap with indicator, dimension lines, guide lines, labels (checked light/dark), list with copy and delete – as standalone building blocks for the UI rework.</li>
<li><b>CLI:</b> <code>--distance</code>, <code>--radius</code>, <code>--angle</code>, <code>--length</code>, <code>--area face@P</code>, <code>--body</code>, <code>--section …</code> (contours, circles, thinnest wall), <code>--wall</code>, <code>--at</code>; element references <code>vertex@P</code>, <code>mid@P</code>, <code>center@P</code>, <code>edge@P</code>, <code>face@P</code>, <code>body@P</code>, <code>region@P</code>; JSON <code>measurements</code> (protocol type), <code>section</code>, <code>at</code>.</li>
</ul>

<h2>2 Decisions</h2>
<table>
<tr><th>Question</th><th>Decision</th></tr>
<tr><td>Where measuring happens</td><td>In the engine (worker or engine process) on the neutral B-rep of the node parts in context coordinates; the UI sends ray, its raycast hit (node, face ID, point), pixel size and snap radius, and receives the element under the mouse and, after the last click, the measurement with dimension lines. As with the sketch editor, no selection state in the engine.</td></tr>
<tr><td>Distance</td><td>Coarse search over the tessellation (BVH), refinement by alternating projection onto the exact curves and surfaces; outside a trimmed face onto its boundary. Parallel planes: normal distance as the value, the minimum distance of the bounded faces alongside; parallel axes: axis distance.</td></tr>
<tr><td>Radius</td><td>From the B-rep parameters; fillets on circular edges (in SMLib surfaces of revolution with a circular-arc generatrix) are recognized, free-form via principal curvature (approximated).</td></tr>
<tr><td>Angle of two planes</td><td>Angle between the planes (0–90°, chamfer 45°), plus the opening angle at the intersection line and the angle of the normals.</td></tr>
<tr><td>Section</td><td>Kernel Boolean with the kept half-space (convention of <code>cad render --section</code>), cap faces as regions; segments from the cut surface in closed form. One Boolean per part and plane, cached.</td></tr>
<tr><td>Wall thickness</td><td>At a contour point perpendicular to the contour into the material up to the next contour; without a point the minimum over samples (estimate).</td></tr>
<tr><td>Link to the language</td><td>Face tags → nodes → lines and named instances; plus grid points, grid lines, grid planes and anchors on which a point, an axis or a plane lies.</td></tr>
<tr><td>CLI references</td><td>The element nearest to a point (<code>face@[x,y,z]</code>): unambiguous and readable, the UI copies measurements in this form; <code>--at P</code> shows what lies at P.</td></tr>
<tr><td>Place in the UI</td><td>Context bar and list as standalone components (<code>MeasureBar</code>, <code>MeasurePanel</code>); for now in the stack at the top right and to the right of the view, per the UI concept later at the top left below the toolbar and as a tab "Measure".</td></tr>
</table>

<h2>3 Flow</h2>
${pre(`Mouse moved  → Measure { kind, picks, ray, hit {node, face, point}, pixel, snap_px, free, section? | clip? }
              ← Measure { hover {kind, label, point, reference, origin, lines}, prompt, section {regions, area, exact} }
Click        → Measure { …, click: true }
              ← picks (selection stored)  or  result: Measurement { value, label, values, lines, guides, points,
                                                                    at, details, exact, tol, text, cli }
Engine:  measure_model(node)  = per part B-rep + tessellation (context)      measure_section(node, plane) = Boolean
         snap → vertex | edge midpoint | circle center | edge | face  (in section: contour, region)
         measurement → cad_measure::{distance, radius, angle, face_area, Section::wall …}
CLI:     face@[x,y,z] → same selection → same measurement → JSON/text`)}

<h2>4 Measurement kinds in the UI</h2>
<p>All scenarios click with the mouse (Playwright, Chromium with SwiftShader) and check the values against the geometry.</p>
${scen("M01")}
${scen("M02")}
${scen("M03")}
${scen("M04")}
${scen("M05", { wide: true })}

<h2>5 Measuring in the section</h2>
${scen("M06")}

<h2>6 List, copy, link to the language, theme</h2>
${scen("M07")}
${scen("M08", { wide: true })}
${scen("M09")}

<h2>7 Desktop app (Tauri)</h2>
<p>Scenario T17 in <code>web/e2e-tauri/desktop.e2e.mjs</code> (tauri-driver, WebKitWebDriver, Xvfb): the app <code>cad</code> with the native engine process, messages via <code>TauriTransport</code>; the view is operated with pointer events on the canvas.</p>
${t17 ? `<p>Result: <span class="${t17.ok ? "ok" : "bad"}">${t17.ok ? "passed" : "failed"}</span> in ${num(t17.ms / 1000, 1)} s${tauri?.metrics?.measureTauri?.hoverMedianMs != null ? `, response under the mouse median ${num(tauri.metrics.measureTauri.hoverMedianMs, 1)} ms` : ""}.</p><ul class="checks">${t17.checks.map((c) => `<li class="${c.ok ? "" : "bad"}">${md(c.text)}</li>`).join("")}</ul>${(t17.images ?? []).map((i) => fig(tauriImg(i.name), i.label)).join("")}${t17.error ? pre(t17.error) : ""}` : "<p>(missing)</p>"}

<h2>8 CLI: <code>cad measure</code></h2>
<p>The same measurements without UI, for agents with <code>--json</code>. Output from <code>tools/report/report19-data.sh</code> (release binary, without daemon; times per call as the mean of 3 runs including loading and kernel).</p>
<h3>Housing in section z = 10: regions, wall thickness, inner dimension (${num(times["housing-section"])} ms)</h3>
<div class="two"><div>${term(cliTxt("housing-section") ?? "(missing)")}</div><div>${fig(cliImg("housing-section.png"), "`cad render gehaeuse.scad --section z=10 --view top`: the two wall regions")}</div></div>
<h3>Housing: inner distance, part, edge length (${num(times["housing-3d"])} ms)</h3>
${term(cliTxt("housing-3d") ?? "(missing)")}
<h3>Hole plate: bore, fillet, pitch (${num(times["plate-radius"])} ms)</h3>
${term(cliTxt("plate-radius") ?? "(missing)")}
<h3>Hole plate in section z = 2: circles, area, radius, wall (${num(times["plate-section"])} ms)</h3>
<div class="two"><div>${term(cliTxt("plate-section") ?? "(missing)")}</div><div>${fig(cliImg("plate-section.png"), "`cad render lochplatte.scad --section z=2 --view top`")}</div></div>
<h3>What lies at P: <code>--at</code> (${num(times["plate-at"])} ms)</h3>
${term(cliTxt("plate-at") ?? "(missing)")}
<h3>Angle of a chamfer (${num(times["chamfer-angle"])} ms)</h3>
${term(cliTxt("chamfer-angle") ?? "(missing)")}
<h3>JSON for agents (excerpt: radius of the bore)</h3>
${jsonExcerpt ? pre(jsonExcerpt) : "<p>(missing)</p>"}
<h3>JSON of the section (excerpt)</h3>
${sectionExcerpt ? pre(sectionExcerpt) : "<p>(missing)</p>"}

<h2>9 Analytic check</h2>
<p>Small models, measured via the CLI (same logic as the UI), against the formula. Limit 10⁻⁶ relative; volume comes from the kernel's integration (SMLib, ~10⁻⁴) and has the limit 10⁻³.</p>
${analyticTable}

<h2>10 Measurements</h2>
${scen("M10", { wide: true })}
${wasm ? `<h3>Wasm module</h3><table><tr><th></th><th class="n">raw</th><th class="n">gzip -9</th></tr><tr><td><code>cad_wasm_bg.wasm</code> before (commit ${esc(wasm.base)})</td><td class="n">${num(wasm.before.raw)} B</td><td class="n">${num(wasm.before.gzip)} B</td></tr><tr><td>with measuring</td><td class="n">${num(wasm.after.raw)} B</td><td class="n">${num(wasm.after.gzip)} B</td></tr><tr><td>growth</td><td class="n">${num(wasm.after.raw - wasm.before.raw)} B</td><td class="n">${num(wasm.after.gzip - wasm.before.gzip)} B</td></tr></table>` : ""}

<h2>11 Test runs</h2>
<table>
<tr><th>Run</th><th>Result</th></tr>
<tr><td><code>cargo test --workspace -- --test-threads=2</code></td><td>${cargoSum ? `${cargoSum.ok} passed, ${cargoSum.failed} failed` : "–"}</td></tr>
<tr><td><code>cargo clippy --workspace --all-targets -- -D warnings</code> (native and <code>-p cad-wasm --target wasm32-unknown-unknown</code>) · <code>cargo fmt --check</code></td><td>${clippyOk ? "clean" : "finding"} · ${fmtOk ? "clean" : "diff"}</td></tr>
<tr><td><code>pnpm typecheck</code> · <code>pnpm test</code> (Vitest)</td><td>${typecheck !== null ? (/error TS/i.test(typecheck) ? "errors" : "clean") : "–"} · ${vitestSum ?? "–"} passed</td></tr>
<tr><td><code>pnpm e2e</code> (Playwright, Chromium, full suite)</td><td>${pwSum ? `${pwSum.passed} passed, ${pwSum.failed} failed${pwSum.skipped ? `, ${pwSum.skipped} skipped` : ""}${pwSum.flaky ? `, ${pwSum.flaky} flaky` : ""}${pwFailed.length ? `: ${pwFailed.map(esc).join("; ")}` : ""}` : "–"}</td></tr>
<tr><td>Tauri E2E T17</td><td>${t17 ? (t17.ok ? "passed" : "failed") : "–"}</td></tr>
<tr><td>Analytic cases via the CLI</td><td>${analytic.length ? `${analyticOk} of ${analytic.length} within the limit` : "–"}</td></tr>
</table>
<p class="small">Flaky tests fixed (previously red sporadically, also without measure mode): <code>waitHandles</code> still answered with the handles of the previous cursor line (B06, B10) and now waits for settled handles; <code>clickAt</code> in M01–M11 and the click in T17 wait for the click's own effect instead of any response; T17 picks faces with Alt (without point snapping). Nothing skipped or disabled.</p>
<p>New Rust tests (${newTests.length}):</p>
<p class="small">${newTests.map((x) => `<code>${esc(x.name)}</code> ${x.res === "ok" ? '<span class="ok">ok</span>' : '<span class="bad">FAILED</span>'}`).join(" · ")}</p>
<p class="small">cad-measure <code>tests/analytic.rs</code>: cylinder (radius, axis, lateral and cap area, circle length, normal distance), cone (radius, half angle, apex, lateral area), sphere and torus (radii, areas, bounding box), fillet cylinder and fillet sphere of a cuboid, fillet torus of a circular edge, chamfer 45°/135°, distances, section through two bores (exact circles, area, wall thicknesses, longitudinal section, oblique section as ellipse); unit tests of curves, surfaces and quadrature. cad-session <code>tests/measure.rs</code>: snap (face, vertex, edge midpoint, edge) with provenance, clicks and CLI arguments, references, hole plate, housing with wall thickness in the section, part volume, chamfer, section mode via the protocol. cad-cli <code>tests/messen.rs</code>: JSON and text of the new options, usage errors. Vitest <code>logic/measure.test.ts</code>.</p>

<h2>12 Limitations</h2>
<ul>
<li>The section needs one kernel Boolean per part and plane; if it fails, there is no section (message), no substitute contour from the mesh.</li>
<li>Contours on free-form surfaces and on obliquely cut cones, tori and surfaces of revolution remain kernel curves (<code>exact = false</code>).</li>
<li>Wall thickness perpendicular to the contour at one location; the minimum wall thickness of the section is an estimate from samples.</li>
<li>Distances between free-form surfaces and cases in which the projection ends at a different local minimum hold within the tessellation tolerance; areas of free-form faces from the triangles; volume from the kernel (~10⁻⁴).</li>
<li>CLI references via the nearest point are unambiguous as long as P is closest to one element; results appear in the order of the options.</li>
<li>2D results (sketches without extrusion) cannot be measured; the list of measurements is not saved.</li>
<li>Until the UI rework the context bar sits in the stack at the top right (the view toolbar wraps), the list to the right of the view.</li>
<li>Tauri T17 operates the view with synthetic pointer events; timings from SwiftShader or software GL are only meaningful relative to each other.</li>
</ul>
</main>
</body>
</html>
`;
writeFileSync(join(out, "report.html"), html);
console.log(`${join(out, "report.html")}: ${html.length} characters, ${scenarios.length} scenarios, ${analytic.length} analytic cases`);
