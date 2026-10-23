#!/usr/bin/env node
// Test report 14 ("Skizzen und Namensräume", Plan 17.7 Schritt 1): sketches with mixed
// contours (lines, arcs, Bézier curves, splines) through the kernel, modules as namespaces,
// display and handles in the UI. Writes docs/reports/14-skizzen/report.html (images in img/).
//
// Inputs (gitignored, reports/tmp/report14/):
//   cli/*.png, volumes.tsv, step.tsv, outline.txt, check-*.txt   tools/report/report14-data.sh
//   scenarios.json, img/*.jpg                                   cd web && pnpm exec playwright test e2e/skizzen.spec.ts
//   playwright.json, playwright.txt                             whole suite (pnpm e2e; copy of reports/tmp/report2/playwright.json)
//   vitest.txt, typecheck.txt                                   cd web && pnpm test, pnpm typecheck
//   cargo.txt, clippy.txt, fmt.txt                              cargo test --workspace --no-fail-fast, clippy -D warnings, fmt --check
//   compare.txt, step.txt, alltag.txt, node-test.txt            compare.py --eval, verify.py, tests/alltag/check.sh, test-node.mjs
//
//   node tools/report/report14.mjs
//   node tools/report/print-pdf.mjs docs/reports/14-skizzen/report.html docs/reports/14-skizzen/report.pdf "Testbericht 14: Skizzen und Namensräume"

import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import os from "node:os";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "../..");
const input = join(root, "reports/tmp/report14");
const out = join(root, "docs/reports/14-skizzen");
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
const sci = (x) => (x === null || x === undefined || !Number.isFinite(x) ? "–" : x === 0 ? "0" : x.toExponential(1).replace("e", "·10^").replace(/\^(-?)\+?(\d+)/, (_, s, n) => `<sup>${s === "-" ? "−" : ""}${n}</sup>`));
const md = (t) => esc(t).replace(/`([^`]+)`/g, "<code>$1</code>");
const T = (name) => readText(join(input, name))?.replaceAll(`${input}/`, "").replaceAll(`${root}/`, "") ?? "(fehlt)";
const pre = (t) => `<pre>${esc(t.trimEnd())}</pre>`;
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
const newTests = cargo
  ? [...cargo.matchAll(/^test (\S+) \.\.\. (ok|FAILED)$/gm)]
      .map((m) => ({ name: m[1], res: m[2] }))
      .filter((t) => /sketch|namespace|qualified|curve2::|arc_forms|segments_of|errors_per_segment|rectangle_with|revolve_arc|holes_by|bezier_and|loft_between|step_has_exact|sweep_along|several_contours|handles_of_segments|loops_variables|openscad_files_are|module_without|spec_models/.test(t.name))
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
  return { total: all.length, passed: all.filter((t) => t.status === "passed").length, failed: all.filter((t) => t.status !== "passed" && t.status !== "skipped"), neu: all.filter((t) => t.file?.includes("skizzen")) };
})();
const vitest = readText(join(input, "vitest.txt"));
const vitestSum = vitest ? (/Tests\s+(\d+) passed/.exec(vitest)?.[1] ?? null) : null;
const compare = readText(join(input, "compare.txt"));
const compareSum = compare ? /(\d+)\/(\d+)/.exec(compare.split("\n").reverse().find((l) => /\d+\/\d+ files/.test(l)) ?? "")?.slice(1, 3) : null;
const step = readText(join(input, "step.txt"));
const stepSum = step ? (/\d+\/\d+ passed/.exec(step)?.[0] ?? null) : null;
const stepSketch = step ? step.split("\n").filter((l) => /^skizze/.test(l)) : [];
const alltag = readText(join(input, "alltag.txt"));
const alltagLine = alltag ? alltag.trim().split("\n").at(-1) : null;
const nodeTest = readText(join(input, "node-test.txt"));
const scenarios = (readJson(join(input, "scenarios.json")) ?? []).sort((a, b) => a.id.localeCompare(b.id));
const flaky = readText(join(input, "flaky.txt"));

const volumes = (readText(join(input, "volumes.tsv")) ?? "")
  .trim()
  .split("\n")
  .filter(Boolean)
  .map((l) => {
    const [name, got, want, ms, code] = l.split("\t");
    const g = Number(got);
    const w = Number(want);
    return { name, got: g, want: w, rel: Math.abs(g - w) / Math.abs(w), ms: Number(ms), code };
  });
const steps = (readText(join(input, "step.tsv")) ?? "")
  .trim()
  .split("\n")
  .filter(Boolean)
  .map((l) => {
    const [name, cnt, info, vol] = l.split("\t");
    return { name, cnt: JSON.parse(cnt), info: JSON.parse(info), vol: Number(vol) };
  });

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

const VNAMES = {
  rechteck_halbkreis: "Rechteck 40 × 20 + Halbkreis r 20 (Endpunkt + Radius), h 5",
  tangentialbogen: "dasselbe mit tangentialem Bogen",
  mittelpunkt_winkel: "dasselbe mit Mittelpunkt + Winkel",
  kugel: "rotate_extrude eines Halbkreises: Kugel r 7",
  torus: "rotate_extrude zweier Bögen: Torus R 20, r 4",
  ring_windung: "Quadrat 20 minus Kreis r 5 (Gegenlauf, nonzero)",
  ring_hole: "dasselbe mit `hole { … }`",
  bezier_quadratisch: "Quadrat 4 + quadratische Bézier-Kappe (Parabel 2/3 · b · h)",
  stern_schleife: "Stern aus 12 Geraden per Schleife in einem Skizzen-Modul",
  rohr_pfad: "sweep eines Kreises r 2 entlang Gerade–Bogen–Gerade (Pappus)",
  gehrung_pfad: "sweep eines Quadrats 2 entlang einer Ecke (Gehrung)",
  twist_bogen: "linear_extrude(twist = 90) eines Langlochs (Volumen ohne twist)",
};

const html = `<!doctype html>
<html lang="de">
<head>
<meta charset="utf-8">
<title>Testbericht 14 – Skizzen und Namensräume</title>
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
  figure img { max-width: 100%; max-height: 92mm; border: 1px solid var(--border); border-radius: 1mm; }
  figcaption { font-size: 7.8pt; color: var(--muted); }
  .two { display: grid; grid-template-columns: 1fr 1fr; gap: 3mm; align-items: start; }
  .codefig { display: grid; grid-template-columns: 1fr 1fr; gap: 3mm; align-items: start; }
  .note { border-left: 1mm solid #e8b84a; padding: 1mm 2.5mm; background: #fff8e6; margin: 2mm 0; }
</style>
</head>
<body>
<main>
<h1>Testbericht 14 – Skizzen und Namensräume</h1>
<div class="meta">Plan 17.7, Schritt 1: <code>sketch { … }</code> mit gemischten Konturen, Module als Namensräume · cad-syntax, cad-eval, cad-ir, cad-kernel, cad-engine, kernel-smlib (Fassade <code>sc_sketch.cpp</code>), cad-step, cad-session, cad-cli, web · ${esc(date)} · Branch ${esc(branch)} · Commit ${esc(commit)} · ${esc(rustc)} · ${os.cpus().length} Kerne</div>

<div class="summary">
  <div>cargo test --workspace<b class="${cargoSum && cargoSum.failed === 0 ? "ok" : "bad"}">${cargoSum ? `${cargoSum.ok} / ${cargoSum.ok + cargoSum.failed}` : "–"}</b>davon ${newTests.length} neu/erweitert</div>
  <div>clippy · fmt<b class="${clippyOk && fmtOk ? "ok" : "bad"}">${clippy == null ? "–" : clippyOk ? "sauber" : "Befund"} · ${fmt == null ? "–" : fmtOk ? "sauber" : "Abw."}</b>-D warnings</div>
  <div>Vitest · Playwright<b class="${pwStats && pwStats.failed.length === 0 ? "ok" : "bad"}">${vitestSum ?? "–"} · ${pwStats ? `${pwStats.passed} / ${pwStats.total}` : "–"}</b>neu: S01–S05</div>
  <div>OpenSCAD · STEP<b class="${compareSum && compareSum[0] === compareSum[1] ? "ok" : "bad"}">${compareSum ? `${compareSum[0]}/${compareSum[1]}` : "–"} · ${esc(stepSum?.match(/\d+\/\d+/)?.[0] ?? "–")}</b>compare.py · verify.py (OCCT)</div>
  <div>Alltagsmodelle<b class="${alltag && /0 mit Problemen/.test(alltag) ? "ok" : "bad"}">${alltag && /0 mit Problemen/.test(alltag) ? "unverändert" : "–"}</b>tests/alltag/check.sh</div>
  <div>Wasm-Kernel (Node)<b class="${nodeTest && /all node tests passed/.test(nodeTest) ? "ok" : "bad"}">${nodeTest && /all node tests passed/.test(nodeTest) ? "bestanden" : "–"}</b>test-node.mjs</div>
</div>

<h2>1 Auftrag und Ergebnis</h2>
<p>Schritt 1 von Plan 17: <b>Skizzen</b> – ein 2D-Block aus Segmenten (Geraden, Kreisbögen, Bézier-Kurven, Splines), jede Zeile ein Segment mit eigener Darstellung, eigenen Griffen und eigenen Fehlern – als Profil für <code>linear_extrude</code>, <code>rotate_extrude</code>, <code>loft</code> und <code>sweep</code> (auch als Pfad), exakt durch den Kernel bis in STEP; und <b>Module als Namensräume</b> (<code>teile.schraube(d = 3)</code>). Der Zeichenmodus (Schritt 4) und <code>corner</code>/<code>offset</code>/<code>stroke</code> (Schritt 2) gehören nicht dazu; <code>corner()</code> ist reserviert (Warnung).</p>
<ul>
<li>Volumen gegen Formeln (Abschnitt 5): alle Fälle innerhalb der Integrationsgenauigkeit von SMLib (≤ ${sci(Math.max(...volumes.filter((v) => v.name !== "twist_bogen").map((v) => v.rel)))} relativ; die twist-Extrusion ist eine kubische Loft-Näherung wie bisher).</li>
<li>STEP: Bögen als <code>CIRCLE</code>, Geraden als <code>LINE</code>, extrudierte Bögen als <code>CYLINDRICAL_SURFACE</code>, keine angenäherten Kanten; OpenCASCADE liest alle vier neuen Beispiele mit gleichem Volumen (≤ 10<sup>−6</sup>).</li>
<li>OpenSCAD-Dateien unverändert: compare.py ${compareSum ? `${compareSum[0]}/${compareSum[1]}` : "–"}; Benutzermodule namens <code>line</code>/<code>arc</code> und <code>spline(points)</code> außerhalb von Skizzen verhalten sich wie vorher (Tests).</li>
</ul>

<h2>2 Skizzen: Sprache</h2>
<p>Entscheidungen (Plan 17.8): <code>sketch</code> ist ein eingebautes 2D-Modul; <code>sketch {</code> und <code>hole {</code> gehen ohne Klammern; Segmente sind gewöhnliche Anweisungen, die an die gerade ausgewertete Skizze anhängen – auch aus Modulen und Schleifen heraus. In einer Skizze haben Segmentnamen Vorrang vor gleichnamigen Benutzermodulen, außerhalb bleibt alles wie in OpenSCAD. Andere Geometrie und Transformationen in einer Skizze sind ein Fehler. Jede Skizze ist <b>ein</b> Knoten <code>Profile::Sketch</code>; die Anweisungen der Segmente stehen in <code>EvalOutput::sketches</code>, damit der Cache-Schlüssel nicht von Quellpositionen abhängt.</p>
<table>
<tr><th>Segment</th><th>Formen</th></tr>
<tr><td><code>start(p)</code></td><td>beginnt eine Kontur; Punkte dürfen Rasterpunkte und Anker sein (z ≠ 0: Warnung, projiziert)</td></tr>
<tr><td><code>line</code></td><td><code>to = p</code>, <code>by = v</code> (relativ), <code>length = l, angle = a</code>; <code>length</code> allein setzt die Richtung fort</td></tr>
<tr><td><code>arc</code></td><td><code>to, r [, large, cw]</code> wie SVG · <code>center, angle</code> (Grad, Vorzeichen = Richtung) · <code>center, to</code> · <code>through, to</code> · <code>to</code> allein = tangential</td></tr>
<tr><td><code>bezier</code></td><td><code>c1, to</code> (quadratisch) · <code>c1, c2, to</code> (kubisch)</td></tr>
<tr><td><code>spline</code></td><td>Punkte nach dem aktuellen Punkt, <code>tangent = true | v</code>, <code>end_tangent = v</code>; kubische globale Interpolation (Sehnenlänge, gemittelte Knoten)</td></tr>
<tr><td><code>close()</code></td><td>Gerade zum Start; endet die Kontur am Start, ist sie auch ohne <code>close()</code> geschlossen</td></tr>
<tr><td><code>hole { … }</code></td><td>Konturen darin sind Löcher (in jeder Laufrichtung)</td></tr>
</table>
<p><b>Löcher und mehrere Konturen:</b> Windungsregel <i>nonzero</i>; eine Kontur ist Rand, wo die Windungszahl nur auf einer Seite ≠ 0 ist; Inseln in Löchern über die Verschachtelung. <b>Sweep:</b> ohne <code>path</code> ist eine offene Skizze (<code>sketch(closed = false)</code>) unter den Kindern der Pfad. <b>Loft:</b> gleiche Segmentzahl → Segment für Segment (jedes Segment eine Fläche, Zuordnung durch <code>start</code>); sonst wird jeder Abschnitt eine geschlossene B-Spline, die am +x-Strahl vom Schwerpunkt beginnt (wie Kreise/Polygone), damit nichts verdreht.</p>
<h3>Beispiele (<code>examples/spec/skizze_*.scad</code>)</h3>
<div class="codefig">${pre(src("examples/spec/skizze_profil.scad"))}<div>${fig(cli("profil"), "skizze_profil.scad: Halter (Bögen, Bohrung im Gegenlauf, Langloch per `hole`), Drehknopf (tangentialer Bogen, kubische Bézier)")}${fig(cli("profil-oben"), "von oben")}</div></div>
<div class="codefig">${pre(src("examples/spec/skizze_zahnriemenscheibe.scad"))}<div>${fig(cli("scheibe"), "skizze_zahnriemenscheibe.scad: 20 Zahnlücken per Schleife (Bogen durch einen Punkt + Bogen um die Mitte), Nabe als zweite Kontur")}${fig(cli("scheibe-oben"), "von oben")}</div></div>
<div class="codefig">${pre(src("examples/spec/skizze_sweep.scad"))}${fig(cli("sweep"), "skizze_sweep.scad: Rohrbogen (glatter Pfad: ein Kurvenzug, Kreisring als Profil), Rahmen entlang eines Pfads mit Ecken (Gehrung)")}</div>

<h2>3 Module als Namensräume</h2>
<p><code>module name { … }</code> ohne Klammern; Aufrufe <code>a.b.c(…)</code>, auch benannt (<code>k: teile.schraube();</code>). Der erste Teil wird lexikalisch gesucht, jeder weitere nur direkt im Körper des vorigen Moduls. Der Körper ist der Namensraum, mit den Parametern des äußeren Moduls auf ihren Standardwerten; er gehört zu keiner Instanz – Raster, benannte Instanzen und Anker des äußeren Moduls sieht das innere nicht, die Geometrie des äußeren entsteht nicht. <b>Entscheidung Funktionen/Konstanten:</b> erreichbar (<code>teile.kopf(d)</code>, <code>teile.M3</code>) – Bibliotheken bündeln Hilfsfunktionen und Maße mit ihren Modulen ohne eigene Datei; OpenSCAD kennt keinen Punkt hinter Modulnamen (kein Konflikt), eine Variable gleichen Namens verdeckt das Modul. Kontext: <code>cad --module teile.platte.lochplatte</code>, UI-Pfadleiste „teile › rohr“ (S05).</p>
<div class="codefig">${pre(src("examples/spec/skizze_bibliothek.scad"))}<div>${fig(cli("bibliothek"), "skizze_bibliothek.scad: profile.stern(), profile.langloch(), teile.platte.lochplatte(n = 4), teile.schraube(), Scheibe mit d = teile.kopf(teile.M3)")}${pre(T("outline.txt").split("\n").slice(0, 12).join("\n"))}${pre(T("check-modul.txt"))}</div></div>

<h2>4 Kernel-Anbindung</h2>
<ul>
<li><code>cad_kernel::Segment::{Line, Arc, Nurbs}</code> (<code>curve2.rs</code>): NURBS-Form, affine Abbildung (Bögen bleiben unter Ähnlichkeiten Bögen, sonst exakte rationale Ellipsenbögen), Abtastung, globale Interpolation, Verbinden zu einer B-Spline (Bézier-Zerlegung, Graderhöhung); <code>Loop::Composite</code>, <code>PlacedPath</code>, <code>Kernel::sweep_path</code>.</li>
<li>Fassade <code>smlib/facade/sc_sketch.cpp</code>, eingebunden am Ende von <code>sc_facade.cpp</code> (nutzt dessen interne Hilfen; allein übersetzt leer). Schleifenart 3: Geraden → <code>SmLine</code>, Bögen → rationale quadratische B-Splines (<code>SM_CF_CIRCULAR_ARC</code>), Bézier/Splines → B-Splines; Stöße exakt verschweißt. Änderungen an bestehenden Funktionen: ein Feld in <code>Loop</code>, je ein Zweig in <code>parse_loops</code>/<code>loop_curves</code>, die Bedingung im twist-Zweig von <code>sc_extrude</code>, <code>loft_segmentwise</code> in <code>sc_loft</code>. Neu: <code>sc_sketch_sweep</code> (glatter Pfad: Kurven-Sweep mit Profil-Bezugspunkt aus Patch 0003; Ecken: Gehrung).</li>
<li>STEP: SMLib hält extrudierte Bögen als <code>SmSurfOfExtrusion</code> der exakten B-Spline; OpenCASCADE rechnete auf solchen Flächen (ohne p-Kurven) bis 2,5·10<sup>−3</sup> falsche Volumen. Der Writer schreibt eine Extrusion eines Kreisbogens längs seiner Achse deshalb als <code>CYLINDRICAL_SURFACE</code> (Orientierung angepasst) – danach stimmt OCCT auf 10<sup>−9</sup> bzw. die Kernel-Genauigkeit.</li>
</ul>
<table>
<tr><th>Modell</th><th>Flächen</th><th>CIRCLE</th><th>LINE</th><th>B-Spline-Kurven</th><th>Flächentypen</th><th>angenähert</th></tr>
${steps.map((s) => `<tr><td>${esc(s.name)}</td><td class="n">${esc(s.info.faces)}</td><td class="n">${s.cnt.CIRCLE}</td><td class="n">${s.cnt.LINE}</td><td class="n">${s.cnt.B_SPLINE_CURVE_WITH_KNOTS}</td><td>${esc(Object.entries(s.info.surfaces ?? {}).map(([k, v]) => `${k} ${v}`).join(", "))}</td><td class="n">${esc(s.info.approximated_edges)} / ${esc(s.info.approximated_faces)}</td></tr>`).join("\n")}
</table>
<p>OpenCASCADE (<code>tests/step/verify.py</code>) für die neuen Beispiele (Spalten: Körper, Flächen, V Kernel, V OCCT, dV, dA, dBox):</p>
${pre(stepSketch.join("\n"))}

<h2>5 Messwerte: Volumen gegen Formeln</h2>
<p>Je Fall eine kleine Datei (<code>reports/tmp/report14/cases/</code>), <code>cad measure --volume</code> mit dem Release-Build (Zeit: ganzer CLI-Aufruf ohne Daemon). Die Geometrie ist exakt; die Abweichung ist die der Volumenintegration in SMLib.</p>
<table>
<tr><th>Fall</th><th>Kernel mm³</th><th>Formel mm³</th><th>rel. Abw.</th><th>ms</th></tr>
${volumes.map((v) => `<tr><td>${md(VNAMES[v.name] ?? v.name)}</td><td class="n">${de(v.got, 4)}</td><td class="n">${de(v.want, 4)}</td><td class="n">${sci(v.rel)}</td><td class="n">${de(v.ms)}</td></tr>`).join("\n")}
</table>
<p class="small">twist: die Extrusion mit Verdrehung ist (wie bisher für alle Profile) eine kubische Loft-Näherung durch Schnitte alle 7,5°.</p>

<h2>6 Fehler je Segmentzeile</h2>
${pre(T("cases/fehler.scad"))}
${pre(T("check-fehler.txt"))}
<p>Geprüft werden: Kontur nicht geschlossen (die Skizze bleibt mit einer rot gestrichelten Schlusslinie sichtbar), Selbstüberschneidung (an der späteren Zeile, mit Verweis), unmöglicher Bogen (Radius &lt; halbe Sehne, drei Punkte auf einer Geraden, tangential auf der Tangente, Winkel 0 oder &gt; 360°), Kontur ohne Fläche, Segment vor <code>start</code>/nach <code>close()</code>. Eine sich schneidende Kontur meldet zusätzlich der Kernel an der Extrusion.</p>

<h2>7 UI: Darstellung und Griffe</h2>
<p>Steht der Cursor in einer Skizze (Segmentzeile, Schleife, <code>hole</code>, Aufruf eines Skizzen-Moduls), zeichnet die Ansicht sie in ihrer Ebene: Netz als Umriss mit leichter Füllung (durch den Körper sichtbar), Segmente als Linien, das Segment der Cursorzeile dick in der Akzentfarbe, Fehler rot, Kontrollpolygone und Bogenmittelpunkte gestrichelt, Start als Ring mit Richtungspfeil, Ende offener Konturen als Quadrat. Griffe: End-/Startpunkte (Literal oder Raster-Snap; <code>line(by = …)</code> relativ, ohne Snap), Bogenradius, Mittelpunkt, Durchgangspunkt, Bézier-Kontrollpunkte, Spline-Punkte. Prüfungen über den Zustand der UI (<code>window.__cad</code>) und Pixel der Ebenen.</p>
${scenarios.map((s) => `<h3>${esc(s.id)} ${esc(s.title)}</h3><p>${md(s.caption)}</p>${(s.images ?? []).length === 2 ? pair(pw(s.images[0].name), s.images[0].label, pw(s.images[1].name), s.images[1].label) : (s.images ?? []).map((i) => fig(pw(i.name), i.label)).join("")}${s.code && s.code.split("\n").length <= 16 ? pre(s.code) : ""}<ul>${(s.checks ?? []).map((c) => `<li>${md(c)}</li>`).join("")}</ul>`).join("\n")}

<h2>8 Testläufe</h2>
<table>
<tr><th>Lauf</th><th>Ergebnis</th></tr>
<tr><td><code>cargo test --workspace --no-fail-fast</code></td><td>${cargoSum ? `${cargoSum.ok} bestanden, ${cargoSum.failed} fehlgeschlagen` : "–"}</td></tr>
<tr><td><code>cargo clippy --workspace --all-targets -- -D warnings</code> · <code>cargo fmt --check</code></td><td>${clippyOk ? "sauber" : "Befund"} · ${fmtOk ? "sauber" : "Abweichung"}</td></tr>
<tr><td><code>pnpm typecheck</code> · <code>pnpm test</code> (Vitest)</td><td>${readText(join(input, "typecheck.txt")) !== null ? (/error TS/i.test(readText(join(input, "typecheck.txt"))) ? "Fehler" : "sauber") : "–"} · ${vitestSum ?? "–"} bestanden</td></tr>
<tr><td><code>pnpm e2e</code> (Playwright, Chromium)</td><td>${pwStats ? `${pwStats.passed} / ${pwStats.total}${pwStats.failed.length ? ` – fehlgeschlagen: ${pwStats.failed.map((f) => esc(f.title)).join(", ")}` : ""}` : "–"}${flaky ? ` · ${md(flaky.trim())}` : ""}</td></tr>
<tr><td><code>tests/openscad-compat/compare.py --eval</code></td><td>${compareSum ? `${compareSum[0]}/${compareSum[1]}` : "–"}</td></tr>
<tr><td><code>tests/step/verify.py</code> (OpenCASCADE)</td><td>${esc(stepSum ?? "–")}</td></tr>
<tr><td><code>tests/alltag/check.sh</code></td><td>${esc(alltagLine ?? "–")}</td></tr>
<tr><td><code>node crates/kernel-smlib/js/test-node.mjs</code></td><td>${nodeTest && /all node tests passed/.test(nodeTest) ? "bestanden" : "–"}</td></tr>
</table>
<p>Neue bzw. erweiterte Rust-Tests (${newTests.length}):</p>
<p class="small">${newTests.map((t) => `<code>${esc(t.name)}</code> ${t.res === "ok" ? '<span class="ok">ok</span>' : '<span class="bad">FAILED</span>'}`).join(" · ")}</p>
<p class="small">Vitest: <code>web/src/logic/sketch.test.ts</code> (Skizze unter dem Cursor, Hilfslinien, Griffe der Segmente, Modul mit Pfad); Lezer-Grammatik für <code>sketch {</code>, Module ohne Klammern und <code>a.b()</code>. CLI: <code>sketch_and_namespaces</code> (Volumen, Funktion im Namensraum, <code>outline</code>, <code>--module teile.platte</code>, Fehler je Zeile).</p>

<h2>9 Grenzen</h2>
<ul>
<li><code>corner</code>, <code>fillet2d</code>, <code>offset</code>, <code>stroke</code> fehlen (Schritt 2); keine Transformationen innerhalb einer Skizze (ein in der Skizze aufgerufenes Modul zeichnet in Skizzenkoordinaten).</li>
<li>Selbstüberschneidung wird auf abgetasteten Polylinien geprüft (Toleranz relativ zur Skizzengröße); Überschneidungen zwischen Konturen sind erlaubt (nonzero); die Verschachtelung von Löchern und Inseln setzt sich nicht schneidende Konturen voraus.</li>
<li>Sweep entlang einer Skizze mit Ecken nur als Gehrung und mit um den Ursprung zentriertem Profil; geschlossene Pfade nicht.</li>
<li>Loft mit verschiedener Segmentzahl verbindet jeden Abschnitt zu einer Kurve (C0 an den Stößen): die Flächen folgen nicht mehr den Segmenten.</li>
<li>STEP: nur Extrusionen von Kreisbögen werden als Zylinder geschrieben; Rotationsflächen von Bögen bleiben <code>SURFACE_OF_REVOLUTION</code> der exakten Kurve (Kugel/Torus nicht erkannt; OCCT liest sie richtig).</li>
<li><code>cad render --line N</code> einer Segmentzeile hebt die ganze Skizze hervor; die Hervorhebung des einzelnen Segments gibt es nur in der UI.</li>
<li>Namensräume: Konstanten werden je Zugriff in einem neuen Namensraum ausgewertet (keine Zwischenspeicherung); <code>--module name</code> ohne Pfad nimmt das erste Modul des Namens.</li>
</ul>
</main>
</body>
</html>
`;
writeFileSync(join(out, "report.html"), html);
console.log(`${join(out, "report.html")}: ${html.length} Zeichen, ${scenarios.length} Szenarien, ${volumes.length} Volumenfälle`);
