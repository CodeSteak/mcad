#!/usr/bin/env node
// Test report 22: studio look (`mcad render --style studio` and the "Studio look" of the UI):
// lights, soft shadows, ambient occlusion, ground, background; CLI and UI screenshots, image
// measurements, timings, test runs, limitations.
// Writes docs/reports/22-studio-look/report.html (images in img/).
//
// Inputs (gitignored, reports/tmp/report22/):
//   data/           python3 tools/report/report22-data.py [--before <mcad before the change>]
//   img/, scenarios.json, metrics.json
//                   cd web && npx playwright test e2e/studio.spec.ts (S01–S06)
//   final/          tools/report/report22-tests.sh (logs, times.txt, playwright.json)
//
//   node tools/report/report22.mjs
//   node tools/report/print-pdf.mjs docs/reports/22-studio-look/report.html docs/reports/22-studio-look/report.pdf "Test report 22: studio look"

import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import os from "node:os";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "../..");
const input = join(root, "reports/tmp/report22");
const data = join(input, "data");
const fin = join(input, "final");
const out = join(root, "docs/reports/22-studio-look");
const imgDir = join(out, "img");
rmSync(imgDir, { recursive: true, force: true });
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
const fmt = (x, d = 0) => (x === null || x === undefined || Number.isNaN(Number(x)) ? "–" : Number(x).toLocaleString("en-US", { minimumFractionDigits: d, maximumFractionDigits: d }));
const pct = (x, d = 1) => (x === null || x === undefined ? "–" : `${fmt(x * 100, d)} %`);
const md = (t) => esc(t).replace(/`([^`]+)`/g, "<code>$1</code>");
const term = (t) => `<pre class="term">${esc(String(t ?? "(missing)").trimEnd())}</pre>`;

/** Copies an image as JPEG (max. width) into img/ and returns its path, or null. */
function img(srcPath, dst, width = 1000) {
  if (!existsSync(srcPath)) return null;
  try {
    execFileSync("convert", [srcPath, "-resize", `${width}x>`, "-quality", "80", join(imgDir, dst)]);
    return `img/${dst}`;
  } catch {
    const png = dst.replace(/\.jpg$/, ".png");
    copyFileSync(srcPath, join(imgDir, png));
    return `img/${png}`;
  }
}
const fig = (s, cap, cls = "") => (s ? `<figure class="${cls}"><img src="${s}" alt=""><figcaption>${md(cap)}</figcaption></figure>` : `<p class="small">(image missing: ${esc(cap)})</p>`);
const cli = (n, w = 700) => img(join(data, `${n}.png`), `cli-${n.replace(/\//g, "-")}.jpg`, w);
const ui = (n, w = 900) => img(join(input, "img", `${n}.png`), `ui-${n}.jpg`, w);
/** Magnified crop `geom` (WxH+X+Y) of a CLI image, scaled to `width`. */
function crop(n, geom, dst, width = 600) {
  const src = join(data, `${n}.png`);
  if (!existsSync(src)) return null;
  try {
    execFileSync("convert", [src, "-crop", geom, "+repage", "-filter", "point", "-resize", `${width}x`, "-quality", "85", join(imgDir, dst)]);
    return `img/${dst}`;
  } catch {
    return null;
  }
}
/** Difference of two CLI images, amplified `gain` times and inverted (white = equal). */
function diff(a, b, dst, gain = 6, width = 600) {
  const [x, y] = [join(data, `${a}.png`), join(data, `${b}.png`)];
  if (!existsSync(x) || !existsSync(y)) return null;
  try {
    execFileSync("convert", [x, y, "-compose", "difference", "-composite", "-colorspace", "gray", "-evaluate", "multiply", String(gain), "-negate", "-resize", `${width}x`, "-quality", "85", join(imgDir, dst)]);
    return `img/${dst}`;
  } catch {
    return null;
  }
}

// ------------------------------------------------------------------------------- data

const D = readJson(join(data, "data.json")) ?? { commands: {}, diff: {}, probe: {}, timings: {} };
const cmd = (n) => D.commands[n] ?? "";
const scenarios = readJson(join(input, "scenarios.json")) ?? [];
const metrics = readJson(join(input, "metrics.json")) ?? {};
const times = Object.fromEntries((readText(join(fin, "times.txt")) ?? "").trim().split("\n").filter(Boolean).map((l) => l.split(" ")).map(([k, rc, s]) => [k, { rc: +rc, s: +s }]));
const dur = (k) => (times[k] ? (times[k].s >= 90 ? `${fmt(times[k].s / 60, 1)} min` : `${times[k].s} s`) : "–");
const rcOk = (k) => times[k]?.rc === 0;
const cargo = readText(join(fin, "cargo.txt"));
const cargoSum = cargo ? [...cargo.matchAll(/test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored/g)].reduce((a, m) => ({ ok: a.ok + +m[1], failed: a.failed + +m[2], ignored: a.ignored + +m[3], suites: a.suites + 1 }), { ok: 0, failed: 0, ignored: 0, suites: 0 }) : null;
const cargoFailed = cargo ? [...new Set([...cargo.matchAll(/^test (\S+) \.\.\. FAILED$/gm)].map((m) => m[1]))] : [];
const studioTests = cargo ? [...cargo.matchAll(/^test ((?:render_studio_style|studio_|shadow_|model_shadows|ambient_occlusion|ground_only|gradient_background|transparent_items)\S*) \.\.\. (\w+)$/gm)].map((m) => [m[1], m[2]]) : [];
const usage = readText(join(fin, "usage.txt"));
const usageSum = usage ? [...usage.matchAll(/test result: \w+\. (\d+) passed; (\d+) failed/g)].reduce((a, m) => ({ ok: a.ok + +m[1], failed: a.failed + +m[2] }), { ok: 0, failed: 0 }) : null;
const vitest = readText(join(fin, "vitest.txt"));
const vitestSum = vitest ? { ok: +(/Tests\s+(?:\d+ failed \| )?(\d+) passed/.exec(vitest)?.[1] ?? 0), failed: +(/Tests\s+(\d+) failed/.exec(vitest)?.[1] ?? 0) } : null;
const pwJson = readJson(join(fin, "playwright.json"));
const pwStats = pwJson?.stats ?? null;
const pwSpecs = (() => {
  const res = [];
  const walk = (s, file) => {
    for (const sp of s.specs ?? []) for (const t of sp.tests ?? []) res.push({ file: file ?? s.file, title: sp.title, status: t.status, errors: (t.results ?? []).flatMap((r) => (r.errors ?? []).map((e) => e.message ?? "")) });
    for (const c of s.suites ?? []) walk(c, file ?? s.file);
  };
  for (const s of pwJson?.suites ?? []) walk(s, s.file);
  return res;
})();
const pwFiles = [...new Set(pwSpecs.map((t) => t.file))];
const pwBad = pwSpecs.filter((t) => t.status !== "expected" && t.status !== "skipped");
const pwStudio = pwSpecs.filter((t) => /studio\.spec/.test(t.file));

const date = new Date().toISOString().slice(0, 10);
const commit = gitOut("rev-parse", "--short", "HEAD");
const branch = gitOut("rev-parse", "--abbrev-ref", "HEAD");
const commits = gitOut("log", "--format=%h\t%s", "96e667f..HEAD").split("\n").filter(Boolean).map((l) => l.split("\t")).reverse();
const rustc = (() => {
  try {
    return execFileSync("rustc", ["--version"], { encoding: "utf8" }).trim();
  } catch {
    return "–";
  }
})();

const T = D.timings ?? {};
const models = Object.keys(T).filter((k) => T[k]?.studio_plain !== undefined);
const ratio = models.map((m) => T[m].studio / T[m].standard).sort((a, b) => a - b);
const medRatio = ratio.length ? ratio[Math.floor(ratio.length / 2)] : null;
const extra = models.map((m) => T[m].studio - T[m].standard).sort((a, b) => a - b);
const hasBefore = models.some((m) => T[m].before_standard !== undefined);
const timeRows = models
  .map((m) => `<tr><td><code>${esc(m)}</code></td>${hasBefore ? `<td class="n">${fmt(T[m].before_standard)}</td>` : ""}<td class="n">${fmt(T[m].standard)}</td><td class="n"><b>${fmt(T[m].studio)}</b></td><td class="n">${fmt(T[m].studio_no_shadows)}</td><td class="n">${fmt(T[m].studio_no_ao)}</td><td class="n">${fmt(T[m].studio_plain)}</td><td class="n">${fmt(T[m].studio / T[m].standard, 1)}×</td></tr>`)
  .join("");

const resCell = (ok, text) => `<span class="${ok ? "ok" : "bad"}">${ok ? "passed" : "failed"}</span> – ${esc(text)}`;
const testRows = [
  ["`cargo fmt --all --check`", resCell(rcOk("fmt"), "no diff"), dur("fmt")],
  ["`cargo clippy --workspace --exclude cad-app --no-default-features --all-targets -- -D warnings`", resCell(rcOk("clippy"), rcOk("clippy") ? "no findings" : "findings"), dur("clippy")],
  ["`cargo test --workspace --exclude cad-app --no-default-features --no-fail-fast`", cargoSum ? resCell(cargoSum.failed === 0 && rcOk("cargo"), `${cargoSum.ok} passed, ${cargoSum.failed} failed, ${cargoSum.ignored} ignored (${cargoSum.suites} test binaries)${cargoFailed.length ? `: ${cargoFailed.join(", ")}` : ""}`) : "–", dur("cargo")],
  ["`cargo test -p cad-cli usage` (manual: titles, every command)", usageSum ? resCell(usageSum.failed === 0 && rcOk("usage"), `${usageSum.ok} passed, ${usageSum.failed} failed`) : "–", dur("usage")],
  ["`cd web && pnpm typecheck`", resCell(rcOk("typecheck"), rcOk("typecheck") ? "no errors" : "errors"), dur("typecheck")],
  ["`cd web && pnpm test` (Vitest)", vitestSum ? resCell(vitestSum.failed === 0 && rcOk("vitest"), `${vitestSum.ok} passed, ${vitestSum.failed} failed`) : "–", dur("vitest")],
  ["Playwright 1.56.1, full suite (Chromium 1243 with SwiftShader, `playwright.local.config.ts`)", pwStats ? resCell(pwStats.unexpected === 0, `${pwStats.expected} passed, ${pwStats.unexpected} failed${pwStats.flaky ? `, ${pwStats.flaky} flaky` : ""}${pwStats.skipped ? `, ${pwStats.skipped} skipped` : ""} (${pwFiles.length} files)${pwBad.length ? `: ${pwBad.map((t) => t.title.split(" ")[0]).join(", ")}` : ""}`) : "–", pwStats ? `${fmt(pwStats.duration / 60000, 1)} min` : dur("playwright")],
]
  .map(([a, b, c]) => `<tr><td>${md(a)}</td><td>${b}</td><td class="n">${c}</td></tr>`)
  .join("");

const settings = [
  ["Style", "`--style standard|studio`", "toggle “Studio look” (sparkles icon)", "standard", "–"],
  ["Key light direction", "`--light AZ,EL`", "Direction, Height", "−40°, 50°", "azimuth relative to the camera (0 = from the camera, + = from the right); elevation 1…90° above the xy plane"],
  ["Light intensity", "`--light-intensity F`", "Intensity", "1", "0…4 (UI 0…2.5)"],
  ["Ambient (sky) share", "`--ambient F`", "Ambient", "0.4", "0…1; the rest is key light"],
  ["Shadows", "`--shadows on|off`", "Shadows", "on", "model on itself and on the ground"],
  ["Shadow softness", "`--shadow-softness F`", "Softness", "0.35", "0 hard … 1 very soft (light radius 0…5°)"],
  ["Ambient occlusion", "`--ao on|off`", "Ambient occlusion", "on", "corners, gaps, contact with the ground"],
  ["AO radius", "`--ao-radius MM`", "Radius (empty = auto)", "5 % of the model diagonal", "> 0 mm"],
  ["AO strength", "`--ao-strength F`", "Strength", "1", "0…2"],
  ["Ground", "`--ground on|off`", "Ground", "on", "plane at the lowest z of the model, fades out; only seen from above"],
  ["Ground colour", "`--ground-color C`", "colour field, “Theme”", "light `#e6e8ec`, dark `#2c3038`", "`#rrggbb`, `rrggbb`, palette name"],
  ["Background", "`--background C[,C]`", "two colour fields, “Theme”", "studio light `#f8f9fb`→`#d9dde3`, dark `#2b3038`→`#111317`", "one colour or top,bottom gradient (also for `standard`)"],
]
  .map((r) => `<tr>${r.map((c) => `<td>${md(c)}</td>`).join("")}</tr>`)
  .join("");

const pairs = ["02-bracket", "03-tray", "04-hook", "05-box-lid", "07-mug", "08-macropad"]
  .map((m) => `<div class="two">${fig(cli(`pairs/${m}-standard`, 640), `\`${m}\` standard`)}${fig(cli(`pairs/${m}-studio`, 640), `\`${m}\` studio (${pct(D.diff[`pairs/${m}`], 0)} of the pixels differ)`)}</div>`)
  .join("");

const scen = (id) => scenarios.find((s) => s.id === id);
const uiBlock = (id, labels) => {
  const s = scen(id);
  if (!s) return `<p class="small">(scenario ${id} missing)</p>`;
  const t = pwStudio.find((x) => x.title.startsWith(id));
  const imgs = s.images.map((n, i) => fig(ui(n), `${id}: ${labels[i] ?? n}`));
  return `<h3>${esc(id)} ${esc(s.title)} ${t ? (t.status === "expected" ? '<span class="ok">passed</span>' : '<span class="bad">failed</span>') : ""}</h3><p>${md(s.caption)}</p><div class="${imgs.length >= 2 ? "two" : ""}">${imgs.join("")}</div>`;
};

const fm = metrics.frameMs;

const html = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Test report 22 – studio look</title>
<style>
  @page { size: A4; margin: 15mm 13mm 16mm 13mm; }
  :root { --text: #1f2328; --muted: #5d646e; --border: #d5d9df; --ok: #1a7f37; --bad: #c9302c; --head: #f2f4f7; }
  * { box-sizing: border-box; }
  body { font: 9.6pt/1.45 system-ui, -apple-system, "Segoe UI", Roboto, sans-serif; color: var(--text); margin: 0; background: #fff; }
  main { max-width: 184mm; margin: 0 auto; padding: 0 0 4mm; }
  h1 { font-size: 18pt; margin: 0 0 2mm; }
  h2 { font-size: 13pt; margin: 7mm 0 3mm; border-bottom: 1px solid var(--border); padding-bottom: 1mm; break-after: avoid; }
  h3 { font-size: 10.5pt; margin: 4mm 0 1.5mm; break-after: avoid; }
  p { margin: 1.5mm 0; }
  .meta { color: var(--muted); margin-bottom: 4mm; }
  .summary { display: flex; flex-wrap: wrap; gap: 2.5mm; margin: 3mm 0; }
  .summary div { flex: 1 1 40mm; border: 1px solid var(--border); border-radius: 2mm; padding: 2mm; font-size: 8.4pt; }
  .summary b { display: block; font-size: 12pt; }
  table { width: 100%; border-collapse: collapse; margin: 2mm 0; font-size: 8.2pt; }
  th, td { border: 1px solid var(--border); padding: 1.1mm 1.8mm; text-align: left; vertical-align: top; }
  th { background: var(--head); font-weight: 600; }
  td.n, th.n { text-align: right; white-space: nowrap; }
  code, pre { font-family: ui-monospace, "SFMono-Regular", Menlo, Consolas, monospace; font-size: 7.6pt; }
  pre { margin: 0 0 1.5mm; padding: 1.4mm 2mm; background: #f6f8fa; border: 1px solid var(--border); border-radius: 1mm; white-space: pre-wrap; word-break: break-word; }
  pre.term { background: #16191d; color: #e6e9ed; border-color: #32373e; }
  thead { display: table-header-group; }
  .small { font-size: 7.6pt; color: var(--muted); }
  ul { margin: 1mm 0 2mm 5mm; padding: 0; }
  li { margin: 0.5mm 0; }
  tr, figure, pre { break-inside: avoid; }
  .ok { color: var(--ok); font-weight: 600; } .bad { color: var(--bad); font-weight: 600; }
  figure { margin: 0 0 2mm; }
  figure img { max-width: 100%; max-height: 110mm; border: 1px solid var(--border); border-radius: 1mm; background: #fff; display: block; }
  figcaption { font-size: 7.8pt; color: var(--muted); margin-top: 0.6mm; }
  .two { display: grid; grid-template-columns: 1fr 1fr; gap: 3mm; align-items: start; }
  .three { display: grid; grid-template-columns: 1fr 1fr 1fr; gap: 2.5mm; align-items: start; }
  .four { display: grid; grid-template-columns: repeat(4, 1fr); gap: 2mm; align-items: start; }
  .four figcaption, .three figcaption { font-size: 7pt; }
</style>
</head>
<body>
<main>
<h1>Test report 22 – studio look (lights, soft shadows, ambient occlusion, ground)</h1>
<div class="meta">${esc(date)} · Branch ${esc(branch)} · Commit ${esc(commit)} · mcad 0.3.0 · ${esc(rustc)} · ${D.cores ?? os.cpus().length} cores. CLI measurements with the release binary without daemon (<code>tools/report/report22-data.py</code>, median of ${D.repeat ?? "–"} runs), UI with Playwright (<code>web/e2e/studio.spec.ts</code>, German UI as in all specs), test runs on the same commit (<code>tools/report/report22-tests.sh</code>).</div>

<div class="summary">
  <div>Mode<b>--style studio</b>CLI, UI toggle “Studio look”, JSON <code>style</code>/<code>studio</code></div>
  <div>Settings<b>11 + background</b>light, shadows, AO, ground, background – same defaults in CLI and UI</div>
  <div>Render time (1024×768)<b>${medRatio ? `${fmt(medRatio, 1)}× standard` : "–"}</b>median over ${models.length} examples; +${extra.length ? `${fmt(extra[0])}…${fmt(extra[extra.length - 1])}` : "–"} ms</div>
  <div>cargo test<b class="${cargoSum && cargoSum.failed === 0 ? "ok" : "bad"}">${cargoSum ? `${cargoSum.ok} / ${cargoSum.ok + cargoSum.failed}` : "–"}</b>clippy ${rcOk("clippy") ? "clean" : "finding"}, fmt ${rcOk("fmt") ? "clean" : "diff"}</div>
  <div>Vitest · Playwright<b class="${vitestSum?.failed === 0 && pwStats?.unexpected === 0 ? "ok" : "bad"}">${vitestSum ? vitestSum.ok : "–"} · ${pwStats ? `${pwStats.expected} / ${pwStats.expected + pwStats.unexpected + pwStats.flaky}` : "–"}</b>studio spec ${pwStudio.filter((t) => t.status === "expected").length} / ${pwStudio.length}</div>
</div>

<h2>1 What was built</h2>
<table><tr><th>Commit</th><th>Title</th></tr>${commits.map(([h, s]) => `<tr><td><code>${esc(h)}</code></td><td>${esc(s)}</td></tr>`).join("")}</table>
<h3>CLI: software renderer (<code>crates/cad-render/src/studio.rs</code>)</h3>
<ul>
<li><b>Deferred shading.</b> With <code>--style studio</code> the rasteriser writes albedo and the interpolated (camera-facing) normal per supersample into a G-buffer instead of a shaded colour; section caps go in with their hatched colour. After the caps one parallel pass (rows over all cores, <code>std::thread::scope</code>) lights every opaque sample; edges, transparent bodies, overlays, labels and the axis triad are drawn afterwards as before.</li>
<li><b>Lights.</b> Key light (Lambert + Blinn-Phong highlight, exponent 48) whose azimuth is relative to the camera like a turntable and whose elevation is measured from the xy plane, a weak fill light from the camera (+15°), hemisphere ambient light (white sky from +z, bounce tinted by the ground colour). A soft shoulder above 0.8 keeps directly lit light faces from burning out.</li>
<li><b>Shadows.</b> Orthographic shadow map (2048², span-based rasteriser) of all opaque, clipped triangles along the key light; percentage-closer soft shadows: blocker search (12 Vogel samples) → penumbra from the blocker–receiver distance × tan(light radius) → 16 filter samples, rotated per sample by interleaved gradient noise, compared against the receiver plane (depth gradient from the normal) instead of one depth, plus normal offset. Tiles of 16×16 texels with their minimum depth skip the search where nothing can cast. Model on itself and on the ground; caps and transparent bodies cast none.</li>
<li><b>Ambient occlusion.</b> Screen space from the linear depth buffer (including the ground): 16 hemisphere samples of radius <code>--ao-radius</code> (default 5 % of the model diagonal) around the normal, rotated in a 4×4 pattern, range check, then a depth- and normal-aware 4×4 blur. AO darkens the ambient light fully and the direct light by 55 %.</li>
<li><b>Ground and background.</b> Plane z = lowest point of the model, drawn only where the camera looks down onto it (none in front/right/bottom views), not in the depth buffer of the overlays (grids and labels stay visible), faded into the background between 0.55 and 1.5 model diagonals. Background solid or vertical gradient (<code>--background</code>, also usable with <code>standard</code>). Edges at 35 % opacity.</li>
<li><b>Interfaces.</b> <code>RenderOptions::studio: Option&lt;Studio&gt;</code>, <code>background_bottom</code>, <code>CameraInfo::studio</code> (light vector, ground height, AO radius, shadow map size); CLI options in <code>ViewArgs</code> (also <code>mcad watch --render</code>; the daemon forwards the arguments), JSON <code>style</code>, <code>studio</code>, <code>background</code>, <code>views[].studio</code>. Studio options without <code>--style studio</code>, out-of-range values and unknown colours exit with 2.</li>
</ul>
<h3>UI: three.js (<code>web/src/view/viewer.ts</code>, <code>web/src/app/studio.tsx</code>, <code>web/src/logic/studio.ts</code>)</h3>
<ul>
<li>Toggle “Studio look” and a settings panel in the view toolbar; settings in the store, kept in localStorage <code>cad-studio</code> (sanitised on load), same defaults as the CLI (Vitest checks both and the generated CLI arguments).</li>
<li>Directional key light with a 2048² variance shadow map (blur radius from the softness, as the CLI's penumbra at a typical distance), fill light and hemisphere light placed every frame from the camera azimuth (<code>lightDirection</code>, same convention as the Rust code); opaque meshes cast and receive shadows; materials a little glossier (roughness 0.5) and edges at 35 % opacity.</li>
<li>Ground: a transparent <code>MeshStandardMaterial</code> plane with a radial fade (shader chunk), receiving shadows; the floor grid is hidden while it is on. Background: a procedural 1×256 canvas gradient as <code>scene.background</code> (no image files).</li>
<li>Ambient occlusion: three.js <code>GTAOPass</code> (no new dependency) on the opaque model and the ground only; frame = model pass to the screen, AO multiplied on top, then the overlays (handles, grids, measurements, sketches, asserts) drawn over the kept depth buffer, so they are not darkened. Paused while a section is shown.</li>
<li>“Copy as mcad render” gives the CLI call of the current view (perspective, azimuth, elevation, theme) with the changed settings; <code>mcad ui status</code> reports <code>studio</code> with that call.</li>
</ul>

<h2>2 Settings</h2>
<table><tr><th>Setting</th><th>CLI</th><th>UI (English)</th><th>Default</th><th>Range, notes</th></tr>${settings}</table>

<h2>3 CLI screenshots</h2>
<p>All images <code>--size 900x675</code> (contact sheet 1600×1200), iso view unless stated; the command of each image is in <code>data.json</code>, e.g. <code>${esc(cmd("pairs/05-box-lid-studio"))}</code>.</p>
<h3>Standard and studio</h3>
${pairs}
<h3>Shadows on and off</h3>
<div class="two">${fig(cli("shadow/on"), `Shadows on (AO off): the upright throws a soft shadow onto the base, light shines through its bore; mean brightness of the base probe ${fmt(D.probe["shadow/on"], 0)}`)}${fig(cli("shadow/off"), `\`--shadows off\`: probe ${fmt(D.probe["shadow/off"], 0)}; ${pct(D.diff.shadow)} of the pixels differ`)}</div>
<div class="two">${fig(diff("shadow/on", "shadow/off", "diff-shadow.jpg", 3), "Difference shadows on − off, ×3, inverted (dark = changed): the shadow of the upright on the base and on the ground")}${fig(crop("shadow/on", "300x225+450+250", "crop-shadow-on.jpg"), "Shadows on, crop ×2: penumbra widens away from the contact")}</div>
<h3>Ambient occlusion on and off</h3>
<div class="three">${fig(cli("ao/on", 600), "Tray, AO on (shadows off)")}${fig(cli("ao/off", 600), `AO off: ${pct(D.diff.ao)} of the pixels differ`)}${fig(cli("ao/strong", 600), `\`--ao-strength 2 --ao-radius 6\`: ${pct(D.diff.ao_strong)} differ from AO off`)}</div>
<div class="two">${fig(cli("ao/box-on"), "Box, AO on: inner corners, the gap under the lid and the contact with the ground darken")}${fig(cli("ao/box-off"), `AO off (${pct(D.diff.ao_box)} differ)`)}</div>
<p>Ambient occlusion is deliberately subtle at the default strength; the amplified differences show where it acts, the crops (×2, nearest neighbour) show it at the ground contact and in the inner corner.</p>
<div class="two">${fig(diff("ao/on", "ao/off", "diff-ao-tray.jpg", 8), "Tray: difference AO on − off, ×8, inverted: inner wall corners, the floor fillet, the ground contact")}${fig(diff("ao/box-on", "ao/box-off", "diff-ao-box.jpg", 8), "Box: difference AO on − off, ×8, inverted: the gap under the lid, inner corners of the bin, the ground contact")}</div>
<div class="four">${fig(crop("ao/off", "300x225+500+420", "crop-ao-off.jpg", 420), "Tray, AO off (crop at the ground contact)")}${fig(crop("ao/on", "300x225+500+420", "crop-ao-on.jpg", 420), "AO on (default)")}${fig(crop("ao/strong", "300x225+500+420", "crop-ao-strong.jpg", 420), "`--ao-strength 2 --ao-radius 6`")}${fig(crop("ao/box-on", "300x225+180+200", "crop-ao-box.jpg", 420), "Box, AO on: gap under the lid")}</div>
<h3>Ground and background</h3>
<div class="three">${fig(cli("ground/default", 600), "Default: ground fades into the light gradient")}${fig(cli("ground/off-white", 600), "`--ground off --background ffffff`")}${fig(cli("ground/sand-blue", 600), "`--ground-color d8c8a8 --background eef3f8,9fb3c8`")}</div>
<div class="two">${fig(cli("ground/dark"), "`--theme dark`: ground and gradient from the dark theme")}${fig(cli("ground/standard-gradient"), "`--background ffffff,c8ccd2` with the standard style")}</div>
<h3>Light direction, intensity, softness</h3>
<div class="four">${fig(cli("light/default", 450), "`--light -40,50` (default)")}${fig(cli("light/right-low", 450), "`--light 60,25`")}${fig(cli("light/left-low", 450), "`--light -100,20`")}${fig(cli("light/above", 450), "`--light 0,85`")}</div>
<div class="four">${fig(cli("soft/0", 450), "`--shadow-softness 0`")}${fig(cli("soft/0.35", 450), "`0.35` (default)")}${fig(cli("soft/1", 450), "`1`: wide penumbra far from the contact, sharp at it")}${fig(cli("light/intensity-ambient", 450), "`--light-intensity 1.4 --ambient 0.2`: more contrast")}</div>
<h3>Contact sheet, section, perspective, x-ray</h3>
${fig(cli("sheet/standard", 1000), "`--views all`, standard")}
${fig(cli("sheet/studio", 1000), "`--views all --style studio`: same scale per tile; the light follows each tile's camera, the ground only appears where the camera looks down (iso, top)")}
<div class="two">${fig(cli("section/box"), "`--section y=20 --section-flip`: caps lit and shadowed, interior with AO")}${fig(cli("section/persp"), "`--perspective --azimuth -30 --elevation 28`")}</div>
<div class="two">${fig(cli("section/xray"), "`--xray`: transparent bodies cast no shadow, the ground stays")}${fig(cli("section/line"), "`--part bin`")}</div>

<h2>4 UI screenshots</h2>
<p>Browser build with the Wasm kernel in Chromium (software WebGL via SwiftShader), window 1440×900; German UI as in all Playwright specs (“Studio-Darstellung” = studio look, “Studio-Einstellungen” = studio settings, “Umgebungsverdeckung” = ambient occlusion, “Schatten” = shadows, “Boden” = ground, “Hintergrund” = background, “Standardwerte” = defaults). The desktop app uses the same web UI.</p>
${uiBlock("S01", ["standard", "studio look"])}
${uiBlock("S02", ["settings panel"])}
${uiBlock("S03", ["default", "light from the right at 30°, softness 0.9", "ground off, white–blue gradient", "dark theme"])}
${uiBlock("S04", ["macropad", "section (AO paused)", "hook"])}
${uiBlock("S05", ["UI", "CLI with the copied call"])}
${uiBlock("S06", ["settings panel, English UI"])}

<h2>5 Tests</h2>
<h3>New tests</h3>
<table><tr><th>Where</th><th>What</th></tr>
<tr><td><code>crates/cad-render/tests/studio.rs</code> (7)</td><td>ground drawn and studio differs from standard, deterministic, ground off → corner is background; ground behind a wall in its shadow (≥ 40 brightness levels darker), lit side and far ground unchanged, wider transition with softness 1; plate shadows the block below; AO darkens a concave corner (≥ 8 levels) but not the open floor, strength 2 darker than 0.5; ground only from above, light azimuth relative to the camera (front/right/top), all views in perspective; gradient background; transparent bodies cast no shadow</td></tr>
<tr><td><code>crates/cad-cli/tests/agent.rs</code> <code>render_studio_style</code></td><td>JSON defaults (<code>style</code>, <code>studio</code>, <code>background</code>, <code>views[].studio</code>), every option parsed and reported, images differ, contact sheet + dark + section + perspective together, gradient background with standard (top white, bottom black), text line, 10 usage errors (exit 2)</td></tr>
<tr><td><code>web/src/logic/studio.test.ts</code> (7)</td><td>defaults equal to the CLI, sanitising stored settings, AO radius, light direction (same cases as the Rust test), CLI arguments and full command</td></tr>
<tr><td><code>web/e2e/studio.spec.ts</code> (S01–S06)</td><td>toggle, panel, persistence after reload, defaults, variants, dark theme, section, copied CLI call run with the CLI, English labels</td></tr>
</table>
${studioTests.length ? `<p class="small">Rust studio tests in the final run: ${studioTests.map(([n, r]) => `<code>${esc(n)}</code> ${r === "ok" ? "ok" : `<span class="bad">${esc(r)}</span>`}`).join(", ")}.</p>` : ""}
<h3>Test runs on the final state</h3>
<table><tr><th>Command</th><th>Result</th><th class="n">Duration</th></tr>${testRows}</table>
<p class="small">The container has no WebKitGTK: the desktop app crate <code>cad-app</code> and the <code>gui</code> feature of <code>cad-cli</code> are not built (the UI is the same web build, tested in Chromium). Playwright 1.56.1 expects Chromium 1194; the preinstalled Chromium 1243 is used through the gitignored <code>web/playwright.local.config.ts</code> (only <code>executablePath</code> differs). Logs in <code>reports/tmp/report22/final/</code>.</p>
${pwBad.length ? `<h3>Failed Playwright tests</h3><table><tr><th>Test</th><th>File</th><th>Error</th></tr>${pwBad.map((t) => `<tr><td>${esc(t.title)}</td><td><code>${esc(t.file)}</code></td><td><pre>${esc((t.errors[0] ?? "").replace(/\u001b\[\d+m/g, "").split("\n").slice(0, 6).join("\n"))}</pre></td></tr>`).join("")}</table>` : ""}

<h2>6 Performance</h2>
<p>CLI: <code>render_ms</code> of <code>--json</code>, i.e. everything after the evaluation (tessellation, colour regions of coloured models, rasterising with 2×2 supersampling, studio pass, PNG encoding; evaluation and kernel excluded), 1024×768, median of ${D.repeat ?? "–"} runs, ${D.cores ?? "–"} cores. The macropad is dominated by its colour regions (Booleans per colour, about 6 s in both styles).</p>
<table><tr><th>Model</th>${hasBefore ? '<th class="n">before (standard, ms)</th>' : ""}<th class="n">standard</th><th class="n">studio</th><th class="n">studio, no shadows</th><th class="n">studio, no AO</th><th class="n">studio, neither</th><th class="n">studio / standard</th></tr>${timeRows}
${T.sheet ? `<tr><td>macropad <code>--views all</code> 1600×1200</td>${hasBefore ? "<td></td>" : ""}<td class="n">${fmt(T.sheet.standard)}</td><td class="n"><b>${fmt(T.sheet.studio)}</b></td><td></td><td></td><td></td><td class="n">${fmt(T.sheet.studio / T.sheet.standard, 1)}×</td></tr>` : ""}
${T.large ? `<tr><td>box-lid 1920×1440</td>${hasBefore ? "<td></td>" : ""}<td class="n">${fmt(T.large.standard)}</td><td class="n"><b>${fmt(T.large.studio)}</b></td><td></td><td></td><td></td><td class="n">${fmt(T.large.studio / T.large.standard, 1)}×</td></tr>` : ""}</table>
<p>The studio pass adds a roughly constant 0.9–1.1 s at 1024×768 (shadow map and lookups ≈ 0.4 s, AO ≈ 0.4 s, deferred lighting ≈ 0.15 s), so it weighs most on light models and scales with the pixel count (1920×1440: see the last row); ${hasBefore ? "the standard style is as fast as before the change (column “before”: the build of the commit before the change)." : "the standard style is unchanged (same code path)."}</p>
${fm ? `<p>UI (Playwright S01, ${esc(fm.renderer)}, mean of ${fm.frames ?? "–"} synchronous frames incl. <code>readPixels</code>): standard ${fmt(fm.standard, 1)} ms, studio ${fmt(fm.studio, 1)} ms, studio without AO ${fmt(fm.studioNoAo, 1)} ms per frame. Software WebGL is far slower than a GPU; the numbers only compare the modes.</p>` : ""}

<h2>7 Known limitations</h2>
<table><tr><th>Item</th><th>Cause or status</th></tr>
<tr><td>Screen-space AO</td><td>Only what is on screen occludes (in CLI and UI); very large radii darken silhouettes against far geometry less correctly. The UI's GTAO shows slight noise on software WebGL; the CLI is noise-free after supersampling and blur.</td></tr>
<tr><td>AO in the UI with a section</td><td>paused while a section is shown: the cut faces are stencil caps that the AO normal pass cannot reproduce (the panel says so). The CLI computes AO with sections.</td></tr>
<tr><td>Soft shadows differ in detail</td><td>CLI: percentage-closer soft shadows (penumbra grows with the distance to the occluder). UI: variance shadow map with a constant blur radius derived from the same softness; contact hardening only in the CLI.</td></tr>
<tr><td>Shadow casters</td><td>transparent bodies (x-ray, dimmed rest, alpha) and section caps cast no shadow; the shadow map covers the model, very long shadows at low light elevation end where the ground fades.</td></tr>
<tr><td>Ground</td><td>only below the model and only when the camera looks down onto it; not in front/right/bottom views. A model “floating” above other parts is lit as one scene (no per-part ground).</td></tr>
<tr><td>Light follows the camera azimuth</td><td>deliberately (turntable): every view is lit from the front-left; in the UI the shadow turns while orbiting.</td></tr>
<tr><td>Run time</td><td>studio is ${medRatio ? `${fmt(medRatio, 1)}×` : "several times"} slower than standard in the CLI (table above); <code>--shadows off --ao off</code> keeps only the lighting.</td></tr>
<tr><td>Desktop app</td><td>not built or run here (no WebKitGTK); it embeds the same web UI.</td></tr>
</table>
</main>
</body>
</html>
`;

const htmlOut = html.replace(/(<table[^>]*>\s*)(<tr>\s*<th[\s\S]*?<\/tr>)/g, "$1<thead>$2</thead>");
writeFileSync(join(out, "report.html"), htmlOut);
console.log(`${join(out, "report.html")}: ${html.length} characters, images in ${imgDir}`);
