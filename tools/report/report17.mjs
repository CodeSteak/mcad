#!/usr/bin/env node
// Test report 17 ("Stencil font cad stencil mono", plan 17.4/17.11, step 3b): the own
// monospaced stencil font as default font of text(), its parameters, the comparison with Allerta
// Stencil, the example stencil, kernel times, Wasm size, the Playwright fix of C16.
// Writes docs/reports/17-schablonenschrift/report.html (images in img/).
//
// Inputs (gitignored, reports/tmp/report17/):
//   chart/*.png, cli/*.png, glyphs-*.tsv, var-*.tsv, times.tsv, gcode.json, *.check.txt,
//   schild-*.measure.txt, schablone-measure.txt, compat-text.txt   tools/report/report17-data.sh
//   scenarios.json, img/*.jpg, browser-metrics.json  cd web && pnpm exec playwright test e2e/stencil.spec.ts
//   playwright.txt                                   whole suite (cd web && pnpm e2e > …)
//   c16.txt                                          C16 repeated before/after the fix (one line each)
//   vitest.txt, typecheck.txt                        cd web && pnpm test, pnpm typecheck
//   cargo.txt, clippy.txt, fmt.txt                   cargo test --workspace --no-fail-fast, clippy -D warnings, fmt --check
//   compare.txt                                      python3 tests/openscad-compat/compare.py --eval
//   wasm.json                                        {"base", "before": {raw, gzip}, "after": {raw, gzip}}
//
//   node tools/report/report17.mjs
//   node tools/report/print-pdf.mjs docs/reports/17-schablonenschrift/report.html docs/reports/17-schablonenschrift/report.pdf "Test report 17: Stencil font"

import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import os from "node:os";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "../..");
const input = join(root, "reports/tmp/report17");
const out = join(root, "docs/reports/17-schablonenschrift");
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
const T = (name) => readText(join(input, name))?.replaceAll(`${input}/`, "").replaceAll(`${root}/`, "") ?? "(missing)";
const pre = (t) => `<pre>${esc(String(t).trimEnd())}</pre>`;
const src = (p) => readText(join(root, p)) ?? "(missing)";

function img(srcPath, dst, width = 1440) {
  if (!existsSync(srcPath)) return null;
  if (srcPath.endsWith(".png") && dst.endsWith(".jpg")) {
    try {
      execFileSync("convert", [srcPath, "-resize", `${width}x>`, "-quality", "88", join(imgDir, dst)]);
      return `img/${dst}`;
    } catch {
      dst = dst.replace(/\.jpg$/, ".png");
    }
  }
  copyFileSync(srcPath, join(imgDir, dst));
  return `img/${dst}`;
}
const cli = (n) => img(join(input, "cli", `${n}.png`), `cli-${n}.jpg`);
const chart = (n, w = 1700) => img(join(input, "chart", `${n}.png`), `chart-${n}.jpg`, w);
const pw = (n) => img(join(input, "img", `${n}.jpg`), `ui-${n}.jpg`);
const fig = (s, cap) => (s ? `<figure><img src="${s}" alt=""><figcaption>${md(cap)}</figcaption></figure>` : `<p class="small">(image missing: ${esc(cap)})</p>`);
const pair = (a, ca, b, cb) => `<div class="two">${fig(a, ca)}${fig(b, cb)}</div>`;

// ------------------------------------------------------------------------------- data

const glyphs = (name) =>
  (readText(join(input, `glyphs-${name}.tsv`)) ?? "")
    .trim()
    .split("\n")
    .filter(Boolean)
    .map((l) => {
      const [ch, code, p, h, a] = l.split("\t");
      return { ch, code, pieces: +p.split(" ")[1], holes: +h.split(" ")[1], area: +a.split(" ")[1] };
    });
const reg = glyphs("regular");
const bold = glyphs("bold");
const holesReg = reg.reduce((s, g) => s + g.holes, 0);
const holesBold = bold.reduce((s, g) => s + g.holes, 0);
const piecesHist = (() => {
  const h = {};
  for (const g of reg) h[g.pieces] = (h[g.pieces] ?? 0) + 1;
  return Object.entries(h).sort((a, b) => +a[0] - +b[0]);
})();

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
const NEW = /stencil|text_profiles_and_fonts|text_stencil_parameters|required_chars_in_every_font|font_names|every_glyph_profile|extruded_text_volume|sign_text_kernel|spec_models/;
const newTests = cargo ? [...cargo.matchAll(/^test (\S+) \.\.\. (ok|FAILED)$/gm)].map((m) => ({ name: m[1], res: m[2] })).filter((t) => NEW.test(t.name)) : [];
const clippy = readText(join(input, "clippy.txt"));
const fmt = readText(join(input, "fmt.txt"));
const clippyOk = clippy != null && !/^(error|warning)/m.test(clippy);
const fmtOk = fmt != null && fmt.trim() === "";
const pwText = readText(join(input, "playwright.txt"));
const pwSum = pwText ? { passed: +(/(\d+) passed/.exec(pwText)?.[1] ?? 0), failed: +(/(\d+) failed/.exec(pwText)?.[1] ?? 0), flaky: +(/(\d+) flaky/.exec(pwText)?.[1] ?? 0) } : null;
const pwFailed = pwText ? [...pwText.matchAll(/^\s+\d+\) \[chromium\] › (.+)$/gm)].map((m) => m[1].trim()) : [];
const vitest = readText(join(input, "vitest.txt"));
const vitestSum = vitest ? (/Tests\s+(\d+) passed/.exec(vitest)?.[1] ?? null) : null;
const typecheck = readText(join(input, "typecheck.txt"));
const compare = readText(join(input, "compare.txt"));
const compareLine = compare ? compare.trim().split("\n").reverse().find((l) => /\d+\/\d+/.test(l)) ?? null : null;
const scenarios = (readJson(join(input, "scenarios.json")) ?? []).sort((a, b) => a.id.localeCompare(b.id));
const browser = readJson(join(input, "browser-metrics.json")) ?? {};
const wasm = readJson(join(input, "wasm.json"));
const gcode = readJson(join(input, "gcode.json"));
const c16 = readText(join(input, "c16.txt"));
const compatText = (readText(join(input, "compat-text.txt")) ?? "").trim();
// `cad measure --volume` prints "Volume …: N mm³" (with --lang de "Volumen …"; the regex matches both)
const vol = (name) => {
  const t = readText(join(input, name)) ?? "";
  return /volume[^\d]*([\d.]+)/i.exec(t)?.[1] ?? null;
};

const times = (() => {
  const by = {};
  for (const l of (readText(join(input, "times.tsv")) ?? "").trim().split("\n").filter(Boolean)) {
    const [name, evalMs, kernelMs, calls, nodes] = l.split("\t");
    (by[name] ??= []).push({ evalMs: +evalMs, kernelMs: +kernelMs, calls: +calls, nodes: +nodes });
  }
  const med = (a) => a.slice().sort((x, y) => x - y)[Math.floor(a.length / 2)];
  return Object.fromEntries(Object.entries(by).map(([k, v]) => [k, { evalMs: med(v.map((x) => x.evalMs)), kernelMs: med(v.map((x) => x.kernelMs)), calls: v[0].calls, nodes: v[0].nodes, runs: v.length }]));
})();
const t = (k) => times[k];
const timeRow = (k, label) => {
  const cells = ["stencil", "allerta", "jetbrains"].map((f) => t(`${k}-${f}`));
  return `<tr><td>${md(label)}</td>${cells.map((c) => `<td class="n">${num(c?.kernelMs)} <span class="small">(${num(c?.calls)} calls)</span></td>`).join("")}<td class="n">${cells[0] && cells[1] ? num(cells[0].kernelMs / cells[1].kernelMs, 2) : "–"}</td></tr>`;
};
const bms = (k) => browser[k]?.kernel_ms;

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

const html = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Test report 17 – Stencil font "cad stencil mono"</title>
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
  figure img { max-width: 100%; max-height: 120mm; border: 1px solid var(--border); border-radius: 1mm; background: #fff; }
  figcaption { font-size: 7.8pt; color: var(--muted); }
  .two { display: grid; grid-template-columns: 1fr 1fr; gap: 3mm; align-items: start; }
  .codefig { display: grid; grid-template-columns: 1fr 1fr; gap: 3mm; align-items: start; }
  .note { border-left: 1mm solid #e8b84a; padding: 1mm 2.5mm; background: #fff8e6; margin: 2mm 0; }
</style>
</head>
<body>
<main>
<h1>Test report 17 – Stencil font "cad stencil mono"</h1>
<div class="meta">Plan 17.7, step 3b: custom monospaced stencil font as the default font of <code>text()</code> · cad-font (<code>stencil.rs</code>), cad-eval (<code>text()</code> parameters), tests in cad-font, cad-eval, cad-engine, compat-tests, Playwright; fix for Playwright C16/C20 · ${esc(date)} · branch ${esc(branch)} · commit ${esc(commit)} · ${esc(rustc)} · ${os.cpus().length} cores</div>

<div class="summary">
  <div>Glyphs without holes<b class="${holesReg === 0 && holesBold === 0 && reg.length ? "ok" : "bad"}">${reg.length} / ${reg.length}</b>Regular and Bold; 8 parameter combinations in the test</div>
  <div>cargo test --workspace<b class="${cargoSum && cargoSum.failed === 0 ? "ok" : "bad"}">${cargoSum ? `${cargoSum.ok} / ${cargoSum.ok + cargoSum.failed}` : "–"}</b>${newTests.length} of them new/changed</div>
  <div>clippy · fmt<b class="${clippyOk && fmtOk ? "ok" : "bad"}">${clippy == null ? "–" : clippyOk ? "clean" : "finding"} · ${fmt == null ? "–" : fmtOk ? "clean" : "diff"}</b>-D warnings</div>
  <div>Vitest · Playwright<b class="${pwSum && pwSum.failed === 0 ? "ok" : "bad"}">${vitestSum ?? "–"} · ${pwSum ? `${pwSum.passed} / ${pwSum.passed + pwSum.failed}` : "–"}</b>new: K01–K03; C16 fixed</div>
  <div>Sign, kernel native<b>${num(t("schild-stencil")?.kernelMs)} ms</b>Allerta Stencil ${num(t("schild-allerta")?.kernelMs)} ms</div>
  <div>Wasm cad_wasm<b>${wasm ? `${wasm.after.raw >= wasm.before.raw ? "+" : ""}${num((wasm.after.raw - wasm.before.raw) / 1024, 1)} KiB` : "–"}</b>${wasm ? `gzip ${wasm.after.gzip >= wasm.before.gzip ? "+" : ""}${num((wasm.after.gzip - wasm.before.gzip) / 1024, 1)} KiB` : ""}</div>
</div>

<h2>1 Task and result</h2>
<p>Plan 17.4 calls for a custom, very simple monospaced stencil font made of lines and arcs as the default font, with bridges so that no islands fall out. Implemented:</p>
<ul>
<li><b>"cad stencil mono"</b> (Regular, Bold) in <code>crates/cad-font/src/stencil.rs</code>: ${reg.length} glyphs – the whole required character set <code>cad_font::REQUIRED_CHARS</code> without fallback, plus <code>’ ” $</code>. Each glyph is a sequence of center lines (lines, tangential circular arcs), drawn with a pen of the stroke width (flat ends, miter), plus dots, minus bridges, trimmed at the baseline and cap line. The contour is computed exactly by <code>cad_kernel::region2</code> (pieces of the lines and arcs); STEP and the kernel see lines and circles.</li>
<li><b>No holes:</b> every glyph, rendered to a profile, has 0 holes – the counters of O, A, B, D, P, R, 0, 4, 6, 8, 9, Ä, ö, @, %, &amp;, §, °, Ø … are connected to the outside via bridges. Checked in cad-font for 8 combinations of stroke width and bridge width (regions without clockwise loops, closed loops, no slivers, no overlaps) and in the engine for every glyph via <code>cad_engine::region_2d</code>.</li>
<li><b>Parameters</b> <code>text(…, stroke_width = …, bridge_width = …)</code> in mm: stroke width default 0.14·<code>size</code> (Bold 0.2·<code>size</code>), at most 0.2·<code>size</code>; bridge width default 0.1·<code>size</code>, 0 (no bridges – closed counters like a normal font) or 0.03…0.2·<code>size</code>. Values outside: warning at the argument, clamped. With other fonts: warning "only takes effect with …".</li>
<li><b>Metrics</b> as for the other fonts: em = <code>size</code>/0.72, cap height = <code>size</code> (720/1000 em), advance 600/1000 em × 1000/1024 – exactly as JetBrains Mono, so texts stay as wide as with the previous default font.</li>
<li><b>Default font:</b> <code>text()</code> without <code>font</code> and with an unknown family uses the stencil font (previously JetBrains Mono); missing characters (e.g. Cyrillic) still come from JetBrains Mono of the same weight.</li>
<li>Along the way: Playwright <b>C16</b> (axis-constrained dragging, phase B), which recently failed even on its own, is fixed – the cause was in the test (handles of the previous cursor line, section 10).</li>
</ul>

<h2>2 Decisions</h2>
<table>
<tr><th>Question</th><th>Decision</th></tr>
<tr><td>Form of the font</td><td>Rust data instead of a font file: per character a small drawing function over named positions (center lines left/right/middle, baseline, x-height, cap height, ascender, descender, depending on the stroke width), building blocks stadium, bowl, C arc, dots, commas. No TTF build, no license question; Wasm growth is code only (section 8).</td></tr>
<tr><td>Stroke</td><td><code>region2::stroke</code> (from step 2) with flat ends and miter: right angles stay square, ends are straight cuts (stencil look). Free ends run to the outer line (baseline, cap height), joints end on the center line of the other stroke – so no notches occur. Diagonals (A, K, M, N, V, W, X, Y, Z, 1, 2, 4, 7, <code>/</code>) are extended beyond the line and trimmed at the baseline/cap line (straight feet, flat apexes).</td></tr>
<tr><td>Bridges</td><td>A bridge is a band of the bridge width across a stroke (length 1.3 half strokes to each side), subtracted from the glyph. Rules: stem separated from the bowl for B, D, P, R, b, d, g, p, q (bridges at the stem), round shapes at top and bottom (O, Q, 0, 8, o), open A (legs at the cap line apart by the bridge width), e and a at the crossbar or at the bowl next to the stem, 4 at the crossbar before the stem, 6/9 diagonally at the bowl, rings (°, %, Ø, @, &amp;, §) on one side. ß/ẞ are open (gap ≥ bridge width to the stem).</td></tr>
<tr><td>Parameters</td><td><code>stroke_width</code> and <code>bridge_width</code> in mm (for printing and milling, absolute dimensions are what matters: "bridge at least two nozzle widths"); if not given, proportional to <code>size</code>. Style Bold = default stroke width 0.2·<code>size</code>. The limits come from the test of the parameter space: above 0.2·<code>size</code> strokes of a, e, #, @ grow together and close counters; bridges below 0.03·<code>size</code> are closed again by heavy strokes.</td></tr>
<tr><td>Metrics</td><td>1000 units per em, cap height 720, x-height 530, ascender 760, descender −210, advance 600, side bearing 60 (glyph box 480 wide). Vertical text: advance 1000, glyph centered on the pen (as the shaper does for TrueType).</td></tr>
<tr><td>Default font and OpenSCAD</td><td>OpenSCAD does not know the font (without <code>font</code> it takes the fontconfig default font, usually Liberation Sans): a model without <code>font</code> looks different in OpenSCAD. The compatibility tests do not depend on it (section 9); anyone who needs the same dimensions as OpenSCAD specifies a bundled TrueType font. Unknown families → default font with a warning (previously JetBrains Mono).</td></tr>
</table>

<h2>3 Character chart</h2>
<p>All ${reg.length} glyphs directly from the font (example program <code>cargo run -p cad-font --example stencil_chart</code>): cell = advance, lines baseline, x-height, cap height, descender. A glyph with a hole would be red.</p>
${fig(chart("regular"), "cad stencil mono Regular (stroke 0.14·size, bridge 0.1·size)")}
${fig(chart("bold"), "cad stencil mono Bold (stroke 0.2·size, bridge 0.1·size)")}
<p>Parts per glyph (Regular): ${piecesHist.map(([p, n]) => `${n}× ${p} part${p === "1" ? "" : "s"}`).join(", ")}; holes: Regular ${holesReg}, Bold ${holesBold}.</p>

<h2>4 Parameters</h2>
<p>The same eight characters with different stroke and bridge widths (fractions of <code>size</code>). <code>bridge_width = 0</code> turns the bridges off: then O, B, 0, 8 … have counters again (red = glyph with a hole, intended here).</p>
${fig(chart("var-0.06-0.04", 1360), "Stroke 0.06 · bridge 0.04")}
${fig(chart("var-0.14-0.1", 1360), "Default: stroke 0.14 · bridge 0.1")}
${fig(chart("var-0.2-0.2", 1360), "Stroke 0.2 · bridge 0.2 (limits)")}
${fig(chart("var-0.14-0", 1360), "Stroke 0.14 · without bridges (bridge_width = 0)")}
<p>Warnings for values outside the limits and for other fonts:</p>
${pre(T("warn.check.txt"))}

<h2>5 Comparison with Allerta Stencil</h2>
<p>Alphabet charts via the CLI (<code>text(…, size = 8)</code>, extruded 0.6 mm, from above): the stencil font has all characters itself; Allerta Stencil (proportional) lacks 20 of them (taken from JetBrains Mono, not stenciled, report 16).</p>
${fig(cli("alphabet-stencil"), "cad stencil mono Regular")}
${fig(cli("alphabet-stencil-bold"), "cad stencil mono Bold")}
${fig(cli("alphabet-allerta"), "Allerta Stencil – ẞ „ ‚ { } _ # ° ² ³ § ~ ^ · • … ± µ Ø ø from JetBrains Mono")}
<h3>Same sign in both fonts</h3>
${pair(cli("schild-stencil"), `cad stencil mono (default) – volume ${vol("schild-stencil.measure.txt") ?? "–"} mm³`, cli("schild-allerta"), `Allerta Stencil – volume ${vol("schild-allerta.measure.txt") ?? "–"} mm³`)}
${pre(src("reports/tmp/report17/cases/schild-stencil.scad"))}
<table>
<tr><th>Property</th><th>cad stencil mono</th><th>Allerta Stencil</th></tr>
<tr><td>Width</td><td>monospaced, 0.814·size per character (as JetBrains Mono)</td><td>proportional, with kerning</td></tr>
<tr><td>Curves</td><td>lines and circular arcs (arcs &gt; 90° split in the kernel)</td><td>quadratic Bézier curves → one B-spline per flank</td></tr>
<tr><td>Required character set</td><td>complete, all stenciled</td><td>20 characters missing (fallback, without bridges)</td></tr>
<tr><td>Stroke, bridge</td><td>adjustable (<code>stroke_width</code>, <code>bridge_width</code>), Bold</td><td>fixed</td></tr>
<tr><td>License</td><td>source code of the project</td><td>SIL OFL 1.1, Reserved Font Name (unmodified only)</td></tr>
</table>

<h2>6 Example: stencil</h2>
<p><code>tests/models/spec/stencil.mcad</code>: text cut out of a 1 mm thin plate. Without bridges the inner parts of A, 0, 8, B, 9 would fall out; with the stencil font they stay attached to the plate.</p>
<div class="codefig">${pre(src("tests/models/spec/stencil.mcad"))}<div>${fig(cli("schablone"), "rendered (iso)")}${fig(cli("schablone-oben"), "top view")}</div></div>
<h3>Printed: G-code from PrusaSlicer</h3>
<p>A real print is not possible in this environment; instead: STL exported (<code>cad export</code>), sliced to G-code with PrusaSlicer 2.7 (default profile, layer 0.2 mm, nozzle 0.4 mm), and the top layer drawn from the G-code (<code>tools/report/report17-gcode.py</code>): every island of the plate is connected to the rest, the bridges (1.2 mm = 3 nozzle widths) are printed as separate paths.</p>
${fig(cli("schablone-gcode"), `top layer (z = ${gcode?.top_z ?? "–"} mm) from the G-code, ${gcode?.layers ?? "–"} layers, estimated print time ${gcode?.["estimated printing time (normal mode)"] ?? "–"}, filament ${gcode?.["filament used [mm]"] ?? "–"} mm`)}

<h2>7 How a glyph is built</h2>
<p>Example B: stem <code>vline(l, 0, cap)</code>; upper and lower bowl as a path from the stem (<code>bowl</code>: crossbar, quarter arcs, crossbar back); three bridges directly to the right of the stem (crossbars top, middle, bottom). <code>region2::stroke</code> turns the paths into bands, miter pieces and ends, <code>region2::combine</code> unites them and subtracts the bridges. Result: stem and "3"-shaped bowl as two closed contours of lines and arcs, without a hole. The positions depend on the stroke width (center line = outer line ∓ half stroke), so cap height and glyph box stay the same for every stroke width.</p>
${pre(`'B' => {
    g.vline(l, y0, cap);
    bowl(g, l, r - 0.06 * wd, t, m, (t - m) / 2.0);
    bowl(g, l, r, m, b, (m - b) / 2.0);
    for y in [t, m, b] {
        g.cut(l + h + bw / 2.0, y, 90.0);
    }
}`)}

<h2>8 Measurements</h2>
<h3>Kernel time</h3>
<p>Release build, <code>cad eval --json</code> without daemon, median of 3 runs; kernel time in ms (all kernel calls). The stencil font needs fewer kernel calls and less time than Allerta Stencil and JetBrains Mono: lines and circles instead of B-splines; its own computation time (regions, cached) is part of the evaluation (language ${num(t("schild-stencil")?.evalMs, 1)} ms for the sign).</p>
<table>
<tr><th>Model</th><th>cad stencil mono</th><th>Allerta Stencil</th><th>JetBrains Mono</th><th>Stencil / Allerta</th></tr>
${timeRow("hello", "\"Hello World\", extruded")}
${timeRow("vierzig", "40 characters, extruded")}
${timeRow("schild", "Sign: plate ∪ \"SAFETY GLASSES REQUIRED\" (size 4.2)")}
</table>
<p>Browser (Chromium, SMLib as Wasm, Playwright K03): sign ${num(bms("schild-stencil"))} ms versus ${num(bms("schild-allerta"))} ms with Allerta Stencil; "Hello World" ${num(bms("hello-stencil"))} versus ${num(bms("hello-allerta"))} ms. Stencil example native: ${num(t("schablone")?.kernelMs)} ms.</p>
<h3>Wasm module</h3>
${wasm ? `<table><tr><th></th><th>raw</th><th>gzip -9</th></tr><tr><td><code>cad_wasm_bg.wasm</code> before (commit ${esc(wasm.base)})</td><td class="n">${num(wasm.before.raw)} B</td><td class="n">${num(wasm.before.gzip)} B</td></tr><tr><td>with stencil font</td><td class="n">${num(wasm.after.raw)} B</td><td class="n">${num(wasm.after.gzip)} B</td></tr><tr><td>growth</td><td class="n">${num(wasm.after.raw - wasm.before.raw)} B</td><td class="n">${num(wasm.after.gzip - wasm.before.gzip)} B</td></tr></table><p class="small">Same build (release, wasm-opt -O3) for both states. <code>region2</code> was already in the module (offset, stroke); only the drawing functions are added.</p>` : "<p>(missing)</p>"}

<h2>9 OpenSCAD compatibility</h2>
<p>Do the compatibility tests depend on the default font? No: the 36 comparison models (<code>tests/models/openscad-compat</code>, <code>tests/openscad-compat/projects</code>) contain no <code>text()</code> (${compatText ? `found: ${esc(compatText)}` : "grep finds no file"}), so <code>compare.py</code> and the stage 1 tests (<code>crates/compat-tests</code>) are unaffected; the text comparison with OpenSCAD from report 16 (<code>hull_and_area_like_openscad</code>, <code>report16-openscad.py</code>) names every font explicitly. ${compareLine ? `Run <code>compare.py --eval</code>: ${esc(compareLine)}.` : ""} Intentional deviation: <code>text("…")</code> without <code>font</code> gives the stencil font here and the fontconfig default in OpenSCAD – for identical dimensions, specify a font (plan 17.11, skill).</p>

<h2>10 UI</h2>
<p>There is no font selection in the UI (fonts are specified in the code; custom ones via "⇡ Load file"), so no change there. New scenarios in <code>web/e2e/stencil.spec.ts</code>:</p>
${scenarios.map((s) => `<h3>${esc(s.id)} ${esc(s.title)}</h3><p>${md(s.caption)}</p>${(s.images ?? []).map((i) => fig(pw(i.name), i.label)).join("")}${s.code && s.code.split("\n").length <= 12 ? pre(s.code) : ""}<ul>${(s.checks ?? []).map((c) => `<li>${md(c)}</li>`).join("")}</ul>`).join("\n")}
<h3>C16 axis-constrained dragging (report 4) – cause and fix</h3>
<p>Since report 15, C16 ran into the test timeout depending on timing, recently also on its own. Cause: after <code>clickLine(2, 3)</code> the viewer rebuilds the handles only in the next animation frame; until then <code>__cad.handles()</code> returns the handles of the previous cursor line. That line holds <code>cube([60, 30, 1])</code>, whose vector is also a point handle – <code>waitHandles("… kind === 'point'")</code> was satisfied immediately, the test took the first handle (the plate corner at x = 1118 px instead of the translate point at 770 px), the drag did not start (<code>drag() = null</code>), the mouse rotated the camera, and waiting for "axis x" ran until the timeout. The axis selection itself was correct in all runs. Fix in the test: <code>pointHandleOn(page, zeile)</code> waits for the point handle <b>of the line</b> and takes exactly that one (likewise C20, same gap). No test skipped.</p>
${c16 ? pre(c16) : ""}

<h2>11 Test runs</h2>
<table>
<tr><th>Run</th><th>Result</th></tr>
<tr><td><code>cargo test --workspace --no-fail-fast</code></td><td>${cargoSum ? `${cargoSum.ok} passed, ${cargoSum.failed} failed` : "–"}</td></tr>
<tr><td><code>cargo clippy --workspace --all-targets -- -D warnings</code> · <code>cargo fmt --check</code></td><td>${clippyOk ? "clean" : "finding"} · ${fmtOk ? "clean" : "diff"}</td></tr>
<tr><td><code>pnpm typecheck</code> · <code>pnpm test</code> (Vitest)</td><td>${typecheck !== null ? (/error TS/i.test(typecheck) ? "errors" : "clean") : "–"} · ${vitestSum ?? "–"} passed</td></tr>
<tr><td><code>pnpm e2e</code> (Playwright, Chromium)</td><td>${pwSum ? `${pwSum.passed} passed, ${pwSum.failed} failed${pwFailed.length ? `: ${pwFailed.map(esc).join("; ")}` : ""}` : "–"}</td></tr>
<tr><td><code>tests/openscad-compat/compare.py --eval</code></td><td>${esc(compareLine ?? "–")}</td></tr>
</table>
<p>New or changed Rust tests (${newTests.length}):</p>
<p class="small">${newTests.map((x) => `<code>${esc(x.name)}</code> ${x.res === "ok" ? '<span class="ok">ok</span>' : '<span class="bad">FAILED</span>'}`).join(" · ")}</p>
<p class="small">cad-font: coverage (<code>required_chars_in_every_font</code> with both styles), no holes/slivers/overlaps for 8 parameter combinations, closed counters without bridges, metrics (cap height = size, advance = JetBrains Mono, every glyph in its cell), areas (H exact, o against formula, alphabet against JetBrains Mono, stroke/bridge/Bold/size), alignment, directions, fallback, font names. cad-eval: default font, parameters, warnings. cad-engine: every glyph as a profile without a hole (Regular, Bold), volume = area × height, kernel time of the sign against Allerta Stencil. compat-tests: example <code>stencil.mcad</code>.</p>

<h2>12 Limitations</h2>
<ul>
<li>Design deliberately simple: one stroke width per glyph (only @ with 0.72 times the stroke, ² ³ with 0.75 times), no optical corrections, no kerning pairs (monospaced). W, w, m, @, * become very dense in Bold.</li>
<li>Parameter limits from the test (stroke ≤ 0.2·size, bridge 0 or 0.03…0.2·size); values in between checked at 8 combinations, not exhaustively. On arcs, a bridge is cut radially straight; its width applies on the center line.</li>
<li>The font does not compose combining characters (e.g. "a" + U+0308): the character comes from JetBrains Mono. Precomposed umlauts are included.</li>
<li>Characters outside the scope (Cyrillic, Greek, arrows …) come from JetBrains Mono – without bridges, so they can have holes.</li>
<li>Glyphs of different characters touch only with <code>spacing</code> &lt; 1 or with the continuous strokes <code>—</code>, <code>_</code> (across the whole cell as in monospace fonts); then counters can form between glyphs.</li>
<li>Models without <code>font</code> look different in OpenSCAD (fontconfig default there).</li>
</ul>
</main>
</body>
</html>
`;
writeFileSync(join(out, "report.html"), html);
console.log(`${join(out, "report.html")}: ${html.length} characters, ${scenarios.length} scenarios, ${reg.length} glyphs`);
