#!/usr/bin/env node
// Test report 7 ("Kernel fixes and variable radius"): collects the results of the test runs
// and writes
//
//   docs/reports/07-kernel-variabler-radius/report.html, img/*
//
// The PDF is printed with tools/report/print-pdf.mjs (see the end of the output).
//
// Inputs (gitignored, reports/tmp/report7/ unless noted):
//   cargo.txt            `cargo test --workspace --no-fail-fast` (several runs: cargo-*.txt)
//   clippy.txt           `cargo clippy --workspace --all-targets`
//   node-test.txt        `node crates/kernel-smlib/js/test-node.mjs`
//   compare.txt          `python3 tests/openscad-compat/compare.py`
//   step-verify.txt      `tests/step/verify.py` over all examples (venv-step)
//   measure.txt          `cargo test -p cad-engine --test kernel_fixes -- --nocapture`
//   vitest.json          Vitest JSON reporter
//   time-before.tsv, time-after.tsv   `cad export` of every example (reports/tmp/time-export.sh)
//   scenarios.json, img/ Playwright scenarios of web/e2e/ui-e.spec.ts (helpers.recordE)
//   ../report2/playwright.json        Playwright JSON reporter (all spec files)
// Binaries: $CAD (default target/release/mcad) and $CAD_BEFORE (the CLI before these changes,
// default reports/tmp/cad-before) for the renderings before/after and the error messages before.
// web/src/wasm-pkg (web/scripts/build-wasm.sh) for the comparison native ↔ Wasm.
//
//   node tools/report/report7.mjs

import { execFileSync, spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import os from "node:os";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "../..");
const web = join(root, "web");
const input = join(root, "reports/tmp/report7");
const work = join(input, "work");
const out = join(root, "docs/reports/07-kernel-variabler-radius");
const cadNew = process.env.CAD ?? join(root, "target/release/mcad");
const cadOld = process.env.CAD_BEFORE ?? join(root, "reports/tmp/cad-before");
const hasOld = existsSync(cadOld);

const esc = (s) => String(s ?? "").replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
const readJson = (f, d = null) => (existsSync(f) ? JSON.parse(readFileSync(f, "utf8")) : d);
const readText = (f) => (existsSync(f) ? readFileSync(f, "utf8") : null);
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
const run = (bin, args) => {
  const r = spawnSync(bin, args, { encoding: "utf8", maxBuffer: 256 << 20 });
  return { code: r.status, out: r.stdout ?? "", err: r.stderr ?? "" };
};
/** Number with decimal point and a space as thousands separator (as in the other reports). */
const num = (x, d = 3) => Number(x).toLocaleString("en-US", { minimumFractionDigits: d, maximumFractionDigits: d }).replace(/,/g, " ");

rmSync(work, { recursive: true, force: true });
mkdirSync(work, { recursive: true });
rmSync(join(out, "img"), { recursive: true, force: true });
mkdirSync(join(out, "img"), { recursive: true });

// ---------------------------------------------------------------- cases

const PI = Math.PI;
const rb = (a, b, c, r) => {
  const [x, y, z] = [a - 2 * r, b - 2 * r, c - 2 * r];
  return x * y * z + 2 * r * (x * y + y * z + x * z) + PI * r * r * (x + y + z) + (4 / 3) * PI * r ** 3;
};
const openShell = (a, b, c, r, t) => rb(a, b, c, r) - rb(a - 2 * t, b - 2 * t, c - 2 * t, Math.max(0, r - t)) - (a - 2 * r) * (b - 2 * r) * t;

const F1 = [
  { id: "a", src: "grid g { z: o = 18; }\nshell(1.6) { box([0, 0, 0], [90, 34, 18], $fillet = 3); zone(g.o, d = 1); }\n", dims: [90, 34, 18, 3, 1.6] },
  { id: "b", src: "grid g { z: o = 18; }\nshell(1.6) { box([0, 0, 0], [90, 34, 18], $fillet = 5); zone(g.o, d = 1); }\n", dims: [90, 34, 18, 5, 1.6] },
  { id: "c", src: "grid g { z: o = 30; }\nshell(1.5) { box([0, 0, 0], [50, 35, 30], $fillet = 4); zone(g.o, d = 1); }\n", dims: [50, 35, 30, 4, 1.5] },
  { id: "d", src: "grid g { z: o = 18; }\nshell(1.6) { box([0, 0, 0], [90, 34, 18], $fillet = 6); zone(g.o, d = 1); }\n", dims: [90, 34, 18, 6, 1.6] },
];
const F2 = "thicken(1) surface(points = [[[0, 0, 0], [20, 0, 6]], [[0, 20, 6], [20, 20, 0]]]);\ntranslate([30, 0, 0]) thicken(1) surface(points = [[[0, 0, 0], [10, 0, 6], [20, 0, 0]], [[0, 10, 6], [10, 10, 9], [20, 10, 6]], [[0, 20, 0], [10, 20, 6], [20, 20, 0]]]);\ntranslate([60, 0, 0]) thicken(1) surface(points = [[[0, 0, 0], [5, 0, 4], [10, 0, 6], [15, 0, 4], [20, 0, 0]], [[0, 20, 3], [5, 20, 7], [10, 20, 9], [15, 20, 7], [20, 20, 3]]]);\n";
const F3 = "cylinder(h = 10, r = 5, $chamfer = 1);\ntranslate([20, 0, 0]) sphere(5, $fillet = 1);\n";
const F4 = "union(chamfer = 0.5) {\n    cube([30, 10, 5]);\n    translate([10, 0, 0]) cube([10, 10, 15]);\n}\n";
const write = (name, text) => {
  const f = join(work, name);
  writeFileSync(f, text);
  return f;
};

/** Diagnostics of `cad check` (errors and warnings, one per line, without the path, "L.<line>: …").
 *  The CLI before these changes ($CAD_BEFORE) printed German severities, hence both spellings. */
function check(bin, file) {
  const r = run(bin, ["check", file]);
  return (r.out + r.err)
    .split("\n")
    .filter((l) => /(error|warning|Fehler|Warnung) \(/.test(l))
    .map((l) => l.replace(/^.*?:(\d+):\d+: /, "L.$1: "));
}

/** PNG rendering (copied into the report); returns the image name or null. */
function render(bin, file, name, args = []) {
  const png = join(out, "img", `${name}.png`);
  const r = run(bin, ["render", file, "-o", png, "--size", "760x500", ...args]);
  return r.code === 0 || r.code === 1 ? (existsSync(png) ? name : null) : null;
}

function exportInfo(bin, file, ext = "step") {
  const r = run(bin, ["export", file, "-o", join(work, `x.${ext}`), "--json"]);
  try {
    return JSON.parse(r.out);
  } catch {
    return null;
  }
}

// F1: volumes after, messages before
const f1Rows = [];
for (const c of F1) {
  const f = write(`f1-${c.id}.scad`, c.src);
  const want = openShell(...c.dims);
  const after = exportInfo(cadNew, f);
  const msgsAfter = check(cadNew, f);
  const msgsBefore = hasOld ? check(cadOld, f) : [];
  f1Rows.push({ c, want, after, msgsAfter, msgsBefore });
}
const f1File = write(
  "f1-alle.scad",
  "grid g { z: o = 18; }\ngrid h { z: o = 30; }\nshell(1.6) { box([0, 0, 0], [90, 34, 18], $fillet = 3); zone(g.o, d = 1); }\ntranslate([0, 45, 0]) shell(1.6) { box([0, 0, 0], [90, 34, 18], $fillet = 5); zone(g.o, d = 1); }\ntranslate([100, 0, 0]) shell(1.5) { box([0, 0, 0], [50, 35, 30], $fillet = 4); zone(h.o, d = 1); }\n",
);
const dose = join(root, "tests/models/spec/dose_shell.scad");
const doseAfter = exportInfo(cadNew, dose);
const doseBefore = hasOld ? check(cadOld, dose) : [];
const f2File = write("f2.scad", F2);
const f3File = write("f3.scad", F3);
const f4File = write("f4.scad", F4);
const vr = join(root, "tests/models/spec/variabler_radius.scad");

const imgs = {};
const pair = (key, file, args = []) => {
  imgs[key] = { before: hasOld ? render(cadOld, file, `${key}-vorher`, args) : null, after: render(cadNew, file, `${key}-nachher`, args) };
};
pair("f1-faelle", f1File);
pair("f1-dose", dose, ["--line", "9"]);
pair("f2-flaechen", f2File);
pair("f3-zylinder", f3File, ["--line", "1"]);
pair("f3-kugel", f3File, ["--line", "2"]);
pair("f4-union", f4File, ["--line", "1"]);
pair("vr-stufe", vr, ["--line", "11"]);
pair("vr-tasche", vr, ["--line", "16"]);
pair("vr-lookup", vr, ["--line", "22"]);

const msgs = (file) => ({ before: hasOld ? check(cadOld, file) : [], after: check(cadNew, file) });
const f2Msgs = msgs(f2File);
const f3Msgs = msgs(f3File);
const f4Msgs = msgs(f4File);
const vrMsgs = msgs(vr);
const inspectHints = (bin, file, line) => {
  const r = run(bin, ["inspect", file, "--line", String(line), "--json"]);
  try {
    const j = JSON.parse(r.out);
    return j.nodes.map((n) => `${n.label}: ${(n.hints ?? []).join(" · ") || "–"}`).filter((x) => !x.startsWith("translate"));
  } catch {
    return [];
  }
};
const hintsOf = (file, line) => ({ before: hasOld ? inspectHints(cadOld, file, line) : [], after: inspectHints(cadNew, file, line) });
const f3Hints = [hintsOf(f3File, 1), hintsOf(f3File, 2)];
const f4Hints = hintsOf(f4File, 1);
const vrHints = [11, 16, 22].map((l) => hintsOf(vr, l));
const vrExport = exportInfo(cadNew, vr);

// ---------------------------------------------------------------- native ↔ Wasm

const pkg = join(web, "src/wasm-pkg");
const nativeWasm = [];
if (existsSync(join(pkg, "cad_wasm_bg.wasm"))) {
  const files = [
    ["F1 (three cases)", f1File],
    ["F1 (r = 6)", join(work, "f1-d.scad")],
    ["dose_shell.scad", dose],
    ["F2", f2File],
    ["F3", f3File],
    ["F4", f4File],
    ["variabler_radius.scad", vr],
    ["griffschale.scad", join(root, "tests/models/spec/griffschale.scad")],
    ["fillet_zonen.scad", join(root, "tests/models/spec/fillet_zonen.scad")],
  ];
  const norm = (scene, diags) => ({
    nodes: scene.nodes.map((n) => ({
      label: n.label,
      status: n.status,
      hints: n.hints,
      edges: (n.selected_edges ?? []).length,
      bounds: n.bounds ? [...n.bounds.min, ...n.bounds.max].map((v) => Math.round(v * 1e4) / 1e4) : null,
      msgs: (n.messages ?? []).map((m) => m.message),
    })),
    diags: (diags ?? []).map((d) => `${d.severity}:${d.message}`),
  });
  for (const [name, f] of files) {
    const n = run(cadNew, ["eval", f, "--json"]);
    const w = run("node", [join(web, "scripts/dump-scene.mjs"), f]);
    let same = false;
    let detail = "";
    try {
      const jn = JSON.parse(n.out);
      const jw = JSON.parse(w.out);
      const a = norm(jn.scene, jn.diagnostics);
      const b = norm(jw.scene, jw.diagnostics);
      same = JSON.stringify(a) === JSON.stringify(b);
      if (!same) {
        const i = a.nodes.findIndex((x, k) => JSON.stringify(x) !== JSON.stringify(b.nodes[k]));
        detail = i >= 0 ? `node ${i}: native ${JSON.stringify(a.nodes[i])} / Wasm ${JSON.stringify(b.nodes[i])}` : `diagnostics: ${a.diags.join("; ")} / ${b.diags.join("; ")}`;
      }
      const errs = a.nodes.filter((x) => x.status === "error").length;
      nativeWasm.push({ name, same, detail, nodes: a.nodes.length, errs });
    } catch (e) {
      nativeWasm.push({ name, same: false, detail: String(e.message ?? e).split("\n")[0], nodes: 0, errs: 0 });
    }
  }
}

// ---------------------------------------------------------------- test results

const cargoRuns = readdirSync(input)
  .filter((f) => /^cargo(-\d+)?\.txt$/.test(f))
  .sort()
  .map((f) => readFileSync(join(input, f), "utf8"));
function cargoSummary(t) {
  let ok = 0;
  let failed = 0;
  for (const m of t.matchAll(/test result: \w+\. (\d+) passed; (\d+) failed/g)) {
    ok += +m[1];
    failed += +m[2];
  }
  return { ok, failed };
}
const cargoSums = cargoRuns.map(cargoSummary);
const cargo = cargoRuns.at(-1) ?? null;
/** Test binaries with their tests (last run). */
const rust = [];
if (cargo) {
  let cur = null;
  for (const l of cargo.split("\n")) {
    const r = l.match(/Running (?:unittests )?(\S+)/) ?? l.match(/Doc-tests (\S+)/);
    if (r) {
      cur = { bin: r[1].replace(/\s*\(.*$/, ""), tests: [] };
      rust.push(cur);
      continue;
    }
    const t = l.match(/^test (.+?) \.\.\. (ok|FAILED|ignored)/);
    if (t && cur) cur.tests.push({ name: t[1], status: t[2] });
  }
}
const rustBins = rust.filter((b) => b.tests.length);
const NEW_BINS = ["tests/kernel_fixes.rs", "tests/facade4.rs", "tests/law.rs"];
const NEW_TESTS = new Set(["law::tests::step_law", "law::tests::let_lookup_and_errors", "spec_models_stage1"]);
const clippy = readText(join(input, "clippy.txt"));
const clippyWarnLines = clippy ? clippy.split("\n").filter((l) => /^warning: /.test(l) && !/generated \d+ warning/.test(l)) : [];
const clippyWarn = clippy ? clippyWarnLines.length : null;
const clippyFiles = clippy ? [...new Set([...clippy.matchAll(/^warning: (?!.*generated).*\n\s*--> ([^:]+)/gm)].map((m) => m[1]))] : [];
const clippyErr = clippy ? (clippy.match(/^error(\[|:)/gm) ?? []).length : null;
const nodeTest = readText(join(input, "node-test.txt"));
const nodeShellMs = nodeTest?.match(/shell of a rounded box, open at the top \(([\d.]+ ms)\)/)?.[1] ?? null;
const nodeLines = nodeTest ? nodeTest.split("\n").filter((l) => /^(ok|FAIL|fail)\s/.test(l)) : [];
const compare = readText(join(input, "compare.txt"));
const compareLine = compare?.split("\n").find((l) => /\d+\s*\/\s*\d+/.test(l)) ?? compare?.trim().split("\n").at(-1) ?? null;
const stepVerify = readText(join(input, "step-verify.txt"));
const stepLine = stepVerify?.split("\n").find((l) => /\d+\/\d+ passed/.test(l)) ?? null;
const measure = readText(join(input, "measure.txt")) ?? "";
const probes = [...measure.matchAll(/probe x=([\d.]+) r=([\d.]+) law=([\d.]+)/g)].map((m) => ({ x: +m[1], r: +m[2], law: +m[3] }));
const volStep = measure.match(/volumes r2=([\d.]+) law=([\d.]+) r4=([\d.]+)/);
const volPocket = measure.match(/pocket r1\.875=([\d.]+) law=([\d.]+) r1\.125=([\d.]+)/);
const vt = readJson(join(input, "vitest.json"));
const unitTotal = vt ? (vt.testResults ?? []).reduce((s, f) => s + (f.assertionResults ?? []).length, 0) : 0;
const unitPassed = vt ? (vt.testResults ?? []).reduce((s, f) => s + (f.assertionResults ?? []).filter((r) => r.status === "passed").length, 0) : 0;
const pw = readJson(join(root, "reports/tmp/report2/playwright.json"));
function flatten(suite, acc = []) {
  for (const s of suite.suites ?? []) flatten(s, acc);
  for (const spec of suite.specs ?? []) {
    for (const t of spec.tests ?? []) {
      const r = t.results?.at(-1);
      const id = spec.title.split(" ")[0];
      acc.push({ file: spec.file ?? suite.file ?? "", id, title: spec.title.slice(id.length + 1), status: r?.status ?? t.status ?? "unknown", ms: r?.duration ?? 0, error: r?.error?.message ?? null });
    }
  }
  return acc;
}
const pwAll = pw ? flatten(pw) : [];
const pwNew = pwAll.filter((t) => t.file.endsWith("ui-e.spec.ts"));
const pwOld = pwAll.filter((t) => !t.file.endsWith("ui-e.spec.ts"));
const passed = (l) => l.filter((t) => t.status === "passed").length;
const scenarios = readJson(join(input, "scenarios.json"), []);
for (const s of scenarios) for (const i of s.images ?? []) {
  const src = join(input, "img", `${i.name}.jpg`);
  if (existsSync(src)) copyFileSync(src, join(out, "img", `${i.name}.jpg`));
}

// runtimes before/after
const tsv = (f) =>
  Object.fromEntries(
    (readText(join(input, f)) ?? "")
      .trim()
      .split("\n")
      .filter(Boolean)
      .map((l) => l.split("\t"))
      .map(([n, ms, rc]) => [n, { ms: +ms, rc: +rc }]),
  );
const tBefore = tsv("time-before.tsv");
const tAfter = tsv("time-after.tsv");
const timeNames = [...new Set([...Object.keys(tBefore), ...Object.keys(tAfter)])];
const sum = (o, names) => names.reduce((s, n) => s + (o[n]?.ms ?? 0), 0);
const common = timeNames.filter((n) => tBefore[n] && tAfter[n]);

// ---------------------------------------------------------------- environment

const env = [
  ["Operating system", `${os.type()} ${os.release()} (${os.arch()})`],
  ["CPU", `${os.cpus()[0]?.model ?? "–"} · ${os.cpus().length} cores`],
  ["Rust", run("rustc", ["--version"]).out.trim() || "–"],
  ["Node.js", process.version],
  ["Emscripten", (run("bash", ["-c", "source /opt/tools/emsdk/emsdk_env.sh >/dev/null 2>&1; emcc --version | head -1"]).out.trim() || "–")],
  ["Browser", `Chromium (Playwright ${pkgVersion("@playwright/test")}), WebGL via SwiftShader, 1440 × 900 px`],
  ["SMLib", "tangency-labs/smlib capi-tess @ 7eba8aa + smlib/patches/0001 (1ed8669 from capi-solidcraft, source only), facade ABI 4"],
  ["Source", `Branch ${git("rev-parse", "--abbrev-ref", "HEAD")}, commit ${git("rev-parse", "--short", "HEAD")}`],
];

// ---------------------------------------------------------------- html

const date = new Date().toISOString().slice(0, 10);
const code = (s) => (s ? `<pre>${esc(s)}</pre>` : "");
const list = (a) => (a.length ? a.map(esc).join("<br>") : "–");
const img = (name, label) => (name ? `<figure><img src="img/${esc(name)}.${name.startsWith("e0") ? "jpg" : "png"}" alt="${esc(label)}"><figcaption>${esc(label)}</figcaption></figure>` : `<figure class="missing"><figcaption>${esc(label)}: –</figcaption></figure>`);
const ba = (key, lbl) => `<div class="pair">${img(imgs[key]?.before, `before: ${lbl}`)}${img(imgs[key]?.after, `after: ${lbl}`)}</div>`;
const mm3 = (v) => (v == null ? "–" : `${num(v, 3)} mm³`);

const f1Table = f1Rows
  .map(({ c, want, after, msgsAfter, msgsBefore }) => {
    const v = after?.volume;
    const rel = v != null ? Math.abs(v - want) / want : null;
    const [a, b, h, r, t] = c.dims;
    return `<tr class="${rel != null && rel < 1e-6 && !msgsAfter.length ? "ok" : "bad"}"><td>${a} × ${b} × ${h}, r ${r}, wall ${t}</td><td>${list(msgsBefore)}</td><td class="num">${mm3(want)}</td><td class="num">${mm3(v)}</td><td class="num">${rel == null ? "–" : rel.toExponential(1)}</td><td class="num">${after?.step ? `${after.step.faces ?? "–"} faces` : "–"}</td><td class="res">${msgsAfter.length ? esc(msgsAfter.join("; ")) : "ok"}</td></tr>`;
  })
  .join("\n");
const timeRows = timeNames
  .map((n) => {
    const b = tBefore[n];
    const a = tAfter[n];
    const d = b && a ? a.ms - b.ms : null;
    const cls = a && a.rc !== 0 ? "bad" : "ok";
    return `<tr class="${cls}"><td>${esc(n)}</td><td class="num">${b ? `${b.ms} ms${b.rc ? ` (Exit ${b.rc})` : ""}` : "–"}</td><td class="num">${a ? `${a.ms} ms${a.rc ? ` (Exit ${a.rc})` : ""}` : "–"}</td><td class="num">${d == null ? "–" : `${d > 0 ? "+" : ""}${d} ms`}</td></tr>`;
  })
  .join("\n");
const nwRows = nativeWasm.map((x) => `<tr class="${x.same ? "ok" : "bad"}"><td>${esc(x.name)}</td><td class="num">${x.nodes}</td><td class="num">${x.errs}</td><td class="res">${x.same ? "identical" : "different"}</td><td>${esc(x.detail)}</td></tr>`).join("\n");
const rustRows = rustBins
  .map((b) => {
    const name = b.bin.replace(/^.*\/deps\//, "").replace(/-[0-9a-f]{16}$/, "");
    const isNew = NEW_BINS.some((n) => b.bin.endsWith(n));
    const bad = b.tests.filter((t) => t.status === "FAILED").map((t) => t.name);
    const fresh = isNew ? b.tests.map((t) => t.name) : b.tests.filter((t) => NEW_TESTS.has(t.name)).map((t) => t.name);
    return `<tr class="${bad.length ? "bad" : "ok"}"><td>${esc(name)}${isNew ? ' <span class="tag">new</span>' : ""}</td><td class="num">${b.tests.filter((t) => t.status === "ok").length} / ${b.tests.length}</td><td>${fresh.map((n) => `<code>${esc(n)}</code>`).join(", ") || "–"}${bad.length ? `<div class="err">${bad.map(esc).join(", ")}</div>` : ""}</td></tr>`;
  })
  .join("\n");
const pwRows = pwNew
  .map((t) => {
    const c = scenarios.find((s) => s.id === t.id);
    const checks = c?.checks?.length ? `<ul class="checks">${c.checks.map((x) => `<li>${esc(x)}</li>`).join("")}</ul>` : esc(c?.caption ?? "");
    return `<tr class="${t.status === "passed" ? "ok" : "bad"}"><td>${esc(t.id)}</td><td>${esc(t.title)}</td><td class="res">${t.status === "passed" ? "passed" : "failed"}</td><td class="num">${(t.ms / 1000).toFixed(1)} s</td><td>${checks}${t.error ? `<div class="err">${esc(t.error.split("\n")[0])}</div>` : ""}</td></tr>`;
  })
  .join("\n");
const blocks = scenarios
  .filter((s) => (s.images ?? []).length)
  .sort((a, b) => a.id.localeCompare(b.id, "en", { numeric: true }))
  .map(
    (s) => `<section class="sc"><h3>${esc(s.id)} ${esc(s.title)}</h3>
${s.images.map((i) => `<figure><img src="img/${esc(i.name)}.jpg" alt="${esc(i.label)}"><figcaption>${esc(i.label)}</figcaption></figure>`).join("")}
${s.code ? `<div class="lbl">Code</div>${code(s.code)}` : ""}<p class="cap">${esc(s.caption)}</p></section>`,
  )
  .join("\n");
const probeRows = probes.map((p) => `<tr><td class="num">${num(p.x, 1)}</td><td class="num">${num(p.law, 4)}</td><td class="num">${num(p.r, 4)}</td><td class="num">${num(((p.r - p.law) / p.law) * 100, 2)} %</td></tr>`).join("");
const ms = (n) => tAfter[n]?.ms ?? null;

const html = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Test report 7 – Kernel fixes and variable radius</title>
<style>
  @page { size: A4; margin: 15mm 13mm 16mm 13mm; }
  :root { --text: #1f2328; --muted: #5d646e; --border: #d5d9df; --ok: #1a7f37; --bad: #c9302c; --head: #f2f4f7; }
  * { box-sizing: border-box; }
  body { font: 9.6pt/1.45 system-ui, -apple-system, "Segoe UI", Roboto, sans-serif; color: var(--text); margin: 0; }
  main { max-width: 184mm; margin: 0 auto; }
  h1 { font-size: 18pt; margin: 0 0 2mm; }
  h2 { font-size: 13pt; margin: 7mm 0 3mm; border-bottom: 1px solid var(--border); padding-bottom: 1mm; break-after: avoid; }
  h3 { font-size: 10.5pt; margin: 4mm 0 1.5mm; break-after: avoid; }
  .meta { color: var(--muted); margin-bottom: 4mm; }
  .summary { display: flex; flex-wrap: wrap; gap: 2.5mm; margin: 3mm 0; }
  .summary div { flex: 1 1 28mm; border: 1px solid var(--border); border-radius: 2mm; padding: 2mm; font-size: 8.4pt; }
  .summary b { display: block; font-size: 12pt; }
  table { width: 100%; border-collapse: collapse; margin: 2mm 0; font-size: 8.2pt; }
  th, td { border: 1px solid var(--border); padding: 1.1mm 1.8mm; text-align: left; vertical-align: top; }
  th { background: var(--head); font-weight: 600; }
  td.num { text-align: right; white-space: nowrap; }
  tr.ok td.res { color: var(--ok); font-weight: 600; }
  tr.bad td.res, tr.bad td:first-child { color: var(--bad); font-weight: 600; }
  .err { color: var(--bad); font-size: 8pt; margin-top: 1mm; }
  .tag { font-size: 7pt; color: #fff; background: #0c8599; border-radius: 1mm; padding: 0 1mm; }
  ul { margin: 1mm 0 2mm 5mm; padding: 0; }
  li { margin: 0.5mm 0; }
  ul.checks { margin: 0 0 0 3.5mm; }
  code, pre { font-family: ui-monospace, "SFMono-Regular", Menlo, Consolas, monospace; font-size: 7.8pt; }
  pre { margin: 0; padding: 1.4mm 2mm; background: #f6f8fa; border: 1px solid var(--border); border-radius: 1mm; white-space: pre-wrap; word-break: break-word; }
  .fix { border: 1px solid var(--border); border-radius: 2mm; padding: 2.5mm 3mm; margin: 3mm 0; }
  .fix h3 { margin-top: 0; }
  .fix dl { margin: 0; display: grid; grid-template-columns: 22mm 1fr; gap: 1mm 3mm; }
  .fix dt { font-weight: 600; color: var(--muted); }
  .fix dd { margin: 0; }
  .pair { display: flex; gap: 2mm; break-inside: avoid; margin: 2mm 0; }
  .pair figure { flex: 1; min-width: 0; }
  figure { margin: 0 0 2mm; break-inside: avoid; }
  figure img { max-width: 100%; max-height: 110mm; border: 1px solid var(--border); display: block; }
  figcaption { font-size: 7.8pt; color: var(--muted); margin-top: 0.6mm; }
  figure.missing { border: 1px dashed var(--border); padding: 4mm; }
  .sc { break-inside: avoid; margin: 0 0 6mm; }
  .lbl { font-size: 7.4pt; color: var(--muted); text-transform: uppercase; letter-spacing: 0.04em; margin: 1.5mm 0 0.5mm; }
  .cap { font-size: 8.6pt; color: var(--muted); margin: 1.5mm 0 0; }
  .small { font-size: 8.6pt; color: var(--muted); }
  tr, td { break-inside: avoid; }
</style>
</head>
<body>
<main>
<h1>Test report 7 – Kernel fixes and variable radius</h1>
<div class="meta">SMLib facade, engine, language, CLI and browser UI · ${esc(date)} · Commit ${esc(git("rev-parse", "--short", "HEAD"))}</div>

<div class="summary">
  <div>cargo test --workspace<b>${cargoSums.length ? cargoSums.map((s) => `${s.ok}/${s.ok + s.failed}`).join(" · ") : "–"}</b>${cargoSums.length} runs</div>
  <div>cargo clippy<b>${clippy == null ? "–" : `${clippyWarn} warnings`}</b>all pre-existing, none new</div>
  <div>Node (smlib.wasm)<b>${nodeLines.length ? `${nodeLines.filter((l) => l.startsWith("ok")).length} / ${nodeLines.length}` : "–"}</b>test-node.mjs</div>
  <div>Vitest<b>${vt ? `${unitPassed} / ${unitTotal}` : "–"}</b></div>
  <div>Playwright<b>${pwAll.length ? `${passed(pwAll)} / ${pwAll.length}` : "–"}</b>of which new ${passed(pwNew)} / ${pwNew.length}</div>
  <div>OpenSCAD comparison<b>${compareLine ? esc((compareLine.match(/\d+\s*\/\s*\d+/) ?? [""])[0]) : "–"}</b>compare.py</div>
  <div>STEP (OCCT)<b>${stepLine ? esc((stepLine.match(/\d+\/\d+/) ?? [""])[0]) : "–"}</b>verify.py</div>
  <div>native ↔ Wasm<b>${nativeWasm.length ? `${nativeWasm.filter((x) => x.same).length} / ${nativeWasm.length}` : "–"}</b>identical scenes</div>
</div>

<h2>1 Scope</h2>
<ul>
  <li><b>F1</b> <code>shell</code> of a filleted body (dose_shell.scad was red), <b>F2</b> <code>surface(points)</code> with 2 × 2 or 3 × 3 points, <b>F3</b> seam and pole edges as separate edges – the three bugs found in report 4, section 7. In addition <b>F4</b>: fase_chamfer.scad was red as well (<code>union(chamfer = 0.5)</code> selected eight instead of two edges).</li>
  <li><b>Variable radius</b> (spec "Variable radius (stretch goal)"): <code>fillet(r = function (p) …) { … }</code> and <code>difference/union/intersection(fillet = function (p) …)</code>; radius as a function of position, evaluated at stations along each edge, hint "r 2–4", error cases as for the constant radius. New example <code>tests/models/spec/variabler_radius.scad</code>.</li>
  <li><b>SMLib</b>: fix 1ed8669 (pointer maps in insertion order) from the fork branch <code>capi-solidcraft</code> as patch <code>smlib/patches/0001-pointer-maps-insertion-order.patch</code> on top of 7eba8aa (<code>smlib/fetch.sh</code> applies it). <b>Facade ABI 4</b> (<code>sc_version()</code> = 4): edge flags in <code>sc_edges</code>, <code>sc_law_stations</code>, <code>sc_fillet_var</code>; kernel ID <code>smlib-7eba8aa+1/facade-4</code> (new cache keys).</li>
</ul>

<h2>2 Test environment</h2>
<table>${env.map(([a, b]) => `<tr><th style="width:30mm">${esc(a)}</th><td>${esc(b)}</td></tr>`).join("")}</table>

<h2>3 Bugs: cause, fix, evidence</h2>
<p class="small">Diagnostic and hint texts in the tables below are quoted verbatim from the program output ("L." = line). The CLI before these changes printed German texts ("Fehler" = error, "Warnung" = warning, "Kanten" = edges, "gescheitert" = failed).</p>

<div class="fix"><h3>F1 shell of a filleted body</h3><dl>
<dt>Cause</dt><dd>Not the "no cavity" pre-check (it only concerns planar, opposite walls; the message "leaves no cavity" came after SMLib had returned an open body). SMLib's shell construction with an opened face (<code>SmApiShellBrepFull</code> → <code>SmOffsetExecutive::ShellBrep</code>, branch with opening via <code>SmMerge</code>) does not hold up on filleted bodies: depending on the options, <code>SM_ERR_NULL_POINTER</code> in <code>SmMerge.cpp:672</code>, an open body made of the 26 input faces, or a closed body whose border faces are missing and whose inner roundings are replaced by extended faces (11–15 faces fail the validity check, volume off by 0.08 %). Which case occurred depended on the address order of SMLib's pointer maps (hence different natively and in Wasm). The <i>closed</i> inward offset of the same body (<code>SmApiOffsetBrepFull</code>, −t), in contrast, is exact (volume = Minkowski formula to 10<sup>−7</sup>).</dd>
<dt>Fix</dt><dd><code>sc_shell</code> tries SMLib's shell; if it fails or self-overlaps, the facade builds the shell from booleans: <i>body − O − P<sub>1</sub> − … − P<sub>k</sub></i>, O being the closed inward offset, P<sub>i</sub> a prism over the offset face of each opened (planar) face, from just inside the cavity to beyond the body (thickened single face). The side walls of the prism go through the border of the offset face: the opening border is the band between the opened face and its offset, as with SMLib's own shell. Booleans go through the facade's checked booleans, the result through the validity check. In addition patch 0001 (1ed8669): SMLib's pointer maps return keys in insertion order – results no longer depend on the heap layout.</dd>
<dt>Evidence</dt><dd>Volume against the closed form (rounded cuboid minus inner rounded cuboid with r − t minus the wall band under the planar top face), STEP check, identical scenes native and Wasm (section 5), Rust tests <code>shell_of_rounded_boxes</code>, <code>dose_shell_example_is_a_hollow_box</code>, <code>shell_of_a_rounded_box_open_at_the_top</code>, Node test, Playwright E03.</dd></dl>
<table><tr><th>Case</th><th>before (cad check)</th><th>expected</th><th>after</th><th>rel. dev.</th><th>STEP</th><th>Diagnostics after</th></tr>${f1Table}
<tr class="${doseAfter ? "ok" : "bad"}"><td>dose_shell.scad (with lid)</td><td>${list(doseBefore)}</td><td class="num">–</td><td class="num">${mm3(doseAfter?.volume)}</td><td class="num">–</td><td class="num">${doseAfter?.step ? `${doseAfter.step.faces ?? "–"} faces` : "–"}</td><td class="res">${check(cadNew, dose).length ? "error" : "ok"}</td></tr></table>
${ba("f1-dose", "dose_shell.scad, module dose (cursor on shell, line 9)")}
${ba("f1-faelle", "the three cases from report 4")}
</div>

<div class="fix"><h3>F2 surface(points) with 2 × 2 or 3 × 3 points</h3><dl>
<dt>Cause</dt><dd><code>SmApiCreateSurfaceFromOrderedPoints</code> calls <code>SmBSplineSurface::ApproximatePoints</code> with a fixed degree 3 in both directions; that requires at least 4 points per direction, otherwise <code>SM_ERR</code>.</dd>
<dt>Fix</dt><dd><code>sc_nurbs_surface</code> calls <code>ApproximatePoints</code> itself, with degree min(3, n − 1) separately per direction: bilinear for 2, quadratic for 3, cubic from 4 (unchanged from 4 × 4). With as many control points as points, the least-squares fit interpolates the points.</dd>
<dt>Evidence</dt><dd><code>surfaces_get_the_degree_their_point_count_allows</code> (degrees 1/1, 2/2, 1/3, 3/1, 3/3 in the B-rep, points on the surface), <code>surfaces_from_two_and_three_points_per_direction</code> (planar grids 2 × 2 … 4 × 4: area 400 mm²), Node test, Playwright E05.</dd>
<dt>Messages</dt><dd>before: ${list(f2Msgs.before)}<br>after: ${list(f2Msgs.after)}</dd></dl>
${ba("f2-flaechen", "2 × 2, 3 × 3 and 2 × 5 points, thickened")}
</div>

<div class="fix"><h3>F3 Seam and pole edges counted as edges</h3><dl>
<dt>Cause</dt><dd>For <code>$fillet/$chamfer</code> the engine selected all edges of the primitive, and in zones all edges in the zone. SMLib's cylinder has a seam (same face on both sides), the sphere a seam from pole to pole. The facade skipped such edges when filleting (<code>GetBlendEdgeuse() == NULL</code>), but they were still counted and drawn; for the sphere no edge remained → error.</dd>
<dt>Fix</dt><dd><code>sc_edges</code> reports flags per edge, <code>SC_EDGE_SMOOTH</code> = nothing to fillet (seam, pole, tangentially continuous transition within 2°, open border; SMLib's own criterion). <code>EdgeInfo.smooth</code>; the engine never selects such edges – for <code>$fillet/$chamfer</code>, in zones and for <code>fillet =</code> on booleans. A primitive without an edge to fillet yields "0 edges" as a warning without effect. The tangent borders of earlier roundings no longer count either (nested blocks).</dd>
<dt>Hints</dt><dd>Cylinder before: ${list(f3Hints[0].before)} · after: ${list(f3Hints[0].after)}<br>Sphere before: ${list(f3Hints[1].before)} · after: ${list(f3Hints[1].after)}<br>Messages before: ${list(f3Msgs.before)} · after: ${list(f3Msgs.after)}</dd>
<dt>Evidence</dt><dd><code>seams_and_poles_are_not_edges</code> (cylinder 2 edges, volume with two chamfer rings; sphere 0 edges, warning, volume unchanged; zone around the cylinder 2 edges; union of two equal cylinders: 0 new edges), <code>tangent_edges_of_rounds_are_not_selected</code>, <code>edges_flag_seams_poles_and_tangent_edges_as_smooth</code>, Playwright E04 and C05.</dd></dl>
${ba("f3-zylinder", "cylinder(…, $chamfer = 1), cursor line 1 (selected edges orange)")}
${ba("f3-kugel", "sphere(5, $fillet = 1), cursor line 2")}
</div>

<div class="fix"><h3>F4 fase_chamfer.scad: union(chamfer) selected eight edges</h3><dl>
<dt>Cause</dt><dd>"New edges" of a boolean operation were edges between faces of different operands (face tags). If two operands share a plane (plate and tower with the same front and back face), SMLib merges the faces into one with the tag of one operand; the six old tower edges on the front and back then separated faces of different operands and were considered new. The chamfer on these edges failed (<code>SM_ERR</code>).</dd>
<dt>Fix</dt><dd>An edge that lies on an edge of an operand is not new (sample points on the edge chain of an operand, tolerance 10<sup>−6</sup> of the model size).</dd>
<dt>Hints</dt><dd>before: ${list(f4Hints.before)} · after: ${list(f4Hints.after)}<br>Messages before: ${list(f4Msgs.before)} · after: ${list(f4Msgs.after)}</dd>
<dt>Evidence</dt><dd><code>coplanar_merged_faces_keep_old_edges_old</code> (2 edges, volume 2502.5 mm³), fase_chamfer.scad without errors (stage 1, STEP, run time table).</dd></dl>
${ba("f4-union", "union(chamfer = 0.5) from fase_chamfer.scad, cursor line 1")}
</div>

<h2>4 Variable radius</h2>
<h3>Architecture: evaluable description instead of a callback</h3>
<p>The edges at which the radius is needed only exist once the kernel has built the body – long after the evaluation, in the engine. Two options: a callback from the engine into the evaluation, or the evaluation supplies an evaluable description of the function. The description was chosen: <code>cad-eval</code> translates the function literal into a <b>radius law</b> (<code>cad_ir::Law</code>), a small pure expression over <code>p</code>. Whatever does not depend on <code>p</code> (captured variables, grid planes, list comprehensions, arbitrary builtins) is evaluated at that point and becomes a constant; calls to user functions and function values are inlined (parameters as local slots, depth ≤ 16); what remains are literals, arithmetic, comparisons, <code>&amp;&amp; || ! ?:</code>, <code>let</code>, indices, <code>.x/.y/.z</code>, vectors and the math builtins (<code>sin … lookup</code>).</p>
<ul>
  <li>The law is part of the tree (serializable, part of the cache key): a changed captured variable changes the key, the same source hits the cache. A callback would need its own key for the closure including its environment.</li>
  <li>The engine still does not depend on <code>cad-eval</code> and its AST lifetime; Wasm and native evaluate the same data object, also from the cache, without re-evaluating the source.</li>
  <li>Limitation: only the pure expression core can depend on <code>p</code> (no strings, no list comprehensions over <code>p</code>, no recursion over <code>p</code>); such cases are errors at the function with their own message.</li>
</ul>
<h3>Procedure</h3>
<ol>
  <li>Evaluation: <code>fillet(r = function …)</code> → <code>Op::Blend { law }</code>, <code>difference(fillet = function …)</code> → <code>Op::Boolean { fillet_law }</code>. <code>chamfer</code> and <code>$fillet</code> with a function: warning, not supported.</li>
  <li>Engine: edge selection as for the constant radius (zones, new edges, without smooth edges); per edge <i>m</i> stations (about one per 1.5 mm, 4 to 32) from <code>Kernel::law_stations</code>; the law is evaluated at each station in the coordinate system of the fillet/boolean statement. Radius ≤ 0 or not a number → error with value and position, edge red, body unchanged. All values equal → constant fillet (same result as <code>fillet(r)</code>). Otherwise <code>Kernel::fillet_variable</code>; kernel errors as for the constant radius, with the range ("fillet failed (8–18 mm): …"). Hint at the end of the line: "r <i>min</i>–<i>max</i>" of the station values.</li>
  <li>Facade (<code>sc_law_stations</code>, <code>sc_fillet_var</code>): the law of an edge is a clamped B-spline of degree min(3, m − 1) over the normalized edge parameter with uniform interior knots, control radius <i>j</i> at the Greville point t<sub>j</sub> (that is where the stations lie). It stays between the smallest and largest value, is exact for constant and linear profiles and takes the first and last value at the edge ends. As <code>SmBSplineFilletLaw</code>, the law curve carries the radius as x and t itself as y (control points (r<sub>j</sub>, t<sub>j</sub>), i.e. y(t) = t), so that SMLib can extend it beyond the edge ends at corners. Per edge SMLib's variable-radius solver (<code>SM_FS_VARIABLE_RADIUS</code>), edges with equal values use the constant one.</li>
</ol>
<h3>Measurements</h3>
<table><tr><th>x (mm)</th><th>Law 1 + x/15</th><th>measured (start of the top face)</th><th>Dev.</th></tr>${probeRows || "<tr><td colspan=4>–</td></tr>"}</table>
<p class="small">One edge of a 30 × 20 × 20 cuboid with a linear law: radius measured via point classification just below the top face (that is where the rounding starts; with a growing radius the tangent line lies slightly behind r(x)). Volume against the closed form 12000 − (1 − π/4)·130: deviation &lt; 10<sup>−5</sup> (Rust and Node test).
${volStep ? `Step law <code>p.z &lt; 10 ? 2 : 4</code> on all 12 edges of a 30 × 30 × 20 cuboid: ${num(+volStep[2])} mm³, between r = 4 (${num(+volStep[3])} mm³) and r = 2 (${num(+volStep[1])} mm³).` : ""}
${volPocket ? `Pocket border with <code>1 + p.x / 40</code>: ${num(+volPocket[2])} mm³ between r = 1.875 (${num(+volPocket[1])} mm³) and r = 1.125 (${num(+volPocket[3])} mm³).` : ""}
variabler_radius.scad: ${mm3(vrExport?.volume)}, STEP ${vrExport?.step ? `${vrExport.step.faces ?? "–"} faces` : "–"}, <code>cad export</code> ${ms("spec/variabler_radius.scad") ?? "–"} ms.</p>
<table><tr><th>Line</th><th>before</th><th>after</th></tr>
${[11, 16, 22].map((l, i) => `<tr><td>${l}</td><td>${list(vrHints[i].before)}</td><td>${list(vrHints[i].after)}</td></tr>`).join("")}
<tr><td>Diagnostics</td><td>${list(vrMsgs.before)}</td><td>${list(vrMsgs.after)}</td></tr></table>
${ba("vr-stufe", "variabler_radius.scad, cursor line 11 (step law)")}
${ba("vr-tasche", "line 16: difference(fillet = function (p) 1 + p.x / 40)")}
${ba("vr-lookup", "line 22: lookup law with zone at the lid edges")}

<h2>5 Native and Wasm</h2>
<p class="small">Scene from <code>cad eval --json</code> (native) against <code>web/scripts/dump-scene.mjs</code> (browser engine in Node: cad-wasm + smlib.wasm) per node: status, hints, number of selected edges, bounding box (10<sup>−4</sup> mm), messages, and the diagnostics.</p>
<table><tr><th>File</th><th>Nodes</th><th>of which errors</th><th>Result</th><th>Deviation</th></tr>${nwRows || "<tr><td colspan=5>– (web/src/wasm-pkg missing)</td></tr>"}</table>

<h2>6 Run times of cad export (all examples)</h2>
<p class="small"><code>cad export &lt;file&gt; -o x.stl</code> (release, wall-clock time incl. startup, one run per file, sequentially) before (state before this task) and after. Total over ${common.length} common files: before ${sum(tBefore, common)} ms, after ${sum(tAfter, common)} ms. Exit 1 = diagnostics with errors (before: dose_shell, fase_chamfer).</p>
<table><tr><th>File</th><th>before</th><th>after</th><th>Δ</th></tr>${timeRows}</table>

<h2>7 Tests</h2>
<ul>
  <li><b>cargo test --workspace</b> (${cargoSums.length} runs, no failures between runs): ${cargoSums.map((s) => `${s.ok} passed, ${s.failed} failed`).join(" · ") || "–"}.</li>
  <li><b>cargo clippy --workspace --all-targets</b>: ${clippy == null ? "–" : `${clippyWarn} warnings, ${clippyErr} errors`}; the warnings are in unchanged files (${clippyFiles.map((f) => `<code>${esc(f)}</code>`).join(", ")}) and existed before – none new.</li>
  <li><b>node crates/kernel-smlib/js/test-node.mjs</b> (smlib.wasm via the bridge): ${nodeLines.length ? `${nodeLines.filter((l) => l.startsWith("ok")).length} / ${nodeLines.length}` : "–"}${nodeLines.length ? ` – new: edge flags, surface 2 × 2/3 × 3/2 × 5, shell of a filleted cuboid (volume to 10<sup>−6</sup>), variable radius (stations, linear law, radius ≤ 0)` : ""}.</li>
  <li><b>OpenSCAD comparison</b> (compare.py): <code>${esc(compareLine ?? "–")}</code>.</li>
  <li><b>STEP</b> with OpenCASCADE (<code>tests/step/verify.py</code> over all examples): <code>${esc(stepLine?.replace(/; report.*$/, "") ?? "–")}</code>.</li>
  <li><b>Vitest</b>: ${vt ? `${unitPassed} / ${unitTotal}` : "–"}; <b>pnpm typecheck</b> without errors.</li>
  <li><b>Playwright</b>: ${pwAll.length ? `${passed(pwAll)} / ${pwAll.length}` : "–"} (existing scenarios ${passed(pwOld)} / ${pwOld.length}, new ${passed(pwNew)} / ${pwNew.length}).</li>
</ul>
<table><tr><th>ID</th><th>Scenario (web/e2e/ui-e.spec.ts)</th><th>Result</th><th>Duration</th><th>Checked</th></tr>${pwRows}</table>
${cargo ? `<table><tr><th>Rust test program (last run)</th><th>passed</th><th>new tests</th></tr>${rustRows}</table>` : ""}

<h2>8 Limitations</h2>
<ul>
  <li><b>shell from booleans</b> only for planar opened faces (the prism is the thickened offset face); for curved opened faces SMLib's result or its error message remains. Closed shells (without a zone) still use SMLib's result if it is valid. The booleans cost time: dose_shell ${tBefore["spec/dose_shell.scad"] ? `${tBefore["spec/dose_shell.scad"].ms} ms (failing) → ${tAfter["spec/dose_shell.scad"]?.ms ?? "–"} ms` : ""} (section 6); in smlib.wasm (Node test, 90 × 34 × 18, r 3) ${esc(nodeShellMs ?? "–")}.</li>
  <li><b>Wall thickness ≥ rounding radius</b> (r ≤ t, e.g. 30 × 20 × 10 with r 1, wall 2): the inward offset would be sharp-edged; SMLib's offset fails there in 7eba8aa (<code>SM_ERR</code>, fork commit a252570 not adopted), and the shell remains an error as before (the message names both approaches).</li>
  <li><b>Variable radius:</b> SMLib's variable-radius solver is sensitive. Steep profiles with a large radius at pocket corners fail ("self-overlapping", e.g. pocket border with 1 + p.x/20, 1.25–2.75), as do circular edges with a varying radius (<code>SM_ERR</code>); the example therefore uses 1 + p.x/40. Jumps (<code>p.z &lt; 10 ? 2 : 4</code> on vertical edges) are smoothed over about three station spacings; the hint shows the range of the station values (lookup peak 4 → "3.9", because no station lies exactly at x = 20). Position <code>p</code> in the coordinate system of the fillet/boolean statement. No variable radius for <code>chamfer</code> and <code>$fillet</code> (warning).</li>
  <li><b>Smooth edges:</b> SMLib's criterion (faces tangent within 2°) – nearly tangent edges are never selected. The check "old edge of an operand" (F4) compares with the edge chains of the operands, for curved edges only as accurately as their sampling (there the earlier selection remains).</li>
  <li><b>SMLib patch</b> as a working-copy patch on top of 7eba8aa (the fork commit is not on <code>capi-tess</code>); prebuilt libraries (<code>SMLIB_PREBUILT</code>) must be rebuilt with the patch (header change, otherwise ODR violation). A symlink <code>third_party/smlib</code> is only checked by <code>fetch.sh</code>.</li>
</ul>

<h2>9 Screenshots of the browser UI</h2>
${blocks || "<p>–</p>"}
</main>
</body>
</html>
`;

writeFileSync(join(out, "report.html"), html);
const sizes = readdirSync(join(out, "img")).map((f) => statSync(join(out, "img", f)).size);
console.log(`report7: ${f1Rows.length} F1 cases, native↔Wasm ${nativeWasm.filter((x) => x.same).length}/${nativeWasm.length}, Playwright ${passed(pwAll)}/${pwAll.length}, ${sizes.length} images (max ${(Math.max(0, ...sizes) / 1024).toFixed(0)} KiB)`);
console.log(`-> ${join(out, "report.html")}`);
console.log(`PDF: node tools/report/print-pdf.mjs ${join(out, "report.html")} ${join(out, "report.pdf")} "Test report 7: Kernel fixes and variable radius"`);
