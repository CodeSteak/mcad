#!/usr/bin/env node
// Test report 16 ("Text and SVG import", plan 17.4/17.5, step 3 without the stencil font):
// text() with the bundled OFL fonts, import() of SVG files, comparison with OpenSCAD 2021.01,
// kernel times, Wasm size. Writes docs/reports/16-text-svg/report.html (images in img/).
//
// Inputs (gitignored, reports/tmp/report16/):
//   cli/*.png, cases/*, times.tsv, openscad.tsv, *.check.txt, schild-colors.txt, svg_logo.*
//                                                   tools/report/report16-data.sh
//   scenarios.json, img/*.jpg, browser-metrics.json cd web && pnpm exec playwright test e2e/text-svg.spec.ts
//   playwright.json                                 whole suite (pnpm e2e; copy of reports/tmp/report2/playwright.json)
//   vitest.txt, typecheck.txt                       cd web && pnpm test, pnpm typecheck
//   cargo.txt, clippy.txt, fmt.txt                  cargo test --workspace --no-fail-fast, clippy -D warnings, fmt --check
//   compare.txt, step.txt, alltag.txt, node-test.txt compare.py --eval, verify.py, tests/everyday/check.sh, test-node.mjs
//   wasm.json                                       {"before": {raw, gzip}, "after": {raw, gzip}, "fonts": bytes}
//
//   node tools/report/report16.mjs
//   node tools/report/print-pdf.mjs docs/reports/16-text-svg/report.html docs/reports/16-text-svg/report.pdf "Test report 16: Text and SVG import"

import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import os from "node:os";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "../..");
const input = join(root, "reports/tmp/report16");
const out = join(root, "docs/reports/16-text-svg");
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
const svgSrc = (n) => img(join(root, "tests/models/svg", `${n}.svg`), `src-${n}.svg`);
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
const NEW = /cad_svg|cad_font|cad_outline|text_svg|path::tests|tests::(units_|shapes_exact|transforms|fill_rules|hidden_and|open_paths|unsupported_warns|select_by_id|errors|nested_svg|hull_and_area|glyph_o|kerning|align_and|directions|font_names|user_font|cff_outlines|required_chars|fallback_to|circle_area|normalize_rules)|import_(svg|relative|volume|open)|text_(profiles|with|volume)|data_urls|changed_svg|examples_check|watch_follows|unsupported_operations|spec_models/;
const newTests = cargo ? [...cargo.matchAll(/^test (\S+) \.\.\. (ok|FAILED)$/gm)].map((m) => ({ name: m[1], res: m[2] })).filter((t) => NEW.test(t.name)) : [];
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
const typecheck = readText(join(input, "typecheck.txt"));
const compare = readText(join(input, "compare.txt"));
const compareSum = compare ? /(\d+)\/(\d+)/.exec(compare.split("\n").reverse().find((l) => /\d+\/\d+ files/.test(l)) ?? "")?.slice(1, 3) : null;
const step = readText(join(input, "step.txt"));
const stepSum = step ? (/\d+\/\d+ passed/.exec(step)?.[0] ?? null) : null;
const alltag = readText(join(input, "alltag.txt"));
const alltagLine = alltag ? alltag.trim().split("\n").at(-1) : null;
const nodeTest = readText(join(input, "node-test.txt"));
const scenarios = (readJson(join(input, "scenarios.json")) ?? []).sort((a, b) => a.id.localeCompare(b.id));
const browser = readJson(join(input, "browser-metrics.json")) ?? {};
const wasm = readJson(join(input, "wasm.json"));
const flaky = readText(join(input, "flaky.txt"));

const times = (() => {
  const by = {};
  for (const l of (readText(join(input, "times.tsv")) ?? "").trim().split("\n").filter(Boolean)) {
    const [name, evalMs, kernelMs, calls, nodes] = l.split("\t");
    (by[name] ??= []).push({ evalMs: +evalMs, kernelMs: +kernelMs, calls: +calls, nodes: +nodes });
  }
  const med = (a) => a.slice().sort((x, y) => x - y)[Math.floor(a.length / 2)];
  return Object.fromEntries(Object.entries(by).map(([k, v]) => [k, { evalMs: med(v.map((x) => x.evalMs)), kernelMs: med(v.map((x) => x.kernelMs)), calls: v[0].calls, nodes: v[0].nodes, runs: v.length }]));
})();
const osc = (readText(join(input, "openscad.tsv")) ?? "")
  .trim()
  .split("\n")
  .filter(Boolean)
  .map((l) => {
    const [kind, name, code, ...n] = l.split("\t");
    const v = n.map(Number);
    return { kind, name, code, o: v.slice(0, 5), c: v.slice(5, 10) };
  });
const maxBoundDiff = (r) => Math.max(...[0, 1, 2, 3].map((i) => Math.abs(r.o[i] - r.c[i])));
const relArea = (r) => Math.abs(r.o[4] - r.c[4]) / Math.abs(r.o[4]);
// case names are data keys of report16-openscad.py; "ohne-viewbox" gets an English gloss
const oscName = (n) => (n === "ohne-viewbox" ? "ohne-viewbox (without viewBox)" : n);
const oscRow = (r) =>
  `<tr><td>${md(oscName(r.name))}</td><td class="n">${num(r.o[0], 3)} … ${num(r.o[2], 3)}<br>${num(r.o[1], 3)} … ${num(r.o[3], 3)}</td><td class="n">${num(r.c[0], 3)} … ${num(r.c[2], 3)}<br>${num(r.c[1], 3)} … ${num(r.c[3], 3)}</td><td class="n">${num(maxBoundDiff(r), 4)}</td><td class="n">${num(r.o[4], 3)}</td><td class="n">${num(r.c[4], 3)}</td><td class="n ${relArea(r) < 0.005 ? "" : "warnc"}">${num(100 * relArea(r), 3)} %</td></tr>`;
const oscText = osc.filter((r) => r.kind === "text");
const oscSvg = osc.filter((r) => r.kind !== "text");
const textMaxBound = oscText.length ? Math.max(...oscText.map(maxBoundDiff)) : null;
const textMaxArea = oscText.length ? Math.max(...oscText.map(relArea)) : null;

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

const FONTS = [
  ["jetbrains", "JetBrains Mono (default)", "JetBrains Mono 2.304, Regular"],
  ["jetbrains-bold", "JetBrains Mono:style=Bold", "JetBrains Mono 2.304, Bold"],
  ["inter", "Inter", "Inter 4.1, Regular"],
  ["inter-bold", "Inter:style=Bold", "Inter 4.1, Bold"],
  ["allerta", "Allerta Stencil", "Allerta Stencil (Google Fonts), Regular"],
];
const fontBytes = (() => {
  const d = join(root, "crates/cad-font/fonts");
  return existsSync(d) ? readdirSync(d).filter((f) => /\.ttf$/.test(f)).map((f) => [f, statSync(join(d, f)).size]) : [];
})();
const fallbackWarn = T("fallback.check.txt");

const t = (k) => times[k];
const timeRow = (k, label, browserKey) =>
  `<tr><td>${md(label)}</td><td class="n">${num(t(k)?.nodes)}</td><td class="n">${num(t(k)?.calls)}</td><td class="n">${num(t(k)?.evalMs, 1)}</td><td class="n">${num(t(k)?.kernelMs)}</td><td class="n">${browserKey && browser[browserKey] ? num(browser[browserKey].kernel_ms) : "–"}</td></tr>`;

const html = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Test report 16 – Text and SVG import</title>
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
  td.warnc { color: #9a6700; }
  code, pre { font-family: ui-monospace, "SFMono-Regular", Menlo, Consolas, monospace; font-size: 7.6pt; }
  pre { margin: 0 0 1.5mm; padding: 1.4mm 2mm; background: #f6f8fa; border: 1px solid var(--border); border-radius: 1mm; white-space: pre-wrap; word-break: break-word; }
  .small { font-size: 7.6pt; color: var(--muted); font-weight: 400; }
  ul { margin: 1mm 0 2mm 5mm; padding: 0; }
  li { margin: 0.5mm 0; }
  tr, td, figure, pre { break-inside: avoid; }
  .ok { color: var(--ok); font-weight: 600; } .bad { color: var(--bad); font-weight: 600; }
  figure { margin: 2mm 0; }
  figure img { max-width: 100%; max-height: 92mm; border: 1px solid var(--border); border-radius: 1mm; background: #fff; }
  figcaption { font-size: 7.8pt; color: var(--muted); }
  .two { display: grid; grid-template-columns: 1fr 1fr; gap: 3mm; align-items: start; }
  .two figure img.src { padding: 3mm; }
  .codefig { display: grid; grid-template-columns: 1fr 1fr; gap: 3mm; align-items: start; }
  .note { border-left: 1mm solid #e8b84a; padding: 1mm 2.5mm; background: #fff8e6; margin: 2mm 0; }
</style>
</head>
<body>
<main>
<h1>Test report 16 – Text and SVG import</h1>
<div class="meta">Plan 17.7, step 3 (without the custom stencil font): <code>text()</code> with bundled OFL fonts, <code>import()</code> of SVG · new crates cad-outline, cad-svg, cad-font; cad-eval, cad-ir, cad-engine (color regions), cad-step (ellipses), SMLib facade (disjoint union), cad-cli, cad-desktop, web · ${esc(date)} · branch ${esc(branch)} · commit ${esc(commit)} · ${esc(rustc)} · ${os.cpus().length} cores</div>

<div class="summary">
  <div>cargo test --workspace<b class="${cargoSum && cargoSum.failed === 0 ? "ok" : "bad"}">${cargoSum ? `${cargoSum.ok} / ${cargoSum.ok + cargoSum.failed}` : "–"}</b>${newTests.length} of them new/changed</div>
  <div>clippy · fmt<b class="${clippyOk && fmtOk ? "ok" : "bad"}">${clippy == null ? "–" : clippyOk ? "clean" : "finding"} · ${fmt == null ? "–" : fmtOk ? "clean" : "diff"}</b>-D warnings</div>
  <div>Vitest · Playwright<b class="${pwStats && pwStats.failed.length <= (flaky ? 1 : 0) ? "ok" : "bad"}">${vitestSum ?? "–"} · ${pwStats ? `${pwStats.passed} / ${pwStats.total}` : "–"}</b>new: T01–T05${flaky ? "; C16 timing-dependent" : ""}</div>
  <div>OpenSCAD text<b class="${textMaxArea !== null && textMaxArea < 0.005 ? "ok" : "bad"}">${textMaxBound !== null ? `≤ ${num(textMaxBound, 3)} mm` : "–"}</b>bounding box; area ≤ ${textMaxArea !== null ? num(100 * textMaxArea, 2) : "–"} %</div>
  <div>Wasm cad_wasm<b>${wasm ? `+${num((wasm.after.raw - wasm.before.raw) / 1e6, 2)} MB` : "–"}</b>${wasm ? `${num(wasm.after.raw / 1e6, 2)} MB, gzip ${num(wasm.after.gzip / 1e6, 2)} MB` : ""}</div>
  <div>"Hello World"<b>${num(t("hello")?.kernelMs)} ms</b>kernel native; browser ${browser.hello ? num(browser.hello.kernel_ms) : "–"} ms</div>
</div>

<h2>1 Task and result</h2>
<p>From step 3 of plan 17: <b>SVG import</b> (subset, 17.5) and <b>text</b> with bundled free fonts (17.4). The custom stencil font <code>"cad stencil mono"</code> is not part of this (it needs <code>stroke</code> from step 2); the name is reserved as the future default font; until then <b>JetBrains Mono</b> is the default font (hint for <code>font = "cad stencil mono"</code>).</p>
<ul>
<li><code>text(t, size, font, halign, valign, spacing, direction, language, script)</code>: exact outlines (TrueType quadratic, CFF cubic), kerning and ligatures via <code>rustybuzz</code>, holes by nonzero, <code>color()</code> as usual. Bounding box and area against OpenSCAD 2021.01 with the same font files: bounding box ≤ ${textMaxBound !== null ? num(textMaxBound, 3) : "–"} mm, area ≤ ${textMaxArea !== null ? num(100 * textMaxArea, 2) : "–"} % (OpenSCAD samples the curves).</li>
<li><code>import("….svg", id, scale, dpi = 96, center)</code>: paths (all commands), rectangles with rounding, circles and ellipses exact, polygons, groups and nested <code>svg</code> with <code>transform</code>, units, <code>fill-rule</code>, <code>display:none</code>; everything else with a warning and the SVG line.</li>
<li>Files relative to the including file (CLI: file system; browser: project files; desktop: reads and writes them); changed SVGs change the cache key, <code>cad watch</code> follows them, and so does the UI scene (T02).</li>
<li>Required character set with umlauts, ß/ẞ and punctuation in all bundled fonts (test); gaps (Allerta Stencil only) are filled from JetBrains Mono with a warning.</li>
<li>Along the way: color regions of models that are only unions of their color nodes, without a Boolean with the whole (multicolor text: seconds instead of abort by the watchdog).</li>
<li>Found with the alphabet chart: SMLib united disjoint glyph parts wrongly (Allerta "c": one piece twice, once reversed; volume 2.0 instead of 7.3 mm³, "a" in "abcd" disappeared, without a message). The facade now unites bodies with disjoint bounding boxes without an SMLib Boolean (<code>disjoint_union</code>: both shells into one B-rep); regression test <code>text_volume_is_area_times_height</code>.</li>
</ul>

<h2>2 Decisions</h2>
<table>
<tr><th>Question</th><th>Decision</th></tr>
<tr><td>Libraries</td><td><code>roxmltree</code> 0.21 (XML, line numbers for messages), <code>rustybuzz</code> 0.20.1 with <code>ttf-parser</code> 0.25 (shaping like HarfBuzz, outlines, pure Rust, Wasm-capable). Path data, transformations, units, arcs, fill rules: own code.</td></tr>
<tr><td>Split</td><td>New crates <code>cad-outline</code> (neutral contours), <code>cad-svg</code>, <code>cad-font</code>; in <code>cad-eval</code> only the builtins (<code>import.rs</code>) and loading of data files (<code>Project::assets</code>). IR: <code>Seg::Nurbs</code> (from step 2) for B-spline chains and elliptical arcs. Engine unchanged except for the color regions; facade: union of disjoint bodies without an SMLib Boolean (below).</td></tr>
<tr><td>Profile nodes</td><td>One <code>Profile::Sketch</code> per filled SVG element or per glyph, several under a group (union): overlapping elements or glyphs give the union, as in painting; within one element its fill rule decides.</td></tr>
<tr><td>Curves</td><td>Tangent-continuous chains of Bézier curves of a glyph or a path become <b>one</b> B-spline (C1 knots, parameter lengths from the tangent lengths; exact); corners separate them. Fewer kernel faces (glyph "o": 4 instead of ~30).</td></tr>
<tr><td>Size <code>size</code></td><td>as in OpenSCAD: em = <code>size</code>/0.72 (OpenSCAD sets the font at <code>size</code> points at 100 dpi); cap height ≈ <code>size</code>. Advance, offset and alignment times 1000/1024 as in OpenSCAD (its FreeType conversion): text stays as wide as in OpenSCAD.</td></tr>
<tr><td>Alignment</td><td><code>halign</code> by the advance, <code>valign</code> by the glyph bounding box (top/center/bottom), <code>baseline</code> 0; vertical text with the offsets from the shaper (centered as in OpenSCAD).</td></tr>
<tr><td>Default font</td><td>JetBrains Mono Regular until <code>"cad stencil mono"</code> exists (name reserved, hint). Unknown family: warning + JetBrains Mono; unknown style: warning + Regular of the family.</td></tr>
<tr><td>Missing characters</td><td>Per-character fallback to JetBrains Mono of the same weight (OS/2 weight ≥ 600 → Bold) with a warning at the statement: a readable character in its place is more useful than a box or a gap that is overlooked when printing; JetBrains Mono covers the required character set and much more. Allerta Stencil stays unmodified (Reserved Font Name "Allerta": modified versions may not use that name).</td></tr>
<tr><td>SVG units</td><td>as in OpenSCAD where it is consistent; default <code>dpi = 96</code> (OpenSCAD 2021.01: 72 for numbers without a unit). Deviations see 5.3.</td></tr>
<tr><td>Files</td><td>Every string in the project that ends in <code>.svg/.ttf/.otf/.ttc/.otc</code> is read on load (also parameter default values); binary files are stored in text projects (browser, <code>SetFiles</code>) as a <code>data:</code> URL. A name composed at run time that appears nowhere as a string: error "not loaded".</td></tr>
</table>

<h2>3 Fonts</h2>
<p>Bundled in <code>crates/cad-font/fonts/</code> (SIL OFL 1.1, license texts and sources with checksums in <code>fonts/README.md</code>), embedded with <code>include_bytes!</code>: ${fontBytes.map(([f, b]) => `<code>${esc(f)}</code> ${num(b / 1024)} KiB`).join(", ")}. Selection by family and style as in OpenSCAD (<code>"Inter:style=Bold"</code>), custom fonts by path (<code>font = "datei.ttf"</code>) or by upload in the UI (T03).</p>
<h3>Alphabet charts</h3>
<p>Per font <code>text(…, size = 8)</code>, extruded 0.6 mm, viewed from above (generator <code>tools/report/report16-data.sh</code>). Rows 3–5: digits, <b>umlauts and punctuation</b> of the required character set.</p>
${FONTS.map(([k, name, ver]) => fig(cli(`alphabet-${k}`), `${name} – ${ver}`)).join("\n")}
<h3>Required character set</h3>
<p><code>cad_font::REQUIRED_CHARS</code> (test <code>required_chars_in_every_font</code>: every bundled font has a real glyph for each character, no <code>.notdef</code>): A–Z a–z 0–9, Ä Ö Ü ä ö ü ß ẞ, space, <code>. , ; : ! ? ' " „ “ ‚ ‘ » « ( ) [ ] { } - – — _ / \\ &amp; % # + * = &lt; &gt; ° ² ³ € @ § ~ ^ | · • …</code>, <code>± × ÷ µ Ø ø</code>. Also applies to the future stencil font (plan 17.4).</p>
<table>
<tr><th>Font</th><th>missing characters</th><th>Consequence</th></tr>
<tr><td>JetBrains Mono Regular/Bold</td><td>none</td><td>–</td></tr>
<tr><td>Inter Regular/Bold</td><td>none</td><td>–</td></tr>
<tr><td>Allerta Stencil</td><td><code>ẞ „ ‚ { } _ # ° ² ³ § ~ ^ · • … ± µ Ø ø</code></td><td>from JetBrains Mono Regular, warning (see chart: these characters are not stenciled)</td></tr>
</table>
${pre(fallbackWarn)}

<h2>4 Text against OpenSCAD 2021.01</h2>
<p>Same font files (OpenSCAD via a fontconfig file that knows only <code>crates/cad-font/fonts</code>), <code>text(…)</code> exported as 2D (polygons) against <code>cad measure --bounds --volume</code> of a 1 mm extrusion (<code>tools/report/report16-openscad.py</code>). Bounding box in x (top) and y (bottom), mm.</p>
<table>
<tr><th>Case</th><th>OpenSCAD bbox</th><th>cad bbox</th><th>max. dev.</th><th>OpenSCAD mm²</th><th>cad mm²</th><th>Area</th></tr>
${oscText.map(oscRow).join("\n")}
</table>
<p class="small">OpenSCAD's area is smaller because it samples the curves as polygons (chords inside the curve); the bounding box agrees to ≤ 0.001 mm (OpenSCAD writes 6 digits; its polygon vertices lie on the curve). OpenSCAD measures <code>valign</code> on rasterized glyph metrics (FreeType), hence up to 0.004 mm there. "Hello World", "AV" (kerning), "fi" and "-&gt;" (ligatures in JetBrains Mono) show that shaping and advance are identical.</p>

<h2>5 SVG import</h2>
<h3>5.1 Examples: source and result</h3>
<p>Left the SVG file as the browser draws it (source in <code>tests/models/svg/</code>, own drawings), right <code>linear_extrude(2) import("….svg")</code> from above.</p>
${[
  ["logo", "Ring of half-circle arcs with a hole (evenodd), eight rotated rectangles with rounding, polygon"],
  ["blatt", "cubic Bézier (C, S), vein as reverse direction (nonzero), stem from Q/T, rotated group"],
  ["haus", "relative commands (h, v, m), windows and door as holes (evenodd), rectangle with rx, scaled group with a circle"],
  ["welle", "Q/T wave, ellipse, circle with scale(2 1) (elliptical arcs), skewX, rotated elliptical arc (A)"],
]
  .map(([n, cap]) => `<h3>${esc(n)}.svg</h3><p class="small">${md(cap)}</p><div class="two">${svgSrc(n) ? `<figure><img class="src" src="${svgSrc(n)}" alt=""><figcaption>Source (browser)</figcaption></figure>` : ""}${fig(cli(`svg-${n}`), "import, extruded, from above")}</div>`)
  .join("\n")}
<h3>5.2 Supported and unsupported</h3>
<table>
<tr><th>Element / property</th><th>Implementation</th></tr>
<tr><td><code>path</code></td><td>M L H V C S Q T A Z, absolute and relative, implicit repetition, compact numbers (<code>10-10.5.5</code>) and arc flags (<code>a10 10 0 0110 10</code>); on error the part before it is kept, warning</td></tr>
<tr><td>Arcs <code>A</code>, <code>circle</code>, <code>ellipse</code>, <code>rect rx/ry</code></td><td>exact circular arcs (under similarity transforms), otherwise exact elliptical arcs (rational B-spline); radii too small → scaled up as in SVG</td></tr>
<tr><td><code>polygon</code> · <code>polyline</code>, <code>line</code>, open paths</td><td>area · path only (warning "open path is not filled"); paths only → open sketch (as a <code>sweep</code> path)</td></tr>
<tr><td><code>g</code>, <code>a</code>, nested <code>svg</code>, <code>transform</code></td><td>matrix, translate, scale, rotate (also about a point), skewX, skewY; lists from right to left</td></tr>
<tr><td><code>viewBox</code>, <code>width/height</code>, <code>preserveAspectRatio</code></td><td>px, mm, cm, in, pt, pc, without unit (<code>dpi</code>); origin bottom left</td></tr>
<tr><td><code>fill</code>, <code>fill-rule</code>, <code>display</code>, <code>visibility</code></td><td>as attribute and in <code>style</code>, inherited; <code>fill="none"</code>: not imported (warning)</td></tr>
<tr><td>Text, images, <code>use</code>, CSS classes/<code>&lt;style&gt;</code>, masks, clip paths, filters, gradients/patterns as fill, stroke width</td><td>warning with the SVG line (once per message); gradients are imported as a plain area</td></tr>
<tr><td>Metadata (Inkscape, Sodipodi, <code>defs</code>, <code>title</code>)</td><td>silently skipped</td></tr>
<tr><td><code>id = "…"</code></td><td>only this element (with its content); unknown: error</td></tr>
</table>
${pre(src("reports/tmp/report16/cases/warn.svg"))}
${pre(T("warn.check.txt"))}
<h3>5.3 Units and comparison with OpenSCAD</h3>
<p>OpenSCAD 2021.01 (<code>import(…, dpi = 96)</code>, exported as 2D) against cad. With a <code>viewBox</code> both agree. Intentional deviations (SVG rule instead of OpenSCAD quirk): <b>without a <code>viewBox</code></b> OpenSCAD takes user units as mm but mirrors about the height in pixels (row "ohne-viewbox"); a y offset of the <code>viewBox</code> is added by OpenSCAD instead of subtracted; OpenSCAD always fills by evenodd and turns strokes into areas.</p>
<table>
<tr><th>File</th><th>OpenSCAD bbox</th><th>cad bbox</th><th>max. dev.</th><th>OpenSCAD mm²</th><th>cad mm²</th><th>Area</th></tr>
${oscSvg.map(oscRow).join("\n")}
</table>

<h2>6 Examples</h2>
<h3><code>tests/models/spec/text_sign.mcad</code></h3>
<div class="codefig">${pre(src("tests/models/spec/text_sign.mcad"))}<div>${fig(cli("text_schild"), "Sign: plate from a sketch, rim, raised text in Inter Bold and Allerta Stencil, four colors")}${fig(cli("text_schild-oben"), "top view")}${pre(T("schild-colors.txt"))}</div></div>
<h3><code>tests/models/spec/svg_logo.mcad</code></h3>
<div class="codefig">${pre(src("tests/models/spec/svg_logo.mcad"))}<div>${fig(cli("svg_logo"), "Logo, leaf (scaled), house as an engraving, from wave.svg only id = \"oval\" and id = \"bogen\"")}${fig(cli("svg_logo-oben"), "top view")}</div></div>
<p class="small">STEP of svg_logo: ${esc(T("svg_logo.step-counts.json"))} – circles stay <code>CIRCLE</code>, extruded arcs <code>CYLINDRICAL_SURFACE</code>.</p>

<h2>7 Measurements</h2>
<h3>Kernel time</h3>
<p>Release build, <code>cad eval --json</code> without daemon, median of 3 runs (language = evaluation including shaping and SVG; kernel = all kernel calls). Browser: the same models in Chromium (SMLib as Wasm, T05).</p>
<table>
<tr><th>Model</th><th>Nodes</th><th>Kernel calls</th><th>Language ms</th><th>Kernel ms native</th><th>Kernel ms browser</th></tr>
${timeRow("hello", "\"Hello World\" (Inter), extruded", "hello")}
${timeRow("vierzig", "40 characters (Inter), extruded", "40")}
${timeRow("vierzig_mono", "40 characters (JetBrains Mono)", null)}
${timeRow("platte", "\"Hello World\" on a plate (union)", null)}
${timeRow("svg_logo", "tests/models/spec/svg_logo.mcad", null)}
${timeRow("text_schild", "tests/models/spec/text_sign.mcad", null)}
</table>
<p>Per glyph: one extrusion per contour (outer, hole), difference for holes, one union of all glyphs (they do not touch). Text <b>on</b> a face is expensive: SMLib imprints each glyph into the top face (sign: one union of ≈ 9 s native). The new color region rule (plan 17.10) avoids the Booleans with the whole here; before, the contact sheet of T01 took 21 s natively and failed ("intersection failed"), now 4 s.</p>
<h3>Wasm module</h3>
${wasm ? `<table><tr><th></th><th>raw</th><th>gzip -9</th></tr><tr><td><code>cad_wasm_bg.wasm</code> before (commit ${esc(wasm.base ?? "cafe7a4")})</td><td class="n">${num(wasm.before.raw)} B</td><td class="n">${num(wasm.before.gzip)} B</td></tr><tr><td>with text/SVG (before the merge of step 2)</td><td class="n">${num(wasm.after.raw)} B</td><td class="n">${num(wasm.after.gzip)} B</td></tr>${wasm.merged ? `<tr><td>after the merge with sketches step 2 (state of this report)</td><td class="n">${num(wasm.merged.raw)} B</td><td class="n">${num(wasm.merged.gzip)} B</td></tr>` : ""}<tr><td>growth from text/SVG</td><td class="n">+${num(wasm.after.raw - wasm.before.raw)} B</td><td class="n">+${num(wasm.after.gzip - wasm.before.gzip)} B</td></tr><tr><td>of which font files</td><td class="n">${num(wasm.fonts)} B</td><td class="n">–</td></tr></table><p class="small">The rest of the growth is code (rustybuzz/ttf-parser, roxmltree, SVG and text logic). <code>smlib.wasm</code>: ${num(wasm.smlib)} B (+726 B from <code>disjoint_union</code> in the facade).</p>` : "<p>(missing)</p>"}

<h2>8 UI</h2>
<p>Checks via the UI state (<code>window.__cad</code>), scenes and bounding boxes; screenshots from <code>web/e2e/text-svg.spec.ts</code>.</p>
${scenarios.map((s) => `<h3>${esc(s.id)} ${esc(s.title)}</h3><p>${md(s.caption)}</p>${(s.images ?? []).length === 2 ? pair(pw(s.images[0].name), s.images[0].label, pw(s.images[1].name), s.images[1].label) : (s.images ?? []).map((i) => fig(pw(i.name), i.label)).join("")}${s.code && s.code.split("\n").length <= 16 ? pre(s.code) : ""}<ul>${(s.checks ?? []).map((c) => `<li>${md(c)}</li>`).join("")}</ul>`).join("\n")}

<h2>9 Test runs</h2>
<table>
<tr><th>Run</th><th>Result</th></tr>
<tr><td><code>cargo test --workspace --no-fail-fast</code></td><td>${cargoSum ? `${cargoSum.ok} passed, ${cargoSum.failed} failed` : "–"}</td></tr>
<tr><td><code>cargo clippy --workspace --all-targets -- -D warnings</code> · <code>cargo fmt --check</code></td><td>${clippyOk ? "clean" : "finding"} · ${fmtOk ? "clean" : "diff"}</td></tr>
<tr><td><code>pnpm typecheck</code> · <code>pnpm test</code> (Vitest)</td><td>${typecheck !== null ? (/error TS/i.test(typecheck) ? "errors" : "clean") : "–"} · ${vitestSum ?? "–"} passed</td></tr>
<tr><td><code>pnpm e2e</code> (Playwright, Chromium)</td><td>${pwStats ? `${pwStats.passed} / ${pwStats.total}${pwStats.failed.length ? ` – failed: ${pwStats.failed.map((f) => esc(f.title)).join(", ")}` : ""}` : "–"}${flaky ? ` · ${md(flaky.trim())}` : ""}</td></tr>
<tr><td><code>tests/openscad-compat/compare.py --eval</code></td><td>${compareSum ? `${compareSum[0]}/${compareSum[1]}` : "–"}</td></tr>
<tr><td><code>tests/step/verify.py</code> (OpenCASCADE)</td><td>${esc(stepSum ?? "–")}</td></tr>
<tr><td><code>tests/everyday/check.sh</code></td><td>${esc(alltagLine ?? "–")}</td></tr>
<tr><td><code>node crates/kernel-smlib/js/test-node.mjs</code></td><td>${nodeTest && /all node tests passed/.test(nodeTest) ? "passed" : "–"}</td></tr>
</table>
<p>New or changed Rust tests (${newTests.length}):</p>
<p class="small">${newTests.map((t) => `<code>${esc(t.name)}</code> ${t.res === "ok" ? '<span class="ok">ok</span>' : '<span class="bad">FAILED</span>'}`).join(" · ")}</p>
<p class="small">SVG (<code>cad-svg</code>): path commands, arcs and flags, transformations, units with OpenSCAD reference values, fill rules, hidden/unfilled elements, warnings with line, <code>id</code>, error cases. Text (<code>cad-font</code>): bounding box and area against OpenSCAD (20 cases), glyph "O" with a hole, kerning "AV", alignment, directions, font names, custom font, CFF outlines, required character set, fallback. <code>cad-eval</code>: profiles, project files, errors, data URLs. CLI: examples, volume, STEP circle, <code>watch</code> follows the SVG. Vitest: data files (<code>web/src/logic/files.test.ts</code>).</p>

<h2>10 Limitations</h2>
<ul>
<li>The custom stencil font is missing (step 3, after <code>stroke</code>); Allerta Stencil lacks 20 characters of the required character set (taken from JetBrains Mono, not stenciled).</li>
<li>Kernel time: many glyphs on a common face are expensive (the union in SMLib grows superlinearly: 9 glyphs ≈ 1.5 s, 28 ≈ 9 s native); in the browser it takes about 1.3 to 1.8 times as long; a single kernel call over 15 s triggers the watchdog.</li>
<li>Fill rules assume non-intersecting contours within one element (fonts with overlapping contours, e.g. some variable fonts, and self-intersecting SVG paths are not resolved). Variable fonts only in the default instance.</li>
<li>SVG: no text, no <code>use</code> clones, no CSS classes, no strokes as areas, no masks/clip paths; percent and em lengths are treated like plain numbers.</li>
<li>File names must appear as strings in the project (a name composed at run time is not loaded).</li>
<li>Found with the logo: the facade computed unions of extruded sketch arcs over 90° wrongly (sketches step 1); fixed in step 2 (<code>profile::split_arcs</code>), the interim workaround in the import was removed.</li>
<li>STEP: the writer now writes extrusions of elliptical arcs as an extrusion of an <code>ELLIPSE</code> (OpenCASCADE had integrated the rational B-spline wrongly by 3.6·10<sup>−3</sup>). Open: OpenCASCADE measures the extrusion of the quadratic Q/T wave from <code>wave.svg</code> 3 % too small (kernel 652.25 mm², analytical 652.19 mm²) – <code>svg_logo.mcad</code> therefore takes only the ellipse and the arc from <code>wave.svg</code>; the wave itself is shown in 5.1.</li>
<li>Default alignment of vertical text as in OpenSCAD (shaper offset); <code>valign</code> by the actual glyph bounding box (OpenSCAD: rasterized FreeType metrics, ≤ 0.005 mm).</li>
</ul>
</main>
</body>
</html>
`;
writeFileSync(join(out, "report.html"), html);
console.log(`${join(out, "report.html")}: ${html.length} characters, ${scenarios.length} scenarios, ${osc.length} OpenSCAD comparisons`);
