#!/usr/bin/env node
// Test report 4 ("Verrundung, Fase, Shell und Profiloperationen"): collects the results of the
// Phase C tests and writes
//
//   docs/reports/04-fillet-profile/report.html, report.pdf, img/*.jpg
//
// Inputs (written by the test runs, gitignored under reports/tmp/):
//   report2/playwright.json   Playwright JSON reporter (web/playwright.config.ts); web/e2e/ui-c.spec.ts
//                             is reported here, ui.spec.ts / ui-b.spec.ts as regression
//   report4/vitest.json       Vitest JSON reporter
//   report4/cargo.txt         output of `cargo test --workspace`
//   report4/scenarios.json    title/caption/screenshots/code/checks per scenario (web/e2e/helpers.ts)
//   report4/metrics.json      evaluation times (web/e2e/ui-c.spec.ts, C18)
//   report4/img/*.jpg         screenshots
//
//   cd web && pnpm test:report4     (Rust tests, Vitest, Playwright, then this script)
//   node tools/report/report4.mjs [--run]
//
// Missing inputs are produced by the script (Vitest; the Phase C scenarios when playwright.json
// has none of them). `--run` reruns Vitest and all Playwright scenarios. The PDF is printed with
// Chromium (page.pdf, A4, backgrounds on).

import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import os from "node:os";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "../..");
const web = join(root, "web");
const input = join(root, "reports/tmp/report4");
const pwFile = join(root, "reports/tmp/report2/playwright.json");
const out = join(root, "docs/reports/04-fillet-profile");
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

const rerun = process.argv.includes("--run");
const sh = (args) => {
  try {
    execFileSync("pnpm", ["exec", ...args], { cwd: web, stdio: "inherit" });
  } catch {
    /* failed tests still write their results */
  }
};
const hasPhaseC = (j) => JSON.stringify(j?.suites ?? []).includes('"ui-c.spec.ts"');
mkdirSync(input, { recursive: true });
if (rerun || !existsSync(join(input, "vitest.json"))) {
  console.log("report4: Vitest …");
  sh(["vitest", "run", "--reporter=default", "--reporter=json", `--outputFile.json=${join(input, "vitest.json")}`]);
}
if (rerun) {
  console.log("report4: Playwright (alle Szenarien) …");
  sh(["playwright", "test"]);
} else if (!hasPhaseC(readJson(pwFile))) {
  console.log("report4: Playwright (web/e2e/ui-c.spec.ts) …");
  sh(["playwright", "test", "e2e/ui-c.spec.ts"]);
}

const pw = readJson(pwFile);
const vt = readJson(join(input, "vitest.json"));
const scenarios = readJson(join(input, "scenarios.json"), []);
const metrics = readJson(join(input, "metrics.json"), {});
const cargo = existsSync(join(input, "cargo.txt")) ? readFileSync(join(input, "cargo.txt"), "utf8") : null;
if (!pw || !hasPhaseC(pw)) {
  console.error(`fehlt: Ergebnisse von web/e2e/ui-c.spec.ts in ${pwFile} (cd web && pnpm e2e)`);
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
const e2e = flatten(pw, "ui-c.spec.ts");
// "ui-b.spec.ts" / "ui-c.spec.ts" do not end with "ui.spec.ts".
const phaseAUnique = flatten(pw, "ui.spec.ts");
const phaseB = flatten(pw, "ui-b.spec.ts");
const passed = (l) => l.filter((t) => t.status === "passed").length;

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
const NEW_UNIT = new Set(["web/src/logic/curves.test.ts", "web/src/logic/zones.test.ts", "web/src/engine/transport.test.ts"]);
const CHANGED_UNIT = new Set(["web/src/logic/display.test.ts", "web/src/logic/drag.test.ts"]);

/** Rust tests: `test name ... ok` lines grouped by test binary. */
const rust = [];
if (cargo) {
  let cur = null;
  for (const l of cargo.split("\n")) {
    const run = l.match(/Running (?:unittests )?(\S+)/) ?? l.match(/Doc-tests (\S+)/);
    if (run) {
      cur = { bin: run[1].replace(/\s*\(.*$/, ""), tests: [] };
      rust.push(cur);
      continue;
    }
    const t = l.match(/^test (.+?) \.\.\. (ok|FAILED|ignored)/);
    if (t && cur) cur.tests.push({ name: t[1], status: t[2] });
  }
}
const rustBins = rust.filter((b) => b.tests.length);
const rustTotal = rustBins.reduce((s, b) => s + b.tests.length, 0);
const rustOk = rustBins.reduce((s, b) => s + b.tests.filter((t) => t.status === "ok").length, 0);
const rustBad = rustBins.flatMap((b) => b.tests.filter((t) => t.status === "FAILED").map((t) => `${b.bin}: ${t.name}`));
const NEW_RUST = new Set(["unchanged_literals_keep_their_text", "profile_operation_handles", "spec_models_stage1"]);

// ---------------------------------------------------------------- images

rmSync(join(out, "img"), { recursive: true, force: true });
mkdirSync(join(out, "img"), { recursive: true });
const used = new Set();
for (const s of scenarios) for (const i of s.images ?? []) used.add(i.name);
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
  ["Engine", `cad_wasm_bg.wasm ${mib(asset(/^cad_wasm_bg-.*\.wasm$/))}, smlib.wasm ${mib(asset(/^smlib-.*\.wasm$/))} (SMLib-Kernel, Emscripten)`],
  ["Quelltext", `Branch ${git("rev-parse", "--abbrev-ref", "HEAD")}, Commit ${git("rev-parse", "--short", "HEAD")}`],
];

// ---------------------------------------------------------------- html

const date = new Date().toLocaleDateString("de-DE", { year: "numeric", month: "long", day: "numeric" });
const scen = (id) => scenarios.find((s) => s.id === id);
const statusLabel = (s) => (s === "passed" ? "bestanden" : s === "skipped" ? "übersprungen" : "fehlgeschlagen");

const scenarioRows = e2e
  .map((t) => {
    const c = scen(t.id);
    const checks = c?.checks?.length ? `<ul class="checks">${c.checks.map((x) => `<li>${esc(x)}</li>`).join("")}</ul>` : "";
    return `<tr class="${t.status === "passed" ? "ok" : "bad"}"><td>${esc(t.id)}</td><td>${esc(t.title)}</td><td class="res">${statusLabel(t.status)}</td><td class="num">${(t.ms / 1000).toFixed(1)} s</td><td>${checks || esc(c?.caption ?? "")}${t.error ? `<div class="err">${esc(t.error.split("\n")[0])}</div>` : ""}</td></tr>`;
  })
  .join("\n");

const unitRows = unit
  .map((f) => `<tr class="${f.failed.length ? "bad" : "ok"}"><td>${esc(f.file)}${NEW_UNIT.has(f.file) ? ' <span class="tag">neu</span>' : CHANGED_UNIT.has(f.file) ? ' <span class="tag alt">erweitert</span>' : ""}</td><td class="num">${f.passed} / ${f.total}</td><td>${f.failed.map(esc).join("<br>") || "–"}</td></tr>`)
  .join("\n");

const rustRows = rustBins
  .map((b) => {
    const fresh = b.tests.filter((t) => NEW_RUST.has(t.name)).map((t) => t.name);
    const bad = b.tests.filter((t) => t.status === "FAILED").map((t) => t.name);
    const name = b.bin.replace(/^.*\/deps\//, "").replace(/-[0-9a-f]{16}$/, "");
    return `<tr class="${bad.length ? "bad" : "ok"}"><td>${esc(name)}</td><td class="num">${b.tests.filter((t) => t.status === "ok").length} / ${b.tests.length}</td><td>${fresh.map((n) => `<code>${esc(n)}</code>`).join(", ") || "–"}${bad.length ? `<div class="err">${bad.map(esc).join(", ")}</div>` : ""}</td></tr>`;
  })
  .join("\n");

const ev = metrics.evalTimes ?? {};
const metricRows = Object.entries(ev)
  .map(([, o]) => [o.label, `${fmtMs(o.median)} / ${fmtMs(o.medianTotal)}`, `Kernel-Zeit der vollständigen Szene / SetSource → Szene in der App, Median aus 5 Varianten (Einzelwerte Kernel ${o.kernel.map((v) => Math.round(v)).join(", ")} ms)${o.failed ? `; ${o.failed} von 5 Varianten im Kernel gescheitert (siehe Abschnitt 7)` : ""}`])
  .concat([["Größte Bilddatei im Bericht", kib(maxImg), "JPEG, Qualität 82"]])
  .map(([a, b, c]) => `<tr><td>${esc(a)}</td><td class="num">${esc(b)}</td><td>${esc(c)}</td></tr>`)
  .join("\n");

const code = (s) => (s ? `<pre>${esc(s)}</pre>` : "");
const blocks = scenarios
  .filter((s) => (s.images ?? []).length)
  .sort((a, b) => a.id.localeCompare(b.id, "de", { numeric: true }))
  .map((s) => {
    const img = (i) => `<div class="lbl">${esc(i.label)}</div><img src="img/${esc(i.name)}.jpg" alt="${esc(s.title)}: ${esc(i.label)}">`;
    const text = `${s.code ? `<div class="lbl">Code</div>${code(s.code)}` : ""}<p class="cap">${esc(s.caption)}</p>`;
    if (s.images.length === 1) {
      return `<section class="sc">
<h3>${esc(s.id)} ${esc(s.title)}</h3>
<div class="side"><div class="img">${img(s.images[0])}</div><div class="txt">${text}</div></div>
</section>`;
    }
    return `<section class="sc">
<h3>${esc(s.id)} ${esc(s.title)}</h3>
<div class="pair">${s.images.map((i) => `<div>${img(i)}</div>`).join("")}</div>
${text}
</section>`;
  })
  .join("\n");

const BUGS = [
  {
    crate: "kernel-smlib / smlib (sc_shell)",
    title: "shell eines verrundeten Körpers scheitert; Meldung hängt von der Plattform ab",
    code: `grid g { z: o = 18; }
shell(1.6) { box([0, 0, 0], [90, 34, 18], $fillet = 3); zone(g.o, d = 1); }
shell(1.6) { box([0, 0, 0], [90, 34, 18], $fillet = 5); zone(g.o, d = 1); }
shell(1.6) { box([0, 0, 0], [90, 34, 18], $fillet = 6); zone(g.o, d = 1); }
// Maße von tests/models/spec/dose_shell.scad (dort mit grid g { z: o = 30; }):
shell(1.5) { box([0, 0, 0], [50, 35, 30], $fillet = 4); zone(g.o, d = 1); }`,
    text: "Mit dem SMLib-Fork (tangency-labs/smlib, capi-tess @ 7eba8aa) scheitert jede dieser shells, im Browser (Wasm) wie nativ (cad check): $fillet = 3 mit „SmApiShellBrepFull failed: SM_ERR_NULL_POINTER (1006)“, $fillet = 5 und 6 mit „wall thickness 1.6 leaves no cavity (SMLib returned no closed solid)“ bzw. „self-overlapping“, die Maße von dose_shell.scad mit „self-overlapping solid at 10 face(s)“ – dose_shell.scad ist deshalb rot. Für dieselbe Eingabe unterscheiden sich die Meldungen zwischen Wasm und nativ ($fillet = 8: Wasm „self-overlapping“, nativ SM_ERR (1001)). Mit dem vorigen Kernel-Stand meldete $fillet = 5 „ok“ bei leerem Ergebnis und $fillet = 6 „ok“ ohne Außenwand; diese stillen Fehler gibt es nicht mehr. Scharfkantige und gefaste Körper ($chamfer) ergeben korrekte Hohlkörper (griffschale.scad, Messung C18).",
  },
  {
    crate: "kernel-smlib (nurbs_surface)",
    title: "surface(points) mit 2 × 2 oder 3 × 3 Kontrollpunkten scheitert",
    code: `surface(points = [[[0, 0, 0], [20, 0, 6]], [[0, 20, 6], [20, 20, 0]]]);`,
    text: "„surface gescheitert: SmApiCreateSurfaceFromOrderedPoints failed: SM_ERR (1001)“, ebenso für 3 × 3. cad-eval nimmt ab 2 × 2 Punkte an, die Spec nennt kein Minimum; 4 × 4 funktioniert. Vorschlag: Grad an die Punktzahl anpassen (wie interp_curve bei drei Pfadpunkten).",
  },
  {
    crate: "cad-engine ($fillet / $chamfer)",
    title: "Nahtkanten zählen als eigene Kanten eines Primitivs",
    code: `cylinder(h = 10, r = 5, $chamfer = 1);   // „3 Kanten“
sphere(5, $fillet = 1);                    // Fehler`,
    text: "Beim Zylinder werden Deckel-, Boden- und die Nahtkante ausgewählt („3 Kanten“, gezeichnet 3); bei der Kugel nur die Naht („1 Kante“), und die Verrundung scheitert mit „fillet: none of the edges can be blended (smooth, seam or open edges)“ – Zeile rot statt „0 Kanten“ bzw. ohne Wirkung. Glatte Naht- und Polkanten sollten nicht ausgewählt werden.",
  },
];
const bugBlocks = BUGS.map((b, i) => `<section class="bug"><h3>F${i + 1} ${esc(b.title)}</h3><div class="small"><b>Crate:</b> ${esc(b.crate)}</div>${code(b.code)}<p>${esc(b.text)}</p></section>`).join("\n");

const html = `<!doctype html>
<html lang="de">
<head>
<meta charset="utf-8">
<title>Testbericht 4 – Verrundung, Fase, Shell und Profiloperationen</title>
<style>
  @page { size: A4; margin: 16mm 14mm 16mm 14mm; }
  :root { --text: #1f2328; --muted: #5d646e; --border: #d5d9df; --ok: #1a7f37; --bad: #c9302c; --head: #f2f4f7; }
  * { box-sizing: border-box; }
  body { font: 10pt/1.45 system-ui, -apple-system, "Segoe UI", Roboto, sans-serif; color: var(--text); margin: 0; }
  main { max-width: 182mm; margin: 0 auto; }
  h1 { font-size: 19pt; margin: 0 0 2mm; }
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
  .tag.alt { background: #6741d9; }
  ul { margin: 1mm 0 2mm 5mm; padding: 0; }
  li { margin: 0.6mm 0; }
  ul.checks { margin: 0 0 0 3.5mm; }
  ul.checks li { margin: 0; }
  code, pre { font-family: ui-monospace, "SFMono-Regular", Menlo, Consolas, monospace; font-size: 8pt; }
  pre { margin: 0; padding: 1.5mm 2mm; background: #f6f8fa; border: 1px solid var(--border); border-radius: 1mm; white-space: pre-wrap; word-break: break-word; }
  .sc, .bug { break-inside: avoid; page-break-inside: avoid; margin: 0 0 7mm; }
  .pair { display: flex; gap: 2mm; }
  .pair > div { flex: 1; min-width: 0; }
  .pair img { max-width: 100%; max-height: 74mm; width: auto; height: auto; border: 1px solid var(--border); display: block; }
  .side { display: flex; gap: 3mm; align-items: flex-start; }
  .side > .img { flex: 0 0 auto; max-width: 58%; }
  .side > .img img { max-width: 100%; max-height: 88mm; width: auto; height: auto; border: 1px solid var(--border); display: block; }
  .side > .txt { flex: 1; min-width: 0; }
  .lbl { font-size: 7.5pt; color: var(--muted); text-transform: uppercase; letter-spacing: 0.04em; margin: 1mm 0 0.5mm; }
  .cap { font-size: 8.8pt; color: var(--muted); margin: 1.5mm 0 0; }
  .small { font-size: 8.8pt; color: var(--muted); }
  .bug p { font-size: 9pt; margin: 1.5mm 0 0; }
  tr, td { break-inside: avoid; page-break-inside: avoid; }
</style>
</head>
<body>
<main>
<h1>Testbericht 4 – Verrundung, Fase, Shell und Profiloperationen</h1>
<div class="meta">Darstellung in der Browser-UI · ${esc(date)} · Commit ${esc(git("rev-parse", "--short", "HEAD"))}</div>

<div class="summary">
  <div>Phase C (Playwright)<b>${passed(e2e)} / ${e2e.length} bestanden</b></div>
  <div>Phase A/B (Regression)<b>${phaseAUnique.length + phaseB.length ? `${passed(phaseAUnique) + passed(phaseB)} / ${phaseAUnique.length + phaseB.length}` : "–"}</b></div>
  <div>Vitest<b>${vt ? `${unitPassed} / ${unitTotal}` : "–"}</b></div>
  <div>cargo test --workspace<b>${cargo ? `${rustOk} / ${rustTotal}` : "–"}</b></div>
  <div>Fehler in fremden Crates<b>${BUGS.length}</b></div>
</div>

<h2>1 Umfang</h2>
<p>Geprüft und vervollständigt wurde die Darstellung der Spec-Abschnitte „Verrundung und Fase“, „Boolesche Operationen“ (Parameter <code>fillet =</code>/<code>chamfer =</code>), „Profiloperationen und Flächen“, „Primitive/2D“ und „Fehler und Warnungen“ sowie zwei offene Punkte aus Phase A/B.</p>
<ul>
  <li><b>fillet/chamfer-Zeile:</b> Zonen blau halbtransparent, ausgewählte Kanten orange, „n Zonen · m Kanten“ am Zeilenende (vorhanden, jetzt mit Pixelprüfung). <b>Neu: Cursor auf einer Zone</b> zeigt nur diese Zone und nur die Kanten des Blocks, die in ihr liegen (in der Ansicht an den Zonen-Meshes geprüft, <code>inside</code>: alle Abtastpunkte, <code>touch</code>: einer); eine Zone über <code>translate</code> ohne Umriss/Koordinatenkreuze der Transformation. Leere Auswahl gelb mit „0 Kanten“, Fehlschlag rot mit roter Kante und Körper ohne Verrundung (Engine), <code>difference(fillet = r)</code>, <code>$fillet</code>/<code>$chamfer</code>, <code>select = "touch"</code> und verschachtelte Blöcke geprüft.</li>
  <li><b>shell:</b> wie der Zonenblock; <b>neu</b> sind die geöffneten Flächen des Körpers orange gefüllt (Flächen des Körper-Meshes, die vollständig in einer Zone liegen; Anzahl gegen <code>opened_faces</code> der Engine geprüft), auch beim Cursor auf einer Zone.</li>
  <li><b>Profiloperationen (neu):</b> <code>linear_extrude</code> – Profil als Umriss unten (bei <code>center</code> auf −h/2), Höhengriff oben; <code>rotate_extrude</code> – Profil in der xz-Ebene, Drehachse als gestrichelte Linie, Winkelgriff auf dem Ring; <code>loft</code> – alle Profile als Umrisse an ihrer Lage, Ergebnis halbtransparent; <code>sweep</code> – Pfad als Kurve (kubische Interpolation wie im Kernel), Griffe an Literal-Punkten, Profil am Startpunkt senkrecht zur Starttangente; <code>surface</code> – Kontrollnetz als Linien mit Griffen, Fläche beidseitig; <code>thicken</code> – Fläche als Umriss, Ergebnis voll. Die Griffe (Höhe, Winkel, Pfadpunkte, Kontrollnetz) lieferte cad-eval schon; neu sind ein Test dafür und ihre Darstellung/Prüfung im Browser.</li>
  <li><b>2D-Formen allein:</b> Umriss mit leichter Füllung in der xy-Ebene; die Warnung bei z-Abweichung von Rasterpunkten erscheint gelb am Zeilenende.</li>
  <li><b>Neu einpassen:</b> nach einer deutlichen Größenänderung (Diagonale × 1,6 oder Mitte um mehr als die halbe Diagonale verschoben) passt die Ansicht selbst neu ein, solange der Nutzer die Kamera nicht bewegt (und keinen Griff gezogen) hat; sonst erscheint der Knopf „Neu einpassen“.</li>
  <li><b>Achsgebundenes Ziehen</b> von Punktgriffen: Umschalttaste – Achse des lokalen Systems aus der Mausbewegung; Tasten X/Y/Z – Achse direkt (erneut: frei), auch mitten im Ziehen. Führungslinie in Achsfarbe, Snap nur auf Ziele auf der Achse, die übrigen Komponenten behalten ihren Text.</li>
  <li><b>Beispiele (neu, im Beispielmenü):</b> griffschale.scad (shell), flasche.scad (rotate_extrude + fillet), lueftungsgitter.scad (linear_extrude + Raster, chamfer =), kabelfuehrung.scad (sweep), freiformdeckel.scad (surface + thicken).</li>
  <li><b>Entwurfsmodus beim Ziehen (Engine-Stand 58735e2):</b> Verrundungen, die länger als 25 ms brauchen, setzt die Engine beim Ziehen aus (<code>NodeReport.blend_suspended</code>). Die Statuszeile zeigt dann „Verrundung beim Ziehen ausgesetzt (Zeile …)“, am Zeilenende steht derselbe Hinweis; nach dem Loslassen wertet die Engine vollständig aus und der Hinweis verschwindet (C20).</li>
  <li><b>Watchdog-Sperre (Plan 7.3):</b> die UI merkt sich bei jedem <code>Progress</code> den Schlüssel von <code>current</code>. Beendet der Watchdog den Worker, bekommt der neue Worker vor <code>SetSource</code> <code>{type: "BlockKeys", keys: […]}</code> mit allen bisher hängenden Schlüsseln (die Liste bleibt über weitere Neustarts erhalten) und dann die letzte Quelle erneut; der gesperrte Knoten meldet „Zeitüberschreitung, Knoten gesperrt bis zur nächsten Änderung“, seine Zeile ist rot (C19, Unit-Test transport.test.ts). Ohne neuen gesperrten Knoten wird die Quelle nicht erneut gesendet, damit dieselbe Eingabe nicht erneut hängt.</li>
  <li><b>tools/report/report2-ui.mjs</b> läuft wieder eigenständig: fehlen Vitest-Ergebnisse oder enthält das gemeinsame playwright.json keine Phase-A-Szenarien (es hält nur den letzten Lauf), führt es diese Tests selbst aus.</li>
</ul>
<p class="small">Änderungen an Rust-Crates: <code>cad-session</code> – Szenenknoten mit <code>op_info</code> (aufgelöste Parameter von fillet/chamfer/shell inkl. <code>select</code>, zone, linear_extrude, rotate_extrude, loft, sweep-Pfad, surface-Netz, thicken), <code>blend_suspended</code> je Knoten und <code>suspended_blends</code> in den Statistiken (aus dem Entwurfsmodus der Engine), Protokoll-Typen neu erzeugt; <code>cad-eval</code> – beim freien Ziehen werden Literale, deren Wert sich nicht ändert, nicht neu geschrieben (sonst würde das achsgebundene Ziehen z. B. 5.5 zu 6 runden), Tests für Profil-Griffe; <code>compat-tests</code> – Erwartungen für die fünf neuen Beispiele (der Test verlangt einen Eintrag je Datei in tests/models/spec).</p>

<h2>2 Testumgebung</h2>
<table>${env.map(([a, b]) => `<tr><th style="width:32mm">${esc(a)}</th><td>${esc(b)}</td></tr>`).join("")}</table>
<p class="small">Die End-to-End-Tests laufen gegen den Produktions-Build mit echten Maus- und Tastaturereignissen. Geprüft wird über den Testhaken <code>window.__cad</code> (Darstellungsplan, gezeichnete Kanten nach Art, Zonen-/Flächenprüfung, Kamera, Griffe, Editortext) und über Pixel: für eine Ebene (z. B. <code>edge:selected</code>, <code>open-faces</code>, <code>mode:profile</code>, <code>guide:path</code>) werden die Bildpunkte gezählt, die sich ändern, wenn sie ausgeblendet wird.</p>

<h2>3 Szenarien (Playwright, web/e2e/ui-c.spec.ts)</h2>
<table>
<tr><th>ID</th><th>Szenario</th><th>Ergebnis</th><th>Dauer</th><th>Geprüft</th></tr>
${scenarioRows}
</table>
<p class="small">Regression im selben Lauf: Phase A (web/e2e/ui.spec.ts) ${passed(phaseAUnique)} / ${phaseAUnique.length}, Phase B (web/e2e/ui-b.spec.ts) ${passed(phaseB)} / ${phaseB.length} bestanden.</p>

<h2>4 Unit- und Rust-Tests</h2>
${vt ? `<table><tr><th>Vitest-Datei</th><th>bestanden</th><th>Fehlgeschlagen</th></tr>${unitRows}</table>
<p class="small">Neu: kubische Pfad-Interpolation, Startrahmen des Profils, Punkt-in-Mesh mit Toleranz, Kanten in einer Zone (inside/touch), geöffnete Flächen, Kriterium für das Neu-Einpassen. Erweitert: Darstellungsplan für Profiloperationen, Zonen und shell (Fixture <code>profile.scad</code> aus der echten Engine), achsgebundenes Ziehen.</p>` : "<p>Keine Vitest-Ergebnisse gefunden.</p>"}
${cargo ? `<table><tr><th>Rust-Testprogramm (cargo test --workspace)</th><th>bestanden</th><th>neue/geänderte Tests</th></tr>${rustRows}</table>${rustBad.length ? `<p class="err">Fehlgeschlagen: ${rustBad.map(esc).join(", ")}</p>` : ""}` : ""}

<h2>5 Messwerte</h2>
<table><tr><th>Modell</th><th>Kernel / gesamt</th><th>Messung</th></tr>${metricRows}</table>
<p class="small">Jede Variante hat andere Parameter (kein Cache-Treffer im betroffenen Knoten). „Gesamt“ ist SetSource bis zur vollständigen Szene im Worker einschließlich Sprachauswertung und Tessellierung; die Zeiten stammen aus headless Chromium mit SwiftShader auf ${esc(os.cpus().length)} Kernen.</p>

<h2>6 Bekannte Grenzen und Abweichungen</h2>
<ul>
  <li><b>Kanten einer Zone, geöffnete Flächen:</b> die Ansicht prüft an den tessellierten Zonen-Meshes (Toleranz 1,5 × Sehnenfehler); die Engine wählt mit der exakten Kernel-Klassifikation. In Grenzfällen (Kante genau auf einer gekrümmten Zonenfläche) kann die Ansicht abweichen; im Test stimmen Anzahl und Engine überein.</li>
  <li><b>sweep-Pfad:</b> die Kurve wird in der UI nach demselben Verfahren wie in der SMLib-Fassade interpoliert (kubisch, Sehnenlängen, bei drei Punkten zwei Zusatzpunkte); ändert sich das Verfahren im Kernel, weicht die Anzeige ab. Der Startrahmen folgt sc_sweep (x = globales X in die Normalebene projiziert).</li>
  <li><b>Profil von linear_extrude/rotate_extrude:</b> gezeigt wird das Mesh der direkten Kinder (2D, flach in ihrer Ebene); z-Verschiebungen von 2D-Transformationen gehen dabei wie in der Engine verloren.</li>
  <li><b>Zone über translate:</b> der Cursor auf der Zone zeigt nur Zone und Kanten, nicht das Koordinatenkreuz der Transformation (Spec-Zeile „Cursor auf einer Zone“ geht vor); der Griff am Zonenmittelpunkt bleibt.</li>
  <li><b>Neu einpassen:</b> automatisch nur, solange die Kamera seit dem letzten Einpassen nicht gedreht/verschoben/gezoomt und kein Griff gezogen wurde; „Alles zeigen“ und die Standardansichten stellen diesen Zustand wieder her.</li>
  <li><b>Achsgebunden:</b> Achsen des lokalen Systems des Griffs (bei 2D-Punkten nur x/y); mit Umschalt wartet der Griff, bis 6 px Bewegung die Achse bestimmen. Wird die Achse erst während des Ziehens gewählt, geht die Achslinie durch die aktuelle Lage.</li>
  <li><b>Offene Fläche neben Körpern</b> auf oberster Ebene ist eine implizite Vereinigung und damit ein Fehler („zuerst thicken“) – Spec-konform, im Test getrennt geladen.</li>
  <li><b>Watchdog-Sperre:</b> gesperrt wird der Knoten aus dem letzten <code>Progress.current</code> vor dem Hängen; gesperrt ist sein Inhaltsschlüssel, gleiche Knoten an anderer Stelle sind mitgesperrt. Die Liste gilt bis zum Neuladen der Seite; eine Änderung am Knoten ändert den Schlüssel und hebt die Sperre damit auf.</li>
  <li><b>Entwurfsmodus:</b> der Hinweis zeigt den zuletzt gezeichneten Stand; die Schwelle (25 ms) und die Auswahl der ausgesetzten Verrundungen liegen in der Engine.</li>
  <li>Einige Ergebnisse hängen am Kernel: dose_shell.scad ist wegen F1 derzeit rot (Zeile rot, Körper ohne shell); griffschale.scad verwendet deshalb $chamfer = 4 (auf beiden Plattformen ein korrekter Hohlkörper); die Tests C08, C17 und C18 prüfen zusätzlich, dass ein „ok“-Knoten Geometrie hat.</li>
</ul>

<h2>7 Gefundene Fehler in fremden Crates</h2>
<p class="small">Nicht behoben (andere Crates als dieser Auftrag); minimale Beispiele zum Nachstellen im Browser oder mit <code>cad check datei.scad</code> (cad-cli, nativer Kernel).</p>
${bugBlocks}

<h2>8 Screenshots und Codeausschnitte</h2>
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
  footerTemplate: '<div style="font-size:7pt;color:#777;width:100%;text-align:center">Testbericht 4 – Verrundung, Fase, Shell und Profiloperationen · Seite <span class="pageNumber"></span> von <span class="totalPages"></span></div>',
  margin: { top: "14mm", bottom: "16mm", left: "12mm", right: "12mm" },
});
await browser.close();
console.log(`report4: ${passed(e2e)}/${e2e.length} Phase C, ${passed(phaseAUnique)}/${phaseAUnique.length} Phase A, ${passed(phaseB)}/${phaseB.length} Phase B, ${unitPassed}/${unitTotal} Vitest, ${rustOk}/${rustTotal} Rust, ${used.size} Bilder (max ${kib(maxImg)})`);
console.log(`-> ${join(out, "report.html")}`);
console.log(`-> ${join(out, "report.pdf")}`);
