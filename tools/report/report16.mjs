#!/usr/bin/env node
// Test report 16 ("Text und SVG-Import", Plan 17.4/17.5, Schritt 3 ohne Schablonenschrift):
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
//   compare.txt, step.txt, alltag.txt, node-test.txt compare.py --eval, verify.py, tests/alltag/check.sh, test-node.mjs
//   wasm.json                                       {"before": {raw, gzip}, "after": {raw, gzip}, "fonts": bytes}
//
//   node tools/report/report16.mjs
//   node tools/report/print-pdf.mjs docs/reports/16-text-svg/report.html docs/reports/16-text-svg/report.pdf "Testbericht 16: Text und SVG-Import"

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
const de = (x, d = 0) => (x === null || x === undefined || Number.isNaN(Number(x)) ? "–" : Number(x).toLocaleString("de-DE", { minimumFractionDigits: d, maximumFractionDigits: d }));
const md = (t) => esc(t).replace(/`([^`]+)`/g, "<code>$1</code>");
const T = (name) => readText(join(input, name))?.replaceAll(`${input}/`, "").replaceAll(`${root}/`, "") ?? "(fehlt)";
const pre = (t) => `<pre>${esc(String(t).trimEnd())}</pre>`;
const src = (p) => readText(join(root, p)) ?? "(fehlt)";

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
const svgSrc = (n) => img(join(root, "examples/svg", `${n}.svg`), `src-${n}.svg`);
const fig = (s, cap) => (s ? `<figure><img src="${s}" alt=""><figcaption>${md(cap)}</figcaption></figure>` : `<p class="small">(Bild fehlt: ${esc(cap)})</p>`);
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
const NEW = /cad_svg|cad_font|cad_outline|text_svg|path::tests|tests::(units_|shapes_exact|transforms|fill_rules|hidden_and|open_paths|unsupported_warns|select_by_id|errors|nested_svg|hull_and_area|glyph_o|kerning|align_and|directions|font_names|user_font|cff_outlines|required_chars|fallback_to|circle_area|normalize_rules)|import_|text_|data_urls|changed_svg|examples_check|watch_follows|unsupported_operations/;
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
const oscRow = (r) =>
  `<tr><td>${md(r.name)}</td><td class="n">${de(r.o[0], 3)} … ${de(r.o[2], 3)}<br>${de(r.o[1], 3)} … ${de(r.o[3], 3)}</td><td class="n">${de(r.c[0], 3)} … ${de(r.c[2], 3)}<br>${de(r.c[1], 3)} … ${de(r.c[3], 3)}</td><td class="n">${de(maxBoundDiff(r), 4)}</td><td class="n">${de(r.o[4], 3)}</td><td class="n">${de(r.c[4], 3)}</td><td class="n ${relArea(r) < 0.005 ? "" : "warnc"}">${de(100 * relArea(r), 3)} %</td></tr>`;
const oscText = osc.filter((r) => r.kind === "text");
const oscSvg = osc.filter((r) => r.kind !== "text");
const textMaxBound = oscText.length ? Math.max(...oscText.map(maxBoundDiff)) : null;
const textMaxArea = oscText.length ? Math.max(...oscText.map(relArea)) : null;

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

const FONTS = [
  ["jetbrains", "JetBrains Mono (Standard)", "JetBrains Mono 2.304, Regular"],
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
  `<tr><td>${md(label)}</td><td class="n">${de(t(k)?.nodes)}</td><td class="n">${de(t(k)?.calls)}</td><td class="n">${de(t(k)?.evalMs, 1)}</td><td class="n">${de(t(k)?.kernelMs)}</td><td class="n">${browserKey && browser[browserKey] ? de(browser[browserKey].kernel_ms) : "–"}</td></tr>`;

const html = `<!doctype html>
<html lang="de">
<head>
<meta charset="utf-8">
<title>Testbericht 16 – Text und SVG-Import</title>
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
<h1>Testbericht 16 – Text und SVG-Import</h1>
<div class="meta">Plan 17.7, Schritt 3 (ohne eigene Schablonenschrift): <code>text()</code> mit mitgelieferten OFL-Schriften, <code>import()</code> von SVG · neue Crates cad-outline, cad-svg, cad-font; cad-eval, cad-ir, cad-engine (Farbregionen), cad-cli, cad-desktop, web · ${esc(date)} · Branch ${esc(branch)} · Commit ${esc(commit)} · ${esc(rustc)} · ${os.cpus().length} Kerne</div>

<div class="summary">
  <div>cargo test --workspace<b class="${cargoSum && cargoSum.failed === 0 ? "ok" : "bad"}">${cargoSum ? `${cargoSum.ok} / ${cargoSum.ok + cargoSum.failed}` : "–"}</b>davon ${newTests.length} neu/geändert</div>
  <div>clippy · fmt<b class="${clippyOk && fmtOk ? "ok" : "bad"}">${clippy == null ? "–" : clippyOk ? "sauber" : "Befund"} · ${fmt == null ? "–" : fmtOk ? "sauber" : "Abw."}</b>-D warnings</div>
  <div>Vitest · Playwright<b class="${pwStats && pwStats.failed.length === 0 ? "ok" : "bad"}">${vitestSum ?? "–"} · ${pwStats ? `${pwStats.passed} / ${pwStats.total}` : "–"}</b>neu: T01–T05</div>
  <div>OpenSCAD Text<b class="${textMaxArea !== null && textMaxArea < 0.005 ? "ok" : "bad"}">${textMaxBound !== null ? `≤ ${de(textMaxBound, 3)} mm` : "–"}</b>Hülle; Fläche ≤ ${textMaxArea !== null ? de(100 * textMaxArea, 2) : "–"} %</div>
  <div>Wasm cad_wasm<b>${wasm ? `+${de((wasm.after.raw - wasm.before.raw) / 1e6, 2)} MB` : "–"}</b>${wasm ? `${de(wasm.after.raw / 1e6, 2)} MB, gzip ${de(wasm.after.gzip / 1e6, 2)} MB` : ""}</div>
  <div>„Hello World“<b>${de(t("hello")?.kernelMs)} ms</b>Kernel nativ; Browser ${browser.hello ? de(browser.hello.kernel_ms) : "–"} ms</div>
</div>

<h2>1 Auftrag und Ergebnis</h2>
<p>Aus Schritt 3 von Plan 17: <b>SVG-Import</b> (Teilmenge, 17.5) und <b>Text</b> mit mitgelieferten freien Schriften (17.4). Die eigene Schablonenschrift <code>"cad stencil mono"</code> gehört nicht dazu (sie braucht <code>stroke</code> aus Schritt 2); der Name ist als künftige Standardschrift reserviert, bis dahin ist <b>JetBrains Mono</b> die Standardschrift (Hinweis bei <code>font = "cad stencil mono"</code>).</p>
<ul>
<li><code>text(t, size, font, halign, valign, spacing, direction, language, script)</code>: Umrisse exakt (TrueType quadratisch, CFF kubisch), Kerning und Ligaturen über <code>rustybuzz</code>, Löcher nach nonzero, <code>color()</code> wie sonst. Hülle und Fläche gegen OpenSCAD 2021.01 mit denselben Schriftdateien: Hülle ≤ ${textMaxBound !== null ? de(textMaxBound, 3) : "–"} mm, Fläche ≤ ${textMaxArea !== null ? de(100 * textMaxArea, 2) : "–"} % (OpenSCAD tastet die Kurven ab).</li>
<li><code>import("….svg", id, scale, dpi = 96, center)</code>: Pfade (alle Befehle), Rechtecke mit Rundung, Kreise und Ellipsen exakt, Polygone, Gruppen und verschachtelte <code>svg</code> mit <code>transform</code>, Einheiten, <code>fill-rule</code>, <code>display:none</code>; alles andere mit Warnung und SVG-Zeile.</li>
<li>Dateien relativ zur einbindenden Datei (CLI: Dateisystem; Browser: Projektdateien; Desktop: liest und schreibt sie); geänderte SVGs ändern den Cache-Schlüssel, <code>cad watch</code> folgt ihnen, die UI-Szene auch (T02).</li>
<li>Pflicht-Zeichensatz mit Umlauten, ß/ẞ und Satzzeichen in allen mitgelieferten Schriften (Test); Lücken (nur Allerta Stencil) kommen mit Warnung aus JetBrains Mono.</li>
<li>Nebenbei: Farbregionen von Modellen, die nur Vereinigungen ihrer Farbknoten sind, ohne Boolean mit dem Ganzen (mehrfarbiger Text: Sekunden statt Abbruch durch den Watchdog).</li>
</ul>

<h2>2 Entscheidungen</h2>
<table>
<tr><th>Frage</th><th>Entscheidung</th></tr>
<tr><td>Bibliotheken</td><td><code>roxmltree</code> 0.21 (XML, Zeilen für Meldungen), <code>rustybuzz</code> 0.20.1 mit <code>ttf-parser</code> 0.25 (Shaping wie HarfBuzz, Umrisse, reines Rust, Wasm-tauglich). Pfaddaten, Transformationen, Einheiten, Bögen, Füllregeln: eigener Code.</td></tr>
<tr><td>Aufteilung</td><td>Neue Crates <code>cad-outline</code> (neutrale Konturen), <code>cad-svg</code>, <code>cad-font</code>; in <code>cad-eval</code> nur die Builtins (<code>import.rs</code>) und das Laden der Datendateien (<code>Project::assets</code>). IR: neues Segment <code>Seg::Nurbs</code>. Engine/Fassade unverändert außer den Farbregionen.</td></tr>
<tr><td>Profil-Knoten</td><td>Ein <code>Profile::Sketch</code> je gefülltem SVG-Element bzw. je Glyphe, mehrere unter einer Gruppe (Vereinigung): überlappende Elemente oder Glyphen ergeben die Vereinigung wie beim Malen; innerhalb eines Elements entscheidet seine Füllregel.</td></tr>
<tr><td>Kurven</td><td>Tangentenstetige Ketten von Bézierkurven einer Glyphe oder eines Pfads werden <b>eine</b> B-Spline (Knoten C1, Parameterlängen aus den Tangentenlängen; exakt), Ecken trennen. Weniger Kernel-Flächen (Glyphe „o“: 4 statt ~30).</td></tr>
<tr><td>Größe <code>size</code></td><td>wie OpenSCAD: em = <code>size</code>/0,72 (OpenSCAD setzt die Schrift auf <code>size</code> Punkt bei 100 dpi); Versalhöhe ≈ <code>size</code>. Laufweite, Versatz und Ausrichtung mal 1000/1024 wie OpenSCAD (dessen FreeType-Umrechnung): Text bleibt gleich breit wie in OpenSCAD.</td></tr>
<tr><td>Ausrichtung</td><td><code>halign</code> nach der Laufweite, <code>valign</code> nach der Glyphenhülle (top/center/bottom), <code>baseline</code> 0; senkrechter Text mit den Versätzen des Shapers (wie OpenSCAD zentriert).</td></tr>
<tr><td>Standardschrift</td><td>JetBrains Mono Regular, bis <code>"cad stencil mono"</code> existiert (Name reserviert, Hinweis). Unbekannte Familie: Warnung + JetBrains Mono; unbekannter Stil: Warnung + Regular der Familie.</td></tr>
<tr><td>Fehlende Zeichen</td><td>Rückfall je Zeichen auf JetBrains Mono gleichen Gewichts (OS/2-Gewicht ≥ 600 → Bold) mit Warnung am Statement: ein lesbares Zeichen an seiner Stelle ist nützlicher als ein Kästchen oder eine Lücke, die beim Drucken übersehen wird; JetBrains Mono deckt den Pflicht-Zeichensatz und weit mehr ab. Allerta Stencil bleibt unverändert (Reserved Font Name „Allerta“: veränderte Fassungen dürften nicht so heißen).</td></tr>
<tr><td>SVG-Einheiten</td><td>wie OpenSCAD, wo es konsistent ist; Standard <code>dpi = 96</code> (OpenSCAD 2021.01: 72 für Zahlen ohne Einheit). Abweichungen siehe 5.3.</td></tr>
<tr><td>Dateien</td><td>Jeder Text im Projekt, der auf <code>.svg/.ttf/.otf/.ttc/.otc</code> endet, wird beim Laden gelesen (auch Parameter-Standardwerte); Binärdateien liegen in Text-Projekten (Browser, <code>SetFiles</code>) als <code>data:</code>-URL. Ein zur Laufzeit zusammengesetzter Name, der nirgends als Text steht: Fehler „nicht geladen“.</td></tr>
</table>

<h2>3 Schriften</h2>
<p>Mitgeliefert in <code>crates/cad-font/fonts/</code> (SIL OFL 1.1, Lizenztexte und Quellen mit Prüfsummen in <code>fonts/README.md</code>), eingebettet mit <code>include_bytes!</code>: ${fontBytes.map(([f, b]) => `<code>${esc(f)}</code> ${de(b / 1024)} KiB`).join(", ")}. Auswahl per Familie und Stil wie OpenSCAD (<code>"Inter:style=Bold"</code>), eigene Schriften per Pfad (<code>font = "datei.ttf"</code>) bzw. Upload in der UI (T03).</p>
<h3>Alphabet-Tafeln</h3>
<p>Je Schrift <code>text(…, size = 8)</code>, 0,6 mm extrudiert, von oben (Generator <code>tools/report/report16-data.sh</code>). Zeile 3–5: Ziffern, <b>Umlaute und Satzzeichen</b> des Pflicht-Zeichensatzes.</p>
${FONTS.map(([k, name, ver]) => fig(cli(`alphabet-${k}`), `${name} – ${ver}`)).join("\n")}
<h3>Pflicht-Zeichensatz</h3>
<p><code>cad_font::REQUIRED_CHARS</code> (Test <code>required_chars_in_every_font</code>: jede mitgelieferte Schrift hat für jedes Zeichen eine echte Glyphe, kein <code>.notdef</code>): A–Z a–z 0–9, Ä Ö Ü ä ö ü ß ẞ, Leerzeichen, <code>. , ; : ! ? ' " „ “ ‚ ‘ » « ( ) [ ] { } - – — _ / \\ &amp; % # + * = &lt; &gt; ° ² ³ € @ § ~ ^ | · • …</code>, <code>± × ÷ µ Ø ø</code>. Gilt auch für die künftige Schablonenschrift (Plan 17.4).</p>
<table>
<tr><th>Schrift</th><th>fehlende Zeichen</th><th>Folge</th></tr>
<tr><td>JetBrains Mono Regular/Bold</td><td>keine</td><td>–</td></tr>
<tr><td>Inter Regular/Bold</td><td>keine</td><td>–</td></tr>
<tr><td>Allerta Stencil</td><td><code>ẞ „ ‚ { } _ # ° ² ³ § ~ ^ · • … ± µ Ø ø</code></td><td>aus JetBrains Mono Regular, Warnung (siehe Tafel: diese Zeichen sind nicht schabloniert)</td></tr>
</table>
${pre(fallbackWarn)}

<h2>4 Text gegen OpenSCAD 2021.01</h2>
<p>Gleiche Schriftdateien (OpenSCAD über eine fontconfig-Datei, die nur <code>crates/cad-font/fonts</code> kennt), <code>text(…)</code> als 2D exportiert (Polygone) gegen <code>cad measure --bounds --volume</code> einer 1-mm-Extrusion (<code>tools/report/report16-openscad.py</code>). Hülle in x (oben) und y (unten), mm.</p>
<table>
<tr><th>Fall</th><th>OpenSCAD Hülle</th><th>cad Hülle</th><th>max. Abw.</th><th>OpenSCAD mm²</th><th>cad mm²</th><th>Fläche</th></tr>
${oscText.map(oscRow).join("\n")}
</table>
<p class="small">Die Fläche von OpenSCAD ist kleiner, weil es die Kurven als Polygone abtastet (Sehnen innerhalb der Kurve); die Hülle stimmt auf 6 Stellen. valign misst OpenSCAD an den gerasterten Glyphenmetriken (FreeType), daher bis 0,005 mm Unterschied. „Hello World“, „AV“ (Kerning), „fi“ und „-&gt;“ (Ligaturen in JetBrains Mono) zeigen, dass Shaping und Laufweite gleich sind.</p>

<h2>5 SVG-Import</h2>
<h3>5.1 Beispiele: Quelle und Ergebnis</h3>
<p>Links die SVG-Datei, wie der Browser sie zeichnet (Quelle in <code>examples/svg/</code>, eigene Zeichnungen), rechts <code>linear_extrude(2) import("….svg")</code> von oben.</p>
${[
  ["logo", "Ring aus vier Bögen mit Loch (evenodd), acht gedrehte Rechtecke mit Rundung, Polygon"],
  ["blatt", "kubische Bézier (C, S), Rippe als Gegenlauf (nonzero), Stiel aus Q/T, gedrehte Gruppe"],
  ["haus", "relative Befehle (h, v, m), Fenster und Tür als Löcher (evenodd), Rechteck mit rx, skalierte Gruppe mit Kreis"],
  ["welle", "Q/T-Welle, Ellipse, Kreis mit scale(2 1) (Ellipsenbögen), skewX, gedrehter Ellipsenbogen (A)"],
]
  .map(([n, cap]) => `<h3>${esc(n)}.svg</h3><p class="small">${md(cap)}</p><div class="two">${svgSrc(n) ? `<figure><img class="src" src="${svgSrc(n)}" alt=""><figcaption>Quelle (Browser)</figcaption></figure>` : ""}${fig(cli(`svg-${n}`), "import, extrudiert, von oben")}</div>`)
  .join("\n")}
<h3>5.2 Unterstützt und nicht unterstützt</h3>
<table>
<tr><th>Element / Eigenschaft</th><th>Umsetzung</th></tr>
<tr><td><code>path</code></td><td>M L H V C S Q T A Z, absolut und relativ, implizite Wiederholung, kompakte Zahlen (<code>10-10.5.5</code>) und Bogen-Flags (<code>a10 10 0 0110 10</code>); Fehler: der Teil davor bleibt, Warnung</td></tr>
<tr><td>Bögen <code>A</code>, <code>circle</code>, <code>ellipse</code>, <code>rect rx/ry</code></td><td>exakte Kreisbögen (unter Ähnlichkeiten), sonst exakte Ellipsenbögen (rationale B-Spline); Radien zu klein → hochskaliert wie SVG</td></tr>
<tr><td><code>polygon</code> · <code>polyline</code>, <code>line</code>, offene Pfade</td><td>Fläche · nur Pfad (Warnung „offener Pfad wird nicht gefüllt“); nur Pfade → offene Skizze (als <code>sweep</code>-Pfad)</td></tr>
<tr><td><code>g</code>, <code>a</code>, verschachteltes <code>svg</code>, <code>transform</code></td><td>matrix, translate, scale, rotate (auch um Punkt), skewX, skewY; Listen von rechts nach links</td></tr>
<tr><td><code>viewBox</code>, <code>width/height</code>, <code>preserveAspectRatio</code></td><td>px, mm, cm, in, pt, pc, ohne Einheit (<code>dpi</code>); Ursprung unten links</td></tr>
<tr><td><code>fill</code>, <code>fill-rule</code>, <code>display</code>, <code>visibility</code></td><td>als Attribut und in <code>style</code>, vererbt; <code>fill="none"</code>: nicht übernommen (Warnung)</td></tr>
<tr><td>Text, Bilder, <code>use</code>, CSS-Klassen/<code>&lt;style&gt;</code>, Masken, Clip-Pfade, Filter, Verläufe/Muster als Füllung, Strichstärke</td><td>Warnung mit SVG-Zeile (je Meldung einmal); Verläufe werden als einfache Fläche übernommen</td></tr>
<tr><td>Metadaten (Inkscape, Sodipodi, <code>defs</code>, <code>title</code>)</td><td>stillschweigend übergangen</td></tr>
<tr><td><code>id = "…"</code></td><td>nur dieses Element (mit Inhalt); unbekannt: Fehler</td></tr>
</table>
${pre(src("reports/tmp/report16/cases/warn.svg"))}
${pre(T("warn.check.txt"))}
<h3>5.3 Einheiten und Vergleich mit OpenSCAD</h3>
<p>OpenSCAD 2021.01 (<code>import(…, dpi = 96)</code>, 2D exportiert) gegen cad. Mit <code>viewBox</code> stimmen beide überein. Abweichungen mit Absicht (SVG-Regel statt OpenSCAD-Eigenheit): <b>ohne <code>viewBox</code></b> nimmt OpenSCAD Nutzereinheiten als mm, spiegelt aber um die Höhe in Pixeln (Zeile „ohne-viewbox“); ein y-Versatz der <code>viewBox</code> wird bei OpenSCAD addiert statt abgezogen; OpenSCAD füllt immer nach evenodd und macht aus Strichen Flächen.</p>
<table>
<tr><th>Datei</th><th>OpenSCAD Hülle</th><th>cad Hülle</th><th>max. Abw.</th><th>OpenSCAD mm²</th><th>cad mm²</th><th>Fläche</th></tr>
${oscSvg.map(oscRow).join("\n")}
</table>

<h2>6 Beispiele</h2>
<h3><code>examples/spec/text_schild.scad</code></h3>
<div class="codefig">${pre(src("examples/spec/text_schild.scad"))}<div>${fig(cli("text_schild"), "Schild: Platte aus einer Skizze, Rand, erhabener Text in Inter Bold und Allerta Stencil, vier Farben")}${fig(cli("text_schild-oben"), "von oben")}${pre(T("schild-colors.txt"))}</div></div>
<h3><code>examples/spec/svg_logo.scad</code></h3>
<div class="codefig">${pre(src("examples/spec/svg_logo.scad"))}<div>${fig(cli("svg_logo"), "die vier SVGs: Logo, Blatt (skaliert), Haus als Prägung, Welle (nur id = \"welle\" und alles)")}${fig(cli("svg_logo-oben"), "von oben")}</div></div>
<p class="small">STEP von svg_logo: ${esc(T("svg_logo.step-counts.json"))} – Kreise bleiben <code>CIRCLE</code>, extrudierte Bögen <code>CYLINDRICAL_SURFACE</code>.</p>

<h2>7 Messwerte</h2>
<h3>Kernel-Zeit</h3>
<p>Release-Build, <code>cad eval --json</code> ohne Daemon, Median aus 3 Läufen (Sprache = Auswertung inkl. Shaping und SVG; Kernel = alle Kernel-Aufrufe). Browser: dieselben Modelle in Chromium (SMLib als Wasm, T05).</p>
<table>
<tr><th>Modell</th><th>Knoten</th><th>Kernel-Aufrufe</th><th>Sprache ms</th><th>Kernel ms nativ</th><th>Kernel ms Browser</th></tr>
${timeRow("hello", "„Hello World“ (Inter), extrudiert", "hello")}
${timeRow("vierzig", "40 Zeichen (Inter), extrudiert", "40")}
${timeRow("vierzig_mono", "40 Zeichen (JetBrains Mono)", null)}
${timeRow("platte", "„Hello World“ auf einer Platte (Vereinigung)", null)}
${timeRow("svg_logo", "examples/spec/svg_logo.scad", null)}
${timeRow("text_schild", "examples/spec/text_schild.scad", null)}
</table>
<p>Je Glyphe: Extrusion je Kontur (Außen, Loch), Differenz für Löcher, eine Vereinigung aller Glyphen (sie berühren sich nicht). Teuer ist Text <b>auf</b> einer Fläche: SMLib prägt jede Glyphe in die Deckfläche (Schild: eine Vereinigung von ≈ 9 s nativ). Die neue Farbregion-Regel (Plan 17.9) vermeidet dabei die Booleans mit dem Ganzen; vorher brauchte der Kontaktbogen von T01 nativ 21 s und scheiterte („Schnittmenge gescheitert“), jetzt 4 s.</p>
<h3>Wasm-Modul</h3>
${wasm ? `<table><tr><th></th><th>roh</th><th>gzip -9</th></tr><tr><td><code>cad_wasm_bg.wasm</code> vorher (Commit ${esc(wasm.base ?? "cafe7a4")})</td><td class="n">${de(wasm.before.raw)} B</td><td class="n">${de(wasm.before.gzip)} B</td></tr><tr><td>nachher</td><td class="n">${de(wasm.after.raw)} B</td><td class="n">${de(wasm.after.gzip)} B</td></tr><tr><td>Zuwachs</td><td class="n">+${de(wasm.after.raw - wasm.before.raw)} B</td><td class="n">+${de(wasm.after.gzip - wasm.before.gzip)} B</td></tr><tr><td>davon Schriftdateien</td><td class="n">${de(wasm.fonts)} B</td><td class="n">–</td></tr></table><p class="small">Der Rest des Zuwachses ist Code (rustybuzz/ttf-parser, roxmltree, SVG- und Textlogik). <code>smlib.wasm</code> unverändert (${de(wasm.smlib)} B).</p>` : "<p>(fehlt)</p>"}

<h2>8 UI</h2>
<p>Prüfungen über den Zustand der UI (<code>window.__cad</code>), Szenen und Hüllen; Screenshots aus <code>web/e2e/text-svg.spec.ts</code>.</p>
${scenarios.map((s) => `<h3>${esc(s.id)} ${esc(s.title)}</h3><p>${md(s.caption)}</p>${(s.images ?? []).length === 2 ? pair(pw(s.images[0].name), s.images[0].label, pw(s.images[1].name), s.images[1].label) : (s.images ?? []).map((i) => fig(pw(i.name), i.label)).join("")}${s.code && s.code.split("\n").length <= 16 ? pre(s.code) : ""}<ul>${(s.checks ?? []).map((c) => `<li>${md(c)}</li>`).join("")}</ul>`).join("\n")}

<h2>9 Testläufe</h2>
<table>
<tr><th>Lauf</th><th>Ergebnis</th></tr>
<tr><td><code>cargo test --workspace --no-fail-fast</code></td><td>${cargoSum ? `${cargoSum.ok} bestanden, ${cargoSum.failed} fehlgeschlagen` : "–"}</td></tr>
<tr><td><code>cargo clippy --workspace --all-targets -- -D warnings</code> · <code>cargo fmt --check</code></td><td>${clippyOk ? "sauber" : "Befund"} · ${fmtOk ? "sauber" : "Abweichung"}</td></tr>
<tr><td><code>pnpm typecheck</code> · <code>pnpm test</code> (Vitest)</td><td>${typecheck !== null ? (/error TS/i.test(typecheck) ? "Fehler" : "sauber") : "–"} · ${vitestSum ?? "–"} bestanden</td></tr>
<tr><td><code>pnpm e2e</code> (Playwright, Chromium)</td><td>${pwStats ? `${pwStats.passed} / ${pwStats.total}${pwStats.failed.length ? ` – fehlgeschlagen: ${pwStats.failed.map((f) => esc(f.title)).join(", ")}` : ""}` : "–"}</td></tr>
<tr><td><code>tests/openscad-compat/compare.py --eval</code></td><td>${compareSum ? `${compareSum[0]}/${compareSum[1]}` : "–"}</td></tr>
<tr><td><code>tests/step/verify.py</code> (OpenCASCADE)</td><td>${esc(stepSum ?? "–")}</td></tr>
<tr><td><code>tests/alltag/check.sh</code></td><td>${esc(alltagLine ?? "–")}</td></tr>
<tr><td><code>node crates/kernel-smlib/js/test-node.mjs</code></td><td>${nodeTest && /all node tests passed/.test(nodeTest) ? "bestanden" : "–"}</td></tr>
</table>
<p>Neue bzw. geänderte Rust-Tests (${newTests.length}):</p>
<p class="small">${newTests.map((t) => `<code>${esc(t.name)}</code> ${t.res === "ok" ? '<span class="ok">ok</span>' : '<span class="bad">FAILED</span>'}`).join(" · ")}</p>
<p class="small">SVG (<code>cad-svg</code>): Pfadbefehle, Bögen und Flags, Transformationen, Einheiten mit OpenSCAD-Referenzwerten, Füllregeln, verborgene/ungefüllte Elemente, Warnungen mit Zeile, <code>id</code>, Fehlerfälle. Text (<code>cad-font</code>): Hülle und Fläche gegen OpenSCAD (20 Fälle), Glyphe „O“ mit Loch, Kerning „AV“, Ausrichtung, Richtungen, Schriftnamen, eigene Schrift, CFF-Umrisse, Pflicht-Zeichensatz, Rückfall. <code>cad-eval</code>: Profile, Projektdateien, Fehler, data-URLs. CLI: Beispiele, Volumen, STEP-Kreis, <code>watch</code> folgt dem SVG. Vitest: Datendateien (<code>web/src/logic/files.test.ts</code>).</p>

<h2>10 Grenzen</h2>
<ul>
<li>Eigene Schablonenschrift fehlt (Schritt 3, nach <code>stroke</code>); Allerta Stencil hat 20 Zeichen des Pflicht-Zeichensatzes nicht (aus JetBrains Mono, nicht schabloniert).</li>
<li>Kernel-Zeit: viele Glyphen auf einer gemeinsamen Fläche sind teuer (Vereinigung in SMLib wächst überlinear: 9 Glyphen ≈ 1,5 s, 28 ≈ 9 s nativ); der Browser ist ähnlich schnell, ein einzelner Aufruf über 15 s löst den Watchdog aus.</li>
<li>Füllregeln setzen sich nicht schneidende Konturen innerhalb eines Elements voraus (Schriften mit überlappenden Konturen, z. B. manche Variable Fonts, und sich selbst schneidende SVG-Pfade werden nicht aufgelöst). Variable Fonts nur in der Standardinstanz.</li>
<li>SVG: kein Text, keine <code>use</code>-Klone, keine CSS-Klassen, keine Striche als Fläche, keine Masken/Clip-Pfade; Prozent- und em-Längen werden wie Zahlen behandelt.</li>
<li>Dateinamen müssen als Text im Projekt stehen (ein zur Laufzeit zusammengesetzter Name wird nicht geladen).</li>
<li>Umgehung: Kreisbögen über 90° werden beim Import geviertelt, weil die SMLib-Fassade Vereinigungen solcher Skizzenbögen falsch berechnet (Skizzen Schritt 1; Behebung im Skizzen-Schritt 2); danach entfällt sie.</li>
<li>Standardausrichtung senkrechten Texts wie OpenSCAD (Versatz des Shapers); <code>valign</code> nach der echten Glyphenhülle (OpenSCAD: gerasterte FreeType-Metriken, ≤ 0,005 mm).</li>
</ul>
</main>
</body>
</html>
`;
writeFileSync(join(out, "report.html"), html);
console.log(`${join(out, "report.html")}: ${html.length} Zeichen, ${scenarios.length} Szenarien, ${osc.length} OpenSCAD-Vergleiche`);
