#!/usr/bin/env node
// Test report 3 ("UI Phase B: Griffe, Snap, Parameterleiste"): collects the results of the
// Phase B tests and writes
//
//   docs/reports/03-ui-phase-b/report.html, report.pdf, img/*.jpg
//
// Inputs (written by the test runs, gitignored under reports/tmp/):
//   report2/playwright.json   Playwright JSON reporter (web/playwright.config.ts); only
//                             web/e2e/ui-b.spec.ts is reported here, ui.spec.ts as regression
//   report3/vitest.json       Vitest JSON reporter
//   report3/cargo.txt         output of the Rust tests of the changed crates
//   report3/scenarios.json    title/caption/screenshots/code per scenario (web/e2e/helpers.ts)
//   report3/metrics.json      drag latency and frame rate (web/e2e/ui-b.spec.ts, B14)
//   report3/img/*.jpg         screenshots
//
//   cd web && pnpm test:report3     (Rust tests, Vitest, Playwright, then this script)
//   node tools/report/report3-ui.mjs
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
const input = join(root, "reports/tmp/report3");
const pwFile = join(root, "reports/tmp/report2/playwright.json");
const out = join(root, "docs/reports/03-ui-phase-b");
const require = createRequire(join(web, "package.json"));

const readJson = (f, d = null) => (existsSync(f) ? JSON.parse(readFileSync(f, "utf8")) : d);
const esc = (s) => String(s ?? "").replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
const fmtMs = (v) => (typeof v === "number" ? `${Math.round(v)} ms` : "–");
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

const pw = readJson(pwFile);
const vt = readJson(join(input, "vitest.json"));
const scenarios = readJson(join(input, "scenarios.json"), []);
const metrics = readJson(join(input, "metrics.json"), {});
const cargo = existsSync(join(input, "cargo.txt")) ? readFileSync(join(input, "cargo.txt"), "utf8") : null;
if (!pw) {
  console.error(`fehlt: ${pwFile} (zuerst: cd web && pnpm e2e)`);
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
      acc.push({ id, title: spec.title.slice(id.length + 1), status: r?.status ?? t.status ?? "unbekannt", ms: r?.duration ?? 0, error: r?.error?.message ?? null });
    }
  }
  return acc;
}
const e2e = flatten(pw, "ui-b.spec.ts");
const phaseA = flatten(pw, "ui.spec.ts");
const e2ePassed = e2e.filter((t) => t.status === "passed").length;
const aPassed = phaseA.filter((t) => t.status === "passed").length;
if (!e2e.length) {
  console.error("keine Ergebnisse von web/e2e/ui-b.spec.ts in playwright.json");
  process.exit(1);
}

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
const NEW_UNIT = new Set(["web/src/logic/format.test.ts", "web/src/logic/handles.test.ts", "web/src/logic/drag.test.ts", "web/src/logic/params.test.ts", "web/src/editor/editGroup.test.ts"]);

/** Rust tests: `test name ... ok` lines grouped by test binary. */
const rust = [];
if (cargo) {
  let cur = null;
  for (const l of cargo.split("\n")) {
    const run = l.match(/Running (?:unittests )?(\S+)/);
    if (run) {
      cur = { bin: run[1].replace(/\s*\(.*$/, ""), tests: [] };
      rust.push(cur);
      continue;
    }
    const t = l.match(/^test (\S+) \.\.\. (ok|FAILED|ignored)/);
    if (t && cur) cur.tests.push({ name: t[1], status: t[2] });
  }
}
const rustBins = rust.filter((b) => b.tests.length);
const rustTotal = rustBins.reduce((s, b) => s + b.tests.length, 0);
const rustOk = rustBins.reduce((s, b) => s + b.tests.filter((t) => t.status === "ok").length, 0);
const NEW_RUST = new Set([
  "computed_values_have_locked_handles",
  "one_d_handles_snap_to_grid_planes",
  "plane_snap_respects_visibility",
  "zone_center_handle_through_translate",
  "drag_always_answers_and_plane_snap",
]);

// ---------------------------------------------------------------- images

rmSync(join(out, "img"), { recursive: true, force: true });
mkdirSync(join(out, "img"), { recursive: true });
const used = new Set();
for (const s of scenarios) for (const k of [s.before, s.after]) if (k) used.add(k);
for (const n of used) {
  const src = join(input, "img", `${n}.jpg`);
  if (existsSync(src)) copyFileSync(src, join(out, "img", `${n}.jpg`));
}
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
  ["Engine", `cad_wasm_bg.wasm ${mib(asset(/^cad_wasm_bg-.*\.wasm$/))}, smlib.wasm ${mib(asset(/^smlib-.*\.wasm$/))}`],
  ["Quelltext", `Branch ${git("rev-parse", "--abbrev-ref", "HEAD")}, Commit ${git("rev-parse", "--short", "HEAD")}`],
];

// ---------------------------------------------------------------- html

const date = new Date().toLocaleDateString("de-DE", { year: "numeric", month: "long", day: "numeric" });
const scen = (id) => scenarios.find((s) => s.id === id);
const statusLabel = (s) => (s === "passed" ? "bestanden" : s === "skipped" ? "übersprungen" : "fehlgeschlagen");

const scenarioRows = e2e
  .map((t) => {
    const c = scen(t.id);
    return `<tr class="${t.status === "passed" ? "ok" : "bad"}"><td>${esc(t.id)}</td><td>${esc(t.title)}</td><td class="res">${statusLabel(t.status)}</td><td class="num">${(t.ms / 1000).toFixed(1)} s</td><td>${esc(c?.caption ?? "")}${t.error ? `<div class="err">${esc(t.error.split("\n")[0])}</div>` : ""}</td></tr>`;
  })
  .join("\n");

const unitRows = unit
  .map((f) => `<tr class="${f.failed.length ? "bad" : "ok"}"><td>${esc(f.file)}${NEW_UNIT.has(f.file) ? " <span class=\"tag\">neu</span>" : ""}</td><td class="num">${f.passed} / ${f.total}</td><td>${f.failed.map(esc).join("<br>") || "–"}</td></tr>`)
  .join("\n");

const rustRows = rustBins
  .map((b) => {
    const fresh = b.tests.filter((t) => NEW_RUST.has(t.name)).map((t) => t.name);
    const bad = b.tests.filter((t) => t.status === "FAILED").map((t) => t.name);
    const name = b.bin.replace(/^.*\/deps\//, "").replace(/-[0-9a-f]{16}$/, "");
    return `<tr class="${bad.length ? "bad" : "ok"}"><td>${esc(name)}</td><td class="num">${b.tests.filter((t) => t.status === "ok").length} / ${b.tests.length}</td><td>${fresh.map((n) => `<code>${esc(n)}</code>`).join(", ") || "–"}${bad.length ? `<div class="err">${bad.map(esc).join(", ")}</div>` : ""}</td></tr>`;
  })
  .join("\n");

const d = metrics.drag ?? {};
const c = metrics.dragCube ?? {};

const metricRows = [
  ["Ziehen → Szene, grob (Median)", fmtMs(d.coarse?.median), `Drag-Nachricht bis die Szene des daraus geschriebenen Quelltexts gezeichnet ist, grobe Tessellierung (10 × Toleranz); ${d.coarse?.n ?? "–"} Szenen`],
  ["Ziehen → Szene, grob (p95 / max)", `${fmtMs(d.coarse?.p95)} / ${fmtMs(d.coarse?.max)}`, "95-%-Quantil und Maximum derselben Messung"],
  ["Loslassen → feine Szene", fmtMs(d.fine?.last), "pointerup bis die Szene mit feinem Wurzel-Mesh gezeichnet ist (DragEnd, Undo-Gruppe schließen, Neuauswertung aus dem Cache)"],
  ["Bildrate beim Ziehen", typeof d.fps === "number" ? `${d.fps.toFixed(0)} fps` : "–", `requestAnimationFrame-Zähler während des Ziehens (${d.durationMs ? (d.durationMs / 1000).toFixed(1) : "–"} s); längste Lücke ${fmtMs(d.maxGapMs)}`],
  ["Mausbewegungen / Drag-Nachrichten", `${d.moves ?? "–"} / ${d.sent ?? "–"}`, `höchstens eine Nachricht in Flug, ${d.dropped ?? "–"} Zwischenstände übersprungen (letzte Position gewinnt)`],
  ["Leichtes Modell: Ziehen → Szene grob (Median / p95)", `${fmtMs(c.coarse?.median)} / ${fmtMs(c.coarse?.p95)}`, `B01, ${c.model ?? "–"}: ${c.coarse?.n ?? "–"} Szenen; Aufwand von UI, Worker und Protokoll ohne nennenswerte Kernel-Arbeit`],
  ["Leichtes Modell: Loslassen → fein / fps", `${fmtMs(c.fine?.last)} / ${typeof c.fps === "number" ? c.fps.toFixed(0) : "–"} fps`, `längste Frame-Lücke ${fmtMs(c.maxGapMs)}, ${c.sent ?? "–"} Drag-Nachrichten`],
  ["Größte Bilddatei im Bericht", kib(maxImg), "JPEG, Qualität 82"],
]
  .map(([a, b, c]) => `<tr><td>${esc(a)}</td><td class="num">${esc(b)}</td><td>${esc(c)}</td></tr>`)
  .join("\n");

const code = (s) => (s ? `<pre>${esc(s)}</pre>` : "<pre class=\"none\">–</pre>");
const shotsFor = (s) => {
  const imgs = [s.before && { n: s.before, l: s.before.includes("waehrend") ? "während" : "vorher" }, s.after && { n: s.after, l: s.after.includes("waehrend") ? "während" : "nachher" }].filter(Boolean);
  if (!imgs.length) return "";
  return `<div class="pair">${imgs.map((i) => `<div><div class="lbl">${esc(i.l)}</div><img src="img/${esc(i.n)}.jpg" alt="${esc(s.title)} ${esc(i.l)}"></div>`).join("")}</div>`;
};
const blocks = e2e
  .map((t) => scen(t.id))
  .filter(Boolean)
  .map(
    (s) => `<section class="sc">
<h3>${esc(s.id)} ${esc(s.title)}</h3>
${shotsFor(s)}
${s.codeBefore || s.codeAfter ? `<div class="codes"><div><div class="lbl">Code vorher</div>${code(s.codeBefore)}</div><div><div class="lbl">Code nachher</div>${code(s.codeAfter)}</div></div>` : ""}
<p class="cap">${esc(s.caption)}</p>
</section>`,
  )
  .join("\n");

const html = `<!doctype html>
<html lang="de">
<head>
<meta charset="utf-8">
<title>Testbericht 3 – UI Phase B</title>
<style>
  @page { size: A4; margin: 16mm 14mm 16mm 14mm; }
  :root { --text: #1f2328; --muted: #5d646e; --border: #d5d9df; --ok: #1a7f37; --bad: #c9302c; --head: #f2f4f7; }
  * { box-sizing: border-box; }
  body { font: 10pt/1.45 system-ui, -apple-system, "Segoe UI", Roboto, sans-serif; color: var(--text); margin: 0; }
  main { max-width: 182mm; margin: 0 auto; }
  h1 { font-size: 20pt; margin: 0 0 2mm; }
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
  code, pre { font-family: ui-monospace, "SFMono-Regular", Menlo, Consolas, monospace; font-size: 8pt; }
  pre { margin: 0; padding: 1.5mm 2mm; background: #f6f8fa; border: 1px solid var(--border); border-radius: 1mm; white-space: pre-wrap; word-break: break-word; }
  pre.none { color: var(--muted); }
  .sc { break-inside: avoid; page-break-inside: avoid; margin: 0 0 7mm; }
  .pair { display: flex; gap: 2mm; }
  .pair > div { flex: 1; min-width: 0; }
  .pair img { max-width: 100%; max-height: 76mm; width: auto; height: auto; border: 1px solid var(--border); display: block; }
  .lbl { font-size: 7.5pt; color: var(--muted); text-transform: uppercase; letter-spacing: 0.04em; margin: 1mm 0 0.5mm; }
  .codes { display: flex; gap: 2mm; margin-top: 1.5mm; }
  .codes > div { flex: 1; min-width: 0; }
  .cap { font-size: 8.8pt; color: var(--muted); margin: 1.5mm 0 0; }
  .small { font-size: 8.8pt; color: var(--muted); }
  tr, td { break-inside: avoid; page-break-inside: avoid; }
</style>
</head>
<body>
<main>
<h1>Testbericht 3 – UI Phase B: Griffe, Snap, Parameterleiste</h1>
<div class="meta">Bearbeiten in der Ansicht · ${esc(date)} · Commit ${esc(git("rev-parse", "--short", "HEAD"))}</div>

<div class="summary">
  <div>Phase B (Playwright)<b>${e2ePassed} / ${e2e.length} bestanden</b></div>
  <div>Phase A (Regression)<b>${phaseA.length ? `${aPassed} / ${phaseA.length}` : "–"}</b></div>
  <div>Vitest<b>${vt ? `${unitPassed} / ${unitTotal}` : "–"}</b></div>
  <div>Rust (geänderte Crates)<b>${cargo ? `${rustOk} / ${rustTotal}` : "–"}</b></div>
  <div>Ziehen → Szene grob (Würfel / Lochplatte)<b>${fmtMs(c.coarse?.median)} / ${fmtMs(d.coarse?.median)}</b></div>
</div>

<h2>1 Umfang</h2>
<p>Phase B ergänzt die Browser-UI um das Bearbeiten in der 3D-Ansicht. Der Code bleibt die einzige Quelle: Griffe ändern Literale oder schreiben Rasterreferenzen, die Parameterleiste ändert nur die Vorschau.</p>
<ul>
  <li><b>Griffe</b> der Zeile unter dem Cursor, nur bei Literal oder Variable mit Literal; berechnete Werte werden als gesperrter Griff (grauer Ring) gezeigt. Arten: cube-/square-Ecke, box-/rect-Eckpunkte, Radius (sphere, circle, cylinder, torus), Höhe (cylinder, linear_extrude), cylinder zwischen zwei Punkten, translate-Ziel (Pfeil alt → neu, Griff am Ziel), Drehring pro Achse (rotate), Skalierung an der Hüllenecke, Rasterebenen mit Literal (Griff auf einer Kante der Rasterbox, Ziehen entlang der Achse), Punktvariablen, polygon-/spline-Punkte, Zonenmittelpunkt über translate. Farben: Literal orange, Standardwert eines Modulparameters violett, reine Referenz türkis, gesperrt grau.</li>
  <li><b>Ziehen:</b> <code>DragStart</code> → <code>Drag</code> (höchstens eine Nachricht in Flug, letzte Position gewinnt) → <code>Edits</code> → Editor wendet sie als Live-Änderung an und sendet den Quelltext sofort; die Engine tesselliert grob, nach <code>DragEnd</code> fein. Ein Ziehvorgang ist ein Undo-Schritt (Live-Änderungen ohne Verlauf, am Ende als ein isoliertes Ereignis). Esc bricht ab.</li>
  <li><b>Snappen</b> im Bildschirmraum (Fangradius einstellbar, Standard 14 px) auf Rasterpunkte des Moduls und sichtbare Anker (<code>SnapTargets</code>), Ziel hervorgehoben und beschriftet; geschrieben wird die Referenz. 1D-Griffe (Höhe, Radius, Ebene) snappen auf Rasterebenen derselben Achse und schreiben <code>g.name</code> (neu in cad-eval). <b>Alt:</b> kein Snap, Zahlen gerundet auf die Schrittweite (Einstellungen, Standard 1 mm / 15°).</li>
  <li><b>Parameterleiste</b> neben der Ansicht im Modulkontext: Zahl (Eingabefeld + Schieberegler), Bool (Schalter), Vektor (drei Felder); Reset pro Parameter, Markierung geänderter Werte, „Alle zurücksetzen“, Werte pro Modul bis Sitzungsende, „Als Standard übernehmen“ (<code>SetDefault</code>). Vorschau über <code>SetContext { overrides }</code>.</li>
  <li><b>Inline-Regler</b> für Zahl- und Bool-Literale von Zuweisungen am Zeilenende (Schieberegler, Schalter); ein Schieberegler-Zug ist ein Undo-Schritt.</li>
  <li><b>Strg+Klick</b> auf einen Raster- oder Ankerausdruck springt zur Deklaration.</li>
</ul>
<p class="small">Änderungen an Rust-Crates: <code>cad-eval</code> (gesperrte Griffe als <code>locked_handles</code>, <code>SnapRef::Plane</code> mit Prüfung, dass der Griff nach dem Schreiben exakt auf der Ebene liegt und die Ebene an der Stelle sichtbar ist), <code>cad-session</code> (<code>Drag</code> antwortet immer, <code>SetDragOptions</code>, Szene mit <code>locked_handles</code>), Protokoll-Typen neu erzeugt.</p>

<h2>2 Testumgebung</h2>
<table>${env.map(([a, b]) => `<tr><th style="width:32mm">${esc(a)}</th><td>${esc(b)}</td></tr>`).join("")}</table>
<p class="small">Die End-to-End-Tests laufen gegen den Produktions-Build mit echten Mausereignissen auf dem Canvas (Playwright <code>mouse.down/move/up</code>, ein Schritt pro Animationsframe). Gewartet wird auf Szenen-Versionen, den Zieh-Zustand und den Editortext über den Testhaken <code>window.__cad</code>, nie auf feste Zeiten. Der Haken liefert nur Bildschirmpositionen (Griffe, Rasterpunkte, Punkte auf Ring oder Achse).</p>

<h2>3 Szenarien (Playwright, web/e2e/ui-b.spec.ts)</h2>
<table>
<tr><th>ID</th><th>Szenario</th><th>Ergebnis</th><th>Dauer</th><th>Prüfung</th></tr>
${scenarioRows}
</table>
<p class="small">Regression: die ${phaseA.length} Szenarien aus Phase A (web/e2e/ui.spec.ts) liefen im selben Lauf, ${aPassed} bestanden.</p>

<h2>4 Unit-Tests</h2>
${vt ? `<table><tr><th>Vitest-Datei</th><th>bestanden</th><th>Fehlgeschlagen</th></tr>${unitRows}</table>
<p class="small">Neu: Snap-Suche im Bildschirmraum, Strahl → Wert (Ebene, Achse, Ring), Vorschau-Rundung, „höchstens ein Drag in Flug“, Rundung und Zahlendarstellung, Undo-Gruppierung mit dem echten CodeMirror-Verlauf, Abbildung der Engine-Edits, Zustand der Parameterleiste, Inline-Regler. Fixture <code>griffe.scad</code> aus der echten Engine.</p>` : "<p>Keine Vitest-Ergebnisse gefunden.</p>"}
${cargo ? `<table><tr><th>Rust-Testprogramm</th><th>bestanden</th><th>neue Tests</th></tr>${rustRows}</table>` : ""}

<h2>5 Messwerte</h2>
<table><tr><th>Größe</th><th>Wert</th><th>Messung</th></tr>${metricRows}</table>
<p class="small">Oberer Teil: ${esc(d.model ?? "–")}. Jede Änderung von t wertet die Differenz mit Verrundung und vier Bohrungen vollständig neu aus (kein Cache-Treffer); die Latenz ist hier von der Kernel-Zeit bestimmt (SetSource → vollständige Szene ${esc((d.evalMs ?? []).length ? `Median ${Math.round([...d.evalMs].sort((x, y) => x - y)[Math.floor(d.evalMs.length / 2)])} ms` : "–")}). Gleiche gerundete Werte erzeugen keine Änderung; jede Änderung wartet auf ihre Szene, bevor die nächste Position gesendet wird (sonst würde jeder neue Quelltext den laufenden Kernel-Auftrag abbrechen). Die Bildrate ist durch das Software-Rendering (SwiftShader) begrenzt; Phase A maß beim Drehen 14 fps.</p>

<h2>6 Bekannte Grenzen und Abweichungen</h2>
<ul>
  <li><b>Sprung zur Rasterdeklaration mit Strg+Klick</b> statt einfachem Klick: ein einfacher Klick muss weiter den Cursor setzen, sonst wären Rasterausdrücke nicht editierbar. Referenzen eines Moduls kennt die UI erst im Modulkontext (Cursor im Modul).</li>
  <li><b>Ohne Alt</b> wird gesnappt, wenn ein Ziel im Fangradius liegt, sonst werden Zahlen geschrieben (gerundet). Ein Griff, der nur aus Referenzen besteht (z. B. <code>box(g[…], g[…])</code>), wird ohne Alt nie zu Zahlen; er wechselt nur zwischen Zielen.</li>
  <li><b>Snap von 1D-Griffen</b> nur auf Ebenen, auf denen der Griff nach dem Schreiben von <code>g.name</code> exakt landet (gleicher Ursprung und Maßstab entlang der Achse). Unter einem verschobenen Teilbaum oder bei <code>center = true</code>/<code>d =</code> (Faktor 2) gibt es daher keine Ebenen-Ziele; geschrieben würde sonst ein Ausdruck wie <code>g.deckel - 5</code>, der den Griff sperrt.</li>
  <li>Punktgriffe werden in der Ebene durch den Griff senkrecht zur Blickrichtung gezogen (2D-Punkte in ihrer xy-Ebene); entlang der Blickrichtung ändert sich nichts. Für eine bestimmte Achse hilft eine Standardansicht.</li>
  <li>Undo-Gruppe: am Ende des Ziehens wird die Gesamtänderung einmal zurückgenommen und als ein Verlaufseintrag wieder angewendet; dabei entsteht eine zusätzliche Neuauswertung aus dem Cache.</li>
  <li>Inline-Regler erscheinen nur bei Zuweisungen mit Zahl- oder Bool-Literal (Spec, Werttypen); der Bereich richtet sich nach dem Wert beim Start des Ziehens (0 … 2 × Wert, mindestens 10).</li>
  <li>Parameter ohne Standardwert werden beim Reset auf 0 / false / [0, 0, 0] gesetzt (Spec-Tabelle); Zeichenketten und Listen sind nur lesbar.</li>
  <li>Die Messwerte stammen aus headless Chromium mit SwiftShader.</li>
</ul>

<h2>7 Screenshots und Codeausschnitte</h2>
${blocks}
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
  footerTemplate: '<div style="font-size:7pt;color:#777;width:100%;text-align:center">Testbericht 3 – UI Phase B · Seite <span class="pageNumber"></span> von <span class="totalPages"></span></div>',
  margin: { top: "14mm", bottom: "16mm", left: "12mm", right: "12mm" },
});
await browser.close();
console.log(`report3: ${e2ePassed}/${e2e.length} Phase B, ${aPassed}/${phaseA.length} Phase A, ${unitPassed}/${unitTotal} Vitest, ${rustOk}/${rustTotal} Rust, ${used.size} Bilder (max ${kib(maxImg)})`);
console.log(`-> ${join(out, "report.html")}`);
console.log(`-> ${join(out, "report.pdf")}`);
