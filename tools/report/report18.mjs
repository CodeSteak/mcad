#!/usr/bin/env node
// Test report 18 ("Skizzen-Editor", Plan 17.6/17.12, Schritt 4): drawing mode with raster in the
// browser and in Tauri – every tool with a screenshot and the code it wrote, the snap display,
// undo, dragging handles, deleting segments, sketches under transformations, the dark theme,
// response times, the Tauri scenario, test runs, limits.
// Writes docs/reports/18-skizzen-editor/report.html (images in img/).
//
// Inputs (gitignored, reports/tmp/report18/):
//   scenarios.json, img/*.jpg, metrics.json     cd web && pnpm exec playwright test e2e/skizzen-editor.spec.ts
//   playwright.txt                              whole suite (cd web && pnpm e2e > …)
//   vitest.txt, typecheck.txt                   cd web && pnpm test, pnpm typecheck
//   cargo.txt, clippy.txt, fmt.txt              cargo test --workspace, clippy -D warnings, fmt --check
//   tauri/tauri-e2e.json, tauri/img/*.png       node web/e2e-tauri/desktop.e2e.mjs --only T16 --out reports/tmp/report18/tauri
//   wasm.json (optional)                        {"base", "before": {raw, gzip}, "after": {raw, gzip}}
//
//   node tools/report/report18.mjs
//   node tools/report/print-pdf.mjs docs/reports/18-skizzen-editor/report.html docs/reports/18-skizzen-editor/report.pdf "Testbericht 18: Skizzen-Editor"

import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import os from "node:os";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "../..");
const input = join(root, "reports/tmp/report18");
const out = join(root, "docs/reports/18-skizzen-editor");
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
const de = (x, d = 0) => (x === null || x === undefined || Number.isNaN(Number(x)) ? "–" : Number(x).toLocaleString("de-DE", { minimumFractionDigits: d, maximumFractionDigits: d }));
const md = (t) => esc(t).replace(/`([^`]+)`/g, "<code>$1</code>");
const pre = (t) => `<pre>${esc(String(t).trimEnd())}</pre>`;

function img(srcPath, dst, width = 1440) {
  if (!existsSync(srcPath)) return null;
  if (srcPath.endsWith(".png") && dst.endsWith(".jpg")) {
    try {
      execFileSync("convert", [srcPath, "-resize", `${width}x>`, "-quality", "86", join(imgDir, dst)]);
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
const fig = (s, cap) => (s ? `<figure><img src="${s}" alt=""><figcaption>${md(cap)}</figcaption></figure>` : `<p class="small">(Bild fehlt: ${esc(cap)})</p>`);

// ------------------------------------------------------------------------------- data

const scenarios = readJson(join(input, "scenarios.json")) ?? [];
const byId = Object.fromEntries(scenarios.map((s) => [s.id, s]));
const metrics = readJson(join(input, "metrics.json")) ?? {};
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
const NEW = /sketch_edit::|sketch_editor|sweep_continues|intersections_of_lines|step_rounding|dragging_a_sketch_point|sketch_op_info/;
const newTests = cargo ? [...cargo.matchAll(/^test (\S+) \.\.\. (ok|FAILED)$/gm)].map((m) => ({ name: m[1], res: m[2] })).filter((t) => NEW.test(t.name) || /^(new_sketch|line_|grid_points|snaps_by|tangent_line|arcs_bezier|center_arc|corner_tool|close_and|hole_blocks|plane_of|anchors_are|segment_under|draws_lines|sketch_under|syntax_errors_stop)/.test(t.name)) : [];
const clippy = readText(join(input, "clippy.txt"));
const fmt = readText(join(input, "fmt.txt"));
const clippyOk = clippy != null && !/^(error|warning)/m.test(clippy);
const fmtOk = fmt != null && fmt.trim() === "";
const pwText = readText(join(input, "playwright.txt"));
const pwSum = pwText ? { passed: +(/(\d+) passed/.exec(pwText)?.[1] ?? 0), failed: +(/(\d+) failed/.exec(pwText)?.[1] ?? 0), flaky: +(/(\d+) flaky/.exec(pwText)?.[1] ?? 0), skipped: +(/(\d+) skipped/.exec(pwText)?.[1] ?? 0) } : null;
const pwFailed = pwText ? [...pwText.matchAll(/^\s+\d+\) \[chromium\] › (.+)$/gm)].map((m) => m[1].trim()) : [];
const vitest = readText(join(input, "vitest.txt"));
const vitestSum = vitest ? (/Tests\s+(\d+) passed/.exec(vitest)?.[1] ?? null) : null;
const typecheck = readText(join(input, "typecheck.txt"));
const tauri = readJson(join(input, "tauri", "tauri-e2e.json"));
const t16 = tauri?.results?.find((r) => r.id === "T16") ?? null;
const wasm = readJson(join(input, "wasm.json"));
const hover = metrics.hover ?? null;

const date = new Date().toLocaleDateString("de-DE", { day: "2-digit", month: "2-digit", year: "numeric" });
const commit = git("rev-parse", "--short", "HEAD");
const branch = git("rev-parse", "--abbrev-ref", "HEAD");
const rustc = (() => {
  try {
    return execFileSync("rustc", ["--version"], { encoding: "utf8" }).trim();
  } catch {
    return "–";
  }
})();

/** One scenario: caption, code next to the screenshots, checks. */
function scen(id, opts = {}) {
  const s = byId[id];
  if (!s) return `<p class="small">(Szenario ${esc(id)} fehlt)</p>`;
  const images = (s.images ?? []).map((i) => fig(pw(i.name), i.label)).join("");
  const code = s.code ? pre(s.code) : "";
  const body = code && !opts.wide ? `<div class="codefig"><div>${code}</div><div>${images}</div></div>` : `${images}${code}`;
  return `<h3>${esc(s.id)} ${esc(s.title)}</h3><p>${md(s.caption)}</p>${body}<ul class="checks">${(s.checks ?? []).map((c) => `<li>${md(c)}</li>`).join("")}</ul>`;
}

const html = `<!doctype html>
<html lang="de">
<head>
<meta charset="utf-8">
<title>Testbericht 18 – Skizzen-Editor</title>
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
  ul.checks li { font-size: 8.2pt; }
  tr, td, figure, pre { break-inside: avoid; }
  .ok { color: var(--ok); font-weight: 600; } .bad { color: var(--bad); font-weight: 600; }
  figure { margin: 0 0 2mm; }
  figure img { max-width: 100%; max-height: 110mm; border: 1px solid var(--border); border-radius: 1mm; background: #fff; }
  figcaption { font-size: 7.8pt; color: var(--muted); }
  .codefig { display: grid; grid-template-columns: 0.85fr 1.15fr; gap: 3mm; align-items: start; break-inside: avoid; }
  .note { border-left: 1mm solid #e8b84a; padding: 1mm 2.5mm; background: #fff8e6; margin: 2mm 0; }
</style>
</head>
<body>
<main>
<h1>Testbericht 18 – Skizzen-Editor</h1>
<div class="meta">Plan 17.7, Schritt 4: Zeichenmodus mit Raster in Browser und Tauri · cad-eval (<code>sketch_edit.rs</code>, Zeichenzustand der Skizzen), cad-session (Protokoll <code>Sketch</code>), Web-UI (<code>app/sketch.ts</code>, <code>sketchUi.tsx</code>, <code>logic/sketchEditor.ts</code>, Viewer) · ${esc(date)} · Branch ${esc(branch)} · Commit ${esc(commit)} · ${esc(rustc)} · ${os.cpus().length} Kerne</div>

<div class="summary">
  <div>Werkzeuge<b class="ok">9 + 2</b>Linie, 3 Bögen, Bézier, Rechteck, Kreis, Ecke runden, Auswahl; Schließen, Löschen</div>
  <div>cargo test --workspace<b class="${cargoSum && cargoSum.failed === 0 ? "ok" : "bad"}">${cargoSum ? `${cargoSum.ok} / ${cargoSum.ok + cargoSum.failed}` : "–"}</b>davon ${newTests.length} neu</div>
  <div>clippy · fmt<b class="${clippyOk && fmtOk ? "ok" : "bad"}">${clippy == null ? "–" : clippyOk ? "sauber" : "Befund"} · ${fmt == null ? "–" : fmtOk ? "sauber" : "Abw."}</b>-D warnings</div>
  <div>Vitest · Playwright<b class="${pwSum && pwSum.failed === 0 ? "ok" : "bad"}">${vitestSum ?? "–"} · ${pwSum ? `${pwSum.passed} / ${pwSum.passed + pwSum.failed}` : "–"}</b>neu: Z01–Z17</div>
  <div>Tauri T16<b class="${t16?.ok ? "ok" : "bad"}">${t16 ? (t16.ok ? "bestanden" : "fehlgeschlagen") : "–"}</b>${t16 ? `${t16.checks.length} Prüfungen, nativer Kernel` : ""}</div>
  <div>Snap + Vorschau<b>${hover ? `${de(hover.medianMs, 1)} ms` : "–"}</b>Median je Mausbewegung (Browser, ${hover ? hover.segments : "–"} Segmente)</div>
</div>

<h2>1 Auftrag und Ergebnis</h2>
<p>Plan 17.6: Werkzeuge in der Ansicht, Zeichnen in der Skizzenebene mit Raster, Snap mit Anzeige, jede Aktion schreibt Code in den <code>sketch</code>-Block am Cursor oder legt einen an, Undo wie beim Ziehen, Bearbeiten über Griffe, Segment löschen; der Code bleibt die einzige Quelle. Umgesetzt:</p>
<ul>
<li><b>Zeichenmodus</b> (Schalter „Skizze“): Kamera senkrecht auf die Skizzenebene – auch unter Transformationen, im <code>rotate_extrude</code> (xz-Ebene) und im zentrierten <code>linear_extrude</code> (Boden); Schrittraster, Rasterlinien und -punkte des Moduls oder der Skizze in der Ebene; Werkzeugleiste, Statuszeile, Esc beendet.</li>
<li><b>Werkzeuge</b> Linie (Kette, Klick auf den Anfang schließt), Bogen durch drei Punkte, Bogen um einen Mittelpunkt, tangentialer Bogen, Bézier (kubisch), Rechteck, Kreis, Ecke runden (<code>corner(r)</code>), Schließen, Löschen, Auswahl. Jede Aktion ist ein Text-Edit in der Sprache, die es schon gibt, und ein Undo-Schritt.</li>
<li><b>Snap</b> auf Rasterpunkte und Anker (Referenzen wie <code>g[b, u]</code>, <code>k.ecke</code>), Anfang, Ecken, End-, Mittel- und Schnittpunkte, Bogenmitten, tangential (<code>line(length = …)</code>), horizontal/vertikal, Rasterlinien (<code>[g.b, 40]</code>), Schrittweite; Alt = frei mit Schrittweite. Die Markierung zeigt die Art, die Beschriftung den Text, der geschrieben würde.</li>
<li><b>Bearbeiten:</b> mit „Auswahl“ die Griffe aller Segmente (bestehende Griff-Logik: Literal ändern, Rasterreferenz umschreiben), Klick auf ein Segment setzt den Cursor, Entf löscht.</li>
<li><b>Logik in Rust:</b> Skizze und Einfügestelle, Zeichenzustand, Snap, Code, Vorschau, Löschen, Ecken, Raster, Ebene und Strahlschnitt in <code>cad-eval</code>/<code>cad-session</code>; die UI reicht Maus und Tasten weiter und wendet die Antwort an. Dasselbe über <code>WorkerTransport</code> (Wasm) und <code>TauriTransport</code> (nativer Engine-Prozess).</li>
</ul>

<h2>2 Entscheidungen</h2>
<table>
<tr><th>Frage</th><th>Entscheidung</th></tr>
<tr><td>Wohin der Code geht</td><td>Skizze = die <code>sketch</code>-Anweisung, die den Cursor enthält oder auf seiner Zeile beginnt. Einfügen nach der Anweisung der Cursorzeile (bzw. der letzten davor), im innersten <code>hole</code>-Block; auf der <code>sketch {</code>-Zeile ans Ende; leere Blöcke werden aufgeklappt. Ohne Skizze: neuer <code>sketch { … }</code>-Block am Ende des Kontextmoduls bzw. der Datei. Nach jeder Aktion steht der Cursor hinter dem neuen Code.</td></tr>
<tr><td>Aktueller Punkt</td><td>Aus der Auswertung: <code>SketchInfo::steps</code> hält jede Segmentanweisung in Auswertungsreihenfolge mit dem Zustand danach (Kontur, Anfang, Punkt, Richtung; Geometrie vor <code>corner()</code>) – auch für Schleifen und Modulaufrufe im Block. Linie, Bögen, Bézier setzen die offene Kontur fort, wenn ihr erster Punkt deren Ende ist; sonst <code>start(p)</code>. Eine neue Kontur mitten in einer weiterlaufenden kommt hinter deren Ende.</td></tr>
<tr><td>Zustand der Werkzeuge</td><td>Die Engine ist ohne Zustand: die gewählten Punkte (Position, Text, Art) gehen mit jeder Anfrage hin und mit der Antwort zurück. Höchstens eine Anfrage unterwegs, die letzte Mausposition gewinnt, Klicks in Reihenfolge.</td></tr>
<tr><td>Undo</td><td>Eine Transaktion je Aktion mit <code>isolateHistory</code> (auch <code>start</code> + erste Linie zusammen); die Quelle geht sofort an die Engine, die Nachrichten in Reihenfolge bearbeitet – die nächste Anfrage sieht den neuen Text. Strg+Z wirkt auch mit dem Fokus in der Ansicht.</td></tr>
<tr><td>Referenzen statt Zahlen</td><td>Rasterpunkte und Anker schreiben ihre Referenz; ein Rechteck aus zwei Rasterpunkten bekommt auch die anderen Ecken als Rasterpunkte. Freie Punkte: Zahlen auf die Schrittweite gerundet; exakte Punkte (End-, Schnittpunkte …): kürzeste Zahl mit 6 Stellen. 2D-Punkte schreiben <code>g[a, b]</code> (auch beim Griff-Snap, statt <code>g[a, b, _]</code>).</td></tr>
<tr><td>Ebene</td><td><code>sketch_frame</code>: Knotensystem, im <code>rotate_extrude</code> die xz-Ebene, beim zentrierten <code>linear_extrude</code> der Boden – für Editor, Griffe (<code>Handle.frame</code>) und Darstellung (<code>OpInfo::Sketch.plane</code>).</td></tr>
<tr><td>Griffe und Werkzeuge</td><td>Zeichenwerkzeuge zeichnen auch auf bestehenden Punkten (dort keine Griffe, die den Klick abfangen); Griffe aller Segmente mit „Auswahl“.</td></tr>
<tr><td>Toleranz</td><td>Snap-Radius in Pixeln (Einstellungen, 14 px) mal Pixelgröße am Treffpunkt des Strahls; die UI schickt nur Strahl, Pixelgröße und Radius.</td></tr>
</table>

<h2>3 Ablauf</h2>
${pre(`Maus bewegt     → Sketch { cursor, tool, points, ray, pixel, snap_px, free }        (≤ 1 unterwegs)
                 ← Sketch { plane, snap {point, text, kind, label}, guides, preview, prompt }
Klick           → Sketch { …, click: true }
                 ← points (Punkt gemerkt)  oder  edits + select (Aktion fertig)
                   UI: eine Transaktion (Undo-Schritt), Cursor hinter den Code, SetSource sofort
Schließen/Entf  → Sketch { tool: close | delete, cursor }  ← edits

Engine (cad-eval::sketch_edit, Session):  place → Skizze, Einfügestelle, Zeichenzustand, Raster/Anker
                                          snap → Punkt + Text + Art     click → Code   preview → Polylinien
UI (app/sketch.ts, Viewer):               Strahl, Tasten, Antwort anwenden, zeichnen, Kamera`)}

<h2>4 Werkzeuge</h2>
<p>Jedes Szenario zeichnet mit der Maus (Playwright, echte Zeigerereignisse) und prüft den geschriebenen Code wörtlich; links der Code nach der Aktion, rechts die Ansicht.</p>
${scen("Z02")}
${scen("Z03")}
${scen("Z04")}
${scen("Z05")}
${scen("Z06")}
${scen("Z07")}
${scen("Z08")}
${scen("Z09")}
${scen("Z10")}

<h2>5 Snap</h2>
${scen("Z11")}

<h2>6 Bearbeiten: Undo, Griffe, Löschen</h2>
${scen("Z12")}
${scen("Z13")}
${scen("Z14")}

<h2>7 Zeichenmodus, Ebenen, Thema</h2>
${scen("Z01")}
${scen("Z15")}
${scen("Z16", { wide: true })}

<h2>8 Desktop-App (Tauri)</h2>
<p>Szenario T16 in <code>web/e2e-tauri/desktop.e2e.mjs</code> (tauri-driver, WebKitWebDriver, Xvfb): die echte App <code>cad</code> mit dem nativen Engine-Prozess, Nachrichten über den <code>TauriTransport</code>. Die Ansicht wird mit Zeigerereignissen auf dem Canvas bedient (WebDriver hat keine Maus über WebGL-Inhalten, die sich verlässlich bewegen ließe).</p>
${t16 ? `<p>Ergebnis: <span class="${t16.ok ? "ok" : "bad"}">${t16.ok ? "bestanden" : "fehlgeschlagen"}</span> in ${de(t16.ms / 1000, 1)} s${tauri?.metrics?.sketchTauri?.hoverMedianMs != null ? `, Snap + Vorschau Median ${de(tauri.metrics.sketchTauri.hoverMedianMs, 1)} ms je Bewegung` : ""}.</p><ul class="checks">${t16.checks.map((c) => `<li class="${c.ok ? "" : "bad"}">${md(c.text)}</li>`).join("")}</ul>${(t16.images ?? []).map((i) => fig(tauriImg(i.name), i.label)).join("")}${t16.error ? pre(t16.error) : ""}` : "<p>(fehlt)</p>"}

<h2>9 Messwerte</h2>
${scen("Z17", { wide: true })}
${wasm ? `<h3>Wasm-Modul</h3><table><tr><th></th><th>roh</th><th>gzip -9</th></tr><tr><td><code>cad_wasm_bg.wasm</code> vorher (Commit ${esc(wasm.base)})</td><td class="n">${de(wasm.before.raw)} B</td><td class="n">${de(wasm.before.gzip)} B</td></tr><tr><td>mit Skizzen-Editor</td><td class="n">${de(wasm.after.raw)} B</td><td class="n">${de(wasm.after.gzip)} B</td></tr><tr><td>Zuwachs</td><td class="n">${de(wasm.after.raw - wasm.before.raw)} B</td><td class="n">${de(wasm.after.gzip - wasm.before.gzip)} B</td></tr></table>` : ""}

<h2>10 Testläufe</h2>
<table>
<tr><th>Lauf</th><th>Ergebnis</th></tr>
<tr><td><code>cargo test --workspace -- --test-threads=2</code></td><td>${cargoSum ? `${cargoSum.ok} bestanden, ${cargoSum.failed} fehlgeschlagen` : "–"}</td></tr>
<tr><td><code>cargo clippy --workspace --all-targets -- -D warnings</code> · <code>cargo fmt --check</code></td><td>${clippyOk ? "sauber" : "Befund"} · ${fmtOk ? "sauber" : "Abweichung"}</td></tr>
<tr><td><code>pnpm typecheck</code> · <code>pnpm test</code> (Vitest)</td><td>${typecheck !== null ? (/error TS/i.test(typecheck) ? "Fehler" : "sauber") : "–"} · ${vitestSum ?? "–"} bestanden</td></tr>
<tr><td><code>pnpm e2e</code> (Playwright, Chromium, ganze Suite)</td><td>${pwSum ? `${pwSum.passed} bestanden, ${pwSum.failed} fehlgeschlagen${pwSum.skipped ? `, ${pwSum.skipped} übersprungen` : ""}${pwFailed.length ? `: ${pwFailed.map(esc).join("; ")}` : ""}` : "–"}</td></tr>
<tr><td>Tauri-E2E T16</td><td>${t16 ? (t16.ok ? "bestanden" : "fehlgeschlagen") : "–"}</td></tr>
</table>
<p>Neue Rust-Tests (${newTests.length}):</p>
<p class="small">${newTests.map((x) => `<code>${esc(x.name)}</code> ${x.res === "ok" ? '<span class="ok">ok</span>' : '<span class="bad">FAILED</span>'}`).join(" · ")}</p>
<p class="small">cad-eval <code>tests/sketch_edit.rs</code>: Einfügestelle, neue Skizze in Datei und Kontextmodul, Ketten, neue Kontur hinter einer weiterlaufenden, Snap-Arten, Schnittpunkte, tangential, Rechteck mit Rasterreferenzen, Bögen, Bézier, Kreis, Mittelpunktsbogen auf Rasterpunkten, Ecken einfügen und ändern, Schließen, Löschen, <code>hole</code>- und leere Blöcke, Ebenen, Anker, Segment unter der Maus, Griff-Snap in Kurzform; Unit-Tests im Modul (Bogenwinkel über 180°, Schnitte, Rundung). cad-session <code>tests/sketch_editor.rs</code>: Protokoll mit Strahl (neue Skizze, Kette, Schließen, Löschen), Ebene unter Transformation, Syntaxfehler, Auswahl, <code>OpInfo.plane</code>. Vitest <code>logic/sketchEditor.test.ts</code>: Raster, Kamera, Markierungen, Undo-Schritt mit Cursor.</p>

<h2>11 Grenzen</h2>
<ul>
<li>Nur die Hauptdatei: in einer anderen Projektdatei startet der Modus nicht; Skizzen aus eingebundenen Dateien sind kein Ziel.</li>
<li>Anweisungen in Schleifen oder Modulen werden weder gelöscht noch bekommt ihre Ecke ein <code>corner()</code> (Meldung). Der Zeichenzustand mitten in einer Schleife ist der nach ihrem letzten Durchlauf.</li>
<li>Snap nur auf die Skizze, sichtbare Raster und Anker – nicht auf Körperkanten oder andere Skizzen; horizontal/vertikal nur zum vorigen Punkt.</li>
<li>Bézier nur kubisch, kein Spline-Werkzeug; <code>line(by = …)</code> und <code>length</code> + <code>angle</code> entstehen nicht (außer tangential <code>line(length = …)</code>).</li>
<li>Eine neue Skizze ist eine bloße 2D-Skizze (zum Körper: z. B. <code>linear_extrude</code> davor schreiben).</li>
<li>Die Kamera behält „z oben“ der Bahnsteuerung: in gedrehten Ebenen steht die y-Achse der Skizze nicht unbedingt senkrecht auf dem Bildschirm. Mit einem Zeichenwerkzeug dreht die linke Maustaste die Kamera nicht (rechts verschieben, Rad zoomen, „Auswahl“ zum Drehen).</li>
<li>Tauri: T16 bedient die Ansicht mit synthetischen Zeigerereignissen.</li>
</ul>
</main>
</body>
</html>
`;
writeFileSync(join(out, "report.html"), html);
console.log(`${join(out, "report.html")}: ${html.length} Zeichen, ${scenarios.length} Szenarien`);
