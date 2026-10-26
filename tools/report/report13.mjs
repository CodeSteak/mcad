#!/usr/bin/env node
// Test report 13 ("geometry fixes"): the geometry and kernel findings of the everyday
// models (tests/models/alltag/README.md, tests/alltag/fehler) – cause, correction, evidence
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
//   node tools/report/print-pdf.mjs docs/reports/13-geometrie-korrekturen/report.html docs/reports/13-geometrie-korrekturen/report.pdf "Test report 13: geometry fixes"

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
const num = (x, d = 0) => (x === null || x === undefined || Number.isNaN(Number(x)) ? "–" : Number(x).toLocaleString("en-US", { minimumFractionDigits: d, maximumFractionDigits: d }));
const md = (t) => esc(t).replace(/`([^`]+)`/g, "<code>$1</code>").replace(/\*\*([^*]+)\*\*/g, "<b>$1</b>").replace(/\*([^*\s][^*]*)\*/g, "<i>$1</i>");
const T = (name) => readText(join(input, name))?.replaceAll(`${input}/`, "").replaceAll(`${root}/`, "") ?? "(missing)";
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
const fig = (src, cap) => (src ? `<figure><img src="${src}" alt=""><figcaption>${md(cap)}</figcaption></figure>` : `<p class="small">(image missing: ${esc(cap)})</p>`);
const pair = (n, cb, ca) => `<div class="two">${fig(shot(n, "vorher"), `Before: ${cb}`)}${fig(shot(n, "nachher"), `After: ${ca}`)}</div>`;

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
  if (r.crash) return '<span class="bad">crash (exit 139), no output</span>';
  const v = `V ${num(r.volume, 1)} mm³`;
  if (r.errs.length) return `${v}<br><span class="small">${r.errs.length} ${r.errs.length === 1 ? "error" : "errors"}: ${esc(r.errs[0].message.slice(0, 150))}${r.errs.length > 1 ? " …" : ""}</span>`;
  if (r.warns.length) return `${v}<br><span class="small">warning: ${esc(r.warns[0].message.slice(0, 120))}</span>`;
  return `${v}<br><span class="small">no message</span>`;
};
const fehlerTxt = readText(join(input, "fehler.txt")) ?? readText(join(input, "alltag.txt")) ?? "";
const checkState = (name) => {
  const l = fehlerTxt.split("\n").find((x) => x.includes(name));
  if (!l) return "–";
  // check.sh prints "behoben"/"offen" (German) or "fixed"/"open"
  return /^(behoben|fixed)/.test(l.trim()) ? '<span class="ok">fixed</span>' : '<span class="bad">open</span>';
};
const soll = (name) => {
  const t = readFileSync(join(fehlerDir, name), "utf8");
  return [...t.matchAll(/^\/\/ ((?:SOLL|EXPECT)[^\n]*)$/gm)].map((m) => m[1]).join("; ");
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
const alltagLine = alltag ? (alltag.split("\n").find((l) => /(Modelle, \d+ mit Problemen|models, \d+ with problems)/.test(l)) ?? "–") : "–";
const fehlerLine = fehlerTxt ? (fehlerTxt.split("\n").find((l) => /(behoben, \d+ offen|fixed, \d+ open)/.test(l))?.trim() ?? "–") : "–";

const date = new Date().toISOString().slice(0, 10);
const commit = git("rev-parse", "--short", "HEAD");
const branch = git("rev-parse", "--abbrev-ref", "HEAD");

// ------------------------------------------------------------------------------- findings

const findings = [
  {
    id: "F9",
    title: "crash with union(fillet) and tangential contact",
    files: ["f09a_absturz_tangentialer_dom.scad", "f09b_absturz_zapfen_sweep.scad"],
    cause: "In `SmFilletExecutive::CreateFilletCorners` (SMLib) a corner type is chosen for each vertex of the edges to be filleted. If there is no suitable one (`SmFilletCorner::Create` returns NULL, \"Unimplemented corner case\"), the vertex was skipped with `continue` – but the table `m_vVertexCornerMap` still held the list of its edges for it (`SmTArray<SmEdgeuse*>*`). `GetFilletCornerOfVertex` later returned this list as `SmFilletCorner*`; `CollectTangentEdgeSequence` called virtual methods on it → access to address 0x2f08 (gdb with debug objects, valgrind: \"not stack'd, malloc'd or free'd\"). In F9a the missing corner type is the tangential contact boss/wall, in F9b the nearly tangential circular edge pin/sweep.",
    fix: "**smlib/patches/0002-fillet-missing-corner.patch:** without a corner type the entry is removed and the fillet ends with `SM_ERR_INVALID_INPUT`, the edge is recorded in `SmFilletErrorInfo`. The facade translates this into \"SMLib cannot blend the corner where these edges meet (faces that touch tangentially there?)\" with the edge (red in `render --line`). In F9a the union already reports the tangential contact (see F8c): boss and wall touch along a line, there is no manifold result. The daemon fallback was no solution: the crash comes from the process itself.",
    tests: /^(f09_|fillet_at_a_nearly)/,
  },
  {
    id: "F8",
    title: "booleans with torus and rotate_extrude",
    files: ["f08a_union_torus.scad", "f08b_union_ring_verloren.scad", "f08c_differenz_torus_tangential.scad"],
    img: [["f08a", "two overlapping bodies, no intersection with the top face", "one body, the ring half inside the plate"], ["f08b", "ring missing", "ring present"], ["f08c", "volume 0 without message", "error message, plate remains"]],
    cause: "Not the torus itself but its **transformation**: the facade called `SmBrep::Transform(m, TRUE)`; with `TRUE` SMLib derives the model size from the bounding box of the *vertices* and sets all tolerances (vertices, edges, faces, and those of the global context) from it. A torus and the solid of revolution of a circle have exactly one vertex – bounding box 0, tolerance 0. With tolerance 0 the surface intersection misses every intersection curve: union = both bodies side by side (F8a, 6049.65 mm³ = sum), ring lost (F8b), intersection empty. Verified: the same torus created directly at z = 2 intersects correctly, moved (or only rotated or scaled) it does not; the geometry (B-rep stream) is identical, only the tolerances 1e-5 → 0 differ. F7 (torus as zone selects no edge) has the same cause.",
    fix: "`apply_matrix` transforms with `Transform(m, FALSE)`: rigid motions leave the tolerances unchanged; with scaling the facade sets them from the whole (loose) bounding box of the body, the context stays untouched. F8c: the torus touches top and bottom face tangentially along two circles; the result is not manifold (SMLib returns it with the correct volume 5260.36 mm³, but not closed; `measure` showed 0). The boolean check no longer accepts a non-closed result of two closed bodies as a \"last resort\" but reports \"the result is not a closed solid: the operands touch tangentially or along an edge there (let them overlap a little or keep a gap)\". If the bodies only touch (interiors do not overlap), a − b = a and a ∩ b = empty apply instead of a non-manifold body.",
    tests: /^(f08_|f07_|moved_|tangent_torus|difference_of_bodies)/,
  },
  {
    id: "F5",
    title: "chamfer via zone on the filleted box: volume 0, status ok",
    files: ["f05_fase_volumen_null.scad"],
    img: [["f05", "open shell (26 faces), volume 0, no message", "error message, body without chamfer remains"]],
    cause: "SMLib returns `SM_SUCCESS` and an open shell: the top face is missing. The reason is the geometry: at the corners the wall between outer radius 6 and the sharp inner corner is only 6 − 3√2 ≈ 1.76 mm thick; the outer chamfer takes 0.8 mm, the inner chamfer at the sharp corner 0.8·√2 ≈ 1.13 mm – the chamfers intersect, the top face falls apart. Re-measured: `chamfer(0.7)` works, `chamfer(0.75)` does not (computed limit 0.73). Chamfers of the outer and of the inner edge each work on their own.",
    fix: "The facade checks every result of a fillet/chamfer (`blend_core`): closed shell if the input was closed; volume > 0 and volume change at most as large as a fillet of this size can cause near the edges (20 r² per mm of edge length + 10 r³ per edge + 2 %). Violated → \"SMLib returned a body that is not closed (faces missing): the blends of neighbouring edges collide, size too large for the wall between them?\" with the edges. In addition: if the selection fails as a whole and splits into separate edge chains (without a shared vertex), the chains are filleted one after another (edges found again in the intermediate body via their geometry) – futile here because the chamfers really collide, but helpful where SMLib only fails on the combination.",
    tests: /^(f05_|chamfer_result_must)/,
  },
  {
    id: "F10",
    title: "union(fillet) of filleted shell and boss: only the boss",
    files: ["f10_union_fillet_schale_verloren.scad"],
    img: [["f10", "only boss with fillet, volume 0", "error message, union without fillet remains"]],
    cause: "The union itself is correct (26,043 mm³). The concave fillet r 1 around the boss (7.5 mm from the inner corner) runs onto the bottom fillet of the shell (inner radius 1.5 between x = 2 and 3.5); SMLib then returns an open body consisting of boss and fillet.",
    fix: "The plausibility check of the fillet (F5) detects the open shell: error with the edges, the union without the concave fillet remains (instead of volume 0 without message). A correctly computed fillet across two faces remains open (SMLib).",
    tests: /^f10_/,
  },
  {
    id: "F1",
    title: "sweep with symmetric spline profile \"not centred\"",
    files: ["f01_sweep_spline_profil.scad"],
    img: [["f01", "rejected (no body)", "sweep of the ellipse"]],
    cause: "SMLib (`CreateCurveSweep`, `bMoveProfileCenterToPath`) places the center of the bounding box of the profile curves on the path. `CalculateBoundingBox` of a B-spline curve is the bounding box of its *control points*: for the closed interpolating spline through (5,0) (0,3) (−5,0) (0,−3) x −15…5, although the curve itself is symmetric (sampled ±5, ±3.08). The facade rejected every such \"off-center\" profile on a curved path.",
    fix: "**smlib/patches/0003-sweep-profile-reference-point.patch:** `SmSweepOptions` gets `bUseProfileRefPoint`/`sProfileRefPoint`; the facade specifies the origin of the profile frame (path start). Any profile, including deliberately off-center ones (circle at (0, 4)), thus follows the path with its origin; the shift after the sweep on straight paths is no longer needed. Only the mitered variant for planar polyline paths lacks the option (the message names the workaround: spline path).",
    tests: /^(f01_|sweep_of_off_centre)/,
  },
  {
    id: "F2",
    title: "cylinder ∪ curved sweep: NULL_POINTER, duplicate diagnostic",
    files: ["f02_union_zylinder_sweep.scad"],
    cause: "The sweep is valid (closed, no overlapping faces) and unites with cuboids, even when they cut the end cap. With the cylinder (also as NURBS, also rotated) SMLib's surface intersection fails near the tilted end cap (cap x 16.3…21.7 intersects r = 20). The facade tries orderings and offsets of 1e-6…1e-4 – without success. The duplicate message came from the engine: the failed union returns its operands, the root unites them again and fails a second time.",
    fix: "Engine: pieces from a failed boolean operation carry a marker (`NodeOutput::unmerged`); if the union in the parent node fails on them again, this is not reported a second time. The kernel bug itself remains open.",
    tests: /^f02_/,
  },
  {
    id: "F3, F4",
    title: "sweep of a 16-gon \"not a closed solid\"; union(fillet) on a 32-gon sweep",
    files: ["f03_sweep_polygon16.scad", "f04_union_fillet_sweep_polygon.scad"],
    cause: "SMLib builds one side face per polygon side; between some neighbors, twin edges at distance 0 stay unstitched (1 face each). `StitchFaces`/`SmApiStitchAdvanced` with tolerance 1e-5…1e-3 do not stitch them either (0 stitched). On other paths 12- or 32-gons are affected. F4: the fillet of the 32 facet edges at the circular edge fails (`SM_ERR`).",
    fix: "Message with cause: \"SMLib left 3 seam(s) between the side faces of the polygon profile open (SMLib limitation on curved paths; circle() or a spline() profile work)\". A fix in SMLib (construction of the side faces in the sweep) remains open.",
    tests: /^f03_/,
  },
  {
    id: "F13",
    title: "message for shell of a body with fillet ≤ wall thickness",
    files: ["f13_shell_verrundung.scad"],
    cause: "The inner surface of a convex fillet with radius r < t degenerates when offset by t; SMLib only reports SM_ERR, the fallback via booleans fails as well.",
    fix: "If both paths fail, the facade determines the smallest convex radius of curvature of the faces that are not opened (normal differences on a 7×7 grid per face) and names it: \"wall thickness 2 is not smaller than the radius of a convex rounding of the body (about 2): the inner surface degenerates there (rounding radius must be > wall thickness)\".",
    tests: /^f13_/,
  },
  {
    id: "F6, F11, F12, F15",
    title: "kernel bugs with message, without fix",
    files: ["f06_fase_konkave_ecke.scad", "f11_stift_deckungsgleich.scad", "f12_shell_loft_quadrat_kreis.scad", "f15_knauf_kehle.scad"],
    cause: "F6: SMLib has no working corner type for the corner \"convex edge ends at a short concave edge\" (SM_ERR; the chain is connected, filleting one after another does not help). F11: coincident cylinder surfaces (pin 3.0 in bore 3.0); offsets of 1e-6…1e-4 of the model size change nothing. F12: `SmApiShellBrepFull` of the loft square → circle (NULL_POINTER in SmMerge) and the fallback via booleans fail. F15: fillet sweep d 6 / sphere d 9 (SM_ERR) – with d 10 it works.",
    fix: "No fix within the scope of this report; the messages mark the edges (F6, F15) or name the operation. Workarounds: zones on grid lines instead of on the plane (F6), 0.05 mm clearance (F11), difference of two lofts (F12), sphere d 10 (F15).",
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
<dl class="f"><dt>Examples</dt><dd>${f.files.map((x) => `<code>tests/alltag/fehler/${esc(x)}</code>`).join(", ")}</dd>
<dt>Cause</dt><dd>${md(f.cause)}</dd>
<dt>Fix</dt><dd>${md(f.fix)}</dd>
<dt>Evidence</dt><dd>${f.files.map((x) => `<code>${esc(x.replace(/\.scad$/, ""))}</code>: before ${cell(result(x, "vorher"))} → after ${cell(result(x, "nachher"))}`).join("<br>")}<br>Tests: ${testsOf(f.tests)}</dd></dl>
${(f.img ?? []).map(([n, b, a]) => pair(n, b, a)).join("\n")}
</section>`,
  )
  .join("\n");

const html = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Test report 13 – geometry fixes</title>
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
<h1>Test report 13 – geometry fixes</h1>
<div class="meta">Geometry and kernel bugs from the everyday models (<code>tests/models/alltag/README.md</code>, <code>tests/alltag/fehler/</code>) · SMLib facade, SMLib patches 0002/0003, cad-engine, cad-render, cad-cli · ${esc(date)} · Branch ${esc(branch)} · Commit ${esc(commit)} · ${os.cpus().length} cores</div>

<div class="summary">
  <div>cargo test --workspace<b class="${cargoSum && cargoSum.failed === 0 ? "ok" : "bad"}">${cargoSum ? `${cargoSum.ok} / ${cargoSum.ok + cargoSum.failed}` : "–"}</b>of which ${newTests.length} new regression tests</div>
  <div>clippy · fmt<b class="${clippyOk && fmtOk ? "ok" : "bad"}">${clippy == null ? "–" : clippyOk ? "clean" : "finding"} · ${fmt == null ? "–" : fmtOk ? "clean" : "diff"}</b>-D warnings</div>
  <div>Wasm kernel (Node)<b class="${nodeOk ? "ok" : "bad"}">${node == null ? "–" : nodeOk ? "passed" : "error"}</b>facade rebuilt</div>
  <div>OpenSCAD · STEP<b class="ok">${esc(compareSum ?? "–")} · ${esc(stepSum ?? "–")}</b>compare.py, verify.py</div>
  <div>Everyday models<b class="${/ 0 (mit Problemen|with problems)/.test(alltagLine) ? "ok" : "bad"}">${esc(alltagLine.split(",").slice(0, 2).join(","))}</b>volumes unchanged</div>
  <div>Bug examples<b>${esc(fehlerLine)}</b>against their expected state</div>
  <div>Key rack<b class="ok">${zv && zn ? `${num(zv.evaluate / 1000, 1)} → ${num(zn.evaluate / 1000, 1)} s` : "–"}</b>evaluation per call</div>
</div>

<h2>1 Overview</h2>
<p>The everyday models found one crash, four silently wrong results and a number of kernel bugs. Each minimal example in <code>tests/alltag/fehler/</code> now carries its expected state in its header (<code>// SOLL: fehlerfrei</code> = error-free, <code>SOLL-VOLUMEN</code> = expected volume, <code>SOLL-FEHLER: Text</code> = expected error – where there is no correct result, a clear message is the goal); <code>tests/alltag/check.sh --fehler</code> reports "fixed" or "open" per example. The table shows both states (before: state before the fixes, commit <code>cba3ab4</code>; after: this commit), <code>cad measure --volume --json</code>.</p>
<table><tr><th>Example</th><th>Expected</th><th>before</th><th>after</th><th>check.sh</th></tr>
${overviewRows}
</table>
<p><b>Nothing silently wrong any more:</b> of the findings with a wrong result and no message (F5, F8a–c, F10), F8a/F8b are now computed correctly, F5, F8c and F10 are reported (F5 and F8c have no correct result: colliding chamfers, tangential contact). No example crashes any more.</p>

<h2>2 Fixes in the facade and in SMLib</h2>
<table><tr><th>Location</th><th>Change</th><th>Findings</th></tr>
<tr><td><code>smlib/patches/0002-fillet-missing-corner.patch</code></td><td>SmFilletExecutive::CreateFilletCorners: vertex without corner type → error instead of type confusion (crash)</td><td>F9</td></tr>
<tr><td><code>smlib/patches/0003-sweep-profile-reference-point.patch</code></td><td>SmSweepOptions::bUseProfileRefPoint/sProfileRefPoint: specify the reference point of the profile for curve sweeps</td><td>F1</td></tr>
<tr><td><code>apply_matrix</code></td><td>Transform(m, FALSE): rigid motions keep the tolerances, scaling sets them from the whole bounding box; context untouched</td><td>F7, F8a, F8b</td></tr>
<tr><td><code>checked_boolean</code></td><td>non-closed result of two closed bodies or impossible volume → error instead of "last resort"; merely touching bodies: a − b = a, a ∩ b = empty</td><td>F8c, F9a</td></tr>
<tr><td><code>blend_core</code>, <code>do_fillet_like</code></td><td>result of a fillet/chamfer: closed, volume plausible; separate edge chains one after another; message for a missing corner type</td><td>F5, F10, F9b</td></tr>
<tr><td><code>sc_sweep</code></td><td>profile origin as reference point (0003); open side seams of polygon profiles reported with cause</td><td>F1, F3</td></tr>
<tr><td><code>sc_shell</code></td><td>message with the smallest convex radius ≤ wall thickness</td><td>F13</td></tr>
<tr><td><code>sc_mass_props</code></td><td>volume with relative accuracy 1e-6 instead of 1e-8 (error ≤ 5e-6 measured)</td><td>run time</td></tr>
</table>
<p class="small">The patches are in <code>smlib/patches/</code>; <code>smlib/fetch.sh</code> applies them in order. Native libraries: <code>smlib/build-native.sh</code>; the Wasm kernel is rebuilt (<code>smlib/build-wasm.sh</code>) and passes <code>node crates/kernel-smlib/js/test-node.mjs</code>.</p>

<h2>3 Findings in detail</h2>
${findingHtml}

<h2>4 Display and CLI (S1–S4)</h2>
<h3>S1 – profile grid with rotate_extrude</h3>
<p><b>Cause:</b> a 2D grid (x = radius, y = height) is drawn in the coordinate system of its module, i.e. in the xy plane; after <code>rotate_extrude</code> the profile lies in the xz plane. <b>Fix (CLI):</b> <code>cad render --show grids</code> checks the references of each grid (<code>Scene.refs</code>): if all lie in 2D nodes below the same <code>rotate_extrude</code>, the grid is drawn in its profile plane (frame RE · Rx(90°) · RE⁻¹ · N · frame_module) and a note is printed. Grids with references outside (or via variables) stay as they are.</p>
${pair("s1", "grid p in the xy plane, 95 mm behind the cup", "grid p in the profile plane: p.unten at r 36, p.oben at r 41")}
<p><b>UI (not changed, <code>web/</code>):</b> the 3D view draws grids from <code>Scene.grids[].frame</code>. For the same fix it would need either the rule above (references from <code>Scene.refs</code> → 2D nodes → <code>op_info.kind == "rotate_extrude"</code>) or, better, a frame that the scene supplies per grid (<code>Grid.display_frame</code>, computed in <code>cad-session</code> by the same rule). For dragging/snapping to grid points the handles would have to be mapped into the profile plane as well.</p>
<h3>S3 – outline and visibility</h3>
<p><code>cad outline</code> listed instances named in the body of a module (<code>dom[0]</code> … in <code>pi_platte</code>) under the file's "named instances" and their anchors as file anchors. Now: a separate section "named instances in modules (visible only there)" grouped by module; in the JSON <code>scope: "file" | "module"</code> and <code>in_module</code>; anchors only of the file instances. That <code>measure --between dom[0].oben …</code> still works in file context is language semantics (cad-eval) and remains a proposal: resolve such names only via the instance chain (<code>platte.dom[0].oben</code>) or with a warning.</p>
<h3>S4 – section caps and grid labels</h3>
<p>A model made of parts was drawn in section as one mesh of the root: the caps of touching parts merged. Now, as soon as the parts make up the whole model, it is drawn per part: its own cap with alternating hatching and its own section contour. Grid labels: rows of indexed planes (<code>rippe[0]</code> … <code>rippe[7]</code>) get one label <code>g.rippe[0..7]</code>; 3 px remain between labels (before, they could touch directly).</p>
${pair("s4", "lid and lower part one cap", "lid separated (contour, opposite hatching)")}
${pair("s4b", "one label per rib", "one label per row")}
<h3>S2 – plane names as values, anchor lists (assessment only)</h3>
<ul>
<li><b>Plane names as values</b> (<code>g[i % 2 == 0 ? loch_l : loch_r, …]</code>): the names in <code>g[…]</code> are selectors today, not expressions; the workaround via numbers (<code>g.loch_l</code>) loses the grid binding (snap, dragging). <b>Proposal:</b> a value type "plane" (<code>g.loch_l</code> still yields the number, but <code>plane(g.loch_l)</code> or a selector expression <code>g[sel(i % 2 == 0 ? "loch_l" : "loch_r"), …]</code>) that <code>g[…]</code> accepts; this keeps the origin for handles. Effort medium (cad-eval: selector evaluation, drag write-back only for literal selectors).</li>
<li><b>Anchor lists</b> (<code>anchor loch[i] = …</code> in a loop): fits the existing indexing of instances (<code>k[i]</code>) and planes (<code>rippe[i]</code>). <b>Proposal:</b> the same rule as for planes (name + index, outside <code>k.loch[2]</code>), snap targets per element. Effort low to medium.</li>
<li><b>Grid-local coordinates</b> for profiles in rotated planes (S1): a grid with its own frame (<code>grid p in xz { … }</code>) would also solve S1 in the language.</li>
</ul>

<h2>5 Run time</h2>
<p><b>Finding:</b> key rack 4.0 s kernel, but ≈ 16 s per evaluation. <b>Measurement:</b> stack samples with gdb (every ~0.8 s) during <code>cad check schluesselbrett.scad</code>:</p>
${pre(T("profil.txt"))}
<p><b>Cause:</b> after the kernel run the session computes the volume of each part (part list of the scene, <code>part_volumes</code>); after the mass properties (1e-4), <code>sc_mass_props</code> additionally called <code>SmApiBrepComputeVolume(…, 1e-8)</code>. The accuracy 1e-8 costs about 11 s for the swept hooks via adaptive Runge–Kutta integration per face (<code>SmFace::ComputePreciseProperties</code>). Measured relative volume error: 1e-6 → ≤ 4.4e-6, 1e-7 → ≤ 1.1e-7, 1e-8 → ≤ 1.5e-8 (sphere, torus, cone). <b>Fix:</b> 1e-6 (part volumes are displayed to 0.1 mm³; checks use 0.1 %).</p>
<table><tr><th>cad render schluesselbrett.scad (median of ${zn?.n ?? "–"})</th><th>before</th><th>after</th></tr>
<tr><td>Evaluation (evaluate_ms, without image)</td><td class="n">${zv ? num(zv.evaluate, 0) : "–"} ms</td><td class="n">${zn ? num(zn.evaluate, 0) : "–"} ms</td></tr>
<tr><td>of which kernel run (kernel_ms)</td><td class="n">${zv ? num(zv.kernel, 0) : "–"} ms</td><td class="n">${zn ? num(zn.kernel, 0) : "–"} ms</td></tr>
<tr><td>Image (render_ms)</td><td class="n">${zv ? num(zv.render, 0) : "–"} ms</td><td class="n">${zn ? num(zn.render, 0) : "–"} ms</td></tr></table>
<p class="small">The remainder between kernel and evaluation is now mainly the part volumes (1e-6) and tessellation/scene. A further step would be to compute the volumes only when they are needed (<code>outline</code>, <code>measure</code>, UI part list), or to take them from the mass properties (1e-4).</p>

<h2>6 Open</h2>
<ul>
<li>F2 (cylinder ∪ curved sweep), F3/F4 (sweeps of polygon profiles), F6 (corner type convex/concave), F11 (coincident cylinders), F12 (shell of a loft), F15 (fillet sweep/sphere): SMLib bugs; messages and workarounds as above.</li>
<li>F10: the fillet across two faces is reported, not computed.</li>
<li>F5, F8c, F9a have no correct result (collision, tangential contact); an automatic "overlap a little" would be possible, but changes the geometry without being asked to.</li>
<li>S1 in the UI (proposal above), S2 and the visibility of <code>dom[0].oben</code> in file context (language semantics).</li>
<li>The fillet check relies on coarse volumes (1e-2); errors that change the volume only slightly go undetected.</li>
</ul>

<h2>7 Test runs</h2>
<h3>New regression tests</h3>
<p>${newTests.map((t) => `<code>${esc(t.name)}</code> ${t.res === "ok" ? '<span class="ok">ok</span>' : '<span class="bad">FAILED</span>'}`).join(", ") || "–"}</p>
<p class="small">Kernel: <code>crates/kernel-smlib/tests/alltag.rs</code>; engine: <code>crates/cad-engine/tests/alltag_fehler.rs</code> (reads the minimal examples); CLI: <code>crates/cad-cli/tests/geometrie.rs</code>.</p>
<h3>Bug examples (check.sh --fehler)</h3>
${pre(fehlerTxt.split("\n").filter((l) => /^\s+(behoben|offen|fixed|open)|Fehlerbeispiele|[Bb]ug examples/.test(l)).join("\n") || "(missing)")}
<h3>Everyday models, OpenSCAD comparison, STEP</h3>
${pre([alltagLine, compareSum ? `compare.py: ${compareSum} files match` : "compare.py: –", stepSum ? `verify.py: ${stepSum}` : "verify.py: –"].join("\n"))}
<h3>cargo test, clippy, fmt, Wasm</h3>
${pre([cargoSum ? `cargo test --workspace: ${cargoSum.ok} passed, ${cargoSum.failed} failed` : "cargo test: –", `clippy -D warnings: ${clippy == null ? "–" : clippyOk ? "clean" : "finding"}`, `fmt --check: ${fmt == null ? "–" : fmtOk ? "clean" : "difference"}`, `node test-node.mjs: ${node == null ? "–" : nodeOk ? "all node tests passed" : "error"}`].join("\n"))}
</main>
</body>
</html>
`;
writeFileSync(join(out, "report.html"), html);
console.log(`${join(out, "report.html")}: ${html.length} characters, ${examples.length} examples, ${newTests.length} new tests`);
