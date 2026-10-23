#!/usr/bin/env node
// Test report 7 ("Kernel-Korrekturen und variabler Radius"): collects the results of the test runs
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
// Binaries: $CAD (default target/release/cad) and $CAD_BEFORE (the CLI before these changes,
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
const cadNew = process.env.CAD ?? join(root, "target/release/cad");
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
const de = (x, d = 3) => Number(x).toLocaleString("de-DE", { minimumFractionDigits: d, maximumFractionDigits: d });

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

/** Diagnostics of `cad check` (errors and warnings, one per line, without the path). */
function check(bin, file) {
  const r = run(bin, ["check", file]);
  return (r.out + r.err)
    .split("\n")
    .filter((l) => /(Fehler|Warnung) \(/.test(l))
    .map((l) => l.replace(/^.*?:(\d+):\d+: /, "Z.$1: "));
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
const dose = join(root, "examples/spec/dose_shell.scad");
const doseAfter = exportInfo(cadNew, dose);
const doseBefore = hasOld ? check(cadOld, dose) : [];
const f2File = write("f2.scad", F2);
const f3File = write("f3.scad", F3);
const f4File = write("f4.scad", F4);
const vr = join(root, "examples/spec/variabler_radius.scad");

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
    ["F1 (drei Fälle)", f1File],
    ["F1 (r = 6)", join(work, "f1-d.scad")],
    ["dose_shell.scad", dose],
    ["F2", f2File],
    ["F3", f3File],
    ["F4", f4File],
    ["variabler_radius.scad", vr],
    ["griffschale.scad", join(root, "examples/spec/griffschale.scad")],
    ["fillet_zonen.scad", join(root, "examples/spec/fillet_zonen.scad")],
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
        detail = i >= 0 ? `Knoten ${i}: nativ ${JSON.stringify(a.nodes[i])} / Wasm ${JSON.stringify(b.nodes[i])}` : `Diagnosen: ${a.diags.join("; ")} / ${b.diags.join("; ")}`;
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
      acc.push({ file: spec.file ?? suite.file ?? "", id, title: spec.title.slice(id.length + 1), status: r?.status ?? t.status ?? "unbekannt", ms: r?.duration ?? 0, error: r?.error?.message ?? null });
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
  ["Betriebssystem", `${os.type()} ${os.release()} (${os.arch()})`],
  ["CPU", `${os.cpus()[0]?.model ?? "–"} · ${os.cpus().length} Kerne`],
  ["Rust", run("rustc", ["--version"]).out.trim() || "–"],
  ["Node.js", process.version],
  ["Emscripten", (run("bash", ["-c", "source /opt/tools/emsdk/emsdk_env.sh >/dev/null 2>&1; emcc --version | head -1"]).out.trim() || "–")],
  ["Browser", `Chromium (Playwright ${pkgVersion("@playwright/test")}), WebGL über SwiftShader, 1440 × 900 px`],
  ["SMLib", "tangency-labs/smlib capi-tess @ 7eba8aa + smlib/patches/0001 (1ed8669 aus capi-solidcraft, nur Quelltext), Fassade ABI 4"],
  ["Quelltext", `Branch ${git("rev-parse", "--abbrev-ref", "HEAD")}, Commit ${git("rev-parse", "--short", "HEAD")}`],
];

// ---------------------------------------------------------------- html

const date = new Date().toLocaleDateString("de-DE", { year: "numeric", month: "long", day: "numeric" });
const code = (s) => (s ? `<pre>${esc(s)}</pre>` : "");
const list = (a) => (a.length ? a.map(esc).join("<br>") : "–");
const img = (name, label) => (name ? `<figure><img src="img/${esc(name)}.${name.startsWith("e0") ? "jpg" : "png"}" alt="${esc(label)}"><figcaption>${esc(label)}</figcaption></figure>` : `<figure class="missing"><figcaption>${esc(label)}: –</figcaption></figure>`);
const ba = (key, lbl) => `<div class="pair">${img(imgs[key]?.before, `vorher: ${lbl}`)}${img(imgs[key]?.after, `nachher: ${lbl}`)}</div>`;
const mm3 = (v) => (v == null ? "–" : `${de(v, 3)} mm³`);

const f1Table = f1Rows
  .map(({ c, want, after, msgsAfter, msgsBefore }) => {
    const v = after?.volume;
    const rel = v != null ? Math.abs(v - want) / want : null;
    const [a, b, h, r, t] = c.dims;
    return `<tr class="${rel != null && rel < 1e-6 && !msgsAfter.length ? "ok" : "bad"}"><td>${a} × ${b} × ${h}, r ${r}, Wand ${t}</td><td>${list(msgsBefore)}</td><td class="num">${mm3(want)}</td><td class="num">${mm3(v)}</td><td class="num">${rel == null ? "–" : rel.toExponential(1)}</td><td class="num">${after?.step ? `${after.step.faces ?? "–"} Flächen` : "–"}</td><td class="res">${msgsAfter.length ? esc(msgsAfter.join("; ")) : "ok"}</td></tr>`;
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
const nwRows = nativeWasm.map((x) => `<tr class="${x.same ? "ok" : "bad"}"><td>${esc(x.name)}</td><td class="num">${x.nodes}</td><td class="num">${x.errs}</td><td class="res">${x.same ? "identisch" : "abweichend"}</td><td>${esc(x.detail)}</td></tr>`).join("\n");
const rustRows = rustBins
  .map((b) => {
    const name = b.bin.replace(/^.*\/deps\//, "").replace(/-[0-9a-f]{16}$/, "");
    const isNew = NEW_BINS.some((n) => b.bin.endsWith(n));
    const bad = b.tests.filter((t) => t.status === "FAILED").map((t) => t.name);
    const fresh = isNew ? b.tests.map((t) => t.name) : b.tests.filter((t) => NEW_TESTS.has(t.name)).map((t) => t.name);
    return `<tr class="${bad.length ? "bad" : "ok"}"><td>${esc(name)}${isNew ? ' <span class="tag">neu</span>' : ""}</td><td class="num">${b.tests.filter((t) => t.status === "ok").length} / ${b.tests.length}</td><td>${fresh.map((n) => `<code>${esc(n)}</code>`).join(", ") || "–"}${bad.length ? `<div class="err">${bad.map(esc).join(", ")}</div>` : ""}</td></tr>`;
  })
  .join("\n");
const pwRows = pwNew
  .map((t) => {
    const c = scenarios.find((s) => s.id === t.id);
    const checks = c?.checks?.length ? `<ul class="checks">${c.checks.map((x) => `<li>${esc(x)}</li>`).join("")}</ul>` : esc(c?.caption ?? "");
    return `<tr class="${t.status === "passed" ? "ok" : "bad"}"><td>${esc(t.id)}</td><td>${esc(t.title)}</td><td class="res">${t.status === "passed" ? "bestanden" : "fehlgeschlagen"}</td><td class="num">${(t.ms / 1000).toFixed(1)} s</td><td>${checks}${t.error ? `<div class="err">${esc(t.error.split("\n")[0])}</div>` : ""}</td></tr>`;
  })
  .join("\n");
const blocks = scenarios
  .filter((s) => (s.images ?? []).length)
  .sort((a, b) => a.id.localeCompare(b.id, "de", { numeric: true }))
  .map(
    (s) => `<section class="sc"><h3>${esc(s.id)} ${esc(s.title)}</h3>
${s.images.map((i) => `<figure><img src="img/${esc(i.name)}.jpg" alt="${esc(i.label)}"><figcaption>${esc(i.label)}</figcaption></figure>`).join("")}
${s.code ? `<div class="lbl">Code</div>${code(s.code)}` : ""}<p class="cap">${esc(s.caption)}</p></section>`,
  )
  .join("\n");
const probeRows = probes.map((p) => `<tr><td class="num">${de(p.x, 1)}</td><td class="num">${de(p.law, 4)}</td><td class="num">${de(p.r, 4)}</td><td class="num">${de(((p.r - p.law) / p.law) * 100, 2)} %</td></tr>`).join("");
const ms = (n) => tAfter[n]?.ms ?? null;

const html = `<!doctype html>
<html lang="de">
<head>
<meta charset="utf-8">
<title>Testbericht 7 – Kernel-Korrekturen und variabler Radius</title>
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
<h1>Testbericht 7 – Kernel-Korrekturen und variabler Radius</h1>
<div class="meta">SMLib-Fassade, Engine, Sprache, CLI und Browser-UI · ${esc(date)} · Commit ${esc(git("rev-parse", "--short", "HEAD"))}</div>

<div class="summary">
  <div>cargo test --workspace<b>${cargoSums.length ? cargoSums.map((s) => `${s.ok}/${s.ok + s.failed}`).join(" · ") : "–"}</b>${cargoSums.length} Läufe</div>
  <div>cargo clippy<b>${clippy == null ? "–" : `${clippyWarn} Warnungen`}</b>alle schon vorher, keine neuen</div>
  <div>Node (smlib.wasm)<b>${nodeLines.length ? `${nodeLines.filter((l) => l.startsWith("ok")).length} / ${nodeLines.length}` : "–"}</b>test-node.mjs</div>
  <div>Vitest<b>${vt ? `${unitPassed} / ${unitTotal}` : "–"}</b></div>
  <div>Playwright<b>${pwAll.length ? `${passed(pwAll)} / ${pwAll.length}` : "–"}</b>davon neu ${passed(pwNew)} / ${pwNew.length}</div>
  <div>OpenSCAD-Vergleich<b>${compareLine ? esc((compareLine.match(/\d+\s*\/\s*\d+/) ?? [""])[0]) : "–"}</b>compare.py</div>
  <div>STEP (OCCT)<b>${stepLine ? esc((stepLine.match(/\d+\/\d+/) ?? [""])[0]) : "–"}</b>verify.py</div>
  <div>nativ ↔ Wasm<b>${nativeWasm.length ? `${nativeWasm.filter((x) => x.same).length} / ${nativeWasm.length}` : "–"}</b>identische Szenen</div>
</div>

<h2>1 Umfang</h2>
<ul>
  <li><b>F1</b> <code>shell</code> eines verrundeten Körpers (dose_shell.scad war rot), <b>F2</b> <code>surface(points)</code> mit 2 × 2 oder 3 × 3 Punkten, <b>F3</b> Naht- und Polkanten als eigene Kanten – die drei in Bericht 4, Abschnitt 7 gefundenen Fehler. Dazu <b>F4</b>: fase_chamfer.scad war ebenfalls rot (<code>union(chamfer = 0.5)</code> wählte acht statt zwei Kanten).</li>
  <li><b>Variabler Radius</b> (Spec „Variabler Radius (Stretch Goal)“): <code>fillet(r = function (p) …) { … }</code> und <code>difference/union/intersection(fillet = function (p) …)</code>; Radius als Funktion der Position, an Stützpunkten jeder Kante ausgewertet, Hinweis „r 2–4“, Fehlerfälle wie beim konstanten Radius. Neues Beispiel <code>examples/spec/variabler_radius.scad</code>.</li>
  <li><b>SMLib</b>: Korrektur 1ed8669 (Zeigertabellen in Einfügereihenfolge) aus dem Fork-Zweig <code>capi-solidcraft</code> als Patch <code>smlib/patches/0001-pointer-maps-insertion-order.patch</code> über 7eba8aa (<code>smlib/fetch.sh</code> wendet ihn an). <b>Fassaden-ABI 4</b> (<code>sc_version()</code> = 4): Kantenflags in <code>sc_edges</code>, <code>sc_law_stations</code>, <code>sc_fillet_var</code>; Kernel-ID <code>smlib-7eba8aa+1/facade-4</code> (neue Cache-Schlüssel).</li>
</ul>

<h2>2 Testumgebung</h2>
<table>${env.map(([a, b]) => `<tr><th style="width:30mm">${esc(a)}</th><td>${esc(b)}</td></tr>`).join("")}</table>

<h2>3 Fehler: Ursache, Korrektur, Nachweis</h2>

<div class="fix"><h3>F1 shell eines verrundeten Körpers</h3><dl>
<dt>Ursache</dt><dd>Nicht die Vorprüfung „no cavity“ (sie betrifft nur ebene, gegenüberliegende Wände; die Meldung „leaves no cavity“ kam nach SMLib, das einen offenen Körper zurückgab). SMLibs Schalenbildung mit geöffneter Fläche (<code>SmApiShellBrepFull</code> → <code>SmOffsetExecutive::ShellBrep</code>, Zweig mit Öffnung über <code>SmMerge</code>) trägt an verrundeten Körpern nicht: je nach Optionen <code>SM_ERR_NULL_POINTER</code> in <code>SmMerge.cpp:672</code>, ein offener Körper aus den 26 Eingangsflächen, oder ein geschlossener Körper, dessen Randflächen fehlen und dessen innere Rundungen durch verlängerte Flächen ersetzt sind (11–15 Flächen fallen in der Gültigkeitsprüfung durch, Volumen 0,08 % daneben). Welcher Fall eintrat, hing an der Adressreihenfolge von SMLibs Zeigertabellen (daher nativ und Wasm verschieden). Der <i>geschlossene</i> Innenversatz desselben Körpers (<code>SmApiOffsetBrepFull</code>, −t) ist dagegen exakt (Volumen = Minkowski-Formel auf 10<sup>−7</sup>).</dd>
<dt>Korrektur</dt><dd><code>sc_shell</code> versucht SMLibs Schale; scheitert sie oder überlappt sie sich, baut die Fassade die Schale aus Booleans: <i>Körper − O − P<sub>1</sub> − … − P<sub>k</sub></i>, O der geschlossene Innenversatz, P<sub>i</sub> ein Prisma über der Versatzfläche jeder geöffneten (ebenen) Fläche, von knapp innerhalb des Hohlraums bis über den Körper hinaus (aufgedickte Einzelfläche). Die Seitenwände des Prismas gehen durch den Rand der Versatzfläche: der Öffnungsrand ist das Band zwischen geöffneter Fläche und ihrem Versatz, wie bei SMLibs eigener Schale. Booleans über die geprüften Booleans der Fassade, Ergebnis mit der Gültigkeitsprüfung. Zusätzlich Patch 0001 (1ed8669): SMLibs Zeigertabellen geben Schlüssel in Einfügereihenfolge aus – Ergebnisse hängen nicht mehr von der Heap-Lage ab.</dd>
<dt>Nachweis</dt><dd>Volumen gegen die geschlossene Form (abgerundeter Quader minus innerer abgerundeter Quader mit r − t minus Wandband unter der ebenen Deckfläche), STEP-Prüfung, gleiche Szenen nativ und Wasm (Abschnitt 5), Rust-Tests <code>shell_of_rounded_boxes</code>, <code>dose_shell_example_is_a_hollow_box</code>, <code>shell_of_a_rounded_box_open_at_the_top</code>, Node-Test, Playwright E03.</dd></dl>
<table><tr><th>Fall</th><th>vorher (cad check)</th><th>erwartet</th><th>nachher</th><th>rel. Abw.</th><th>STEP</th><th>Diagnosen nachher</th></tr>${f1Table}
<tr class="${doseAfter ? "ok" : "bad"}"><td>dose_shell.scad (mit Deckel)</td><td>${list(doseBefore)}</td><td class="num">–</td><td class="num">${mm3(doseAfter?.volume)}</td><td class="num">–</td><td class="num">${doseAfter?.step ? `${doseAfter.step.faces ?? "–"} Flächen` : "–"}</td><td class="res">${check(cadNew, dose).length ? "Fehler" : "ok"}</td></tr></table>
${ba("f1-dose", "dose_shell.scad, Modul dose (Cursor auf shell, Zeile 9)")}
${ba("f1-faelle", "die drei Fälle aus Bericht 4")}
</div>

<div class="fix"><h3>F2 surface(points) mit 2 × 2 oder 3 × 3 Punkten</h3><dl>
<dt>Ursache</dt><dd><code>SmApiCreateSurfaceFromOrderedPoints</code> ruft <code>SmBSplineSurface::ApproximatePoints</code> fest mit Grad 3 in beiden Richtungen auf; das braucht mindestens 4 Punkte je Richtung, sonst <code>SM_ERR</code>.</dd>
<dt>Korrektur</dt><dd><code>sc_nurbs_surface</code> ruft <code>ApproximatePoints</code> selbst auf, mit Grad min(3, n − 1) je Richtung getrennt: bilinear bei 2, quadratisch bei 3, kubisch ab 4 (ab 4 × 4 unverändert). Mit so vielen Kontrollpunkten wie Punkten interpoliert die Ausgleichsrechnung die Punkte.</dd>
<dt>Nachweis</dt><dd><code>surfaces_get_the_degree_their_point_count_allows</code> (Grade 1/1, 2/2, 1/3, 3/1, 3/3 im B-Rep, Punkte auf der Fläche), <code>surfaces_from_two_and_three_points_per_direction</code> (ebene Gitter 2 × 2 … 4 × 4: Fläche 400 mm²), Node-Test, Playwright E05.</dd>
<dt>Meldungen</dt><dd>vorher: ${list(f2Msgs.before)}<br>nachher: ${list(f2Msgs.after)}</dd></dl>
${ba("f2-flaechen", "2 × 2, 3 × 3 und 2 × 5 Punkte, aufgedickt")}
</div>

<div class="fix"><h3>F3 Naht- und Polkanten zählten als Kanten</h3><dl>
<dt>Ursache</dt><dd>Die Engine wählte bei <code>$fillet/$chamfer</code> alle Kanten des Primitivs und in Zonen alle Kanten in der Zone. SMLibs Zylinder hat eine Naht (gleiche Fläche auf beiden Seiten), die Kugel eine Naht von Pol zu Pol. Die Fassade übersprang solche Kanten beim Verrunden (<code>GetBlendEdgeuse() == NULL</code>), gezählt und gezeichnet wurden sie trotzdem; bei der Kugel blieb keine Kante übrig → Fehler.</dd>
<dt>Korrektur</dt><dd><code>sc_edges</code> meldet je Kante Flags, <code>SC_EDGE_SMOOTH</code> = nichts zu verrunden (Naht, Pol, tangential stetiger Übergang innerhalb 2°, offener Rand; SMLibs eigenes Kriterium). <code>EdgeInfo.smooth</code>; die Engine wählt solche Kanten nie – bei <code>$fillet/$chamfer</code>, in Zonen und bei <code>fillet =</code> an Booleans. Ein Primitiv ohne Kante zum Verrunden ergibt „0 Kanten“ als Warnung ohne Wirkung. Auch die tangentialen Ränder früherer Rundungen zählen nicht mehr (verschachtelte Blöcke).</dd>
<dt>Hinweise</dt><dd>Zylinder vorher: ${list(f3Hints[0].before)} · nachher: ${list(f3Hints[0].after)}<br>Kugel vorher: ${list(f3Hints[1].before)} · nachher: ${list(f3Hints[1].after)}<br>Meldungen vorher: ${list(f3Msgs.before)} · nachher: ${list(f3Msgs.after)}</dd>
<dt>Nachweis</dt><dd><code>seams_and_poles_are_not_edges</code> (Zylinder 2 Kanten, Volumen mit zwei Fasenringen; Kugel 0 Kanten, Warnung, Volumen unverändert; Zone um den Zylinder 2 Kanten; Vereinigung zweier gleicher Zylinder: 0 neue Kanten), <code>tangent_edges_of_rounds_are_not_selected</code>, <code>edges_flag_seams_poles_and_tangent_edges_as_smooth</code>, Playwright E04 und C05.</dd></dl>
${ba("f3-zylinder", "cylinder(…, $chamfer = 1), Cursor Zeile 1 (ausgewählte Kanten orange)")}
${ba("f3-kugel", "sphere(5, $fillet = 1), Cursor Zeile 2")}
</div>

<div class="fix"><h3>F4 fase_chamfer.scad: union(chamfer) wählte acht Kanten</h3><dl>
<dt>Ursache</dt><dd>„Neue Kanten“ einer Booleschen Operation waren Kanten zwischen Flächen verschiedener Operanden (Flächen-Tags). Teilen sich zwei Operanden eine Ebene (Platte und Turm mit gleicher Vorder- und Rückseite), verschmilzt SMLib die Flächen zu einer mit dem Tag eines Operanden; die sechs alten Turmkanten an Vorder- und Rückseite trennten dann Flächen verschiedener Operanden und galten als neu. Die Fase an diesen Kanten scheiterte (<code>SM_ERR</code>).</dd>
<dt>Korrektur</dt><dd>Eine Kante, die auf einer Kante eines Operanden liegt, ist nicht neu (Abtastpunkte auf dem Kantenzug eines Operanden, Toleranz 10<sup>−6</sup> der Modellgröße).</dd>
<dt>Hinweise</dt><dd>vorher: ${list(f4Hints.before)} · nachher: ${list(f4Hints.after)}<br>Meldungen vorher: ${list(f4Msgs.before)} · nachher: ${list(f4Msgs.after)}</dd>
<dt>Nachweis</dt><dd><code>coplanar_merged_faces_keep_old_edges_old</code> (2 Kanten, Volumen 2502,5 mm³), fase_chamfer.scad fehlerfrei (Stufe 1, STEP, Laufzeittabelle).</dd></dl>
${ba("f4-union", "union(chamfer = 0.5) aus fase_chamfer.scad, Cursor Zeile 1")}
</div>

<h2>4 Variabler Radius</h2>
<h3>Architektur: auswertbare Beschreibung statt Rückruf</h3>
<p>Die Kanten, an denen der Radius gebraucht wird, gibt es erst, wenn der Kernel den Körper gebaut hat – lange nach der Auswertung, in der Engine. Zwei Wege: ein Rückruf aus der Engine in die Auswertung, oder die Auswertung liefert eine auswertbare Beschreibung der Funktion. Gewählt ist die Beschreibung: <code>cad-eval</code> übersetzt das Funktionsliteral in ein <b>Radiusgesetz</b> (<code>cad_ir::Law</code>), einen kleinen reinen Ausdruck über <code>p</code>. Was nicht von <code>p</code> abhängt (eingefangene Variablen, Rasterebenen, Listenausdrücke, beliebige Builtins), wird dabei schon ausgewertet und ist Konstante; Aufrufe von Benutzerfunktionen und Funktionswerten werden eingesetzt (Parameter als lokale Slots, Tiefe ≤ 16); übrig bleiben Literale, Arithmetik, Vergleiche, <code>&amp;&amp; || ! ?:</code>, <code>let</code>, Indizes, <code>.x/.y/.z</code>, Vektoren und die Mathe-Builtins (<code>sin … lookup</code>).</p>
<ul>
  <li>Das Gesetz ist Teil des Baums (serialisierbar, Teil des Cache-Schlüssels): eine geänderte eingefangene Variable ändert den Schlüssel, gleicher Quelltext trifft den Cache. Ein Rückruf bräuchte dafür einen eigenen Schlüssel der Closure samt Umgebung.</li>
  <li>Die Engine hängt weiter nicht von <code>cad-eval</code> und dessen AST-Lebensdauer ab; Wasm und nativ werten dasselbe Datenobjekt aus, auch aus dem Cache, ohne erneute Auswertung des Quelltexts.</li>
  <li>Grenze: nur der reine Ausdruckskern ist von <code>p</code> abhängig möglich (keine Texte, keine Listenausdrücke über <code>p</code>, keine Rekursion über <code>p</code>); solche Fälle sind Fehler an der Funktion mit eigener Meldung.</li>
</ul>
<h3>Ablauf</h3>
<ol>
  <li>Auswertung: <code>fillet(r = function …)</code> → <code>Op::Blend { law }</code>, <code>difference(fillet = function …)</code> → <code>Op::Boolean { fillet_law }</code>. <code>chamfer</code> und <code>$fillet</code> mit Funktion: Warnung, nicht unterstützt.</li>
  <li>Engine: Kantenwahl wie beim konstanten Radius (Zonen, neue Kanten, ohne glatte Kanten); je Kante <i>m</i> Stützpunkte (etwa einer je 1,5 mm, 4 bis 32) von <code>Kernel::law_stations</code>; das Gesetz im System der fillet-/Boolean-Anweisung an jedem Stützpunkt ausgewertet. Radius ≤ 0, keine Zahl → Fehler mit Wert und Position, Kante rot, Körper unverändert. Alle Werte gleich → konstante Verrundung (gleiches Ergebnis wie <code>fillet(r)</code>). Sonst <code>Kernel::fillet_variable</code>; Kernelfehler wie beim konstanten Radius mit dem Bereich („Verrundung gescheitert (8–18 mm): …“). Hinweis am Zeilenende: „r <i>min</i>–<i>max</i>“ der Stützwerte.</li>
  <li>Fassade (<code>sc_law_stations</code>, <code>sc_fillet_var</code>): das Gesetz einer Kante ist ein geklemmter B-Spline vom Grad min(3, m − 1) über dem normierten Kantenparameter mit gleichmäßigen inneren Knoten, Kontrollradius <i>j</i> am Greville-Punkt t<sub>j</sub> (dort liegen die Stützpunkte). Er bleibt zwischen kleinstem und größtem Wert, ist exakt für konstante und lineare Verläufe und nimmt an den Kantenenden den ersten und letzten Wert an. Als <code>SmBSplineFilletLaw</code> trägt die Gesetzeskurve den Radius als x und t selbst als y (Kontrollpunkte (r<sub>j</sub>, t<sub>j</sub>), also y(t) = t), damit SMLib sie an Ecken über die Kantenenden hinaus verlängern kann. Je Kante SMLibs Variabler-Radius-Löser (<code>SM_FS_VARIABLE_RADIUS</code>), Kanten mit gleichen Werten der konstante.</li>
</ol>
<h3>Messwerte</h3>
<table><tr><th>x (mm)</th><th>Gesetz 1 + x/15</th><th>gemessen (Beginn der Deckfläche)</th><th>Abw.</th></tr>${probeRows || "<tr><td colspan=4>–</td></tr>"}</table>
<p class="small">Eine Kante eines 30 × 20 × 20-Quaders mit linearem Gesetz: Radius gemessen über Punktklassifikation knapp unter der Deckfläche (dort beginnt die Rundung; bei wachsendem Radius liegt die Berührlinie etwas hinter r(x)). Volumen gegen die geschlossene Form 12000 − (1 − π/4)·130: Abweichung &lt; 10<sup>−5</sup> (Rust- und Node-Test).
${volStep ? `Stufengesetz <code>p.z &lt; 10 ? 2 : 4</code> an allen 12 Kanten eines 30 × 30 × 20-Quaders: ${de(+volStep[2])} mm³, zwischen r = 4 (${de(+volStep[3])} mm³) und r = 2 (${de(+volStep[1])} mm³).` : ""}
${volPocket ? `Taschenrand mit <code>1 + p.x / 40</code>: ${de(+volPocket[2])} mm³ zwischen r = 1,875 (${de(+volPocket[1])} mm³) und r = 1,125 (${de(+volPocket[3])} mm³).` : ""}
variabler_radius.scad: ${mm3(vrExport?.volume)}, STEP ${vrExport?.step ? `${vrExport.step.faces ?? "–"} Flächen` : "–"}, <code>cad export</code> ${ms("spec/variabler_radius.scad") ?? "–"} ms.</p>
<table><tr><th>Zeile</th><th>vorher</th><th>nachher</th></tr>
${[11, 16, 22].map((l, i) => `<tr><td>${l}</td><td>${list(vrHints[i].before)}</td><td>${list(vrHints[i].after)}</td></tr>`).join("")}
<tr><td>Diagnosen</td><td>${list(vrMsgs.before)}</td><td>${list(vrMsgs.after)}</td></tr></table>
${ba("vr-stufe", "variabler_radius.scad, Cursor Zeile 11 (Stufengesetz)")}
${ba("vr-tasche", "Zeile 16: difference(fillet = function (p) 1 + p.x / 40)")}
${ba("vr-lookup", "Zeile 22: lookup-Gesetz mit Zone an den Deckelkanten")}

<h2>5 Nativ und Wasm</h2>
<p class="small">Szene von <code>cad eval --json</code> (nativ) gegen <code>web/scripts/dump-scene.mjs</code> (Browser-Engine in Node: cad-wasm + smlib.wasm) je Knoten: Status, Hinweise, Anzahl ausgewählter Kanten, Hülle (10<sup>−4</sup> mm), Meldungen, und die Diagnosen.</p>
<table><tr><th>Datei</th><th>Knoten</th><th>davon Fehler</th><th>Ergebnis</th><th>Abweichung</th></tr>${nwRows || "<tr><td colspan=5>– (web/src/wasm-pkg fehlt)</td></tr>"}</table>

<h2>6 Laufzeiten cad export (alle Beispiele)</h2>
<p class="small"><code>cad export &lt;datei&gt; -o x.stl</code> (Release, Wandzeit inkl. Start, ein Lauf je Datei, nacheinander) vorher (Stand vor diesem Auftrag) und nachher. Summe über ${common.length} gemeinsame Dateien: vorher ${sum(tBefore, common)} ms, nachher ${sum(tAfter, common)} ms. Exit 1 = Diagnosen mit Fehlern (vorher dose_shell, fase_chamfer).</p>
<table><tr><th>Datei</th><th>vorher</th><th>nachher</th><th>Δ</th></tr>${timeRows}</table>

<h2>7 Tests</h2>
<ul>
  <li><b>cargo test --workspace</b> (${cargoSums.length} Läufe, ohne Ausfälle zwischen den Läufen): ${cargoSums.map((s) => `${s.ok} bestanden, ${s.failed} fehlgeschlagen`).join(" · ") || "–"}.</li>
  <li><b>cargo clippy --workspace --all-targets</b>: ${clippy == null ? "–" : `${clippyWarn} Warnungen, ${clippyErr} Fehler`}; die Warnungen stehen in nicht geänderten Dateien (${clippyFiles.map((f) => `<code>${esc(f)}</code>`).join(", ")}) und bestanden schon vorher – keine neuen.</li>
  <li><b>node crates/kernel-smlib/js/test-node.mjs</b> (smlib.wasm über die Brücke): ${nodeLines.length ? `${nodeLines.filter((l) => l.startsWith("ok")).length} / ${nodeLines.length}` : "–"}${nodeLines.length ? ` – neu: Kantenflags, surface 2 × 2/3 × 3/2 × 5, shell eines verrundeten Quaders (Volumen auf 10<sup>−6</sup>), variabler Radius (Stützpunkte, lineares Gesetz, Radius ≤ 0)` : ""}.</li>
  <li><b>OpenSCAD-Vergleich</b> (compare.py): <code>${esc(compareLine ?? "–")}</code>.</li>
  <li><b>STEP</b> mit OpenCASCADE (<code>tests/step/verify.py</code> über alle Beispiele): <code>${esc(stepLine?.replace(/; report.*$/, "") ?? "–")}</code>.</li>
  <li><b>Vitest</b>: ${vt ? `${unitPassed} / ${unitTotal}` : "–"}; <b>pnpm typecheck</b> ohne Fehler.</li>
  <li><b>Playwright</b>: ${pwAll.length ? `${passed(pwAll)} / ${pwAll.length}` : "–"} (bisherige Szenarien ${passed(pwOld)} / ${pwOld.length}, neu ${passed(pwNew)} / ${pwNew.length}).</li>
</ul>
<table><tr><th>ID</th><th>Szenario (web/e2e/ui-e.spec.ts)</th><th>Ergebnis</th><th>Dauer</th><th>Geprüft</th></tr>${pwRows}</table>
${cargo ? `<table><tr><th>Rust-Testprogramm (letzter Lauf)</th><th>bestanden</th><th>neue Tests</th></tr>${rustRows}</table>` : ""}

<h2>8 Grenzen</h2>
<ul>
  <li><b>shell aus Booleans</b> nur für ebene geöffnete Flächen (das Prisma ist die aufgedickte Versatzfläche); bei gekrümmten geöffneten Flächen bleibt SMLibs Ergebnis bzw. dessen Fehlermeldung. Geschlossene Schalen (ohne Zone) nehmen weiter SMLibs Ergebnis, wenn es gültig ist. Die Booleans kosten Zeit: dose_shell ${tBefore["spec/dose_shell.scad"] ? `${tBefore["spec/dose_shell.scad"].ms} ms (scheiternd) → ${tAfter["spec/dose_shell.scad"]?.ms ?? "–"} ms` : ""} (Abschnitt 6); in smlib.wasm (Node-Test, 90 × 34 × 18, r 3) ${esc(nodeShellMs ?? "–")}.</li>
  <li><b>Wanddicke ≥ Rundungsradius</b> (r ≤ t, z. B. 30 × 20 × 10 mit r 1, Wand 2): der Innenversatz wäre scharfkantig; SMLibs Versatz scheitert dort in 7eba8aa (<code>SM_ERR</code>, Fork-Commit a252570 nicht übernommen), die Schale bleibt wie vorher ein Fehler (Meldung nennt beide Wege).</li>
  <li><b>Variabler Radius:</b> SMLibs Variabler-Radius-Löser ist empfindlich. Steile Verläufe mit großem Radius an Taschenecken scheitern („überlappt sich“, z. B. Taschenrand mit 1 + p.x/20, 1,25–2,75), ebenso Kreiskanten mit veränderlichem Radius (<code>SM_ERR</code>); das Beispiel nutzt deshalb 1 + p.x/40. Sprünge (<code>p.z &lt; 10 ? 2 : 4</code> an senkrechten Kanten) werden über etwa drei Stützpunktabstände geglättet; der Hinweis zeigt den Bereich der Stützwerte (lookup-Spitze 4 → „3.9“, weil kein Stützpunkt genau auf x = 20 liegt). Position <code>p</code> im System der fillet-/Boolean-Anweisung. Kein variabler Radius für <code>chamfer</code> und <code>$fillet</code> (Warnung).</li>
  <li><b>Glatte Kanten:</b> SMLibs Kriterium (Flächen innerhalb 2° tangential) – fast tangentiale Kanten werden nie gewählt. Die Prüfung „alte Kante eines Operanden“ (F4) vergleicht mit den Kantenzügen der Operanden, für gekrümmte Kanten nur so genau wie deren Abtastung (dort bleibt die frühere Wahl).</li>
  <li><b>SMLib-Patch</b> als Arbeitskopie-Patch über 7eba8aa (der Fork-Commit liegt nicht auf <code>capi-tess</code>); vorgebaute Bibliotheken (<code>SMLIB_PREBUILT</code>) müssen mit dem Patch neu gebaut werden (Header-Änderung, sonst ODR-Bruch). Ein Symlink <code>third_party/smlib</code> wird von <code>fetch.sh</code> nur geprüft.</li>
</ul>

<h2>9 Screenshots der Browser-UI</h2>
${blocks || "<p>–</p>"}
</main>
</body>
</html>
`;

writeFileSync(join(out, "report.html"), html);
const sizes = readdirSync(join(out, "img")).map((f) => statSync(join(out, "img", f)).size);
console.log(`report7: ${f1Rows.length} F1-Fälle, nativ↔Wasm ${nativeWasm.filter((x) => x.same).length}/${nativeWasm.length}, Playwright ${passed(pwAll)}/${pwAll.length}, ${sizes.length} Bilder (max ${(Math.max(0, ...sizes) / 1024).toFixed(0)} KiB)`);
console.log(`-> ${join(out, "report.html")}`);
console.log(`PDF: node tools/report/print-pdf.mjs ${join(out, "report.html")} ${join(out, "report.pdf")} "Testbericht 7: Kernel-Korrekturen und variabler Radius"`);
