#!/usr/bin/env node
// Test report 2 ("UI Phase A"): collects the results of the UI tests and writes
//
//   docs/reports/02-ui-phase-a/report.html, report.pdf, img/*.jpg
//
// Inputs (written by the test runs, gitignored under reports/tmp/report2/):
//   playwright.json   Playwright JSON reporter (web/playwright.config.ts)
//   vitest.json       Vitest JSON reporter (pnpm test --reporter=json --outputFile=…)
//   scenarios.json    title/caption/screenshot per e2e scenario (web/e2e/helpers.ts)
//   metrics.json      load time, edit → scene, fps (web/e2e/ui.spec.ts)
//   img/*.jpg         screenshots
//
//   cd web && pnpm test:report      (runs Vitest, Playwright, then this script)
//   node tools/report/report2-ui.mjs
//
// The PDF is printed with Chromium (page.pdf, A4, backgrounds on).

import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import os from "node:os";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "../..");
const web = join(root, "web");
const input = join(root, "reports/tmp/report2");
const out = join(root, "docs/reports/02-ui-phase-a");
const require = createRequire(join(web, "package.json"));

const readJson = (f, d = null) => (existsSync(f) ? JSON.parse(readFileSync(f, "utf8")) : d);
const esc = (s) => String(s ?? "").replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
const fmtMs = (v) => (typeof v === "number" ? `${Math.round(v)} ms` : "–");
const median = (a) => (a && a.length ? [...a].sort((x, y) => x - y)[Math.floor(a.length / 2)] : null);
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

const pw = readJson(join(input, "playwright.json"));
const vt = readJson(join(input, "vitest.json"));
const scenarios = readJson(join(input, "scenarios.json"), []);
const metrics = readJson(join(input, "metrics.json"), {});
if (!pw) {
  console.error(`fehlt: ${join(input, "playwright.json")} (zuerst: cd web && pnpm e2e)`);
  process.exit(1);
}

/** Playwright tests flattened: { id, title, status, ms }. */
function flatten(suite, acc = []) {
  // Only the Phase A scenarios (web/e2e/ui.spec.ts); Phase B has its own report.
  if (suite.file && !suite.file.endsWith("ui.spec.ts")) return acc;
  for (const s of suite.suites ?? []) flatten(s, acc);
  for (const spec of suite.specs ?? []) {
    for (const t of spec.tests ?? []) {
      const r = t.results?.at(-1);
      const id = spec.title.split(" ")[0];
      acc.push({ id, title: spec.title.slice(id.length + 1), status: r?.status ?? t.status ?? "unbekannt", ms: r?.duration ?? 0, error: r?.error?.message ?? null });
    }
  }
  return acc;
}
const e2e = flatten(pw);
const e2ePassed = e2e.filter((t) => t.status === "passed").length;

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

// ---------------------------------------------------------------- images

rmSync(join(out, "img"), { recursive: true, force: true });
mkdirSync(join(out, "img"), { recursive: true });
const images = [];
for (const s of scenarios) {
  if (!s.image) continue;
  const src = join(input, "img", `${s.image}.jpg`);
  if (!existsSync(src)) continue;
  copyFileSync(src, join(out, "img", `${s.image}.jpg`));
  images.push(s);
}
// Further screenshots of the scenarios (a, b, c variants) that are not the main image.
const extra = existsSync(join(input, "img"))
  ? readdirSync(join(input, "img"))
      .filter((f) => f.endsWith(".jpg") && !images.some((s) => `${s.image}.jpg` === f))
      .sort()
  : [];
for (const f of extra) copyFileSync(join(input, "img", f), join(out, "img", f));
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
  ["Betriebssystem", `${os.type()} ${os.release()} (${os.arch()})`],
  ["CPU", `${os.cpus()[0]?.model ?? "–"} · ${os.cpus().length} Kerne`],
  ["Node.js", process.version],
  ["Browser", `Chromium ${chromiumVersion} (headless, Playwright ${pkgVersion("@playwright/test")})`],
  ["Grafik", "WebGL über SwiftShader (Software-Rendering, keine GPU)"],
  ["Fenster", "1440 × 900 px, Gerätepixelverhältnis 1"],
  ["Build", `Vite ${pkgVersion("vite")} Produktions-Build, Vorschau-Server (vite preview)`],
  ["Bibliotheken", `React ${pkgVersion("react")}, three.js ${pkgVersion("three")}, CodeMirror ${pkgVersion("@codemirror/view")}, Zustand ${pkgVersion("zustand")}, Vitest ${pkgVersion("vitest")}`],
  ["Engine", `cad_wasm_bg.wasm ${mib(asset(/^cad_wasm_bg-.*\.wasm$/))}, smlib.wasm ${mib(asset(/^smlib-.*\.wasm$/))} (je einmal im Build)`],
  ["Quelltext", `Branch ${git("rev-parse", "--abbrev-ref", "HEAD")}, Commit ${git("rev-parse", "--short", "HEAD")}`],
];

// ---------------------------------------------------------------- html

const date = new Date().toLocaleDateString("de-DE", { year: "numeric", month: "long", day: "numeric" });
const caption = (id) => scenarios.find((s) => s.id === id);

const statusLabel = (s) => (s === "passed" ? "bestanden" : s === "skipped" ? "übersprungen" : "fehlgeschlagen");
const scenarioRows = e2e
  .map((t) => {
    const c = caption(t.id);
    return `<tr class="${t.status === "passed" ? "ok" : "bad"}"><td>${esc(t.id)}</td><td>${esc(t.title)}</td><td class="res">${statusLabel(t.status)}</td><td class="num">${(t.ms / 1000).toFixed(1)} s</td><td>${esc(c?.caption ?? "")}${t.error ? `<div class="err">${esc(t.error.split("\n")[0])}</div>` : ""}</td></tr>`;
  })
  .join("\n");

const unitRows = unit
  .map((f) => `<tr class="${f.failed.length ? "bad" : "ok"}"><td>${esc(f.file)}</td><td class="num">${f.passed} / ${f.total}</td><td>${f.failed.map(esc).join("<br>") || "–"}</td></tr>`)
  .join("\n");

const m = metrics;
const metricRows = [
  ["Ladezeit bis zur ersten Szene", fmtMs(m.firstSceneMs), "Navigation bis zum ersten Zeichnen der vollständigen Szene von gehaeuse.scad (Wasm laden, Auswertung, Tessellierung, Rendern)"],
  ["Erste Auswertung", fmtMs(m.firstEvalMs), "SetSource bis vollständige Szene (kalter Cache)"],
  ["Tastendruck → neue Szene (Median)", fmtMs(m.editToSceneMedianMs), `App-Messung: letzte Änderung bis zum Zeichnen der neuen Szene; Einzelwerte ${(m.editToSceneMs ?? []).map((v) => Math.round(v)).join(", ")} ms`],
  ["Tastendruck → neue Szene, Testsicht (Median)", fmtMs(m.keyToSceneE2eMedianMs), "Playwright: Tastendruck bis die Szene im Testhaken sichtbar ist (inkl. Abfrageintervall 50 ms)"],
  ["Auswertung (Median)", fmtMs(m.evalMsMedian), "SetSource bis vollständige Szene während der Änderungen (warmer Cache)"],
  ["Bildrate beim Drehen", typeof m.orbitFps === "number" ? `${m.orbitFps.toFixed(0)} fps` : "–", "120 Bilder Kameradrehung um die z-Achse, je Bild ein vollständiges Rendern (SwiftShader)"],
  ["Größte Bilddatei im Bericht", kib(maxImg), "JPEG, Qualität 82"],
]
  .map(([a, b, c]) => `<tr><td>${esc(a)}</td><td class="num">${esc(b)}</td><td>${esc(c)}</td></tr>`)
  .join("\n");

const figure = (s) => `<figure><img src="img/${esc(s.image)}.jpg" alt="${esc(s.title)}"><figcaption><b>${esc(s.id)} ${esc(s.title)}.</b> ${esc(s.caption)}</figcaption></figure>`;
const extraCaptions = {
  "s03a-cursor-deckel": "Cursor auf deckel() in Zeile 22: Modulaufruf voll, Rest gedimmt.",
  "s07b-doppelklick": "Nach Doppelklick auf das Gehäuse: Cursor in module gehaeuse, Ansicht im Modulkontext.",
  "s08b-datei": "Nach Klick auf „Datei“ in der Pfadleiste: Cursor auf k: gehaeuse(w = 60), ganze Datei.",
  "s09b-root-modifier": "25_root_modifier.scad: nur der !-Teilbaum wird gezeigt, Hinweis in der Statuszeile.",
  "s13a-raster-zeile": "Raster nur mit den Ebenen der Zeile 12 (box zwischen Rasterpunkten).",
  "s13c-raster-hover": "Hover über g[loch[i], mitte, boden]: Ausdruck markiert, Rasterpunkt hervorgehoben (bei Schleifen der Punkt des ersten Durchlaufs).",
  "s20b-profil": "27_spacers_2d.scad, Ansicht von oben: Cursor auf square([30, 10]) – 2D-Form als Umriss mit leichter Füllung, Rest gedimmt.",
};
const extraFigure = (n) => `<figure><img src="img/${esc(n)}.jpg" alt="${esc(n)}"><figcaption>${esc(extraCaptions[n] ?? n)}</figcaption></figure>`;
/** Main screenshot of each scenario followed by its further screenshots (same sNN prefix). */
const figures = images
  .map((s) => {
    const prefix = s.image.match(/^s\d+/)?.[0];
    const more = extra.map((f) => f.replace(/\.jpg$/, "")).filter((n) => n.match(/^s\d+/)?.[0] === prefix);
    return [figure(s), ...more.map(extraFigure)].join("\n");
  })
  .join("\n");

const html = `<!doctype html>
<html lang="de">
<head>
<meta charset="utf-8">
<title>Testbericht 2 – UI Phase A</title>
<style>
  @page { size: A4; margin: 16mm 14mm 16mm 14mm; }
  :root { --text: #1f2328; --muted: #5d646e; --border: #d5d9df; --ok: #1a7f37; --bad: #c9302c; --head: #f2f4f7; }
  * { box-sizing: border-box; }
  body { font: 10pt/1.45 system-ui, -apple-system, "Segoe UI", Roboto, sans-serif; color: var(--text); margin: 0; }
  main { max-width: 182mm; margin: 0 auto; }
  h1 { font-size: 20pt; margin: 0 0 2mm; }
  h2 { font-size: 13pt; margin: 8mm 0 3mm; border-bottom: 1px solid var(--border); padding-bottom: 1mm; }
  .meta { color: var(--muted); margin-bottom: 5mm; }
  .summary { display: flex; gap: 4mm; margin: 4mm 0; }
  .summary div { flex: 1; border: 1px solid var(--border); border-radius: 2mm; padding: 3mm; }
  .summary b { display: block; font-size: 15pt; }
  table { width: 100%; border-collapse: collapse; margin: 2mm 0; font-size: 8.8pt; }
  th, td { border: 1px solid var(--border); padding: 1.4mm 2mm; text-align: left; vertical-align: top; }
  th { background: var(--head); font-weight: 600; }
  td.num { text-align: right; white-space: nowrap; }
  tr.ok td.res { color: var(--ok); font-weight: 600; }
  tr.bad td.res, tr.bad td:first-child { color: var(--bad); font-weight: 600; }
  .err { color: var(--bad); font-size: 8pt; margin-top: 1mm; }
  ul { margin: 1mm 0 2mm 5mm; padding: 0; }
  li { margin: 0.6mm 0; }
  figure { margin: 0 0 6mm; break-inside: avoid; page-break-inside: avoid; }
  figure img { width: 100%; border: 1px solid var(--border); display: block; }
  figcaption { font-size: 8.8pt; color: var(--muted); margin-top: 1.5mm; }
  figure img { max-height: 112mm; object-fit: contain; }
  .small { font-size: 8.8pt; color: var(--muted); }
  tr, td { break-inside: avoid; page-break-inside: avoid; }
</style>
</head>
<body>
<main>
<h1>Testbericht 2 – UI Phase A</h1>
<div class="meta">Browser-UI des parametrischen CAD (TypeScript/React, CodeMirror 6, three.js) · ${esc(date)} · Commit ${esc(git("rev-parse", "--short", "HEAD"))}</div>

<div class="summary">
  <div>End-to-End (Playwright)<b>${e2ePassed} / ${e2e.length} bestanden</b></div>
  <div>Unit-Tests (Vitest)<b>${vt ? `${unitPassed} / ${unitTotal} bestanden` : "nicht gelaufen"}</b></div>
  <div>Erste Szene<b>${fmtMs(m.firstSceneMs)}</b></div>
  <div>Tastendruck → Szene<b>${fmtMs(m.editToSceneMedianMs)}</b></div>
</div>

<h2>1 Umfang</h2>
<p>Geprüft wird die Browser-Oberfläche nach Phase A: links der Editor, rechts die 3D-Ansicht mit Pfadleiste, unten die Statuszeile. Die Engine (Rust, Wasm, SMLib-Kernel) läuft in einem Web Worker; die UI wertet nur die Szenenbeschreibung aus und lädt Meshes pro Knoten nach.</p>
<ul>
  <li><b>Editor:</b> Lezer-Grammatik (OpenSCAD mit <code>grid</code>, <code>anchor</code>, <code>k:</code>), Diagnosen (rot unterstrichen, Zeile rot bzw. gelb, Meldung am Zeilenende), Inline-Hinweise (Werte, „4 Kanten“), <code>*</code>-Zeilen ausgegraut, Positionen alter Szenen über <code>ChangeSet.mapPos</code>.</li>
  <li><b>3D-Ansicht:</b> Meshes pro Knoten mit Cache nach <code>mesh_key</code>, Platzierung über <code>display_world</code>, Kanten als Linien, Drehen/Verschieben/Zoomen, „Alles zeigen“, Standardansichten, Achsenkreuz, Boden.</li>
  <li><b>Code → Ansicht:</b> Cursor (Knoten voll, Rest gedimmt), Hover über Zeile, difference/intersection/fillet/chamfer/shell, Modifier <code>#</code> <code>%</code> <code>!</code> <code>*</code>, Transformationen mit Koordinatenkreuzen vorher/nachher und Umriss an der Ausgangsposition, 2D-Formen als Umriss mit leichter Füllung.</li>
  <li><b>Ansicht → Code:</b> Hover über Fläche markiert die Zeile, Klick setzt den Cursor, Doppelklick auf eine Modulinstanz springt in die Definition.</li>
  <li><b>Modulkontext:</b> <code>SetContext</code> beim Cursor in einer Moduldefinition, Pfadleiste mit Sprung eine Ebene hoch.</li>
  <li><b>Raster und Anker (lesend):</b> Ebenen am Rand der Hülle, Schnittpunkte, Literal durchgezogen / Ausdruck gestrichelt, Schalter „alle Raster“, Hover über Rasterausdruck, Anker als Rauten mit Pfeil.</li>
  <li><b>Dateien und Fehler:</b> Autospeichern in IndexedDB, Öffnen/Speichern, Beispielmenü, Export STL/3MF, Syntaxfehler und Watchdog.</li>
</ul>

<h2>2 Testumgebung</h2>
<table>${env.map(([a, b]) => `<tr><th style="width:32mm">${esc(a)}</th><td>${esc(b)}</td></tr>`).join("")}</table>
<p class="small">Die End-to-End-Tests laufen gegen den Produktions-Build. Gewartet wird auf Szenen-Versionen und den Darstellungszustand über den Testhaken <code>window.__cad</code>, nicht auf feste Zeiten. Hover und Klick in der Ansicht sind echte Mausereignisse auf dem Canvas; der Haken liefert nur einen Bildpunkt, unter dem eine Fläche der gesuchten Zeile liegt.</p>

<h2>3 Szenarien (Playwright)</h2>
<table>
<tr><th>ID</th><th>Szenario</th><th>Ergebnis</th><th>Dauer</th><th>Prüfung</th></tr>
${scenarioRows}
</table>

<h2>4 Unit-Tests (Vitest)</h2>
${vt ? `<table><tr><th>Datei</th><th>bestanden</th><th>Fehlgeschlagen</th></tr>${unitRows}</table>
<p class="small">Logik ohne Browser: Lezer-Grammatik auf allen Beispielen, Zuordnung Code ↔ Knoten (Cursor, Fläche → Zeile, Mehrdeutigkeit, Modulkontext), Darstellungszustände, Rasteranzeige, Pfadleiste und Definitionssprung, Editor-Dekorationen, Abbildung alter Positionen. Szenen als Fixtures aus der echten Engine (<code>web/scripts/dump-scene.mjs</code>).</p>` : "<p>Keine Vitest-Ergebnisse gefunden.</p>"}

<h2>5 Messwerte</h2>
<table><tr><th>Größe</th><th>Wert</th><th>Messung</th></tr>${metricRows}</table>
<p class="small">Software-Rendering ohne GPU; auf einem Desktop mit GPU sind Bildraten deutlich höher. Die Zeiten enthalten die Auswertung im Worker (Sprache + Kernel) und das Nachladen der Meshes.</p>

<h2>6 Bekannte Grenzen und Abweichungen</h2>
<ul>
  <li>Griffe, Ziehen, Snappen und die Parameterleiste sind nicht Teil von Phase A; Raster und Anker werden nur angezeigt.</li>
  <li>Hover über Kanten wird über die angrenzende Fläche aufgelöst (Kanten tragen keine Knoten-Tags). Union-Kinder erhalten beim Hover einen dünnen Umriss, beim Cursor nicht.</li>
  <li>Beim Wechsel des Modulkontexts sendet die UI den Quelltext mit neuer Version erneut, damit jede Szenen-Version genau eine Auswertung bezeichnet (Meshes werden pro Version angefordert). Das kostet eine zusätzliche Sprachauswertung von wenigen Millisekunden.</li>
  <li>Bei Mehrdeutigkeit (gleicher Inhalt an mehreren Stellen, z. B. zwei Deckel) wird über die Lage des Trefferpunkts in den Knotenhüllen getrennt; bleibt sie bestehen, werden alle Zeilen markiert.</li>
  <li>Rasterausdrücke in Schleifen (z. B. <code>g[loch[i], mitte, boden]</code>) liefert die Szene einmal; beim Hover wird der Punkt des ersten Durchlaufs hervorgehoben.</li>
  <li>Der Editor bricht lange Zeilen um; Hinweise am Zeilenende können dadurch in eine zweite Bildschirmzeile rutschen.</li>
  <li>Der Watchdog-Test blockiert den Worker über eine Testnachricht; ein echter hängender Kernel-Aufruf wurde nicht nachgestellt.</li>
  <li>Die Messwerte stammen aus einem headless Chromium mit SwiftShader und sind nur relativ aussagekräftig.</li>
</ul>

<div class="figs">
<h2>7 Screenshots</h2>
${figures}
</div>
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
  footerTemplate: '<div style="font-size:7pt;color:#777;width:100%;text-align:center">Testbericht 2 – UI Phase A · Seite <span class="pageNumber"></span> von <span class="totalPages"></span></div>',
  margin: { top: "14mm", bottom: "16mm", left: "12mm", right: "12mm" },
});
await browser.close();
console.log(`report2: ${e2ePassed}/${e2e.length} e2e, ${unitPassed}/${unitTotal} unit, ${images.length + extra.length} Bilder (max ${kib(maxImg)})`);
console.log(`-> ${join(out, "report.html")}`);
console.log(`-> ${join(out, "report.pdf")}`);
