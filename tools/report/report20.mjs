#!/usr/bin/env node
// Test report 20 ("UI rework", docs/concepts/ui-redesign): before/after screenshots
// per area (head bar, editor, view and modes, side panel, light/dark, Tauri), the checks of the
// bars at 1280/1440/1920 px, keyboard, contrast of the teal accent, the app id for Wayland, test
// runs, deviations from the concept.
// Writes docs/reports/20-ui-ueberarbeitung/report.html (images in img/).
//
// Inputs (gitignored, reports/tmp/report20/):
//   vorher/*.png, nachher/*.png          node tools/report/report20-shots.mjs --label vorher|nachher --url …
//                                        (vorher = before: build of the commit before the overhaul)
//   img/*.png, scenarios.json,           cd web && pnpm exec playwright test e2e/ui-overhaul.spec.ts
//   metrics.json
//   tauri-vorher/img/*.png               node web/e2e-tauri/desktop.e2e.mjs --bin <cad vorher> --out reports/tmp/report20/tauri-vorher
//   tauri/tauri-e2e.json, tauri/img/*    node web/e2e-tauri/desktop.e2e.mjs --out reports/tmp/report20/tauri
//   playwright.txt                       whole suite (cd web && pnpm e2e > …)
//   vitest.txt, typecheck.txt,           cd web && pnpm test, pnpm typecheck,
//   contrast.txt                         SHOW_CONTRAST=1 pnpm exec vitest run src/logic/contrast.test.ts
//   cargo.txt, clippy.txt, fmt.txt       cargo test -p cad-app, clippy -p cad-app -D warnings, fmt --check
//   wmclass.txt, wmclass-vorher.txt      python3 tools/report/report20-wmclass.py <cad> (WM_CLASS under Xvfb)
//
//   node tools/report/report20.mjs
//   node tools/report/print-pdf.mjs docs/reports/20-ui-ueberarbeitung/report.html docs/reports/20-ui-ueberarbeitung/report.pdf "Test report 20: UI rework"

import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import os from "node:os";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "../..");
const input = join(root, "reports/tmp/report20");
const out = join(root, "docs/reports/20-ui-ueberarbeitung");
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
const num = (x, d = 0) => (x === null || x === undefined || Number.isNaN(Number(x)) ? "–" : Number(x).toLocaleString("en-US", { minimumFractionDigits: d, maximumFractionDigits: d }));
const md = (t) => esc(t).replace(/`([^`]+)`/g, "<code>$1</code>");
const pre = (t) => `<pre>${esc(String(t).trimEnd())}</pre>`;

/** Copies an image as JPEG (max. width) into img/ and returns its path, or null. */
function img(srcPath, dst, width = 1200) {
  if (!existsSync(srcPath)) return null;
  try {
    execFileSync("convert", [srcPath, "-resize", `${width}x>`, "-quality", "72", join(imgDir, dst)]);
    return `img/${dst}`;
  } catch {
    const png = dst.replace(/\.jpg$/, ".png");
    copyFileSync(srcPath, join(imgDir, png));
    return `img/${png}`;
  }
}
const before = (n, w) => img(join(input, "vorher", `${n}.png`), `vorher-${n}.jpg`, w);
const after = (n, w) => img(join(input, "nachher", `${n}.png`), `nachher-${n}.jpg`, w);
const pw = (n, w) => img(join(input, "img", `${n}.png`), `ui-${n}.jpg`, w);
const tauriBefore = (n) => img(join(input, "tauri-vorher", "img", `${n}.png`), `tauri-vorher-${n}.jpg`);
const tauriAfter = (n) => img(join(input, "tauri", "img", `${n}.png`), `tauri-${n}.jpg`);
const fig = (s, cap, cls = "") => (s ? `<figure class="${cls}"><img src="${s}" alt=""><figcaption>${md(cap)}</figcaption></figure>` : `<p class="small">(image missing: ${esc(cap)})</p>`);
/** Before/after pair, side by side or stacked (wide images). */
const pair = (b, a, cap, stacked = false) => `<div class="${stacked ? "stack" : "two"}">${fig(b, `Before: ${cap}`)}${fig(a, `After: ${cap}`)}</div>`;

// ------------------------------------------------------------------------------- data

const scenarios = readJson(join(input, "scenarios.json")) ?? [];
const byId = Object.fromEntries(scenarios.map((s) => [s.id, s]));
const metrics = readJson(join(input, "metrics.json")) ?? {};
const shotsBefore = readJson(join(input, "vorher", "shots.json")) ?? {};
const pwText = readText(join(input, "playwright.txt"));
const pwSum = pwText ? { passed: +(/(\d+) passed/.exec(pwText)?.[1] ?? 0), failed: +(/(\d+) failed/.exec(pwText)?.[1] ?? 0), flaky: +(/(\d+) flaky/.exec(pwText)?.[1] ?? 0), skipped: +(/(\d+) skipped/.exec(pwText)?.[1] ?? 0) } : null;
const pwFailed = pwText ? [...pwText.matchAll(/^\s+\d+\) \[chromium\] › (.+)$/gm)].map((m) => m[1].trim()) : [];
const pwFiles = pwText ? [...new Set([...pwText.matchAll(/› e2e\/([\w.-]+\.spec\.ts):/g)].map((m) => m[1]))] : [];
const vitest = readText(join(input, "vitest.txt"));
const vitestSum = vitest ? (/Tests\s+(\d+) passed/.exec(vitest)?.[1] ?? null) : null;
const typecheck = readText(join(input, "typecheck.txt"));
const typecheckOk = typecheck !== null && !/error TS/.test(typecheck);
const contrastTxt = readText(join(input, "contrast.txt")) ?? "";
const cargo = readText(join(input, "cargo.txt"));
const cargoSum = cargo ? [...cargo.matchAll(/test result: \w+\. (\d+) passed; (\d+) failed/g)].reduce((a, m) => ({ ok: a.ok + +m[1], failed: a.failed + +m[2] }), { ok: 0, failed: 0 }) : null;
const clippy = readText(join(input, "clippy.txt"));
const fmt = readText(join(input, "fmt.txt"));
const clippyOk = clippy !== null && !/^(error|warning)/m.test(clippy);
const fmtOk = fmt !== null && fmt.trim() === "";
const tauri = readJson(join(input, "tauri", "tauri-e2e.json"));
const tauriBeforeRun = readJson(join(input, "tauri-vorher", "tauri-e2e.json"));
const t18 = tauri?.results?.find((r) => r.id === "T18") ?? null;
const wmclass = readText(join(input, "wmclass.txt"));
const wmclassBefore = readText(join(input, "wmclass-vorher.txt"));

/** Contrast values per theme from `SHOW_CONTRAST=1 vitest run contrast.test.ts`. */
const contrast = (() => {
  const res = { light: {}, dark: {} };
  let cur = null;
  for (const l of contrastTxt.split("\n")) {
    const t = l.trim();
    if (t === "light" || t === "dark") cur = t;
    const m = /^(--[\w-]+)\/(--[\w-]+) ([\d.]+)$/.exec(t);
    if (cur && m) (res[cur][`${m[1]}/${m[2]}`] ??= []).push(+m[3]);
  }
  return res;
})();
const CONTRAST_ROWS = [
  ["--accent/--panel", "accent as text (links, path) on panel", 4.5],
  ["--accent/--bg", "accent on background", 4.5],
  ["--on-accent/--accent", "text on primary button", 4.5],
  ["--on-accent/--accent-hover", "text on primary button (hover)", 4.5],
  ["--accent-text/--accent-soft", "active toggle, menu check mark", 4.5],
  ["--text/--accent-soft", "menu item with focus", 4.5],
  ["--text/--seg-on", "active segment (mode, tab)", 4.5],
  ["--text/--ctl-hover", "hover of quiet buttons", 4.5],
  ["--focus/--panel", "focus ring on panel", 3],
  ["--focus/--view-bg", "focus ring over the 3D view", 3],
  ["--view-accent/--view-bg", "view accent (hover, selection)", 3],
  ["--view-accent/--view-model", "accent on the model", 2],
  ["--text/--vc-face", "view cube label", 4.5],
  ["--axis-x/--view-bg", "x axis at the cube", 3],
];
const tokens = (css, theme) => {
  const m = new RegExp(theme === "light" ? ':root,\\s*:root\\[data-theme="light"\\] \\{([^}]*)\\}' : ':root\\[data-theme="dark"\\] \\{([^}]*)\\}').exec(css);
  const o = {};
  for (const d of (m?.[1] ?? "").split(";")) {
    const i = d.indexOf(":");
    if (i > 0) o[d.slice(0, i).trim()] = d.slice(i + 1).trim();
  }
  return o;
};
const css = readText(join(root, "web/src/styles.css")) ?? "";
const tl = tokens(css, "light");
const td = tokens(css, "dark");
const swatch = (c) => `<span class="sw" style="background:${esc(c)}"></span><code>${esc(c)}</code>`;

const date = new Date().toISOString().slice(0, 10);
const commit = gitOut("rev-parse", "--short", "HEAD");
const branch = gitOut("rev-parse", "--abbrev-ref", "HEAD");

function scen(id) {
  const s = byId[id];
  if (!s) return `<p class="small">(scenario ${esc(id)} missing)</p>`;
  const images = (s.images ?? []).map((i) => fig(pw(i.name, 1200), i.label)).join("");
  return `<h3>${esc(s.id)} ${esc(s.title)}</h3><p>${md(s.caption)}</p><div class="grid">${images}</div><ul class="checks">${(s.checks ?? []).map((c) => `<li>${md(c)}</li>`).join("")}</ul>`;
}

/** Bar names in metrics.json come from web/e2e/ui-overhaul.spec.ts (German, or English if translated). */
const BAR_NAMES = {
  header: /^(Kopfleiste|head(er)? bar)$/i,
  mode: /^(Werkzeugleiste Modus|mode toolbar|toolbar mode)$/i,
  camera: /^(Werkzeugleiste Kamera|camera toolbar|toolbar camera)$/i,
  context: /^(Kontextleiste|context bar)$/i,
};
const findBar = (rep, key) => rep.bars.find((b) => BAR_NAMES[key].test(b.name));

/** Table of the bar check (U01): worst case per width/theme/mode. */
const barTable = (() => {
  const bars = metrics.bars ?? {};
  const rows = Object.entries(bars).map(([k, rep]) => {
    const worst = rep.bars.reduce((a, b) => Math.max(a, b.scroll - b.client), -Infinity);
    const out = rep.bars.reduce((a, b) => a + b.children, 0);
    const hd = findBar(rep, "header");
    const mode = findBar(rep, "mode");
    const cam = findBar(rep, "camera");
    const ctx = findBar(rep, "context");
    return `<tr><td>${esc(k)}</td><td class="n">${worst <= 0 ? '<span class="ok">≤ 0</span>' : `<span class="bad">${worst}</span>`}</td><td class="n">${out}</td><td class="n">${hd?.height ?? "–"}</td><td class="n">${mode && cam ? cam.left - mode.right : "–"}</td><td class="n">${ctx ? `${ctx.right - ctx.left} × ${ctx.height}` : "–"}</td></tr>`;
  });
  return rows.length
    ? `<table><tr><th>State (width, theme, mode)</th><th class="n">max. scrollWidth − clientWidth</th><th class="n">protruding elements</th><th class="n">header bar px</th><th class="n">gap mode–camera px</th><th class="n">context bar px</th></tr>${rows.join("")}</table>`
    : "<p>(missing: metrics.json from U01)</p>";
})();

/** Before: widths of the old head bar (cut off at 1280/1440). */
const oldHeader = Object.entries(shotsBefore)
  .filter(([k]) => /^\d+-/.test(k))
  .map(([k, v]) => {
    const tb = v.find((x) => /toolbar/.test(x.cls));
    return tb ? `<tr><td>${esc(k)}</td><td class="n">${tb.scroll}</td><td class="n">${tb.client}</td><td>${tb.scroll > tb.client ? '<span class="bad">clipped / scrolls</span>' : "fits"}</td></tr>` : "";
  })
  .join("");

const tauriRows = (tauri?.results ?? [])
  .map((r) => `<tr><td>${esc(r.id)}</td><td>${esc(r.title)}</td><td class="${r.ok ? "ok" : "bad"}">${r.ok ? "passed" : "failed"}</td><td class="n">${num(r.ms / 1000, 1)} s</td></tr>`)
  .join("");
const tauriOk = tauri ? tauri.results.filter((r) => r.ok).length : null;

const shortcuts = [
  ["File", "Ctrl+N New (desktop app), Ctrl+O Open, Ctrl+S Save, Ctrl+⇧+S Save as, Ctrl+E export in the last chosen format"],
  ["Menus", "Enter/Space/↓ open, ↑↓ Home End move, → submenu, ← back, Esc closes (focus back to the button), Tab closes"],
  ["Tabs", "← → switch the file, Shift+F10 or context menu key: Set as main file, Close"],
  ["Mode, segmented controls, tabs", "← → (Home/End) select; exactly one value active"],
  ["Sketch", "V select, L line, A arc 3 points, ⇧A arc around center, T tangent, B Bézier, R rectangle, C circle, F round corner, Enter close, Del delete, N ⊥ view, Esc discard points / done"],
  ["Measure", "D distance, R radius, W angle, L length, F area, K body, T wall thickness, Del delete selected measurement, Esc discard selection / exit"],
];

// ------------------------------------------------------------------------------- HTML

const html = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Test report 20 – UI rework</title>
<style>
  @page { size: A4; margin: 14mm 12mm 16mm 12mm; }
  :root { --text: #1f2328; --muted: #5d646e; --border: #d5d9df; --ok: #1a7f37; --bad: #c9302c; --head: #f2f4f7; --accent: #0f766e; }
  * { box-sizing: border-box; }
  body { font: 9.4pt/1.45 system-ui, -apple-system, "Segoe UI", Roboto, sans-serif; color: var(--text); margin: 0; }
  main { max-width: 186mm; margin: 0 auto; }
  h1 { font-size: 18pt; margin: 0 0 2mm; }
  h2 { font-size: 13pt; margin: 7mm 0 3mm; border-bottom: 1px solid var(--border); padding-bottom: 1mm; break-after: avoid; }
  h2.page { break-before: page; }
  h3 { font-size: 10.5pt; margin: 4mm 0 1.5mm; break-after: avoid; }
  p { margin: 1.5mm 0; }
  .meta { color: var(--muted); margin-bottom: 4mm; }
  .summary { display: flex; flex-wrap: wrap; gap: 2.5mm; margin: 3mm 0; }
  .summary div { flex: 1 1 28mm; border: 1px solid var(--border); border-radius: 2mm; padding: 2mm; font-size: 8.2pt; }
  .summary b { display: block; font-size: 12pt; }
  table { width: 100%; border-collapse: collapse; margin: 2mm 0; font-size: 8pt; }
  th, td { border: 1px solid var(--border); padding: 1mm 1.6mm; text-align: left; vertical-align: top; }
  th { background: var(--head); font-weight: 600; }
  td.n, th.n { text-align: right; white-space: nowrap; }
  code, pre { font-family: ui-monospace, "SFMono-Regular", Menlo, Consolas, monospace; font-size: 7.6pt; }
  pre { margin: 0 0 1.5mm; padding: 1.4mm 2mm; background: #f6f8fa; border: 1px solid var(--border); border-radius: 1mm; white-space: pre-wrap; word-break: break-word; }
  .small { font-size: 7.6pt; color: var(--muted); }
  ul { margin: 1mm 0 2mm 5mm; padding: 0; }
  li { margin: 0.5mm 0; }
  ul.checks li { font-size: 8pt; }
  tr, td, figure, pre { break-inside: avoid; }
  .ok { color: var(--ok); font-weight: 600; } .bad { color: var(--bad); font-weight: 600; }
  figure { margin: 0 0 2mm; break-inside: avoid; }
  figure img { max-width: 100%; max-height: 120mm; border: 1px solid var(--border); border-radius: 1mm; background: #fff; display: block; }
  figcaption { font-size: 7.6pt; color: var(--muted); margin-top: 0.6mm; }
  .two { display: grid; grid-template-columns: 1fr 1fr; gap: 3mm; align-items: start; }
  .stack figure img { width: 100%; }
  .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 3mm; align-items: start; }
  .sw { display: inline-block; width: 9px; height: 9px; border-radius: 2px; border: 1px solid #0003; margin-right: 3px; vertical-align: -1px; }
  .note { border-left: 3px solid var(--accent); padding: 1.5mm 3mm; background: #f3faf8; margin: 2mm 0; }
</style>
</head>
<body>
<main>
<h1>Test report 20 – UI rework</h1>
<div class="meta">Implementation of the approved concept <code>docs/concepts/ui-redesign/concept.html</code> (all proposals option A, accent teal instead of blue) · Browser and Tauri · ${esc(date)} · Branch ${esc(branch)} · Commit ${esc(commit)} · ${os.cpus().length} cores</div>

<div class="summary">
  <div>Playwright (full suite)<b class="${pwSum && pwSum.failed === 0 ? "ok" : "bad"}">${pwSum ? `${pwSum.passed} / ${pwSum.passed + pwSum.failed}` : "–"}</b>${pwSum ? `${pwSum.skipped} skipped, ${pwSum.flaky} flaky` : ""}</div>
  <div>new among them U01–U09<b class="${scenarios.length >= 9 ? "ok" : "bad"}">${scenarios.length}</b>bars, menus, tabs, modes, cube, side panel, keyboard, themes</div>
  <div>Tauri E2E<b class="${tauri && tauriOk === tauri.results.length && !tauri.fatal ? "ok" : "bad"}">${tauri ? `${tauriOk} / ${tauri.results.length}` : "–"}</b>incl. T18 (new)</div>
  <div>Vitest<b class="${vitestSum ? "ok" : "bad"}">${vitestSum ?? "–"}</b>incl. contrast light/dark</div>
  <div>tsc<b class="${typecheckOk ? "ok" : "bad"}">${typecheck === null ? "–" : typecheckOk ? "0 errors" : "errors"}</b>Rust cad-app: ${cargoSum ? `${cargoSum.ok} test${cargoSum.ok === 1 ? "" : "s"}` : "–"}, clippy ${clippy === null ? "–" : clippyOk ? "ok" : "findings"}, fmt ${fmt === null ? "–" : fmtOk ? "ok" : "diff"}</div>
</div>

<h2>1. What changed</h2>
<ul>
  <li><b>Building blocks</b> in <code>web/src/app/ui/</code>: <code>Button</code> (primary, standard, quiet, toggle with <code>aria-pressed</code>), <code>IconButton</code>, <code>Segmented</code> (radio group with arrow keys), <code>MenuButton</code>/<code>SplitButton</code>/<code>ContextMenu</code> (WAI-ARIA menus with submenus, rendered into <code>&lt;body&gt;</code>, never clipped), <code>Toolbar</code>/<code>ToolGroup</code>/<code>ToolSep</code>. One height 26 px, radius 6 px, Inter 12 px (the Inter 4.1 file shipped with <code>cad-font</code> – same widths in Chromium and WebKitGTK). Icons from Lucide (<code>lucide-react</code> 1.52, ISC), three custom ones in the same stroke (line, arc, ⊥). Tooltips with keyboard shortcut, <code>aria-label</code> = plain name, <code>aria-keyshortcuts</code>.</li>
  <li><b>Header bar</b>: app name, "File ▾" (New, Open, Open folder, Recently opened ›, Save as, Add file, Load file, Load/save ZIP), "Examples ▾" with submenus, file name centered (change dot in the desktop app, path into the module context as previously in the path bar), "Save" primary, split export button (remembers the format, Ctrl+E), theme and settings icon buttons.</li>
  <li><b>Editor</b>: only tabs and "+"; right click/Shift+F10 on a tab: "Set as main file", "Close".</li>
  <li><b>View</b>: the rows "File …" and "Parts …" are gone. Toolbar at the top: on the left mode Model/Section/Sketch/Measure (exactly one), on the right Fit, View ▾ (Iso, Top, Front, Right, Back, Left, Bottom), All grids, Display ▾ (grid, color regions, side panel). The mode's context bar always at the top left below it. Clickable view cube at the bottom right instead of the axis triad.</li>
  <li><b>Context bars</b>: Section (X/Y/Z/View, position, flip, copy CLI), Sketch (icon groups, Arc ▾, keyboard shortcuts, "Done" primary), Measure (measurement kind as segmented control, "In section", copy, delete, "Done").</li>
  <li><b>Side panel</b> on the right, collapsible, tabs Parts (visibility, volume, export in the ⋯ menu, "without part", total), Colors, Measure, in the module context Parameters.</li>
  <li><b>Accent teal</b>: light <code>${esc(tl["--accent"])}</code>, dark <code>${esc(td["--accent"])}</code>; likewise focus ring, selection/hover of the view (<code>--view-accent</code>, <code>--view-flat</code>, <code>--hover-line</code>, <code>--viewhover-line</code>); the model palette is unchanged.</li>
  <li><b>Arch package/Wayland</b>: the desktop file is named after the app ID (<code>dev.controller-cad.desktop.desktop</code>), the app sets the app ID as GLib program name (Wayland <code>app_id</code>, X11 <code>WM_CLASS</code>).</li>
</ul>

<h2 class="page">2. Header bar</h2>
<p>Before: ten equally weighted text buttons, two select lists and three export buttons; at 1280 and 1440 px the bar extended beyond the window (it scrolled horizontally, "Settings" and the theme were clipped):</p>
${oldHeader ? `<table><tr><th>Before (width, theme)</th><th class="n">scrollWidth</th><th class="n">clientWidth</th><th>Result</th></tr>${oldHeader}</table>` : ""}
${pair(before("kopf-1280-hell", 1300), after("kopf-1280-hell", 1300), "header bar at 1280 px, light", true)}
${pair(before("kopf-1440-dunkel", 1440), after("kopf-1440-dunkel", 1440), "header bar at 1440 px, dark", true)}
${pair(before("kopf-1920-hell", 1920), after("kopf-1920-hell", 1920), "header bar at 1920 px, light", true)}
${scen("U02")}

<h2 class="page">3. Editor</h2>
${pair(before("editor-hell", 800), after("editor-hell", 800), "editor with the example project (three files), light")}
${pair(before("editor-dunkel", 800), after("editor-dunkel", 800), "the same, dark")}
${scen("U03")}

<h2 class="page">4. View and modes</h2>
${pair(before("ansicht-modell-hell", 900), after("ansicht-modell-hell", 900), "model (multicolor.mcad), light")}
${pair(before("ansicht-schnitt-dunkel", 900), after("ansicht-schnitt-dunkel", 900), "section z, dark")}
${pair(before("ansicht-skizze-hell", 900), after("ansicht-skizze-hell", 900), "sketch (sketch_lid.mcad, tool line), light")}
${pair(before("ansicht-messen-hell", 900), after("ansicht-messen-hell", 900), "measure, light")}
${scen("U04")}
${scen("U05")}
${scen("U06")}

<h2 class="page">5. Side panel</h2>
${pair(before("farben-hell", 700), after("farben-hell", 700), "Colors & filaments, light")}
${pair(before("bauteile-dunkel", 1300), after("bauteile-dunkel", 600), "parts (before a row above the view, after a tab), dark")}
${scen("U07")}

<h2 class="page">6. Light and dark</h2>
${pair(before("gesamt-hell"), after("gesamt-hell"), "full window 1440 px, light", true)}
${pair(before("gesamt-dunkel"), after("gesamt-dunkel"), "full window 1440 px, dark", true)}
<h3>Accent and contrast (WCAG 2, <code>contrast.test.ts</code>)</h3>
<p>The token values are checked in <code>web/src/logic/contrast.test.ts</code> (text ≥ 4.5:1, controls ≥ 3:1); newly added are the pairs for buttons, segmented controls, menus, the focus ring and the view cube.</p>
<table><tr><th>Token</th><th>light</th><th>dark</th></tr>
${["--accent", "--accent-hover", "--accent-soft", "--accent-text", "--on-accent", "--focus", "--view-accent", "--view-flat", "--hover-line", "--viewhover-line"].map((k) => `<tr><td><code>${k}</code></td><td>${swatch(tl[k])}</td><td>${swatch(td[k])}</td></tr>`).join("")}
</table>
<table><tr><th>Pair</th><th>Use</th><th class="n">Minimum</th><th class="n">light</th><th class="n">dark</th></tr>
${CONTRAST_ROWS.map(([k, use, min]) => {
  const l = contrast.light[k]?.[0];
  const d = contrast.dark[k]?.[0];
  const c = (v) => (v === undefined ? "–" : `<span class="${v >= min ? "ok" : "bad"}">${num(v, 2)}</span>`);
  return `<tr><td><code>${esc(k)}</code></td><td>${esc(use)}</td><td class="n">${num(min, 1)}</td><td class="n">${c(l)}</td><td class="n">${c(d)}</td></tr>`;
}).join("")}
</table>
${scen("U09")}

<h2 class="page">7. Widths 1280, 1440 and 1920 px</h2>
<p>U01 measures every bar per width, theme and mode: <code>scrollWidth ≤ clientWidth</code> (nothing clipped or scrolled), no control extends beyond its bar or the window, every bar single-line, mode and camera bars do not overlap. For narrower views (small windows, dragged divider) the mode and measurement-kind segmented controls show icons only (container queries).</p>
${barTable}
${scen("U01")}
${scen("U08")}
<h3>Keyboard shortcuts and keyboard</h3>
<table><tr><th>Area</th><th>Keys</th></tr>${shortcuts.map(([a, b]) => `<tr><td>${esc(a)}</td><td>${esc(b)}</td></tr>`).join("")}</table>

<h2 class="page">8. Desktop app (Tauri)</h2>
<p>Same interface, embedded in <code>cad</code> (WebKitGTK under Xvfb, tauri-driver). Before: images of the old state from the same test sequence.</p>
${pair(tauriBefore("t12-farben"), tauriAfter("t12-farben"), "T12 Colors & filaments (light)")}
${pair(tauriBefore("t13-dunkel"), tauriAfter("t13-dunkel"), "T13 dark theme")}
${pair(tauriBefore("t16-rechteck"), tauriAfter("t16-rechteck"), "T16 sketch, rectangle on grid points")}
${pair(tauriBefore("t17-messen"), tauriAfter("t17-messen"), "T17 measure")}
${t18 ? `<h3>T18 ${esc(t18.title)}</h3><ul class="checks">${t18.checks.map((c) => `<li class="${c.ok ? "" : "bad"}">${md(c.text)}</li>`).join("")}</ul><div class="grid">${t18.images.map((i) => fig(tauriAfter(i.name.replace(/\.png$/, "")), i.label)).join("")}</div>` : "<p>(T18 missing)</p>"}
<h3>All Tauri scenarios</h3>
${tauriRows ? `<table><tr><th>No.</th><th>Scenario</th><th>Result</th><th class="n">Duration</th></tr>${tauriRows}</table>` : "<p>(missing: tauri-e2e.json)</p>"}
${tauriBeforeRun ? `<p class="small">Before run (old state): ${tauriBeforeRun.results.filter((r) => r.ok).length} / ${tauriBeforeRun.results.length} passed.</p>` : ""}

<h2>9. Arch package: app ID under Wayland</h2>
<p>Under Wayland the taskbar maps a window to the desktop file of the same name via its <code>app_id</code> (KDE, GNOME). Tauri does not set the app ID itself (<code>enableGTKAppId</code> is off because a registered GApplication would hand a second instance over to the first); GTK therefore took the program name <code>cad</code>, and the desktop file was named <code>controller-cad.desktop</code>. Now:</p>
<ul>
  <li><code>crates/cad-app</code> sets the GLib program name to the <code>identifier</code> from <code>tauri.conf.json</code> (<code>dev.controller-cad.desktop</code>) before GTK starts; GTK uses it as Wayland <code>app_id</code> and as X11 <code>WM_CLASS</code>. Configuration and WebView data stay under the same identifier (no migration).</li>
  <li>The desktop file is named after the app ID: <code>packaging/linux/dev.controller-cad.desktop.desktop</code>, <code>StartupWMClass=dev.controller-cad.desktop</code>; <code>make install</code> derives the name from <code>tauri.conf.json</code>, <code>make uninstall</code> also removes the old name; <code>packaging/arch/smoke-test.sh</code> checks the file and <code>WM_CLASS</code>.</li>
</ul>
${wmclassBefore ? `<p>Before (the old state copied as <code>cad-old</code>: the program name is the file name of the binary):</p>${pre(wmclassBefore)}` : ""}
${wmclass ? `<p>After (<code>tools/report/report20-wmclass.py</code>, libX11 via ctypes under Xvfb):</p>${pre(wmclass)}` : ""}

<h2>10. Test runs</h2>
<table>
<tr><th>Run</th><th>Result</th></tr>
<tr><td>Playwright, full suite (${pwFiles.length} files)</td><td class="${pwSum && pwSum.failed === 0 ? "ok" : "bad"}">${pwSum ? `${pwSum.passed} passed, ${pwSum.failed} failed, ${pwSum.skipped} skipped` : "–"}${pwFailed.length ? `<br>${pwFailed.map(esc).join("<br>")}` : ""}</td></tr>
<tr><td>Tauri E2E (tauri-driver, WebKitWebDriver, Xvfb)</td><td class="${tauri && tauriOk === tauri.results.length ? "ok" : "bad"}">${tauri ? `${tauriOk} / ${tauri.results.length}` : "–"}</td></tr>
<tr><td>Vitest</td><td class="${vitestSum ? "ok" : "bad"}">${vitestSum ? `${vitestSum} passed` : "–"}</td></tr>
<tr><td>tsc --noEmit</td><td class="${typecheckOk ? "ok" : "bad"}">${typecheck === null ? "–" : typecheckOk ? "no errors" : "errors"}</td></tr>
<tr><td>Rust <code>cad-app</code> (program name): cargo test / clippy -D warnings / fmt --check</td><td>${cargoSum ? `${cargoSum.ok} passed, ${cargoSum.failed} failed` : "–"} · clippy ${clippy === null ? "–" : clippyOk ? '<span class="ok">ok</span>' : '<span class="bad">findings</span>'} · fmt ${fmt === null ? "–" : fmtOk ? '<span class="ok">ok</span>' : '<span class="bad">diff</span>'}</td></tr>
</table>
<p>The Playwright tests find controls by role and name (<code>getByRole("radio", { name: "Schnitt" })</code>, menu items via <code>menuitem</code>/<code>menuitemradio</code>, helpers in <code>web/e2e/helpers.ts</code>); test IDs remain only on displays (status line, labels in the view, list rows). The Tauri scenarios select via <code>[role=…][aria-label=…]</code>.</p>

<h2>11. Deviations from the concept and limitations</h2>
<ul>
  <li><b>Renaming</b> a tab does not exist (did not exist before); the context menu has "Set as main file" and "Close".</li>
  <li><b>Parameter bar</b>: the concept does not mention it; it is now the "Parameters" tab of the side panel, which appears and is selected in the module context (before a separate column that narrowed the view further). On leaving the module the panel returns to the previous tab.</li>
  <li><b>Path bar</b>: the path into the module context sits centered in the header bar next to the file name (click jumps back), instead of in the removed row "File …".</li>
  <li><b>X-ray/ghost</b> in the view area: the UI had no such toggle (only the CLI <code>cad render --xray</code>); the other display options are in the "Display" menu (eye icon): All grids, Show color regions (also in the Colors tab), Side panel.</li>
  <li><b>Measure</b> additionally has "Done" (like Sketch); copy/delete act on the selected measurement, otherwise the last one. "In section" turns the section on; after "Done" the view is in Section mode (the section is view state, Measure and Sketch leave it in place).</li>
  <li><b>Default split</b> editor : view 33 : 67 (before 46 : 54; in the concept draft 30 %), so that toolbar and context bars stay single-line at 1280 px with the side panel open and the mode appears with text; the divider can still be dragged, narrow views show mode and measurement kind as icons.</li>
  <li><b>Ctrl+N</b> is intercepted only by the desktop app (Chromium reserves the key for a new window); in the browser no shortcut is shown on the menu item.</li>
  <li><b>View cube</b>: face centers are buttons with names ("View Top"), edges and corners mouse only; via keyboard there is the "View ▾" menu. The cube is an orthographic SVG projection (no perspective), a click jumps without animation.</li>
  <li>The colors of the model palette, the git markers and syntax highlighting are unchanged; <code>--view-profile</code> (profiles, petrol) is close in color to the new accent.</li>
  <li>Wayland itself cannot be checked in the container; what is checked is the program name (X11 <code>WM_CLASS</code> under Xvfb), which GTK also uses as <code>app_id</code>.</li>
</ul>
</main>
</body>
</html>
`;

writeFileSync(join(out, "report.html"), html);
console.log(`${join(out, "report.html")}: ${html.length} characters`);
