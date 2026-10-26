#!/usr/bin/env node
// Test report 15 ("Skizzen: Ecken, Offset, Stroke", Plan 17.7 Schritt 2): corner() in sketches,
// fillet2d/chamfer2d with zones, offset (OpenSCAD-compatible, exact arcs), stroke of open paths,
// exact 2D regions in the kernel, display and handles in the UI. Writes
// docs/reports/15-skizzen-ecken-offset/report.html (images in img/).
//
// Inputs (gitignored, reports/tmp/report15/):
//   cli/*.png, areas.tsv, openscad.tsv, step.tsv, timings.tsv,
//   check-fehler.txt, inspect-halter.txt                       tools/report/report15-data.sh
//   scenarios.json, img/*.jpg                                   cd web && pnpm exec playwright test e2e/skizzen-ecken.spec.ts
//   playwright.json                                             whole suite (pnpm e2e; copy of reports/tmp/report2/playwright.json)
//   vitest.txt, typecheck.txt                                   cd web && pnpm test, pnpm typecheck
//   cargo.txt, clippy.txt, fmt.txt                              cargo test --workspace --no-fail-fast, clippy -D warnings, fmt --check
//   compare.txt, step.txt, alltag.txt, node-test.txt            compare.py --eval, verify.py, tests/alltag/check.sh, test-node.mjs
//
//   node tools/report/report15.mjs
//   node tools/report/print-pdf.mjs docs/reports/15-skizzen-ecken-offset/report.html docs/reports/15-skizzen-ecken-offset/report.pdf "Testbericht 15: Skizzen – Ecken, Offset, Stroke"

import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import os from "node:os";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "../..");
const input = join(root, "reports/tmp/report15");
const out = join(root, "docs/reports/15-skizzen-ecken-offset");
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
const pct = (x, d = 3) => (x === null || x === undefined || !Number.isFinite(x) ? "–" : `${de(x * 100, d)} %`);
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
const NEW = /region2::|op2d::|biarcs|curve2::|sketch_ops|corner_(rules|errors|radius)|concave_corners|arguments_of_2d|sketch_corners_offset_stroke|spec_models|tree_bounds|openscad_models|ring_of_long|step_has_exact/;
const newTests = cargo ? [...cargo.matchAll(/^test (\S+) \.\.\. (ok|FAILED)$/gm)].map((m) => ({ name: m[1], res: m[2] })).filter((t) => NEW.test(t.name)) : [];
const failedTests = cargo ? [...cargo.matchAll(/^test (\S+) \.\.\. FAILED$/gm)].map((m) => m[1]) : [];
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
const compare = readText(join(input, "compare.txt"));
const compareSum = compare ? /(\d+)\/(\d+)/.exec(compare.split("\n").reverse().find((l) => /\d+\/\d+ files/.test(l)) ?? "")?.slice(1, 3) : null;
const compareNew = compare ? compare.split("\n").filter((l) => /^(file|3[456]_|-{10})/.test(l)) : [];
const step = readText(join(input, "step.txt"));
const stepSum = step ? (/\d+\/\d+ passed/.exec(step)?.[0] ?? null) : null;
const stepNew = step ? step.split("\n").filter((l) => /^(model|skizze_(deckel|halter|kabelkanal)|3[456]_)/.test(l)) : [];
const alltag = readText(join(input, "alltag.txt"));
const alltagLine = alltag ? alltag.trim().split("\n").at(-1) : null;
const nodeTest = readText(join(input, "node-test.txt"));
const scenarios = (readJson(join(input, "scenarios.json")) ?? []).sort((a, b) => a.id.localeCompare(b.id));
const flaky = readText(join(input, "flaky.txt"));

const tsv = (name) =>
  (readText(join(input, name)) ?? "")
    .trim()
    .split("\n")
    .filter(Boolean)
    .map((l) => l.split("\t"));
const areas = tsv("areas.tsv").map(([name, got, want, ms, code]) => {
  const g = Number(got);
  const w = Number(want);
  return { name, got: g, want: w, rel: Math.abs(g - w) / Math.abs(w), ms: Number(ms), code };
});
const pyEval = (e) => {
  if (!e) return null;
  try {
    return Number(execFileSync("python3", ["-c", `from math import *; print(repr(${e}))`], { encoding: "utf8" }).trim());
  } catch {
    return null;
  }
};
const osc = tsv("openscad.tsv").map(([name, code, vo, vu, exact, bo, bu, cls]) => {
  const o = Number(vo);
  const u = Number(vu);
  const x = pyEval(exact);
  const bO = JSON.parse(bo || "{}");
  const bU = JSON.parse(bu || "{}");
  let dbox = null;
  if (bO.min && bU.min) dbox = Math.max(...[0, 1].flatMap((i) => [Math.abs(bO.min[i] - bU.min[i]), Math.abs(bO.max[i] - bU.max[i])]));
  return { name, code, o, u, x, rel: (u - o) / o, relx: x ? (u - x) / x : null, relox: x ? (o - x) / x : null, dbox, cls };
});
const steps = tsv("step.tsv").map(([name, cnt, info, vol]) => ({ name, cnt: JSON.parse(cnt), info: JSON.parse(info), vol: Number(vol) }));
const timings = Object.fromEntries(tsv("timings.tsv").map(([n, ms]) => [n, Number(ms)]));

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

const ANAMES = {
  rechteck_fillet2d: "`fillet2d(4) square([30, 20])`: ab − (4 − π) r²",
  rechteck_corner: "dasselbe als Skizze mit vier `corner(r = 4)` (Startecke direkt nach `start`)",
  quadrat_kreis: "`fillet2d(5) square(10)`: die Geraden verschwinden, ein Kreis",
  rechteck_chamfer2d: "`chamfer2d(2) square([30, 20])`: ab − 4 · d²/2",
  l_konkav: "L-Profil, alle Ecken r 2: 5 konvexe (−), 1 konkave (+) je (1 − π/4) r²",
  l_zone: "dasselbe, nur die konkave Ecke (Zone)",
  offset_quadrat: "`offset(r = 2) square(10)`: (a + 2r)² − (4 − π) r²",
  offset_quadrat_fein: "`offset(r = 0.5) square(20)`",
  offset_ring: "`offset(r = 1)` eines Rings 10/4: π (11² − 3²)",
  offset_delta_fase: "`offset(delta = 2, chamfer = true) square(10)`: Schnitt im Abstand 2",
  stroke_bogen_flach: "`stroke(2, cap = \"flat\")` eines Viertelkreises r 10: Ringsektor",
  stroke_bogen_rund: "dasselbe mit runden Kappen: Ringsektor + π (w/2)²",
  stroke_bogen_eckig: "dasselbe mit eckigen Kappen: Ringsektor + w²",
  stroke_ecke_gehrung: "Polylinie mit rechtem Winkel, w 2, Gehrung: Bänder + 1",
  stroke_ecke_rund: "dasselbe, runder Stoß: Bänder + π/4",
  stroke_ecke_fase: "dasselbe, Fase: Bänder + 1/2",
  stroke_kreisrand: "`stroke(2) circle(10)`: Ring π (11² − 9²)",
  corner_bogen: "Viertelkreis r 10, `corner(r = 1)` zwischen Gerade und Bogen (geschlossene Form)",
};
const ONAMES = {
  quadrat_r: "offset(r = 2) square(10)",
  quadrat_r_innen: "offset(r = −2) square(10)",
  kreis_r: "offset(3) circle(r = 5) – positional = r",
  l_r: "offset(r = 1.5) L-Profil (konkave Ecke bleibt spitz)",
  l_r_innen: "offset(r = −1.5) L-Profil (konkave Ecke wird rund)",
  ohne_argument: "offset() square(10) – r = 1",
  r_vor_delta: "offset(r = 2, delta = 1) – r gewinnt",
  quadrat_delta: "offset(delta = 2) square(10)",
  quadrat_fase: "offset(delta = 2, chamfer = true) square(10)",
  quadrat_delta_innen: "offset(delta = −2) square(10)",
  stern_delta: "offset(delta = 1) Stern (10 Spitzen)",
  stern_fase: "offset(delta = 1, chamfer = true) Stern",
  spitzes_dreieck: "offset(delta = 1) spitzes Dreieck (5,7°): Gehrung ohne Grenze",
  l_delta: "offset(delta = 1.5) L-Profil",
  l_delta_innen: "offset(delta = −1.5) L-Profil",
  l_fase_innen: "offset(delta = −1.5, chamfer = true) L-Profil",
  platte: "Platte 30 × 20 mit Kreis- und Quadratloch (ohne offset)",
  platte_r: "offset(r = 1) Platte: Löcher kleiner",
  platte_r_innen: "offset(r = −1) Platte: Löcher größer",
  loch_schliesst: "offset(r = 4.5) Platte: beide Löcher geschlossen",
  platte_delta: "offset(delta = 1) Platte",
  zusammenwachsen: "offset(r = 1.5) zweier Quadrate im Abstand 2: wachsen zusammen",
  steg_verschwindet: "offset(r = −1): Steg 1,5 mm verschwindet, zwei Teile",
  oeffnen: "offset(r = 2) offset(r = −2): Öffnen – Ecken rund, schmale Teile weg",
};
const oscMax = osc.length ? Math.max(...osc.map((r) => Math.abs(r.rel))) : null;
const oscPoly = osc.filter((r) => r.cls === "polygonal");
const oscPolyMax = oscPoly.length ? Math.max(...oscPoly.map((r) => Math.abs(r.rel))) : null;
const oscBox = osc.length ? Math.max(...osc.map((r) => r.dbox ?? 0)) : null;
const areaMax = areas.length ? Math.max(...areas.map((a) => a.rel)) : null;

const html = `<!doctype html>
<html lang="de">
<head>
<meta charset="utf-8">
<title>Testbericht 15 – Skizzen: Ecken, Offset, Stroke</title>
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
<h1>Testbericht 15 – Skizzen: Ecken, Offset, Stroke</h1>
<div class="meta">Plan 17.7, Schritt 2: <code>corner</code>, <code>fillet2d</code>/<code>chamfer2d</code>, <code>offset</code>, <code>stroke</code> · cad-kernel (<code>region2.rs</code>, Biarcs in <code>curve2.rs</code>), cad-ir, cad-eval, cad-engine (<code>op2d.rs</code>), cad-session, cad-cli, web · ${esc(date)} · Branch ${esc(branch)} · Commit ${esc(commit)} · ${esc(rustc)} · ${os.cpus().length} Kerne</div>

<div class="summary">
  <div>cargo test --workspace<b class="${cargoSum && cargoSum.failed === 0 ? "ok" : "bad"}">${cargoSum ? `${cargoSum.ok} / ${cargoSum.ok + cargoSum.failed}` : "–"}</b>davon ${newTests.length} neu/erweitert</div>
  <div>clippy · fmt<b class="${clippyOk && fmtOk ? "ok" : "bad"}">${clippy == null ? "–" : clippyOk ? "sauber" : "Befund"} · ${fmt == null ? "–" : fmtOk ? "sauber" : "Abw."}</b>-D warnings</div>
  <div>Vitest · Playwright<b class="${pwStats && pwStats.failed.length === 0 ? "ok" : "bad"}">${vitestSum ?? "–"} · ${pwStats ? `${pwStats.passed} / ${pwStats.total}` : "–"}</b>neu: E01–E05</div>
  <div>OpenSCAD · STEP<b class="${compareSum && compareSum[0] === compareSum[1] ? "ok" : "bad"}">${compareSum ? `${compareSum[0]}/${compareSum[1]}` : "–"} · ${esc(stepSum?.match(/\d+\/\d+/)?.[0] ?? "–")}</b>compare.py · verify.py (OCCT)</div>
  <div>offset je Fall<b class="${oscMax !== null && oscMax < 0.01 ? "ok" : "bad"}">≤ ${pct(oscMax, 3)}</b>${osc.length} Fälle gegen OpenSCAD</div>
  <div>Alltag · Wasm (Node)<b class="${alltag && /0 mit Problemen/.test(alltag) && nodeTest && /all node tests passed/.test(nodeTest) ? "ok" : "bad"}">${alltag && /0 mit Problemen/.test(alltag) ? "unverändert" : "–"} · ${nodeTest && /all node tests passed/.test(nodeTest) ? "ok" : "–"}</b>check.sh · test-node.mjs</div>
</div>

<h2>1 Auftrag und Ergebnis</h2>
<p>Schritt 2 von Plan 17: <b>Ecken</b> in Skizzen (<code>corner(r = …)</code>, <code>corner(chamfer = …)</code>) und an beliebigen 2D-Profilen (<code>fillet2d</code>, <code>chamfer2d</code>, mit Zonen wie in 3D), <b><code>offset</code></b> OpenSCAD-kompatibel und exakt für Geraden und Bögen, <b><code>stroke</code></b> für offene Pfade; Darstellung und Griffe in UI und CLI. Grundlage ist ein neues Modul <code>cad_kernel::region2</code>: exakte Booleans von Regionen aus Geraden und Kreisbögen, aus denen Offset, Strich und die Auswertung beliebiger 2D-Teilbäume gebaut sind. Text, SVG-Import (Schritt 3) und der Zeichenmodus (Schritt 4) gehören nicht dazu.</p>
<ul>
<li>Flächen gegen Formeln (Abschnitt 7): alle ${areas.length} Fälle innerhalb der Volumenintegration von SMLib (≤ ${sci(areaMax)} relativ) – Rechteck mit verrundeten Ecken = ab − (4 − π) r², Offset eines Quadrats = (a + 2r)² − (4 − π) r², Strich eines Kreisbogens = Ringsektor + Kappen.</li>
<li>OpenSCAD 2021.01 (Abschnitt 4): ${osc.length} Einzelfälle, Abweichung der Fläche ≤ ${pct(oscMax, 3)} (polygonal, d. h. <code>delta</code> an Polygonen: ≤ ${pct(oscPolyMax, 4)}), Hülle ≤ ${de(oscBox, 4)} mm; compare.py mit den neuen Modellen 34–36: ${compareSum ? `${compareSum[0]}/${compareSum[1]}` : "–"}. Die Restabweichung bei <code>r</code> ist die Facettierung von OpenSCAD (<code>$fn = 128</code>): wir rechnen exakte Bögen.</li>
<li>STEP: Bögen als <code>CIRCLE</code>, extrudierte Bögen als Zylinder, keine B-Spline-Kurven und keine angenäherten Kanten in den neuen Beispielen; OpenCASCADE liest sie mit gleichem Volumen (verify.py).</li>
</ul>

<h2>2 corner() in Skizzen</h2>
<p><b>Regeln</b> (Plan 17.9): <code>corner()</code> zwischen zwei Segmenten rundet die Ecke zwischen ihnen – die Schlusslinie von <code>close()</code> zählt als Segment, <code>corner()</code> vor <code>close()</code> rundet also die Ecke zur Schlusslinie; <code>corner()</code> <b>direkt nach <code>start(p)</code></b> rundet die Startecke (zwischen letztem und erstem Segment); <code>corner()</code> nach dem letzten Segment einer Kontur, die am Start endet (keine Schlusslinie nötig), ebenfalls. Die Skizze wird zuerst ohne Ecken aufgebaut (tangentiale Bögen und <code>line(length)</code> beziehen sich auf die spitze Ecke), dann werden alle Ecken einer Kontur auf den ursprünglichen Segmenten berechnet; brauchen zwei Ecken überlappende Teile eines Segments, scheitern beide mit dem größten gemeinsamen Wert – unabhängig von der Reihenfolge. Die Verrundung ist ein Segment der corner-Zeile (Hervorhebung), eine Startecke steht am Ende der Kontur. Fase: Schnittpunkte im Abstand <code>d</code> vom Eckpunkt.</p>
<table>
<tr><th>Segmente</th><th>Lösung</th></tr>
<tr><td>Gerade – Gerade, Gerade – Bogen, Bogen – Bogen</td><td>exakt: Mittelpunkt = Schnitt der um r versetzten Kurven (Gerade/Gerade, Gerade/Kreis, Kreis/Kreis mit R ± r je nach Seite), Fußpunkte auf den Segmenten, kürzester Kandidat</td></tr>
<tr><td>mit Bézier oder Spline</td><td>Newton auf <code>a(u) + r·n<sub>a</sub>(u) = b(w) + r·n<sub>b</sub>(w)</code> (Startwert aus den Tangenten), Abbruch bei Rest 10<sup>−13</sup> der Größe, Warnung über 10<sup>−6</sup>; die Kurve wird am Parameter gekürzt (bleibt B-Spline). Test: Gerade–Bézier, Rest &lt; 10<sup>−9</sup>, Abstand Mittelpunkt–Kurve = r auf 10<sup>−6</sup></td></tr>
</table>
<p><b>Fehler an der corner-Zeile:</b> Radius passt nicht („höchstens ≈ r“, per Bisektion), tangentiale Ecke („keine Ecke“), Spitze (180°), nach <code>close()</code>, vor <code>start</code>, zweimal an einer Ecke, Start/Ende einer offenen Kontur, Wert ≤ 0. <b>Griff:</b> Radius auf der Mitte der Verrundung, Basis im Eckpunkt, linear in r (UI-Test E01).</p>
<div class="codefig">${pre(src("crates/cad-cli/tests/data/skizze_ecken.scad"))}<div>${fig(cli("ecken-handles"), "`cad render --line 5 --view top --show handles`: die Skizze hervorgehoben, Griffe (Radius auf den Verrundungen)")}${fig(cli("ecken-offset"), "`--line 13`: offset – Ausgangskontur, Offset-Kontur gestrichelt")}</div></div>
${pre(T("cases/fehler.scad"))}
${pre(T("check-fehler.txt"))}

<h2>3 fillet2d und chamfer2d</h2>
<p><code>fillet2d(r) { profil; zone; … }</code> / <code>chamfer2d(d)</code>: das erste Kind ist das Profil (beliebige Herkunft: <code>square</code>, <code>polygon</code>, Skizzen, 2D-Booleans), weitere Kinder sind Zonen (2D-Formen und <code>zone(linie | ebene, d)</code>). Ecken sind Stöße mit Knick, konvex und konkav; die Verrundung liegt auf der Innenseite des Knicks (konvex: Material weg, konkav: Material dazu). <b>Auswahl</b> wie in 3D: <code>select = "inside"</code> (Standard) – der Eckpunkt liegt in einer Zone; <code>"touch"</code> – auch wenn nur das ersetzte Stück des Profils eine Zone berührt. Inline-Hinweis „n Ecken“, leere Auswahl ist eine Warnung, zu große Radien sind Fehler je Ecke (die übrigen Ecken werden gerundet). Verschachtelte Blöcke wirken von innen nach außen; die Übergänge einer Verrundung sind tangential und zählen nicht. Eine einzelne geschlossene Kontur behält ihre Segmente (Splines bleiben Splines), alles andere geht durch die exakte Region.</p>
<div class="codefig">${pre(src("tests/models/spec/skizze_halter.scad"))}<div>${fig(cli("halter"), "skizze_halter.scad: Innenecke r 5 (Zone), alle übrigen Ecken r 1, Langlöcher aus `stroke`")}${fig(cli("halter-fillet"), "`--line 18` von oben: die fünf neuen Eckbögen des äußeren Blocks")}</div></div>
${pre(T("inspect-halter.txt").split("\n").slice(0, 14).join("\n"))}

<h2>4 offset: Semantik und OpenSCAD-Vergleich</h2>
<p>Wie OpenSCAD 2021.01: erstes Positionsargument <code>r</code>, ohne Argumente <code>r = 1</code>, <code>r</code> gewinnt vor <code>delta</code>, <code>chamfer</code> nur mit <code>delta</code>. <code>r &gt; 0</code>: konvexe Ecken werden <b>exakte Bögen</b>, konkave bleiben spitz; <code>r &lt; 0</code>: konvexe bleiben spitz, konkave werden rund, schmale Teile verschwinden, Löcher wachsen. <code>delta</code>: Ecken bis zum Schnitt der versetzten Kanten (ohne Gehrungsgrenze – OpenSCAD setzt <code>miter_limit</code> 10<sup>6</sup>, das spitze Dreieck unten belegt es), <code>chamfer = true</code> schneidet wie Clippers <code>jtSquare</code> senkrecht zur Winkelhalbierenden im Abstand <code>delta</code> vom Eckpunkt (an OpenSCAD gemessen: 196 − 4·(2√2 − 2)² für das Quadrat). Exakt: Region ∪ (Bänder ± r längs der Segmente ∪ Kreise in den Ecken) bzw. Region ∖ (…) für r &lt; 0 – die Minkowski-Summe mit dem Kreis aus Geraden und Bögen; <code>delta</code>: einseitige Bänder und Eckstücke (Gehrung/Fase) an auseinanderlaufenden Ecken.</p>
<p>Je Fall eine Datei <code>linear_extrude(1) …</code> (Volumen = Fläche, <code>reports/tmp/report15/openscad/</code>); OpenSCAD exportiert STL (<code>$fn = 128</code> bei Rundungen wie <code>make-reference.py</code>), wir messen exakt (<code>cad measure</code>). „exakt“: geschlossene Formel, wo es eine gibt.</p>
<table>
<tr><th>Fall</th><th>OpenSCAD mm²</th><th>wir mm²</th><th>Abw.</th><th>exakt</th><th>OpenSCAD − exakt</th><th>Hülle Δ mm</th></tr>
${osc.map((r) => `<tr><td>${esc(ONAMES[r.name] ?? r.name)}</td><td class="n">${de(r.o, 4)}</td><td class="n">${de(r.u, 4)}</td><td class="n">${pct(r.rel, 4)}</td><td class="n">${r.x ? de(r.x, 4) : "–"}</td><td class="n">${r.relox !== null ? pct(r.relox, 4) : "–"}</td><td class="n">${de(r.dbox, 4)}</td></tr>`).join("\n")}
</table>
<p class="small">Die Abweichung unserer Werte von der Formel ist dort, wo es eine gibt, die Integrationsgenauigkeit von SMLib (Abschnitt 7); die von OpenSCAD die Facettierung (Kreisbögen als 128-Ecke: Sehnen innen, etwas weniger Fläche) bzw. Clippers ganzzahlige Koordinaten (Fase des Quadrats: 3·10<sup>−7</sup>).</p>
<p><b>compare.py</b> mit den neuen Modellen (<code>tests/models/openscad-compat/34_offset_round.scad</code>, <code>35_offset_delta.scad</code>, <code>36_offset_holes.scad</code>; Referenz mit <code>make-reference.py</code> nur für diese Dateien, <code>stl_stats.classify</code> zählt <code>offset(r)</code> zu den Rundungen):</p>
${pre(compareNew.join("\n"))}
<div class="codefig">${pre(src("tests/models/spec/skizze_deckel.scad"))}<div>${fig(cli("deckel"), "skizze_deckel.scad: Umriss mit vier `corner(r = 6)`, Steckrand als Ring aus zwei Offsets nach innen")}${fig(cli("deckel-offset"), "`--line 30`: Ausgangskontur und Offset-Kontur (gestrichelt), die Eckradien 6 → 3,8 bleiben Bögen")}</div></div>

<h2>5 stroke</h2>
<p><code>stroke(w, cap = "round" | "flat" | "square", join = "round" | "miter" | "bevel")</code> (Standard rund/rund): Kinder sind offene Skizzen (Pfade; eine am Start endende Kontur ist geschlossen) und beliebige 2D-Profile (ihr Rand als geschlossene Pfade: <code>stroke(1) circle(5)</code> ist ein Ring). Bänder der halben Breite längs der Segmente (Gerade: Rechteck, Bogen: Ringsektor), Stöße rund, mit Gehrung (bis 4 Halbbreiten wie SVGs <code>stroke-miterlimit</code>, sonst Fase) oder Fase, Enden rund, flach oder um w/2 verlängert; sich kreuzende Pfade werden vereinigt.</p>
<div class="codefig">${pre(src("tests/models/spec/skizze_kabelkanal.scad"))}<div>${fig(cli("kabelkanal"), "skizze_kabelkanal.scad: Boden und Wände aus `stroke` eines Pfads mit tangentialem Bogen und `corner(r = 12)`, Rippen als Zickzack mit Gehrung")}${fig(cli("kabelkanal-stroke"), "`--line 22`: Mittellinie des Bodens")}</div></div>

<h2>6 Kernel: exakte 2D-Regionen</h2>
<ul>
<li><b><code>region2::combine(operanden, prädikat)</code></b>: alle Segmente werden an ihren exakten Schnittpunkten geteilt (Gerade–Gerade, Gerade–Kreis, Kreis–Kreis einschließlich Berührung, Überlappung kollinearer Geraden und gleicher Kreise; Punkte im Abstand 10<sup>−9</sup> der Größe verschmelzen), doppelte Stücke entfallen. Jedes Stück bleibt, wo das Prädikat links und rechts verschieden ist (Testpunkte neben der Stückmitte; Windungszahl je Füllregel über den Strahl nach +x, Bögen in y-monotone Stücke geteilt), mit der Region links. Verbinden zu Schleifen mit der engsten Drehung an gemeinsamen Ecken, Stücke derselben Geraden bzw. desselben Kreises verschmelzen. Ergebnis: äußere Schleifen gegen, Löcher im Uhrzeigersinn, nur Geraden und Bögen.</li>
<li>2D-Teilbäume: das Prädikat ist der boolesche Ausdruck der Blätter (Füllregel je Blatt: Polygone even-odd, sonst nonzero); ein Zufallstest vergleicht 60 Booleans aus Kreisen, Rechtecken, Polygonen und Skizzen mit dem Polygon-Clipping der Anzeige (≤ 10<sup>−3</sup>).</li>
<li><b>B-Splines</b> gehen als G1-Biarcs ein (zwei tangentiale Bögen je Stück, Stücke halbiert bis zur Abweichung ≤ 1 µm, höchstens 10<sup>−5</sup> der Modellgröße). Begründung: der Offset einer B-Spline ist keine B-Spline, der eines Bogens ein Bogen – der Offset ist exakt zur Näherung und tangentenstetig, Schnitte bleiben geschlossen lösbar, STEP enthält Linien und Kreise; 1 µm liegt weit unter Druck- und Fertigungstoleranzen und über der Rechengenauigkeit.</li>
<li>Engine: <code>Op::Offset</code>, <code>Op::Blend2</code>, <code>Op::Stroke</code> sind 2D-Knoten; <code>profile::lift</code> berechnet die Region (zwischengespeichert nach Teilbaum-Inhalt) und gibt sie als Skizzen-Blätter weiter – Extrude, Revolve, Loft, Sweep und STEP sehen exakte Geraden und Bögen. <code>cad_engine::region_2d</code> liefert die Region eines 2D-Knotens (Hüllen der Kompatibilitätstests).</li>
</ul>
<p class="note"><b>Korrektur aus Schritt 1 (Bögen über 90°).</b> Die Fassade baut einen Bogen als rationale quadratische B-Spline mit Spannen ≤ 90°; ein längerer Bogen ist eine Kurve aus mehreren Spannen. Booleans mit Extrusionen solcher Kurven scheiterten in SMLib je nach Lage (Ring aus zwei 180°-Bögen ∪ Quader, um 157,5° gedreht: „volume 924.12 impossible for operands 976.79, 43.2“; mit 4 × 90° richtig). Die Engine teilt Bögen über 90° jetzt vor dem Kernel in gleiche Stücke ≤ 90° (<code>profile::split_arcs</code>; Extrude, Revolve, Sweep – nicht Loft); ein Halbkreis wird zwei Zylinderflächen. Regressionstest <code>ring_of_long_arcs_unites_with_a_box</code>: Bögen von 180°, 120°, 90° und 360° je mit acht Lagen des Quaders, Volumen = 3 × exakte 2D-Fläche der Vereinigung (≤ 10<sup>−5</sup>). Dabei gefunden: die Windungszahl der Region nahm Bogenendpunkte aus den Winkeln (Rundung ~10<sup>−15</sup>), ein Strahl genau durch einen Eckpunkt zählte dann falsch (Ring aus drei 120°-Bögen ohne Loch) – jetzt gelten die gemeinsamen Eckpunkte der Schleife (Test <code>rings_of_arcs_of_any_sweep</code>).</p>
<table>
<tr><th>Modell</th><th>Flächen</th><th>CIRCLE</th><th>LINE</th><th>B-Spline-Kurven</th><th>Flächentypen</th><th>angenähert</th></tr>
${steps.map((s) => `<tr><td>${esc(s.name)}</td><td class="n">${esc(s.info.faces)}</td><td class="n">${s.cnt.CIRCLE}</td><td class="n">${s.cnt.LINE}</td><td class="n">${s.cnt.B_SPLINE_CURVE_WITH_KNOTS}</td><td>${esc(Object.entries(s.info.surfaces ?? {}).map(([k, v]) => `${k} ${v}`).join(", "))}</td><td class="n">${esc(s.info.approximated_edges)} / ${esc(s.info.approximated_faces)}</td></tr>`).join("\n")}
</table>
<p>OpenCASCADE (<code>tests/step/verify.py</code>) für die neuen Modelle (Spalten: Körper, Flächen, V Kernel, V OCCT, dV, dA, dBox, Flächentypen):</p>
${pre(stepNew.join("\n"))}

<h2>7 Messwerte: Flächen gegen Formeln, Laufzeit</h2>
<p>Je Fall <code>linear_extrude(1) …</code> (<code>reports/tmp/report15/cases/</code>), <code>cad measure --volume</code> mit dem Release-Build (Zeit: ganzer CLI-Aufruf ohne Daemon). Die Geometrie ist exakt; die Abweichung ist die der Volumenintegration in SMLib.</p>
<table>
<tr><th>Fall</th><th>Kernel mm²</th><th>Formel mm²</th><th>rel. Abw.</th><th>ms</th></tr>
${areas.map((v) => `<tr><td>${md(ANAMES[v.name] ?? v.name)}</td><td class="n">${de(v.got, 4)}</td><td class="n">${de(v.want, 4)}</td><td class="n">${sci(v.rel)}</td><td class="n">${de(v.ms)}</td></tr>`).join("\n")}
</table>
<p><b>Laufzeit</b> (Release, <code>cad check</code>, bester von drei Läufen, ganzer Aufruf): Zahnrad mit 160 Ecken und Bohrung – <code>offset(r = 0.5)</code>, <code>fillet2d(0.4)</code> aller 160 Ecken – und <code>stroke</code> einer Spline, nur 2D: <b>${de(timings.zeit2d)} ms</b>; dasselbe extrudiert (Kernel): ${de(timings.zeit3d)} ms. Beispiele: Deckel ${de(timings.skizze_deckel)} ms, Halter ${de(timings.skizze_halter)} ms, Kabelkanal ${de(timings.skizze_kabelkanal)} ms (überwiegend Kernel-Booleans).</p>

<h2>8 UI: Darstellung und Griffe</h2>
<p>Cursor auf <code>offset</code>: die Kinder als Umriss (Ausgangskontur), die Offset-Kontur gestrichelt in der Akzentfarbe; auf <code>stroke</code>: der Pfad als Umriss, die Mittellinie gestrichelt; auf <code>fillet2d</code>/<code>chamfer2d</code>: Zonen blau, das Profil vor dem Verrunden als Umriss, die neuen Eckstücke wie ausgewählte Kanten, „n Ecken“ an der Zeile; auf <code>corner</code>: die Verrundung als aktives Segment der Skizze mit Radius-Griff. Die Hilfslinien kommen von der Engine (<code>SceneNode.guides</code>: <code>source</code>, <code>result</code>, <code>centerline</code>). Prüfungen über den Zustand der UI (<code>window.__cad</code>) und Pixel der Ebenen (<code>guide:offset</code>, <code>guide:centerline</code>, <code>sketch:active</code>).</p>
${scenarios.map((s) => `<h3>${esc(s.id)} ${esc(s.title)}</h3><p>${md(s.caption)}</p>${(s.images ?? []).length === 2 ? pair(pw(s.images[0].name), s.images[0].label, pw(s.images[1].name), s.images[1].label) : (s.images ?? []).map((i) => fig(pw(i.name), i.label)).join("")}${s.code && s.code.split("\n").length <= 16 ? pre(s.code) : ""}<ul>${(s.checks ?? []).map((c) => `<li>${md(c)}</li>`).join("")}</ul>`).join("\n")}

<h2>9 Testläufe</h2>
<table>
<tr><th>Lauf</th><th>Ergebnis</th></tr>
<tr><td><code>cargo test --workspace --no-fail-fast</code></td><td>${cargoSum ? `${cargoSum.ok} bestanden, ${cargoSum.failed} fehlgeschlagen${failedTests.length ? ` (${failedTests.map(esc).join(", ")})` : ""}` : "–"}</td></tr>
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
<p class="small">Vitest: <code>web/src/logic/sketch2d.test.ts</code> (Offset-Kontur, Mittellinie, Zonen und Eckbögen von fillet2d, Radius-Griff von corner an Szenen der Beispiele). CLI: <code>sketch_corners_offset_stroke</code> (Volumen aller vier Operationen, Hinweis „4 Ecken“, Radius-Griff, Hilfslinien in <code>render</code>, Fehler an der corner-Zeile). Kompatibilität: <code>openscad_compat</code>, <code>tree_bounds</code> (Hüllen der neuen Modelle exakt über <code>region_2d</code>), <code>spec_models</code> (drei neue Beispiele).</p>

<h2>10 Grenzen</h2>
<ul>
<li><code>offset(delta)</code> an Bögen verlängert die Tangenten (Gehrung der Tangentenlinien) – OpenSCAD kennt dort nur Polygone. B-Splines gehen in <code>offset</code>, <code>stroke</code> und 2D-Booleans als Biarcs (1 µm) ein, im Ergebnis stehen Bögen statt der Spline; <code>corner</code> und <code>fillet2d</code> einer einzelnen Kontur behalten die Spline.</li>
<li><code>fillet2d</code>/<code>chamfer2d</code> haben keinen Radius-Griff (nur <code>corner</code>); Zonen sind 2D-Formen und <code>zone(linie | ebene)</code>, 3D-Zonen werden mit Warnung ignoriert.</li>
<li>Die Region klassifiziert Teilstücke über Testpunkte im Abstand ~10<sup>−6</sup> der Größe: Ränder, die sich näher kommen, ohne sich zu schneiden oder zu überlappen, können falsch eingeordnet werden. Kandidatenpaare über eine x-Sortierung (im schlechtesten Fall quadratisch).</li>
<li>2D-Booleans <i>zwischen</i> Ergebnissen (<code>difference() { stroke(…); stroke(…); }</code>) bleiben 3D-Booleans der extrudierten Teile wie bisher; exakt in 2D rechnen <code>offset</code>, <code>fillet2d</code>, <code>stroke</code> (über ihren ganzen Teilbaum).</li>
<li><code>stroke</code>: Gehrungsgrenze fest 4 Halbbreiten; ein Bogen mit Radius unter der halben Breite wird als Sektor plus gespiegeltem Sektor überstrichen. Fehler einer 2D-Operation unter einer Extrusion erscheinen an beiden Zeilen (wie bisher bei 2D-Fehlern).</li>
<li>Lofts verbinden Skizzen Segment für Segment und bekommen Bögen über 90° weiter als Kurven aus mehreren Spannen; ob SMLib-Booleans mit solchen Loft-Flächen dieselbe Schwäche haben, ist nicht geprüft.</li>
<li><code>cad render --line N</code> einer corner-Zeile hebt die ganze Skizze hervor (Hervorhebung des einzelnen Segments nur in der UI, wie in Bericht 14).</li>
</ul>
</main>
</body>
</html>
`;
writeFileSync(join(out, "report.html"), html);
console.log(`${join(out, "report.html")}: ${html.length} Zeichen, ${scenarios.length} Szenarien, ${areas.length} Flächenfälle, ${osc.length} OpenSCAD-Fälle`);
