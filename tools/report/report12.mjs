#!/usr/bin/env node
// Test report 12 ("Korrekturen Farben und Daemon"): the findings F1–F9 of the independent
// review (tests/farben-pruefung/README.md) and the daemon socket path – cause, correction,
// evidence (before: the reviewer's images, after: CLI and UI), Bambu Studio result, limits.
// Writes docs/reports/12-farben-korrekturen/report.html (images in img/).
//
// Inputs (gitignored, reports/tmp/report12/):
//   cli/*.png, *.txt, *.json, sources/              tools/report/report12-data.sh
//   scenarios.json, img/*.jpg                       cd web && pnpm exec playwright test e2e/farben-korrekturen.spec.ts
//   playwright.json                                 copy of reports/tmp/report2/playwright.json (whole suite: pnpm e2e)
//   vitest.txt, typecheck.txt                       cd web && pnpm test, pnpm typecheck
//   tauri/tauri-e2e.json, tauri/img/*.png           node web/e2e-tauri/desktop.e2e.mjs --bin target/release/mcad --out reports/tmp/report12/tauri
//   cargo.txt, clippy.txt, fmt.txt                  cargo test --workspace --no-fail-fast, clippy -D warnings, fmt --check
//   compare.txt, step.txt, alltag.txt               tests/openscad-compat/compare.py, tests/step/verify.py, tests/alltag/check.sh
//
//   node tools/report/report12.mjs
//   node tools/report/print-pdf.mjs docs/reports/12-farben-korrekturen/report.html docs/reports/12-farben-korrekturen/report.pdf "Testbericht 12: Korrekturen Farben und Daemon"

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
const de = (x, d = 0) => (x === null || x === undefined || Number.isNaN(Number(x)) ? "–" : Number(x).toLocaleString("de-DE", { minimumFractionDigits: d, maximumFractionDigits: d }));
const md = (t) => esc(t).replace(/`([^`]+)`/g, "<code>$1</code>");
const T = (name) => readText(join(input, name))?.replaceAll(`${input}/`, "").replaceAll(`${root}/`, "") ?? "(fehlt)";
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
const before = (n) => img(join(root, "tests/farben-pruefung/img", n), `vorher-${n.replace(/\.png$/, ".jpg")}`);
const cli = (n) => img(join(input, "cli", `${n}.png`), `cli-${n}.jpg`);
const pw = (n) => img(join(input, "img", `${n}.jpg`), `ui-${n}.jpg`);
const tauriImg = (n) => img(join(input, "tauri/img", n), `tauri-${n.replace(/\.png$/, ".jpg")}`);
const fig = (src, cap) => (src ? `<figure><img src="${src}" alt=""><figcaption>${md(cap)}</figcaption></figure>` : `<p class="small">(Bild fehlt: ${esc(cap)})</p>`);
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
const alltagLine = alltag ? (alltag.trim().split("\n").reverse().find((l) => /bestanden|abweich|Modelle/i.test(l)) ?? alltag.trim().split("\n").at(-1)) : null;
const verify = readJson(join(input, "verify.json"));
const verifyCg = readJson(join(input, "verify-cg.json"));
const scenarios = (readJson(join(input, "scenarios.json")) ?? []).sort((a, b) => a.id.localeCompare(b.id));
const scen = (id) => scenarios.find((s) => s.id === id);
const rand = readJson(join(input, "f5-randnames.json"));

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
  ["F1", "hoch", "Gleiche Geometrie in verschiedenen Farben: Ansicht und STEP in der ersten Farbe", "behoben"],
  ["F2", "hoch", "Slotwahl in der UI an einem color() mit Parameter-/Schleifenfarbe färbte alle Farben", "behoben (filament()-Tabelle)"],
  ["F3", "mittel", "Ansicht ≠ Druck (Überlappung, Verrundung an Farbgrenze, color() im 2D-Profil)", "behoben (Regionen in der Ansicht; gemischte 2D-Farben: Warnung)"],
  ["F4", "mittel", "Material außerhalb von Bauteilen: UI ließ es weg, CLI-Meldung falsch", "behoben"],
  ["F5", "mittel", "Zufallsfarben kollidieren", "behoben"],
  ["F6", "mittel/niedrig", "Splitter bei gleichfarbigen Nachbarn", "behoben"],
  ["F7", "niedrig", "Alpha-Varianten, stilles Klemmen, transparent, leerer Name", "behoben (Regeln + Warnungen)"],
  ["F8", "niedrig", "> 16 Slots, geteilte Slots ohne Hinweis, Farbgruppe bei einfarbig", "behoben"],
  ["F9", "niedrig", "--per-color mit .stl, Farbliste bei --part, Dateiname bei use/include", "behoben"],
  ["D", "–", "Daemon startet bei zu langem Socketpfad still nicht", "behoben (Ersatzpfad)"],
  ["B", "–", "Bambu Studio: Farbdialog überschreibt Slots", "vermieden (3MF ohne Farbgruppe)"],
];

const testsOf = (re) => newTests.filter((t) => re.test(t.name)).map((t) => `<code>${esc(t.name)}</code> ${t.res === "ok" ? '<span class="ok">ok</span>' : '<span class="bad">FAILED</span>'}`).join(", ") || "–";

const html = `<!doctype html>
<html lang="de">
<head>
<meta charset="utf-8">
<title>Testbericht 12 – Korrekturen Farben und Daemon</title>
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
<h1>Testbericht 12 – Korrekturen Farben und Daemon</h1>
<div class="meta">Befunde F1–F9 der unabhängigen Prüfung (<code>tests/farben-pruefung/</code>) und Daemon-Socketpfad · cad-eval, cad-engine, cad-session, cad-cli, cad-ipc, web · ${esc(date)} · Branch ${esc(branch)} · Commit ${esc(commit)} · ${esc(rustc)} · ${os.cpus().length} Kerne</div>

<div class="summary">
  <div>cargo test --workspace<b class="${cargoSum && cargoSum.failed === 0 ? "ok" : "bad"}">${cargoSum ? `${cargoSum.ok} / ${cargoSum.ok + cargoSum.failed}` : "–"}</b>davon ${newTests.length} neue Regressionstests</div>
  <div>clippy · fmt<b class="${clippyOk && fmtOk ? "ok" : "bad"}">${clippy == null ? "–" : clippyOk ? "sauber" : "Befund"} · ${fmt == null ? "–" : fmtOk ? "sauber" : "Abw."}</b>-D warnings</div>
  <div>Vitest · Playwright<b class="${pwStats && pwStats.failed.length === 0 ? "ok" : "bad"}">${vitestSum ?? "–"} · ${pwStats ? `${pwStats.passed} / ${pwStats.total}` : "–"}</b>neu: G01–G04</div>
  <div>Tauri-E2E<b class="${tauri && tauriOk === tauri.results.length && !tauri.fatal ? "ok" : "bad"}">${tauri ? `${tauriOk} / ${tauri.results.length}` : "–"}</b>neu: T15</div>
  <div>OpenSCAD · STEP<b class="${compareSum && compareSum[0] === compareSum[1] ? "ok" : "bad"}">${compareSum ? `${compareSum[0]}/${compareSum[1]}` : "–"} · ${esc(stepSum?.match(/\d+\/\d+/)?.[0] ?? "–")}</b>compare.py · verify.py</div>
  <div>Alltagsmodelle<b class="${alltag && !/FEHL|weicht ab|abweichend/i.test(alltag) ? "ok" : "bad"}">${alltag ? "unverändert" : "–"}</b>tests/alltag/check.sh</div>
  <div>3MF (lib3mf, trimesh)<b class="${verify?.ok ? "ok" : "bad"}">${verify ? (verify.ok ? "bestanden" : "Fehler") : "–"}</b>Farbdialog Bambu: ${verify ? (verify.bambu_model?.color_dialog ? "ja" : "nein") : "–"}</div>
</div>

<h2>1 Auftrag und Ergebnis</h2>
<p>Ein unabhängiger Test-Agent hat die Farbfunktionen aus Bericht 11 geprüft und neun Befunde (F1–F9) sowie einen Daemon-Fehler gemeldet (<code>tests/farben-pruefung/README.md</code>, Testfälle in <code>cases/</code>, Bilder in <code>img/</code>). Alle Befunde sind behoben; jeder hat einen Regressionstest in Rust, die UI-Befunde zusätzlich in Vitest, Playwright und Tauri-E2E. Die Testfälle der Prüfung laufen vollständig als <code>crates/cad-cli/tests/farben_pruefung.rs</code> (jede Datei: keine Fehler, Regionsvolumen = Bauteilvolumen, Zufallsfarben mit Abstand zur Palette). „Vorher“-Bilder sind die des Prüfers (Commit d8fe983), „nachher“ die CLI und UI dieses Commits.</p>
<table>
<tr><th>Nr.</th><th>Schwere</th><th>Befund</th><th>Status</th></tr>
${status.map(([n, s, b, st]) => `<tr><td>${n}</td><td>${esc(s)}</td><td>${md(b)}</td><td class="ok">${esc(st)}</td></tr>`).join("\n")}
</table>

<h2>2 Befunde</h2>

<section class="finding">
<h3>F1 (hoch) – gleiche Geometrie, verschiedene Farben</h3>
<dl class="f">
<dt>Ursache</dt><dd>Der Cache-Schlüssel eines Knotens enthält die Farbe nicht (Absicht: eine neue Farbe rechnet nichts neu). <code>color("red") cube(5)</code> und <code>translate(…) color("blue") cube(5)</code> haben daher denselben Würfelknoten-Inhalt, dasselbe Flächen-Tag und – aus dem Cache – dieselben Kernel-Flächen. Ansicht (<code>tagColors</code>: Tag → erster Knoten mit Farbe) und STEP (<code>nodes_for_tag(tag).first()</code>) nahmen für alle Flächen dieses Tags die erste Farbe. Das 3MF war richtig, weil die Farbregionen die Instanzen einzeln platzieren.</dd>
<dt>Korrektur</dt><dd>Die Cache-Wiederverwendung bleibt; die Farbe wird über Instanzen und Regionen zugeordnet: (1) die Ansicht zeigt bei Modellen mit Farben die <b>Farbregionen</b> (die Körper des 3MF), sobald sie gerechnet sind (UI „Farbregionen zeigen“, jetzt Standard; <code>cad render</code> ohne <code>--show faces</code>); (2) die Farbe je Fläche (davor, für hervorgehobene Teilbäume und <code>--show faces</code>) nimmt nur Knoten unterhalb des gezeichneten Knotens und unter mehreren gleichen Inhalts den, dessen Hülle das Dreieck enthält (<code>faceColors</code> in <code>web/src/logic/colors.ts</code>, <code>face_colors</code> in <code>cad-cli/src/render.rs</code>); (3) STEP-Flächenfarben kommen aus der Regelmenge der Regionen ohne Boolean: Abtastpunkte jeder Fläche gegen das platzierte Material jedes Farbknotens (<code>kernel.classify</code>), der späteste gewinnt (<code>Engine::face_region_colors</code>), als synthetische Tags in einer Kopie der B-Rep.</dd>
<dt>Tests</dt><dd>${testsOf(/^f1_|^all_review/)}; Vitest „Bericht 12 F1“; Playwright G01; Tauri T15.</dd>
</dl>
${pair(before("pal2.png"), "vorher: pal2.scad, drei rote Würfel (Prüfer)", cli("f1-pal2"), "nachher: `cad render` (Farbregionen)")}
${pair(cli("f1-pal2-flaechen"), "nachher: `--show faces` (Farbe je Fläche je Instanz)", cli("f1-palette"), "nachher: palette.scad, 16 gleiche Kacheln in einer Schleife")}
<p>STEP von pal2.scad (Flächen je Farbe):</p>${pre(T("f1-step.txt"))}
${fig(pw("g01-flaechen"), "UI: Farbe je Fläche mit „Farbregionen zeigen“ aus – jede Instanz in ihrer Farbe")}
<p class="small">Grenze: die Farbe je Fläche ist eine Näherung (Hüllen-Test; bei überlappenden Hüllen gleich geformter Körper verschiedener Farbe die spätere). Exakt sind die Regionen, die die Ansicht nach der Rechnung zeigt; in STEP bekommt eine über eine Farbgrenze verschmolzene Fläche eine Farbe (Mehrheit der Abtastpunkte).</p>
</section>

<section class="finding">
<h3>F2 (hoch) – Slotwahl bei Parameter- oder Schleifenfarbe</h3>
<dl class="f">
<dt>Ursache</dt><dd><code>SetFilament</code> schrieb <code>filament = n</code> an die erste <code>color()</code>-Anweisung der Farbe. Bei <code>module bar(c) color(c) …</code> oder <code>for (i) color(names[i]) …</code> färbt diese Anweisung mehrere Farben – alle bekamen den Slot (bzw. die erste Angabe gewann mit Warnung).</dd>
<dt>Entscheidung</dt><dd>Eine Slot-Tabelle im Code: <code>filament("red", 4);</code> auf oberster Ebene der Hauptdatei, eine Anweisung je Farbe. Begründung: der Code bleibt die einzige Quelle (CLI und UI exportieren dasselbe, Undo/Git sehen die Änderung, ein Agent liest es mit); die Tabelle gilt für genau eine Farbe und ist unabhängig davon, wo die Farbe entsteht (Parameter, Schleife, Bibliothek); es ist ein gewöhnlicher Modulaufruf (keine neue Syntax, OpenSCAD meldet nur ein unbekanntes Modul); ein Block <code>filaments { … }</code> bräuchte Parser-Änderungen, <code>$filaments = […]</code> hätte die Gültigkeitsregeln von <code>$</code>-Variablen und ließe sich an jeder Stelle überschreiben. Die UI schreibt <code>filament = n</code> weiter an die Anweisung, wenn <b>alle</b> Anweisungen der Farbe sie als Literal nennen (lesbar am Ort), sonst die Tabellenzeile; gibt es eine Tabellenzeile, ändert sie deren Slot, „automatisch“ entfernt die Zeile.</dd>
<dt>Regeln</dt><dd>Die Tabelle wird vor allen anderen Anweisungen ausgewertet (eigener Durchlauf) und hat Vorrang; ein widersprechendes <code>filament = m</code> warnt und wird ignoriert; <code>filament()</code> in Modulen, Schleifen oder Bibliotheken warnt („nur auf oberster Ebene der Hauptdatei“); ein eigenes Modul <code>filament</code> des Nutzers hat Vorrang.</dd>
<dt>Tests</dt><dd>${testsOf(/set_filament_of_a_parameter|filament_table_rules|filament_declarations/)}; Playwright G02; Tauri T15.</dd>
</dl>
${fig(pw("g02-tabelle"), "UI nach Slot 4 für red in slot_mod.scad: `filament(\"red\", 4);` im Code, blue und green automatisch (vorher bekamen alle drei Slot 4)")}
<p><code>cad outline</code> der Datei mit Tabelle:</p>${pre(T("f2-outline.txt").split("\n").filter((l) => /Farben|F\d/.test(l)).join("\n"))}
</section>

<section class="finding">
<h3>F3 (mittel) – Ansicht ≠ Druck</h3>
<dl class="f">
<dt>Ursache</dt><dd>(a) Bei Überlappung verschmilzt der Kernel koplanare Flächen beider Körper zu einer Fläche mit einem Tag – die Farbe je Fläche kann die Farbgrenze nicht zeigen. (b) Die Verrundung an einer Farbgrenze gehört dem ungefärbten <code>union</code>-Knoten (grau gezeigt), die Regionen geben sie der Restfarbe (rot). (c) <code>color()</code> im 2D-Profil: die Ansicht zeigte die Farbe des 2D-Blatts, die Farbtabelle sah unter Profiloperationen nicht nach (Standard, ohne Eintrag, ohne Warnung).</dd>
<dt>Korrektur</dt><dd>(a, b) Die Ansicht zeigt die Farbregionen (F1). (c) Haben alle 2D-Blätter einer Extrusion ohne eigene Farbe dieselbe Farbe, ist die Extrusion ein Körper dieser Farbe (wie <code>color()</code> um sie); gemischte 2D-Farben werden nicht getrennt: ein Körper in Standard und die Warnung „Farben im 2D-Profil von linear_extrude werden nicht getrennt …“.</dd>
<dt>Tests</dt><dd>${testsOf(/^f3_/)}; Playwright G03.</dd>
</dl>
${pair(before("r_overlap23.png"), "vorher: r_overlap2/3 (links), Regionen (rechts) – Prüfer", cli("f3-overlap3"), "nachher: r_overlap3.scad, blau besitzt die Überlappung")}
${pair(before("r_fillet2.png"), "vorher: Verrundung grau, exportiert rot (Prüfer)", cli("f3-fillet2"), "nachher: Verrundung rot wie im Druck")}
${pair(before("r_extrude_faces.png"), "vorher: 2D-Farbe gezeigt, als Standard exportiert (Prüfer)", cli("f3-extrude"), "nachher: rot und blau wie exportiert, gemischtes Profil Standard mit Warnung")}
${pre(T("f3-extrude.txt"))}
<p class="small">Grenze: gemischte Farben in einem 2D-Profil unter einer Extrusion werden nicht in Körper je Farbe geteilt (Warnung; Abhilfe: je Farbe extrudieren).</p>
</section>

<section class="finding">
<h3>F4 (mittel) – Material außerhalb von Bauteilen</h3>
<dl class="f">
<dt>Ursache</dt><dd>Farbliste und UI-Export nahmen nur die Bauteile (Rest: 0 mm³, im 3MF weggelassen); der CLI-Export ohne <code>--part</code> nahm die Wurzel als ein Objekt und meldete trotzdem „je Bauteil ein Objekt“.</dd>
<dt>Korrektur</dt><dd>Die Regionen umfassen die Bauteile und die obersten Knoten, die weder Bauteil sind noch eines enthalten („ohne Bauteil“, <code>cad_session::rest_nodes</code>); der Standard-Export (CLI ohne <code>--part</code> und UI, beide <code>Session::export_colors</code>) schreibt je Bauteil ein Objekt und das Objekt „ohne Bauteil“ – die Dateien von CLI und UI sind gleich. Die Meldung zählt die Objekte auf (<code>color_export_notes</code>), die UI zeigt sie in der Statuszeile.</dd>
<dt>Tests</dt><dd>${testsOf(/^f4_|material_outside/)}; Playwright G04; Tauri T15.</dd>
</dl>
${pre(T("f4-export.txt"))}
</section>

<section class="finding">
<h3>F5 (mittel) – Zufallsfarben kollidieren</h3>
<dl class="f">
<dt>Ursache</dt><dd>Nur der Farbton kam aus dem Hash (L, C fest): Namen fielen auf denselben Wert (<code>baz</code>/<code>violet</code>, <code>x</code>/<code>rose</code>) und wurden zu einem Eintrag und Slot zusammengelegt; andere lagen nah an Palettenfarben.</dd>
<dt>Korrektur</dt><dd>Farbton, Helligkeit (0,45–0,80) und Chroma (0,06–0,16) aus dem Hash; eine Kandidatenfolge je Name (SplitMix64); gewählt wird der erste Kandidat mit ΔE<sub>OK</sub> ≥ 0,07 zu Palette und Standard und – im Modell, in Reihenfolge des ersten Auftretens – ≥ 0,06 zu den Zufallsfarben anderer Namen. Verschiedene Namen fallen so nie zufällig zusammen; zusammengelegt wird nur bei exakt gleichem Wert (z. B. <code>"#f00"</code> und <code>[1, 0, 0]</code>).</dd>
<dt>Nachweis</dt><dd>${f5 ? `randnames.scad: ${f5.n} unbekannte Namen (36 Anweisungen, rot/Rot/ROT ist ein Name), ${f5.all} Einträge auf ${f5.slots} verschiedenen Slots; kleinster Abstand zweier Zufallsfarben ΔE<sub>OK</sub> ${de(f5.minPair.d, 3)} (${esc(f5.minPair.a)}/${esc(f5.minPair.b)}), zur Palette ${de(f5.minPal.d, 3)} (${esc(f5.minPal.a)} ↔ ${esc(f5.minPal.b)}).` : "–"}</dd>
<dt>Tests</dt><dd>${testsOf(/^f5_|random_colours_do_not/)}.</dd>
</dl>
${f5 ? `<p>${f5.rows.map((r) => `<span class="sw" style="background:${esc(r.key)}"></span>${esc(r.label)}`).join(" · ")}</p>` : ""}
<p class="small">Grenze: weicht ein Name im Modell aus, hängt seine Farbe von den Namen davor ab (deterministisch, aber nicht modellübergreifend gleich).</p>
</section>

<section class="finding">
<h3>F6 – Splitter bei gleichfarbigen Nachbarn</h3>
<dl class="f">
<dt>Ursache</dt><dd><code>Stück(C) = R ∩ M(C) − ⋃ M(später)</code> zog auch einen berührenden späteren Körper derselben Farbe ab; an der Berührfläche blieben dünne Splitter (0,00016 mm), die nach dem Vereinigen überlappten: r_touch.scad rot 2000,032 statt 2000 mm³, ohne Warnung.</dd>
<dt>Korrektur</dt><dd>Abgezogen wird nur das Material späterer Farbknoten <b>anderer</b> Farbe; die Stücke einer Farbe werden wie bisher vereinigt. Die Zuordnung ist dieselbe (gleiche Farbe), die Splitter entfallen.</dd>
<dt>Tests</dt><dd>${testsOf(/^f6_|all_review/)} (alle Fälle: Summe = Volumen auf 10⁻⁶).</dd>
</dl>
${pre(T("m-r_touch.txt"))}
</section>

<section class="finding">
<h3>F7 – Alpha, Klemmen, transparent, leerer Name</h3>
<dl class="f">
<dt>Regeln</dt><dd>Die Identität einer Druckfarbe ist ihr RGB-Wert: <code>alpha</code> wirkt nur in der Ansicht (die Farbregion wird transparent gezeichnet), <code>color("red", alpha = 0.5)</code> ist der Eintrag <code>red</code> (ein Slot, die Konfliktprüfung sieht über Alpha). Vektorwerte und <code>alpha</code> außerhalb 0–1 werden mit Warnung geklemmt (bei 0–255-Werten mit Hinweis auf <code>"#rrggbb"</code>). <code>color("transparent")</code> ist keine Druckfarbe: Warnung, das Material bleibt Standard (kein Filament <code>#00000000</code>); <code>color("")</code> und <code>color()</code> warnen und färben nicht.</dd>
<dt>Tests</dt><dd>${testsOf(/^f7_/)}.</dd>
</dl>
${pre(T("m-values.txt"))}
</section>

<section class="finding">
<h3>F8 – Slots über 16, geteilte Slots, einfarbiges 3MF</h3>
<dl class="f">
<dt>Korrektur</dt><dd>Die Farbtabelle meldet automatische Slots über 16 als Warnung („mehr als 16 Filamente …“), zwei Farben auf demselben festen Slot und einen festen Slot 1 neben Standard als Hinweis (Info). Das 3MF hat keine Farbgruppe mehr (Abschnitt 3), auch nicht mit <code>--color-group</code> bei nur einer Farbe.</dd>
<dt>Tests</dt><dd>${testsOf(/^f8_|no_color_group/)}.</dd>
</dl>
${pre(T("m-f1.txt"))}
</section>

<section class="finding">
<h3>F9 – --per-color, --part, use/include</h3>
<dl class="f">
<dt>Korrektur</dt><dd><code>--per-color</code> verlangt die Endung <code>.zip</code> (sonst Exit 2 mit Vorschlag); <code>--per-color --part</code> zeigt nur die Farben und Volumen der gewählten Bauteile; die Farbliste nennt Zeilen anderer Dateien als <code>lib.scad:1</code> (CLI und UI; in der UI öffnet der Klick die Datei an der Stelle).</dd>
<dt>Tests</dt><dd>${testsOf(/^f9_/)}; Vitest „Bericht 12 F9“.</dd>
</dl>
${pre(T("f9-stl.txt") + "\n" + T("f9-part.txt"))}
</section>

<section class="finding">
<h3>Daemon – Socketpfad länger als die Unix-Grenze</h3>
<dl class="f">
<dt>Ursache</dt><dd><code>sun_path</code> fasst 108 Byte (Linux; 107 ohne NUL). Mit einem tiefen <code>CAD_IPC_DIR</code> scheiterte <code>bind</code> („path must be shorter than SUN_LEN“), der Client rechnete still ohne Daemon.</dd>
<dt>Korrektur</dt><dd><code>cad_ipc::socket_path_in</code>: ist <code>&lt;dir&gt;/&lt;name&gt;.sock</code> zu lang, liegt der Socket in <code>/tmp/cad-&lt;uid&gt;/&lt;hash&gt;-&lt;name&gt;.sock</code> (Verzeichnis mit denselben Prüfungen: 0700, eigener Benutzer, kein Symlink; <code>hash</code> = FNV-1a des kanonischen Laufzeitverzeichnisses, damit sich mehrere lange Verzeichnisse nicht stören). Sperr- und Logdatei bleiben im Laufzeitverzeichnis; <code>list</code> findet auch Ersatz-Sockets (UI-Endpunkte); <code>cad daemon status</code> zeigt den Pfad mit „Ersatzpfad“ (JSON <code>socket_substitute</code>), das Log nennt ihn.</dd>
<dt>Tests</dt><dd>${testsOf(/long_ipc/)}.</dd>
</dl>
${pre(T("daemon-long.txt"))}
</section>

<h2>3 Bambu Studio: Farbdialog und Slots</h2>
<p>Quellen abgerufen am ${esc(date)} (<code>raw.githubusercontent.com</code>, BambuStudio <code>master</code>, OrcaSlicer <code>main</code>; Prüfsummen in <code>reports/tmp/report12/sources.sha256</code>):</p>
${pre(T("sources.txt"))}
<ul>
<li><b>Bedingung des Dialogs</b> (<code>Plater.cpp</code>): <code>load_model &amp;&amp; !color_group_map.empty() &amp;&amp; !volume_color_data.empty() &amp;&amp; en_3mf_file_type != From_BBS</code>. <code>volume_color_data</code> füllt der Importer nur für Teile, deren Dreiecke über <code>pid</code>/<code>pindex</code> eine Farbe haben (<code>bbs_3mf.cpp</code>, <code>triangle_colors.size() == triangles_count</code>).</li>
<li><b>OK im Dialog</b> ruft <code>apply_volume_filament_ids</code> und setzt <code>extruder</code> jedes Teils neu; „Abbrechen“ und Schließen lassen ihn.</li>
<li><b><code>Metadata/model_settings.config</code></b> liest der Importer für alle 3MF (nicht an die Versionsprüfung gebunden) und übernimmt <code>extruder</code> je Teil und Objekt.</li>
<li><b>Wege, den Dialog zu vermeiden:</b> (1) keine Farbgruppe – gewählt; (2) <code>Application = BambuStudio-…</code> – verworfen: gibt einen fremden Hersteller an, schaltet die Projekt-Pfade des Importers ein (Versionsprüfung, erwartete Projektdateien, Platten) und könnte bei neuen Bambu-Versionen brechen; (3) <code>project_settings.config</code> mit <code>filament_colour</code> – verworfen: wird für fremde Dateien nicht geladen (<code>dont_load_config</code> ohne Bambu-Version), OrcaSlicer würde es über Drucker- und Prozesseinstellungen legen; (4) Paint-Daten (<code>mmu_segmentation</code>) – unnötig, die Teile tragen den Slot bereits.</li>
<li><b>OrcaSlicer</b> hat diesen Dialog nicht; Farbgruppen nutzt sein Importer nur für Objekte ohne Volumen-Metadaten (<code>color_group_id_to_extruder_id_map</code>), mit <code>model_settings.config</code> gelten die <code>extruder</code>-Werte. Der Verzicht auf die Farbgruppe ist also mit Orca verträglich.</li>
</ul>
<p><b>Umsetzung:</b> das mehrfarbige 3MF hat keine <code>m:colorgroup</code> und keine <code>pid</code>/<code>pindex</code> mehr; die Slots stehen nur in <code>model_settings.config</code>. Die Farben der Slots kommen in Bambu Studio aus den Filamenten des Druckers (AMS). Für Programme, die 3MF-Farben anzeigen, schreibt <code>cad export --color-group</code> die Farbgruppe weiter; die Meldung sagt dann „Bambu Studio öffnet einen Farbdialog – dort ‚Abbrechen‘ wählen“.</p>
<table>
<tr><th>Datei</th><th>lib3mf</th><th>trimesh</th><th>Bambu-Modell: extruder je Teil</th><th>Farbdialog (Modell von Plater.cpp)</th></tr>
<tr><td>mehrfarbig.3mf (Standard)</td><td>${verify ? (verify.results?.lib3mf ? '<span class="ok">ok</span>' : '<span class="bad">Fehler</span>') : "–"}</td><td>${verify ? (verify.results?.trimesh ? '<span class="ok">dicht</span>' : '<span class="bad">offen</span>') : "–"}</td><td>${esc(verify?.bambu_model?.objects?.map((o) => `${o.name}: ${o.volumes.map((v) => v.extruder).join("/")}`).join(", ") ?? "–")}</td><td>${verify ? (verify.bambu_model?.color_dialog ? "öffnet" : '<span class="ok">öffnet nicht</span>') : "–"}</td></tr>
<tr><td>mehrfarbig-cg.3mf (<code>--color-group</code>)</td><td>${verifyCg ? (verifyCg.results?.lib3mf ? '<span class="ok">ok</span>' : '<span class="bad">Fehler</span>') : "–"}</td><td>${verifyCg ? (verifyCg.results?.trimesh ? '<span class="ok">dicht</span>' : '<span class="bad">offen</span>') : "–"}</td><td>${esc(verifyCg?.bambu_model?.objects?.map((o) => `${o.name}: ${o.volumes.map((v) => v.extruder).join("/")}`).join(", ") ?? "–")}</td><td>${verifyCg ? (verifyCg.bambu_model?.color_dialog ? "öffnet („Abbrechen“ behält Slots)" : "öffnet nicht") : "–"}</td></tr>
</table>
${pre(T("export.txt"))}
<p class="small">Grenze: am Programm selbst (Bambu Studio, OrcaSlicer) nicht geprüft – kein grafisches Programm im Container. Ob Bambu Studio fehlende Filamente beim Laden ergänzt, wenn ein Teil Slot 5 nutzt und das Projekt nur vier Filamente hat, hängt vom Ladeweg ab (<code>Plater.cpp</code>: bei „nur Geometrie“ werden Filamente bis zur höchsten Nummer angelegt).</p>

<h2>4 UI und Desktop</h2>
${scenarios.map((s) => `<h3>${esc(s.id)} ${esc(s.title)}</h3><p>${md(s.caption)}</p>${(s.images ?? []).map((i) => fig(pw(i.name), i.label)).join("")}<ul>${(s.checks ?? []).map((c) => `<li>${md(c)}</li>`).join("")}</ul>`).join("\n")}
${tauri ? (() => {
  const t15 = tauri.results.find((r) => r.id === "T15");
  return t15 ? `<h3>T15 (Tauri) ${esc(t15.title)}</h3><ul>${t15.checks.map((c) => `<li class="${c.ok ? "" : "bad"}">${md(c.text)}</li>`).join("")}</ul>${t15.images.map((i) => fig(tauriImg(i.name), i.label)).join("")}` : "";
})() : ""}

<h2>5 Testläufe</h2>
<table>
<tr><th>Lauf</th><th>Ergebnis</th></tr>
<tr><td><code>cargo test --workspace --no-fail-fast</code></td><td>${cargoSum ? `${cargoSum.ok} bestanden, ${cargoSum.failed} fehlgeschlagen` : "–"}</td></tr>
<tr><td><code>cargo clippy --workspace --all-targets -- -D warnings</code> · <code>cargo fmt --check</code></td><td>${clippyOk ? "sauber" : "Befund"} · ${fmtOk ? "sauber" : "Abweichung"}</td></tr>
<tr><td><code>pnpm typecheck</code> · <code>pnpm test</code> (Vitest)</td><td>${readText(join(input, "typecheck.txt")) !== null ? (/error/i.test(readText(join(input, "typecheck.txt"))) ? "Fehler" : "sauber") : "–"} · ${vitestSum ?? "–"} bestanden</td></tr>
<tr><td><code>pnpm e2e</code> (Playwright, Chromium)</td><td>${pwStats ? `${pwStats.passed} / ${pwStats.total}${pwStats.failed.length ? ` – fehlgeschlagen: ${pwStats.failed.map((f) => esc(f.title)).join(", ")}` : ""}` : "–"}</td></tr>
<tr><td>Tauri-E2E (<code>web/e2e-tauri/desktop.e2e.mjs</code>)</td><td>${tauri ? `${tauriOk} / ${tauri.results.length}${tauri.results.filter((r) => !r.ok).map((r) => ` – ${esc(r.id)} fehlgeschlagen`).join("")}` : "–"}</td></tr>
<tr><td><code>tests/openscad-compat/compare.py</code></td><td>${compareSum ? `${compareSum[0]}/${compareSum[1]}` : "–"}</td></tr>
<tr><td><code>tests/step/verify.py</code></td><td>${esc(stepSum ?? "–")}</td></tr>
<tr><td><code>tests/alltag/check.sh</code> (Volumen der Alltagsmodelle)</td><td>${esc(alltagLine ?? "–")}</td></tr>
</table>
<p>Neue Rust-Tests:</p>
<ul>${newTests.map((t) => `<li><code>${esc(t.name)}</code> ${t.res === "ok" ? '<span class="ok">ok</span>' : '<span class="bad">FAILED</span>'}</li>`).join("")}</ul>

<h2>6 Grenzen</h2>
<ul>
<li>Die Ansicht zeigt die exakten Regionen erst nach ihrer Berechnung (Booleans je Farbknoten, gecacht); bis dahin, beim Ziehen und für den hervorgehobenen Teilbaum unter dem Cursor gilt die genäherte Farbe je Fläche. Ist der Cursor auf einer Anweisung, wird der Rest gedimmt gezeigt (wie bisher).</li>
<li>STEP teilt keine Flächen an Farbgrenzen (eine verschmolzene Fläche bekommt die Mehrheitsfarbe).</li>
<li>Gemischte Farben in einem 2D-Profil unter einer Extrusion werden nicht getrennt (Warnung).</li>
<li>Zufallsfarben, die im Modell ausweichen, hängen von den Namen davor ab.</li>
<li>Bambu Studio und OrcaSlicer wurden nicht am Programm geprüft, sondern über ihren Quellcode und ein Modell des Importers (<code>tools/report/verify3mf.py</code>).</li>
<li>Der Ersatz-Socket liegt fest unter <code>/tmp</code> (nicht <code>$TMPDIR</code>, das selbst lang sein kann).</li>
</ul>
</main>
</body>
</html>
`;
writeFileSync(join(out, "report.html"), html);
console.log(`${join(out, "report.html")}: ${html.length} Zeichen, ${scenarios.length} Szenarien, Tauri ${tauriOk ?? "–"}`);
