#!/usr/bin/env node
// Test report 13 ("Korrekturen Geometrie"): the geometry and kernel findings of the everyday
// models (examples/alltag/README.md, tests/alltag/fehler) – cause, correction, evidence
// (before/after: volumes and diagnostics of the minimal examples, cad render images), open
// points, run-time analysis. Writes docs/reports/13-geometrie-korrekturen/report.html.
//
// Inputs (gitignored, reports/tmp/report13/):
//   fehler/*.json, img/*.png, zeit.*.json, fehler.txt    CAD_OLD=… tools/report/report13-data.sh
//   profil.txt                                           gdb stack samples of `cad check
//                                                        schluesselbrett.scad` (poor man's profiler)
//   cargo.txt, clippy.txt, fmt.txt, node.txt              cargo test --workspace --no-fail-fast,
//                                                        cargo clippy --workspace --all-targets -- -D warnings,
//                                                        cargo fmt --all --check,
//                                                        node crates/kernel-smlib/js/test-node.mjs DIR
//   compare.txt, step.txt, alltag.txt                    tests/openscad-compat/compare.py,
//                                                        tests/step/verify.py, tests/alltag/check.sh --fehler
//
//   node tools/report/report13.mjs
//   node tools/report/print-pdf.mjs docs/reports/13-geometrie-korrekturen/report.html docs/reports/13-geometrie-korrekturen/report.pdf "Testbericht 13: Korrekturen Geometrie"

import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import os from "node:os";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "../..");
const input = join(root, "reports/tmp/report13");
const out = join(root, "docs/reports/13-geometrie-korrekturen");
const imgDir = join(out, "img");
mkdirSync(imgDir, { recursive: true });

const esc = (s) => String(s ?? "").replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
const readText = (f) => (existsSync(f) ? readFileSync(f, "utf8") : null);
const readJson = (f) => {
  try {
    return existsSync(f) ? JSON.parse(readFileSync(f, "utf8")) : null;
  } catch {
    return null;
  }
};
const git = (...a) => {
  try {
    return execFileSync("git", a, { cwd: root, encoding: "utf8" }).trim();
  } catch {
    return "–";
  }
};
const de = (x, d = 0) => (x === null || x === undefined || Number.isNaN(Number(x)) ? "–" : Number(x).toLocaleString("de-DE", { minimumFractionDigits: d, maximumFractionDigits: d }));
const md = (t) => esc(t).replace(/`([^`]+)`/g, "<code>$1</code>").replace(/\*\*([^*]+)\*\*/g, "<b>$1</b>").replace(/\*([^*\s][^*]*)\*/g, "<i>$1</i>");
const T = (name) => readText(join(input, name))?.replaceAll(`${input}/`, "").replaceAll(`${root}/`, "") ?? "(fehlt)";
const pre = (t) => `<pre>${esc(t.trimEnd())}</pre>`;

// ------------------------------------------------------------------------------- images

function img(src, dst) {
  if (!existsSync(src)) return null;
  try {
    execFileSync("convert", [src, "-resize", "1200x>", "-quality", "85", join(imgDir, dst)]);
  } catch {
    dst = dst.replace(/\.jpg$/, ".png");
    copyFileSync(src, join(imgDir, dst));
  }
  return `img/${dst}`;
}
const shot = (n, v) => img(join(input, "img", `${n}.${v}.png`), `${n}-${v}.jpg`);
const fig = (src, cap) => (src ? `<figure><img src="${src}" alt=""><figcaption>${md(cap)}</figcaption></figure>` : `<p class="small">(Bild fehlt: ${esc(cap)})</p>`);
const pair = (n, cb, ca) => `<div class="two">${fig(shot(n, "vorher"), `Vorher: ${cb}`)}${fig(shot(n, "nachher"), `Nachher: ${ca}`)}</div>`;

// ------------------------------------------------------------------------------- minimal examples

const fehlerDir = join(root, "tests/alltag/fehler");
const examples = readdirSync(fehlerDir).filter((f) => f.endsWith(".scad")).sort();
const result = (name, v) => {
  const j = readJson(join(input, "fehler", `${name.replace(/\.scad$/, "")}.${v}.json`));
  if (!j) return { missing: true };
  if (j.crash) return { crash: true };
  const errs = (j.diagnostics ?? []).filter((d) => d.severity === "error");
  const warns = (j.diagnostics ?? []).filter((d) => d.severity === "warning");
  return { volume: j.volume?.volume ?? null, errs, warns };
};
const cell = (r) => {
  if (r.missing) return "–";
  if (r.crash) return '<span class="bad">Absturz (Exit 139), keine Ausgabe</span>';
  const v = `V ${de(r.volume, 1)} mm³`;
  if (r.errs.length) return `${v}<br><span class="small">${r.errs.length} Fehler: ${esc(r.errs[0].message.slice(0, 150))}${r.errs.length > 1 ? " …" : ""}</span>`;
  if (r.warns.length) return `${v}<br><span class="small">Warnung: ${esc(r.warns[0].message.slice(0, 120))}</span>`;
  return `${v}<br><span class="small">keine Meldung</span>`;
};
const fehlerTxt = readText(join(input, "fehler.txt")) ?? readText(join(input, "alltag.txt")) ?? "";
const checkState = (name) => {
  const l = fehlerTxt.split("\n").find((x) => x.includes(name));
  if (!l) return "–";
  return l.trim().startsWith("behoben") ? '<span class="ok">behoben</span>' : '<span class="bad">offen</span>';
};
const soll = (name) => {
  const t = readFileSync(join(fehlerDir, name), "utf8");
  return [...t.matchAll(/^\/\/ (SOLL[^\n]*)$/gm)].map((m) => m[1]).join("; ");
};

// ------------------------------------------------------------------------------- run time

const zeit = (v) => {
  const a = readJson(join(input, `zeit.${v}.json`));
  if (!a || !a.length) return null;
  const med = (k) => a.map((x) => x[k]).sort((p, q) => p - q)[Math.floor(a.length / 2)];
  return { evaluate: med("evaluate_ms"), kernel: med("kernel_ms"), render: med("render_ms"), n: a.length };
};
const zv = zeit("vorher");
const zn = zeit("nachher");

// ------------------------------------------------------------------------------- tests

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
  ? [...cargo.matchAll(/^test (\S+) \.\.\. (ok|FAILED)$/gm)].map((m) => ({ name: m[1], res: m[2] })).filter((t) =>
      /^(f0\d_|f1\d_|s[134]_|moved_|tangent_torus|difference_of_bodies|sweep_of_off_centre|fillet_at_a_nearly|chamfer_result_must|volume_accuracy)/.test(t.name),
    )
  : [];
const testsOf = (re) => newTests.filter((t) => re.test(t.name)).map((t) => `<code>${esc(t.name)}</code> ${t.res === "ok" ? '<span class="ok">ok</span>' : '<span class="bad">FAILED</span>'}`).join(", ") || "–";
const clippy = readText(join(input, "clippy.txt"));
const fmt = readText(join(input, "fmt.txt"));
const clippyOk = clippy != null && !/^(error|warning)/m.test(clippy);
const fmtOk = fmt != null && fmt.trim() === "";
const node = readText(join(input, "node.txt"));
const nodeOk = node != null && /all node tests passed/.test(node);
const compare = readText(join(input, "compare.txt"));
const compareSum = compare ? (/(\d+\/\d+) files match/.exec(compare)?.[1] ?? null) : null;
const step = readText(join(input, "step.txt"));
const stepSum = step ? (/\d+\/\d+ passed/.exec(step)?.[0] ?? null) : null;
const alltag = readText(join(input, "alltag.txt"));
const alltagLine = alltag ? (alltag.split("\n").find((l) => /Modelle, \d+ mit Problemen/.test(l)) ?? "–") : "–";
const fehlerLine = fehlerTxt ? (fehlerTxt.split("\n").find((l) => /behoben, \d+ offen/.test(l))?.trim() ?? "–") : "–";

const date = new Date().toLocaleDateString("de-DE", { day: "2-digit", month: "2-digit", year: "numeric" });
const commit = git("rev-parse", "--short", "HEAD");
const branch = git("rev-parse", "--abbrev-ref", "HEAD");

// ------------------------------------------------------------------------------- findings

const findings = [
  {
    id: "F9",
    title: "Absturz bei union(fillet) mit tangentialem Kontakt",
    files: ["f09a_absturz_tangentialer_dom.scad", "f09b_absturz_zapfen_sweep.scad"],
    cause: "In `SmFilletExecutive::CreateFilletCorners` (SMLib) wird für jeden Eckpunkt der zu verrundenden Kanten ein Ecktyp gewählt. Gibt es keinen passenden (`SmFilletCorner::Create` liefert NULL, „Unimplemented corner case“), wurde der Eckpunkt mit `continue` übergangen – die Tabelle `m_vVertexCornerMap` enthielt für ihn aber weiter die Liste seiner Kanten (`SmTArray<SmEdgeuse*>*`). `GetFilletCornerOfVertex` gab diese Liste später als `SmFilletCorner*` zurück; `CollectTangentEdgeSequence` rief darauf virtuelle Methoden auf → Zugriff auf Adresse 0x2f08 (gdb mit Debug-Objekten, valgrind: „not stack'd, malloc'd or free'd“). Bei F9a ist ein Ecktyp der tangentialen Berührung Dom/Wand, bei F9b die fast tangentiale Kreiskante Zapfen/Sweep.",
    fix: "**smlib/patches/0002-fillet-missing-corner.patch:** ohne Ecktyp wird der Eintrag entfernt und die Verrundung mit `SM_ERR_INVALID_INPUT` beendet, die Kante im `SmFilletErrorInfo` vermerkt. Die Fassade übersetzt das in „SMLib cannot blend the corner where these edges meet (faces that touch tangentially there?)“ mit der Kante (rot in `render --line`). Bei F9a meldet schon die Vereinigung den tangentialen Kontakt (siehe F8c): Dom und Wand berühren sich entlang einer Linie, ein mannigfaltiges Ergebnis gibt es nicht. Der Daemon-Rückfall war keine Lösung: der Absturz kommt aus dem Prozess selbst.",
    tests: /^(f09_|fillet_at_a_nearly)/,
  },
  {
    id: "F8",
    title: "Booleans mit Torus und rotate_extrude",
    files: ["f08a_union_torus.scad", "f08b_union_ring_verloren.scad", "f08c_differenz_torus_tangential.scad"],
    img: [["f08a", "zwei sich überlappende Körper, kein Schnitt mit der Deckfläche", "ein Körper, der Ring halb in der Platte"], ["f08b", "Ring fehlt", "Ring vorhanden"], ["f08c", "Volumen 0 ohne Meldung", "Fehlermeldung, Platte bleibt"]],
    cause: "Nicht der Torus selbst, sondern seine **Transformation**: Die Fassade rief `SmBrep::Transform(m, TRUE)`; mit `TRUE` leitet SMLib die Modellgröße aus der Hülle der *Eckpunkte* ab und setzt alle Toleranzen (Eckpunkte, Kanten, Flächen, außerdem die des globalen Kontexts) daraus. Ein Torus und der Rotationskörper eines Kreises haben genau einen Eckpunkt – Hülle 0, Toleranz 0. Mit Toleranz 0 verfehlt der Flächenschnitt jede Schnittkurve: Vereinigung = beide Körper nebeneinander (F8a, 6049,65 mm³ = Summe), Ring verloren (F8b), Schnittmenge leer. Nachgewiesen: derselbe Torus direkt bei z = 2 erzeugt schneidet richtig, verschoben (auch nur gedreht oder skaliert) nicht; die Geometrie (B-Rep-Strom) ist identisch, nur die Toleranzen 1e-5 → 0 unterscheiden sich. F7 (Torus als Zone wählt keine Kante) hat dieselbe Ursache.",
    fix: "`apply_matrix` transformiert mit `Transform(m, FALSE)`: starre Bewegungen lassen die Toleranzen unverändert; bei Skalierung setzt die Fassade sie aus der ganzen (losen) Hülle des Körpers, der Kontext bleibt unberührt. F8c: Der Torus berührt Deck- und Bodenfläche tangential entlang zweier Kreise; das Ergebnis ist nicht mannigfaltig (SMLib liefert es mit richtigem Volumen 5260,36 mm³, aber nicht geschlossen, `measure` zeigte 0). Die Prüfung der Booleans nimmt ein nicht geschlossenes Ergebnis zweier geschlossener Körper nicht mehr als „letzten Ausweg“ an, sondern meldet „the result is not a closed solid: the operands touch tangentially or along an edge there (let them overlap a little or keep a gap)“. Berühren sich die Körper nur (Innenräume überlappen nicht), gilt a − b = a und a ∩ b = leer statt eines nicht mannigfaltigen Körpers.",
    tests: /^(f08_|f07_|moved_|tangent_torus|difference_of_bodies)/,
  },
  {
    id: "F5",
    title: "Fase über Zone am verrundeten Kasten: Volumen 0, Status ok",
    files: ["f05_fase_volumen_null.scad"],
    img: [["f05", "offene Hülle (26 Flächen), Volumen 0, keine Meldung", "Fehlermeldung, Körper ohne Fase bleibt"]],
    cause: "SMLib liefert `SM_SUCCESS` und eine offene Hülle: die Deckfläche fehlt. Grund ist die Geometrie: An den Ecken ist die Wand zwischen Außenradius 6 und der scharfen Innenecke nur 6 − 3√2 ≈ 1,76 mm dick; die Außenfase nimmt 0,8 mm, die Innenfase an der scharfen Ecke 0,8·√2 ≈ 1,13 mm – die Fasen überschneiden sich, die Deckfläche zerfällt. Nachgemessen: `chamfer(0.7)` geht, `chamfer(0.75)` nicht (Grenze rechnerisch 0,73). Fasen der äußeren und der inneren Kante je allein gehen.",
    fix: "Die Fassade prüft jedes Ergebnis einer Verrundung/Fase (`blend_core`): geschlossene Hülle, wenn der Eingang geschlossen war; Volumen > 0 und Volumenänderung höchstens so groß, wie eine Verrundung dieser Größe sie nahe den Kanten bewirken kann (20 r² je mm Kantenlänge + 10 r³ je Kante + 2 %). Verletzt → „SMLib returned a body that is not closed (faces missing): the blends of neighbouring edges collide, size too large for the wall between them?“ mit den Kanten. Zusätzlich: scheitert die Auswahl als Ganzes und zerfällt sie in getrennte Kantenketten (ohne gemeinsamen Eckpunkt), werden die Ketten nacheinander verrundet (Kanten im Zwischenkörper über ihre Geometrie wiedergefunden) – hier vergeblich, weil die Fasen wirklich kollidieren, aber hilfreich, wo SMLib nur an der Menge scheitert.",
    tests: /^(f05_|chamfer_result_must)/,
  },
  {
    id: "F10",
    title: "union(fillet) aus verrundeter Schale und Dom: nur der Dom",
    files: ["f10_union_fillet_schale_verloren.scad"],
    img: [["f10", "nur Dom mit Kehle, Volumen 0", "Fehlermeldung, Vereinigung ohne Kehle bleibt"]],
    cause: "Die Vereinigung selbst ist richtig (26 043 mm³). Die Kehle r 1 um den Dom (7,5 mm von der Innenecke) läuft auf die Bodenverrundung der Schale (Innenradius 1,5 zwischen x = 2 und 3,5); SMLib gibt dann einen offenen Körper aus Dom und Kehle zurück.",
    fix: "Die Plausibilitätsprüfung der Verrundung (F5) erkennt die offene Hülle: Fehler mit den Kanten, die Vereinigung ohne Kehle bleibt (statt Volumen 0 ohne Meldung). Eine richtig gerechnete Kehle über zwei Flächen bleibt offen (SMLib).",
    tests: /^f10_/,
  },
  {
    id: "F1",
    title: "sweep mit symmetrischem Spline-Profil „not centred“",
    files: ["f01_sweep_spline_profil.scad"],
    img: [["f01", "abgelehnt (kein Körper)", "Sweep der Ellipse"]],
    cause: "SMLib (`CreateCurveSweep`, `bMoveProfileCenterToPath`) setzt den Mittelpunkt der Hülle der Profilkurven auf den Pfad. `CalculateBoundingBox` einer B-Spline-Kurve ist die Hülle ihrer *Kontrollpunkte*: für den geschlossenen Interpolationsspline durch (5,0) (0,3) (−5,0) (0,−3) x −15…5, obwohl die Kurve selbst symmetrisch ist (abgetastet ±5, ±3,08). Die Fassade lehnte jedes so „außermittige“ Profil auf gekrümmtem Pfad ab.",
    fix: "**smlib/patches/0003-sweep-profile-reference-point.patch:** `SmSweepOptions` erhält `bUseProfileRefPoint`/`sProfileRefPoint`; die Fassade gibt den Ursprung des Profilrahmens (Pfadanfang) vor. Damit folgen beliebige, auch bewusst außermittige Profile (Kreis bei (0, 4)) dem Pfad mit ihrem Ursprung; das Verschieben nach dem Sweep auf geraden Pfaden entfällt. Nur die Gehrungs-Variante für ebene Polylinien-Pfade hat die Option nicht (Meldung nennt den Ausweg Spline-Pfad).",
    tests: /^(f01_|sweep_of_off_centre)/,
  },
  {
    id: "F2",
    title: "Zylinder ∪ gekrümmter Sweep: NULL_POINTER, doppelte Diagnose",
    files: ["f02_union_zylinder_sweep.scad"],
    cause: "Der Sweep ist gültig (geschlossen, keine sich überlappenden Flächen) und vereinigt sich mit Quadern, auch wenn diese die Endkappe schneiden. Mit dem Zylinder (auch als NURBS, auch gedreht) scheitert SMLibs Flächenschnitt nahe der gekippten Endkappe (Kappe x 16,3…21,7 schneidet r = 20). Die Fassade versucht Reihenfolgen und Verschiebungen 1e-6…1e-4 – ohne Erfolg. Die doppelte Meldung kam aus der Engine: die gescheiterte Vereinigung gibt ihre Operanden zurück, die Wurzel vereinigt sie erneut und scheitert ein zweites Mal.",
    fix: "Engine: Teile aus einer gescheiterten Boolean-Operation tragen eine Markierung (`NodeOutput::unmerged`); scheitert die Vereinigung im Elternknoten an ihnen erneut, wird das nicht noch einmal gemeldet. Der Kernel-Fehler selbst bleibt offen.",
    tests: /^f02_/,
  },
  {
    id: "F3, F4",
    title: "Sweep eines 16-Ecks „not a closed solid“; union(fillet) an 32-Eck-Sweep",
    files: ["f03_sweep_polygon16.scad", "f04_union_fillet_sweep_polygon.scad"],
    cause: "SMLib baut je Polygonseite eine Seitenfläche; zwischen einigen Nachbarn bleiben Zwillingskanten mit Abstand 0 unverklebt (je 1 Fläche). `StitchFaces`/`SmApiStitchAdvanced` mit Toleranz 1e-5…1e-3 kleben sie ebenfalls nicht (0 geklebt). Auf anderen Pfaden betrifft es 12- oder 32-Ecke. F4: die Verrundung der 32 Facettenkanten an der Kreiskante scheitert (`SM_ERR`).",
    fix: "Meldung mit Ursache: „SMLib left 3 seam(s) between the side faces of the polygon profile open (SMLib limitation on curved paths; circle() or a spline() profile work)“. Eine Korrektur in SMLib (Aufbau der Seitenflächen im Sweep) bleibt offen.",
    tests: /^f03_/,
  },
  {
    id: "F13",
    title: "Meldung bei shell eines Körpers mit Verrundung ≤ Wandstärke",
    files: ["f13_shell_verrundung.scad"],
    cause: "Die Innenfläche einer konvexen Verrundung mit Radius r < t entartet beim Versatz um t; SMLib meldet nur SM_ERR, der Ersatzweg über Booleans scheitert ebenso.",
    fix: "Scheitern beide Wege, bestimmt die Fassade den kleinsten konvexen Krümmungsradius der nicht geöffneten Flächen (Normalen-Differenzen auf einem 7×7-Raster je Fläche) und nennt ihn: „wall thickness 2 is not smaller than the radius of a convex rounding of the body (about 2): the inner surface degenerates there (rounding radius must be > wall thickness)“.",
    tests: /^f13_/,
  },
  {
    id: "F6, F11, F12, F15",
    title: "Kernel-Fehler mit Meldung, ohne Korrektur",
    files: ["f06_fase_konkave_ecke.scad", "f11_stift_deckungsgleich.scad", "f12_shell_loft_quadrat_kreis.scad", "f15_knauf_kehle.scad"],
    cause: "F6: SMLib hat für die Ecke „konvexe Kante endet an kurzer konkaver Kante“ keinen funktionierenden Ecktyp (SM_ERR; die Kette ist zusammenhängend, das Nacheinander hilft nicht). F11: deckungsgleiche Zylinderflächen (Stift 3,0 in Bohrung 3,0); Verschiebungen um 1e-6…1e-4 der Modellgröße ändern daran nichts. F12: `SmApiShellBrepFull` des Lofts Quadrat → Kreis (NULL_POINTER in SmMerge) und der Ersatzweg über Booleans scheitern. F15: Verrundung Sweep d 6 / Kugel d 9 (SM_ERR) – mit d 10 geht es.",
    fix: "Keine Korrektur im Rahmen dieses Berichts; die Meldungen markieren die Kanten (F6, F15) bzw. nennen die Operation. Umgehungen: Zonen an Rasterlinien statt an der Ebene (F6), 0,05 mm Spiel (F11), Differenz zweier Lofts (F12), Kugel d 10 (F15).",
    tests: /^$/,
  },
];

const overviewRows = examples
  .map((n) => `<tr><td><code>${esc(n.replace(/\.scad$/, ""))}</code></td><td class="small">${esc(soll(n))}</td><td>${cell(result(n, "vorher"))}</td><td>${cell(result(n, "nachher"))}</td><td>${checkState(n)}</td></tr>`)
  .join("\n");

const findingHtml = findings
  .map(
    (f) => `<section class="finding">
<h3>${esc(f.id)} – ${esc(f.title)}</h3>
<dl class="f"><dt>Beispiele</dt><dd>${f.files.map((x) => `<code>tests/alltag/fehler/${esc(x)}</code>`).join(", ")}</dd>
<dt>Ursache</dt><dd>${md(f.cause)}</dd>
<dt>Korrektur</dt><dd>${md(f.fix)}</dd>
<dt>Nachweis</dt><dd>${f.files.map((x) => `<code>${esc(x.replace(/\.scad$/, ""))}</code>: vorher ${cell(result(x, "vorher"))} → nachher ${cell(result(x, "nachher"))}`).join("<br>")}<br>Tests: ${testsOf(f.tests)}</dd></dl>
${(f.img ?? []).map(([n, b, a]) => pair(n, b, a)).join("\n")}
</section>`,
  )
  .join("\n");

const html = `<!doctype html>
<html lang="de">
<head>
<meta charset="utf-8">
<title>Testbericht 13 – Korrekturen Geometrie</title>
<style>
  @page { size: A4; margin: 15mm 13mm 16mm 13mm; }
  :root { --text: #1f2328; --muted: #5d646e; --border: #d5d9df; --ok: #1a7f37; --bad: #c9302c; --head: #f2f4f7; }
  * { box-sizing: border-box; }
  body { font: 9.4pt/1.45 system-ui, -apple-system, "Segoe UI", Roboto, sans-serif; color: var(--text); margin: 0; }
  main { max-width: 184mm; margin: 0 auto; }
  h1 { font-size: 18pt; margin: 0 0 2mm; }
  h2 { font-size: 13pt; margin: 7mm 0 3mm; border-bottom: 1px solid var(--border); padding-bottom: 1mm; break-after: avoid; }
  h3 { font-size: 10.5pt; margin: 5mm 0 1.5mm; break-after: avoid; }
  p { margin: 1.5mm 0; }
  .meta { color: var(--muted); margin-bottom: 4mm; }
  .summary { display: flex; flex-wrap: wrap; gap: 2.5mm; margin: 3mm 0; }
  .summary div { flex: 1 1 28mm; border: 1px solid var(--border); border-radius: 2mm; padding: 2mm; font-size: 8.2pt; }
  .summary b { display: block; font-size: 12pt; }
  table { width: 100%; border-collapse: collapse; margin: 2mm 0; font-size: 8pt; }
  th, td { border: 1px solid var(--border); padding: 1mm 1.6mm; text-align: left; vertical-align: top; }
  th { background: var(--head); font-weight: 600; }
  td.n { text-align: right; white-space: nowrap; }
  code, pre { font-family: ui-monospace, "SFMono-Regular", Menlo, Consolas, monospace; font-size: 7.5pt; }
  pre { margin: 0 0 1.5mm; padding: 1.4mm 2mm; background: #f6f8fa; border: 1px solid var(--border); border-radius: 1mm; white-space: pre-wrap; word-break: break-word; }
  .small { font-size: 7.5pt; color: var(--muted); font-weight: 400; }
  ul { margin: 1mm 0 2mm 5mm; padding: 0; }
  li { margin: 0.5mm 0; }
  tr, figure, pre { break-inside: avoid; }
  .ok { color: var(--ok); font-weight: 600; } .bad { color: var(--bad); font-weight: 600; }
  figure { margin: 2mm 0; }
  figure img { max-width: 100%; max-height: 80mm; border: 1px solid var(--border); border-radius: 1mm; }
  figcaption { font-size: 7.6pt; color: var(--muted); }
  .two { display: grid; grid-template-columns: 1fr 1fr; gap: 3mm; align-items: start; break-inside: avoid; }
  dl.f { display: grid; grid-template-columns: 20mm 1fr; gap: 0.8mm 3mm; margin: 1.5mm 0; }
  dl.f dt { font-weight: 600; color: var(--muted); }
  dl.f dd { margin: 0; }
  .note { border-left: 1mm solid #e8b84a; padding: 1mm 2.5mm; background: #fff8e6; margin: 2mm 0; }
</style>
</head>
<body>
<main>
<h1>Testbericht 13 – Korrekturen Geometrie</h1>
<div class="meta">Geometrie- und Kernelfehler aus den Alltagsmodellen (<code>examples/alltag/README.md</code>, <code>tests/alltag/fehler/</code>) · SMLib-Fassade, SMLib-Patches 0002/0003, cad-engine, cad-render, cad-cli · ${esc(date)} · Branch ${esc(branch)} · Commit ${esc(commit)} · ${os.cpus().length} Kerne</div>

<div class="summary">
  <div>cargo test --workspace<b class="${cargoSum && cargoSum.failed === 0 ? "ok" : "bad"}">${cargoSum ? `${cargoSum.ok} / ${cargoSum.ok + cargoSum.failed}` : "–"}</b>davon ${newTests.length} neue Regressionstests</div>
  <div>clippy · fmt<b class="${clippyOk && fmtOk ? "ok" : "bad"}">${clippy == null ? "–" : clippyOk ? "sauber" : "Befund"} · ${fmt == null ? "–" : fmtOk ? "sauber" : "Abw."}</b>-D warnings</div>
  <div>Wasm-Kernel (Node)<b class="${nodeOk ? "ok" : "bad"}">${node == null ? "–" : nodeOk ? "bestanden" : "Fehler"}</b>Fassade neu gebaut</div>
  <div>OpenSCAD · STEP<b class="ok">${esc(compareSum ?? "–")} · ${esc(stepSum ?? "–")}</b>compare.py, verify.py</div>
  <div>Alltagsmodelle<b class="${/ 0 mit Problemen/.test(alltagLine) ? "ok" : "bad"}">${esc(alltagLine.split(",").slice(0, 2).join(","))}</b>Volumina unverändert</div>
  <div>Fehlerbeispiele<b>${esc(fehlerLine)}</b>gegen ihren Soll-Zustand</div>
  <div>Schlüsselbrett<b class="ok">${zv && zn ? `${de(zv.evaluate / 1000, 1)} → ${de(zn.evaluate / 1000, 1)} s` : "–"}</b>Auswertung je Aufruf</div>
</div>

<h2>1 Überblick</h2>
<p>Die Alltagsmodelle fanden einen Absturz, vier still falsche Ergebnisse und eine Reihe von Kernelfehlern. Jedes Minimalbeispiel in <code>tests/alltag/fehler/</code> trägt jetzt im Kopf seinen Soll-Zustand (<code>// SOLL: fehlerfrei</code>, <code>SOLL-VOLUMEN</code>, <code>SOLL-FEHLER: Text</code> – wo es kein richtiges Ergebnis gibt, ist eine klare Meldung das Ziel); <code>tests/alltag/check.sh --fehler</code> meldet je Beispiel „behoben“ oder „offen“. Die Tabelle zeigt beide Stände (vorher: Stand vor den Korrekturen, Commit <code>cba3ab4</code>; nachher: dieser Commit), <code>cad measure --volume --json</code>.</p>
<table><tr><th>Beispiel</th><th>Soll</th><th>vorher</th><th>nachher</th><th>check.sh</th></tr>
${overviewRows}
</table>
<p><b>Nichts mehr still falsch:</b> Von den Befunden mit falschem Ergebnis ohne Meldung (F5, F8a–c, F10) sind F8a/F8b richtig gerechnet, F5, F8c und F10 werden gemeldet (F5 und F8c haben kein richtiges Ergebnis: kollidierende Fasen, tangentiale Berührung). Kein Beispiel stürzt mehr ab.</p>

<h2>2 Korrekturen in der Fassade und in SMLib</h2>
<table><tr><th>Stelle</th><th>Änderung</th><th>Befunde</th></tr>
<tr><td><code>smlib/patches/0002-fillet-missing-corner.patch</code></td><td>SmFilletExecutive::CreateFilletCorners: Eckpunkt ohne Ecktyp → Fehler statt Typverwechslung (Absturz)</td><td>F9</td></tr>
<tr><td><code>smlib/patches/0003-sweep-profile-reference-point.patch</code></td><td>SmSweepOptions::bUseProfileRefPoint/sProfileRefPoint: Bezugspunkt des Profils beim Kurven-Sweep vorgeben</td><td>F1</td></tr>
<tr><td><code>apply_matrix</code></td><td>Transform(m, FALSE): starre Bewegungen behalten die Toleranzen, Skalierung setzt sie aus der ganzen Hülle; Kontext unberührt</td><td>F7, F8a, F8b</td></tr>
<tr><td><code>checked_boolean</code></td><td>nicht geschlossenes Ergebnis zweier geschlossener Körper oder unmögliches Volumen → Fehler statt „letzter Ausweg“; nur berührende Körper: a − b = a, a ∩ b = leer</td><td>F8c, F9a</td></tr>
<tr><td><code>blend_core</code>, <code>do_fillet_like</code></td><td>Ergebnis einer Verrundung/Fase: geschlossen, Volumen plausibel; getrennte Kantenketten nacheinander; Meldung für fehlenden Ecktyp</td><td>F5, F10, F9b</td></tr>
<tr><td><code>sc_sweep</code></td><td>Profilursprung als Bezugspunkt (0003); offene Seitennähte bei Polygonprofilen mit Ursache gemeldet</td><td>F1, F3</td></tr>
<tr><td><code>sc_shell</code></td><td>Meldung mit kleinstem konvexem Radius ≤ Wandstärke</td><td>F13</td></tr>
<tr><td><code>sc_mass_props</code></td><td>Volumen mit relativer Genauigkeit 1e-6 statt 1e-8 (Fehler ≤ 5e-6 gemessen)</td><td>Laufzeit</td></tr>
</table>
<p class="small">Die Patches liegen unter <code>smlib/patches/</code>, <code>smlib/fetch.sh</code> wendet sie der Reihe nach an. Native Bibliotheken: <code>smlib/build-native.sh</code>; der Wasm-Kernel ist neu gebaut (<code>smlib/build-wasm.sh</code>) und besteht <code>node crates/kernel-smlib/js/test-node.mjs</code>.</p>

<h2>3 Befunde im Einzelnen</h2>
${findingHtml}

<h2>4 Darstellung und CLI (S1–S4)</h2>
<h3>S1 – Profilraster bei rotate_extrude</h3>
<p><b>Ursache:</b> Ein 2D-Raster (x = Radius, y = Höhe) wird im System seines Moduls gezeichnet, also in der xy-Ebene; das Profil liegt nach <code>rotate_extrude</code> in der xz-Ebene. <b>Korrektur (CLI):</b> <code>cad render --show grids</code> prüft für jedes Raster seine Bezüge (<code>Scene.refs</code>): liegen alle in 2D-Knoten unter demselben <code>rotate_extrude</code>, wird das Raster in dessen Profilebene gezeichnet (Rahmen RE · Rx(90°) · RE⁻¹ · N · frame_module) und ein Hinweis ausgegeben. Raster mit Bezügen außerhalb (oder über Variablen) bleiben, wie sie sind.</p>
${pair("s1", "Raster p in der xy-Ebene, 95 mm hinter dem Becher", "Raster p in der Profilebene: p.unten bei r 36, p.oben bei r 41")}
<p><b>UI (nicht geändert, <code>web/</code>):</b> Die 3D-Ansicht zeichnet Raster aus <code>Scene.grids[].frame</code>. Für dieselbe Korrektur bräuchte sie entweder die Regel oben (Bezüge aus <code>Scene.refs</code> → 2D-Knoten → <code>op_info.kind == "rotate_extrude"</code>) oder besser einen Rahmen, den die Szene je Raster mitliefert (<code>Grid.display_frame</code>, in <code>cad-session</code> nach derselben Regel berechnet). Zum Ziehen/Einrasten an Rasterpunkten müssten die Griffe ebenso in die Profilebene abgebildet werden.</p>
<h3>S3 – outline und Sichtbarkeit</h3>
<p><code>cad outline</code> führte Instanzen, die im Rumpf eines Moduls benannt werden (<code>dom[0]</code> … in <code>pi_platte</code>), unter „Benannte Instanzen“ der Datei und ihre Anker als Datei-Anker. Jetzt: eigener Abschnitt „Benannte Instanzen in Modulen (nur dort sichtbar)“ gruppiert nach Modul; im JSON <code>scope: "file" | "module"</code> und <code>in_module</code>; Anker nur der Datei-Instanzen. Dass <code>measure --between dom[0].oben …</code> im Dateikontext noch funktioniert, ist Sprachsemantik (cad-eval) und bleibt als Vorschlag: solche Namen nur über die Instanzkette (<code>platte.dom[0].oben</code>) oder mit Warnung auflösen.</p>
<h3>S4 – Schnittkappen und Rasterbeschriftung</h3>
<p>Ein Modell aus Bauteilen wurde im Schnitt als ein Netz der Wurzel gezeichnet: die Kappen berührender Bauteile verschmolzen. Jetzt wird es, sobald die Bauteile das ganze Modell ausmachen, je Bauteil gezeichnet: eigene Kappe mit wechselnder Schraffur und eigener Schnittkontur. Rasterbeschriftungen: Reihen indizierter Ebenen (<code>rippe[0]</code> … <code>rippe[7]</code>) erhalten eine Beschriftung <code>g.rippe[0..7]</code>; zwischen Beschriftungen bleiben 3 px (vorher konnten sie direkt aneinanderstoßen).</p>
${pair("s4", "Deckel und Unterteil eine Kappe", "Deckel abgesetzt (Kontur, Gegenschraffur)")}
${pair("s4b", "je Rippe eine Beschriftung", "eine Beschriftung je Reihe")}
<h3>S2 – Ebenennamen als Werte, Anker-Listen (nur Bewertung)</h3>
<ul>
<li><b>Ebenennamen als Werte</b> (<code>g[i % 2 == 0 ? loch_l : loch_r, …]</code>): Die Namen in <code>g[…]</code> sind heute Selektoren, keine Ausdrücke; die Umgehung über Zahlen (<code>g.loch_l</code>) verliert die Rasterbindung (Snap, Ziehen). <b>Vorschlag:</b> ein Wert-Typ „Ebene“ (<code>g.loch_l</code> ergibt weiter die Zahl, aber <code>plane(g.loch_l)</code> bzw. ein Selektor-Ausdruck <code>g[sel(i % 2 == 0 ? "loch_l" : "loch_r"), …]</code>), den <code>g[…]</code> annimmt; so bleibt die Herkunft für Griffe erhalten. Aufwand mittel (cad-eval: Selektorauswertung, Drag-Rückschreiben nur für literale Selektoren).</li>
<li><b>Anker-Listen</b> (<code>anchor loch[i] = …</code> in einer Schleife): passt zur bestehenden Indizierung von Instanzen (<code>k[i]</code>) und Ebenen (<code>rippe[i]</code>). <b>Vorschlag:</b> gleiche Regel wie bei Ebenen (Name + Index, außen <code>k.loch[2]</code>), Snap-Ziele je Element. Aufwand gering bis mittel.</li>
<li><b>Rasterlokale Koordinaten</b> für Profile in gedrehten Ebenen (S1): ein Raster mit eigenem Rahmen (<code>grid p in xz { … }</code>) würde S1 auch in der Sprache lösen.</li>
</ul>

<h2>5 Laufzeit</h2>
<p><b>Befund:</b> Schlüsselbrett 4,0 s Kernel, aber ≈ 16 s je Auswertung. <b>Messung:</b> Stapelproben mit gdb (alle ~0,8 s) während <code>cad check schluesselbrett.scad</code>:</p>
${pre(T("profil.txt"))}
<p><b>Ursache:</b> Nach dem Kernel-Lauf berechnet die Sitzung das Volumen jedes Bauteils (Bauteilliste der Szene, <code>part_volumes</code>); <code>sc_mass_props</code> rief nach den Masseeigenschaften (1e-4) zusätzlich <code>SmApiBrepComputeVolume(…, 1e-8)</code>. Die Genauigkeit 1e-8 kostet über adaptive Runge-Kutta-Integration je Fläche (<code>SmFace::ComputePreciseProperties</code>) bei den gesweepten Haken rund 11 s. Gemessener relativer Volumenfehler: 1e-6 → ≤ 4,4e-6, 1e-7 → ≤ 1,1e-7, 1e-8 → ≤ 1,5e-8 (Kugel, Torus, Kegel). <b>Korrektur:</b> 1e-6 (Bauteilvolumina werden mit 0,1 mm³ angezeigt; Prüfungen nutzen 0,1 %).</p>
<table><tr><th>cad render schluesselbrett.scad (Median von ${zn?.n ?? "–"})</th><th>vorher</th><th>nachher</th></tr>
<tr><td>Auswertung (evaluate_ms, ohne Bild)</td><td class="n">${zv ? de(zv.evaluate, 0) : "–"} ms</td><td class="n">${zn ? de(zn.evaluate, 0) : "–"} ms</td></tr>
<tr><td>davon Kernel-Lauf (kernel_ms)</td><td class="n">${zv ? de(zv.kernel, 0) : "–"} ms</td><td class="n">${zn ? de(zn.kernel, 0) : "–"} ms</td></tr>
<tr><td>Bild (render_ms)</td><td class="n">${zv ? de(zv.render, 0) : "–"} ms</td><td class="n">${zn ? de(zn.render, 0) : "–"} ms</td></tr></table>
<p class="small">Der Rest zwischen Kernel und Auswertung sind jetzt im Wesentlichen die Bauteilvolumina (1e-6) und Tessellierung/Szene. Weiter ginge es, die Volumina nur zu berechnen, wenn sie gebraucht werden (<code>outline</code>, <code>measure</code>, UI-Bauteilliste), oder sie aus den Masseeigenschaften (1e-4) zu nehmen.</p>

<h2>6 Offen</h2>
<ul>
<li>F2 (Zylinder ∪ gekrümmter Sweep), F3/F4 (Sweeps von Polygonprofilen), F6 (Ecktyp konvex/konkav), F11 (deckungsgleiche Zylinder), F12 (shell eines Lofts), F15 (Kehle Sweep/Kugel): SMLib-Fehler; Meldungen und Umgehungen wie oben.</li>
<li>F10: die Kehle über zwei Flächen wird gemeldet, nicht gerechnet.</li>
<li>F5, F8c, F9a haben kein richtiges Ergebnis (Kollision, tangentiale Berührung); ein automatisches „ein wenig überlappen“ wäre möglich, verändert aber die Geometrie ohne Auftrag.</li>
<li>S1 in der UI (Vorschlag oben), S2 und die Sichtbarkeit von <code>dom[0].oben</code> im Dateikontext (Sprachsemantik).</li>
<li>Die Prüfung der Verrundung stützt sich auf grobe Volumina (1e-2); Fehler, die das Volumen nur wenig ändern, bleiben unerkannt.</li>
</ul>

<h2>7 Testläufe</h2>
<h3>Neue Regressionstests</h3>
<p>${newTests.map((t) => `<code>${esc(t.name)}</code> ${t.res === "ok" ? '<span class="ok">ok</span>' : '<span class="bad">FAILED</span>'}`).join(", ") || "–"}</p>
<p class="small">Kernel: <code>crates/kernel-smlib/tests/alltag.rs</code>; Engine: <code>crates/cad-engine/tests/alltag_fehler.rs</code> (liest die Minimalbeispiele); CLI: <code>crates/cad-cli/tests/geometrie.rs</code>.</p>
<h3>Fehlerbeispiele (check.sh --fehler)</h3>
${pre(fehlerTxt.split("\n").filter((l) => /^\s+(behoben|offen)|Fehlerbeispiele/.test(l)).join("\n") || "(fehlt)")}
<h3>Alltagsmodelle, OpenSCAD-Vergleich, STEP</h3>
${pre([alltagLine, compareSum ? `compare.py: ${compareSum} files match` : "compare.py: –", stepSum ? `verify.py: ${stepSum}` : "verify.py: –"].join("\n"))}
<h3>cargo test, clippy, fmt, Wasm</h3>
${pre([cargoSum ? `cargo test --workspace: ${cargoSum.ok} passed, ${cargoSum.failed} failed` : "cargo test: –", `clippy -D warnings: ${clippy == null ? "–" : clippyOk ? "sauber" : "Befund"}`, `fmt --check: ${fmt == null ? "–" : fmtOk ? "sauber" : "Abweichung"}`, `node test-node.mjs: ${node == null ? "–" : nodeOk ? "all node tests passed" : "Fehler"}`].join("\n"))}
</main>
</body>
</html>
`;
writeFileSync(join(out, "report.html"), html);
console.log(`${join(out, "report.html")}: ${html.length} Zeichen, ${examples.length} Beispiele, ${newTests.length} neue Tests`);
