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
//   node tools/report/print-pdf.mjs docs/reports/21-hull-stl-asserts/report.html docs/reports/21-hull-stl-asserts/report.pdf "Testbericht 21: hull/minkowski, STL-Import, Asserts, Nutzer-Feedback, mcad"

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
const de = (x, d = 0) => (x === null || x === undefined || Number.isNaN(Number(x)) ? "–" : Number(x).toLocaleString("de-DE", { minimumFractionDigits: d, maximumFractionDigits: d }));
const sci = (x) => (x === null || x === undefined ? "–" : x === 0 ? "0" : `${x > 0 ? "+" : "−"}${Math.abs(x).toExponential(1).replace(".", ",").replace("e-", "·10⁻").replace("e+", "·10^")}`.replace(/10⁻(\d+)/, (_, e) => `10⁻${[...e].map((c) => "⁰¹²³⁴⁵⁶⁷⁸⁹"[+c]).join("")}`));
const md = (t) => esc(t).replace(/`([^`]+)`/g, "<code>$1</code>");
const pre = (t) => `<pre>${esc(String(t).trimEnd())}</pre>`;
const term = (t) => {
  const x = String(t ?? "(fehlt)").trimEnd();
  return `<pre class="term${x.split("\n").length > 18 ? " long" : ""}">${esc(x)}</pre>`;
};
/** Long decimals in check texts of the E2E runs (179.99999999999994) rounded. */
const num = (t) => String(t).replace(/\d+\.\d{6,}/g, (m) => String(+(+m).toFixed(4)));

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
const fig = (s, cap, cls = "") => (s ? `<figure class="${cls}"><img src="${s}" alt=""><figcaption>${md(cap)}</figcaption></figure>` : `<p class="small">(Bild fehlt: ${esc(cap)})</p>`);
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
const dur = (k) => (times[k] ? (times[k].s >= 90 ? `${de(times[k].s / 60, 1)} min` : `${times[k].s} s`) : "–");
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

const date = "25.10.2026";
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
  ["0ea0d78", "Bericht"],
];
let phase = "A";
const commitRows = commits
  .map(([h, s]) => {
    for (const [p, n] of PHASES) if (h.startsWith(p)) phase = n;
    return `<tr><td>${esc(phase)}</td><td><code>${esc(h)}</code></td><td>${md(s)}</td></tr>`;
  })
  .join("");

// ---- phase A: offset spike (raw lines of the spike in phase-a/offset-spike.txt)
const SPIKE_BODIES = [
  ["box", "Quader 10×20×30"],
  ["rotated box", "Quader gedreht"],
  ["cylinder", "Zylinder r 5, h 10"],
  ["sphere", "Kugel r 5"],
  ["L", "L-Profil (konkave Kante)"],
  ["filleted box", "verrundeter Quader (r 1)"],
  ["tetra", "Tetraeder"],
  ["wedge", "45°-Keil (Extrusion)"],
  ["wedge polyhedron", "45°-Keil (Polyeder)"],
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
      ? "überlappt sich selbst"
      : /closed solid/.test(s)
        ? "nicht geschlossen"
        : /trim curves up to ([\d.]+)/.test(s)
          ? `nicht exakt: Trimmkurven bis ${de(+/trim curves up to ([\d.]+)/.exec(s)[1], 2)} mm neben den Kanten`
          : /lies up to ([\d.]+)/.test(s)
            ? `nicht exakt: Rand bis ${de(+/lies up to ([\d.]+)/.exec(s)[1], 2)} mm neben dem Abstand`
            : /fillet/.test(s)
              ? "Verrundung scheitert"
              : s.slice(5, 60);
    return `<span class="bad">Fehler</span> (${esc(why)})`;
  }
  const rel = /rel=([+-][\d.e-]+)/.exec(s)?.[1];
  const ms = /([\d.]+) ms/.exec(s)?.[1];
  return `<span class="ok">ok</span>${rel ? `, ${sci(+rel)}` : ""}${ms ? `, ${de(+ms)} ms` : ""}`;
};
const spikeFaces = (s) => {
  const f = /faces=(\{[^}]*\})/.exec(s ?? "")?.[1];
  if (!f) return "–";
  const o = JSON.parse(f);
  const names = { plane: "Ebene", cylinder: "Zylinder", sphere: "Kugel", revolution: "Rotation", nurbs: "NURBS" };
  return Object.entries(o)
    .map(([k, v]) => `${v} ${names[k] ?? k}`)
    .join(", ");
};
const spikeTable = `<table><tr><th>Körper (r = 2)</th><th>Rundung ohne Selbstschnitt (mode 0)</th><th>Rundung mit Selbstschnitt (mode 2, gewählt)</th><th>Ersatz: Flächen-Offset + Verrundung</th><th>Flächen (mode 2)</th></tr>${SPIKE_BODIES.map(
  ([k, n]) => `<tr><td>${esc(n)}</td><td>${spikeCell(spike[`${k}|mode=0`])}</td><td>${spikeCell(spike[`${k}|mode=2`])}</td><td>${spikeCell(spike[`${k}|ext+fillet`])}</td><td>${esc(spikeFaces(spike[`${k}|mode=2`]))}</td></tr>`,
).join("")}</table>`;

// ---- phase A: hull/minkowski cases
const hullOk = hull.filter((r) => r.rel !== null && Math.abs(r.rel) <= 1e-5).length;
const hullWithFormula = hull.filter((r) => r.want !== null).length;
const hullWarn = hull.filter((r) => r.warnings.length).length;
const hullTable = `<table class="narrow"><colgroup><col style="width:22%"><col style="width:8%"><col style="width:15%"><col style="width:9%"><col style="width:9%"><col style="width:8%"><col style="width:7%"><col style="width:22%"></colgroup><tr><th>Fall</th><th>Operation</th><th>Formel</th><th class="n">Soll</th><th class="n">gemessen</th><th class="n">rel. Abw.</th><th class="n">Aufruf</th><th>Sonderfall bzw. Warnung</th></tr>${hull
  .map((r) => {
    const ok = r.rel !== null && Math.abs(r.rel) <= 1e-5;
    const note = r.hint ? esc(r.hint.replace(/^exakt:\s*/, "")) : r.warnings.length ? `<span class="warn">Warnung:</span> ${esc(r.warnings[0].replace(/^(hull|minkowski): /, ""))}` : "";
    return `<tr><td>${esc(r.what)}</td><td><code>${esc(r.op)}</code></td><td>${esc(r.formula)}</td><td class="n">${r.want === null ? "–" : de(r.want, 4)}</td><td class="n">${de(r.got, 4)}</td><td class="n ${r.rel === null ? "" : ok ? "ok" : "bad"}">${sci(r.rel)}</td><td class="n">${de(r.ms)} ms</td><td class="small2">${note}</td></tr>`;
  })
  .join("")}</table>`;
const HULL_PICS = ["2d_riemen", "2d_mink_L", "2d_text_G", "3d_kugeln_r5_r2", "3d_kapsel_gedreht", "3d_quader_zylinder", "3d_kugeln_quader", "3d_mink_wuerfel", "3d_mink_extrude_zyl", "3d_mink_tetraeder", "3d_kugeln_ungleich", "3d_kugeln_ebene"];
const hullGrid = `<div class="grid4">${HULL_PICS.map((n) => {
  const r = hull.find((x) => x.name === n);
  return fig(dimg("a", `${n}.png`, 480), r ? `${r.what}${r.warnings.length ? " – Warnung, Kinder unverändert" : ""}` : n);
}).join("")}</div>`;

// ---- phase B: STL bench + chart
const sph = bench.spheres ?? [];
const benchTable = `<table class="narrow"><tr><th class="n">Dreiecke</th><th class="n">Flächen</th><th>automatisch</th><th class="n">Sprache</th><th class="n">B-Rep-Aufbau</th><th class="n">B-Rep + Boolean</th><th class="n">Netz: Kernel / Boolean</th><th class="n">Anzeige + Bild</th></tr>${sph
  .map((r) => `<tr><td class="n">${de(r.triangles)}</td><td class="n">${de(r.faces)}</td><td>${r.auto === "solid" ? "B-Rep" : "Netz"}</td><td class="n">${de(r.mesh?.eval_ms)} ms</td><td class="n">${r.solid ? `${de(r.solid.kernel_ms / 1000, 2)} s` : "–"}</td><td class="n">${r.solid ? `${de(r.solid.bool_ms / 1000, 1)} s` : "–"}</td><td class="n">${de(r.mesh?.kernel_ms)} / ${de(r.mesh?.bool_ms)} ms</td><td class="n">${de(r.mesh?.render_ms)} ms</td></tr>`)
  .join("")}</table>`;
const realTable = `<table><tr><th>Modell (mcad-Export, wieder importiert mit <code>as = "solid"</code>)</th><th class="n">Dreiecke</th><th class="n">Flächen</th><th>automatisch</th><th class="n">B-Rep-Aufbau</th><th class="n">+ Boolean (Zylinder r 6)</th></tr>${(bench.real ?? [])
  .map((r) => `<tr><td>${r.model === "platte" ? "Platte 60×40×5 mit 8 Bohrungen" : "Gehäuse aus <code>box.mcad</code> mit Verrundung"}</td><td class="n">${de(r.triangles)}</td><td class="n">${de(r.faces)}</td><td>${r.auto === "solid" ? "B-Rep" : "Netz"}</td><td class="n">${de(r.kernel_ms / 1000, 2)} s</td><td class="n">${de(r.bool_ms / 1000, 2)} s</td></tr>`)
  .join("")}</table>`;

/** Log-log line chart: faces vs. seconds, B-rep build, B-rep + Boolean, mesh (language + kernel). */
function stlChart() {
  const W = 680, H = 380, L = 58, R = 150, T = 18, B = 66;
  const xs = [100, 30000], ys = [0.001, 300];
  const lx = (v) => L + ((Math.log10(v) - Math.log10(xs[0])) / (Math.log10(xs[1]) - Math.log10(xs[0]))) * (W - L - R);
  const ly = (v) => T + (1 - (Math.log10(v) - Math.log10(ys[0])) / (Math.log10(ys[1]) - Math.log10(ys[0]))) * (H - T - B);
  const series = [
    { name: "B-Rep + Boolean", color: "var(--s2)", pts: sph.filter((r) => r.solid).map((r) => [r.faces, r.solid.bool_ms / 1000]) },
    { name: "B-Rep-Aufbau", color: "var(--s1)", pts: sph.filter((r) => r.solid).map((r) => [r.faces, r.solid.kernel_ms / 1000]) },
    { name: "als Netz (Sprache + Kernel)", color: "var(--s3)", pts: sph.filter((r) => r.mesh).map((r) => [r.faces, Math.max((r.mesh.eval_ms + r.mesh.kernel_ms) / 1000, 0.001)]) },
  ];
  const grid = [];
  for (const v of [100, 1000, 10000]) grid.push(`<line x1="${lx(v)}" x2="${lx(v)}" y1="${T}" y2="${H - B}" class="g"/><text x="${lx(v)}" y="${H - B + 16}" text-anchor="middle" class="t">${de(v)}</text>`);
  for (const v of [300, 3000, 30000]) grid.push(`<text x="${lx(v)}" y="${H - B + 16}" text-anchor="middle" class="t">${de(v)}</text>`);
  for (const v of [0.001, 0.01, 0.1, 1, 10, 100]) grid.push(`<line x1="${L}" x2="${W - R}" y1="${ly(v)}" y2="${ly(v)}" class="g"/><text x="${L - 6}" y="${ly(v) + 4}" text-anchor="end" class="t">${de(v, v < 1 ? String(v).split(".")[1].length : 0)}</text>`);
  const limit = `<line x1="${lx(500)}" x2="${lx(500)}" y1="${T}" y2="${H - B}" class="lim"/><text x="${lx(500) + 4}" y="${T + 10}" class="t">Grenze 500 Flächen</text>`;
  const two = `<line x1="${L}" x2="${W - R}" y1="${ly(2)}" y2="${ly(2)}" class="lim2"/><text x="${L + 4}" y="${ly(2) - 4}" class="t">2 s</text>`;
  const lines = series
    .map((s) => {
      const d = s.pts.map(([x, y], i) => `${i ? "L" : "M"}${lx(x).toFixed(1)},${ly(y).toFixed(1)}`).join(" ");
      const dots = s.pts.map(([x, y]) => `<circle cx="${lx(x).toFixed(1)}" cy="${ly(y).toFixed(1)}" r="4" fill="${s.color}" stroke="#fff" stroke-width="2"><title>${esc(s.name)}: ${de(x)} Flächen, ${de(y, y < 1 ? 3 : 1)} s</title></circle>`).join("");
      const [ex, ey] = s.pts[s.pts.length - 1] ?? [0, 0];
      return `<path d="${d}" fill="none" stroke="${s.color}" stroke-width="2"/>${dots}<text x="${lx(ex) + 8}" y="${ly(ey) + 4}" class="lbl">${esc(s.name)}</text>`;
    })
    .join("");
  const legend = series.map((s, i) => `<g transform="translate(${L + i * 190},${H - 6})"><line x1="0" x2="18" y1="-4" y2="-4" stroke="${s.color}" stroke-width="2"/><circle cx="9" cy="-4" r="4" fill="${s.color}" stroke="#fff" stroke-width="2"/><text x="24" y="0" class="t">${esc(s.name)}</text></g>`).join("");
  return `<figure class="chart"><svg viewBox="0 0 ${W} ${H}" width="100%" role="img" aria-label="Laufzeit des STL-Imports über der Flächenzahl">
<style>.g{stroke:#e4e6ea;stroke-width:1}.t{font:10px system-ui,sans-serif;fill:#5d646e}.lbl{font:10.5px system-ui,sans-serif;fill:#1f2328}.lim{stroke:#1f2328;stroke-width:1;stroke-dasharray:4 3}.lim2{stroke:#9aa0a8;stroke-width:1;stroke-dasharray:2 3}</style>
${grid.join("")}<line x1="${L}" x2="${W - R}" y1="${H - B}" y2="${H - B}" stroke="#9aa0a8"/><line x1="${L}" x2="${L}" y1="${T}" y2="${H - B}" stroke="#9aa0a8"/>
${limit}${two}${lines}
<text x="${(L + W - R) / 2}" y="${H - B + 31}" text-anchor="middle" class="t">Flächen nach dem Zusammenfassen (logarithmisch)</text>
<text transform="translate(14,${(T + H - B) / 2}) rotate(-90)" text-anchor="middle" class="t">Sekunden (logarithmisch)</text>
${legend}</svg><figcaption>Kernel-Zeit des STL-Imports (UV-Kugeln r 20, Median aus ${bench.spheres?.[0]?.solid?.runs ?? "–"} Läufen, ab 2500 Flächen 1 Lauf) über der Flächenzahl: B-Rep-Aufbau (<code>as = "solid"</code>), derselbe Körper mit einem Boolean (Zylinder durch beide Pole) und als Netz (Parsen, Aufbereiten und Kernel zusammen). Über 5000 Dreiecken nur als Netz gemessen. Werte in der Tabelle darunter („Sprache“ = Parsen und Aufbereiten des STL, „Anzeige“ = Anzeige-Netz und Bild).</figcaption></figure>`;
}

// ---- phase C: asserts
const assertsOk = asserts.filter((r) => r.ok).length;
const PROB = { overlap: "Schnitt", too_close: "zu nah", apart: "getrennt", gap: "Spalt", line_contact: "nur Linienkontakt", point_contact: "nur Punktkontakt" };
const KEY = { volume: "mm³", distance: "mm", contact_area: "mm²" };
const assertsTable = `<table class="narrow"><tr><th>Fall</th><th>Aufbau</th><th>erwartet</th><th>Ergebnis</th><th class="n">Wert</th><th>Soll</th><th class="n">rel. Abw.</th><th class="n">Prüfung</th><th class="n">Aufruf</th></tr>${asserts
  .map((r) => {
    const res = r.problem ? PROB[r.problem] ?? r.problem : `ok${r.contact ? ` (${{ face: "Fläche", line: "Linie", point: "Punkt" }[r.contact] ?? r.contact}${r.exact ? ", exakt" : ""})` : ""}`;
    const val = r.value === null || r.value === undefined ? "–" : `${de(r.value, Math.abs(r.value) < 1 ? 7 : 4)} ${KEY[r.key] ?? ""}`;
    return `<tr><td>${esc(r.id)}</td><td>${esc(r.title)}</td><td>${esc(r.expected ? PROB[r.expected] ?? r.expected : "ok")}</td><td class="${r.ok ? "ok" : "bad"}">${esc(res)}</td><td class="n">${val}</td><td>${esc(r.formula)}${r.want !== null && r.want !== undefined ? ` = ${de(r.want, 4)}` : ""}</td><td class="n">${r.rel === null || r.rel === undefined ? "–" : sci(r.rel)}</td><td class="n">${de(r.ms_assert, 1)} ms</td><td class="n">${de(r.ms_total)} ms</td></tr>`;
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
const pct = (a, b) => `${b >= a ? "+" : "−"}${de((Math.abs(b - a) / a) * 100, 1)} %`;
const wasmRow = (name, k) =>
  `<tr><td><code>${name}</code></td><td class="n">${de(WASM_BEFORE[k].raw)}</td><td class="n">${de(WASM_C[k].raw)}</td><td class="n">${wasmNow?.[k] ? de(wasmNow[k].raw) : "–"}</td><td class="n">${wasmNow?.[k] ? pct(WASM_BEFORE[k].raw, wasmNow[k].raw) : "–"}</td><td class="n">${de(WASM_BEFORE[k].gz)} → ${wasmNow?.[k] ? de(wasmNow[k].gz) : "–"}</td></tr>`;

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
  ["D1", "Kernel-Fehler mit Ort", "d1", "Die Meldung nennt beide Operanden mit Zeile und die Stelle; JSON `at` und `nodes`; render und UI markieren die Stelle."],
  ["D2", "`-D` ohne Modulkontext", "d2", "Setzt Zuweisungen auf oberster Ebene wie `openscad -D`; unbekannter Name → Warnung."],
  ["D3", "STEP mit getrennten Körpern", "d3", "Ein benannter Körper je Bauteil bzw. Objekt der obersten Ebene; `--merge` schreibt einen Körper wie früher."],
  ["D5", "`slot(p1, p2, r)`", "d5", "Langloch als exakte 2D-Grundform (vorher unbekanntes Modul, Platte ohne Loch)."],
  ["D7", "`-m 'modul(args)'`", "d7", "Modulkontext mit Argumenten wie im Code, verschachtelt `teile.schraube(d = 3)`; Fehler mit Exit 2."],
];
const feedbackRows = feedback
  .map(([id, title, key, what]) => `<tr><td>${esc(id)}</td><td>${md(title)}<br><span class="small">${md(what)}</span></td><td>${term(dTxt(`${key}-vorher`) ?? "(fehlt)")}</td><td>${term(dTxt(`${key}-nachher`) ?? "(fehlt)")}</td></tr>`)
  .join("");

// ---- test runs
const tauriRows = (tauri?.results ?? []).map((r) => `<tr><td>${esc(r.id)}</td><td>${esc(r.title)}</td><td class="${r.ok ? "ok" : "bad"}">${r.ok ? "bestanden" : "fehlgeschlagen"}</td><td class="n">${de(r.ms / 1000, 1)} s</td></tr>`).join("");
const t19 = tauri?.results?.find((r) => r.id === "T19");
const t20 = tauri?.results?.find((r) => r.id === "T20");
const resCell = (ok, text) => `<span class="${ok ? "ok" : "bad"}">${esc(text)}</span>`;
const pwFiles = [...new Set(pwSpecs.map((t) => t.file))];
const pwNew = pwSpecs.filter((t) => /(asserts|phase-d)\.spec\.ts$/.test(t.file) || /^B12/.test(t.title));
const testRows = [
  ["`cargo fmt --all --check`", resCell(fmtOk, fmtOk ? "sauber" : "Abweichung"), dur("fmt")],
  ["`cargo test --workspace --no-fail-fast` (mit Tauri/WebKitGTK, `CARGO_INCREMENTAL=0`)", cargoSum ? resCell(cargoSum.failed === 0 && rcOk("cargo"), `${cargoSum.ok} bestanden, ${cargoSum.failed} fehlgeschlagen, ${cargoSum.ignored} ignoriert (${cargoSum.suites} Test-Binaries)${cargoFailed.length ? `: ${cargoFailed.join(", ")}` : ""}`) : "–", dur("cargo")],
  ["`cargo clippy --workspace --all-targets -- -D warnings` und `-p cad-wasm --target wasm32-unknown-unknown`", resCell(clippyOk, clippyOk ? "sauber" : "Befund"), `${dur("clippy")} + ${dur("clippy-wasm")}`],
  ["`cargo check -p kernel-solidcore --features solidcore`", resCell(rcOk("solidcore"), rcOk("solidcore") ? "ok" : "Fehler"), dur("solidcore")],
  ["`cd web && pnpm test` (Vitest)", vitestSum ? resCell(vitestSum.failed === 0 && rcOk("vitest"), `${vitestSum.ok} bestanden, ${vitestSum.failed} fehlgeschlagen`) : "–", dur("vitest")],
  ["`pnpm typecheck`", resCell(typecheckOk, typecheckOk ? "sauber" : "Fehler"), dur("typecheck")],
  ["`pnpm e2e` (Playwright 1.56.1, Chromium mit SwiftShader, ganze Suite, JSON-Reporter)", pwStats ? resCell(pwStats.unexpected === 0, `${pwStats.expected} bestanden, ${pwStats.unexpected} fehlgeschlagen${pwStats.flaky ? `, ${pwStats.flaky} unbeständig` : ""}${pwStats.skipped ? `, ${pwStats.skipped} übersprungen` : ""} (${pwFiles.length} Dateien)${pwBad.length ? `: ${pwBad.map((t) => t.title.split(" ")[0]).join(", ")}` : ""}`) : "–", pwStats ? `${de(pwStats.duration / 60000, 1)} min` : dur("playwright")],
  ["`node web/e2e-tauri/desktop.e2e.mjs --bin target/release/mcad` (tauri-driver, Xvfb)", tauri ? resCell(tauriOk === tauri.results.length, `${tauriOk} / ${tauri.results.length} bestanden${tauri.results.filter((r) => !r.ok).length ? `: ${tauri.results.filter((r) => !r.ok).map((r) => r.id).join(", ")} rot` : ""}`) : "–", dur("tauri")],
  ["`MCAD=target/release/mcad python3 tests/openscad-compat/compare.py --eval`", compatSum ? resCell(compatSum[1] === compatSum[2] && rcOk("compat"), `${compatSum[1]} / ${compatSum[2]}`) : "–", dur("compat")],
  ["`tests/step/verify.py` (OCCT, cadquery-ocp 8.0.1.1, `--merge` gegen die Wurzel)", stepSum ? resCell(stepSum[1] === stepSum[2], `${stepSum[1]} / ${stepSum[2]}`) : "–", dur("step")],
  ["`tests/step/verify.py --bodies` (neuer Standard: Körper je Bauteil/Objekt)", stepBodiesSum ? resCell(stepBodiesSum[1] === stepBodiesSum[2], `${stepBodiesSum[1]} / ${stepBodiesSum[2]}`) : "–", dur("step-bodies")],
  ["`tests/alltag/check.sh -j 4` (Alltagsmodelle: check, Bauteile, Export)", alltagLast ? resCell(rcOk("alltag"), alltagLast) : "–", dur("alltag")],
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
<html lang="de">
<head>
<meta charset="utf-8">
<title>Testbericht 21 – hull/minkowski, STL-Import, Asserts, Nutzer-Feedback, mcad</title>
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
<h1>Testbericht 21 – hull/minkowski, STL-Import, Kollisions-Asserts, Nutzer-Feedback, Umbenennung in mcad</h1>
<div class="meta">Plan 21 (Phasen A–C), Nutzer-Feedback (Phase D), Umbenennung (Phase E): alle Commits seit <code>852fbc1</code> · ${esc(date)} · Branch ${esc(branch)} · Commit ${esc(commit)} · mcad 0.3.0 · ${esc(rustc)} · ${os.cpus().length} Kerne. Messwerte mit dem Release-Binary ohne Daemon (<code>tools/report/report21-data.py</code>), Testläufe am selben Commit.</div>

<div class="summary">
  <div>hull/minkowski gegen Formel<b class="${hullOk === hullWithFormula && hullWithFormula ? "ok" : "bad"}">${hullOk} / ${hullWithFormula}</b>innerhalb 10⁻⁵ relativ; ${hullWarn} Fälle ohne Sonderfall → Warnung</div>
  <div>STL: B-Rep bis 500 Flächen<b>${r445?.solid ? `${de((r445.solid.kernel_ms + r445.solid.bool_ms) / 1000, 1)} s` : "–"}</b>Aufbau + Boolean bei ${r445 ? de(r445.faces) : "–"} Flächen; ${r4646?.solid ? `${de(r4646.faces)} Flächen: ${de(r4646.solid.bool_ms / 1000)} s` : ""}</div>
  <div>Assert-Fälle<b class="${assertsOk === asserts.length && asserts.length ? "ok" : "bad"}">${assertsOk} / ${asserts.length}</b>Art und Wert wie erwartet; Export bei Verletzung Exit 4</div>
  <div>cargo test --workspace<b class="${cargoSum && cargoSum.failed === 0 ? "ok" : "bad"}">${cargoSum ? `${cargoSum.ok} / ${cargoSum.ok + cargoSum.failed}` : "–"}</b>clippy ${clippyOk ? "sauber" : "Befund"}, fmt ${fmtOk ? "sauber" : "Abweichung"}</div>
  <div>Vitest · Playwright<b class="${vitestSum?.failed === 0 && pwStats?.unexpected === 0 ? "ok" : "bad"}">${vitestSum ? vitestSum.ok : "–"} · ${pwStats ? `${pwStats.expected} / ${pwStats.expected + pwStats.unexpected + pwStats.flaky}` : "–"}</b>Browser mit Wasm-Kernel</div>
  <div>Tauri-E2E<b class="${tauri && tauriOk === tauri.results.length ? "ok" : "bad"}">${tauri ? `${tauriOk} / ${tauri.results.length}` : "–"}</b>Desktop-App mit nativem Kernel</div>
  <div>OpenSCAD · STEP · Alltag<b class="${compatSum && stepSum && rcOk("alltag") && compatSum[1] === compatSum[2] && stepSum[1] === stepSum[2] ? "ok" : "bad"}">${compatSum ? `${compatSum[1]}/${compatSum[2]}` : "–"} · ${stepSum ? `${stepSum[1]}/${stepSum[2]}` : "–"} · ${rcOk("alltag") ? "ok" : "Befund"}</b>Vergleich, OCCT-Prüfung, 19 Modelle</div>
</div>

<h2>1 Umfang</h2>
<table><tr><th>Phase</th><th>Commit</th><th>Titel</th></tr>${commitRows}</table>
<p class="small">Entscheidungen des Nutzers für Plan 21: <code>hull</code>/<code>minkowski</code> nur in exakten Sonderfällen, sonst wie bisher eine Warnung mit unveränderten Kindern (keine Näherung); STL-Import mit automatischer Grenze B-Rep/Netz und Override <code>as = "solid" | "mesh"</code>; Asserts: Verletzung = Warnung, <code>mcad export</code> bricht ab (außer <code>--ignore-asserts</code>), <code>assert_keepout</code> in der UI gut sichtbar.</p>

<h2>2 Phase A: Offset, hull, minkowski</h2>
<h3>Kernel-Offset (Spike)</h3>
<p><code>Kernel::offset(s, d, tag)</code> ist die Minkowski-Summe eines Körpers mit einer Kugel: Fassade <code>sc_offset</code> über <code>SmApiOffsetBrepFull</code> (nach außen, Rundung, Selbstschnitt-Behandlung). Der Spike (Stand <code>83ee5b3</code>, Volumen über die Massen-Eigenschaften gegen die Steiner-Formel) zeigt, wo SMLib exakt ist: an konvexen Kanten entstehen Zylinder, an Ecken Kugeln, solange die Flächen an den Ecken rechtwinklig aufeinandertreffen. Am Tetraeder liegen die Flächen richtig (4 Ebenen, 6 Zylinder, 4 Kugeln mit exakten Mittelpunkten und Radien), die Trimmkurven aber bis 0,33 mm neben den Kanten (Volumen +4,5·10⁻⁴ gegen Steiner); am 45°-Keil scheitert SMLib. Die Fassade prüft deshalb den Abstand des Ergebnisrands zum Körper (je Kante 9 Punkte, Flächenmitte) und die Trimmkurven gegen die 3D-Kanten (10⁻⁶ der Größe) und meldet sonst „offset is not exact“.</p>
${spikeTable}
<p class="small">Spalte „Ersatz“: Flächen-Offset (Flächen verlängert) und danach alle Kanten mit r verrundet – an keinem Körper außer Zylinder und Quader gedreht tragfähig, daher verworfen. Rohdaten <code>reports/tmp/report21/phase-a/offset-spike.txt</code>.</p>

<h3>Sprache und Beispiel</h3>
<p><code>hull()</code> und <code>minkowski()</code> wählen den Fall nach den Kindern (Transformationen, Gruppen, Vereinigungen und geschachtelte Hüllen werden durchgesehen, Kugeln/Zylinder/Bögen nur unter Ähnlichkeitsabbildungen). Exakte Sonderfälle:</p>
<table>
<tr><th>Operation</th><th>Sonderfall</th><th>Ergebnis</th></tr>
<tr><td><code>hull</code> 2D</td><td>jede Region aus Geraden und Kreisbögen (Text, SVG: B-Splines über Biarcs)</td><td>Geraden + Bögen über die Stützfunktion, Übergänge an den gemeinsamen Tangenten in geschlossener Form</td></tr>
<tr><td><code>minkowski</code> 2D</td><td>Profil + Kreis; zwei konvexe Profile (eines ein Polygon)</td><td>Offset bzw. konvexe Summe</td></tr>
<tr><td><code>hull</code> 3D</td><td>ebene Körper; Kugeln/Zylinder/Kegel auf einer Achse; parallele Prismen gleicher Höhe; gleich große Kugeln</td><td>Polyeder; Rotationskörper der 2D-Hülle der Halbschnitte; extrudierte 2D-Hülle; Hülle der Mittelpunkte + Offset r</td></tr>
<tr><td><code>minkowski</code> 3D</td><td>Körper + Kugel; <code>linear_extrude</code> + <code>cylinder</code> (parallele Prismen); zwei konvexe ebene Körper; ein Kind</td><td>Offset; Prisma der 2D-Summe; Polyeder; das Kind</td></tr>
<tr><td>sonst</td><td>z. B. Kugeln verschiedener Radien nicht auf einer Achse, Tetraeder + Kugel, Torus, <code>$fillet</code>-Primitive</td><td>Warnung „hull: &lt;Grund&gt;; Kinder werden unverändert gezeigt“</td></tr>
</table>
<div class="codefig"><div>${pre((readText(join(root, "examples/spec/hull_minkowski.mcad")) ?? "").split("\n").filter((l) => !l.startsWith("//")).join("\n"))}${term(readText(join(data, "a/beispiel_check.txt")))}</div><div>${fig(dimg("a", "beispiel_iso.png", 1000), "`examples/spec/hull_minkowski.mcad`, `mcad render` (iso): Platte als 2D-Hülle von vier Kreisen mit Langlöchern, abgerundeter Block (Würfel ⊕ Kugel), Griff als Kapsel, Lasche (Quader + Zylinder gleicher Höhe), Kissen aus acht gleich großen Kugeln, Leiste aus `linear_extrude` ⊕ `cylinder`")}</div></div>

<h3>Messwerte gegen Formel</h3>
<p>Volumen über <code>mcad measure --volume</code> (exakte B-Rep, Massen-Eigenschaften des Kernels); 2D-Fälle um 1 extrudiert, das Volumen ist die Fläche. Zeit = ganzer CLI-Aufruf ohne Daemon (Parsen, Auswertung, Kernel, Volumen), Median aus 3 Läufen.</p>
${hullTable}
<p class="small">Abweichungen: 2D-Hüllen und Prismen ≤ 10⁻⁸ (Integration); Rotationskörper (Kapsel, Kugeln, Kuppel) 10⁻⁷–10⁻⁶ – Genauigkeit der Volumenintegration von Rotationsflächen, die Geometrie selbst ist exakt (STEP: <code>SURFACE_OF_REVOLUTION</code>, keine B-Splines); Würfel + „Punkt“: der Punkt ist ein Würfel mit 10⁻⁴ Kantenlänge (+2,9·10⁻⁶ ist dessen Beitrag).</p>
${hullGrid}

<h3>Grenzen</h3>
<ul>
<li>Offset (und damit <code>minkowski</code> mit Kugel) nur an Körpern, deren Flächen sich an den Ecken rechtwinklig treffen; Tetraeder, Keile, schräge Polyeder → Warnung, Kinder unverändert.</li>
<li>Gleich große Kugeln mit Mittelpunkten in einer Ebene (abgerundete Platte) sind kein Sonderfall (die Hülle der Mittelpunkte ist flach, ein Offset davon geht nicht) – Fall „4 gleiche Kugeln in einer Ebene“ oben; Umweg: <code>linear_extrude</code> einer 2D-Hülle mit <code>$fillet</code>.</li>
<li><code>rotate_extrude</code>-Kinder, Torus und <code>$fillet</code>-Primitive in <code>hull</code>: Warnung.</li>
</ul>

<h2>3 Phase B: <code>import("x.stl")</code></h2>
<h3>Sprache und Modi</h3>
${pre(`import("../stl/winkel.stl");                     // geschlossen, ≤ 500 Flächen: B-Rep (Booleans gehen)
import("platine.stl", as = "mesh");              // reines Netz (Anzeige, Export, Messen)
import("teil.stl", center = true, scale = 25.4); // Zoll → mm, Hülle um den Ursprung`)}
<table>
<tr><th>Modus</th><th>automatisch, wenn</th><th>geht</th><th>geht nicht</th></tr>
<tr><td>B-Rep (<code>solid</code>)</td><td>geschlossen, mannigfaltig, ≤ 500 Flächen nach dem Zusammenfassen</td><td>alles: Booleans, <code>fillet</code>/<code>$fillet</code>, exaktes Messen, STEP</td><td>–</td></tr>
<tr><td>Netz (<code>mesh</code>)</td><td>&gt; 500 Flächen, offen, nicht mannigfaltig</td><td>Anzeige, Transformationen, <code>color</code>, Bauteile, <code>hull</code> (≤ 2000 Ecken), STL/3MF, STEP facettiert, Hülle/Volumen/Fläche/Schnitt, <code>--clearance</code>, <code>--interference</code> (ja/nein)</td><td>Booleans, <code>fillet</code>/<code>chamfer</code>/<code>shell</code>, <code>minkowski</code>, <code>thicken</code> – Fehler bzw. Warnung mit Hinweis <code>as = "solid"</code></td></tr>
</table>
<p>Aufbereitung in jedem Modus (<code>cad-stl</code>): Ecken verschweißen (10⁻⁶ der Diagonale), NaN-, entartete und doppelte Dreiecke verwerfen, Orientierung einheitlich nach außen, koplanare Nachbarn zu ebenen Flächen zusammenfassen, Ecken auf die Ebenen ihrer Flächen ziehen. ASCII und binär (binär an der Größe 84 + 50·n erkannt, auch mit „solid“-Kopf).</p>
<div class="three">${fig(dimg("b", "beispiel.png", 900), "`examples/spec/stl_import.mcad`: derselbe Winkel als B-Rep mit Bohrung (blau) und gespiegelt als Netz (orange)")}${fig(dimg("b", "solid_kugel1000_boolean.png", 700), "Kugel-STL (968 Dreiecke, 890 Flächen) mit `as = \"solid\"` minus Zylinder")}${fig(dimg("b", "netz_kugel20000.png", 700), "Kugel mit 20 000 Dreiecken automatisch als Netz (Kanten erst ab 20°)")}</div>${term((readText(join(data, "b/stl_import_inspect.txt")) ?? "").split("\n").filter((l) => /^\$|^Z\.|Hinweis/.test(l)).join("\n"))}

<h3>Laufzeit und Grenze 500 Flächen</h3>
${stlChart()}
${benchTable}
${realTable}
<p>Bis 500 Flächen bleibt Import + ein Boolean bei ${r445?.solid ? `${de((r445.solid.kernel_ms + r445.solid.bool_ms) / 1000, 1)} s` : "–"} (${r445 ? de(r445.faces) : "–"} Flächen). Darüber wächst die Zeit des Booleans überlinear (${r890?.solid ? `${de(r890.faces)} Flächen ${de(r890.solid.bool_ms / 1000, 1)} s` : ""}${r4646?.solid ? `, ${de(r4646.faces)} Flächen ${de(r4646.solid.bool_ms / 1000)} s` : ""}); die Zeiten streuen zwischen benachbarten Größen, weil die Zahl der Schnittkanten an den Polen von der Teilung der Kugel abhängt. Als Netz kostet der Kernel nichts; Parsen und Aufbereiten von ${rMesh ? de(rMesh.triangles) : "–"} Dreiecken ${rMesh?.mesh ? de(rMesh.mesh.eval_ms) : "–"} ms, Anzeige-Netz und Bild ≈ 0,2 s unabhängig von der Größe. Realistische Exporte liegen knapp über der Grenze (mehr Flächen als eine Kugel gleicher Dreieckszahl wegen der Bohrungen) und werden automatisch Netz; mit <code>as = "solid"</code> brauchen Aufbau und ein Boolean 1–1,5 s.</p>

<h3>Kombiniertes Modell: Gehäuse aus STL, Platine als Netz</h3>
<div class="two">${fig(dimg("b", "kombi_schnitt.png", 900), "Schnitt y = 10 (`--section-flip`): Gehäuse (STL als B-Rep, zwei Bohrungen), „Platine“ (Kugel mit 20 000 Dreiecken, skaliert, als Netz) und Deckel; Kappen auch am Netz")}${fig(dimg("b", "kombi_xray.png", 900), "Röntgen: dasselbe Modell")}</div>
${term(readText(join(data, "b/kombi_measure.txt")))}
<h3>Grenzen</h3>
<ul>
<li>Kollisionen mit Netzen nur ja/nein (Durchdringung oder Enthaltensein), ohne Schnittvolumen; Abstände über die Dreiecke.</li>
<li>STEP eines Netzes ist facettiert (eine Ebene je Fläche); Booleans und Verrundungen brauchen <code>as = "solid"</code> und sind ab einigen tausend Flächen langsam (Kurve oben).</li>
</ul>

<h2>4 Editor: keine Inline-Schieberegler für Zahlen</h2>
<p>Commit <code>2d3451e</code>: der Schieberegler am Zeilenende von Zahl-Zuweisungen störte beim Lesen und Schreiben und ist entfernt. Bool-Literale behalten ihren Schalter; Zahlen ändern sich weiter über die Parameterleiste und die Griffe. Playwright B12 prüft, dass kein <code>inline-slider</code> mehr erscheint.</p>
<div class="stack">${fig(img(join(root, "docs/reports/03-ui-phase-b/img/b12-nachher.jpg"), "editor-vorher.jpg", 700), "Vorher (Bericht 3, B12): `t = 7;` mit Schieberegler am Zeilenende")}${fig(img(join(root, "reports/tmp/report3/img/b12-vorher.jpg"), "editor-nachher.jpg", 900) ?? img(join(root, "reports/tmp/report3/img/b12-vorher.png"), "editor-nachher.jpg", 900), "Nachher (B12, finaler Lauf): nur noch der Schalter an `hohl = true;`")}</div>

<h2>5 Phase C: Kollisions-Asserts</h2>
<h3>Sprache</h3>
<div class="codefig"><div>${pre((readText(join(root, "examples/spec/asserts.mcad")) ?? "").split("\n").filter((l) => !l.startsWith("//")).join("\n").trim())}</div><div>
<table><tr><th>Assert</th><th>Verletzung (Warnung)</th></tr>
<tr><td><code>assert_keepout(clearance = 0)</code></td><td>ein echter Körper schneidet das Sperrvolumen (Volumen &gt; ε) oder ist näher als <code>clearance</code>; das Sperrvolumen wird gezeigt, aber nicht exportiert, gezählt oder verrechnet</td></tr>
<tr><td><code>assert_disjoint()</code></td><td>ein Paar der Kinder schneidet sich</td></tr>
<tr><td><code>assert_intersect()</code></td><td>die Kinder bilden keine zusammenhängende Gruppe sich schneidender Körper (Berühren ist kein Schnitt)</td></tr>
<tr><td><code>assert_touches(tol, contact)</code></td><td>Spalt &gt; tol, Überschneidung, oder nur Linien-/Punktkontakt bei <code>contact = "face"</code></td></tr>
</table>
<p class="small">Asserts sind Gruppen ihrer Kinder und lassen sich schachteln. Schnitte exakt per Boolean je Körperpaar (Volumen aus den Massen-Eigenschaften), Kontaktflächen exakt über <code>region2</code> für Geraden und Bögen, Abstände über die Tessellierung mit Fußpunkten; Paare im Engine-Cache unter Inhaltsschlüsseln.</p></div></div>

<h3>Testfälle gegen Formel</h3>
<p><code>mcad check --json</code> je Fall (eigener Prozess, kalter Cache, Median aus 3 Läufen). „Prüfung“ = Zeit der Assert-Prüfung, „Aufruf“ = ganzer CLI-Aufruf.</p>
${assertsTable}
<p class="small">Abstände (K2, T7) kommen aus der f32-Tessellierung und weichen um ≈ 10⁻⁷ relativ ab; Volumen und Kontaktflächen sind exakt. Die Kugel (K5, T6) kostet ≈ 0,2 s für den Boolean mit der Kugelfläche.</p>

<h3>Export, CLI-Bilder</h3>
${term(readText(join(data, "c/export.txt")))}
<div class="two">${fig(dimg("c", "asserts-ok-hell.png", 900), "`mcad render asserts.mcad --show asserts`: Kontakt Deckel/Gehäuse 384 mm² grün, gewollter Schnitt Halter/Kabelführung grün, Sperrvolumen gelb gestrichelt")}${fig(dimg("c", "asserts-verletzt-dunkel.png", 900), "Verletzung (`luft = 3`, Halter zu hoch), `--xray --theme dark`: Schnitt rot, Abstand „2 mm < 3“ als Linie mit Fußpunkten")}</div>
${fig(dimg("c", "faelle-hell.png", 1100, true), "`faelle.mcad --show asserts`: alle Arten nebeneinander – Keepout (Schnitt), disjoint (Schnitt 200 mm³), intersect (getrennt), Flächenkontakt 25 mm², Linienkontakt, Spalt 0,2 mm")}

<h3>UI (Browser, Wasm-Kernel)</h3>
<p>Sperrvolumen gelb schraffiert und durch das Modell sichtbar, nicht wählbar oder messbar, Schalter „Sperrvolumen“ (gemerkt); Schnittkörper einer Verletzung rot, Kontakt grün, Spalt als Maßlinie; Statusleiste „n Assert-Verletzungen“, ein Klick darauf oder auf die Warnung am Zeilenende rahmt die Verletzung ein; jeder Export fragt bei Verletzung nach. Bilder aus dem finalen Playwright-Lauf (C01–C05).</p>
<div class="two">${fig(uiC("c01-keepout-ok-hell"), "C01 hell: Sperrvolumen (STL-Winkel) schraffiert, Kontakt 384 mm²")}${fig(uiC("c01-keepout-ok-dunkel"), "C01 dunkel")}</div>
<div class="two">${fig(uiC("c02-keepout-verletzt-hell"), "C02: Verletzung, Schnittkörper rot mit „Schnitt 180 mm³“")}${fig(uiC("c02-keepout-verletzt-eingerahmt-dunkel"), "C02 dunkel: nach Klick auf „1 Assert-Verletzung“ eingerahmt")}</div>
<div class="two">${fig(uiC("c04-export-dialog-hell", 1100), "C04: „Export trotz Assert-Verletzung?“ mit Liste")}${fig(uiC("c05-touches-dunkel"), "C05 dunkel: Kontaktfläche 25 mm² grün, Spalt 0,2 mm mit Maßlinie")}</div>
${t19 ? `<h3>Desktop-App (Tauri T19)</h3><p>T19 ${t19.ok ? '<span class="ok">bestanden</span>' : '<span class="bad">fehlgeschlagen</span>'} (${de(t19.ms / 1000, 1)} s): ${t19.checks.map((c) => md(num(c.text))).join("; ")}.</p><div class="two">${(t19.images ?? []).slice(0, 2).map((i) => fig(tauriImg(i.name), `T19: ${i.label}`)).join("")}</div>` : ""}

<h3>Wasm-Build</h3>
<p>emsdk 6.0.13 und wasm-bindgen-cli 0.2.129 ließen sich im Container einrichten (Downloads über den Proxy); <code>smlib.wasm</code> neu gebaut (≈ 5 min, 244 Ziele, mit <code>sc_offset</code> und den Kontaktstellen aus Phase D), <code>cad-wasm</code> ≈ 65 s. Größen nach <code>wasm-opt -O3</code>:</p>
<table><tr><th>Datei</th><th class="n">vor Phase A (852fbc1)</th><th class="n">nach Phase C</th><th class="n">jetzt</th><th class="n">Δ zu vorher</th><th class="n">gzip -9 vorher → jetzt</th></tr>${wasmRow("cad_wasm_bg.wasm", "cad")}${wasmRow("smlib.wasm", "smlib")}</table>
<p class="small">Werte vor Phase A und nach Phase C aus dem Material der Phase C (gleiche Werkzeuge, alter Stand als eigener Build); „jetzt“ = <code>web/src/wasm-pkg</code> des finalen Stands.</p>

<h3>Grenzen</h3>
<ul>
<li>Kollision mit Netzen nur ja/nein, ohne Volumen (K6).</li>
<li>Die Keepout-Prüfung rechnet Booleans im Worker: an einer Lochplatte 120×80 mit 24 Bohrungen und Verrundung ≈ 2,5 s beim ersten Mal (danach aus dem Cache); so lange ist der Worker belegt.</li>
<li>Punktkontakt (Kugel auf Platte) wird erkannt, aber ohne Ort gemeldet und gezeichnet.</li>
</ul>

<h2>6 Phase D: Nutzer-Feedback</h2>
<p>Je Punkt derselbe Aufruf mit dem Release-Build vor Phase D (<code>cad</code> 0.2.1) und dem finalen Stand (<code>mcad</code> 0.3.0); Eingaben als <code>.scad</code>, weil der alte Build <code>.mcad</code> nicht kennt.</p>
<table class="fb"><tr><th>Nr.</th><th>Punkt</th><th>vorher</th><th>nachher</th></tr>${feedbackRows}</table>

<h3>D1 Kernel-Fehler mit Ort</h3>
<p>Ist ein Boolean-Ergebnis kein geschlossener Körper, liefert die Fassade (Version 6) die nicht-mannigfaltigen Kanten als Polylinien; die Engine kennt den Knoten je Operand und nennt die berührenden Operanden. JSON-Diagnose mit <code>at</code> (Kontextkoordinaten) und <code>nodes</code>; <code>mcad render</code> markiert jede Stelle (Kanten rot, Punkt, „Berührung Z.15 + 16“), die UI ebenso, ein Klick auf die Meldung rahmt die Stelle ein.</p>
<div class="two">${fig(dimg("d", "dom_rippe-vorher.png", 800), "Vorher: `cad render dom_rippe.scad` ohne Markierung (und ohne `slot`)")}${fig(dimg("d", "dom_rippe-nachher.png", 800), "Nachher: Kontaktlinie rot mit „Berührung Z.15 + 16“")}</div>
<div class="two">${fig(dimg("d", "f08c-nachher.png", 800), "`f08c_differenz_torus_tangential`: Zylinder − Torus, beide Berührkreise rot")}${fig(uiD("d01-beruehrung-eingerahmt-dunkel"), "UI (Playwright D01, dunkel): Klick auf die Meldung rahmt die Stelle ein")}</div>

<h3>D3 STEP: Körper je Bauteil und Objekt</h3>
<p>Standard: ein <code>MANIFOLD_SOLID_BREP</code> je Bauteil (Name des Bauteils) und je Objekt der obersten Ebene außerhalb der Bauteile (Name des Objekts durch Transformationen hindurch); berührende Teile bleiben getrennt (vorher verschmolzen). Überschneiden sich Objekte (Summe der Volumen &gt; Volumen der Vereinigung), steht ein Hinweis im Export. <code>--merge</code> schreibt die Wurzel als einen Körper wie früher. Von 64 Beispielmodellen änderten ${stepChanged.length} ihre Körperzahl (Stand <code>8ac92d6</code>, OCCT gelesen):</p>
<table><tr><th>Modell</th><th class="n">vorher (<code>--merge</code>)</th><th class="n">nachher</th><th>Namen nachher</th></tr>${stepChanged.map((r) => `<tr><td><code>${esc(r.model)}</code></td><td class="n">${r.before}</td><td class="n">${r.after}</td><td class="small2">${esc(r.names.length > 90 ? `${r.names.slice(0, 90)} …` : r.names)}</td></tr>`).join("")}</table>
${stepOverlap.length ? `<p class="small">Mit Hinweis „Objekte überschneiden sich“ (finaler Lauf <code>verify.py --bodies</code>, Volumen dort nicht verglichen): ${stepOverlap.map((r) => `<code>${esc(r.model)}</code> (${r.solids} Körper, Summe ${de(r.vOcct, 1)} mm³ gegen ${de(r.vKernel, 1)} mm³ der Vereinigung)`).join(", ")} – siehe Grenzen.</p>` : ""}

<h3>D5 <code>slot(p1, p2, r)</code></h3>
<table><tr><th>Aufruf, um 1 extrudiert</th><th class="n">mcad</th><th class="n">πr² + 2r·|p2 − p1|</th><th class="n">rel. Abw.</th></tr>${slotAreas.map((m) => `<tr><td><code>${esc(m[1])}</code></td><td class="n">${de(+m[2], 4)}</td><td class="n">${de(+m[3], 4)}</td><td class="n">${sci(+m[4])}</td></tr>`).join("")}</table>
<p class="small">Fläche aus der Volumenintegration (Stand <code>91f8ef6</code>); die Kontur selbst besteht aus zwei Halbkreisen und zwei Geraden (exakte Bögen im Schnitt, siehe Tabelle oben). <code>p1 == p2</code> ergibt einen Kreis; Punkte auch als Rasterpunkte/Anker, <code>d = …</code> statt <code>r</code>; Griffe für p1, p2 und r.</p>
<div class="two">${fig(uiD("d02-slot-griffe"), "UI (D02): Griffe p1, p2 und r an `linear_extrude(2) slot(…)`")}${fig(uiD("d02-slot-radius-gezogen"), "D02: Radius gezogen, der Code folgt")}</div>

<h3>D6 JSON-Schema 2</h3>
<table><tr><th></th><th>vorher</th><th>nachher</th></tr>
<tr><td><code>mcad inspect fillet_zonen --line 12 --json</code></td><td><code>schema_version ${esc(d6.vorher?.schema_version ?? "–")}</code>, <code>selected_edges: ${esc(d6.vorher?.selected_edges ?? "–")}</code> (Zahl)</td><td><code>schema_version ${esc(d6.nachher?.schema_version ?? "–")}</code>, <code>selected_edges</code>: ${esc(d6.nachher?.selected_edges ?? "–")} (Weltkoordinaten), <code>selected_edge_count: ${esc(d6.nachher?.selected_edge_count ?? "–")}</code></td></tr>
<tr><td><code>outline</code></td><td>Rasterebene <code>axis</code> als Name, Literale/Bauteile uneinheitlich</td><td><code>axis</code> 0|1|2 wie in der Szene (+ <code>axis_name</code>), <code>variables[].literal</code> als Bereich|null, <code>instances[].part</code> als Name|null</td></tr>
</table>

<h2>7 Phase E: Umbenennung <code>cad</code> → <code>mcad</code></h2>
<p>Binary, Programmname, Meldungen und Manual heißen <code>mcad</code> (Version 0.3.0); neue Dateien bekommen <code>.mcad</code>, <code>.mcad</code> und <code>.scad</code> öffnen überall (CLI, <code>include</code>/<code>use</code>, Dialoge, Ordner-Projekte, Upload, ZIP, Beispielmenü). Inventar über <code>git grep</code> vor der Umbenennung: 1703 Treffer für <code>-w cad</code> außerhalb der Berichte, 187 <code>.scad</code>-Dateien.</p>
<table><tr><th>Bereich</th><th>vorher</th><th>nachher</th></tr>
<tr><td>Binary, Meldungen, Manual, Skill</td><td><code>cad</code>, <code>cad check: …</code></td><td><code>mcad</code>, <code>mcad check: …</code>; Test <code>usage::no_old_program_name</code></td></tr>
<tr><td>Umgebung, Konfiguration, Socket</td><td><code>CAD_*</code>, <code>~/.config/controller-cad/</code>, <code>$XDG_RUNTIME_DIR/controller-cad</code></td><td><code>MCAD_*</code> (alte Namen als Rückfall), <code>~/.config/mcad/</code> (alte Datei gilt, wenn die neue fehlt), <code>$XDG_RUNTIME_DIR/mcad</code></td></tr>
<tr><td>Desktop-App</td><td>productName <code>controller-cad</code>, App-ID <code>dev.controller-cad.desktop</code></td><td><code>mcad</code>, <code>dev.mcad.desktop</code>, Fenstertitel „… – mcad“; Liste zuletzt geöffneter Dateien einmal übernommen</td></tr>
<tr><td>Dateitypen, Paket</td><td>Desktop-Datei mit <code>application/x-openscad</code>, Arch-Paket <code>controller-cad</code></td><td><code>text/x-mcad</code> und <code>application/x-openscad</code> (<code>packaging/linux/mcad.xml</code>), Paket <code>mcad</code> mit <code>replaces=('controller-cad')</code></td></tr>
<tr><td>Beispiele, Testdaten</td><td>94 Dateien <code>*.scad</code></td><td><code>*.mcad</code> per <code>git mv</code>, Verweise nachgezogen</td></tr>
<tr><td>STEP/3MF-Metadaten, Web-Titel</td><td><code>controller-cad</code>, „Parametrisches CAD“</td><td><code>mcad</code>, „mcad – Parametrisches CAD“</td></tr>
</table>
<table><tr><th>bleibt</th><th>Grund</th></tr>
<tr><td>Crate-Namen <code>cad-*</code>, Skill-Ordner <code>cad-cli</code></td><td>intern; Pfade in Briefs und Werkzeugen</td></tr>
<tr><td>Repository <code>controller-cad</code>, Docker-Image/Volume der Arch-Bauumgebung</td><td>Name des Repositorys und der Bau-Infrastruktur</td></tr>
<tr><td>IndexedDB-Name <code>controller-cad</code> (Browser)</td><td>dort liegen die gespeicherten Projekte der Nutzer</td></tr>
<tr><td>Schriftname „cad stencil mono“</td><td>steht in Modellen</td></tr>
<tr><td><code>examples/openscad-compat/*.scad</code>, <code>tests/openscad-compat</code>, <code>tests/farben-pruefung</code>, Fallstudie Bericht 8</td><td>werden mit OpenSCAD verglichen bzw. sind Momentaufnahmen; prüfen nebenbei, dass <code>.scad</code> weiter geht</td></tr>
<tr><td>Berichte <code>docs/reports/**</code></td><td>Momentaufnahmen</td></tr>
</table>
${term(readText(join(data, "e/dateien.txt")))}
${t20 ? `<p>Tauri T20 ${t20.ok ? '<span class="ok">bestanden</span>' : '<span class="bad">fehlgeschlagen</span>'}: ${t20.checks.map((c) => md(num(c.text))).join("; ")}.</p><div class="two">${(t20.images ?? []).slice(0, 2).map((i) => fig(tauriImg(i.name), `T20: ${i.label}`)).join("")}</div>` : ""}

<h2>8 Testläufe am finalen Stand</h2>
<table><tr><th>Befehl</th><th>Ergebnis</th><th class="n">Dauer</th></tr>${testRows}</table>
<p><b>Gefunden und behoben:</b> <code>tests/step/verify.py</code> brach am finalen Stand mit <code>SyntaxError: unmatched ')'</code> ab, bevor es eine Datei prüfte – beim Umstellen des Standardwerts von <code>--cad</code> auf <code>MCAD</code> (Commit <code>4db585b</code>) war eine Klammer zu viel stehen geblieben. Behoben in <code>0ea0d78</code>; danach 64/64 in beiden Arten. Sonst waren alle Läufe im ersten Durchgang grün; nichts übersprungen oder deaktiviert.</p>
<p class="small">Alle Läufe nacheinander auf demselben Rechner (${os.cpus().length} Kerne), Protokolle in <code>reports/tmp/report21/final/</code>. Neu in den Phasen: Playwright ${pwNew.map((t) => t.title.split(" ")[0]).join(", ")}; Tauri T19, T20.</p>
${pwBad.length ? `<h3>Fehlgeschlagene Playwright-Tests</h3><table><tr><th>Test</th><th>Datei</th><th>Fehler</th></tr>${pwBad.map((t) => `<tr><td>${esc(t.title)}</td><td><code>${esc(t.file)}</code></td><td>${pre((t.errors[0] ?? "").replace(/\u001b\[\d+m/g, "").split("\n").slice(0, 6).join("\n"))}</td></tr>`).join("")}</table>` : ""}
<h3>Tauri-E2E</h3>
<table><tr><th>Nr.</th><th>Szenario</th><th>Ergebnis</th><th class="n">Dauer</th></tr>${tauriRows}</table>

<h2>9 Grenzen und offene Punkte</h2>
<table><tr><th>Punkt</th><th>Ursache bzw. Stand</th></tr>
<tr><td>Offset/<code>minkowski</code> mit Kugel an nicht rechtwinkligen Ecken</td><td>SMLib nähert die Eckkugeln mit Trimmkurven bis 0,33 mm neben den Kanten an (Tetraeder) oder scheitert (Keil); die Fassade lehnt ab → Warnung, Kinder unverändert.</td></tr>
<tr><td>Gleich große Kugeln mit Mittelpunkten in einer Ebene</td><td>kein Sonderfall (flache Hülle der Mittelpunkte); Warnung.</td></tr>
<tr><td>Kollision mit STL-Netzen</td><td>nur ja/nein, kein Schnittvolumen.</td></tr>
<tr><td>Keepout-Prüfung im Worker</td><td>Booleans mit verrundeten Körpern blockieren den Worker beim ersten Mal einige Sekunden (Lochplatte ≈ 2,5 s); danach Cache.</td></tr>
<tr><td>Punktkontakt</td><td>erkannt, aber ohne Ort (keine Markierung).</td></tr>
<tr><td>STEP: überlappende Objekte der obersten Ebene</td><td>werden als überlappende Einzelkörper geschrieben (Hinweis im Export). Entscheidung des Nutzers steht aus; Vorschlag: überlappende Objekte zu einem Körper vereinigen, berührende getrennt lassen.</td></tr>
<tr><td>STL-B-Rep über 500 Flächen</td><td>Booleans überlinear (Kurve in Abschnitt 3); Grenze automatisch, mit <code>as = "solid"</code> erzwingbar.</td></tr>
<tr><td>UI-Einstellungen der Desktop-App</td><td>Einstellungen und Thema liegen im <code>localStorage</code> der WebView, den Tauri unter der App-ID ablegt; mit <code>dev.mcad.desktop</code> beginnen sie neu. Übernommen wird nur die Liste zuletzt geöffneter Dateien.</td></tr>
<tr><td>Arch-Paket <code>mcad</code></td><td>PKGBUILD, <code>docker.sh</code> und <code>smoke-test.sh</code> nachgezogen, aber ungebaut: im Container kein <code>makepkg</code> und kein laufender Docker-Daemon.</td></tr>
<tr><td><code>compare.py --self-test</code></td><td>braucht Referenz-STLs von OpenSCAD, die hier fehlen; geprüft ist <code>--eval</code>.</td></tr>
</table>
</main>
</body>
</html>
`;
const htmlOut = html.replace(/(<table[^>]*>\s*(?:<colgroup>.*?<\/colgroup>)?)\s*(<tr>\s*<th[\s\S]*?<\/tr>)/g, "$1<thead>$2</thead>");
writeFileSync(join(out, "report.html"), htmlOut);
console.log(`${join(out, "report.html")}: ${html.length} Zeichen; hull ${hull.length}, STL ${sph.length}, Asserts ${asserts.length}, Bilder in ${imgDir}`);
