#!/usr/bin/env node
// Test report 11 ("colors, multi-filament 3MF, dark theme"): collects the results of the
// test runs and writes docs/reports/11-farben-filamente/report.html (images in img/).
//
// Inputs (gitignored, reports/tmp/report11/):
//   palette.json, outline.json, measure.json, export*.txt, 3dmodel.model, model_settings.config,
//   verify.json/.txt, zufall.json, sources.sha256   tools/report/report11-data.sh (CLI, Python)
//   scenarios.json, metrics.json, img/*.jpg          cd web && pnpm exec playwright test e2e/farben.spec.ts
//   playwright.json                                  copy of reports/tmp/report2/playwright.json (whole suite)
//   vitest.txt                                       cd web && pnpm test
//   tauri/tauri-e2e.json, tauri/img/*.png            node web/e2e-tauri/desktop.e2e.mjs --bin target/release/mcad --out reports/tmp/report11/tauri
//   cargo.txt, clippy.txt, fmt.txt                   cargo test --workspace --no-fail-fast, clippy -D warnings, fmt --check
//   compare.txt, step.txt                            tests/openscad-compat/compare.py --eval, tests/step/verify.py
//
//   node tools/report/report11.mjs
//   node tools/report/print-pdf.mjs docs/reports/11-farben-filamente/report.html docs/reports/11-farben-filamente/report.pdf "Test report 11: colors, multi-filament 3MF, dark theme"

import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import os from "node:os";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "../..");
const input = join(root, "reports/tmp/report11");
const out = join(root, "docs/reports/11-farben-filamente");
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
const num = (x, d = 0) => (x === null || x === undefined || Number.isNaN(Number(x)) ? "–" : Number(x).toLocaleString("en-US", { minimumFractionDigits: d, maximumFractionDigits: d }));
const md = (t) => esc(t).replace(/`([^`]+)`/g, "<code>$1</code>");

// ------------------------------------------------------------------------------- inputs

const palette = readJson(join(input, "palette.json"));
const outline = readJson(join(input, "outline.json"));
const measure = readJson(join(input, "measure.json"));
const exportJson = readJson(join(input, "export.json"));
const exportZip = readText(join(input, "export-zip.txt"));
const zipList = readText(join(input, "zip-list.txt"))?.replaceAll(`${input}/`, "");
const list3mf = readText(join(input, "3mf-list.txt"))?.replaceAll(`${input}/`, "");
const model = readText(join(input, "3dmodel.model"));
const settings = readText(join(input, "model_settings.config"));
const verify = readJson(join(input, "verify.json"));
const verifyText = readText(join(input, "verify.txt"));
const zufall = readJson(join(input, "zufall.json"));
const zufallCheck = readText(join(input, "zufall-check.txt"))?.replaceAll(`${input}/`, "");
const scenarios = (readJson(join(input, "scenarios.json")) ?? []).sort((a, b) => a.id.localeCompare(b.id));
const metrics = readJson(join(input, "metrics.json")) ?? {};
const playwright = readJson(join(input, "playwright.json"));
const vitest = readText(join(input, "vitest.txt"));
const tauri = readJson(join(input, "tauri/tauri-e2e.json"));
const cargo = readText(join(input, "cargo.txt"));
const clippy = readText(join(input, "clippy.txt"));
const fmt = readText(join(input, "fmt.txt"));
const compare = readText(join(input, "compare.txt"));
const step = readText(join(input, "step.txt"));
const sources = readText(join(input, "sources.sha256"));
const example = readText(join(root, "tests/models/spec/multicolor.mcad"));
const css = readText(join(root, "web/src/styles.css")) ?? "";

// images: Playwright (jpg), Tauri (png → jpg with ImageMagick if available)
const copied = new Set();
function img(name, from) {
  const src = join(from, name);
  if (!existsSync(src)) return null;
  let dst = name;
  if (name.endsWith(".png") && from.includes("tauri")) {
    dst = `tauri-${name.replace(/\.png$/, ".jpg")}`;
    try {
      execFileSync("convert", [src, "-resize", "1440x>", "-quality", "84", join(imgDir, dst)]);
    } catch {
      dst = `tauri-${name}`;
      copyFileSync(src, join(imgDir, dst));
    }
  } else copyFileSync(src, join(imgDir, dst));
  copied.add(dst);
  return `img/${dst}`;
}
const pwImg = (n) => img(`${n}.jpg`, join(input, "img"));
const tauriImg = (n) => img(n, join(input, "tauri/img"));

// ------------------------------------------------------------------------------- summaries

const cargoSum = (() => {
  if (!cargo) return null;
  let ok = 0;
  let failed = 0;
  let ignored = 0;
  for (const m of cargo.matchAll(/test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored/g)) {
    ok += +m[1];
    failed += +m[2];
    ignored += +m[3];
  }
  return { ok, failed, ignored };
})();
// cargo prints `Running tests/colors.rs (…)` without the crate: the crate comes from the
// `Running unittests src/… (target/debug/deps/<crate>-<hash>)` line before it.
const cargoSections = (() => {
  const out = [];
  let crate = "?";
  for (const s of (cargo ?? "").split(/\n\s*Running /).slice(1)) {
    const head = s.split("\n")[0];
    const u = /^unittests \S+ \(target\/debug\/deps\/([A-Za-z0-9_]+)-[0-9a-f]+\)/.exec(head);
    if (u) crate = u[1] === "cad" ? "cad-cli" : u[1].replace(/_/g, "-");
    out.push({ file: `${crate}/${head.split(" ")[0]}`, text: s });
  }
  return out;
})();
const testsOf = (file) =>
  cargoSections
    .filter((s) => s.file === file)
    .flatMap((s) => [...s.text.matchAll(/^test (\S+) \.\.\. (ok|FAILED|ignored)$/gm)].map((m) => ({ name: m[1], res: m[2] })));
const colorTests = [
  ...testsOf("cad-eval/tests/palette.rs").map((t) => ({ ...t, file: "cad-eval/tests/palette.rs" })),
  ...testsOf("cad-eval/tests/filament_edit.rs").map((t) => ({ ...t, file: "cad-eval/tests/filament_edit.rs" })),
  ...testsOf("cad-engine/tests/colors.rs").map((t) => ({ ...t, file: "cad-engine/tests/colors.rs" })),
  ...testsOf("cad-session/tests/colors.rs").map((t) => ({ ...t, file: "cad-session/tests/colors.rs" })),
  ...testsOf("cad-cli/tests/colors.rs").map((t) => ({ ...t, file: "cad-cli/tests/colors.rs" })),
];
const pwStats = (() => {
  if (!playwright) return null;
  const all = [];
  const walk = (s) => {
    for (const sp of s.specs ?? []) for (const t of sp.tests ?? []) all.push({ title: sp.title, file: sp.file, status: t.results?.at(-1)?.status ?? t.status });
    for (const c of s.suites ?? []) walk(c);
  };
  for (const s of playwright.suites ?? []) walk(s);
  return { total: all.length, passed: all.filter((t) => t.status === "passed").length, failed: all.filter((t) => t.status !== "passed" && t.status !== "skipped"), farben: all.filter((t) => t.file?.includes("farben")) };
})();
const vitestSum = vitest ? (/Tests\s+(\d+) passed/.exec(vitest)?.[1] ?? null) : null;
const tauriOk = tauri ? tauri.results.filter((r) => r.ok).length : null;
const compareSum = compare ? /(\d+)\/(\d+)/.exec(compare.split("\n").reverse().find((l) => /\d+\/\d+/.test(l)) ?? "")?.slice(1, 3) : null;
const stepSum = step ? (step.split("\n").reverse().find((l) => /\d+\/\d+/.test(l)) ?? "").split(";")[0].trim() : null;
const clippyOk = clippy != null && !/^(error|warning)/m.test(clippy);
const fmtOk = fmt != null && fmt.trim() === "";

const date = new Date().toISOString().slice(0, 10);
const commit = git("rev-parse", "--short", "HEAD");
const branch = git("rev-parse", "--abbrev-ref", "HEAD");
const rustc = (() => {
  try {
    return execFileSync("rustc", ["--version"], { encoding: "utf8" }).trim();
  } catch {
    return "–";
  }
})();

// ------------------------------------------------------------------------------- contrast (styles.css)

function parseCss(c) {
  const s = String(c ?? "").trim().toLowerCase();
  let m = /^#([0-9a-f]{6})$/.exec(s);
  if (m) return { rgb: [0, 2, 4].map((i) => parseInt(m[1].slice(i, i + 2), 16)), a: 1 };
  m = /^rgba?\(([^)]+)\)$/.exec(s);
  if (m) {
    const p = m[1].split(/[ ,/]+/).filter(Boolean).map(Number);
    return { rgb: p.slice(0, 3), a: p.length > 3 ? p[3] : 1 };
  }
  return null;
}
const lin = (x) => (x / 255 <= 0.04045 ? x / 255 / 12.92 : ((x / 255 + 0.055) / 1.055) ** 2.4);
const L = (c) => 0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2]);
const ratio = (a, b) => (Math.max(L(a), L(b)) + 0.05) / (Math.min(L(a), L(b)) + 0.05);
const over = (t, b) => [0, 1, 2].map((i) => t.rgb[i] * t.a + b[i] * (1 - t.a));
function tokens(theme) {
  const outT = {};
  const re = new RegExp(`:root\\[data-theme="${theme}"\\][^{]*\\{([^}]*)\\}`, "g");
  let m;
  while ((m = re.exec(css))) for (const d of m[1].split(";")) {
    const i = d.indexOf(":");
    if (i > 0 && d.slice(0, i).trim().startsWith("--")) outT[d.slice(0, i).trim()] = d.slice(i + 1).trim();
  }
  return outT;
}
const PAIRS = [
  ["Text", "--text", "--panel"],
  ["Muted text", "--muted", "--panel"],
  ["Muted text on surface 2", "--muted", "--panel-2"],
  ["Accent (links, lines of the color list)", "--accent", "--panel"],
  ["Error", "--error", "--panel"],
  ["Error on error line", "--error", "--error-bg", "--panel"],
  ["Warning on warning line", "--warning", "--warning-bg", "--panel"],
  ["Syntax: keyword", "--hl-keyword", "--panel"],
  ["Syntax: number", "--hl-number", "--panel"],
  ["Syntax: string", "--hl-string", "--panel"],
  ["Syntax: comment", "--hl-comment", "--panel"],
  ["Syntax: comment in active line", "--hl-comment", "--active-line", "--panel"],
  ["Syntax: definition", "--hl-def", "--panel"],
  ["Syntax: modifier", "--hl-modifier", "--panel"],
  ["Text on selection", "--text", "--accent-soft", "--panel"],
  ["Text on diff (new)", "--text", "--diff-add", "--panel"],
  ["Text on diff (deleted)", "--text", "--diff-del", "--panel"],
  ["Text on accent", "--on-accent", "--accent"],
  ["Snap label", "--on-snap", "--view-snap"],
  ["Git gutter new (UI, ≥ 3)", "--git-added", "--panel"],
  ["3D: edges on model (≥ 3)", "--view-edge", "--view-model"],
  ["3D: grid on background (≥ 3)", "--view-grid", "--view-bg"],
];
const contrastRows = PAIRS.map(([label, t, b, base]) => {
  const res = {};
  for (const th of ["light", "dark"]) {
    const tk = tokens(th);
    const fg = parseCss(tk[t]);
    const bg = parseCss(tk[b]);
    const bs = base ? parseCss(tk[base]) : null;
    if (!fg || !bg) {
      res[th] = null;
      continue;
    }
    const bb = bg.a < 1 ? over(bg, bs ? bs.rgb : [255, 255, 255]) : bg.rgb;
    res[th] = { r: ratio(fg.a < 1 ? over(fg, bb) : fg.rgb, bb), fg: tk[t], bg: tk[b] };
  }
  return { label, t, b, min: label.includes("≥ 3") ? 3 : 4.5, ...res };
});

// ------------------------------------------------------------------------------- tables

const sw = (hex, extra = "") => `<span class="sw" style="background:${esc(hex)}${extra}"></span>`;
const paletteTable = palette
  ? `<table class="pal"><tr><th>Name</th><th>Base value (export)</th><th>shown light<br><span class="small">on ${esc(palette.view_bg.light)}</span></th><th>shown dark<br><span class="small">on ${esc(palette.view_bg.dark)}</span></th><th>Deut.</th><th>Prot.</th><th>Trit.</th><th>OKLCH (L · C · h)</th></tr>
${palette.colors
  .map(
    (c) => `<tr><td><b>${esc(c.name)}</b></td><td>${sw(c.hex)} <code>${esc(c.hex)}</code></td>
<td class="bgl">${sw(c.light)} <code>${esc(c.light)}</code> <span class="small">${num(c.contrast_light_bg, 2)}:1</span></td>
<td class="bgd">${sw(c.dark)} <code>${esc(c.dark)}</code> <span class="small">${num(c.contrast_dark_bg, 2)}:1</span></td>
<td>${sw(c.sim.deuteranopia)}</td><td>${sw(c.sim.protanopia)}</td><td>${sw(c.sim.tritanopia)}</td>
<td class="small">${c.oklch.map((x, i) => num(x, i === 2 ? 1 : 3)).join(" · ")}</td></tr>`,
  )
  .join("\n")}
<tr><td><b>Default</b> <span class="small">(without color())</span></td><td>${sw(palette.default)} <code>${esc(palette.default)}</code></td><td colspan="6" class="small" style="white-space:normal">Material without <code>color()</code> in exports and in the color list; in the view the model color of the theme (<code>--view-model</code>)</td></tr>
</table>`
  : "<p>palette.json missing</p>";
const strip = (key) =>
  palette ? `<div class="strip">${palette.colors.map((c) => `<span title="${esc(c.name)}" style="background:${esc(key === "hex" ? c.hex : c.sim[key])}"></span>`).join("")}</div>` : "";
const closest = palette
  ? `<table><tr><th>Vision</th><th>smallest distances ΔE<sub>OK</sub> (OKLab, ~0.02 just visible)</th></tr>
${Object.entries(palette.closest)
  .map(([k, v]) => `<tr><td>${esc({ normal: "normal vision", deuteranopia: "deuteranopia (green)", protanopia: "protanopia (red)", tritanopia: "tritanopia (blue)" }[k] ?? k)}</td><td>${v.map((p) => `${esc(p.a)}/${esc(p.b)} <b>${num(p.de, 3)}</b>`).join(" · ")}</td></tr>`)
  .join("\n")}</table>`
  : "";

const randomRows = (zufall?.colors ?? []).map((c) => `<tr><td><code>color("${esc(c.label)}")</code></td><td>${sw(c.key)} <code>${esc(c.key)}</code></td><td>${esc(c.source)}</td></tr>`).join("\n");

const colorsTable = (rows) =>
  `<table><tr><th>Slot</th><th>Color</th><th>Source</th><th>Lines</th><th>Volume</th><th>Mass PLA</th></tr>
${(rows ?? [])
  .map((c) => `<tr><td><b>F${c.filament}</b>${c.explicit ? " <span class='small'>(filament = " + c.filament + ")</span>" : ""}</td><td>${sw(c.key)} ${esc(c.label)} <code>${esc(c.key)}</code></td><td>${esc(c.source)}</td><td>${esc((c.lines ?? []).join(", "))}</td><td class="n">${num(c.volume, 1)} mm³</td><td class="n">${num(c.mass_g, 2)} g</td></tr>`)
  .join("\n")}</table>`;
const partsTable = (measure?.colors?.parts ?? [])
  .map(
    (p) => `<tr><td><b>${esc(p.part ?? "model")}</b></td><td>${p.regions.map((r) => `F${r.filament} ${esc(r.label)} ${num(r.volume, 2)}`).join(" · ")}</td><td class="n">${num(p.regions.reduce((a, r) => a + r.volume, 0), 3)}</td><td class="n">${num(p.total, 3)}</td><td>${p.warning ? esc(p.warning) : "–"}</td></tr>`,
  )
  .join("\n");

// 3MF excerpts
const modelExcerpt = (() => {
  if (!model) return "";
  const lines = model.split("\n");
  const head = lines.slice(0, 11);
  const firstVerts = lines.findIndex((l) => l.includes("<vertices>"));
  const comp = lines.findIndex((l) => l.includes("<components>"));
  const build = lines.findIndex((l) => l.includes("<build>"));
  return [
    ...head,
    ...lines.slice(firstVerts + 1, firstVerts + 3),
    "     …",
    ...lines.slice(comp - 1, comp + 5),
    "  …",
    ...lines.slice(build, build + 4),
  ].join("\n");
})();

const verifyRows = (verify?.expect ?? [])
  .map((r) => `<tr><td>${esc(r.part)}</td><td>${sw("#" + String(r.color_3mf ?? "").slice(1, 7))} ${esc(r.color)}</td><td>F${r.filament}</td><td>${esc(r.extruder_bambu)}</td><td><code>${esc(r.color_3mf)}</code></td><td class="n">${num(r.volume_brep, 3)}</td><td class="n">${num(r.volume_mesh, 3)}</td><td class="${r.ok ? "ok" : "bad"}">${r.ok ? "ok" : "error"}</td></tr>`)
  .join("\n");

const scenarioHtml = scenarios
  .map((s) => {
    const imgs = (s.images ?? []).map((i) => ({ src: pwImg(i.name), label: i.label })).filter((i) => i.src);
    return `<div class="scen"><h3>${esc(s.id)} ${esc(s.title)}</h3><p>${md(s.caption)}</p>
${(s.checks ?? []).length ? `<ul class="checks">${s.checks.map((c) => `<li>${md(c)}</li>`).join("")}</ul>` : ""}
${imgs.map((i) => `<figure><img src="${esc(i.src)}" alt=""><figcaption>${esc(i.label)}</figcaption></figure>`).join("\n")}</div>`;
  })
  .join("\n");

const tauriRows = (tauri?.results ?? [])
  .map((r) => `<tr><td>${esc(r.id)}</td><td>${md(r.title)}</td><td class="n">${num(r.ms / 1000, 1)} s</td><td class="${r.ok ? "ok" : "bad"}">${r.ok ? "ok" : "error"}</td></tr>`)
  .join("\n");
const tauriNew = (tauri?.results ?? []).filter((r) => ["T12", "T13", "T14"].includes(r.id));
const tauriHtml = tauriNew
  .map((r) => {
    const imgs = (r.images ?? []).map((i) => ({ src: tauriImg(i.name), label: i.label })).filter((i) => i.src);
    return `<div class="scen"><h3>${esc(r.id)} ${md(r.title)}</h3><ul class="checks">${r.checks.map((c) => `<li class="${c.ok ? "" : "bad"}">${md(c.text)}</li>`).join("")}</ul>
${r.error ? `<pre>${esc(r.error.slice(0, 800))}</pre>` : ""}
${imgs.map((i) => `<figure><img src="${esc(i.src)}" alt=""><figcaption>${esc(i.label)}</figcaption></figure>`).join("\n")}</div>`;
  })
  .join("\n");

const contrastTable = `<table><tr><th>Pair</th><th>Token</th><th>light</th><th>dark</th><th>Target</th></tr>
${contrastRows
  .map((r) => {
    const c = (x) => (x ? `${sw(x.bg.startsWith("rgba") ? x.bg : x.bg)}${sw(x.fg)} <b class="${x.r >= r.min ? "ok" : "bad"}">${num(x.r, 2)}</b>` : "–");
    return `<tr><td>${esc(r.label)}</td><td class="small"><code>${esc(r.t)}</code> on <code>${esc(r.b)}</code></td><td>${c(r.light)}</td><td>${c(r.dark)}</td><td>${num(r.min, 1)}</td></tr>`;
  })
  .join("\n")}</table>`;

const testDesc = {
  palette_ts_is_current: "generated table `web/src/logic/palette.ts` up to date",
  palette_names_case_insensitive: "names case-insensitive, `grey` = gray, palette values",
  unknown_name_warns_and_gets_a_stable_random_colour: "warning on the argument, stable random color (hash), L/C as palette, different hues, no German aliases",
  hex_vectors_and_alpha_stay_exact: "`#rgb`, `#rrggbbaa`, vectors, `alpha` exact; broken hex warns",
  filament_parameter: "`filament = n`: range 1–16, integer, contradiction warns, not positional",
  children_inherit_the_colour: "children inherit the color, inner `color()` wins",
  palette_is_distinguishable: "ΔE_OK ≥ 0.10 normal, ≥ 0.065 deuteranopia, ≥ 0.05 protanopia, ≥ 0.025 tritanopia; chroma ≤ 0.18, no pure colors",
  display_variants: "display variants: same hue, lightness only where needed, contrast to the view background",
  slot_is_added_to_the_first_statement: "without slot: `, filament = n` on the first statement of the color",
  existing_slots_are_replaced_or_removed: "existing slots replaced (all spellings of the color), \"automatic\" removes them",
  unchanged_value_gives_no_edit: "same value: no edit",
  later_statement_wins_overlaps: "overlap: later statement wins (500 / 1000 mm³)",
  nested_colour_wins_over_its_parent: "inner `color()` wins over the outer one",
  subtracted_material_counts_for_all_colours: "subtracted material applies to all colors; color only in the subtraction: no slot",
  uncoloured_material_is_the_default_colour_on_filament_1: "uncolored material = default, slot 1",
  filament_slots: "explicit slots, lowest free slot, same color in three spellings = one entry",
  fillet_between_colours_goes_to_the_first_coloured_subtree: "fillet between two colors → first color; volume sum",
  resize_scales_the_colour_material: "`resize` when placing the color bodies",
  transforms_and_modules: "modules, loops, rotations",
  regions_are_cached_and_colour_changes_recompute_no_nodes: "regions cached (0 kernel calls), color/slot change recomputes no node",
  multicolour_3mf_structure: "3MF: files, color group, objects, components without transformation, `model_settings.config` (part ID = mesh ID, extruder), STL per color",
  region_meshes_share_coordinates: "meshes of the regions: volume, shared coordinates",
  scene_lists_colours_with_slots: "protocol: `Scene.colors` (also preliminary scene), warning at line 3",
  colour_regions_with_meshes: "`RequestColorRegions` with meshes, old version is discarded",
  set_filament_writes_the_code: "`SetFilament` → edits, undo, error cases",
  multicolour_exports: "export 3MF (multi-color by default), `colors: false`, STL per color",
  outline_lists_colours_slots_and_volumes: "`cad outline`: colors, slots, volumes, sum per part, without kernel no volumes",
  measure_colours: "`cad measure --colors`: volume, mass 1.24 g/cm³, parts",
  export_multicolour_single_and_per_colour: "`cad export` multi-color, `--single-color`, `--per-color`, STL single-color",
  uncoloured_part_needs_no_boolean: "without color nodes: one default region without boolean, volume = part volume, mesh with all bodies",
  render_theme_and_regions: "`cad render --theme dark --show regions`: dark background (pixels), 6 region bodies",
};
const colorTestTable = `<table><tr><th>Test</th><th>Checks</th><th>Result</th></tr>
${colorTests.map((t) => `<tr><td><code>${esc(t.name)}</code><br><span class="small">${esc(t.file)}</span></td><td>${md(testDesc[t.name] ?? "")}</td><td class="${t.res === "ok" ? "ok" : "bad"}">${esc(t.res)}</td></tr>`).join("\n")}</table>`;

const cliImg = (n) => (existsSync(join(imgDir, n)) ? `img/${n}` : null);
const fig = (src, cap) => (src ? `<figure><img src="${esc(src)}" alt=""><figcaption>${cap}</figcaption></figure>` : "");

const html = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Test report 11 – colors, multi-filament 3MF, dark theme</title>
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
  tr, td, figure { break-inside: avoid; }
  .ok { color: var(--ok); font-weight: 600; } .bad { color: var(--bad); font-weight: 600; }
  .sw { display: inline-block; width: 3.6mm; height: 3.6mm; border-radius: 0.8mm; border: 0.25mm solid rgba(0,0,0,0.3); vertical-align: -0.6mm; margin-right: 0.6mm; }
  table.pal td { white-space: nowrap; }
  table.pal td.bgl { background: ${esc(palette?.view_bg.light ?? "#e7eaee")}; }
  table.pal td.bgd { background: ${esc(palette?.view_bg.dark ?? "#16191d")}; color: #e6e9ed; }
  table.pal td.bgd .small { color: #a2aab5; }
  .strip { display: flex; height: 7mm; margin: 0.6mm 0 1.4mm; border-radius: 1mm; overflow: hidden; border: 1px solid var(--border); }
  .strip span { flex: 1; }
  figure { margin: 2mm 0; }
  figure img { max-width: 100%; max-height: 118mm; border: 1px solid var(--border); border-radius: 1mm; }
  figcaption { font-size: 7.8pt; color: var(--muted); }
  .two { display: grid; grid-template-columns: 1fr 1fr; gap: 3mm; }
  .scen { break-inside: auto; }
  ul.checks li { list-style: "✓ "; }
  ol { margin: 1mm 0 2mm 6mm; padding: 0; }
  ol li { margin: 1mm 0; }
  .note { border-left: 1mm solid #e8b84a; padding: 1mm 2.5mm; background: #fff8e6; margin: 2mm 0; }
</style>
</head>
<body>
<main>
<h1>Test report 11 – colors, multi-filament 3MF, dark theme</h1>
<div class="meta">cad-eval (palette), cad-engine (colors, color_export), cad-session, cad-cli, cad-render, web (color list, theme) · ${esc(date)} · Branch ${esc(branch)} · Commit ${esc(commit)} · ${esc(rustc)} · ${os.cpus().length} cores</div>

<div class="summary">
  <div>cargo test --workspace<b class="${cargoSum && cargoSum.failed === 0 ? "ok" : "bad"}">${cargoSum ? `${cargoSum.ok} / ${cargoSum.ok + cargoSum.failed}` : "–"}</b>of which ${colorTests.length} new color tests</div>
  <div>clippy · fmt<b class="${clippyOk && fmtOk ? "ok" : "bad"}">${clippy == null ? "–" : clippyOk ? "clean" : "finding"} · ${fmt == null ? "–" : fmtOk ? "clean" : "diff"}</b>-D warnings</div>
  <div>Vitest · Playwright<b class="${pwStats && pwStats.failed.length === 0 ? "ok" : "bad"}">${vitestSum ?? "–"} · ${pwStats ? `${pwStats.passed} / ${pwStats.total}` : "–"}</b>of which ${pwStats?.farben.length ?? "–"} new scenarios (F01–F07)</div>
  <div>Tauri-E2E<b class="${tauri && tauriOk === tauri.results.length && !tauri.fatal ? "ok" : "bad"}">${tauri ? `${tauriOk} / ${tauri.results.length}` : "–"}</b>tauri-driver + WebKitWebDriver, Xvfb</div>
  <div>3MF checked independently<b class="${verify?.ok ? "ok" : "bad"}">${verify ? (verify.ok ? "passed" : "error") : "–"}</b>lib3mf, trimesh, Bambu model, PrusaSlicer</div>
  <div>OpenSCAD · STEP<b class="${compareSum && compareSum[0] === compareSum[1] ? "ok" : "bad"}">${compareSum ? `${compareSum[0]}/${compareSum[1]}` : "–"} · ${esc(stepSum?.match(/\d+\/\d+/)?.[0] ?? "–")}</b>compare.py · verify.py</div>
  <div>Palette: smallest ΔE<sub>OK</sub><b>${palette ? `${num(palette.closest.normal[0].de, 3)} · ${num(palette.closest.deuteranopia[0].de, 3)}` : "–"}</b>normal · deuteranopia</div>
</div>

<h2>1 Task and result</h2>
<p>Three user requests: <b>multiple colors</b> with an export that Bambu Studio prints "fairly easily with several filaments"; replace the CSS colors with a <b>small, good-looking preset</b> (English names only, muted tones, warning and random color for unknown names); a thoroughly reworked <b>dark theme</b>. Implemented:</p>
<ul>
<li><b>Palette</b> with 16 colors in <code>cad-eval</code> (one source, generated for the UI), unknown names → warning "color <code>x</code> unknown – random color" and a stable color from the hash of the name; hex, vectors and <code>alpha</code> exact (section 2).</li>
<li><b>Color regions</b>: each part is split via kernel booleans into bodies per color (cached), sum of the volumes = part volume; <b>filament slots</b> from <code>color("teal", filament = 2)</code> or automatic (section 3).</li>
<li><b>3MF for Bambu Studio / OrcaSlicer</b>: one object per part, one sub-part per color at the same origin, color group and <code>Metadata/model_settings.config</code> with <code>extruder</code> per sub-part – the format worked out from the slicers' source code (section 5), checked with lib3mf, trimesh, a model of the Bambu importer and PrusaSlicer (section 6). In addition "one STL per color" as a ZIP. CLI: <code>cad export -o x.3mf</code> is multi-color (<code>--single-color</code>), <code>cad outline</code> and <code>cad measure --colors</code> show colors, slots, volume and mass.</li>
<li><b>UI</b>: "Colors &amp; filaments" (swatch, name, lines, slot selector that writes <code>filament = n</code> into the code, volume and mass for PLA 1.24 g/cm³), color per face in the view, "Show color regions" (section 7).</li>
<li><b>Dark theme</b>: choice System (default, follows <code>prefers-color-scheme</code>, also in Tauri)/Light/Dark, token set with WCAG check (text ≥ 4.5:1), editor components with their own default theme adapted, palette colors slightly lightened in the dark theme; <code>cad render --theme dark</code> (section 8).</li>
</ul>

<h2>2 Palette</h2>
<p>The names chosen are those that occur most often in OpenSCAD files – in the corpus of OpenSCAD examples and libraries in the container red (11), green (7), cyan (7), yellow, gray, blue, black (2 each), magenta, lime; from experience with widely used projects additionally white, orange, silver, purple – plus teal, pink, brown as common filament colors. The proposal <code>indigo</code>/<code>slate</code> was dropped: indigo lies between blue and purple (hard to separate with deuteranopia), slate between gray and blue; <code>silver</code> and <code>magenta</code> are more frequent in OpenSCAD files. <code>grey</code>, <code>aqua</code>, <code>fuchsia</code> are the CSS spellings of gray, cyan, magenta; there are no German names.</p>
<p>Values: target values in OKLCH (lightness L, chroma C ≤ 0.18 – muted, no pure RGB corners –, hue h), optimized by a search for the smallest distance in OKLab for normal vision and for simulated deuteranopia and protanopia (Machado, Oliveira, Fernandes 2009, severity 1.0, in linear sRGB). The formulas are in <code>palette.rs</code>; <code>cargo test -p cad-eval --test palette</code> enforces the minimum distances, <code>tools/palette/palette.py</code> recomputes the same values.</p>
${paletteTable}
<p class="small">Strips: palette normal, simulated deuteranopia, protanopia, tritanopia.</p>
${strip("hex")}${strip("deuteranopia")}${strip("protanopia")}${strip("tritanopia")}
${closest}
<ul>
<li><b>Display per theme</b> (same names, same hue): in the dark theme black is raised to L 0.36 (#3b3d41) and dark tones (teal, blue, purple, brown) +0.04 lighter; in the light theme white is lowered to L 0.93 (#e9e8e5). Exports, STEP and 3MF use the base values. The contrast given in the table is that of the flat color against the background of the 3D view; bodies are shaded and have edge lines, so white (light) and black (dark) are also visible, but only barely.</li>
<li><b>Color vision deficiency:</b> 16 colors cannot be separated well for all forms. With deuteranopia red/green (0.070), green/gray and blue/purple are closest, with protanopia orange/green (0.052), with the rare tritanopia teal/blue (0.031) – the list therefore always shows name and slot next to the color.</li>
<li><b>Unknown names:</b> hue from FNV-1a 64 of the lower-cased name, L = 0.65 and C = 0.125 (medians of the twelve chromatic palette colors, C reduced until the value lies in sRGB). Same name → same color, also across sessions, CLI and UI.</li>
</ul>
<table><tr><th>Input</th><th>Color</th><th>Source</th></tr>${randomRows}</table>
<pre>${esc((zufallCheck ?? "").split("\n").slice(0, 3).join("\n"))}</pre>

<h2>3 Rules: color regions and filaments</h2>
<h3>Color regions (volume per color)</h3>
<ul>
<li>The body of a colored subtree belongs to its color: the result of the <code>color()</code> node, placed in the coordinate system of the part (local matrices and <code>resize</code> factors along the path).</li>
<li>If colors overlap, the <b>later statement</b> wins, the earlier one is reduced by it; "later" means pre-order in the tree: an inner <code>color()</code> after the outer one, a later sibling after the earlier one.</li>
<li><b>Subtracted material applies to all</b>: each piece is intersected with the result (<code>difference</code> removes the hole from all colors); colors that occur only in subtracted operands get no slot.</li>
<li>Material in no colored subtree (e.g. the fillet between two colors) belongs to the color on the part, else to the default entry (if the part has uncolored primitives), else to the first color of the part.</li>
<li>Computation: <code>piece(C) = R ∩ M(C) − ⋃ M(later)</code> (piece of C = result ∩ material of C − union of later materials), remainder <code>R − ⋃ M</code>, pieces of one color united, empty ones dropped; about three booleans per color node. Result per part is a list (color, slot, body) with volume; if the sum differs from the part volume, the engine warns ("color regions inaccurate"). Cached by part, color nodes, placements and colors; color and slot are <b>not</b> part of the node keys – a new color or a new slot recomputes no node (test <code>regions_are_cached…</code>). Without color nodes below the part there is no boolean: one region from the bodies of the part, volume = cached part volume (test <code>uncoloured_part_needs_no_boolean</code>).</li>
</ul>
<h3>Filament slots</h3>
<ul>
<li><code>color("teal", filament = 2)</code> sets the slot of the color (1–16, named argument; the first specification applies, a contradiction warns).</li>
<li>Material without <code>color()</code> ("default", #c3c8cf) prints with slot 1.</li>
<li>All other colors get the lowest free slot in the order of their first occurrence. The same color in another spelling (<code>"gray"</code>, <code>"GREY"</code>, <code>"#7b7d81"</code>) is the same entry.</li>
<li><b>Changing the slot in the UI = text edit</b> (<code>SetFilament</code>): every <code>color()</code> statement of the color with <code>filament = …</code> gets the new value, otherwise the first one gets <code>, filament = n</code>; "automatic" removes the argument. Reason: the code stays the only source (CLI export and UI export agree, the agent sees the same), undo and the Git gutter capture the change; a preview override as in the parameter bar would be lost on saving and would be a second source of truth for the export.</li>
</ul>

<h2>4 Example model <code>tests/models/spec/multicolor.mcad</code></h2>
<p>Door sign (white plate, frame teal, lettering "CAD" as relief in red with fixed slot 3, subtracted hanging hole) and game piece (blue body of base and revolved profile, white collar, yellow head) as two parts.</p>
<div class="two">${fig(cliImg("cli-hell.png"), "<code>cad render</code> – color per face")}${fig(cliImg("cli-dunkel.png"), "<code>cad render --theme dark</code>")}</div>
<div class="two">${fig(cliImg("cli-regionen.png"), "<code>--show regions --theme dark</code>: bodies per color")}${fig(cliImg("cli-schnitt.png"), "<code>--section x=80 --part figure --show regions --view right</code>: section through the region bodies; the caps are hatched red as always, the contours show the boundaries between head (wins in the neck), collar and body")}</div>
${fig(cliImg("cli-ansichten.png"), "Contact sheet iso, front, top, right")}
<h3>Colors and volumes (<code>cad measure --colors</code>)</h3>
${colorsTable(measure?.colors?.colors)}
<table><tr><th>Part</th><th>Regions (slot, color, mm³)</th><th>Sum</th><th>Part volume</th><th>Warning</th></tr>${partsTable}</table>
<details open><summary class="small">Source</summary><pre>${esc(example ?? "")}</pre></details>

<h2>5 3MF for Bambu Studio and OrcaSlicer</h2>
<h3>Structure</h3>
<pre>${esc(list3mf ?? "")}</pre>
<ul>
<li><code>3D/3dmodel.model</code> (core 2015/02 + materials extension): <code>m:colorgroup</code> with one <code>m:color</code> per color (#RRGGBBAA); per (part, color) one mesh object with <code>pid</code>/<code>pindex</code> (color at object level); per part one object with <code>components</code> without <code>transform</code> (all meshes in the same coordinate system – nothing is moved); one build item per part. <code>Application</code> = "controller-cad 0.1.0".</li>
<li><code>Metadata/model_settings.config</code>: per part <code>&lt;object id&gt;</code> (= build item) with <code>name</code> and <code>extruder</code> (slot of the largest color), per color <code>&lt;part id&gt;</code> (= ID of the mesh object) with <code>name</code> and <code>extruder</code> = slot.</li>
<li>No <code>Metadata/project_settings.config</code> (see assumptions). Content type for <code>.config</code> registered.</li>
</ul>
<pre>${esc(modelExcerpt)}</pre>
<pre>${esc(settings ?? "")}</pre>
<h3>Assumptions and their sources</h3>
<p>Read in the source code of Bambu Studio (<code>bambulab/BambuStudio</code>, branch master) and OrcaSlicer (<code>SoftFever/OrcaSlicer</code>, branch main), retrieved on 2026-10-23 via raw.githubusercontent.com (checksums below):</p>
<ol>
<li><b>Foreign 3MF:</b> Bambu Studio treats a file as its own only if <code>Application</code> starts with "BambuStudio-" (<code>_handle_end_metadata</code>, OrcaSlicer additionally "OrcaSlicer-"). We do not pose as Bambu Studio; the file is loaded as "From_Other" (<code>Model::read_from_archive</code>; PrusaSlicer files are recognized by <code>PrusaFileParser</code> from the name "PrusaSlicer").</li>
<li><b><code>model_settings.config</code> is also read for foreign files</b> (<code>_load_model_from_file</code>, branch <code>BBS_MODEL_CONFIG_FILE</code>; errors are fatal only for its own files). <code>&lt;object id&gt;</code> is matched via the ID of the build item object, <code>&lt;part id&gt;</code> via the ID of the sub-object (<code>_handle_start_config_volume</code>: without a triangle range, <code>subobject_id</code> counts; <code>_generate_volumes_new</code> looks for <code>volume_iter.subobject_id == sub_object-&gt;id</code>). All other keys of a <code>part</code> go into the volume configuration (<code>volume-&gt;config.set_deserialize</code>), including <code>extruder</code>.</li>
<li><b>Components without <code>p:path</code></b> refer to objects of the same file (<code>_handle_start_component</code>: empty path = current model); <code>_generate_current_object_list</code> resolves them into sub-objects, each component becomes a sub-part (volume) of the same object with the component's transformation (none here).</li>
<li><b>One volume:</b> after loading, the importer removes the <code>extruder</code> of a single volume; then the object's applies – that is why we also write it on the object.</li>
<li><b>Colors:</b> the parser compares element names literally with <code>m:colorgroup</code>/<code>m:color</code> (prefix <code>m</code> required). With <code>pid</code>/<code>pindex</code> on the mesh object every triangle gets the color (<code>_handle_start_triangle</code>), and current Bambu Studio offers the color dialog on opening (<code>Plater.cpp</code>: <code>extract_colors_to_obj_dialog</code>, <code>ObjColorDialog</code>); "Append" adds the colors as filaments, and with colors per volume <code>ObjColorPanel::apply_volume_filament_ids</code> sets the extruders per volume without painting over the volume boundaries. Older versions and OrcaSlicer use the color group only at object level and without metadata.</li>
<li><b>No <code>project_settings.config</code>:</b> Bambu Studio loads it only from its own files (<code>dont_load_config = true</code> without generator version); OrcaSlicer always loads it and applies it over its defaults (<code>config += config_loaded</code>) – a file with only <code>filament_colour</code> would reset printer and process settings there. The filament colors therefore come from the color dialog (Bambu) or from the user/AMS sync.</li>
<li><b>Missing filaments:</b> on import (geometry only) Bambu Studio extends the filament list up to the highest <code>extruder</code> in the file (<code>Plater.cpp</code>: "add extruder … if only import geometry", with colors from <code>get_next_color_for_filament</code>).</li>
</ol>
<pre>${esc((sources ?? "").replace(/\.\/(\S+)/g, (m, f) => f.replace(/_/g, "/").replace(/^([^/]+)\/([^/]+)\//, "$1/$2 ")))}</pre>

<h2>6 Verification of the 3MF</h2>
<p><code>tools/report/verify3mf.py</code> on the file from <code>cad export tests/models/spec/multicolor.mcad --part door_sign --part figure -o mehrfarbig.3mf</code>, expected values from <code>cad measure --colors --json</code>:</p>
<pre>${esc(verifyText ?? "verify.txt missing")}</pre>
<table><tr><th>Part</th><th>Color</th><th>Slot</th><th>Extruder (Bambu model)</th><th>Color in 3MF</th><th>Volume B-rep</th><th>Volume mesh</th><th></th></tr>${verifyRows}</table>
<ul>
<li><b>lib3mf</b> ${esc(verify?.lib3mf?.version ?? "")} (library of the 3MF Consortium): reads without warning, ${Object.keys(verify?.lib3mf?.meshes ?? {}).length} meshes, all manifold and oriented (<code>IsManifoldAndOriented</code>), components and build items without transformation, color per mesh from the color group.</li>
<li><b>trimesh</b> ${esc(verify?.trimesh?.version ?? "")}: ${verify?.trimesh?.geometries ?? "–"} geometries, all watertight.</li>
<li><b>Bambu/Orca model</b>: the importer re-implemented (section 5): build item → components → sub-objects, sub-parts via <code>part id</code>, extruder per sub-part, color per sub-part; file not recognized as a Bambu project (intended), no <code>project_settings.config</code>. Volume B-rep ↔ mesh within 0.5 %.</li>
<li><b>PrusaSlicer</b> ${esc(verify?.prusaslicer?.version ?? "")} (apt, CLI <code>--info</code>): loads the file, ${verify?.prusaslicer?.objects ?? "–"} objects (one per mesh, at the right position; PrusaSlicer does not read <code>model_settings.config</code> and turns components of foreign files into separate objects), all manifold, volumes as above.</li>
<li><b>Bambu Studio / OrcaSlicer themselves</b> were not run: the container has no access to their release files (AppImage), building from source would take hours and many GB. The mapping is therefore derived from the source code and checked with the model; section 10 describes the check in the slicer.</li>
<li>STL per color (ZIP, <code>cad export --per-color</code>): <pre>${esc(zipList ?? "")}</pre></li>
</ul>

<h2>7 UI: colors &amp; filaments (Playwright, Chromium)</h2>
${scenarioHtml}

<h3>Desktop app (Tauri, native kernel)</h3>
${tauriHtml}

<h2>8 Dark theme</h2>
<ul>
<li><b>Choice</b> in the toolbar: System (default; follows <code>prefers-color-scheme</code> with a listener, also when switching at run time), Light, Dark; stored in <code>localStorage</code> (<code>cad-theme</code>). In Tauri, WebKitGTK reports the GTK color scheme (test T14 with <code>GTK_THEME=Adwaita:dark</code>).</li>
<li><b>Tokens</b> per theme in <code>styles.css</code>: surfaces, text, accent, error/warning (also line backgrounds), syntax, selection, hover, diff view, shadows, dialog backdrop, swatches; 3D view (background, floor and grid, model, edges, highlight, dimming, zones, subtracted bodies, section cap, handles including fill, anchors, snap with its own text color), Git gutter. The 3D view re-reads the tokens on a switch.</li>
<li><b>Editor components with their own default theme</b> (CodeMirror: autocompletion, search bar, matches, bracket pairs, fold markers) now follow the tokens; before, they stayed light in the dark theme.</li>
<li><b>Check:</b> <code>web/src/logic/contrast.test.ts</code> computes the WCAG contrast from the tokens for all pairs (translucent backgrounds blended over the surface): text ≥ 4.5:1, UI components ≥ 3:1; F05 and T13 measure the same pairs via <code>getComputedStyle</code> in Chromium and WebKitGTK, F05 additionally pixels of the 3D view (background, palette variant).</li>
</ul>
${contrastTable}

<h2>9 Tests</h2>
<ul>
<li><code>cargo test --workspace</code>: ${cargoSum ? `${cargoSum.ok} passed, ${cargoSum.failed} failed, ${cargoSum.ignored} ignored` : "–"}; <code>cargo clippy --workspace --all-targets -- -D warnings</code>: ${clippy == null ? "–" : clippyOk ? "no findings" : "finding"}; <code>cargo fmt --all --check</code>: ${fmt == null ? "–" : fmtOk ? "no findings" : "differences"}.</li>
<li>Web: <code>pnpm typecheck</code>, Vitest ${vitestSum ?? "–"} (new: <code>colors.test.ts</code>, <code>contrast.test.ts</code>), Playwright ${pwStats ? `${pwStats.passed}/${pwStats.total}` : "–"} (all existing scenarios and F01–F07${pwStats?.failed.length ? `; failed: ${pwStats.failed.map((f) => esc(f.title)).join(", ")}` : ""}).</li>
<li>Tauri E2E ${tauri ? `${tauriOk}/${tauri.results.length}` : "–"} (T01–T11 as in report 9, new T12–T14).</li>
<li>Found and fixed during the runs: (1) the color list is open by default and requested regions for every finished scene; for parts without color this cost a volume computation in the kernel, which with the watchdog set to 1.5 s in C19 triggered a second restart – now without a kernel call (cached part volume). (2) T08: with the color list open, the view bar wraps and the section bar lay over the second row ("element click intercepted") – both are now stacked in one container. (3) Intermittent failure in <code>supervisor_watchdog_replaces_engine_and_blocks_key</code> (1 in 5 runs, already before this task): the input thread of the desktop engine passed on the messages of a frame one by one, so <code>SetSource</code> and <code>__debugHang</code> could fall into two iterations of the engine loop and a different node was blocked; a frame is now one channel message (8/8 runs green). (4) P07 expected the old single-color 3MF of the part bar (now multi-color: build items counted instead of objects).</li>
<li>OpenSCAD comparison <code>compare.py --eval</code>: ${compareSum ? `${compareSum[0]}/${compareSum[1]}` : "–"} (none of the 33 models uses <code>color()</code>; geometry unchanged); STEP <code>verify.py</code> with OpenCASCADE: ${esc(stepSum ?? "–")}.</li>
</ul>
${colorTestTable}
<table><tr><th>Tauri E2E</th><th>Scenario</th><th>Duration</th><th>Result</th></tr>${tauriRows}</table>

<h2>10 Instructions: open in Bambu Studio</h2>
<p class="note">There was no Bambu Studio in the container; please verify these steps yourself. Described is what the source code leads one to expect.</p>
<ol>
<li>Export: in the UI "Colors &amp; filaments" → <b>3MF multi-color</b> (with parts, one object per part) or <code>cad export modell.scad -o modell.3mf</code>. The color list shows slot, volume and mass per color beforehand; slots are changed via the selector (writes <code>filament = n</code>).</li>
<li>In Bambu Studio choose <b>File → Import → 3MF/STL… (Ctrl+I)</b> (or drag the file into the window, or "Open project"). The file does not come from Bambu Studio; expect the notice that only geometry (and colors) are loaded – that is correct.</li>
<li><b>Color dialog</b> (current versions): Bambu Studio shows the colors of the file (e.g. white, teal, red, blue, yellow) and suggests filaments for them. <b>"Append"</b> creates the colors as new filaments; mapping to existing filaments puts the colors on the AMS slots; confirm. With colors per sub-part, each sub-part stays entirely in its filament.</li>
<li>Without the color dialog (older version, OrcaSlicer or cancelled): the slots from the file apply. In the <b>object list</b> (left) the objects <code>schild</code> and <code>figur</code> appear; expanded, one sub-part per color ("schild – white", "schild – teal", "schild – red" …), next to it the filament number (1, 2, 3 …). Bambu Studio adds missing filaments on import up to the highest slot (with its own colors); then choose the right colors/AMS slots in the filament bar ("Sync AMS").</li>
<li>Check: the sub-parts of an object sit exactly on each other (common origin), the lettering stands as relief on the plate, the collar sits at the neck of the figure. "Arrange objects" only moves whole objects.</li>
<li><b>Slice</b>: in the preview choose the view "Filament" – each color in its color, filament changes at the color boundaries, prime tower with several filaments. Compare the mass per filament with the color list (PLA 1.24 g/cm³; the slicer computes with walls/infill, hence different).</li>
</ol>
<p>OrcaSlicer: as above without the color dialog – sub-parts and slots from <code>model_settings.config</code>, set the filament colors yourself. PrusaSlicer does not read this metadata: there use "STL per color (ZIP)", load all STLs together and answer <b>Yes</b> to "Multi-part object?", then set the extruder per sub-part (file names <code>F&lt;slot&gt;_&lt;farbe&gt;.stl</code>).</p>

<h2>11 Known limitations</h2>
<ul>
<li>Color regions depend on SMLib's booleans: where SMLib loses material (e.g. union with a torus, known since report 1 – the example therefore uses a ring made from a cylinder), the regions are wrong; the engine warns ("sum ≠ volume") or reports an error. About three booleans per color node; the UI requests the regions only while the color list is open.</li>
<li>Bodies that meet only along a line do not give a closed body (found with the first letter "D": two bars touched only along an edge, lib3mf reported the mesh as non-manifold; now they overlap).</li>
<li><code>color()</code> in 2D subtrees (below <code>linear_extrude</code> etc.) colors faces but does not split volume. Material outside the parts is missing from the color list if there are parts.</li>
<li>Filament colors in Bambu Studio via the color dialog or by hand (no <code>project_settings.config</code>, see 5.6); not checked in Bambu Studio and OrcaSlicer in the container. PrusaSlicer: components become separate objects, no slots.</li>
<li>Color per face in the view via the face tag: two bodies with identical content and different colors share one tag and both show the first color (known limitation of the provenance); "Show color regions" is not affected.</li>
<li>Color vision deficiency: see section 2 (smallest ΔE with deuteranopia 0.070, tritanopia 0.031). White on a light and black on a dark background distinguishable only by shading and edges.</li>
<li>Theme "System" in Tauri depends on the GTK color scheme; under Xvfb without a scheme it is light. Measurements and images under Xvfb/SwiftShader without GPU.</li>
</ul>

<h2>12 Reproduction</h2>
<pre>export SMLIB_PREBUILT=$PWD/smlib/build-native CAD_NO_DAEMON=1
cargo build --release -p cad-cli                   # after: cd web &amp;&amp; pnpm build:tauri
cargo test --workspace --no-fail-fast &gt; reports/tmp/report11/cargo.txt 2&gt;&amp;1
cargo clippy --workspace --all-targets -- -D warnings &gt; reports/tmp/report11/clippy.txt 2&gt;&amp;1
cargo fmt --all --check &gt; reports/tmp/report11/fmt.txt
(cd web &amp;&amp; pnpm typecheck &amp;&amp; pnpm test &gt; ../reports/tmp/report11/vitest.txt &amp;&amp; pnpm e2e)
cp reports/tmp/report2/playwright.json reports/tmp/report11/
node web/e2e-tauri/desktop.e2e.mjs --bin target/release/mcad --out reports/tmp/report11/tauri
CAD=target/release/mcad python3 tests/openscad-compat/compare.py --eval &gt; reports/tmp/report11/compare.txt
/opt/tools/venv-step/bin/python tests/step/verify.py --cad target/release/mcad &gt; reports/tmp/report11/step.txt
CAD=target/release/mcad PY3MF=/path/venv/bin/python tools/report/report11-data.sh   # lib3mf, trimesh
node tools/report/report11.mjs
node tools/report/print-pdf.mjs docs/reports/11-farben-filamente/report.html docs/reports/11-farben-filamente/report.pdf "Test report 11: colors, multi-filament 3MF, dark theme"</pre>
</main>
</body>
</html>
`;
writeFileSync(join(out, "report.html"), html);
// drop images of earlier runs that the report no longer uses
const used = new Set([...copied, ...readdirSync(imgDir).filter((f) => f.startsWith("cli-"))]);
for (const f of readdirSync(imgDir)) if (!used.has(f)) console.log(`(not used: img/${f})`);
console.log(`${relative(root, join(out, "report.html"))}: ${scenarios.length} scenarios, Tauri ${tauri?.results?.length ?? "–"}, Rust ${cargoSum?.ok ?? "–"}, color tests ${colorTests.length}`);
