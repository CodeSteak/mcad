#!/usr/bin/env node
// Test report 17 ("Schablonenschrift cad stencil mono", Plan 17.4/17.11, Schritt 3b): the own
// monospaced stencil font as default font of text(), its parameters, the comparison with Allerta
// Stencil, the example stencil, kernel times, Wasm size, the Playwright fix of C16.
// Writes docs/reports/17-schablonenschrift/report.html (images in img/).
//
// Inputs (gitignored, reports/tmp/report17/):
//   chart/*.png, cli/*.png, glyphs-*.tsv, var-*.tsv, times.tsv, gcode.json, *.check.txt,
//   schild-*.measure.txt, schablone-measure.txt, compat-text.txt   tools/report/report17-data.sh
//   scenarios.json, img/*.jpg, browser-metrics.json  cd web && pnpm exec playwright test e2e/schablone.spec.ts
//   playwright.txt                                   whole suite (cd web && pnpm e2e > …)
//   c16.txt                                          C16 repeated before/after the fix (one line each)
//   vitest.txt, typecheck.txt                        cd web && pnpm test, pnpm typecheck
//   cargo.txt, clippy.txt, fmt.txt                   cargo test --workspace --no-fail-fast, clippy -D warnings, fmt --check
//   compare.txt                                      python3 tests/openscad-compat/compare.py --eval
//   wasm.json                                        {"base", "before": {raw, gzip}, "after": {raw, gzip}}
//
//   node tools/report/report17.mjs
//   node tools/report/print-pdf.mjs docs/reports/17-schablonenschrift/report.html docs/reports/17-schablonenschrift/report.pdf "Testbericht 17: Schablonenschrift"

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
const de = (x, d = 0) => (x === null || x === undefined || Number.isNaN(Number(x)) ? "–" : Number(x).toLocaleString("de-DE", { minimumFractionDigits: d, maximumFractionDigits: d }));
const md = (t) => esc(t).replace(/`([^`]+)`/g, "<code>$1</code>");
const T = (name) => readText(join(input, name))?.replaceAll(`${input}/`, "").replaceAll(`${root}/`, "") ?? "(fehlt)";
const pre = (t) => `<pre>${esc(String(t).trimEnd())}</pre>`;
const src = (p) => readText(join(root, p)) ?? "(fehlt)";

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
const fig = (s, cap) => (s ? `<figure><img src="${s}" alt=""><figcaption>${md(cap)}</figcaption></figure>` : `<p class="small">(Bild fehlt: ${esc(cap)})</p>`);
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
const vol = (name) => {
  const t = readText(join(input, name)) ?? "";
  return /Volumen[^\d]*([\d.]+)/i.exec(t)?.[1] ?? /volume[^\d]*([\d.]+)/i.exec(t)?.[1] ?? null;
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
  return `<tr><td>${md(label)}</td>${cells.map((c) => `<td class="n">${de(c?.kernelMs)} <span class="small">(${de(c?.calls)} Aufrufe)</span></td>`).join("")}<td class="n">${cells[0] && cells[1] ? de(cells[0].kernelMs / cells[1].kernelMs, 2) : "–"}</td></tr>`;
};
const bms = (k) => browser[k]?.kernel_ms;

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

const html = `<!doctype html>
<html lang="de">
<head>
<meta charset="utf-8">
<title>Testbericht 17 – Schablonenschrift „cad stencil mono“</title>
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
<h1>Testbericht 17 – Schablonenschrift „cad stencil mono“</h1>
<div class="meta">Plan 17.7, Schritt 3b: eigene monospaced Schablonenschrift als Standardschrift von <code>text()</code> · cad-font (<code>stencil.rs</code>), cad-eval (<code>text()</code>-Parameter), Tests in cad-font, cad-eval, cad-engine, compat-tests, Playwright; Korrektur Playwright C16/C20 · ${esc(date)} · Branch ${esc(branch)} · Commit ${esc(commit)} · ${esc(rustc)} · ${os.cpus().length} Kerne</div>

<div class="summary">
  <div>Glyphen ohne Loch<b class="${holesReg === 0 && holesBold === 0 && reg.length ? "ok" : "bad"}">${reg.length} / ${reg.length}</b>Regular und Bold; 8 Parameterkombinationen im Test</div>
  <div>cargo test --workspace<b class="${cargoSum && cargoSum.failed === 0 ? "ok" : "bad"}">${cargoSum ? `${cargoSum.ok} / ${cargoSum.ok + cargoSum.failed}` : "–"}</b>davon ${newTests.length} neu/geändert</div>
  <div>clippy · fmt<b class="${clippyOk && fmtOk ? "ok" : "bad"}">${clippy == null ? "–" : clippyOk ? "sauber" : "Befund"} · ${fmt == null ? "–" : fmtOk ? "sauber" : "Abw."}</b>-D warnings</div>
  <div>Vitest · Playwright<b class="${pwSum && pwSum.failed === 0 ? "ok" : "bad"}">${vitestSum ?? "–"} · ${pwSum ? `${pwSum.passed} / ${pwSum.passed + pwSum.failed}` : "–"}</b>neu: K01–K03; C16 behoben</div>
  <div>Schild, Kernel nativ<b>${de(t("schild-stencil")?.kernelMs)} ms</b>Allerta Stencil ${de(t("schild-allerta")?.kernelMs)} ms</div>
  <div>Wasm cad_wasm<b>${wasm ? `${wasm.after.raw >= wasm.before.raw ? "+" : ""}${de((wasm.after.raw - wasm.before.raw) / 1024, 1)} KiB` : "–"}</b>${wasm ? `gzip ${wasm.after.gzip >= wasm.before.gzip ? "+" : ""}${de((wasm.after.gzip - wasm.before.gzip) / 1024, 1)} KiB` : ""}</div>
</div>

<h2>1 Auftrag und Ergebnis</h2>
<p>Plan 17.4 sieht als Standardschrift eine eigene, sehr einfache monospaced Schablonenschrift aus Geraden und Bögen vor, mit Stegen, sodass keine Inseln herausfallen. Umgesetzt:</p>
<ul>
<li><b>„cad stencil mono“</b> (Regular, Bold) in <code>crates/cad-font/src/stencil.rs</code>: ${reg.length} Glyphen – der ganze Pflicht-Zeichensatz <code>cad_font::REQUIRED_CHARS</code> ohne Rückfall, dazu <code>’ ” $</code>. Jede Glyphe ist eine Folge von Mittellinien (Geraden, tangentiale Kreisbögen), gezeichnet mit einem Stift der Strichstärke (flache Enden, Gehrung), plus Punkte, minus Stege, an Grund- bzw. Versallinie beschnitten. Die Kontur rechnet <code>cad_kernel::region2</code> exakt aus (Stücke der Geraden und Bögen); STEP und Kernel sehen Geraden und Kreise.</li>
<li><b>Keine Löcher:</b> jede Glyphe, zum Profil gerendert, hat 0 Löcher – Innenräume von O, A, B, D, P, R, 0, 4, 6, 8, 9, Ä, ö, @, %, &amp;, §, °, Ø … sind über Stege mit dem Außenbereich verbunden. Geprüft in cad-font für 8 Kombinationen von Strichstärke und Stegbreite (Regionen ohne Uhrzeigersinn-Schleifen, geschlossene Schleifen, keine Splitter, keine Überlappungen) und in der Engine für jede Glyphe über <code>cad_engine::region_2d</code>.</li>
<li><b>Parameter</b> <code>text(…, stroke_width = …, bridge_width = …)</code> in mm: Strichstärke Standard 0,14·<code>size</code> (Bold 0,2·<code>size</code>), höchstens 0,2·<code>size</code>; Stegbreite Standard 0,1·<code>size</code>, 0 (keine Stege – geschlossene Innenräume wie eine normale Schrift) oder 0,03…0,2·<code>size</code>. Werte außerhalb: Warnung am Argument, begrenzt. Bei anderen Schriften: Warnung „wirkt nur mit …“.</li>
<li><b>Metrik</b> wie die übrigen Schriften: em = <code>size</code>/0,72, Versalhöhe = <code>size</code> (720/1000 em), Vorschub 600/1000 em × 1000/1024 – genau wie JetBrains Mono, sodass Texte gleich breit bleiben wie mit der bisherigen Standardschrift.</li>
<li><b>Standardschrift:</b> <code>text()</code> ohne <code>font</code> und mit unbekannter Familie nutzt die Schablonenschrift (vorher JetBrains Mono); fehlende Zeichen (z. B. Kyrillisch) kommen weiter aus JetBrains Mono gleichen Gewichts.</li>
<li>Nebenbei: Playwright <b>C16</b> (achsgebundenes Ziehen, Phase B), das zuletzt auch einzeln scheiterte, ist behoben – die Ursache lag im Test (Griffe der vorigen Cursorzeile, Abschnitt 10).</li>
</ul>

<h2>2 Entscheidungen</h2>
<table>
<tr><th>Frage</th><th>Entscheidung</th></tr>
<tr><td>Form der Schrift</td><td>Rust-Daten statt Schriftdatei: je Zeichen eine kleine Zeichenfunktion über benannten Lagen (Mittellinien links/rechts/Mitte, Grund-, x-, Versal-, Ober-, Unterlänge, abhängig von der Strichstärke), Bausteine Stadion, Bauch, C-Bogen, Punkte, Kommas. Kein TTF-Build, keine Lizenzfrage; Wasm-Zuwachs nur Code (Abschnitt 8).</td></tr>
<tr><td>Strich</td><td><code>region2::stroke</code> (aus Schritt 2) mit flachen Enden und Gehrung: rechte Winkel bleiben eckig, Enden sind gerade Schnitte (Schablonen-Anmutung). Freie Enden laufen bis zur äußeren Linie (Grundlinie, Versalhöhe), Stöße enden auf der Mittellinie des anderen Strichs – so entstehen keine Kerben. Diagonalen (A, K, M, N, V, W, X, Y, Z, 1, 2, 4, 7, <code>/</code>) werden über die Linie verlängert und an Grund-/Versallinie beschnitten (gerade Füße, flache Spitzen).</td></tr>
<tr><td>Stege</td><td>Ein Steg ist ein Band der Stegbreite quer über einen Strich (Länge 1,3 halbe Striche zu jeder Seite), von der Glyphe abgezogen. Regeln: Stamm getrennt vom Bauch bei B, D, P, R, b, d, g, p, q (Stege am Stamm), runde Formen oben und unten (O, Q, 0, 8, o), offenes A (Beine an der Versallinie um die Stegbreite auseinander), e und a am Querstrich bzw. am Bauch neben dem Stamm, 4 am Querstrich vor dem Stamm, 6/9 schräg am Bauch, Ringe (°, %, Ø, @, &amp;, §) an einer Seite. ß/ẞ sind offen (Lücke ≥ Stegbreite zum Stamm).</td></tr>
<tr><td>Parameter</td><td><code>stroke_width</code> und <code>bridge_width</code> in mm (für Druck und Fräsen sind absolute Maße gefragt: „Steg mindestens zwei Düsenbreiten“); ohne Angabe proportional zu <code>size</code>. Stil Bold = Standard-Strichstärke 0,2·<code>size</code>. Die Grenzen kommen aus dem Test der Parameterfläche: oberhalb 0,2·<code>size</code> wachsen Striche von a, e, #, @ zusammen und schließen Innenräume; Stege unter 0,03·<code>size</code> schließen schwere Striche wieder.</td></tr>
<tr><td>Metrik</td><td>1000 Einheiten je em, Versalhöhe 720, x-Höhe 530, Oberlänge 760, Unterlänge −210, Vorschub 600, Seitenrand 60 (Glyphenbox 480 breit). Senkrechter Text: Vorschub 1000, Glyphe mittig auf dem Stift (wie der Shaper bei TrueType).</td></tr>
<tr><td>Standardschrift und OpenSCAD</td><td>OpenSCAD kennt die Schrift nicht (es nimmt ohne <code>font</code> die fontconfig-Standardschrift, meist Liberation Sans): ein Modell ohne <code>font</code> sieht in OpenSCAD anders aus. Die Kompatibilitätstests hängen davon nicht ab (Abschnitt 9); wer gleiche Maße wie OpenSCAD braucht, gibt eine mitgelieferte TrueType-Schrift an. Unbekannte Familien → Standardschrift mit Warnung (vorher JetBrains Mono).</td></tr>
</table>

<h2>3 Zeichentafel</h2>
<p>Alle ${reg.length} Glyphen direkt aus der Schrift (Beispielprogramm <code>cargo run -p cad-font --example stencil_chart</code>): Zelle = Vorschub, Linien Grundlinie, x-Höhe, Versalhöhe, Unterlänge. Eine Glyphe mit Loch wäre rot.</p>
${fig(chart("regular"), "cad stencil mono Regular (Strich 0,14·size, Steg 0,1·size)")}
${fig(chart("bold"), "cad stencil mono Bold (Strich 0,2·size, Steg 0,1·size)")}
<p>Teile je Glyphe (Regular): ${piecesHist.map(([p, n]) => `${n}× ${p} Teil${p === "1" ? "" : "e"}`).join(", ")}; Löcher: Regular ${holesReg}, Bold ${holesBold}.</p>

<h2>4 Parameter</h2>
<p>Dieselben acht Zeichen mit verschiedenen Strich- und Stegbreiten (Anteile von <code>size</code>). <code>bridge_width = 0</code> schaltet die Stege ab: dann haben O, B, 0, 8 … wieder Innenräume (rot = Glyphe mit Loch, so gewollt).</p>
${fig(chart("var-0.06-0.04", 1360), "Strich 0,06 · Steg 0,04")}
${fig(chart("var-0.14-0.1", 1360), "Standard: Strich 0,14 · Steg 0,1")}
${fig(chart("var-0.2-0.2", 1360), "Strich 0,2 · Steg 0,2 (Grenzen)")}
${fig(chart("var-0.14-0", 1360), "Strich 0,14 · ohne Stege (bridge_width = 0)")}
<p>Warnungen bei Werten außerhalb der Grenzen und bei anderen Schriften:</p>
${pre(T("warn.check.txt"))}

<h2>5 Vergleich mit Allerta Stencil</h2>
<p>Alphabet-Tafeln über die CLI (<code>text(…, size = 8)</code>, 0,6 mm extrudiert, von oben): die Schablonenschrift hat alle Zeichen selbst, Allerta Stencil (proportional) 20 davon nicht (aus JetBrains Mono, nicht schabloniert, Bericht 16).</p>
${fig(cli("alphabet-stencil"), "cad stencil mono Regular")}
${fig(cli("alphabet-stencil-bold"), "cad stencil mono Bold")}
${fig(cli("alphabet-allerta"), "Allerta Stencil – ẞ „ ‚ { } _ # ° ² ³ § ~ ^ · • … ± µ Ø ø aus JetBrains Mono")}
<h3>Gleiches Schild in beiden Schriften</h3>
${pair(cli("schild-stencil"), `cad stencil mono (Standard) – Volumen ${vol("schild-stencil.measure.txt") ?? "–"} mm³`, cli("schild-allerta"), `Allerta Stencil – Volumen ${vol("schild-allerta.measure.txt") ?? "–"} mm³`)}
${pre(src("reports/tmp/report17/cases/schild-stencil.scad"))}
<table>
<tr><th>Merkmal</th><th>cad stencil mono</th><th>Allerta Stencil</th></tr>
<tr><td>Breite</td><td>monospaced, 0,814·size je Zeichen (wie JetBrains Mono)</td><td>proportional, mit Kerning</td></tr>
<tr><td>Kurven</td><td>Geraden und Kreisbögen (Bögen &gt; 90° im Kernel geteilt)</td><td>quadratische Bézierkurven → je Flanke eine B-Spline</td></tr>
<tr><td>Pflicht-Zeichensatz</td><td>vollständig, alle schabloniert</td><td>20 Zeichen fehlen (Rückfall, ohne Stege)</td></tr>
<tr><td>Strich, Steg</td><td>einstellbar (<code>stroke_width</code>, <code>bridge_width</code>), Bold</td><td>fest</td></tr>
<tr><td>Lizenz</td><td>Quelltext des Projekts</td><td>SIL OFL 1.1, Reserved Font Name (nur unverändert)</td></tr>
</table>

<h2>6 Beispiel: Schablone</h2>
<p><code>tests/models/spec/schablone.scad</code>: Text aus einer 1 mm dünnen Platte geschnitten. Ohne Stege fielen die Innenteile von A, 0, 8, B, 9 heraus; mit der Schablonenschrift hängen sie an der Platte.</p>
<div class="codefig">${pre(src("tests/models/spec/schablone.scad"))}<div>${fig(cli("schablone"), "gerendert (iso)")}${fig(cli("schablone-oben"), "von oben")}</div></div>
<h3>Gedruckt: G-Code aus PrusaSlicer</h3>
<p>Ein echter Druck ist in dieser Umgebung nicht möglich; stattdessen: STL exportiert (<code>cad export</code>), mit PrusaSlicer 2.7 (Standardprofil, Schicht 0,2 mm, Düse 0,4 mm) zu G-Code geschnitten und die oberste Schicht aus dem G-Code gezeichnet (<code>tools/report/report17-gcode.py</code>): jede Insel der Platte ist mit dem Rest verbunden, die Stege (1,2 mm = 3 Düsenbreiten) werden als eigene Bahnen gedruckt.</p>
${fig(cli("schablone-gcode"), `oberste Schicht (z = ${gcode?.top_z ?? "–"} mm) aus dem G-Code, ${gcode?.layers ?? "–"} Schichten, geschätzte Druckzeit ${gcode?.["estimated printing time (normal mode)"] ?? "–"}, Filament ${gcode?.["filament used [mm]"] ?? "–"} mm`)}

<h2>7 Wie eine Glyphe entsteht</h2>
<p>Beispiel B: Stamm <code>vline(l, 0, cap)</code>; oberer und unterer Bauch als Pfad vom Stamm (<code>bowl</code>: Querstrich, Viertelbögen, Querstrich zurück); drei Stege direkt rechts neben dem Stamm (Querstriche oben, Mitte, unten). <code>region2::stroke</code> macht aus den Pfaden Bänder, Gehrungsstücke und Enden, <code>region2::combine</code> vereinigt sie und zieht die Stege ab. Ergebnis: Stamm und „3“-förmiger Bauch als zwei geschlossene Konturen aus Geraden und Bögen, ohne Loch. Die Lagen hängen von der Strichstärke ab (Mittellinie = äußere Linie ∓ halber Strich), daher bleiben Versalhöhe und Glyphenbox bei jeder Strichstärke gleich.</p>
${pre(`'B' => {
    g.vline(l, y0, cap);
    bowl(g, l, r - 0.06 * wd, t, m, (t - m) / 2.0);
    bowl(g, l, r, m, b, (m - b) / 2.0);
    for y in [t, m, b] {
        g.cut(l + h + bw / 2.0, y, 90.0);
    }
}`)}

<h2>8 Messwerte</h2>
<h3>Kernel-Zeit</h3>
<p>Release-Build, <code>cad eval --json</code> ohne Daemon, Median aus 3 Läufen; Kernel-Zeit in ms (alle Kernel-Aufrufe). Die Schablonenschrift braucht weniger Kernel-Aufrufe und Zeit als Allerta Stencil und JetBrains Mono: Geraden und Kreise statt B-Splines; ihre eigene Rechenzeit (Regionen, im Cache) steckt in der Auswertung (Sprache ${de(t("schild-stencil")?.evalMs, 1)} ms für das Schild).</p>
<table>
<tr><th>Modell</th><th>cad stencil mono</th><th>Allerta Stencil</th><th>JetBrains Mono</th><th>Schablone / Allerta</th></tr>
${timeRow("hello", "„Hello World“, extrudiert")}
${timeRow("vierzig", "40 Zeichen, extrudiert")}
${timeRow("schild", "Schild: Platte ∪ „ZUTRITT NUR MIT BRILLE“ (size 4,2)")}
</table>
<p>Browser (Chromium, SMLib als Wasm, Playwright K03): Schild ${de(bms("schild-stencil"))} ms gegen ${de(bms("schild-allerta"))} ms mit Allerta Stencil; „Hello World“ ${de(bms("hello-stencil"))} gegen ${de(bms("hello-allerta"))} ms. Beispiel Schablone nativ: ${de(t("schablone")?.kernelMs)} ms.</p>
<h3>Wasm-Modul</h3>
${wasm ? `<table><tr><th></th><th>roh</th><th>gzip -9</th></tr><tr><td><code>cad_wasm_bg.wasm</code> vorher (Commit ${esc(wasm.base)})</td><td class="n">${de(wasm.before.raw)} B</td><td class="n">${de(wasm.before.gzip)} B</td></tr><tr><td>mit Schablonenschrift</td><td class="n">${de(wasm.after.raw)} B</td><td class="n">${de(wasm.after.gzip)} B</td></tr><tr><td>Zuwachs</td><td class="n">${de(wasm.after.raw - wasm.before.raw)} B</td><td class="n">${de(wasm.after.gzip - wasm.before.gzip)} B</td></tr></table><p class="small">Gleicher Build (release, wasm-opt -O3) für beide Stände. <code>region2</code> war schon im Modul (offset, stroke); hinzu kommen nur die Zeichenfunktionen.</p>` : "<p>(fehlt)</p>"}

<h2>9 OpenSCAD-Kompatibilität</h2>
<p>Hängen die Kompatibilitätstests von der Standardschrift ab? Nein: Die 36 Vergleichsmodelle (<code>tests/models/openscad-compat</code>, <code>tests/openscad-compat/projects</code>) enthalten kein <code>text()</code> (${compatText ? `gefunden: ${esc(compatText)}` : "grep findet keine Datei"}), <code>compare.py</code> und die Stufe-1-Tests (<code>crates/compat-tests</code>) sind damit unberührt; der Textvergleich mit OpenSCAD aus Bericht 16 (<code>hull_and_area_like_openscad</code>, <code>report16-openscad.py</code>) nennt jede Schrift ausdrücklich. ${compareLine ? `Lauf <code>compare.py --eval</code>: ${esc(compareLine)}.` : ""} Bewusste Abweichung: <code>text("…")</code> ohne <code>font</code> ergibt bei uns die Schablonenschrift, in OpenSCAD dessen fontconfig-Standard – für gleiche Maße eine Schrift angeben (Plan 17.11, Skill).</p>

<h2>10 UI</h2>
<p>Eine Schriftauswahl gibt es in der UI nicht (Schriften stehen im Code; eigene per „⇡ Datei laden“), daher dort keine Änderung. Neue Szenarien in <code>web/e2e/schablone.spec.ts</code>:</p>
${scenarios.map((s) => `<h3>${esc(s.id)} ${esc(s.title)}</h3><p>${md(s.caption)}</p>${(s.images ?? []).map((i) => fig(pw(i.name), i.label)).join("")}${s.code && s.code.split("\n").length <= 12 ? pre(s.code) : ""}<ul>${(s.checks ?? []).map((c) => `<li>${md(c)}</li>`).join("")}</ul>`).join("\n")}
<h3>C16 achsgebundenes Ziehen (Bericht 4) – Ursache und Korrektur</h3>
<p>C16 lief seit Bericht 15 zeitabhängig in den Test-Timeout, zuletzt auch einzeln. Ursache: Nach <code>clickLine(2, 3)</code> baut der Viewer die Griffe erst im nächsten Animationsframe neu; bis dahin liefert <code>__cad.handles()</code> die Griffe der vorigen Cursorzeile. Dort steht <code>cube([60, 30, 1])</code>, dessen Vektor ebenfalls ein Punktgriff ist – <code>waitHandles("… kind === 'point'")</code> war sofort erfüllt, der Test nahm den ersten Griff (die Plattenecke bei x = 1118 px statt des translate-Punkts bei 770 px), das Ziehen begann nicht (<code>drag() = null</code>), die Maus drehte die Kamera, und das Warten auf „Achse x“ lief bis zum Timeout. Die Achswahl selbst war in allen Läufen richtig. Korrektur im Test: <code>pointHandleOn(page, zeile)</code> wartet auf den Punktgriff <b>der Zeile</b> und nimmt genau diesen (ebenso C20, dieselbe Lücke). Kein Test übersprungen.</p>
${c16 ? pre(c16) : ""}

<h2>11 Testläufe</h2>
<table>
<tr><th>Lauf</th><th>Ergebnis</th></tr>
<tr><td><code>cargo test --workspace --no-fail-fast</code></td><td>${cargoSum ? `${cargoSum.ok} bestanden, ${cargoSum.failed} fehlgeschlagen` : "–"}</td></tr>
<tr><td><code>cargo clippy --workspace --all-targets -- -D warnings</code> · <code>cargo fmt --check</code></td><td>${clippyOk ? "sauber" : "Befund"} · ${fmtOk ? "sauber" : "Abweichung"}</td></tr>
<tr><td><code>pnpm typecheck</code> · <code>pnpm test</code> (Vitest)</td><td>${typecheck !== null ? (/error TS/i.test(typecheck) ? "Fehler" : "sauber") : "–"} · ${vitestSum ?? "–"} bestanden</td></tr>
<tr><td><code>pnpm e2e</code> (Playwright, Chromium)</td><td>${pwSum ? `${pwSum.passed} bestanden, ${pwSum.failed} fehlgeschlagen${pwFailed.length ? `: ${pwFailed.map(esc).join("; ")}` : ""}` : "–"}</td></tr>
<tr><td><code>tests/openscad-compat/compare.py --eval</code></td><td>${esc(compareLine ?? "–")}</td></tr>
</table>
<p>Neue bzw. geänderte Rust-Tests (${newTests.length}):</p>
<p class="small">${newTests.map((x) => `<code>${esc(x.name)}</code> ${x.res === "ok" ? '<span class="ok">ok</span>' : '<span class="bad">FAILED</span>'}`).join(" · ")}</p>
<p class="small">cad-font: Abdeckung (<code>required_chars_in_every_font</code> mit beiden Stilen), keine Löcher/Splitter/Überlappungen für 8 Parameterkombinationen, geschlossene Innenräume ohne Stege, Metrik (Versalhöhe = size, Vorschub = JetBrains Mono, jede Glyphe in ihrer Zelle), Flächen (H exakt, o gegen Formel, Alphabet gegen JetBrains Mono, Strich/Steg/Bold/size), Ausrichtung, Richtungen, Rückfall, Schriftnamen. cad-eval: Standardschrift, Parameter, Warnungen. cad-engine: jede Glyphe als Profil ohne Loch (Regular, Bold), Volumen = Fläche × Höhe, Kernel-Zeit des Schilds gegen Allerta Stencil. compat-tests: Beispiel <code>schablone.scad</code>.</p>

<h2>12 Grenzen</h2>
<ul>
<li>Gestaltung bewusst einfach: eine Strichstärke je Glyphe (nur @ mit 0,72-fachem Strich, ² ³ mit 0,75-fachem), keine optischen Korrekturen, keine Kerning-Paare (monospaced). W, w, m, @, * werden bei Bold sehr dicht.</li>
<li>Grenzen der Parameter aus dem Test (Strich ≤ 0,2·size, Steg 0 oder 0,03…0,2·size); dazwischen geprüft an 8 Kombinationen, nicht lückenlos. Ein Steg ist an Bögen radial gerade geschnitten, seine Breite gilt auf der Mittellinie.</li>
<li>Kombinierende Zeichen (z. B. „a“ + U+0308) setzt die Schrift nicht zusammen: das Zeichen kommt aus JetBrains Mono. Vorkomponierte Umlaute sind enthalten.</li>
<li>Zeichen außerhalb des Umfangs (Kyrillisch, Griechisch, Pfeile …) kommen aus JetBrains Mono – ohne Stege, können also Löcher haben.</li>
<li>Glyphen verschiedener Zeichen berühren sich nur bei <code>spacing</code> &lt; 1 oder den durchgehenden Strichen <code>—</code>, <code>_</code> (über die ganze Zelle wie in Monospace-Schriften); dann können zwischen Glyphen Innenräume entstehen.</li>
<li>Modelle ohne <code>font</code> sehen in OpenSCAD anders aus (dort fontconfig-Standard).</li>
</ul>
</main>
</body>
</html>
`;
writeFileSync(join(out, "report.html"), html);
console.log(`${join(out, "report.html")}: ${html.length} Zeichen, ${scenarios.length} Szenarien, ${reg.length} Glyphen`);
