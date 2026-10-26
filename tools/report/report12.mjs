#!/usr/bin/env node
// Test report 12 ("fixes for colors and daemon"): the findings F1–F9 of the independent
// review (tests/color-review/README.md) and the daemon socket path – cause, correction,
// evidence (before: the reviewer's images, after: CLI and UI), Bambu Studio result, limits.
// Writes docs/reports/12-farben-korrekturen/report.html (images in img/).
//
// Inputs (gitignored, reports/tmp/report12/):
//   cli/*.png, *.txt, *.json, sources/              tools/report/report12-data.sh
//   scenarios.json, img/*.jpg                       cd web && pnpm exec playwright test e2e/color-fixes.spec.ts
//   playwright.json                                 copy of reports/tmp/report2/playwright.json (whole suite: pnpm e2e)
//   vitest.txt, typecheck.txt                       cd web && pnpm test, pnpm typecheck
//   tauri/tauri-e2e.json, tauri/img/*.png           node web/e2e-tauri/desktop.e2e.mjs --bin target/release/mcad --out reports/tmp/report12/tauri
//   cargo.txt, clippy.txt, fmt.txt                  cargo test --workspace --no-fail-fast, clippy -D warnings, fmt --check
//   compare.txt, step.txt, alltag.txt               tests/openscad-compat/compare.py, tests/step/verify.py, tests/everyday/check.sh
//
//   node tools/report/report12.mjs
//   node tools/report/print-pdf.mjs docs/reports/12-farben-korrekturen/report.html docs/reports/12-farben-korrekturen/report.pdf "Test report 12: fixes for colors and daemon"

import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import os from "node:os";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "../..");
const input = join(root, "reports/tmp/report12");
const out = join(root, "docs/reports/12-farben-korrekturen");
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
const pre = (t) => `<pre>${esc(t.trimEnd())}</pre>`;

// ------------------------------------------------------------------------------- images

function img(src, dst) {
  if (!existsSync(src)) return null;
  if (src.endsWith(".png") && !dst.endsWith(".png")) {
    try {
      execFileSync("convert", [src, "-resize", "1440x>", "-quality", "85", join(imgDir, dst)]);
      return `img/${dst}`;
    } catch {
      dst = dst.replace(/\.jpg$/, ".png");
    }
  }
  copyFileSync(src, join(imgDir, dst));
  return `img/${dst}`;
}
const before = (n) => img(join(root, "tests/color-review/img", n), `vorher-${n.replace(/\.png$/, ".jpg")}`);
const cli = (n) => img(join(input, "cli", `${n}.png`), `cli-${n}.jpg`);
const pw = (n) => img(join(input, "img", `${n}.jpg`), `ui-${n}.jpg`);
const tauriImg = (n) => img(join(input, "tauri/img", n), `tauri-${n.replace(/\.png$/, ".jpg")}`);
const fig = (src, cap) => (src ? `<figure><img src="${src}" alt=""><figcaption>${md(cap)}</figcaption></figure>` : `<p class="small">(image missing: ${esc(cap)})</p>`);
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
      .filter((t) => /(^|::)(f[1-9]_|all_review|long_ipc|set_filament_of_a_parameter|filament_table_rules|material_outside|random_colours_do_not|filament_declarations|no_color_group)/.test(t.name))
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
  return { total: all.length, passed: all.filter((t) => t.status === "passed").length, failed: all.filter((t) => t.status !== "passed" && t.status !== "skipped"), neu: all.filter((t) => t.file?.includes("farben-korrekturen")) };
})();
const vitest = readText(join(input, "vitest.txt"));
const vitestSum = vitest ? (/Tests\s+(\d+) passed/.exec(vitest)?.[1] ?? null) : null;
const tauri = readJson(join(input, "tauri/tauri-e2e.json"));
const tauriOk = tauri ? tauri.results.filter((r) => r.ok).length : null;
const compare = readText(join(input, "compare.txt"));
const compareSum = compare ? /(\d+)\/(\d+)/.exec(compare.split("\n").reverse().find((l) => /\d+\/\d+/.test(l)) ?? "")?.slice(1, 3) : null;
const step = readText(join(input, "step.txt"));
const stepSum = step ? (/\d+\/\d+ passed/.exec(step)?.[0] ?? null) : null;
const alltag = readText(join(input, "alltag.txt"));
const alltagLine = alltag ? (alltag.trim().split("\n").reverse().find((l) => /passed|bestanden|deviat|abweich|models|Modelle/i.test(l)) ?? alltag.trim().split("\n").at(-1)) : null;
const verify = readJson(join(input, "verify.json"));
const verifyCg = readJson(join(input, "verify-cg.json"));
const scenarios = (readJson(join(input, "scenarios.json")) ?? []).sort((a, b) => a.id.localeCompare(b.id));
const scen = (id) => scenarios.find((s) => s.id === id);
const rand = readJson(join(input, "f5-randnames.json"));

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

// ------------------------------------------------------------------------------- F5 numbers

function oklab(hex) {
  const lin = (c) => (c / 255 <= 0.04045 ? c / 255 / 12.92 : ((c / 255 + 0.055) / 1.055) ** 2.4);
  const [r, g, b] = [1, 3, 5].map((i) => lin(parseInt(hex.slice(i, i + 2), 16)));
  const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
  const m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
  const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
  return [0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s, 1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s, 0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s];
}
const dE = (a, b) => Math.hypot(...oklab(a).map((x, i) => x - oklab(b)[i]));
const PAL = ["#c74240", "#dc813b", "#f7d974", "#91ca55", "#4ca167", "#217274", "#6eccec", "#2e65b9", "#693d8c", "#cd67b7", "#f7b8c6", "#6e4223", "#202225", "#7b7d81", "#abaeb2", "#f4f3f0"];
const f5 = (() => {
  const rows = (rand?.colors ?? []).filter((c) => c.source === "random");
  if (!rows.length) return null;
  let minPair = { d: 9 };
  for (let i = 0; i < rows.length; i++) for (let j = i + 1; j < rows.length; j++) {
    const d = dE(rows[i].key, rows[j].key);
    if (d < minPair.d) minPair = { d, a: rows[i].label, b: rows[j].label };
  }
  let minPal = { d: 9 };
  for (const r of rows) for (const p of PAL) {
    const d = dE(r.key, p);
    if (d < minPal.d) minPal = { d, a: r.label, b: p };
  }
  return { n: rows.length, slots: new Set((rand?.colors ?? []).map((c) => c.filament)).size, all: (rand?.colors ?? []).length, minPair, minPal, rows };
})();

// ------------------------------------------------------------------------------- findings

const status = [
  ["F1", "high", "Same geometry in different colors: view and STEP in the first color", "fixed"],
  ["F2", "high", "Slot choice in the UI on a color() with a parameter/loop color affected all colors", "fixed (filament() table)"],
  ["F3", "medium", "View ≠ print (overlap, fillet at a color boundary, color() in the 2D profile)", "fixed (regions in the view; mixed 2D colors: warning)"],
  ["F4", "medium", "Material outside parts: UI omitted it, CLI message wrong", "fixed"],
  ["F5", "medium", "Random colors collide", "fixed"],
  ["F6", "medium/low", "Slivers between neighbors of the same color", "fixed"],
  ["F7", "low", "Alpha variants, silent clamping, transparent, empty name", "fixed (rules + warnings)"],
  ["F8", "low", "> 16 slots, shared slots without notice, color group for single-color", "fixed"],
  ["F9", "low", "--per-color with .stl, color list with --part, file name with use/include", "fixed"],
  ["D", "–", "Daemon silently fails to start with a socket path that is too long", "fixed (substitute path)"],
  ["B", "–", "Bambu Studio: color dialog overwrites slots", "avoided (3MF without color group)"],
];

const testsOf = (re) => newTests.filter((t) => re.test(t.name)).map((t) => `<code>${esc(t.name)}</code> ${t.res === "ok" ? '<span class="ok">ok</span>' : '<span class="bad">FAILED</span>'}`).join(", ") || "–";

const html = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Test report 12 – fixes for colors and daemon</title>
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
  figure img { max-width: 100%; max-height: 95mm; border: 1px solid var(--border); border-radius: 1mm; }
  figcaption { font-size: 7.8pt; color: var(--muted); }
  .two { display: grid; grid-template-columns: 1fr 1fr; gap: 3mm; align-items: start; }
  dl.f { display: grid; grid-template-columns: 22mm 1fr; gap: 0.8mm 3mm; margin: 1.5mm 0; }
  dl.f dt { font-weight: 600; color: var(--muted); }
  dl.f dd { margin: 0; }
  .note { border-left: 1mm solid #e8b84a; padding: 1mm 2.5mm; background: #fff8e6; margin: 2mm 0; }
  .sw { display: inline-block; width: 3.2mm; height: 3.2mm; border-radius: 0.6mm; border: 0.25mm solid rgba(0,0,0,0.3); vertical-align: -0.5mm; margin-right: 0.5mm; }
  section.finding { break-before: auto; }
</style>
</head>
<body>
<main>
<h1>Test report 12 – fixes for colors and daemon</h1>
<div class="meta">Findings F1–F9 of the independent review (<code>tests/color-review/</code>) and daemon socket path · cad-eval, cad-engine, cad-session, cad-cli, cad-ipc, web · ${esc(date)} · Branch ${esc(branch)} · Commit ${esc(commit)} · ${esc(rustc)} · ${os.cpus().length} cores</div>

<div class="summary">
  <div>cargo test --workspace<b class="${cargoSum && cargoSum.failed === 0 ? "ok" : "bad"}">${cargoSum ? `${cargoSum.ok} / ${cargoSum.ok + cargoSum.failed}` : "–"}</b>of which ${newTests.length} new regression tests</div>
  <div>clippy · fmt<b class="${clippyOk && fmtOk ? "ok" : "bad"}">${clippy == null ? "–" : clippyOk ? "clean" : "finding"} · ${fmt == null ? "–" : fmtOk ? "clean" : "diff"}</b>-D warnings</div>
  <div>Vitest · Playwright<b class="${pwStats && pwStats.failed.length === 0 ? "ok" : "bad"}">${vitestSum ?? "–"} · ${pwStats ? `${pwStats.passed} / ${pwStats.total}` : "–"}</b>new: G01–G04</div>
  <div>Tauri-E2E<b class="${tauri && tauriOk === tauri.results.length && !tauri.fatal ? "ok" : "bad"}">${tauri ? `${tauriOk} / ${tauri.results.length}` : "–"}</b>new: T15</div>
  <div>OpenSCAD · STEP<b class="${compareSum && compareSum[0] === compareSum[1] ? "ok" : "bad"}">${compareSum ? `${compareSum[0]}/${compareSum[1]}` : "–"} · ${esc(stepSum?.match(/\d+\/\d+/)?.[0] ?? "–")}</b>compare.py · verify.py</div>
  <div>Everyday models<b class="${alltag && !/FEHL|ERROR|FAIL|weicht ab|abweichend|deviat/i.test(alltag) ? "ok" : "bad"}">${alltag ? "unchanged" : "–"}</b>tests/everyday/check.sh</div>
  <div>3MF (lib3mf, trimesh)<b class="${verify?.ok ? "ok" : "bad"}">${verify ? (verify.ok ? "passed" : "error") : "–"}</b>Bambu color dialog: ${verify ? (verify.bambu_model?.color_dialog ? "yes" : "no") : "–"}</div>
</div>

<h2>1 Task and result</h2>
<p>An independent test agent reviewed the color features from report 11 and reported nine findings (F1–F9) and one daemon bug (<code>tests/color-review/README.md</code>, test cases in <code>cases/</code>, images in <code>img/</code>). All findings are fixed; each has a regression test in Rust, the UI findings additionally in Vitest, Playwright and Tauri E2E. The review's test cases all run as <code>crates/cad-cli/tests/color_review.rs</code> (each file: no errors, region volume = part volume, random colors at a distance from the palette). "Before" images are the reviewer's (commit d8fe983), "after" images are from the CLI and UI of this commit.</p>
<table>
<tr><th>No.</th><th>Severity</th><th>Finding</th><th>Status</th></tr>
${status.map(([n, s, b, st]) => `<tr><td>${n}</td><td>${esc(s)}</td><td>${md(b)}</td><td class="ok">${esc(st)}</td></tr>`).join("\n")}
</table>

<h2>2 Findings</h2>

<section class="finding">
<h3>F1 (high) – same geometry, different colors</h3>
<dl class="f">
<dt>Cause</dt><dd>The cache key of a node does not contain the color (intended: a new color recomputes nothing). <code>color("red") cube(5)</code> and <code>translate(…) color("blue") cube(5)</code> therefore have the same cube node content, the same face tag and – from the cache – the same kernel faces. The view (<code>tagColors</code>: tag → first node with color) and STEP (<code>nodes_for_tag(tag).first()</code>) took the first color for all faces of this tag. The 3MF was correct because the color regions place the instances individually.</dd>
<dt>Fix</dt><dd>Cache reuse stays; the color is assigned via instances and regions: (1) for models with colors the view shows the <b>color regions</b> (the bodies of the 3MF) as soon as they are computed (UI "Show color regions", now the default; <code>cad render</code> without <code>--show faces</code>); (2) the color per face (before that, for highlighted subtrees and <code>--show faces</code>) only takes nodes below the drawn node and, among several with the same content, the one whose bounding box contains the triangle (<code>faceColors</code> in <code>web/src/logic/colors.ts</code>, <code>face_colors</code> in <code>cad-cli/src/render.rs</code>); (3) STEP face colors come from the rule set of the regions without boolean: sample points of each face against the placed material of each color node (<code>kernel.classify</code>), the latest wins (<code>Engine::face_region_colors</code>), as synthetic tags in a copy of the B-rep.</dd>
<dt>Tests</dt><dd>${testsOf(/^f1_|^all_review/)}; Vitest "report 12 F1"; Playwright G01; Tauri T15.</dd>
</dl>
${pair(before("pal2.png"), "before: pal2.scad, three red cubes (reviewer)", cli("f1-pal2"), "after: `cad render` (color regions)")}
${pair(cli("f1-pal2-flaechen"), "after: `--show faces` (color per face per instance)", cli("f1-palette"), "after: palette.scad, 16 identical tiles in a loop")}
<p>STEP of pal2.scad (faces per color):</p>${pre(T("f1-step.txt"))}
${fig(pw("g01-flaechen"), "UI: color per face with \"Show color regions\" off – each instance in its color")}
<p class="small">Limitation: the color per face is an approximation (bounding box test; with overlapping bounding boxes of identically shaped bodies of different color, the later one). Exact are the regions that the view shows after the computation; in STEP a face merged across a color boundary gets one color (majority of the sample points).</p>
</section>

<section class="finding">
<h3>F2 (high) – slot choice with a parameter or loop color</h3>
<dl class="f">
<dt>Cause</dt><dd><code>SetFilament</code> wrote <code>filament = n</code> on the first <code>color()</code> statement of the color. With <code>module bar(c) color(c) …</code> or <code>for (i) color(names[i]) …</code> this statement colors several colors – all of them got the slot (or the first specification won with a warning).</dd>
<dt>Decision</dt><dd>A slot table in the code: <code>filament("red", 4);</code> at the top level of the main file, one statement per color. Reason: the code stays the only source (CLI and UI export the same, undo/Git see the change, an agent reads it too); the table applies to exactly one color and is independent of where the color arises (parameter, loop, library); it is an ordinary module call (no new syntax, OpenSCAD only reports an unknown module); a block <code>filaments { … }</code> would need parser changes, <code>$filaments = […]</code> would have the scoping rules of <code>$</code> variables and could be overridden anywhere. The UI still writes <code>filament = n</code> on the statement if <b>all</b> statements of the color name it as a literal (readable in place), otherwise the table line; if there is a table line, it changes its slot, "automatic" removes the line.</dd>
<dt>Rules</dt><dd>The table is evaluated before all other statements (separate pass) and takes precedence; a contradicting <code>filament = m</code> warns and is ignored; <code>filament()</code> in modules, loops or libraries warns ("applies only at the top level of the main file"); a user's own module <code>filament</code> takes precedence.</dd>
<dt>Tests</dt><dd>${testsOf(/set_filament_of_a_parameter|filament_table_rules|filament_declarations/)}; Playwright G02; Tauri T15.</dd>
</dl>
${fig(pw("g02-tabelle"), "UI after slot 4 for red in slot_mod.scad: `filament(\"red\", 4);` in the code, blue and green automatic (before, all three got slot 4)")}
<p><code>cad outline</code> of the file with table:</p>${pre(T("f2-outline.txt").split("\n").filter((l) => /Colou?rs|Farben|F\d/.test(l)).join("\n"))}
</section>

<section class="finding">
<h3>F3 (medium) – view ≠ print</h3>
<dl class="f">
<dt>Cause</dt><dd>(a) With overlap the kernel merges coplanar faces of both bodies into one face with one tag – the color per face cannot show the color boundary. (b) The fillet at a color boundary belongs to the uncolored <code>union</code> node (shown gray), the regions give it the remainder color (red). (c) <code>color()</code> in the 2D profile: the view showed the color of the 2D leaf, the color table did not look below profile operations (default, no entry, no warning).</dd>
<dt>Fix</dt><dd>(a, b) The view shows the color regions (F1). (c) If all 2D leaves of an extrusion without its own color have the same color, the extrusion is a body of that color (as with <code>color()</code> around it); mixed 2D colors are not separated: one body in the default color and the warning "colours in the 2D profile of linear_extrude are not separated …".</dd>
<dt>Tests</dt><dd>${testsOf(/^f3_/)}; Playwright G03.</dd>
</dl>
${pair(before("r_overlap23.png"), "before: r_overlap2/3 (left), regions (right) – reviewer", cli("f3-overlap3"), "after: r_overlap3.scad, blue owns the overlap")}
${pair(before("r_fillet2.png"), "before: fillet gray, exported red (reviewer)", cli("f3-fillet2"), "after: fillet red as in the print")}
${pair(before("r_extrude_faces.png"), "before: 2D color shown, exported as default (reviewer)", cli("f3-extrude"), "after: red and blue as exported, mixed profile default with warning")}
${pre(T("f3-extrude.txt"))}
<p class="small">Limitation: mixed colors in a 2D profile below an extrusion are not split into bodies per color (warning; remedy: extrude per color).</p>
</section>

<section class="finding">
<h3>F4 (medium) – material outside parts</h3>
<dl class="f">
<dt>Cause</dt><dd>Color list and UI export only took the parts (rest: 0 mm³, omitted in the 3MF); the CLI export without <code>--part</code> took the root as one object and still reported "one object per part".</dd>
<dt>Fix</dt><dd>The regions comprise the parts and the top-level nodes that are neither a part nor contain one ("outside any part", <code>cad_session::rest_nodes</code>); the default export (CLI without <code>--part</code> and UI, both <code>Session::export_colors</code>) writes one object per part plus the object "outside any part" – the files from CLI and UI are identical. The message lists the objects (<code>color_export_notes</code>), the UI shows them in the status line.</dd>
<dt>Tests</dt><dd>${testsOf(/^f4_|material_outside/)}; Playwright G04; Tauri T15.</dd>
</dl>
${pre(T("f4-export.txt"))}
</section>

<section class="finding">
<h3>F5 (medium) – random colors collide</h3>
<dl class="f">
<dt>Cause</dt><dd>Only the hue came from the hash (L, C fixed): names fell onto the same value (<code>baz</code>/<code>violet</code>, <code>x</code>/<code>rose</code>) and were merged into one entry and slot; others lay close to palette colors.</dd>
<dt>Fix</dt><dd>Hue, lightness (0.45–0.80) and chroma (0.06–0.16) from the hash; one candidate sequence per name (SplitMix64); the first candidate with ΔE<sub>OK</sub> ≥ 0.07 to palette and default and – within the model, in order of first occurrence – ≥ 0.06 to the random colors of other names is chosen. Different names thus never coincide by chance; entries are merged only for exactly the same value (e.g. <code>"#f00"</code> and <code>[1, 0, 0]</code>).</dd>
<dt>Evidence</dt><dd>${f5 ? `randnames.scad: ${f5.n} unknown names (36 statements, rot/Rot/ROT is one name), ${f5.all} entries on ${f5.slots} different slots; smallest distance between two random colors ΔE<sub>OK</sub> ${num(f5.minPair.d, 3)} (${esc(f5.minPair.a)}/${esc(f5.minPair.b)}), to the palette ${num(f5.minPal.d, 3)} (${esc(f5.minPal.a)} ↔ ${esc(f5.minPal.b)}).` : "–"}</dd>
<dt>Tests</dt><dd>${testsOf(/^f5_|random_colours_do_not/)}.</dd>
</dl>
${f5 ? `<p>${f5.rows.map((r) => `<span class="sw" style="background:${esc(r.key)}"></span>${esc(r.label)}`).join(" · ")}</p>` : ""}
<p class="small">Limitation: if a name has to move away within the model, its color depends on the names before it (deterministic, but not the same across models).</p>
</section>

<section class="finding">
<h3>F6 – slivers between neighbors of the same color</h3>
<dl class="f">
<dt>Cause</dt><dd><code>piece(C) = R ∩ M(C) − ⋃ M(later)</code> also subtracted a touching later body of the same color; thin slivers (0.00016 mm) remained at the contact face, which overlapped after the union: r_touch.scad red 2000.032 instead of 2000 mm³, without warning.</dd>
<dt>Fix</dt><dd>Only the material of later color nodes of a <b>different</b> color is subtracted; the pieces of one color are united as before. The assignment is the same (same color), the slivers disappear.</dd>
<dt>Tests</dt><dd>${testsOf(/^f6_|all_review/)} (all cases: sum = volume to 10⁻⁶).</dd>
</dl>
${pre(T("m-r_touch.txt"))}
</section>

<section class="finding">
<h3>F7 – alpha, clamping, transparent, empty name</h3>
<dl class="f">
<dt>Rules</dt><dd>The identity of a print color is its RGB value: <code>alpha</code> only affects the view (the color region is drawn transparent), <code>color("red", alpha = 0.5)</code> is the entry <code>red</code> (one slot, the conflict check ignores alpha). Vector values and <code>alpha</code> outside 0–1 are clamped with a warning (for 0–255 values with a hint to <code>"#rrggbb"</code>). <code>color("transparent")</code> is not a print color: warning, the material stays default (no filament <code>#00000000</code>); <code>color("")</code> and <code>color()</code> warn and do not color.</dd>
<dt>Tests</dt><dd>${testsOf(/^f7_/)}.</dd>
</dl>
${pre(T("m-values.txt"))}
</section>

<section class="finding">
<h3>F8 – slots above 16, shared slots, single-color 3MF</h3>
<dl class="f">
<dt>Fix</dt><dd>The color table reports automatic slots above 16 as a warning ("more than 16 filaments …"), two colors on the same fixed slot and a fixed slot 1 next to the default as a notice (info). The 3MF no longer has a color group (section 3), not even with <code>--color-group</code> when there is only one color.</dd>
<dt>Tests</dt><dd>${testsOf(/^f8_|no_color_group/)}.</dd>
</dl>
${pre(T("m-f1.txt"))}
</section>

<section class="finding">
<h3>F9 – --per-color, --part, use/include</h3>
<dl class="f">
<dt>Fix</dt><dd><code>--per-color</code> requires the extension <code>.zip</code> (otherwise exit 2 with a suggestion); <code>--per-color --part</code> shows only the colors and volumes of the selected parts; the color list names lines of other files as <code>lib.scad:1</code> (CLI and UI; in the UI a click opens the file at that position).</dd>
<dt>Tests</dt><dd>${testsOf(/^f9_/)}; Vitest "report 12 F9".</dd>
</dl>
${pre(T("f9-stl.txt") + "\n" + T("f9-part.txt"))}
</section>

<section class="finding">
<h3>Daemon – socket path longer than the Unix limit</h3>
<dl class="f">
<dt>Cause</dt><dd><code>sun_path</code> holds 108 bytes (Linux; 107 without NUL). With a deep <code>CAD_IPC_DIR</code>, <code>bind</code> failed ("path must be shorter than SUN_LEN") and the client silently computed without the daemon.</dd>
<dt>Fix</dt><dd><code>cad_ipc::socket_path_in</code>: if <code>&lt;dir&gt;/&lt;name&gt;.sock</code> is too long, the socket is placed in <code>/tmp/cad-&lt;uid&gt;/&lt;hash&gt;-&lt;name&gt;.sock</code> (directory with the same checks: 0700, own user, no symlink; <code>hash</code> = FNV-1a of the canonical runtime directory, so that several long directories do not interfere). Lock and log file stay in the runtime directory; <code>list</code> also finds substitute sockets (UI endpoints); <code>cad daemon status</code> shows the path with "substitute path" (JSON <code>socket_substitute</code>), the log names it.</dd>
<dt>Tests</dt><dd>${testsOf(/long_ipc/)}.</dd>
</dl>
${pre(T("daemon-long.txt"))}
</section>

<h2>3 Bambu Studio: color dialog and slots</h2>
<p>Sources retrieved on ${esc(date)} (<code>raw.githubusercontent.com</code>, BambuStudio <code>master</code>, OrcaSlicer <code>main</code>; checksums in <code>reports/tmp/report12/sources.sha256</code>):</p>
${pre(T("sources.txt"))}
<ul>
<li><b>Condition of the dialog</b> (<code>Plater.cpp</code>): <code>load_model &amp;&amp; !color_group_map.empty() &amp;&amp; !volume_color_data.empty() &amp;&amp; en_3mf_file_type != From_BBS</code>. The importer fills <code>volume_color_data</code> only for sub-parts whose triangles have a color via <code>pid</code>/<code>pindex</code> (<code>bbs_3mf.cpp</code>, <code>triangle_colors.size() == triangles_count</code>).</li>
<li><b>OK in the dialog</b> calls <code>apply_volume_filament_ids</code> and resets <code>extruder</code> of every sub-part; "Cancel" and closing leave it.</li>
<li><b><code>Metadata/model_settings.config</code></b> is read by the importer for all 3MF files (not tied to the version check), and it takes over <code>extruder</code> per sub-part and object.</li>
<li><b>Ways to avoid the dialog:</b> (1) no color group – chosen; (2) <code>Application = BambuStudio-…</code> – rejected: claims a foreign vendor, switches on the importer's project paths (version check, expected project files, plates) and could break with new Bambu versions; (3) <code>project_settings.config</code> with <code>filament_colour</code> – rejected: not loaded for foreign files (<code>dont_load_config</code> without Bambu version), OrcaSlicer would apply it over printer and process settings; (4) paint data (<code>mmu_segmentation</code>) – unnecessary, the sub-parts already carry the slot.</li>
<li><b>OrcaSlicer</b> does not have this dialog; its importer uses color groups only for objects without volume metadata (<code>color_group_id_to_extruder_id_map</code>), with <code>model_settings.config</code> the <code>extruder</code> values apply. Dropping the color group is therefore compatible with Orca.</li>
</ul>
<p><b>Implementation:</b> the multi-color 3MF no longer has <code>m:colorgroup</code> nor <code>pid</code>/<code>pindex</code>; the slots are only in <code>model_settings.config</code>. In Bambu Studio the colors of the slots come from the printer's filaments (AMS). For programs that display 3MF colors, <code>cad export --color-group</code> still writes the color group; the message then says "Bambu Studio opens a colour dialog – choose “Cancel” there".</p>
<table>
<tr><th>File</th><th>lib3mf</th><th>trimesh</th><th>Bambu model: extruder per sub-part</th><th>Color dialog (model of Plater.cpp)</th></tr>
<tr><td>mehrfarbig.3mf (default)</td><td>${verify ? (verify.results?.lib3mf ? '<span class="ok">ok</span>' : '<span class="bad">error</span>') : "–"}</td><td>${verify ? (verify.results?.trimesh ? '<span class="ok">watertight</span>' : '<span class="bad">open</span>') : "–"}</td><td>${esc(verify?.bambu_model?.objects?.map((o) => `${o.name}: ${o.volumes.map((v) => v.extruder).join("/")}`).join(", ") ?? "–")}</td><td>${verify ? (verify.bambu_model?.color_dialog ? "opens" : '<span class="ok">does not open</span>') : "–"}</td></tr>
<tr><td>mehrfarbig-cg.3mf (<code>--color-group</code>)</td><td>${verifyCg ? (verifyCg.results?.lib3mf ? '<span class="ok">ok</span>' : '<span class="bad">error</span>') : "–"}</td><td>${verifyCg ? (verifyCg.results?.trimesh ? '<span class="ok">watertight</span>' : '<span class="bad">open</span>') : "–"}</td><td>${esc(verifyCg?.bambu_model?.objects?.map((o) => `${o.name}: ${o.volumes.map((v) => v.extruder).join("/")}`).join(", ") ?? "–")}</td><td>${verifyCg ? (verifyCg.bambu_model?.color_dialog ? "opens (\"Cancel\" keeps slots)" : "does not open") : "–"}</td></tr>
</table>
${pre(T("export.txt"))}
<p class="small">Limitation: not checked in the programs themselves (Bambu Studio, OrcaSlicer) – no graphical program in the container. Whether Bambu Studio adds missing filaments on loading when a sub-part uses slot 5 and the project has only four filaments depends on the loading path (<code>Plater.cpp</code>: with "geometry only", filaments up to the highest number are created).</p>

<h2>4 UI and desktop</h2>
${scenarios.map((s) => `<h3>${esc(s.id)} ${esc(s.title)}</h3><p>${md(s.caption)}</p>${(s.images ?? []).map((i) => fig(pw(i.name), i.label)).join("")}<ul>${(s.checks ?? []).map((c) => `<li>${md(c)}</li>`).join("")}</ul>`).join("\n")}
${tauri ? (() => {
  const t15 = tauri.results.find((r) => r.id === "T15");
  return t15 ? `<h3>T15 (Tauri) ${esc(t15.title)}</h3><ul>${t15.checks.map((c) => `<li class="${c.ok ? "" : "bad"}">${md(c.text)}</li>`).join("")}</ul>${t15.images.map((i) => fig(tauriImg(i.name), i.label)).join("")}` : "";
})() : ""}

<h2>5 Test runs</h2>
<table>
<tr><th>Run</th><th>Result</th></tr>
<tr><td><code>cargo test --workspace --no-fail-fast</code></td><td>${cargoSum ? `${cargoSum.ok} passed, ${cargoSum.failed} failed` : "–"}</td></tr>
<tr><td><code>cargo clippy --workspace --all-targets -- -D warnings</code> · <code>cargo fmt --check</code></td><td>${clippyOk ? "clean" : "finding"} · ${fmtOk ? "clean" : "difference"}</td></tr>
<tr><td><code>pnpm typecheck</code> · <code>pnpm test</code> (Vitest)</td><td>${readText(join(input, "typecheck.txt")) !== null ? (/error/i.test(readText(join(input, "typecheck.txt"))) ? "error" : "clean") : "–"} · ${vitestSum ?? "–"} passed</td></tr>
<tr><td><code>pnpm e2e</code> (Playwright, Chromium)</td><td>${pwStats ? `${pwStats.passed} / ${pwStats.total}${pwStats.failed.length ? ` – failed: ${pwStats.failed.map((f) => esc(f.title)).join(", ")}` : ""}` : "–"}</td></tr>
<tr><td>Tauri-E2E (<code>web/e2e-tauri/desktop.e2e.mjs</code>)</td><td>${tauri ? `${tauriOk} / ${tauri.results.length}${tauri.results.filter((r) => !r.ok).map((r) => ` – ${esc(r.id)} failed`).join("")}` : "–"}</td></tr>
<tr><td><code>tests/openscad-compat/compare.py</code></td><td>${compareSum ? `${compareSum[0]}/${compareSum[1]}` : "–"}</td></tr>
<tr><td><code>tests/step/verify.py</code></td><td>${esc(stepSum ?? "–")}</td></tr>
<tr><td><code>tests/everyday/check.sh</code> (volumes of the everyday models)</td><td>${esc(alltagLine ?? "–")}</td></tr>
</table>
<p>New Rust tests:</p>
<ul>${newTests.map((t) => `<li><code>${esc(t.name)}</code> ${t.res === "ok" ? '<span class="ok">ok</span>' : '<span class="bad">FAILED</span>'}</li>`).join("")}</ul>

<h2>6 Limitations</h2>
<ul>
<li>The view shows the exact regions only after they are computed (booleans per color node, cached); until then, while dragging and for the highlighted subtree under the cursor, the approximated color per face applies. If the cursor is on a statement, the rest is shown dimmed (as before).</li>
<li>STEP does not split faces at color boundaries (a merged face gets the majority color).</li>
<li>Mixed colors in a 2D profile below an extrusion are not separated (warning).</li>
<li>Random colors that have to move away within the model depend on the names before them.</li>
<li>Bambu Studio and OrcaSlicer were not checked in the programs themselves, but via their source code and a model of the importer (<code>tools/report/verify3mf.py</code>).</li>
<li>The substitute socket is always under <code>/tmp</code> (not <code>$TMPDIR</code>, which can itself be long).</li>
</ul>
</main>
</body>
</html>
`;
writeFileSync(join(out, "report.html"), html);
console.log(`${join(out, "report.html")}: ${html.length} characters, ${scenarios.length} scenarios, Tauri ${tauriOk ?? "–"}`);
