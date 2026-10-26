#!/usr/bin/env node
// Test report 21 (Plan 21 and user feedback): offset/hull/minkowski (phase A), STL import (B),
// editor without inline sliders, collision asserts with export abort, UI and Wasm build (C),
// user feedback – kernel errors with a location, -D without module, STEP with separate bodies,
// slot(), JSON schema 2, -m 'module(args)' (D), rename cad → mcad with .mcad (E); test runs at
// the final commit, limits and open points.
// Writes docs/reports/21-hull-stl-asserts/report.html (images in img/).
//
// Inputs (gitignored, reports/tmp/report21/):
//   phase-a … phase-e/                   material of the phases (offset spike, STEP body counts,
//                                        inventory of the rename, slot areas, Wasm sizes)
//   data/a … data/e/                     python3 tools/report/report21-data.py --vorher <cad before phase D>
//   phase-c/img, phase-d/img             cd web && pnpm e2e (asserts.spec.ts, phase-d.spec.ts write there)
//   ../report3/img/b12-vorher.jpg        cd web && pnpm e2e (ui-b.spec.ts, B12: bool switch only)
//   final/times.txt                      "<step> <exit code> <seconds>" per test run
//   final/fmt.txt, cargo.txt, clippy.txt, clippy-wasm.txt, solidcore.txt
//   final/vitest.txt, typecheck.txt, playwright.txt, playwright.json (JSON reporter)
//   final/tauri/tauri-e2e.json, tauri/img node web/e2e-tauri/desktop.e2e.mjs --bin target/release/mcad --out reports/tmp/report21/final/tauri
//   final/compat.txt, step.txt, step-bodies.txt, alltag.txt
//
//   node tools/report/report21.mjs
//   node tools/report/print-pdf.mjs docs/reports/21-hull-stl-asserts/report.html docs/reports/21-hull-stl-asserts/report.pdf "Test report 21: hull/minkowski, STL import, asserts, user feedback, mcad"

import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";
import os from "node:os";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "../..");
const input = join(root, "reports/tmp/report21");
const data = join(input, "data");
const fin = join(input, "final");
const out = join(root, "docs/reports/21-hull-stl-asserts");
const imgDir = join(out, "img");
rmSync(imgDir, { recursive: true, force: true });
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
const fmt = (x, d = 0) => (x === null || x === undefined || Number.isNaN(Number(x)) ? "–" : Number(x).toLocaleString("en-US", { minimumFractionDigits: d, maximumFractionDigits: d }));
const sci = (x) => (x === null || x === undefined ? "–" : x === 0 ? "0" : `${x > 0 ? "+" : "−"}${Math.abs(x).toExponential(1).replace("e-", "·10⁻").replace("e+", "·10^")}`.replace(/10⁻(\d+)/, (_, e) => `10⁻${[...e].map((c) => "⁰¹²³⁴⁵⁶⁷⁸⁹"[+c]).join("")}`));
const md = (t) => esc(t).replace(/`([^`]+)`/g, "<code>$1</code>");
const pre = (t) => `<pre>${esc(String(t).trimEnd())}</pre>`;
const term = (t) => {
  const x = String(t ?? "(missing)").trimEnd();
  return `<pre class="term${x.split("\n").length > 18 ? " long" : ""}">${esc(x)}</pre>`;
};
/** Long decimals in check texts of the E2E runs (179.99999999999994) rounded. */
const roundLong = (t) => String(t).replace(/\d+\.\d{6,}/g, (m) => String(+(+m).toFixed(4)));

/** Copies an image as JPEG (max. width) into img/ and returns its path, or null. */
function img(srcPath, dst, width = 1100, trim = false) {
  if (!existsSync(srcPath)) return null;
  try {
    execFileSync("convert", [srcPath, ...(trim ? ["-trim", "+repage", "-bordercolor", "#f4f5f7", "-border", "24"] : []), "-resize", `${width}x>`, "-quality", "74", join(imgDir, dst)]);
    return `img/${dst}`;
  } catch {
    const png = dst.replace(/\.jpg$/, ".png");
    copyFileSync(srcPath, join(imgDir, png));
    return `img/${png}`;
  }
}
const fig = (s, cap, cls = "") => (s ? `<figure class="${cls}"><img src="${s}" alt=""><figcaption>${md(cap)}</figcaption></figure>` : `<p class="small">(image missing: ${esc(cap)})</p>`);
const dimg = (part, n, w, trim) => img(join(data, part, n), `${part}-${n.replace(/\.png$/, ".jpg")}`, w, trim);
const uiC = (n, w = 900) => img(join(input, "phase-c/img", `${n}.png`), `ui-${n}.jpg`, w);
const uiD = (n, w = 900) => img(join(input, "phase-d/img", `${n}.png`), `ui-${n}.jpg`, w);
const tauriImg = (n, w = 1100) => img(join(fin, "tauri/img", n), `tauri-${n.replace(/\.png$/, ".jpg")}`, w);

// ------------------------------------------------------------------------------- data

const hull = readJson(join(data, "a/hull.json")) ?? [];
const bench = readJson(join(data, "b/bench.json")) ?? { spheres: [], real: [] };
const asserts = readJson(join(data, "c/asserts.json")) ?? [];
const dTxt = (n) => readText(join(data, "d", `${n}.txt`));
const d6 = { vorher: readJson(join(data, "d/d6-vorher.json")), nachher: readJson(join(data, "d/d6-nachher.json")) };
const times = Object.fromEntries((readText(join(fin, "times.txt")) ?? "").trim().split("\n").filter(Boolean).map((l) => l.split(" ")).map(([k, rc, s]) => [k, { rc: +rc, s: +s }]));
const dur = (k) => (times[k] ? (times[k].s >= 90 ? `${fmt(times[k].s / 60, 1)} min` : `${times[k].s} s`) : "–");
const rcOk = (k) => times[k]?.rc === 0;

const cargo = readText(join(fin, "cargo.txt"));
const cargoSum = cargo ? [...cargo.matchAll(/test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored/g)].reduce((a, m) => ({ ok: a.ok + +m[1], failed: a.failed + +m[2], ignored: a.ignored + +m[3], suites: a.suites + 1 }), { ok: 0, failed: 0, ignored: 0, suites: 0 }) : null;
const cargoFailed = cargo ? [...new Set([...cargo.matchAll(/^test (\S+) \.\.\. FAILED$/gm)].map((m) => m[1]))] : [];
const vitest = readText(join(fin, "vitest.txt"));
const vitestSum = vitest ? { ok: +(/Tests\s+(?:\d+ failed \| )?(\d+) passed/.exec(vitest)?.[1] ?? 0), failed: +(/Tests\s+(\d+) failed/.exec(vitest)?.[1] ?? 0) } : null;
const typecheck = readText(join(fin, "typecheck.txt"));
const typecheckOk = typecheck !== null && !/error TS/.test(typecheck) && rcOk("typecheck");
const pwJson = readJson(join(fin, "playwright.json"));
const pwStats = pwJson?.stats ?? null;
const pwSpecs = (() => {
  const res = [];
  const walk = (s, file) => {
    for (const sp of s.specs ?? []) for (const t of sp.tests ?? []) res.push({ file: file ?? s.file, title: sp.title, status: t.status, ms: (t.results ?? []).reduce((a, r) => a + (r.duration ?? 0), 0), errors: (t.results ?? []).flatMap((r) => (r.errors ?? []).map((e) => e.message ?? "")) });
    for (const c of s.suites ?? []) walk(c, file ?? s.file);
  };
  for (const s of pwJson?.suites ?? []) walk(s, s.file);
  return res;
})();
const pwBad = pwSpecs.filter((t) => t.status !== "expected" && t.status !== "skipped");
const tauri = readJson(join(fin, "tauri/tauri-e2e.json"));
const tauriOk = tauri ? tauri.results.filter((r) => r.ok).length : null;
const compat = readText(join(fin, "compat.txt"));
const compatSum = compat ? /(\d+)\s*\/\s*(\d+)/.exec(compat.trim().split("\n").reverse().find((l) => /\d+\s*\/\s*\d+/.test(l)) ?? "") : null;
const stepTxt = readText(join(fin, "step.txt"));
const stepSum = stepTxt ? /(\d+)\/(\d+) passed/.exec(stepTxt) : null;
const stepBodies = readText(join(fin, "step-bodies.txt"));
const stepBodiesSum = stepBodies ? /(\d+)\/(\d+) passed/.exec(stepBodies) : null;
const alltag = readText(join(fin, "alltag.txt"));
const alltagLast = alltag ? alltag.trim().split("\n").slice(-1)[0] : null;
const clippyOk = rcOk("clippy") && rcOk("clippy-wasm");
const fmtOk = rcOk("fmt") && (readText(join(fin, "fmt.txt")) ?? "x").trim() === "";

const date = "2026-10-25";
const commit = gitOut("rev-parse", "--short", "HEAD");
const branch = gitOut("rev-parse", "--abbrev-ref", "HEAD");
const commits = gitOut("log", "--format=%h\t%s", "852fbc1..HEAD").split("\n").filter(Boolean).map((l) => l.split("\t")).reverse();
const rustc = (() => {
  try {
    return execFileSync("rustc", ["--version"], { encoding: "utf8" }).trim();
  } catch {
    return "–";
  }
})();

// Phase of each commit (by hash prefix of the first commit of a phase).
const PHASES = [
  ["83ee5b3", "A"],
  ["b2b9768", "B"],
  ["2d3451e", "Editor"],
  ["c585002", "C"],
  ["a86a21b", "D"],
  ["4db585b", "E"],
  ["0ea0d78", "Report"],
];
// English titles of the commits (the git subjects of these commits are German); others: git subject.
const COMMIT_TITLES = {
  "83ee5b3": "Kernel: outward offset (Minkowski sum with a sphere), exact or error",
  "71e5bce": "Kernel geometry: exact 2D hull from lines and arcs, convex hull in 3D",
  b97c365: "hull() and minkowski(): exact special cases in 2D and 3D, otherwise a warning with the reason",
  a70dc9a: "minkowski() with one child is the child, empty hull()/minkowski() without warning",
  b2b9768: "Kernel: triangle meshes without B-rep, STL parser and mesh preparation (cad-stl)",
  "8847eb2": "Convex hull in 3D: closed also for nearly coplanar points",
  b6e8906: 'import("x.stl"): STL as B-rep or as plain mesh (Plan 21, phase B)',
  f017264: "UI: STL as data file (upload, examples, references in the text)",
  "2d3451e": "Editor: no more inline sliders for number literals",
  c585002: "Collision asserts: assert_keepout/disjoint/intersect/touches, cad export aborts (Plan 21, phase C)",
  "0e20aad": "UI: keep-out volume hatched and hideable, assert results in the view, export confirmation",
  bf44805: "Tauri E2E T19: keep-out volume, framing a violation, export confirmation in the desktop app",
  "235d4af": "Asserts: contact exactness per contact kind, CLI shows intended intersections in green",
  a86a21b: "JSON schema 2: selected_edges as polylines everywhere, uniform fields in outline",
  cbf2963: "-D without module context: set top-level assignments (like openscad -D)",
  "1d530de": "-m 'module(arguments)': module context with arguments as in the code",
  "91f8ef6": "slot(p1, p2, r): slot as exact 2D primitive",
  "8ac92d6": "STEP export: one named body per part and object, --merge for a single one",
  "3164682": "Kernel errors with location: tangentially touching operands with line and position",
  "4db585b": "Rename cad → mcad: binary, program name, file extension .mcad (version 0.3.0)",
  "004d30f": "Tauri E2E: binary mcad, T20 window title, app ID and .scad project",
  "0ea0d78": "verify.py: removed extra parenthesis left over from the rename",
};
let phase = "A";
const commitRows = commits
  .map(([h, s]) => {
    for (const [p, n] of PHASES) if (h.startsWith(p)) phase = n;
    const title = Object.entries(COMMIT_TITLES).find(([k]) => h.startsWith(k) || k.startsWith(h))?.[1] ?? s;
    return `<tr><td>${esc(phase)}</td><td><code>${esc(h)}</code></td><td>${md(title)}</td></tr>`;
  })
  .join("");

// ---- phase A: offset spike (raw lines of the spike in phase-a/offset-spike.txt)
const SPIKE_BODIES = [
  ["box", "Cuboid 10×20×30"],
  ["rotated box", "Cuboid rotated"],
  ["cylinder", "Cylinder r 5, h 10"],
  ["sphere", "Sphere r 5"],
  ["L", "L profile (concave edge)"],
  ["filleted box", "filleted cuboid (r 1)"],
  ["tetra", "Tetrahedron"],
  ["wedge", "45° wedge (extrusion)"],
  ["wedge polyhedron", "45° wedge (polyhedron)"],
];
const spike = (() => {
  const t = readText(join(input, "phase-a/offset-spike.txt")) ?? "";
  const m = {};
  for (const l of t.split("\n")) {
    const r = /^SPIKE (.+?) r=2 (mode=\d|ext\+fillet): (.*)$/.exec(l);
    if (r) m[`${r[1]}|${r[2]}`] = r[3];
  }
  return m;
})();
const spikeCell = (s) => {
  if (!s) return "–";
  if (s.startsWith("FAIL")) {
    const why = /self-overlapping/.test(s)
      ? "self-overlapping"
      : /closed solid/.test(s)
        ? "not closed"
        : /trim curves up to ([\d.]+)/.test(s)
          ? `not exact: trim curves up to ${fmt(+/trim curves up to ([\d.]+)/.exec(s)[1], 2)} mm off the edges`
          : /lies up to ([\d.]+)/.test(s)
            ? `not exact: boundary up to ${fmt(+/lies up to ([\d.]+)/.exec(s)[1], 2)} mm off the offset distance`
            : /fillet/.test(s)
              ? "fillet fails"
              : s.slice(5, 60);
    return `<span class="bad">error</span> (${esc(why)})`;
  }
  const rel = /rel=([+-][\d.e-]+)/.exec(s)?.[1];
  const ms = /([\d.]+) ms/.exec(s)?.[1];
  return `<span class="ok">ok</span>${rel ? `, ${sci(+rel)}` : ""}${ms ? `, ${fmt(+ms)} ms` : ""}`;
};
const spikeFaces = (s) => {
  const f = /faces=(\{[^}]*\})/.exec(s ?? "")?.[1];
  if (!f) return "–";
  const o = JSON.parse(f);
  const names = { plane: "plane", cylinder: "cylinder", sphere: "sphere", revolution: "revolution", nurbs: "NURBS" };
  return Object.entries(o)
    .map(([k, v]) => `${v} ${names[k] ?? k}`)
    .join(", ");
};
const spikeTable = `<table><tr><th>Body (r = 2)</th><th>Rounding without self-intersection (mode 0)</th><th>Rounding with self-intersection (mode 2, chosen)</th><th>Substitute: face offset + fillet</th><th>Faces (mode 2)</th></tr>${SPIKE_BODIES.map(
  ([k, n]) => `<tr><td>${esc(n)}</td><td>${spikeCell(spike[`${k}|mode=0`])}</td><td>${spikeCell(spike[`${k}|mode=2`])}</td><td>${spikeCell(spike[`${k}|ext+fillet`])}</td><td>${esc(spikeFaces(spike[`${k}|mode=2`]))}</td></tr>`,
).join("")}</table>`;

// ---- phase A: hull/minkowski cases
const hullOk = hull.filter((r) => r.rel !== null && Math.abs(r.rel) <= 1e-5).length;
const hullWithFormula = hull.filter((r) => r.want !== null).length;
const hullWarn = hull.filter((r) => r.warnings.length).length;
const hullTable = `<table class="narrow"><colgroup><col style="width:22%"><col style="width:8%"><col style="width:15%"><col style="width:9%"><col style="width:9%"><col style="width:8%"><col style="width:7%"><col style="width:22%"></colgroup><tr><th>Case</th><th>Operation</th><th>Formula</th><th class="n">expected</th><th class="n">measured</th><th class="n">rel. dev.</th><th class="n">call</th><th>Special case or warning</th></tr>${hull
  .map((r) => {
    const ok = r.rel !== null && Math.abs(r.rel) <= 1e-5;
    const note = r.hint ? esc(r.hint.replace(/^(exact|exakt):\s*/, "")) : r.warnings.length ? `<span class="warn">Warning:</span> ${esc(r.warnings[0].replace(/^(hull|minkowski): /, ""))}` : "";
    return `<tr><td>${esc(r.what)}</td><td><code>${esc(r.op)}</code></td><td>${esc(r.formula)}</td><td class="n">${r.want === null ? "–" : fmt(r.want, 4)}</td><td class="n">${fmt(r.got, 4)}</td><td class="n ${r.rel === null ? "" : ok ? "ok" : "bad"}">${sci(r.rel)}</td><td class="n">${fmt(r.ms)} ms</td><td class="small2">${note}</td></tr>`;
  })
  .join("")}</table>`;
const HULL_PICS = ["2d_riemen", "2d_mink_L", "2d_text_G", "3d_kugeln_r5_r2", "3d_kapsel_gedreht", "3d_quader_zylinder", "3d_kugeln_quader", "3d_mink_wuerfel", "3d_mink_extrude_zyl", "3d_mink_tetraeder", "3d_kugeln_ungleich", "3d_kugeln_ebene"];
const hullGrid = `<div class="grid4">${HULL_PICS.map((n) => {
  const r = hull.find((x) => x.name === n);
  return fig(dimg("a", `${n}.png`, 480), r ? `${r.what}${r.warnings.length ? " – warning, children unchanged" : ""}` : n);
}).join("")}</div>`;

// ---- phase B: STL bench + chart
const sph = bench.spheres ?? [];
const benchTable = `<table class="narrow"><tr><th class="n">triangles</th><th class="n">faces</th><th>automatic</th><th class="n">language</th><th class="n">B-rep build</th><th class="n">B-rep + Boolean</th><th class="n">mesh: kernel / Boolean</th><th class="n">display + image</th></tr>${sph
  .map((r) => `<tr><td class="n">${fmt(r.triangles)}</td><td class="n">${fmt(r.faces)}</td><td>${r.auto === "solid" ? "B-rep" : "mesh"}</td><td class="n">${fmt(r.mesh?.eval_ms)} ms</td><td class="n">${r.solid ? `${fmt(r.solid.kernel_ms / 1000, 2)} s` : "–"}</td><td class="n">${r.solid ? `${fmt(r.solid.bool_ms / 1000, 1)} s` : "–"}</td><td class="n">${fmt(r.mesh?.kernel_ms)} / ${fmt(r.mesh?.bool_ms)} ms</td><td class="n">${fmt(r.mesh?.render_ms)} ms</td></tr>`)
  .join("")}</table>`;
const realTable = `<table><tr><th>Model (mcad export, re-imported with <code>as = "solid"</code>)</th><th class="n">triangles</th><th class="n">faces</th><th>automatic</th><th class="n">B-rep build</th><th class="n">+ Boolean (cylinder r 6)</th></tr>${(bench.real ?? [])
  .map((r) => `<tr><td>${r.model === "platte" ? "Plate 60×40×5 with 8 bores" : "Housing from <code>box.mcad</code> with fillet"}</td><td class="n">${fmt(r.triangles)}</td><td class="n">${fmt(r.faces)}</td><td>${r.auto === "solid" ? "B-rep" : "mesh"}</td><td class="n">${fmt(r.kernel_ms / 1000, 2)} s</td><td class="n">${fmt(r.bool_ms / 1000, 2)} s</td></tr>`)
  .join("")}</table>`;

/** Log-log line chart: faces vs. seconds, B-rep build, B-rep + Boolean, mesh (language + kernel). */
function stlChart() {
  const W = 680, H = 380, L = 58, R = 150, T = 18, B = 66;
  const xs = [100, 30000], ys = [0.001, 300];
  const lx = (v) => L + ((Math.log10(v) - Math.log10(xs[0])) / (Math.log10(xs[1]) - Math.log10(xs[0]))) * (W - L - R);
  const ly = (v) => T + (1 - (Math.log10(v) - Math.log10(ys[0])) / (Math.log10(ys[1]) - Math.log10(ys[0]))) * (H - T - B);
  const series = [
    { name: "B-rep + Boolean", color: "var(--s2)", pts: sph.filter((r) => r.solid).map((r) => [r.faces, r.solid.bool_ms / 1000]) },
    { name: "B-rep build", color: "var(--s1)", pts: sph.filter((r) => r.solid).map((r) => [r.faces, r.solid.kernel_ms / 1000]) },
    { name: "as mesh (language + kernel)", color: "var(--s3)", pts: sph.filter((r) => r.mesh).map((r) => [r.faces, Math.max((r.mesh.eval_ms + r.mesh.kernel_ms) / 1000, 0.001)]) },
  ];
  const grid = [];
  for (const v of [100, 1000, 10000]) grid.push(`<line x1="${lx(v)}" x2="${lx(v)}" y1="${T}" y2="${H - B}" class="g"/><text x="${lx(v)}" y="${H - B + 16}" text-anchor="middle" class="t">${fmt(v)}</text>`);
  for (const v of [300, 3000, 30000]) grid.push(`<text x="${lx(v)}" y="${H - B + 16}" text-anchor="middle" class="t">${fmt(v)}</text>`);
  for (const v of [0.001, 0.01, 0.1, 1, 10, 100]) grid.push(`<line x1="${L}" x2="${W - R}" y1="${ly(v)}" y2="${ly(v)}" class="g"/><text x="${L - 6}" y="${ly(v) + 4}" text-anchor="end" class="t">${fmt(v, v < 1 ? String(v).split(".")[1].length : 0)}</text>`);
  const limit = `<line x1="${lx(500)}" x2="${lx(500)}" y1="${T}" y2="${H - B}" class="lim"/><text x="${lx(500) + 4}" y="${T + 10}" class="t">limit 500 faces</text>`;
  const two = `<line x1="${L}" x2="${W - R}" y1="${ly(2)}" y2="${ly(2)}" class="lim2"/><text x="${L + 4}" y="${ly(2) - 4}" class="t">2 s</text>`;
  const lines = series
    .map((s) => {
      const d = s.pts.map(([x, y], i) => `${i ? "L" : "M"}${lx(x).toFixed(1)},${ly(y).toFixed(1)}`).join(" ");
      const dots = s.pts.map(([x, y]) => `<circle cx="${lx(x).toFixed(1)}" cy="${ly(y).toFixed(1)}" r="4" fill="${s.color}" stroke="#fff" stroke-width="2"><title>${esc(s.name)}: ${fmt(x)} faces, ${fmt(y, y < 1 ? 3 : 1)} s</title></circle>`).join("");
      const [ex, ey] = s.pts[s.pts.length - 1] ?? [0, 0];
      return `<path d="${d}" fill="none" stroke="${s.color}" stroke-width="2"/>${dots}<text x="${lx(ex) + 8}" y="${ly(ey) + 4}" class="lbl">${esc(s.name)}</text>`;
    })
    .join("");
  const legend = series.map((s, i) => `<g transform="translate(${L + i * 190},${H - 6})"><line x1="0" x2="18" y1="-4" y2="-4" stroke="${s.color}" stroke-width="2"/><circle cx="9" cy="-4" r="4" fill="${s.color}" stroke="#fff" stroke-width="2"/><text x="24" y="0" class="t">${esc(s.name)}</text></g>`).join("");
  return `<figure class="chart"><svg viewBox="0 0 ${W} ${H}" width="100%" role="img" aria-label="Run time of the STL import over the face count">
<style>.g{stroke:#e4e6ea;stroke-width:1}.t{font:10px system-ui,sans-serif;fill:#5d646e}.lbl{font:10.5px system-ui,sans-serif;fill:#1f2328}.lim{stroke:#1f2328;stroke-width:1;stroke-dasharray:4 3}.lim2{stroke:#9aa0a8;stroke-width:1;stroke-dasharray:2 3}</style>
${grid.join("")}<line x1="${L}" x2="${W - R}" y1="${H - B}" y2="${H - B}" stroke="#9aa0a8"/><line x1="${L}" x2="${L}" y1="${T}" y2="${H - B}" stroke="#9aa0a8"/>
${limit}${two}${lines}
<text x="${(L + W - R) / 2}" y="${H - B + 31}" text-anchor="middle" class="t">faces after merging (logarithmic)</text>
<text transform="translate(14,${(T + H - B) / 2}) rotate(-90)" text-anchor="middle" class="t">seconds (logarithmic)</text>
${legend}</svg><figcaption>Kernel time of the STL import (UV spheres r 20, median of ${bench.spheres?.[0]?.solid?.runs ?? "–"} runs, from 2500 faces 1 run) over the face count: B-rep build (<code>as = "solid"</code>), the same body with one Boolean (cylinder through both poles) and as mesh (parsing, preparation and kernel together). Above 5000 triangles measured as mesh only. Values in the table below ("language" = parsing and preparing the STL, "display" = display mesh and image).</figcaption></figure>`;
}

// ---- phase C: asserts
const assertsOk = asserts.filter((r) => r.ok).length;
const PROB = { overlap: "intersection", too_close: "too close", apart: "separated", gap: "gap", line_contact: "line contact only", point_contact: "point contact only" };
const KEY = { volume: "mm³", distance: "mm", contact_area: "mm²" };
const assertsTable = `<table class="narrow"><tr><th>Case</th><th>Setup</th><th>expected</th><th>Result</th><th class="n">Value</th><th>expected value</th><th class="n">rel. dev.</th><th class="n">check</th><th class="n">call</th></tr>${asserts
  .map((r) => {
    const res = r.problem ? PROB[r.problem] ?? r.problem : `ok${r.contact ? ` (${{ face: "face", line: "line", point: "point" }[r.contact] ?? r.contact}${r.exact ? ", exact" : ""})` : ""}`;
    const val = r.value === null || r.value === undefined ? "–" : `${fmt(r.value, Math.abs(r.value) < 1 ? 7 : 4)} ${KEY[r.key] ?? ""}`;
    return `<tr><td>${esc(r.id)}</td><td>${esc(r.title)}</td><td>${esc(r.expected ? PROB[r.expected] ?? r.expected : "ok")}</td><td class="${r.ok ? "ok" : "bad"}">${esc(res)}</td><td class="n">${val}</td><td>${esc(r.formula)}${r.want !== null && r.want !== undefined ? ` = ${fmt(r.want, 4)}` : ""}</td><td class="n">${r.rel === null || r.rel === undefined ? "–" : sci(r.rel)}</td><td class="n">${fmt(r.ms_assert, 1)} ms</td><td class="n">${fmt(r.ms_total)} ms</td></tr>`;
  })
  .join("")}</table>`;

// Wasm sizes: before phase A and after phase C from phase-c/README.md, now measured.
const wasmNow = (() => {
  const f = join(root, "web/src/wasm-pkg/cad_wasm_bg.wasm");
  const s = join(root, "web/src/wasm-pkg/smlib.wasm");
  if (!existsSync(f)) return null;
  const g = (p) => ({ raw: statSync(p).size, gz: gzipSync(readFileSync(p), { level: 9 }).length });
  return { cad: g(f), smlib: existsSync(s) ? g(s) : null };
})();
const WASM_BEFORE = { cad: { raw: 4545057, gz: 1925346 }, smlib: { raw: 5283311, gz: 1862076 } };
const WASM_C = { cad: { raw: 4887251, gz: 2060574 }, smlib: { raw: 5289122, gz: 1863942 } };
const pct = (a, b) => `${b >= a ? "+" : "−"}${fmt((Math.abs(b - a) / a) * 100, 1)} %`;
const wasmRow = (name, k) =>
  `<tr><td><code>${name}</code></td><td class="n">${fmt(WASM_BEFORE[k].raw)}</td><td class="n">${fmt(WASM_C[k].raw)}</td><td class="n">${wasmNow?.[k] ? fmt(wasmNow[k].raw) : "–"}</td><td class="n">${wasmNow?.[k] ? pct(WASM_BEFORE[k].raw, wasmNow[k].raw) : "–"}</td><td class="n">${fmt(WASM_BEFORE[k].gz)} → ${wasmNow?.[k] ? fmt(wasmNow[k].gz) : "–"}</td></tr>`;

// ---- phase D: STEP body counts (phase-d/d3_step_koerper.txt), slot areas
const stepCounts = (() => {
  const t = readText(join(input, "phase-d/d3_step_koerper.txt")) ?? "";
  return t
    .split("\n")
    .map((l) => /^(\S+\.scad)\s+(\d+)\s+(\d+)\s+(.*)$/.exec(l))
    .filter(Boolean)
    .map((m) => ({ model: m[1].replace(/^examples\//, ""), before: +m[2], after: +m[3], names: m[4] }));
})();
const stepChanged = stepCounts.filter((r) => r.before !== r.after);
// Models whose top-level objects overlap: bodies mode skips the volume (dV rel "-") with several bodies.
const stepOverlap = (stepBodies ?? "")
  .split("\n")
  .map((l) => /^(\S+)\s+ok\s+(\d+)\s+\d+\s+([\d.]+)\s+([\d.]+)\s+-\s/.exec(l))
  .filter((m) => m && +m[2] > 1)
  .map((m) => ({ model: m[1], solids: +m[2], vKernel: +m[3], vOcct: +m[4] }))
  .filter((r) => r.vOcct > r.vKernel * (1 + 1e-5));
const slotAreas = (readText(join(input, "phase-d/d5_slot_flaeche.txt")) ?? "")
  .split("\n")
  .map((l) => /^(slot\(.*\)) h=1: cad ([\d.]+) mm³, Formel .* = ([\d.]+), rel\. Abw\. ([\d.e-]+)/.exec(l))
  .filter(Boolean);

// before/after table of the user feedback
const feedback = [
  ["D1", "Kernel errors with location", "d1", "The message names both operands with line and the location; JSON `at` and `nodes`; render and UI mark the location."],
  ["D2", "`-D` without module context", "d2", "Sets top-level assignments like `openscad -D`; unknown name → warning."],
  ["D3", "STEP with separate bodies", "d3", "One named body per part or top-level object; `--merge` writes one body as before."],
  ["D5", "`slot(p1, p2, r)`", "d5", "Slot as exact 2D primitive (before: unknown module, plate without hole)."],
  ["D7", "`-m 'modul(args)'`", "d7", "Module context with arguments as in the code, nested `teile.schraube(d = 3)`; errors with exit 2."],
];
const feedbackRows = feedback
  .map(([id, title, key, what]) => `<tr><td>${esc(id)}</td><td>${md(title)}<br><span class="small">${md(what)}</span></td><td>${term(dTxt(`${key}-vorher`) ?? "(missing)")}</td><td>${term(dTxt(`${key}-nachher`) ?? "(missing)")}</td></tr>`)
  .join("");

// ---- test runs
const tauriRows = (tauri?.results ?? []).map((r) => `<tr><td>${esc(r.id)}</td><td>${esc(r.title)}</td><td class="${r.ok ? "ok" : "bad"}">${r.ok ? "passed" : "failed"}</td><td class="n">${fmt(r.ms / 1000, 1)} s</td></tr>`).join("");
const t19 = tauri?.results?.find((r) => r.id === "T19");
const t20 = tauri?.results?.find((r) => r.id === "T20");
const resCell = (ok, text) => `<span class="${ok ? "ok" : "bad"}">${esc(text)}</span>`;
const pwFiles = [...new Set(pwSpecs.map((t) => t.file))];
const pwNew = pwSpecs.filter((t) => /(asserts|phase-d)\.spec\.ts$/.test(t.file) || /^B12/.test(t.title));
const testRows = [
  ["`cargo fmt --all --check`", resCell(fmtOk, fmtOk ? "clean" : "diff"), dur("fmt")],
  ["`cargo test --workspace --no-fail-fast` (with Tauri/WebKitGTK, `CARGO_INCREMENTAL=0`)", cargoSum ? resCell(cargoSum.failed === 0 && rcOk("cargo"), `${cargoSum.ok} passed, ${cargoSum.failed} failed, ${cargoSum.ignored} ignored (${cargoSum.suites} test binaries)${cargoFailed.length ? `: ${cargoFailed.join(", ")}` : ""}`) : "–", dur("cargo")],
  ["`cargo clippy --workspace --all-targets -- -D warnings` and `-p cad-wasm --target wasm32-unknown-unknown`", resCell(clippyOk, clippyOk ? "clean" : "finding"), `${dur("clippy")} + ${dur("clippy-wasm")}`],
  ["`cargo check -p kernel-solidcore --features solidcore`", resCell(rcOk("solidcore"), rcOk("solidcore") ? "ok" : "error"), dur("solidcore")],
  ["`cd web && pnpm test` (Vitest)", vitestSum ? resCell(vitestSum.failed === 0 && rcOk("vitest"), `${vitestSum.ok} passed, ${vitestSum.failed} failed`) : "–", dur("vitest")],
  ["`pnpm typecheck`", resCell(typecheckOk, typecheckOk ? "clean" : "errors"), dur("typecheck")],
  ["`pnpm e2e` (Playwright 1.56.1, Chromium with SwiftShader, full suite, JSON reporter)", pwStats ? resCell(pwStats.unexpected === 0, `${pwStats.expected} passed, ${pwStats.unexpected} failed${pwStats.flaky ? `, ${pwStats.flaky} flaky` : ""}${pwStats.skipped ? `, ${pwStats.skipped} skipped` : ""} (${pwFiles.length} files)${pwBad.length ? `: ${pwBad.map((t) => t.title.split(" ")[0]).join(", ")}` : ""}`) : "–", pwStats ? `${fmt(pwStats.duration / 60000, 1)} min` : dur("playwright")],
  ["`node web/e2e-tauri/desktop.e2e.mjs --bin target/release/mcad` (tauri-driver, Xvfb)", tauri ? resCell(tauriOk === tauri.results.length, `${tauriOk} / ${tauri.results.length} passed${tauri.results.filter((r) => !r.ok).length ? `: ${tauri.results.filter((r) => !r.ok).map((r) => r.id).join(", ")} red` : ""}`) : "–", dur("tauri")],
  ["`MCAD=target/release/mcad python3 tests/openscad-compat/compare.py --eval`", compatSum ? resCell(compatSum[1] === compatSum[2] && rcOk("compat"), `${compatSum[1]} / ${compatSum[2]}`) : "–", dur("compat")],
  ["`tests/step/verify.py` (OCCT, cadquery-ocp 8.0.1.1, `--merge` against the root)", stepSum ? resCell(stepSum[1] === stepSum[2], `${stepSum[1]} / ${stepSum[2]}`) : "–", dur("step")],
  ["`tests/step/verify.py --bodies` (new default: body per part/object)", stepBodiesSum ? resCell(stepBodiesSum[1] === stepBodiesSum[2], `${stepBodiesSum[1]} / ${stepBodiesSum[2]}`) : "–", dur("step-bodies")],
  ["`tests/alltag/check.sh -j 4` (everyday models: check, parts, export)", alltagLast ? resCell(rcOk("alltag"), alltagLast) : "–", dur("alltag")],
]
  .map(([c, r, d]) => `<tr><td>${md(c)}</td><td>${r}</td><td class="n">${esc(d)}</td></tr>`)
  .join("");

// ------------------------------------------------------------------------------- HTML

const bOf = (f) => sph.find((r) => r.faces >= f && r.solid);
const r445 = sph.find((r) => r.triangles === 480);
const r890 = sph.find((r) => r.triangles === 968);
const r4646 = sph.filter((r) => r.solid).slice(-1)[0];
const rMesh = sph.slice(-1)[0];

const html = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Test report 21 – hull/minkowski, STL import, asserts, user feedback, mcad</title>
<style>
  @page { size: A4; margin: 15mm 13mm 16mm 13mm; }
  :root { --text: #1f2328; --muted: #5d646e; --border: #d5d9df; --ok: #1a7f37; --bad: #c9302c; --warn: #9a6700; --head: #f2f4f7; --s1: #2a78d6; --s2: #eb6834; --s3: #1baf7a; }
  * { box-sizing: border-box; }
  body { font: 9.6pt/1.45 system-ui, -apple-system, "Segoe UI", Roboto, sans-serif; color: var(--text); margin: 0; background: #fff; }
  main { max-width: 184mm; margin: 0 auto; padding: 0 0 4mm; }
  h1 { font-size: 18pt; margin: 0 0 2mm; }
  h2 { font-size: 13pt; margin: 7mm 0 3mm; border-bottom: 1px solid var(--border); padding-bottom: 1mm; break-after: avoid; }
  h3 { font-size: 10.5pt; margin: 4mm 0 1.5mm; break-after: avoid; }
  p { margin: 1.5mm 0; }
  .meta { color: var(--muted); margin-bottom: 4mm; }
  .summary { display: flex; flex-wrap: wrap; gap: 2.5mm; margin: 3mm 0; }
  .summary div { flex: 1 1 40mm; border: 1px solid var(--border); border-radius: 2mm; padding: 2mm; font-size: 8.4pt; }
  .summary b { display: block; font-size: 12pt; }
  table { width: 100%; border-collapse: collapse; margin: 2mm 0; font-size: 8.2pt; }
  th, td { border: 1px solid var(--border); padding: 1.1mm 1.8mm; text-align: left; vertical-align: top; }
  th { background: var(--head); font-weight: 600; }
  td.n, th.n { text-align: right; white-space: nowrap; }
  td .term, td pre { margin: 0; font-size: 6.9pt; }
  table.fb td:nth-child(3), table.fb td:nth-child(4) { width: 41%; }
  code, pre { font-family: ui-monospace, "SFMono-Regular", Menlo, Consolas, monospace; font-size: 7.6pt; }
  pre { margin: 0 0 1.5mm; padding: 1.4mm 2mm; background: #f6f8fa; border: 1px solid var(--border); border-radius: 1mm; white-space: pre-wrap; word-break: break-word; }
  pre.term { background: #16191d; color: #e6e9ed; border-color: #32373e; }
  pre.term.long { break-inside: auto; }
  thead { display: table-header-group; }
  .small, .small2 { font-size: 7.6pt; color: var(--muted); font-weight: 400; }
  td.small2 { color: var(--text); }
  ul { margin: 1mm 0 2mm 5mm; padding: 0; }
  li { margin: 0.5mm 0; }
  tr, figure, pre { break-inside: avoid; }
  .ok { color: var(--ok); font-weight: 600; } .bad { color: var(--bad); font-weight: 600; } .warn { color: var(--warn); font-weight: 600; }
  figure { margin: 0 0 2mm; }
  figure img { max-width: 100%; max-height: 105mm; border: 1px solid var(--border); border-radius: 1mm; background: #fff; display: block; }
  figcaption { font-size: 7.8pt; color: var(--muted); margin-top: 0.6mm; }
  figure.chart { break-inside: avoid; }
  .two { display: grid; grid-template-columns: 1fr 1fr; gap: 3mm; align-items: start; }
  .three { display: grid; grid-template-columns: 1fr 1fr 1fr; gap: 2.5mm; align-items: start; }
  .grid4 { display: grid; grid-template-columns: repeat(4, 1fr); gap: 2mm; align-items: start; }
  .grid4 figcaption { font-size: 7pt; }
  .codefig { display: grid; grid-template-columns: 0.9fr 1.1fr; gap: 3mm; align-items: start; }
  .codefig pre { break-inside: auto; }
  table.narrow { font-size: 7.4pt; }
  table.narrow th, table.narrow td { padding: 0.9mm 1.2mm; }
  .stack figure img { max-height: 60mm; }
</style>
</head>
<body>
<main>
<h1>Test report 21 – hull/minkowski, STL import, collision asserts, user feedback, rename to mcad</h1>
<div class="meta">Plan 21 (phases A–C), user feedback (phase D), rename (phase E): all commits since <code>852fbc1</code> · ${esc(date)} · Branch ${esc(branch)} · Commit ${esc(commit)} · mcad 0.3.0 · ${esc(rustc)} · ${os.cpus().length} cores. Measurements with the release binary without daemon (<code>tools/report/report21-data.py</code>), test runs on the same commit.</div>

<div class="summary">
  <div>hull/minkowski vs. formula<b class="${hullOk === hullWithFormula && hullWithFormula ? "ok" : "bad"}">${hullOk} / ${hullWithFormula}</b>within 10⁻⁵ relative; ${hullWarn} cases without a special case → warning</div>
  <div>STL: B-rep up to 500 faces<b>${r445?.solid ? `${fmt((r445.solid.kernel_ms + r445.solid.bool_ms) / 1000, 1)} s` : "–"}</b>build + Boolean at ${r445 ? fmt(r445.faces) : "–"} faces; ${r4646?.solid ? `${fmt(r4646.faces)} faces: ${fmt(r4646.solid.bool_ms / 1000)} s` : ""}</div>
  <div>Assert cases<b class="${assertsOk === asserts.length && asserts.length ? "ok" : "bad"}">${assertsOk} / ${asserts.length}</b>kind and value as expected; export on violation exits with 4</div>
  <div>cargo test --workspace<b class="${cargoSum && cargoSum.failed === 0 ? "ok" : "bad"}">${cargoSum ? `${cargoSum.ok} / ${cargoSum.ok + cargoSum.failed}` : "–"}</b>clippy ${clippyOk ? "clean" : "finding"}, fmt ${fmtOk ? "clean" : "diff"}</div>
  <div>Vitest · Playwright<b class="${vitestSum?.failed === 0 && pwStats?.unexpected === 0 ? "ok" : "bad"}">${vitestSum ? vitestSum.ok : "–"} · ${pwStats ? `${pwStats.expected} / ${pwStats.expected + pwStats.unexpected + pwStats.flaky}` : "–"}</b>browser with Wasm kernel</div>
  <div>Tauri E2E<b class="${tauri && tauriOk === tauri.results.length ? "ok" : "bad"}">${tauri ? `${tauriOk} / ${tauri.results.length}` : "–"}</b>desktop app with native kernel</div>
  <div>OpenSCAD · STEP · everyday<b class="${compatSum && stepSum && rcOk("alltag") && compatSum[1] === compatSum[2] && stepSum[1] === stepSum[2] ? "ok" : "bad"}">${compatSum ? `${compatSum[1]}/${compatSum[2]}` : "–"} · ${stepSum ? `${stepSum[1]}/${stepSum[2]}` : "–"} · ${rcOk("alltag") ? "ok" : "finding"}</b>comparison, OCCT check, 19 models</div>
</div>

<h2>1 Scope</h2>
<table><tr><th>Phase</th><th>Commit</th><th>Title</th></tr>${commitRows}</table>
<p class="small">User decisions for Plan 21: <code>hull</code>/<code>minkowski</code> only in exact special cases, otherwise as before a warning with unchanged children (no approximation); STL import with an automatic B-rep/mesh limit and override <code>as = "solid" | "mesh"</code>; asserts: violation = warning, <code>mcad export</code> aborts (unless <code>--ignore-asserts</code>), <code>assert_keepout</code> clearly visible in the UI.</p>

<h2>2 Phase A: Offset, hull, minkowski</h2>
<h3>Kernel offset (spike)</h3>
<p><code>Kernel::offset(s, d, tag)</code> is the Minkowski sum of a body with a sphere: facade <code>sc_offset</code> via <code>SmApiOffsetBrepFull</code> (outward, rounding, self-intersection handling). The spike (state <code>83ee5b3</code>, volume from the mass properties against the Steiner formula) shows where SMLib is exact: convex edges become cylinders and corners become spheres, as long as the faces meet at right angles at the corners. On the tetrahedron the surfaces are correct (4 planes, 6 cylinders, 4 spheres with exact centers and radii), but the trim curves lie up to 0.33 mm off the edges (volume +4.5·10⁻⁴ vs. Steiner); on the 45° wedge SMLib fails. The facade therefore checks the distance of the result boundary to the body (9 points per edge, face center) and the trim curves against the 3D edges (10⁻⁶ of the size), and otherwise reports "offset is not exact".</p>
${spikeTable}
<p class="small">Column "substitute": face offset (faces extended) followed by filleting all edges with r – not viable on any body except the cylinder and the rotated cuboid, therefore discarded. Raw data <code>reports/tmp/report21/phase-a/offset-spike.txt</code>.</p>

<h3>Language and example</h3>
<p><code>hull()</code> and <code>minkowski()</code> choose the case by their children (transformations, groups, unions and nested hulls are looked through; spheres/cylinders/arcs only under similarity transformations). Exact special cases:</p>
<table>
<tr><th>Operation</th><th>Special case</th><th>Result</th></tr>
<tr><td><code>hull</code> 2D</td><td>every region made of lines and circular arcs (text, SVG: B-splines via biarcs)</td><td>lines + arcs via the support function, transitions at the common tangents in closed form</td></tr>
<tr><td><code>minkowski</code> 2D</td><td>profile + circle; two convex profiles (one of them a polygon)</td><td>offset or convex sum</td></tr>
<tr><td><code>hull</code> 3D</td><td>planar bodies; spheres/cylinders/cones on one axis; parallel prisms of equal height; equal spheres</td><td>polyhedron; solid of revolution of the 2D hull of the half sections; extruded 2D hull; hull of the centers + offset r</td></tr>
<tr><td><code>minkowski</code> 3D</td><td>body + sphere; <code>linear_extrude</code> + <code>cylinder</code> (parallel prisms); two convex planar bodies; one child</td><td>offset; prism of the 2D sum; polyhedron; the child</td></tr>
<tr><td>otherwise</td><td>e.g. spheres of different radii not on one axis, tetrahedron + sphere, torus, <code>$fillet</code> primitives</td><td>warning "hull: &lt;reason&gt;; children are shown unchanged"</td></tr>
</table>
<div class="codefig"><div>${pre((readText(join(root, "tests/models/spec/hull_minkowski.mcad")) ?? "").split("\n").filter((l) => !l.startsWith("//")).join("\n"))}${term(readText(join(data, "a/beispiel_check.txt")))}</div><div>${fig(dimg("a", "beispiel_iso.png", 1000), "`tests/models/spec/hull_minkowski.mcad`, `mcad render` (iso): plate as 2D hull of four circles with slots, rounded block (cube ⊕ sphere), handle as capsule, tab (cuboid + cylinder of equal height), cushion of eight equal spheres, strip from `linear_extrude` ⊕ `cylinder`")}</div></div>

<h3>Measurements vs. formula</h3>
<p>Volume via <code>mcad measure --volume</code> (exact B-rep, mass properties of the kernel); 2D cases extruded by 1, so the volume is the area. Time = full CLI call without daemon (parsing, evaluation, kernel, volume), median of 3 runs.</p>
${hullTable}
<p class="small">Deviations: 2D hulls and prisms ≤ 10⁻⁸ (integration); solids of revolution (capsule, spheres, dome) 10⁻⁷–10⁻⁶ – accuracy of the volume integration of surfaces of revolution, the geometry itself is exact (STEP: <code>SURFACE_OF_REVOLUTION</code>, no B-splines); cube + "point": the point is a cube with 10⁻⁴ edge length (+2.9·10⁻⁶ is its contribution).</p>
${hullGrid}

<h3>Limitations</h3>
<ul>
<li>Offset (and thus <code>minkowski</code> with a sphere) only on bodies whose faces meet at right angles at the corners; tetrahedra, wedges, oblique polyhedra → warning, children unchanged.</li>
<li>Equal spheres with centers in one plane (rounded plate) are not a special case (the hull of the centers is flat, an offset of it does not work) – case "4 equal spheres in one plane" above; workaround: <code>linear_extrude</code> of a 2D hull with <code>$fillet</code>.</li>
<li><code>rotate_extrude</code> children, torus and <code>$fillet</code> primitives in <code>hull</code>: warning.</li>
</ul>

<h2>3 Phase B: <code>import("x.stl")</code></h2>
<h3>Language and modes</h3>
${pre(`import("../stl/winkel.stl");                     // closed, ≤ 500 faces: B-rep (Booleans work)
import("platine.stl", as = "mesh");              // pure mesh (display, export, measuring)
import("teil.stl", center = true, scale = 25.4); // inch → mm, bounding box around the origin`)}
<table>
<tr><th>Mode</th><th>automatic if</th><th>works</th><th>does not work</th></tr>
<tr><td>B-rep (<code>solid</code>)</td><td>closed, manifold, ≤ 500 faces after merging</td><td>everything: Booleans, <code>fillet</code>/<code>$fillet</code>, exact measuring, STEP</td><td>–</td></tr>
<tr><td>Mesh (<code>mesh</code>)</td><td>&gt; 500 faces, open, non-manifold</td><td>display, transformations, <code>color</code>, parts, <code>hull</code> (≤ 2000 vertices), STL/3MF, STEP faceted, bounding box/volume/area/section, <code>--clearance</code>, <code>--interference</code> (yes/no)</td><td>Booleans, <code>fillet</code>/<code>chamfer</code>/<code>shell</code>, <code>minkowski</code>, <code>thicken</code> – error or warning with the hint <code>as = "solid"</code></td></tr>
</table>
<p>Preparation in every mode (<code>cad-stl</code>): weld vertices (10⁻⁶ of the diagonal), discard NaN, degenerate and duplicate triangles, orient consistently outward, merge coplanar neighbors into planar faces, pull vertices onto the planes of their faces. ASCII and binary (binary detected by the size 84 + 50·n, also with a "solid" header).</p>
<div class="three">${fig(dimg("b", "beispiel.png", 900), "`tests/models/spec/stl_import.mcad`: the same angle bracket as B-rep with bore (blue) and mirrored as mesh (orange)")}${fig(dimg("b", "solid_kugel1000_boolean.png", 700), "Sphere STL (968 triangles, 890 faces) with `as = \"solid\"` minus cylinder")}${fig(dimg("b", "netz_kugel20000.png", 700), "Sphere with 20 000 triangles automatically as mesh (edges only from 20°)")}</div>${term((readText(join(data, "b/stl_import_inspect.txt")) ?? "").split("\n").filter((l) => /^\$|^Z\.|^line \d|Hinweis|hint:/.test(l)).join("\n"))}

<h3>Run time and the 500-face limit</h3>
${stlChart()}
${benchTable}
${realTable}
<p>Up to 500 faces, import + one Boolean stays at ${r445?.solid ? `${fmt((r445.solid.kernel_ms + r445.solid.bool_ms) / 1000, 1)} s` : "–"} (${r445 ? fmt(r445.faces) : "–"} faces). Above that the Boolean time grows superlinearly (${r890?.solid ? `${fmt(r890.faces)} faces ${fmt(r890.solid.bool_ms / 1000, 1)} s` : ""}${r4646?.solid ? `, ${fmt(r4646.faces)} faces ${fmt(r4646.solid.bool_ms / 1000)} s` : ""}); the times scatter between neighboring sizes because the number of intersection edges at the poles depends on the subdivision of the sphere. As a mesh the kernel costs nothing; parsing and preparing ${rMesh ? fmt(rMesh.triangles) : "–"} triangles takes ${rMesh?.mesh ? fmt(rMesh.mesh.eval_ms) : "–"} ms, display mesh and image ≈ 0.2 s regardless of size. Realistic exports lie just above the limit (more faces than a sphere with the same triangle count because of the bores) and automatically become meshes; with <code>as = "solid"</code> build and one Boolean take 1–1.5 s.</p>

<h3>Combined model: housing from STL, circuit board as mesh</h3>
<div class="two">${fig(dimg("b", "kombi_schnitt.png", 900), "Section y = 10 (`--section-flip`): housing (STL as B-rep, two bores), \"circuit board\" (sphere with 20 000 triangles, scaled, as mesh) and lid; caps also on the mesh")}${fig(dimg("b", "kombi_xray.png", 900), "X-ray: the same model")}</div>
${term(readText(join(data, "b/kombi_measure.txt")))}
<h3>Limitations</h3>
<ul>
<li>Collisions with meshes only yes/no (penetration or containment), without intersection volume; distances via the triangles.</li>
<li>STEP of a mesh is faceted (one plane per face); Booleans and fillets need <code>as = "solid"</code> and are slow from a few thousand faces on (curve above).</li>
</ul>

<h2>4 Editor: no inline sliders for numbers</h2>
<p>Commit <code>2d3451e</code>: the slider at the end of the line of number assignments got in the way of reading and writing and has been removed. Bool literals keep their toggle; numbers can still be changed via the parameter bar and the handles. Playwright B12 checks that no <code>inline-slider</code> appears any more.</p>
<div class="stack">${fig(img(join(root, "docs/reports/03-ui-phase-b/img/b12-nachher.jpg"), "editor-vorher.jpg", 700), "Before (report 3, B12): `t = 7;` with slider at the end of the line")}${fig(img(join(root, "reports/tmp/report3/img/b12-vorher.jpg"), "editor-nachher.jpg", 900) ?? img(join(root, "reports/tmp/report3/img/b12-vorher.png"), "editor-nachher.jpg", 900), "After (B12, final run): only the toggle on `hohl = true;`")}</div>

<h2>5 Phase C: collision asserts</h2>
<h3>Language</h3>
<div class="codefig"><div>${pre((readText(join(root, "tests/models/spec/asserts.mcad")) ?? "").split("\n").filter((l) => !l.startsWith("//")).join("\n").trim())}</div><div>
<table><tr><th>Assert</th><th>Violation (warning)</th></tr>
<tr><td><code>assert_keepout(clearance = 0)</code></td><td>a real body intersects the keep-out volume (volume &gt; ε) or is closer than <code>clearance</code>; the keep-out volume is shown, but not exported, counted or included in calculations</td></tr>
<tr><td><code>assert_disjoint()</code></td><td>a pair of the children intersects</td></tr>
<tr><td><code>assert_intersect()</code></td><td>the children do not form a connected group of intersecting bodies (touching is not intersecting)</td></tr>
<tr><td><code>assert_touches(tol, contact)</code></td><td>gap &gt; tol, overlap, or only line/point contact with <code>contact = "face"</code></td></tr>
</table>
<p class="small">Asserts are groups of their children and can be nested. Intersections exact via Boolean per body pair (volume from the mass properties), contact faces exact via <code>region2</code> for lines and arcs, distances via the tessellation with foot points; pairs cached in the engine under content keys.</p></div></div>

<h3>Test cases vs. formula</h3>
<p><code>mcad check --json</code> per case (separate process, cold cache, median of 3 runs). "Check" = time of the assert check, "call" = full CLI call.</p>
${assertsTable}
<p class="small">Distances (K2, T7) come from the f32 tessellation and deviate by ≈ 10⁻⁷ relative; volumes and contact areas are exact. The sphere (K5, T6) costs ≈ 0.2 s for the Boolean with the spherical surface.</p>

<h3>Export, CLI images</h3>
${term(readText(join(data, "c/export.txt")))}
<div class="two">${fig(dimg("c", "asserts-ok-hell.png", 900), "`mcad render asserts.mcad --show asserts`: contact lid/housing 384 mm² green, intended intersection holder/cable guide green, keep-out volume yellow dashed")}${fig(dimg("c", "asserts-verletzt-dunkel.png", 900), "Violation (`luft = 3`, holder too high), `--xray --theme dark`: intersection red, distance \"2 mm < 3\" as a line with foot points")}</div>
${fig(dimg("c", "faelle-hell.png", 1100, true), "`faelle.mcad --show asserts`: all kinds side by side – keepout (intersection), disjoint (intersection 200 mm³), intersect (separated), face contact 25 mm², line contact, gap 0.2 mm")}

<h3>UI (browser, Wasm kernel)</h3>
<p>Keep-out volume hatched in yellow and visible through the model, not selectable or measurable, toggle "Keep-out volumes" (remembered); intersection body of a violation red, contact green, gap as dimension line; status bar "n assert violations", a click on it or on the warning at the end of the line frames the violation; every export asks for confirmation when there is a violation. Images from the final Playwright run (C01–C05; the Playwright runs use the German UI, <code>locale: "de-DE"</code>).</p>
<div class="two">${fig(uiC("c01-keepout-ok-hell"), "C01 light: keep-out volume (STL angle bracket) hatched, contact 384 mm²")}${fig(uiC("c01-keepout-ok-dunkel"), "C01 dark")}</div>
<div class="two">${fig(uiC("c02-keepout-verletzt-hell"), "C02: violation, intersection body red with \"intersection 180 mm³\" (German UI: „Schnitt 180 mm³“)")}${fig(uiC("c02-keepout-verletzt-eingerahmt-dunkel"), "C02 dark: framed after clicking \"1 assert violation\"")}</div>
<div class="two">${fig(uiC("c04-export-dialog-hell", 1100), "C04: \"Export despite assert violation?\" (German UI: „Export trotz Assert-Verletzung?“) with list")}${fig(uiC("c05-touches-dunkel"), "C05 dark: contact area 25 mm² green, gap 0.2 mm with dimension line")}</div>
${t19 ? `<h3>Desktop app (Tauri T19)</h3><p>T19 ${t19.ok ? '<span class="ok">passed</span>' : '<span class="bad">failed</span>'} (${fmt(t19.ms / 1000, 1)} s): ${t19.checks.map((c) => md(roundLong(c.text))).join("; ")}.</p><div class="two">${(t19.images ?? []).slice(0, 2).map((i) => fig(tauriImg(i.name), `T19: ${i.label}`)).join("")}</div>` : ""}

<h3>Wasm build</h3>
<p>emsdk 6.0.13 and wasm-bindgen-cli 0.2.129 could be set up in the container (downloads via the proxy); <code>smlib.wasm</code> rebuilt (≈ 5 min, 244 targets, with <code>sc_offset</code> and the contact locations from phase D), <code>cad-wasm</code> ≈ 65 s. Sizes after <code>wasm-opt -O3</code>:</p>
<table><tr><th>File</th><th class="n">before phase A (852fbc1)</th><th class="n">after phase C</th><th class="n">now</th><th class="n">Δ vs. before</th><th class="n">gzip -9 before → now</th></tr>${wasmRow("cad_wasm_bg.wasm", "cad")}${wasmRow("smlib.wasm", "smlib")}</table>
<p class="small">Values before phase A and after phase C from the phase C material (same tools, old state as a separate build); "now" = <code>web/src/wasm-pkg</code> of the final state.</p>

<h3>Limitations</h3>
<ul>
<li>Collision with meshes only yes/no, without volume (K6).</li>
<li>The keepout check computes Booleans in the worker: on a hole plate 120×80 with 24 bores and fillet ≈ 2.5 s the first time (afterwards from the cache); the worker is busy for that long.</li>
<li>Point contact (sphere on plate) is detected, but reported and drawn without location.</li>
</ul>

<h2>6 Phase D: user feedback</h2>
<p>For each item the same call with the release build before phase D (<code>cad</code> 0.2.1) and with the final state (<code>mcad</code> 0.3.0); inputs as <code>.scad</code>, because the old build does not know <code>.mcad</code>.</p>
<table class="fb"><tr><th>No.</th><th>Item</th><th>before</th><th>after</th></tr>${feedbackRows}</table>

<h3>D1 Kernel errors with location</h3>
<p>If a Boolean result is not a closed solid, the facade (version 6) returns the non-manifold edges as polylines; the engine knows the node per operand and names the touching operands. JSON diagnostic with <code>at</code> (context coordinates) and <code>nodes</code>; <code>mcad render</code> marks every location (edges red, point, label "contact l.15 + 16"), the UI does the same, and a click on the message frames the location.</p>
<div class="two">${fig(dimg("d", "dom_rippe-vorher.png", 800), "Before: `cad render dom_rippe.scad` without marking (and without `slot`)")}${fig(dimg("d", "dom_rippe-nachher.png", 800), "After: contact line red with \"contact l.15 + 16\"")}</div>
<div class="two">${fig(dimg("d", "f08c-nachher.png", 800), "`f08c_differenz_torus_tangential`: cylinder − torus, both contact circles red")}${fig(uiD("d01-beruehrung-eingerahmt-dunkel"), "UI (Playwright D01, dark): a click on the message frames the location")}</div>

<h3>D3 STEP: body per part and object</h3>
<p>Default: one <code>MANIFOLD_SOLID_BREP</code> per part (name of the part) and per top-level object outside the parts (name of the object through transformations); touching parts stay separate (before: merged). If objects overlap (sum of volumes &gt; volume of the union), the export prints a notice. <code>--merge</code> writes the root as one body as before. Of 64 example models, ${stepChanged.length} changed their body count (state <code>8ac92d6</code>, read with OCCT):</p>
<table><tr><th>Model</th><th class="n">before (<code>--merge</code>)</th><th class="n">after</th><th>Names after</th></tr>${stepChanged.map((r) => `<tr><td><code>${esc(r.model)}</code></td><td class="n">${r.before}</td><td class="n">${r.after}</td><td class="small2">${esc(r.names.length > 90 ? `${r.names.slice(0, 90)} …` : r.names)}</td></tr>`).join("")}</table>
${stepOverlap.length ? `<p class="small">With the notice "objects overlap" (final run <code>verify.py --bodies</code>, volumes not compared there): ${stepOverlap.map((r) => `<code>${esc(r.model)}</code> (${r.solids} bodies, sum ${fmt(r.vOcct, 1)} mm³ vs. ${fmt(r.vKernel, 1)} mm³ of the union)`).join(", ")} – see limitations.</p>` : ""}

<h3>D5 <code>slot(p1, p2, r)</code></h3>
<table><tr><th>Call, extruded by 1</th><th class="n">mcad</th><th class="n">πr² + 2r·|p2 − p1|</th><th class="n">rel. dev.</th></tr>${slotAreas.map((m) => `<tr><td><code>${esc(m[1])}</code></td><td class="n">${fmt(+m[2], 4)}</td><td class="n">${fmt(+m[3], 4)}</td><td class="n">${sci(+m[4])}</td></tr>`).join("")}</table>
<p class="small">Area from the volume integration (state <code>91f8ef6</code>); the contour itself consists of two semicircles and two lines (exact arcs in the section, see table above). <code>p1 == p2</code> yields a circle; points also as grid points/anchors, <code>d = …</code> instead of <code>r</code>; handles for p1, p2 and r.</p>
<div class="two">${fig(uiD("d02-slot-griffe"), "UI (D02): handles p1, p2 and r on `linear_extrude(2) slot(…)`")}${fig(uiD("d02-slot-radius-gezogen"), "D02: radius dragged, the code follows")}</div>

<h3>D6 JSON schema 2</h3>
<table><tr><th></th><th>before</th><th>after</th></tr>
<tr><td><code>mcad inspect fillet_zonen --line 12 --json</code></td><td><code>schema_version ${esc(d6.vorher?.schema_version ?? "–")}</code>, <code>selected_edges: ${esc(d6.vorher?.selected_edges ?? "–")}</code> (number)</td><td><code>schema_version ${esc(d6.nachher?.schema_version ?? "–")}</code>, <code>selected_edges</code>: ${esc(d6.nachher?.selected_edges ?? "–")} (world coordinates), <code>selected_edge_count: ${esc(d6.nachher?.selected_edge_count ?? "–")}</code></td></tr>
<tr><td><code>outline</code></td><td>grid plane <code>axis</code> as name, literals/parts inconsistent</td><td><code>axis</code> 0|1|2 as in the scene (+ <code>axis_name</code>), <code>variables[].literal</code> as range|null, <code>instances[].part</code> as name|null</td></tr>
</table>

<h2>7 Phase E: rename <code>cad</code> → <code>mcad</code></h2>
<p>Binary, program name, messages and manual are called <code>mcad</code> (version 0.3.0); new files get <code>.mcad</code>, and <code>.mcad</code> and <code>.scad</code> open everywhere (CLI, <code>include</code>/<code>use</code>, dialogs, folder projects, upload, ZIP, examples menu). Inventory via <code>git grep</code> before the rename: 1703 hits for <code>-w cad</code> outside the reports, 187 <code>.scad</code> files.</p>
<table><tr><th>Area</th><th>before</th><th>after</th></tr>
<tr><td>Binary, messages, manual, skill</td><td><code>cad</code>, <code>cad check: …</code></td><td><code>mcad</code>, <code>mcad check: …</code>; test <code>usage::no_old_program_name</code></td></tr>
<tr><td>Environment, configuration, socket</td><td><code>CAD_*</code>, <code>~/.config/controller-cad/</code>, <code>$XDG_RUNTIME_DIR/controller-cad</code></td><td><code>MCAD_*</code> (old names as fallback), <code>~/.config/mcad/</code> (the old file applies if the new one is missing), <code>$XDG_RUNTIME_DIR/mcad</code></td></tr>
<tr><td>Desktop app</td><td>productName <code>controller-cad</code>, app ID <code>dev.controller-cad.desktop</code></td><td><code>mcad</code>, <code>dev.mcad.desktop</code>, window title "… – mcad"; list of recently opened files migrated once</td></tr>
<tr><td>File types, package</td><td>desktop file with <code>application/x-openscad</code>, Arch package <code>controller-cad</code></td><td><code>text/x-mcad</code> and <code>application/x-openscad</code> (<code>packaging/linux/mcad.xml</code>), package <code>mcad</code> with <code>replaces=('controller-cad')</code></td></tr>
<tr><td>Examples, test data</td><td>94 files <code>*.scad</code></td><td><code>*.mcad</code> via <code>git mv</code>, references updated</td></tr>
<tr><td>STEP/3MF metadata, web title</td><td><code>controller-cad</code>, "Parametrisches CAD"</td><td><code>mcad</code>, "mcad – Parametrisches CAD"</td></tr>
</table>
<table><tr><th>unchanged</th><th>Reason</th></tr>
<tr><td>Crate names <code>cad-*</code>, skill folder <code>cad-cli</code></td><td>internal; paths in briefs and tools</td></tr>
<tr><td>Repository <code>controller-cad</code>, Docker image/volume of the Arch build environment</td><td>name of the repository and of the build infrastructure</td></tr>
<tr><td>IndexedDB name <code>controller-cad</code> (browser)</td><td>holds the users' saved projects</td></tr>
<tr><td>Font name "cad stencil mono"</td><td>appears in models</td></tr>
<tr><td><code>tests/models/openscad-compat/*.scad</code>, <code>tests/openscad-compat</code>, <code>tests/farben-pruefung</code>, case study report 8</td><td>are compared with OpenSCAD or are snapshots; incidentally check that <code>.scad</code> still works</td></tr>
<tr><td>Reports <code>docs/reports/**</code></td><td>snapshots</td></tr>
</table>
${term(readText(join(data, "e/dateien.txt")))}
${t20 ? `<p>Tauri T20 ${t20.ok ? '<span class="ok">passed</span>' : '<span class="bad">failed</span>'}: ${t20.checks.map((c) => md(roundLong(c.text))).join("; ")}.</p><div class="two">${(t20.images ?? []).slice(0, 2).map((i) => fig(tauriImg(i.name), `T20: ${i.label}`)).join("")}</div>` : ""}

<h2>8 Test runs on the final state</h2>
<table><tr><th>Command</th><th>Result</th><th class="n">Duration</th></tr>${testRows}</table>
<p><b>Found and fixed:</b> <code>tests/step/verify.py</code> aborted on the final state with <code>SyntaxError: unmatched ')'</code> before checking a single file – when the default was switched from <code>--cad</code> to <code>MCAD</code> (commit <code>4db585b</code>), an extra parenthesis had been left behind. Fixed in <code>0ea0d78</code>; afterwards 64/64 in both modes. Otherwise all runs were green on the first pass; nothing skipped or disabled.</p>
<p class="small">All runs sequentially on the same machine (${os.cpus().length} cores), logs in <code>reports/tmp/report21/final/</code>. New in these phases: Playwright ${pwNew.map((t) => t.title.split(" ")[0]).join(", ")}; Tauri T19, T20.</p>
${pwBad.length ? `<h3>Failed Playwright tests</h3><table><tr><th>Test</th><th>File</th><th>Error</th></tr>${pwBad.map((t) => `<tr><td>${esc(t.title)}</td><td><code>${esc(t.file)}</code></td><td>${pre((t.errors[0] ?? "").replace(/\u001b\[\d+m/g, "").split("\n").slice(0, 6).join("\n"))}</td></tr>`).join("")}</table>` : ""}
<h3>Tauri E2E</h3>
<table><tr><th>No.</th><th>Scenario</th><th>Result</th><th class="n">Duration</th></tr>${tauriRows}</table>

<h2>9 Limitations and open items</h2>
<table><tr><th>Item</th><th>Cause or status</th></tr>
<tr><td>Offset/<code>minkowski</code> with a sphere at non-right-angled corners</td><td>SMLib approximates the corner spheres with trim curves up to 0.33 mm off the edges (tetrahedron) or fails (wedge); the facade rejects → warning, children unchanged.</td></tr>
<tr><td>Equal spheres with centers in one plane</td><td>not a special case (flat hull of the centers); warning.</td></tr>
<tr><td>Collision with STL meshes</td><td>yes/no only, no intersection volume.</td></tr>
<tr><td>Keepout check in the worker</td><td>Booleans with filleted bodies block the worker for a few seconds the first time (hole plate ≈ 2.5 s); afterwards cache.</td></tr>
<tr><td>Point contact</td><td>detected, but without location (no marking).</td></tr>
<tr><td>STEP: overlapping top-level objects</td><td>are written as overlapping individual bodies (notice in the export). User decision pending; proposal: unite overlapping objects into one body, keep touching ones separate.</td></tr>
<tr><td>STL B-rep over 500 faces</td><td>Booleans superlinear (curve in section 3); limit automatic, can be forced with <code>as = "solid"</code>.</td></tr>
<tr><td>UI settings of the desktop app</td><td>settings and theme live in the WebView's <code>localStorage</code>, which Tauri stores under the app ID; with <code>dev.mcad.desktop</code> they start fresh. Only the list of recently opened files is migrated.</td></tr>
<tr><td>Arch package <code>mcad</code></td><td>PKGBUILD, <code>docker.sh</code> and <code>smoke-test.sh</code> updated, but not built: no <code>makepkg</code> and no running Docker daemon in the container.</td></tr>
<tr><td><code>compare.py --self-test</code></td><td>needs reference STLs from OpenSCAD, which are missing here; <code>--eval</code> is checked.</td></tr>
</table>
</main>
</body>
</html>
`;

const htmlOut = html.replace(/(<table[^>]*>\s*(?:<colgroup>.*?<\/colgroup>)?)\s*(<tr>\s*<th[\s\S]*?<\/tr>)/g, "$1<thead>$2</thead>");
writeFileSync(join(out, "report.html"), htmlOut);
console.log(`${join(out, "report.html")}: ${html.length} characters; hull ${hull.length}, STL ${sph.length}, asserts ${asserts.length}, images in ${imgDir}`);
