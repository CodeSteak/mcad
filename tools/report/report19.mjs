#!/usr/bin/env node
// Test report 19 ("Messen", Plan 18): measure mode in the browser and in Tauri – every kind with
// a screenshot, measuring in the section, the list (copy, delete), provenance, the dark theme,
// response times; `cad measure` with the same kinds (text and JSON), analytic test models
// against their formulas, test runs, limits.
// Writes docs/reports/19-messen/report.html (images in img/).
//
// Inputs (gitignored, reports/tmp/report19/):
//   scenarios.json, img/*.jpg, metrics.json     cd web && pnpm exec playwright test e2e/messen.spec.ts
//   cli/*.txt, cli/*.json, cli/*.png,           CAD=target/release/cad tools/report/report19-data.sh
//   analytic.json
//   playwright.txt                              whole suite (cd web && pnpm e2e > …)
//   vitest.txt, typecheck.txt                   cd web && pnpm test, pnpm typecheck
//   cargo.txt, clippy.txt, fmt.txt              cargo test --workspace, clippy -D warnings, fmt --check
//   tauri/tauri-e2e.json, tauri/img/*.png       node web/e2e-tauri/desktop.e2e.mjs --only T17 --out reports/tmp/report19/tauri
//   wasm.json (optional)                        {"base", "before": {raw, gzip}, "after": {raw, gzip}}
//
//   node tools/report/report19.mjs
//   node tools/report/print-pdf.mjs docs/reports/19-messen/report.html docs/reports/19-messen/report.pdf "Testbericht 19: Messen"

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
const de = (x, d = 0) => (x === null || x === undefined || Number.isNaN(Number(x)) ? "–" : Number(x).toLocaleString("de-DE", { minimumFractionDigits: d, maximumFractionDigits: d }));
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
const fig = (s, cap) => (s ? `<figure><img src="${s}" alt=""><figcaption>${md(cap)}</figcaption></figure>` : `<p class="small">(Bild fehlt: ${esc(cap)})</p>`);

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
const limitOf = (r) => (/Kernel/.test(r.name) ? 1e-3 : 1e-6);
const analyticOk = analytic.filter((r) => r.err !== null && Math.abs(r.err) <= Math.max(limitOf(r) * Math.abs(r.expect), 1e-9)).length;

const date = new Date().toLocaleDateString("de-DE", { day: "2-digit", month: "2-digit", year: "numeric" });
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
  if (!s) return `<p class="small">(Szenario ${esc(id)} fehlt)</p>`;
  const images = (s.images ?? []).map((i) => fig(pw(i.name), i.label)).join("");
  const code = s.code ? pre(s.code) : "";
  const body = code && !opts.wide && s.code.includes("\n") ? `<div class="codefig"><div>${code}</div><div>${images}</div></div>` : `${images}${code}`;
  return `<h3>${esc(s.id)} ${esc(s.title)}</h3><p>${md(s.caption)}</p>${body}<ul class="checks">${(s.checks ?? []).map((c) => `<li>${md(c)}</li>`).join("")}</ul>`;
}

const sci = (x) => (x === null || x === undefined ? "–" : x === 0 ? "0" : Math.abs(x) < 1e-3 ? x.toExponential(1) : de(x, 6));
const analyticTable = analytic.length
  ? `<table><tr><th>Fall</th><th>Aufruf</th><th class="n">erwartet</th><th class="n">gemessen</th><th class="n">Abweichung</th><th>exakt</th></tr>${analytic
      .map((r) => {
        const ok = r.err !== null && Math.abs(r.err) <= Math.max(limitOf(r) * Math.abs(r.expect), 1e-9);
        return `<tr><td>${esc(r.name)}</td><td><code>${esc(r.args)}</code></td><td class="n">${de(r.expect, 6)}</td><td class="n">${r.got === null ? esc(r.label) : de(r.got, 6)}</td><td class="n ${ok ? "ok" : "bad"}">${sci(r.err)}</td><td>${r.exact ? "ja" : r.exact === false ? `nein${r.tol ? ` (± ${sci(r.tol)})` : ""}` : "–"}</td></tr>`;
      })
      .join("")}</table>`
  : "<p>(fehlt: tools/report/report19-data.sh)</p>";

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
<html lang="de">
<head>
<meta charset="utf-8">
<title>Testbericht 19 – Messen</title>
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
<h1>Testbericht 19 – Messen</h1>
<div class="meta">Plan 18: Messwerkzeug in UI (Browser, Tauri) und CLI mit gemeinsamer Logik in Rust · cad-measure (neu), cad-engine (<code>measure_parts</code>, <code>section_parts</code>), cad-session (<code>measure.rs</code>, Protokoll <code>Measure</code>), cad-cli (<code>measure.rs</code>), Web-UI (<code>app/measure.ts</code>, <code>measureUi.tsx</code>, Viewer) · ${esc(date)} · Branch ${esc(branch)} · Commit ${esc(commit)} · ${esc(rustc)} · ${os.cpus().length} Kerne</div>

<div class="summary">
  <div>Messarten<b class="ok">7</b>Abstand, Radius, Winkel, Länge, Fläche, Körper, Wandstärke – auch im Schnitt</div>
  <div>Analytische Fälle (CLI)<b class="${analyticOk === analytic.length && analytic.length ? "ok" : "bad"}">${analytic.length ? `${analyticOk} / ${analytic.length}` : "–"}</b>innerhalb 10⁻⁶ relativ (Kernel-Volumen 10⁻³)</div>
  <div>cargo test --workspace<b class="${cargoSum && cargoSum.failed === 0 ? "ok" : "bad"}">${cargoSum ? `${cargoSum.ok} / ${cargoSum.ok + cargoSum.failed}` : "–"}</b>davon ${newTests.length} neu</div>
  <div>clippy · fmt<b class="${clippyOk && fmtOk ? "ok" : "bad"}">${clippy == null ? "–" : clippyOk ? "sauber" : "Befund"} · ${fmt == null ? "–" : fmtOk ? "sauber" : "Abw."}</b>-D warnings</div>
  <div>Vitest · Playwright<b class="${pwSum && pwSum.failed === 0 ? "ok" : "bad"}">${vitestSum ?? "–"} · ${pwSum ? `${pwSum.passed} / ${pwSum.passed + pwSum.failed}` : "–"}</b>neu: M01–M11</div>
  <div>Tauri T17<b class="${t17?.ok ? "ok" : "bad"}">${t17 ? (t17.ok ? "bestanden" : "fehlgeschlagen") : "–"}</b>${t17 ? `${t17.checks.length} Prüfungen, nativer Kernel` : ""}</div>
  <div>Antwort unter der Maus<b>${hover ? `${de(hover.median, 1)} ms` : "–"}</b>Median (Browser, Wasm, ${hover ? hover.n : "–"} Antworten)</div>
</div>

<h2>1 Auftrag und Ergebnis</h2>
<p>Auftrag: das Messwerkzeug in der UI (Browser und Tauri) und in der CLI ausbauen – Abstand Punkt–Punkt und zwischen Punkten, Kanten, Flächen (minimal, mit Fußpunkten, parallele Ebenen als Normalabstand), Radius/Durchmesser von Kreiskanten, Bögen, Zylinder-, Kegel-, Kugel- und Torusflächen exakt aus der B-Rep mit Achse/Mittelpunkt, Winkel, Länge, Fläche, Hülle/Volumen; Messen im aktiven Schnitt (Snap auf Konturen, Abstände in der Ebene, Radius von Schnittkreisen, Wandstärke, Fläche der Kontur) mit exakten Schnittkurven; Snap, Maßlinien und Beschriftungen, mehrere Messungen, Löschen, Kopieren als Text/Kommentar/<code>cad measure</code>; Bezug zur Sprache; dieselben Messarten in <code>cad measure</code> mit JSON. Umgesetzt:</p>
<ul>
<li><b>Gemeinsame Logik in Rust:</b> neues Crate <code>cad-measure</code> (Kanten und Flächen exakt, Abstand, Winkel, Radius, Fläche, Schnitt, Wandstärke), in <code>cad-session</code> das Messmodell je Knoten, Snap, Provenienz, Texte und CLI-Argumente – dieselben Funktionen bedienen das Protokoll (UI) und die Referenzen der CLI.</li>
<li><b>Exaktheit:</b> analytische Geometrie in geschlossener Form; Schnittkurven als Ebene ∩ B-Rep aus dem Kernel, ihre Segmente auf analytischen Flächen als Gerade, Kreis oder Ellipse (mit der Abweichung der Kernel-Kurve); sonst mit angegebener Toleranz. Jede Messung trägt <code>exact</code> und <code>tol</code>.</li>
<li><b>UI:</b> Modus „Messen“ mit Kontextleiste (Messart, „Im Schnitt“), Snap mit Anzeige, Maßlinien, Hilfslinien, Beschriftungen (hell/dunkel geprüft), Liste mit Kopieren und Löschen – als eigenständige Bausteine für die UI-Überarbeitung.</li>
<li><b>CLI:</b> <code>--distance</code>, <code>--radius</code>, <code>--angle</code>, <code>--length</code>, <code>--area face@P</code>, <code>--body</code>, <code>--section …</code> (Konturen, Kreise, kleinste Wand), <code>--wall</code>, <code>--at</code>; Elementreferenzen <code>vertex@P</code>, <code>mid@P</code>, <code>center@P</code>, <code>edge@P</code>, <code>face@P</code>, <code>body@P</code>, <code>region@P</code>; JSON <code>measurements</code> (Protokolltyp), <code>section</code>, <code>at</code>.</li>
</ul>

<h2>2 Entscheidungen</h2>
<table>
<tr><th>Frage</th><th>Entscheidung</th></tr>
<tr><td>Wo gemessen wird</td><td>In der Engine (Worker bzw. Engine-Prozess) auf der neutralen B-Rep der Knotenteile in Kontextkoordinaten; die UI schickt Strahl, ihren Raycast-Treffer (Knoten, Flächen-ID, Punkt), Pixelgröße und Snap-Radius, bekommt das Element unter der Maus und nach dem letzten Klick die Messung mit Maßlinien. Wie beim Skizzen-Editor keine Auswahl in der Engine.</td></tr>
<tr><td>Abstand</td><td>Grobsuche über die Tessellierung (BVH), Verfeinerung durch abwechselnde Projektion auf die exakten Kurven und Flächen; außerhalb einer getrimmten Fläche auf ihren Rand. Parallele Ebenen: Normalabstand als Wert, der kleinste Abstand der begrenzten Flächen daneben; parallele Achsen: Achsabstand.</td></tr>
<tr><td>Radius</td><td>Aus den Parametern der B-Rep; Verrundungen an Kreiskanten (bei SMLib Rotationsflächen mit Kreisbogen-Erzeugender) werden erkannt, Freiform über die Hauptkrümmung (genähert).</td></tr>
<tr><td>Winkel zweier Ebenen</td><td>Winkel zwischen den Ebenen (0–90°, Fase 45°), dazu Öffnungswinkel an der Schnittgeraden und Winkel der Normalen.</td></tr>
<tr><td>Schnitt</td><td>Kernel-Boolean mit dem behaltenen Halbraum (Konvention von <code>cad render --section</code>), Deckflächen als Bereiche; Segmente aus der geschnittenen Fläche in geschlossener Form. Ein Boolean je Teil und Ebene, gecacht.</td></tr>
<tr><td>Wandstärke</td><td>An einem Konturpunkt senkrecht zur Kontur ins Material bis zur nächsten Kontur; ohne Punkt die kleinste über Stichproben (Schätzung).</td></tr>
<tr><td>Bezug zur Sprache</td><td>Flächen-Tags → Knoten → Zeilen und benannte Instanzen; dazu Rasterpunkte, -linien, -ebenen und Anker, auf denen ein Punkt, eine Achse oder eine Ebene liegt.</td></tr>
<tr><td>CLI-Referenzen</td><td>Das Element nächst einem Punkt (<code>face@[x,y,z]</code>): eindeutig und lesbar, die UI kopiert Messungen in dieser Form; <code>--at P</code> zeigt, was bei P liegt.</td></tr>
<tr><td>Ort in der UI</td><td>Kontextleiste und Liste als eigenständige Komponenten (<code>MeasureBar</code>, <code>MeasurePanel</code>); vorerst im Stapel rechts oben bzw. rechts neben der Ansicht, nach dem UI-Konzept später links oben unter der Werkzeugleiste und als Reiter „Messen“.</td></tr>
</table>

<h2>3 Ablauf</h2>
${pre(`Maus bewegt  → Measure { kind, picks, ray, hit {node, face, point}, pixel, snap_px, free, section? | clip? }
              ← Measure { hover {kind, label, point, reference, origin, lines}, prompt, section {regions, area, exact} }
Klick        → Measure { …, click: true }
              ← picks (Auswahl gemerkt)  oder  result: Measurement { value, label, values, lines, guides, points,
                                                                     at, details, exact, tol, text, cli }
Engine:  measure_model(node)  = je Teil B-Rep + Tessellierung (Kontext)      measure_section(node, plane) = Boolean
         snap → Eckpunkt | Kantenmitte | Kreismitte | Kante | Fläche  (im Schnitt: Kontur, Bereich)
         measurement → cad_measure::{distance, radius, angle, face_area, Section::wall …}
CLI:     face@[x,y,z] → dieselbe Auswahl → dieselbe Messung → JSON/Text`)}

<h2>4 Messarten in der UI</h2>
<p>Alle Szenarien klicken mit der Maus (Playwright, Chromium mit SwiftShader) und prüfen die Werte gegen die Geometrie.</p>
${scen("M01")}
${scen("M02")}
${scen("M03")}
${scen("M04")}
${scen("M05", { wide: true })}

<h2>5 Messen im Schnitt</h2>
${scen("M06")}

<h2>6 Liste, Kopieren, Bezug zur Sprache, Thema</h2>
${scen("M07")}
${scen("M08", { wide: true })}
${scen("M09")}

<h2>7 Desktop-App (Tauri)</h2>
<p>Szenario T17 in <code>web/e2e-tauri/desktop.e2e.mjs</code> (tauri-driver, WebKitWebDriver, Xvfb): die App <code>cad</code> mit dem nativen Engine-Prozess, Nachrichten über den <code>TauriTransport</code>; die Ansicht wird mit Zeigerereignissen auf dem Canvas bedient.</p>
${t17 ? `<p>Ergebnis: <span class="${t17.ok ? "ok" : "bad"}">${t17.ok ? "bestanden" : "fehlgeschlagen"}</span> in ${de(t17.ms / 1000, 1)} s${tauri?.metrics?.measureTauri?.hoverMedianMs != null ? `, Antwort unter der Maus Median ${de(tauri.metrics.measureTauri.hoverMedianMs, 1)} ms` : ""}.</p><ul class="checks">${t17.checks.map((c) => `<li class="${c.ok ? "" : "bad"}">${md(c.text)}</li>`).join("")}</ul>${(t17.images ?? []).map((i) => fig(tauriImg(i.name), i.label)).join("")}${t17.error ? pre(t17.error) : ""}` : "<p>(fehlt)</p>"}

<h2>8 CLI: <code>cad measure</code></h2>
<p>Dieselben Messungen ohne UI, für Agenten mit <code>--json</code>. Ausgaben aus <code>tools/report/report19-data.sh</code> (Release-Binary, ohne Daemon; Zeiten je Aufruf im Mittel von 3 Läufen inklusive Laden und Kernel).</p>
<h3>Gehäuse im Schnitt z = 10: Bereiche, Wandstärke, Innenmaß (${de(times["housing-section"])} ms)</h3>
<div class="two"><div>${term(cliTxt("housing-section") ?? "(fehlt)")}</div><div>${fig(cliImg("housing-section.png"), "`cad render gehaeuse.scad --section z=10 --view top`: die zwei Wandbereiche")}</div></div>
<h3>Gehäuse: Innenabstand, Bauteil, Kantenlänge (${de(times["housing-3d"])} ms)</h3>
${term(cliTxt("housing-3d") ?? "(fehlt)")}
<h3>Lochplatte: Bohrung, Verrundung, Teilung (${de(times["plate-radius"])} ms)</h3>
${term(cliTxt("plate-radius") ?? "(fehlt)")}
<h3>Lochplatte im Schnitt z = 2: Kreise, Fläche, Radius, Wand (${de(times["plate-section"])} ms)</h3>
<div class="two"><div>${term(cliTxt("plate-section") ?? "(fehlt)")}</div><div>${fig(cliImg("plate-section.png"), "`cad render lochplatte.scad --section z=2 --view top`")}</div></div>
<h3>Was liegt bei P: <code>--at</code> (${de(times["plate-at"])} ms)</h3>
${term(cliTxt("plate-at") ?? "(fehlt)")}
<h3>Winkel einer Fase (${de(times["chamfer-angle"])} ms)</h3>
${term(cliTxt("chamfer-angle") ?? "(fehlt)")}
<h3>JSON für Agenten (Auszug: Radius der Bohrung)</h3>
${jsonExcerpt ? pre(jsonExcerpt) : "<p>(fehlt)</p>"}
<h3>JSON des Schnitts (Auszug)</h3>
${sectionExcerpt ? pre(sectionExcerpt) : "<p>(fehlt)</p>"}

<h2>9 Analytische Prüfung</h2>
<p>Kleine Modelle, gemessen über die CLI (dieselbe Logik wie die UI), gegen die Formel. Grenze 10⁻⁶ relativ; Volumen kommt aus der Integration des Kernels (SMLib, ~10⁻⁴) und hat die Grenze 10⁻³.</p>
${analyticTable}

<h2>10 Messwerte</h2>
${scen("M10", { wide: true })}
${wasm ? `<h3>Wasm-Modul</h3><table><tr><th></th><th class="n">roh</th><th class="n">gzip -9</th></tr><tr><td><code>cad_wasm_bg.wasm</code> vorher (Commit ${esc(wasm.base)})</td><td class="n">${de(wasm.before.raw)} B</td><td class="n">${de(wasm.before.gzip)} B</td></tr><tr><td>mit Messen</td><td class="n">${de(wasm.after.raw)} B</td><td class="n">${de(wasm.after.gzip)} B</td></tr><tr><td>Zuwachs</td><td class="n">${de(wasm.after.raw - wasm.before.raw)} B</td><td class="n">${de(wasm.after.gzip - wasm.before.gzip)} B</td></tr></table>` : ""}

<h2>11 Testläufe</h2>
<table>
<tr><th>Lauf</th><th>Ergebnis</th></tr>
<tr><td><code>cargo test --workspace -- --test-threads=2</code></td><td>${cargoSum ? `${cargoSum.ok} bestanden, ${cargoSum.failed} fehlgeschlagen` : "–"}</td></tr>
<tr><td><code>cargo clippy --workspace --all-targets -- -D warnings</code> (nativ und <code>-p cad-wasm --target wasm32-unknown-unknown</code>) · <code>cargo fmt --check</code></td><td>${clippyOk ? "sauber" : "Befund"} · ${fmtOk ? "sauber" : "Abweichung"}</td></tr>
<tr><td><code>pnpm typecheck</code> · <code>pnpm test</code> (Vitest)</td><td>${typecheck !== null ? (/error TS/i.test(typecheck) ? "Fehler" : "sauber") : "–"} · ${vitestSum ?? "–"} bestanden</td></tr>
<tr><td><code>pnpm e2e</code> (Playwright, Chromium, ganze Suite)</td><td>${pwSum ? `${pwSum.passed} bestanden, ${pwSum.failed} fehlgeschlagen${pwSum.skipped ? `, ${pwSum.skipped} übersprungen` : ""}${pwSum.flaky ? `, ${pwSum.flaky} unbeständig` : ""}${pwFailed.length ? `: ${pwFailed.map(esc).join("; ")}` : ""}` : "–"}</td></tr>
<tr><td>Tauri-E2E T17</td><td>${t17 ? (t17.ok ? "bestanden" : "fehlgeschlagen") : "–"}</td></tr>
<tr><td>Analytische Fälle über die CLI</td><td>${analytic.length ? `${analyticOk} von ${analytic.length} innerhalb der Grenze` : "–"}</td></tr>
</table>
<p class="small">Unbeständige Tests behoben (vorher sporadisch rot, auch ohne den Messmodus): <code>waitHandles</code> antwortete noch mit den Griffen der vorigen Cursorzeile (B06, B10) und wartet jetzt auf stehende Griffe; <code>clickAt</code> in M01–M11 und der Klick in T17 warten auf die eigene Wirkung des Klicks statt auf irgendeine Antwort; T17 wählt Flächen mit Alt (ohne Punktfang). Nichts übersprungen oder deaktiviert.</p>
<p>Neue Rust-Tests (${newTests.length}):</p>
<p class="small">${newTests.map((x) => `<code>${esc(x.name)}</code> ${x.res === "ok" ? '<span class="ok">ok</span>' : '<span class="bad">FAILED</span>'}`).join(" · ")}</p>
<p class="small">cad-measure <code>tests/analytic.rs</code>: Zylinder (Radius, Achse, Mantel- und Deckfläche, Kreislänge, Normalabstand), Kegel (Radius, Halbwinkel, Spitze, Mantel), Kugel und Torus (Radien, Flächen, Hülle), Fillet-Zylinder und -Kugel eines Quaders, Fillet-Torus einer Kreiskante, Fase 45°/135°, Abstände, Schnitt durch zwei Bohrungen (exakte Kreise, Fläche, Wandstärken, Längsschnitt, schräger Schnitt als Ellipse); Unit-Tests der Kurven, Flächen und Quadratur. cad-session <code>tests/measure.rs</code>: Snap (Fläche, Eckpunkt, Kantenmitte, Kante) mit Provenienz, Klicks und CLI-Argumente, Referenzen, Lochplatte, Gehäuse mit Wandstärke im Schnitt, Bauteilvolumen, Fase, Schnittmodus über das Protokoll. cad-cli <code>tests/messen.rs</code>: JSON und Text der neuen Optionen, Aufruffehler. Vitest <code>logic/measure.test.ts</code>.</p>

<h2>12 Grenzen</h2>
<ul>
<li>Der Schnitt braucht einen Kernel-Boolean je Teil und Ebene; scheitert er, gibt es keinen Schnitt (Meldung), keine Ersatzkontur aus dem Netz.</li>
<li>Konturen auf Freiformflächen und auf schräg geschnittenen Kegeln, Tori und Rotationsflächen bleiben Kernel-Kurven (<code>exact = false</code>).</li>
<li>Wandstärke senkrecht zur Kontur an einer Stelle; die kleinste Wandstärke des Schnitts ist eine Schätzung über Stichproben.</li>
<li>Abstände zwischen Freiformflächen und Fälle, in denen die Projektion an einem anderen lokalen Minimum endet, gelten mit der Toleranz der Tessellierung; Flächen von Freiformflächen aus den Dreiecken; Volumen aus dem Kernel (~10⁻⁴).</li>
<li>CLI-Referenzen über den nächsten Punkt sind eindeutig, solange P einem Element am nächsten liegt; die Ergebnisse erscheinen in der Reihenfolge der Optionen.</li>
<li>2D-Ergebnisse (Skizzen ohne Extrusion) lassen sich nicht messen; die Liste der Messungen wird nicht gespeichert.</li>
<li>Die Kontextleiste liegt bis zur UI-Überarbeitung im Stapel rechts oben (die Ansichtsleiste bricht um), die Liste rechts neben der Ansicht.</li>
<li>Tauri T17 bedient die Ansicht mit synthetischen Zeigerereignissen; Messwerte aus SwiftShader bzw. Software-GL sind nur relativ aussagekräftig.</li>
</ul>
</main>
</body>
</html>
`;
writeFileSync(join(out, "report.html"), html);
console.log(`${join(out, "report.html")}: ${html.length} Zeichen, ${scenarios.length} Szenarien, ${analytic.length} analytische Fälle`);
