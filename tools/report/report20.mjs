#!/usr/bin/env node
// Test report 20 ("UI-Überarbeitung", docs/concepts/ui-redesign): before/after screenshots
// per area (head bar, editor, view and modes, side panel, light/dark, Tauri), the checks of the
// bars at 1280/1440/1920 px, keyboard, contrast of the teal accent, the app id for Wayland, test
// runs, deviations from the concept.
// Writes docs/reports/20-ui-ueberarbeitung/report.html (images in img/).
//
// Inputs (gitignored, reports/tmp/report20/):
//   vorher/*.png, nachher/*.png          node tools/report/report20-shots.mjs --label vorher|nachher --url …
//                                        (vorher: build of the commit before the overhaul)
//   img/*.png, scenarios.json,           cd web && pnpm exec playwright test e2e/ui-ueberarbeitung.spec.ts
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
//   node tools/report/print-pdf.mjs docs/reports/20-ui-ueberarbeitung/report.html docs/reports/20-ui-ueberarbeitung/report.pdf "Testbericht 20: UI-Überarbeitung"

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
const de = (x, d = 0) => (x === null || x === undefined || Number.isNaN(Number(x)) ? "–" : Number(x).toLocaleString("de-DE", { minimumFractionDigits: d, maximumFractionDigits: d }));
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
const fig = (s, cap, cls = "") => (s ? `<figure class="${cls}"><img src="${s}" alt=""><figcaption>${md(cap)}</figcaption></figure>` : `<p class="small">(Bild fehlt: ${esc(cap)})</p>`);
/** Before/after pair, side by side or stacked (wide images). */
const pair = (b, a, cap, stacked = false) => `<div class="${stacked ? "stack" : "two"}">${fig(b, `Vorher: ${cap}`)}${fig(a, `Nachher: ${cap}`)}</div>`;

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
  ["--accent/--panel", "Akzent als Text (Links, Pfad) auf Panel", 4.5],
  ["--accent/--bg", "Akzent auf Hintergrund", 4.5],
  ["--on-accent/--accent", "Text auf Primärknopf", 4.5],
  ["--on-accent/--accent-hover", "Text auf Primärknopf (Hover)", 4.5],
  ["--accent-text/--accent-soft", "aktiver Umschalter, Menü-Häkchen", 4.5],
  ["--text/--accent-soft", "Menüeintrag mit Fokus", 4.5],
  ["--text/--seg-on", "aktives Segment (Modus, Reiter)", 4.5],
  ["--text/--ctl-hover", "Hover leiser Knöpfe", 4.5],
  ["--focus/--panel", "Fokusring auf Panel", 3],
  ["--focus/--view-bg", "Fokusring über der 3D-Ansicht", 3],
  ["--view-accent/--view-bg", "Akzent der Ansicht (Hover, Auswahl)", 3],
  ["--view-accent/--view-model", "Akzent auf dem Modell", 2],
  ["--text/--vc-face", "Beschriftung des Ansichtswürfels", 4.5],
  ["--axis-x/--view-bg", "Achse x am Würfel", 3],
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

const date = new Date().toLocaleDateString("de-DE", { day: "2-digit", month: "2-digit", year: "numeric" });
const commit = gitOut("rev-parse", "--short", "HEAD");
const branch = gitOut("rev-parse", "--abbrev-ref", "HEAD");

function scen(id) {
  const s = byId[id];
  if (!s) return `<p class="small">(Szenario ${esc(id)} fehlt)</p>`;
  const images = (s.images ?? []).map((i) => fig(pw(i.name, 1200), i.label)).join("");
  return `<h3>${esc(s.id)} ${esc(s.title)}</h3><p>${md(s.caption)}</p><div class="grid">${images}</div><ul class="checks">${(s.checks ?? []).map((c) => `<li>${md(c)}</li>`).join("")}</ul>`;
}

/** Table of the bar check (U01): worst case per width/theme/mode. */
const barTable = (() => {
  const bars = metrics.bars ?? {};
  const rows = Object.entries(bars).map(([k, rep]) => {
    const worst = rep.bars.reduce((a, b) => Math.max(a, b.scroll - b.client), -Infinity);
    const out = rep.bars.reduce((a, b) => a + b.children, 0);
    const hd = rep.bars.find((b) => b.name === "Kopfleiste");
    const mode = rep.bars.find((b) => b.name === "Werkzeugleiste Modus");
    const cam = rep.bars.find((b) => b.name === "Werkzeugleiste Kamera");
    const ctx = rep.bars.find((b) => b.name === "Kontextleiste");
    return `<tr><td>${esc(k)}</td><td class="n">${worst <= 0 ? '<span class="ok">≤ 0</span>' : `<span class="bad">${worst}</span>`}</td><td class="n">${out}</td><td class="n">${hd?.height ?? "–"}</td><td class="n">${mode && cam ? cam.left - mode.right : "–"}</td><td class="n">${ctx ? `${ctx.right - ctx.left} × ${ctx.height}` : "–"}</td></tr>`;
  });
  return rows.length
    ? `<table><tr><th>Zustand (Breite, Thema, Modus)</th><th class="n">max. scrollWidth − clientWidth</th><th class="n">herausragende Elemente</th><th class="n">Kopfleiste px</th><th class="n">Abstand Modus–Kamera px</th><th class="n">Kontextleiste px</th></tr>${rows.join("")}</table>`
    : "<p>(fehlt: metrics.json aus U01)</p>";
})();

/** Before: widths of the old head bar (cut off at 1280/1440). */
const oldHeader = Object.entries(shotsBefore)
  .filter(([k]) => /^\d+-/.test(k))
  .map(([k, v]) => {
    const tb = v.find((x) => /toolbar/.test(x.cls));
    return tb ? `<tr><td>${esc(k)}</td><td class="n">${tb.scroll}</td><td class="n">${tb.client}</td><td>${tb.scroll > tb.client ? '<span class="bad">abgeschnitten / scrollt</span>' : "passt"}</td></tr>` : "";
  })
  .join("");

const tauriRows = (tauri?.results ?? [])
  .map((r) => `<tr><td>${esc(r.id)}</td><td>${esc(r.title)}</td><td class="${r.ok ? "ok" : "bad"}">${r.ok ? "bestanden" : "fehlgeschlagen"}</td><td class="n">${de(r.ms / 1000, 1)} s</td></tr>`)
  .join("");
const tauriOk = tauri ? tauri.results.filter((r) => r.ok).length : null;

const shortcuts = [
  ["Datei", "Strg+N Neu (Desktop-App), Strg+O Öffnen, Strg+S Speichern, Strg+⇧+S Speichern unter, Strg+E Export im zuletzt gewählten Format"],
  ["Menüs", "Enter/Leertaste/↓ öffnen, ↑↓ Pos1 Ende bewegen, → Untermenü, ← zurück, Esc schließt (Fokus zurück an den Knopf), Tab schließt"],
  ["Tabs", "← → wechseln die Datei, Umschalt+F10 oder Kontextmenütaste: Als Hauptdatei, Schließen"],
  ["Modus, Segmente, Reiter", "← → (Pos1/Ende) wählen; genau ein Wert aktiv"],
  ["Skizze", "V Auswahl, L Linie, A Bogen 3 Punkte, ⇧A Bogen um Mittelpunkt, T tangential, B Bézier, R Rechteck, C Kreis, F Ecke runden, Enter Schließen, Entf Löschen, N ⊥ Ansicht, Esc Punkte verwerfen / Fertig"],
  ["Messen", "D Abstand, R Radius, W Winkel, L Länge, F Fläche, K Körper, T Wandstärke, Entf gewählte Messung löschen, Esc Auswahl verwerfen / beenden"],
];

// ------------------------------------------------------------------------------- HTML

const html = `<!doctype html>
<html lang="de">
<head>
<meta charset="utf-8">
<title>Testbericht 20 – UI-Überarbeitung</title>
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
<h1>Testbericht 20 – UI-Überarbeitung</h1>
<div class="meta">Umsetzung des beschlossenen Konzepts <code>docs/concepts/ui-redesign/concept.html</code> (alle Vorschläge Option A, Akzent Teal statt Blau) · Browser und Tauri · ${esc(date)} · Branch ${esc(branch)} · Commit ${esc(commit)} · ${os.cpus().length} Kerne</div>

<div class="summary">
  <div>Playwright (ganze Suite)<b class="${pwSum && pwSum.failed === 0 ? "ok" : "bad"}">${pwSum ? `${pwSum.passed} / ${pwSum.passed + pwSum.failed}` : "–"}</b>${pwSum ? `${pwSum.skipped} übersprungen, ${pwSum.flaky} flaky` : ""}</div>
  <div>davon neu U01–U09<b class="${scenarios.length >= 9 ? "ok" : "bad"}">${scenarios.length}</b>Leisten, Menüs, Tabs, Modi, Würfel, Seitenpanel, Tastatur, Themen</div>
  <div>Tauri-E2E<b class="${tauri && tauriOk === tauri.results.length && !tauri.fatal ? "ok" : "bad"}">${tauri ? `${tauriOk} / ${tauri.results.length}` : "–"}</b>inkl. T18 (neu)</div>
  <div>Vitest<b class="${vitestSum ? "ok" : "bad"}">${vitestSum ?? "–"}</b>inkl. Kontrast hell/dunkel</div>
  <div>tsc<b class="${typecheckOk ? "ok" : "bad"}">${typecheck === null ? "–" : typecheckOk ? "0 Fehler" : "Fehler"}</b>Rust cad-app: ${cargoSum ? `${cargoSum.ok} Tests` : "–"}, clippy ${clippy === null ? "–" : clippyOk ? "ok" : "Befunde"}, fmt ${fmt === null ? "–" : fmtOk ? "ok" : "Abweichung"}</div>
</div>

<h2>1. Was sich geändert hat</h2>
<ul>
  <li><b>Bausteine</b> in <code>web/src/app/ui/</code>: <code>Button</code> (primär, standard, leise, Umschalter mit <code>aria-pressed</code>), <code>IconButton</code>, <code>Segmented</code> (Radiogruppe mit Pfeiltasten), <code>MenuButton</code>/<code>SplitButton</code>/<code>ContextMenu</code> (WAI-ARIA-Menüs mit Untermenüs, in <code>&lt;body&gt;</code> gerendert, nie abgeschnitten), <code>Toolbar</code>/<code>ToolGroup</code>/<code>ToolSep</code>. Eine Höhe 26 px, Radius 6 px, Inter 12 px (die Inter-4.1-Datei, die <code>cad-font</code> mitbringt – gleiche Breiten in Chromium und WebKitGTK). Symbole aus Lucide (<code>lucide-react</code> 1.52, ISC), drei eigene im selben Strich (Linie, Bogen, ⊥). Tooltips mit Tastenkürzel, <code>aria-label</code> = schlichter Name, <code>aria-keyshortcuts</code>.</li>
  <li><b>Kopfleiste</b>: App-Name, „Datei ▾“ (Neu, Öffnen, Ordner öffnen, Zuletzt geöffnet ›, Speichern unter, Datei hinzufügen, Datei laden, ZIP laden/speichern), „Beispiele ▾“ mit Untermenüs, Dateiname mittig (Änderungspunkt in der Desktop-App, Pfad in den Modulkontext wie früher die Pfadleiste), „Speichern“ primär, geteilter Export-Knopf (merkt das Format, Strg+E), Thema- und Einstellungen-Symbolknopf.</li>
  <li><b>Editor</b>: nur Tabs und „+“; Rechtsklick/Umschalt+F10 auf einem Tab: „Als Hauptdatei“, „Schließen“.</li>
  <li><b>Ansicht</b>: die Zeilen „Datei …“ und „Bauteile …“ entfallen. Werkzeugleiste oben: links Modus Modell/Schnitt/Skizze/Messen (genau einer), rechts Einpassen, Ansicht ▾ (Iso, Oben, Vorne, Rechts, Hinten, Links, Unten), Alle Raster, Anzeige ▾ (Raster, Farbregionen, Seitenpanel). Kontextleiste des Modus immer links oben darunter. Klickbarer Ansichtswürfel unten rechts statt Achsenkreuz.</li>
  <li><b>Kontextleisten</b>: Schnitt (X/Y/Z/Blick, Lage, Umkehren, CLI kopieren), Skizze (Symbolgruppen, Bogen ▾, Tastenkürzel, „Fertig“ primär), Messen (Messart als Segment, „Im Schnitt“, Kopieren, Löschen, „Fertig“).</li>
  <li><b>Seitenpanel</b> rechts, einklappbar, Reiter Bauteile (Sichtbarkeit, Volumen, Export im ⋯-Menü, „ohne Bauteil“, Summe), Farben, Messen, im Modulkontext Parameter.</li>
  <li><b>Akzent Teal</b>: hell <code>${esc(tl["--accent"])}</code>, dunkel <code>${esc(td["--accent"])}</code>; ebenso Fokusring, Auswahl/Hover der Ansicht (<code>--view-accent</code>, <code>--view-flat</code>, <code>--hover-line</code>, <code>--viewhover-line</code>); die Modellpalette bleibt.</li>
  <li><b>Arch-Paket/Wayland</b>: Desktop-Datei heißt nach der App-ID (<code>dev.controller-cad.desktop.desktop</code>), die App setzt die App-ID als GLib-Programmnamen (Wayland-<code>app_id</code>, X11-<code>WM_CLASS</code>).</li>
</ul>

<h2 class="page">2. Kopfleiste</h2>
<p>Vorher zehn gleich gewichtete Textknöpfe, zwei Auswahllisten und drei Export-Knöpfe; bei 1280 und 1440 px ragte die Leiste aus dem Fenster (sie scrollte waagerecht, „Einstellungen“ und das Thema waren abgeschnitten):</p>
${oldHeader ? `<table><tr><th>Vorher (Breite, Thema)</th><th class="n">scrollWidth</th><th class="n">clientWidth</th><th>Ergebnis</th></tr>${oldHeader}</table>` : ""}
${pair(before("kopf-1280-hell", 1300), after("kopf-1280-hell", 1300), "Kopfleiste bei 1280 px, hell", true)}
${pair(before("kopf-1440-dunkel", 1440), after("kopf-1440-dunkel", 1440), "Kopfleiste bei 1440 px, dunkel", true)}
${pair(before("kopf-1920-hell", 1920), after("kopf-1920-hell", 1920), "Kopfleiste bei 1920 px, hell", true)}
${scen("U02")}

<h2 class="page">3. Editor</h2>
${pair(before("editor-hell", 800), after("editor-hell", 800), "Editor mit dem Beispielprojekt (drei Dateien), hell")}
${pair(before("editor-dunkel", 800), after("editor-dunkel", 800), "dasselbe dunkel")}
${scen("U03")}

<h2 class="page">4. Ansicht und Modi</h2>
${pair(before("ansicht-modell-hell", 900), after("ansicht-modell-hell", 900), "Modell (mehrfarbig.scad), hell")}
${pair(before("ansicht-schnitt-dunkel", 900), after("ansicht-schnitt-dunkel", 900), "Schnitt z, dunkel")}
${pair(before("ansicht-skizze-hell", 900), after("ansicht-skizze-hell", 900), "Skizze (skizze_deckel.scad, Werkzeug Linie), hell")}
${pair(before("ansicht-messen-hell", 900), after("ansicht-messen-hell", 900), "Messen, hell")}
${scen("U04")}
${scen("U05")}
${scen("U06")}

<h2 class="page">5. Seitenpanel</h2>
${pair(before("farben-hell", 700), after("farben-hell", 700), "Farben & Filamente, hell")}
${pair(before("bauteile-dunkel", 1300), after("bauteile-dunkel", 600), "Bauteile (vorher Zeile über der Ansicht, nachher Reiter), dunkel")}
${scen("U07")}

<h2 class="page">6. Hell und dunkel</h2>
${pair(before("gesamt-hell"), after("gesamt-hell"), "Gesamtfenster 1440 px, hell", true)}
${pair(before("gesamt-dunkel"), after("gesamt-dunkel"), "Gesamtfenster 1440 px, dunkel", true)}
<h3>Akzent und Kontrast (WCAG 2, <code>contrast.test.ts</code>)</h3>
<p>Die Werte der Tokens werden in <code>web/src/logic/contrast.test.ts</code> geprüft (Text ≥ 4,5:1, Bedienelemente ≥ 3:1); neu aufgenommen sind die Paare der Knöpfe, Segmente, Menüs, des Fokusrings und des Ansichtswürfels.</p>
<table><tr><th>Token</th><th>hell</th><th>dunkel</th></tr>
${["--accent", "--accent-hover", "--accent-soft", "--accent-text", "--on-accent", "--focus", "--view-accent", "--view-flat", "--hover-line", "--viewhover-line"].map((k) => `<tr><td><code>${k}</code></td><td>${swatch(tl[k])}</td><td>${swatch(td[k])}</td></tr>`).join("")}
</table>
<table><tr><th>Paar</th><th>Verwendung</th><th class="n">Minimum</th><th class="n">hell</th><th class="n">dunkel</th></tr>
${CONTRAST_ROWS.map(([k, use, min]) => {
  const l = contrast.light[k]?.[0];
  const d = contrast.dark[k]?.[0];
  const c = (v) => (v === undefined ? "–" : `<span class="${v >= min ? "ok" : "bad"}">${de(v, 2)}</span>`);
  return `<tr><td><code>${esc(k)}</code></td><td>${esc(use)}</td><td class="n">${de(min, 1)}</td><td class="n">${c(l)}</td><td class="n">${c(d)}</td></tr>`;
}).join("")}
</table>
${scen("U09")}

<h2 class="page">7. Breiten 1280, 1440 und 1920 px</h2>
<p>U01 misst je Breite, Thema und Modus jede Leiste: <code>scrollWidth ≤ clientWidth</code> (nichts abgeschnitten oder gescrollt), kein Bedienelement ragt aus seiner Leiste oder dem Fenster, jede Leiste einzeilig, Modus- und Kameraleiste überlappen nicht. Bei schmaleren Ansichten (kleine Fenster, gezogener Teiler) zeigen Modus- und Messart-Segmente nur Symbole (Container-Queries).</p>
${barTable}
${scen("U01")}
${scen("U08")}
<h3>Tastenkürzel und Tastatur</h3>
<table><tr><th>Bereich</th><th>Tasten</th></tr>${shortcuts.map(([a, b]) => `<tr><td>${esc(a)}</td><td>${esc(b)}</td></tr>`).join("")}</table>

<h2 class="page">8. Desktop-App (Tauri)</h2>
<p>Gleiche Oberfläche, eingebettet in <code>cad</code> (WebKitGTK unter Xvfb, tauri-driver). Vorher: Bilder des alten Stands aus derselben Testfolge.</p>
${pair(tauriBefore("t12-farben"), tauriAfter("t12-farben"), "T12 Farben & Filamente (hell)")}
${pair(tauriBefore("t13-dunkel"), tauriAfter("t13-dunkel"), "T13 dunkles Thema")}
${pair(tauriBefore("t16-rechteck"), tauriAfter("t16-rechteck"), "T16 Skizze, Rechteck auf Rasterpunkten")}
${pair(tauriBefore("t17-messen"), tauriAfter("t17-messen"), "T17 Messen")}
${t18 ? `<h3>T18 ${esc(t18.title)}</h3><ul class="checks">${t18.checks.map((c) => `<li class="${c.ok ? "" : "bad"}">${md(c.text)}</li>`).join("")}</ul><div class="grid">${t18.images.map((i) => fig(tauriAfter(i.name.replace(/\.png$/, "")), i.label)).join("")}</div>` : "<p>(T18 fehlt)</p>"}
<h3>Alle Tauri-Szenarien</h3>
${tauriRows ? `<table><tr><th>Nr.</th><th>Szenario</th><th>Ergebnis</th><th class="n">Dauer</th></tr>${tauriRows}</table>` : "<p>(fehlt: tauri-e2e.json)</p>"}
${tauriBeforeRun ? `<p class="small">Vorher-Lauf (alter Stand): ${tauriBeforeRun.results.filter((r) => r.ok).length} / ${tauriBeforeRun.results.length} bestanden.</p>` : ""}

<h2>9. Arch-Paket: App-ID unter Wayland</h2>
<p>Unter Wayland ordnet die Taskleiste ein Fenster über seine <code>app_id</code> der Desktop-Datei gleichen Namens zu (KDE, GNOME). Tauri setzt die App-ID nicht selbst (<code>enableGTKAppId</code> ist aus, weil eine registrierte GApplication eine zweite Instanz an die erste weiterreichen würde); GTK nahm deshalb den Programmnamen <code>cad</code>, die Desktop-Datei hieß <code>controller-cad.desktop</code>. Jetzt:</p>
<ul>
  <li><code>crates/cad-app</code> setzt vor dem Start von GTK den GLib-Programmnamen auf den <code>identifier</code> aus <code>tauri.conf.json</code> (<code>dev.controller-cad.desktop</code>); GTK übernimmt ihn als Wayland-<code>app_id</code> und als X11-<code>WM_CLASS</code>. Konfigurations- und WebView-Daten bleiben unter demselben Bezeichner (keine Migration).</li>
  <li>Die Desktop-Datei heißt nach der App-ID: <code>packaging/linux/dev.controller-cad.desktop.desktop</code>, <code>StartupWMClass=dev.controller-cad.desktop</code>; <code>make install</code> leitet den Namen aus <code>tauri.conf.json</code> ab, <code>make uninstall</code> entfernt auch den alten Namen; <code>packaging/arch/smoke-test.sh</code> prüft Datei und <code>WM_CLASS</code>.</li>
</ul>
${wmclassBefore ? `<p>Vorher (der alte Stand als <code>cad-old</code> kopiert: der Programmname ist der Dateiname des Binarys):</p>${pre(wmclassBefore)}` : ""}
${wmclass ? `<p>Nachher (<code>tools/report/report20-wmclass.py</code>, libX11 über ctypes unter Xvfb):</p>${pre(wmclass)}` : ""}

<h2>10. Testläufe</h2>
<table>
<tr><th>Lauf</th><th>Ergebnis</th></tr>
<tr><td>Playwright, ganze Suite (${pwFiles.length} Dateien)</td><td class="${pwSum && pwSum.failed === 0 ? "ok" : "bad"}">${pwSum ? `${pwSum.passed} bestanden, ${pwSum.failed} fehlgeschlagen, ${pwSum.skipped} übersprungen` : "–"}${pwFailed.length ? `<br>${pwFailed.map(esc).join("<br>")}` : ""}</td></tr>
<tr><td>Tauri-E2E (tauri-driver, WebKitWebDriver, Xvfb)</td><td class="${tauri && tauriOk === tauri.results.length ? "ok" : "bad"}">${tauri ? `${tauriOk} / ${tauri.results.length}` : "–"}</td></tr>
<tr><td>Vitest</td><td class="${vitestSum ? "ok" : "bad"}">${vitestSum ? `${vitestSum} bestanden` : "–"}</td></tr>
<tr><td>tsc --noEmit</td><td class="${typecheckOk ? "ok" : "bad"}">${typecheck === null ? "–" : typecheckOk ? "ohne Fehler" : "Fehler"}</td></tr>
<tr><td>Rust <code>cad-app</code> (Programmname): cargo test / clippy -D warnings / fmt --check</td><td>${cargoSum ? `${cargoSum.ok} bestanden, ${cargoSum.failed} fehlgeschlagen` : "–"} · clippy ${clippy === null ? "–" : clippyOk ? '<span class="ok">ok</span>' : '<span class="bad">Befunde</span>'} · fmt ${fmt === null ? "–" : fmtOk ? '<span class="ok">ok</span>' : '<span class="bad">Abweichung</span>'}</td></tr>
</table>
<p>Die Playwright-Tests finden Bedienelemente über Rolle und Namen (<code>getByRole("radio", { name: "Schnitt" })</code>, Menüeinträge über <code>menuitem</code>/<code>menuitemradio</code>, Helfer in <code>web/e2e/helpers.ts</code>); Test-IDs bleiben nur an Anzeigen (Statuszeile, Beschriftungen in der Ansicht, Zeilen der Listen). Die Tauri-Szenarien wählen über <code>[role=…][aria-label=…]</code>.</p>

<h2>11. Abweichungen vom Konzept und Grenzen</h2>
<ul>
  <li><b>Umbenennen</b> eines Tabs gibt es nicht (gab es vorher nicht); das Kontextmenü hat „Als Hauptdatei“ und „Schließen“.</li>
  <li><b>Parameterleiste</b>: das Konzept nennt sie nicht; sie ist jetzt der Reiter „Parameter“ des Seitenpanels, der im Modulkontext erscheint und gewählt wird (vorher eine eigene Spalte, die die Ansicht zusätzlich schmälerte). Beim Verlassen des Moduls kehrt das Panel zum vorigen Reiter zurück.</li>
  <li><b>Pfadleiste</b>: der Pfad in den Modulkontext steht mittig in der Kopfleiste beim Dateinamen (Klick springt zurück), statt in der entfallenen Zeile „Datei …“.</li>
  <li><b>Röntgen/Ghost</b> im Ansicht-Bereich: die UI hatte keinen solchen Schalter (nur die CLI <code>cad render --xray</code>); die weiteren Anzeigeoptionen stehen im Menü „Anzeige“ (Symbol Auge): Alle Raster, Farbregionen zeigen (auch im Reiter Farben), Seitenpanel.</li>
  <li><b>Messen</b> hat zusätzlich „Fertig“ (wie Skizze); Kopieren/Löschen wirken auf die gewählte, sonst die letzte Messung. „Im Schnitt“ schaltet den Schnitt ein; nach „Fertig“ ist die Ansicht im Modus Schnitt (der Schnitt ist Ansichtszustand, Messen und Skizze lassen ihn stehen).</li>
  <li><b>Standardaufteilung</b> Editor : Ansicht 33 : 67 (vorher 46 : 54; im Konzept-Entwurf 30 %), damit Werkzeug- und Kontextleisten bei 1280 px mit offenem Seitenpanel einzeilig bleiben und der Modus mit Text erscheint; der Teiler ist weiter ziehbar, schmale Ansichten zeigen Modus und Messart als Symbole.</li>
  <li><b>Strg+N</b> fängt nur die Desktop-App ab (Chromium reserviert die Taste für ein neues Fenster); im Browser steht kein Kürzel am Menüeintrag.</li>
  <li><b>Ansichtswürfel</b>: Flächenmitten sind Knöpfe mit Namen („Ansicht Oben“), Kanten und Ecken nur mit der Maus; per Tastatur gibt es das Menü „Ansicht ▾“. Der Würfel ist eine orthografische SVG-Projektion (keine Perspektive), Klick springt ohne Animation.</li>
  <li>Die Farben der Modellpalette, der Git-Markierungen und der Syntaxhervorhebung sind unverändert; <code>--view-profile</code> (Profile, petrol) liegt farblich nahe am neuen Akzent.</li>
  <li>Wayland selbst ist im Container nicht prüfbar; geprüft ist der Programmname (X11-<code>WM_CLASS</code> unter Xvfb), den GTK auch als <code>app_id</code> verwendet.</li>
</ul>
</main>
</body>
</html>
`;

writeFileSync(join(out, "report.html"), html);
console.log(`${join(out, "report.html")}: ${html.length} Zeichen`);
