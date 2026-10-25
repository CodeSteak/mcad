#!/usr/bin/env node
// Test report 11 ("Farben, Multi-Filament-3MF, dunkles Thema"): collects the results of the
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
//   node tools/report/print-pdf.mjs docs/reports/11-farben-filamente/report.html docs/reports/11-farben-filamente/report.pdf "Testbericht 11: Farben, Multi-Filament-3MF, dunkles Thema"

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
const de = (x, d = 0) => (x === null || x === undefined || Number.isNaN(Number(x)) ? "–" : Number(x).toLocaleString("de-DE", { minimumFractionDigits: d, maximumFractionDigits: d }));
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
const example = readText(join(root, "examples/spec/mehrfarbig.scad"));
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
  ["Text gedämpft", "--muted", "--panel"],
  ["Text gedämpft auf Fläche 2", "--muted", "--panel-2"],
  ["Akzent (Links, Zeilen der Farbliste)", "--accent", "--panel"],
  ["Fehler", "--error", "--panel"],
  ["Fehler auf Fehlerzeile", "--error", "--error-bg", "--panel"],
  ["Warnung auf Warnzeile", "--warning", "--warning-bg", "--panel"],
  ["Syntax: Schlüsselwort", "--hl-keyword", "--panel"],
  ["Syntax: Zahl", "--hl-number", "--panel"],
  ["Syntax: Text", "--hl-string", "--panel"],
  ["Syntax: Kommentar", "--hl-comment", "--panel"],
  ["Syntax: Kommentar in aktiver Zeile", "--hl-comment", "--active-line", "--panel"],
  ["Syntax: Definition", "--hl-def", "--panel"],
  ["Syntax: Modifier", "--hl-modifier", "--panel"],
  ["Text auf Auswahl", "--text", "--accent-soft", "--panel"],
  ["Text auf Diff (neu)", "--text", "--diff-add", "--panel"],
  ["Text auf Diff (gelöscht)", "--text", "--diff-del", "--panel"],
  ["Schrift auf Akzent", "--on-accent", "--accent"],
  ["Snap-Beschriftung", "--on-snap", "--view-snap"],
  ["Git-Rand neu (UI, ≥ 3)", "--git-added", "--panel"],
  ["3D: Kanten auf Modell (≥ 3)", "--view-edge", "--view-model"],
  ["3D: Raster auf Hintergrund (≥ 3)", "--view-grid", "--view-bg"],
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
  ? `<table class="pal"><tr><th>Name</th><th>Grundwert (Export)</th><th>hell angezeigt<br><span class="small">auf ${esc(palette.view_bg.light)}</span></th><th>dunkel angezeigt<br><span class="small">auf ${esc(palette.view_bg.dark)}</span></th><th>Deut.</th><th>Prot.</th><th>Trit.</th><th>OKLCH (L · C · h)</th></tr>
${palette.colors
  .map(
    (c) => `<tr><td><b>${esc(c.name)}</b></td><td>${sw(c.hex)} <code>${esc(c.hex)}</code></td>
<td class="bgl">${sw(c.light)} <code>${esc(c.light)}</code> <span class="small">${de(c.contrast_light_bg, 2)}:1</span></td>
<td class="bgd">${sw(c.dark)} <code>${esc(c.dark)}</code> <span class="small">${de(c.contrast_dark_bg, 2)}:1</span></td>
<td>${sw(c.sim.deuteranopia)}</td><td>${sw(c.sim.protanopia)}</td><td>${sw(c.sim.tritanopia)}</td>
<td class="small">${c.oklch.map((x, i) => de(x, i === 2 ? 1 : 3)).join(" · ")}</td></tr>`,
  )
  .join("\n")}
<tr><td><b>Standard</b> <span class="small">(ohne color())</span></td><td>${sw(palette.default)} <code>${esc(palette.default)}</code></td><td colspan="6" class="small" style="white-space:normal">Material ohne <code>color()</code> in Exporten und in der Farbliste; in der Ansicht die Modellfarbe des Themas (<code>--view-model</code>)</td></tr>
</table>`
  : "<p>palette.json fehlt</p>";
const strip = (key) =>
  palette ? `<div class="strip">${palette.colors.map((c) => `<span title="${esc(c.name)}" style="background:${esc(key === "hex" ? c.hex : c.sim[key])}"></span>`).join("")}</div>` : "";
const closest = palette
  ? `<table><tr><th>Sicht</th><th>kleinste Abstände ΔE<sub>OK</sub> (OKLab, ~0,02 gerade sichtbar)</th></tr>
${Object.entries(palette.closest)
  .map(([k, v]) => `<tr><td>${esc({ normal: "normales Sehen", deuteranopia: "Deuteranopie (Grün)", protanopia: "Protanopie (Rot)", tritanopia: "Tritanopie (Blau)" }[k] ?? k)}</td><td>${v.map((p) => `${esc(p.a)}/${esc(p.b)} <b>${de(p.de, 3)}</b>`).join(" · ")}</td></tr>`)
  .join("\n")}</table>`
  : "";

const randomRows = (zufall?.colors ?? []).map((c) => `<tr><td><code>color("${esc(c.label)}")</code></td><td>${sw(c.key)} <code>${esc(c.key)}</code></td><td>${esc(c.source)}</td></tr>`).join("\n");

const colorsTable = (rows) =>
  `<table><tr><th>Slot</th><th>Farbe</th><th>Quelle</th><th>Zeilen</th><th>Volumen</th><th>Masse PLA</th></tr>
${(rows ?? [])
  .map((c) => `<tr><td><b>F${c.filament}</b>${c.explicit ? " <span class='small'>(filament = " + c.filament + ")</span>" : ""}</td><td>${sw(c.key)} ${esc(c.label)} <code>${esc(c.key)}</code></td><td>${esc(c.source)}</td><td>${esc((c.lines ?? []).join(", "))}</td><td class="n">${de(c.volume, 1)} mm³</td><td class="n">${de(c.mass_g, 2)} g</td></tr>`)
  .join("\n")}</table>`;
const partsTable = (measure?.colors?.parts ?? [])
  .map(
    (p) => `<tr><td><b>${esc(p.part ?? "Modell")}</b></td><td>${p.regions.map((r) => `F${r.filament} ${esc(r.label)} ${de(r.volume, 2)}`).join(" · ")}</td><td class="n">${de(p.regions.reduce((a, r) => a + r.volume, 0), 3)}</td><td class="n">${de(p.total, 3)}</td><td>${p.warning ? esc(p.warning) : "–"}</td></tr>`,
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
  .map((r) => `<tr><td>${esc(r.part)}</td><td>${sw("#" + String(r.color_3mf ?? "").slice(1, 7))} ${esc(r.color)}</td><td>F${r.filament}</td><td>${esc(r.extruder_bambu)}</td><td><code>${esc(r.color_3mf)}</code></td><td class="n">${de(r.volume_brep, 3)}</td><td class="n">${de(r.volume_mesh, 3)}</td><td class="${r.ok ? "ok" : "bad"}">${r.ok ? "ok" : "Fehler"}</td></tr>`)
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
  .map((r) => `<tr><td>${esc(r.id)}</td><td>${md(r.title)}</td><td class="n">${de(r.ms / 1000, 1)} s</td><td class="${r.ok ? "ok" : "bad"}">${r.ok ? "ok" : "Fehler"}</td></tr>`)
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

const contrastTable = `<table><tr><th>Paar</th><th>Token</th><th>hell</th><th>dunkel</th><th>Ziel</th></tr>
${contrastRows
  .map((r) => {
    const c = (x) => (x ? `${sw(x.bg.startsWith("rgba") ? x.bg : x.bg)}${sw(x.fg)} <b class="${x.r >= r.min ? "ok" : "bad"}">${de(x.r, 2)}</b>` : "–");
    return `<tr><td>${esc(r.label)}</td><td class="small"><code>${esc(r.t)}</code> auf <code>${esc(r.b)}</code></td><td>${c(r.light)}</td><td>${c(r.dark)}</td><td>${de(r.min, 1)}</td></tr>`;
  })
  .join("\n")}</table>`;

const testDesc = {
  palette_ts_is_current: "generierte Tabelle `web/src/logic/palette.ts` aktuell",
  palette_names_case_insensitive: "Namen Groß/Klein egal, `grey` = gray, Werte der Palette",
  unknown_name_warns_and_gets_a_stable_random_colour: "Warnung am Argument, stabile Zufallsfarbe (Hash), L/C wie Palette, verschiedene Farbtöne, keine deutschen Aliasse",
  hex_vectors_and_alpha_stay_exact: "`#rgb`, `#rrggbbaa`, Vektoren, `alpha` exakt; kaputtes Hex warnt",
  filament_parameter: "`filament = n`: Bereich 1–16, ganzzahlig, Widerspruch warnt, nicht positionell",
  children_inherit_the_colour: "Kinder erben die Farbe, inneres `color()` gewinnt",
  palette_is_distinguishable: "ΔE_OK ≥ 0,10 normal, ≥ 0,065 Deuteranopie, ≥ 0,05 Protanopie, ≥ 0,025 Tritanopie; Chroma ≤ 0,18, keine puren Farben",
  display_variants: "Anzeige-Varianten: gleicher Farbton, Helligkeit nur wo nötig, Kontrast zum Ansichtshintergrund",
  slot_is_added_to_the_first_statement: "ohne Slot: `, filament = n` an die erste Anweisung der Farbe",
  existing_slots_are_replaced_or_removed: "vorhandene Slots ersetzt (alle Schreibweisen der Farbe), „automatisch“ entfernt sie",
  unchanged_value_gives_no_edit: "gleicher Wert: kein Edit",
  later_statement_wins_overlaps: "Überlappung: spätere Anweisung gewinnt (500 / 1000 mm³)",
  nested_colour_wins_over_its_parent: "inneres `color()` gewinnt über das äußere",
  subtracted_material_counts_for_all_colours: "Abgezogenes gilt für alle Farben; Farbe nur im Abzug: kein Slot",
  uncoloured_material_is_the_default_colour_on_filament_1: "ungefärbtes Material = Standard, Slot 1",
  filament_slots: "explizite Slots, niedrigster freier Slot, gleiche Farbe in drei Schreibweisen = ein Eintrag",
  fillet_between_colours_goes_to_the_first_coloured_subtree: "Verrundung zwischen zwei Farben → erste Farbe; Volumensumme",
  resize_scales_the_colour_material: "`resize` beim Platzieren der Farbkörper",
  transforms_and_modules: "Module, Schleifen, Drehungen",
  regions_are_cached_and_colour_changes_recompute_no_nodes: "Regionen gecacht (0 Kernel-Aufrufe), Farb-/Slot-Änderung rechnet keinen Knoten neu",
  multicolour_3mf_structure: "3MF: Dateien, Farbgruppe, Objekte, Komponenten ohne Transformation, `model_settings.config` (Teil-ID = Netz-ID, extruder), STL je Farbe",
  region_meshes_share_coordinates: "Netze der Regionen: Volumen, gemeinsame Koordinaten",
  scene_lists_colours_with_slots: "Protokoll: `Scene.colors` (auch vorläufige Szene), Warnung an Zeile 3",
  colour_regions_with_meshes: "`RequestColorRegions` mit Netzen, alte Version wird verworfen",
  set_filament_writes_the_code: "`SetFilament` → Edits, rückgängig, Fehlerfälle",
  multicolour_exports: "Export 3MF (Standard mehrfarbig), `colors: false`, STL je Farbe",
  outline_lists_colours_slots_and_volumes: "`cad outline`: Farben, Slots, Volumen, Summe je Bauteil, ohne Kernel ohne Volumen",
  measure_colours: "`cad measure --colors`: Volumen, Masse 1,24 g/cm³, Bauteile",
  export_multicolour_single_and_per_colour: "`cad export` mehrfarbig, `--single-color`, `--per-color`, STL einfarbig",
  uncoloured_part_needs_no_boolean: "ohne Farbknoten: eine Standard-Region ohne Boolean, Volumen = Bauteilvolumen, Netz mit allen Körpern",
  render_theme_and_regions: "`cad render --theme dark --show regions`: dunkler Hintergrund (Pixel), 6 Regionskörper",
};
const colorTestTable = `<table><tr><th>Test</th><th>Prüft</th><th>Ergebnis</th></tr>
${colorTests.map((t) => `<tr><td><code>${esc(t.name)}</code><br><span class="small">${esc(t.file)}</span></td><td>${md(testDesc[t.name] ?? "")}</td><td class="${t.res === "ok" ? "ok" : "bad"}">${esc(t.res)}</td></tr>`).join("\n")}</table>`;

const cliImg = (n) => (existsSync(join(imgDir, n)) ? `img/${n}` : null);
const fig = (src, cap) => (src ? `<figure><img src="${esc(src)}" alt=""><figcaption>${cap}</figcaption></figure>` : "");

const html = `<!doctype html>
<html lang="de">
<head>
<meta charset="utf-8">
<title>Testbericht 11 – Farben, Multi-Filament-3MF, dunkles Thema</title>
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
<h1>Testbericht 11 – Farben, Multi-Filament-3MF, dunkles Thema</h1>
<div class="meta">cad-eval (palette), cad-engine (colors, color_export), cad-session, cad-cli, cad-render, web (Farbliste, Thema) · ${esc(date)} · Branch ${esc(branch)} · Commit ${esc(commit)} · ${esc(rustc)} · ${os.cpus().length} Kerne</div>

<div class="summary">
  <div>cargo test --workspace<b class="${cargoSum && cargoSum.failed === 0 ? "ok" : "bad"}">${cargoSum ? `${cargoSum.ok} / ${cargoSum.ok + cargoSum.failed}` : "–"}</b>davon ${colorTests.length} neue Farbtests</div>
  <div>clippy · fmt<b class="${clippyOk && fmtOk ? "ok" : "bad"}">${clippy == null ? "–" : clippyOk ? "sauber" : "Befund"} · ${fmt == null ? "–" : fmtOk ? "sauber" : "Abw."}</b>-D warnings</div>
  <div>Vitest · Playwright<b class="${pwStats && pwStats.failed.length === 0 ? "ok" : "bad"}">${vitestSum ?? "–"} · ${pwStats ? `${pwStats.passed} / ${pwStats.total}` : "–"}</b>davon ${pwStats?.farben.length ?? "–"} neue Szenarien (F01–F07)</div>
  <div>Tauri-E2E<b class="${tauri && tauriOk === tauri.results.length && !tauri.fatal ? "ok" : "bad"}">${tauri ? `${tauriOk} / ${tauri.results.length}` : "–"}</b>tauri-driver + WebKitWebDriver, Xvfb</div>
  <div>3MF unabhängig geprüft<b class="${verify?.ok ? "ok" : "bad"}">${verify ? (verify.ok ? "bestanden" : "Fehler") : "–"}</b>lib3mf, trimesh, Bambu-Modell, PrusaSlicer</div>
  <div>OpenSCAD · STEP<b class="${compareSum && compareSum[0] === compareSum[1] ? "ok" : "bad"}">${compareSum ? `${compareSum[0]}/${compareSum[1]}` : "–"} · ${esc(stepSum?.match(/\d+\/\d+/)?.[0] ?? "–")}</b>compare.py · verify.py</div>
  <div>Palette: kleinster ΔE<sub>OK</sub><b>${palette ? `${de(palette.closest.normal[0].de, 3)} · ${de(palette.closest.deuteranopia[0].de, 3)}` : "–"}</b>normal · Deuteranopie</div>
</div>

<h2>1 Auftrag und Ergebnis</h2>
<p>Drei Nutzerwünsche: <b>Mehrfarbigkeit</b> mit einem Export, den Bambu Studio „recht einfach mit mehreren Filamenten“ druckt; die CSS-Farben durch ein <b>kleines, schönes Preset</b> ersetzen (nur englische Namen, gedeckte Töne, Warnung und Zufallsfarbe bei unbekannten Namen); ein gründlich überarbeitetes <b>dunkles Thema</b>. Umgesetzt:</p>
<ul>
<li><b>Palette</b> mit 16 Farben in <code>cad-eval</code> (eine Quelle, für UI generiert), unbekannte Namen → Warnung „Farbe <code>x</code> unbekannt – Zufallsfarbe“ und stabile Farbe aus dem Hash des Namens; Hex, Vektoren und <code>alpha</code> exakt (Abschnitt 2).</li>
<li><b>Farbregionen</b>: jedes Bauteil wird über Kernel-Booleans in Körper je Farbe geteilt (gecacht), Summe der Volumen = Bauteilvolumen; <b>Filament-Slots</b> aus <code>color("teal", filament = 2)</code> oder automatisch (Abschnitt 3).</li>
<li><b>3MF für Bambu Studio / OrcaSlicer</b>: ein Objekt je Bauteil, ein Teil je Farbe am selben Ursprung, Farbgruppe und <code>Metadata/model_settings.config</code> mit <code>extruder</code> je Teil – das Format aus dem Quellcode der Slicer erarbeitet (Abschnitt 5), geprüft mit lib3mf, trimesh, einem Modell des Bambu-Importers und PrusaSlicer (Abschnitt 6). Dazu „ein STL je Farbe“ als ZIP. CLI: <code>cad export -o x.3mf</code> ist mehrfarbig (<code>--single-color</code>), <code>cad outline</code> und <code>cad measure --colors</code> zeigen Farben, Slots, Volumen und Masse.</li>
<li><b>UI</b>: „Farben &amp; Filamente“ (Farbfeld, Name, Zeilen, Slot-Auswahl, die <code>filament = n</code> in den Code schreibt, Volumen und Masse bei PLA 1,24 g/cm³), Farbe je Fläche in der Ansicht, „Farbregionen zeigen“ (Abschnitt 7).</li>
<li><b>Dunkles Thema</b>: Auswahl System (Standard, folgt <code>prefers-color-scheme</code>, auch in Tauri)/Hell/Dunkel, Token-Satz mit WCAG-Prüfung (Text ≥ 4,5:1), Editor-Teile mit eigenem Standardthema angepasst, Palette-Farben im Dunkeln leicht aufgehellt; <code>cad render --theme dark</code> (Abschnitt 8).</li>
</ul>

<h2>2 Palette</h2>
<p>Gewählt wurden die Namen, die in OpenSCAD-Dateien am häufigsten vorkommen – im Korpus der OpenSCAD-Beispiele und -Bibliotheken im Container red (11), green (7), cyan (7), yellow, gray, blue, black (je 2), magenta, lime; aus Erfahrung mit verbreiteten Projekten zusätzlich white, orange, silver, purple – und dazu teal, pink, brown als häufige Filamentfarben. Der Vorschlag <code>indigo</code>/<code>slate</code> entfiel: indigo liegt zwischen blue und purple (bei Deuteranopie kaum zu trennen), slate zwischen gray und blue; <code>silver</code> und <code>magenta</code> sind in OpenSCAD-Dateien häufiger. <code>grey</code>, <code>aqua</code>, <code>fuchsia</code> sind die CSS-Schreibweisen von gray, cyan, magenta; deutsche Namen gibt es nicht.</p>
<p>Werte: Zielwerte in OKLCH (Helligkeit L, Chroma C ≤ 0,18 – gedeckt, keine reinen RGB-Ecken –, Farbton h), mit einer Suche optimiert auf den kleinsten Abstand in OKLab bei normalem Sehen und für simulierte Deuteranopie und Protanopie (Machado, Oliveira, Fernandes 2009, Schwere 1,0, in linearem sRGB). Die Formeln stehen in <code>palette.rs</code>; <code>cargo test -p cad-eval --test palette</code> erzwingt die Mindestabstände, <code>tools/palette/palette.py</code> rechnet dieselben Werte nach.</p>
${paletteTable}
<p class="small">Streifen: Palette normal, simuliert Deuteranopie, Protanopie, Tritanopie.</p>
${strip("hex")}${strip("deuteranopia")}${strip("protanopia")}${strip("tritanopia")}
${closest}
<ul>
<li><b>Anzeige je Thema</b> (gleiche Namen, gleicher Farbton): im dunklen Thema wird Schwarz auf L 0,36 angehoben (#3b3d41) und dunkle Töne (teal, blue, purple, brown) um +0,04 heller; im hellen Thema wird Weiß auf L 0,93 abgesenkt (#e9e8e5). Exporte, STEP und 3MF nutzen die Grundwerte. Die Kontrastangabe in der Tafel ist die der flachen Farbe zum Hintergrund der 3D-Ansicht; Körper sind schattiert und haben Kantenlinien, daher sind auch Weiß (hell) und Schwarz (dunkel) zu sehen, aber nur knapp.</li>
<li><b>Farbfehlsichtigkeit:</b> 16 Farben lassen sich nicht für alle Formen gut trennen. Bei Deuteranopie liegen red/green (0,070), green/gray und blue/purple am nächsten, bei Protanopie orange/green (0,052), bei der seltenen Tritanopie teal/blue (0,031) – die Liste zeigt deshalb immer Namen und Slot zur Farbe.</li>
<li><b>Unbekannte Namen:</b> Farbton aus FNV-1a 64 des kleingeschriebenen Namens, L = 0,65 und C = 0,125 (Mediane der zwölf bunten Palettenfarben, C kleiner, bis der Wert in sRGB liegt). Gleicher Name → gleiche Farbe, auch über Sitzungen, CLI und UI hinweg.</li>
</ul>
<table><tr><th>Eingabe</th><th>Farbe</th><th>Quelle</th></tr>${randomRows}</table>
<pre>${esc((zufallCheck ?? "").split("\n").slice(0, 3).join("\n"))}</pre>

<h2>3 Regeln: Farbregionen und Filamente</h2>
<h3>Farbregionen (Volumen je Farbe)</h3>
<ul>
<li>Der Körper eines gefärbten Teilbaums gehört seiner Farbe: das Ergebnis des <code>color()</code>-Knotens, ins System des Bauteils gelegt (lokale Matrizen und <code>resize</code>-Faktoren auf dem Weg).</li>
<li>Überlappen sich Farben, gewinnt die <b>spätere Anweisung</b>, die frühere wird um sie verringert; „später“ heißt Pre-Order im Baum: ein inneres <code>color()</code> nach dem äußeren, ein späteres Geschwister nach dem früheren.</li>
<li><b>Abgezogenes gilt für alle</b>: jedes Stück wird mit dem Ergebnis geschnitten (<code>difference</code> nimmt allen Farben das Loch); Farben, die nur in abgezogenen Operanden vorkommen, bekommen keinen Slot.</li>
<li>Material in keinem gefärbten Teilbaum (z. B. die Verrundung zwischen zwei Farben) gehört zur Farbe am Bauteil, sonst zum Standard-Eintrag (wenn das Bauteil ungefärbte Primitive hat), sonst zur ersten Farbe des Bauteils.</li>
<li>Rechnung: <code>Stück(C) = R ∩ M(C) − ⋃ M(später)</code>, Rest <code>R − ⋃ M</code>, Stücke einer Farbe vereinigt, leere verworfen; je Farbknoten etwa drei Booleans. Ergebnis je Bauteil eine Liste (Farbe, Slot, Körper) mit Volumen; weicht die Summe vom Bauteilvolumen ab, warnt die Engine („Farbregionen ungenau“). Gecacht unter Bauteil, Farbknoten, Lagen und Farben; Farbe und Slot sind <b>nicht</b> Teil der Knotenschlüssel – eine neue Farbe oder ein neuer Slot rechnet keinen Knoten neu (Test <code>regions_are_cached…</code>). Ohne Farbknoten unter dem Bauteil gibt es keinen Boolean: eine Region aus den Körpern des Bauteils, Volumen = gecachtes Bauteilvolumen (Test <code>uncoloured_part_needs_no_boolean</code>).</li>
</ul>
<h3>Filament-Slots</h3>
<ul>
<li><code>color("teal", filament = 2)</code> legt den Slot der Farbe fest (1–16, benanntes Argument; die erste Angabe gilt, ein Widerspruch warnt).</li>
<li>Material ohne <code>color()</code> („Standard“, #c3c8cf) druckt mit Slot 1.</li>
<li>Alle anderen Farben bekommen in Reihenfolge ihres ersten Auftretens den niedrigsten freien Slot. Gleiche Farbe in anderer Schreibweise (<code>"gray"</code>, <code>"GREY"</code>, <code>"#7b7d81"</code>) ist derselbe Eintrag.</li>
<li><b>Slot in der UI ändern = Text-Edit</b> (<code>SetFilament</code>): jede <code>color()</code>-Anweisung der Farbe mit <code>filament = …</code> bekommt den neuen Wert, sonst die erste <code>, filament = n</code>; „automatisch“ entfernt das Argument. Begründung: der Code bleibt die einzige Quelle (CLI-Export und UI-Export stimmen überein, der Agent sieht dasselbe), Undo und Git-Rand erfassen die Änderung; ein Vorschau-Override wie in der Parameterleiste ginge beim Speichern verloren und wäre für den Export eine zweite Wahrheit.</li>
</ul>

<h2>4 Beispielmodell <code>examples/spec/mehrfarbig.scad</code></h2>
<p>Türschild (weiße Platte, Rahmen teal, Schrift „CAD“ als Relief in red mit festem Slot 3, abgezogene Aufhängebohrung) und Spielfigur (blauer Körper aus Sockel und gedrehtem Profil, weißer Kragen, gelber Kopf) als zwei Bauteile.</p>
<div class="two">${fig(cliImg("cli-hell.png"), "<code>cad render</code> – Farbe je Fläche")}${fig(cliImg("cli-dunkel.png"), "<code>cad render --theme dark</code>")}</div>
<div class="two">${fig(cliImg("cli-regionen.png"), "<code>--show regions --theme dark</code>: Körper je Farbe")}${fig(cliImg("cli-schnitt.png"), "<code>--section x=80 --part figur --show regions --view right</code>: Schnitt durch die Regionskörper; die Kappen sind wie immer rot schraffiert, die Konturen zeigen die Grenzen zwischen Kopf (gewinnt im Hals), Kragen und Körper")}</div>
${fig(cliImg("cli-ansichten.png"), "Kontaktbogen iso, front, top, right")}
<h3>Farben und Volumen (<code>cad measure --colors</code>)</h3>
${colorsTable(measure?.colors?.colors)}
<table><tr><th>Bauteil</th><th>Regionen (Slot, Farbe, mm³)</th><th>Summe</th><th>Bauteilvolumen</th><th>Warnung</th></tr>${partsTable}</table>
<details open><summary class="small">Quelltext</summary><pre>${esc(example ?? "")}</pre></details>

<h2>5 3MF für Bambu Studio und OrcaSlicer</h2>
<h3>Struktur</h3>
<pre>${esc(list3mf ?? "")}</pre>
<ul>
<li><code>3D/3dmodel.model</code> (Kern 2015/02 + Materials-Erweiterung): <code>m:colorgroup</code> mit einer <code>m:color</code> je Farbe (#RRGGBBAA); je (Bauteil, Farbe) ein Netz-Objekt mit <code>pid</code>/<code>pindex</code> (Farbe auf Objektebene); je Bauteil ein Objekt mit <code>components</code> ohne <code>transform</code> (alle Netze im selben Koordinatensystem – nichts wird verschoben); ein Build-Item je Bauteil. <code>Application</code> = „controller-cad 0.1.0“.</li>
<li><code>Metadata/model_settings.config</code>: je Bauteil <code>&lt;object id&gt;</code> (= Build-Item) mit <code>name</code> und <code>extruder</code> (Slot der größten Farbe), je Farbe <code>&lt;part id&gt;</code> (= ID des Netz-Objekts) mit <code>name</code> und <code>extruder</code> = Slot.</li>
<li>Kein <code>Metadata/project_settings.config</code> (siehe Annahmen). Content-Type für <code>.config</code> eingetragen.</li>
</ul>
<pre>${esc(modelExcerpt)}</pre>
<pre>${esc(settings ?? "")}</pre>
<h3>Annahmen und ihre Quellen</h3>
<p>Gelesen im Quellcode von Bambu Studio (<code>bambulab/BambuStudio</code>, Zweig master) und OrcaSlicer (<code>SoftFever/OrcaSlicer</code>, Zweig main), abgerufen am 23.10.2026 über raw.githubusercontent.com (Prüfsummen unten):</p>
<ol>
<li><b>Fremde 3MF:</b> Bambu Studio gilt eine Datei als eigene nur, wenn <code>Application</code> mit „BambuStudio-“ beginnt (<code>_handle_end_metadata</code>, OrcaSlicer zusätzlich „OrcaSlicer-“). Wir geben uns nicht als Bambu Studio aus; die Datei wird als „From_Other“ geladen (<code>Model::read_from_archive</code>; PrusaSlicer-Dateien erkennt <code>PrusaFileParser</code> am Namen „PrusaSlicer“).</li>
<li><b><code>model_settings.config</code> wird auch für fremde Dateien gelesen</b> (<code>_load_model_from_file</code>, Zweig <code>BBS_MODEL_CONFIG_FILE</code>; Fehler nur für eigene Dateien fatal). <code>&lt;object id&gt;</code> wird über die ID des Build-Item-Objekts zugeordnet, <code>&lt;part id&gt;</code> über die ID des Unterobjekts (<code>_handle_start_config_volume</code>: ohne Dreiecksbereich zählt <code>subobject_id</code>; <code>_generate_volumes_new</code> sucht <code>volume_iter.subobject_id == sub_object-&gt;id</code>). Alle übrigen Schlüssel einer <code>part</code> landen in der Volumen-Konfiguration (<code>volume-&gt;config.set_deserialize</code>), also auch <code>extruder</code>.</li>
<li><b>Komponenten ohne <code>p:path</code></b> verweisen auf Objekte derselben Datei (<code>_handle_start_component</code>: Pfad leer = aktuelles Modell); <code>_generate_current_object_list</code> löst sie in Unterobjekte auf, jede Komponente wird ein Teil (Volumen) desselben Objekts mit der Transformation der Komponente (hier keine).</li>
<li><b>Ein Volumen:</b> nach dem Laden entfernt der Importer das <code>extruder</code> eines einzigen Volumens; dann gilt das des Objekts – deshalb schreiben wir es auch am Objekt.</li>
<li><b>Farben:</b> der Parser vergleicht Elementnamen wörtlich mit <code>m:colorgroup</code>/<code>m:color</code> (Präfix <code>m</code> nötig). Mit <code>pid</code>/<code>pindex</code> am Netz-Objekt bekommt jedes Dreieck die Farbe (<code>_handle_start_triangle</code>), und das aktuelle Bambu Studio bietet beim Öffnen den Farbdialog an (<code>Plater.cpp</code>: <code>extract_colors_to_obj_dialog</code>, <code>ObjColorDialog</code>); „Append“ fügt die Farben als Filamente hinzu, und bei Farben je Volumen setzt <code>ObjColorPanel::apply_volume_filament_ids</code> die Extruder je Volumen, ohne die Volumengrenzen zu übermalen. Ältere Versionen und OrcaSlicer nutzen die Farbgruppe nur auf Objektebene und ohne Metadaten.</li>
<li><b>Kein <code>project_settings.config</code>:</b> Bambu Studio lädt es nur aus eigenen Dateien (<code>dont_load_config = true</code> ohne Generator-Version); OrcaSlicer lädt es immer und legt es über seine Standardwerte (<code>config += config_loaded</code>) – eine Datei nur mit <code>filament_colour</code> setzte dort Drucker- und Prozesseinstellungen zurück. Die Filamentfarben kommen deshalb aus dem Farbdialog (Bambu) oder vom Nutzer/AMS-Abgleich.</li>
<li><b>Fehlende Filamente:</b> beim Import (nur Geometrie) ergänzt Bambu Studio die Filamentliste bis zum höchsten <code>extruder</code> der Datei (<code>Plater.cpp</code>: „add extruder … if only import geometry“, mit Farben aus <code>get_next_color_for_filament</code>).</li>
</ol>
<pre>${esc((sources ?? "").replace(/\.\/(\S+)/g, (m, f) => f.replace(/_/g, "/").replace(/^([^/]+)\/([^/]+)\//, "$1/$2 ")))}</pre>

<h2>6 Prüfung der 3MF</h2>
<p><code>tools/report/verify3mf.py</code> auf der Datei von <code>cad export examples/spec/mehrfarbig.scad --part schild --part figur -o mehrfarbig.3mf</code>, erwartete Werte aus <code>cad measure --colors --json</code>:</p>
<pre>${esc(verifyText ?? "verify.txt fehlt")}</pre>
<table><tr><th>Bauteil</th><th>Farbe</th><th>Slot</th><th>Extruder (Bambu-Modell)</th><th>Farbe in 3MF</th><th>Volumen B-Rep</th><th>Volumen Netz</th><th></th></tr>${verifyRows}</table>
<ul>
<li><b>lib3mf</b> ${esc(verify?.lib3mf?.version ?? "")} (Bibliothek des 3MF-Konsortiums): liest ohne Warnung, ${Object.keys(verify?.lib3mf?.meshes ?? {}).length} Netze, alle mannigfaltig und orientiert (<code>IsManifoldAndOriented</code>), Komponenten und Build-Items ohne Transformation, Farbe je Netz aus der Farbgruppe.</li>
<li><b>trimesh</b> ${esc(verify?.trimesh?.version ?? "")}: ${verify?.trimesh?.geometries ?? "–"} Geometrien, alle wasserdicht.</li>
<li><b>Bambu/Orca-Modell</b>: der Importer nachgebaut (Abschnitt 5): Build-Item → Komponenten → Unterobjekte, Teile über <code>part id</code>, Extruder je Teil, Farbe je Teil; Datei nicht als Bambu-Projekt erkannt (gewollt), kein <code>project_settings.config</code>. Volumen B-Rep ↔ Netz innerhalb 0,5 %.</li>
<li><b>PrusaSlicer</b> ${esc(verify?.prusaslicer?.version ?? "")} (apt, CLI <code>--info</code>): lädt die Datei, ${verify?.prusaslicer?.objects ?? "–"} Objekte (je Netz eines, an der richtigen Stelle; PrusaSlicer liest <code>model_settings.config</code> nicht und macht aus Komponenten fremder Dateien eigene Objekte), alle mannigfaltig, Volumen wie oben.</li>
<li><b>Bambu Studio / OrcaSlicer selbst</b> liefen nicht: im Container gibt es keinen Zugriff auf deren Release-Dateien (AppImage), der Bau aus den Quellen bräuchte Stunden und viele GB. Die Zuordnung ist deshalb aus dem Quellcode abgeleitet und mit dem Modell geprüft; die Prüfung im Slicer beschreibt Abschnitt 10.</li>
<li>STL je Farbe (ZIP, <code>cad export --per-color</code>): <pre>${esc(zipList ?? "")}</pre></li>
</ul>

<h2>7 UI: Farben &amp; Filamente (Playwright, Chromium)</h2>
${scenarioHtml}

<h3>Desktop-App (Tauri, nativer Kernel)</h3>
${tauriHtml}

<h2>8 Dunkles Thema</h2>
<ul>
<li><b>Auswahl</b> in der Werkzeugleiste: System (Standard; folgt <code>prefers-color-scheme</code> mit Listener, auch beim Umschalten im Betrieb), Hell, Dunkel; gespeichert in <code>localStorage</code> (<code>cad-theme</code>). In Tauri meldet WebKitGTK das GTK-Farbschema (Test T14 mit <code>GTK_THEME=Adwaita:dark</code>).</li>
<li><b>Token</b> je Thema in <code>styles.css</code>: Flächen, Text, Akzent, Fehler/Warnung (auch Zeilenhintergründe), Syntax, Auswahl, Hover, Diff-Ansicht, Schatten, Rückseite von Dialogen, Farbfelder; 3D-Ansicht (Hintergrund, Boden und Raster, Modell, Kanten, Hervorhebung, Dimmen, Zonen, abgezogene Körper, Schnittkappe, Griffe samt Füllung, Anker, Snap mit eigener Schriftfarbe), Git-Rand. Die 3D-Ansicht liest die Token beim Wechsel neu.</li>
<li><b>Editor-Teile mit eigenem Standardthema</b> (CodeMirror: Autovervollständigung, Suchleiste, Treffer, Klammerpaare, Faltmarken) folgen jetzt den Token; vorher blieben sie im dunklen Thema hell.</li>
<li><b>Prüfung:</b> <code>web/src/logic/contrast.test.ts</code> rechnet für alle Paare den WCAG-Kontrast aus den Token (durchscheinende Hintergründe über der Fläche gemischt): Text ≥ 4,5:1, UI-Teile ≥ 3:1; F05 und T13 messen dieselben Paare über <code>getComputedStyle</code> in Chromium und WebKitGTK, F05 zusätzlich Pixel der 3D-Ansicht (Hintergrund, Palette-Variante).</li>
</ul>
${contrastTable}

<h2>9 Tests</h2>
<ul>
<li><code>cargo test --workspace</code>: ${cargoSum ? `${cargoSum.ok} bestanden, ${cargoSum.failed} fehlgeschlagen, ${cargoSum.ignored} ignoriert` : "–"}; <code>cargo clippy --workspace --all-targets -- -D warnings</code>: ${clippy == null ? "–" : clippyOk ? "ohne Befund" : "Befund"}; <code>cargo fmt --all --check</code>: ${fmt == null ? "–" : fmtOk ? "ohne Befund" : "Abweichungen"}.</li>
<li>Web: <code>pnpm typecheck</code>, Vitest ${vitestSum ?? "–"} (neu: <code>colors.test.ts</code>, <code>contrast.test.ts</code>), Playwright ${pwStats ? `${pwStats.passed}/${pwStats.total}` : "–"} (alle bisherigen Szenarien und F01–F07${pwStats?.failed.length ? `; fehlgeschlagen: ${pwStats.failed.map((f) => esc(f.title)).join(", ")}` : ""}).</li>
<li>Tauri-E2E ${tauri ? `${tauriOk}/${tauri.results.length}` : "–"} (T01–T11 wie in Bericht 9, neu T12–T14).</li>
<li>Bei den Läufen gefunden und behoben: (1) die Farbliste ist standardmäßig offen und fragte für jede fertige Szene Regionen an; bei Bauteilen ohne Farbe kostete das eine Volumenrechnung im Kernel, die mit dem auf 1,5 s gesetzten Watchdog in C19 einen zweiten Neustart auslöste – jetzt ohne Kernel-Aufruf (gecachtes Bauteilvolumen). (2) T08: bei offener Farbliste bricht die Ansichtsleiste um, die Schnittleiste lag über der zweiten Zeile („element click intercepted“) – beide stehen jetzt untereinander in einem Container. (3) Unregelmäßiger Fehler in <code>supervisor_watchdog_replaces_engine_and_blocks_key</code> (1 von 5 Läufen, schon vor diesem Auftrag): der Eingabe-Thread der Desktop-Engine reichte die Nachrichten eines Frames einzeln weiter, sodass <code>SetSource</code> und <code>__debugHang</code> in zwei Durchläufe der Engine-Schleife fallen konnten und ein anderer Knoten gesperrt wurde; ein Frame ist jetzt eine Kanal-Nachricht (8/8 Läufe grün). (4) P07 erwartete das alte einfarbige 3MF der Bauteilleiste (jetzt mehrfarbig: Build-Items statt Objekte gezählt).</li>
<li>OpenSCAD-Vergleich <code>compare.py --eval</code>: ${compareSum ? `${compareSum[0]}/${compareSum[1]}` : "–"} (keines der 33 Modelle nutzt <code>color()</code>; Geometrie unverändert); STEP <code>verify.py</code> mit OpenCASCADE: ${esc(stepSum ?? "–")}.</li>
</ul>
${colorTestTable}
<table><tr><th>Tauri-E2E</th><th>Szenario</th><th>Dauer</th><th>Ergebnis</th></tr>${tauriRows}</table>

<h2>10 Anleitung: In Bambu Studio öffnen</h2>
<p class="note">Im Container gab es kein Bambu Studio; diese Schritte bitte selbst prüfen. Beschrieben ist, was nach dem Quellcode zu erwarten ist.</p>
<ol>
<li>Exportieren: in der UI „Farben &amp; Filamente“ → <b>3MF mehrfarbig</b> (bei Bauteilen ein Objekt je Bauteil) oder <code>cad export modell.scad -o modell.3mf</code>. Die Farbliste zeigt vorher Slot, Volumen und Masse je Farbe; Slots ändern sich über die Auswahl (schreibt <code>filament = n</code>).</li>
<li>In Bambu Studio <b>Datei → Importieren → 3MF/STL… (Strg+I)</b> wählen (oder die Datei ins Fenster ziehen bzw. „Projekt öffnen“). Die Datei stammt nicht von Bambu Studio; erwartet ist der Hinweis, dass nur Geometrie (und Farben) geladen werden – das ist richtig so.</li>
<li><b>Farbdialog</b> (aktuelle Versionen): Bambu Studio zeigt die Farben der Datei (z. B. weiß, petrol, rot, blau, gelb) und schlägt Filamente dafür vor. Mit <b>„Append“ / „Anhängen“</b> werden die Farben als neue Filamente angelegt, mit der Zuordnung zu vorhandenen Filamenten die Farben auf die Slots im AMS gelegt; bestätigen. Bei Farben je Teil bleibt jedes Teil ganz in seinem Filament.</li>
<li>Ohne Farbdialog (ältere Version, OrcaSlicer oder abgebrochen): die Slots aus der Datei gelten. In der <b>Objektliste</b> (links) erscheinen die Objekte <code>schild</code> und <code>figur</code>; aufgeklappt je ein Teil pro Farbe („schild – white“, „schild – teal“, „schild – red“ …), daneben die Filamentnummer (1, 2, 3 …). Fehlende Filamente ergänzt Bambu Studio beim Import bis zum höchsten Slot (mit eigenen Farben); dann in der Filamentleiste die richtigen Farben/AMS-Slots wählen („AMS synchronisieren“).</li>
<li>Prüfen: die Teile eines Objekts sitzen genau aufeinander (gemeinsamer Ursprung), die Schrift steht als Relief auf der Platte, der Kragen sitzt am Hals der Figur. „Objekte anordnen“ verschiebt nur ganze Objekte.</li>
<li><b>Slicen</b>: in der Vorschau die Ansicht „Filament“ wählen – jede Farbe in ihrer Farbe, Filamentwechsel an den Farbgrenzen, Prime Tower bei mehreren Filamenten. Masse je Filament mit der Farbliste vergleichen (PLA 1,24 g/cm³; der Slicer rechnet mit Wänden/Füllung, daher anders).</li>
</ol>
<p>OrcaSlicer: wie oben ohne Farbdialog – Teile und Slots aus <code>model_settings.config</code>, Filamentfarben selbst setzen. PrusaSlicer liest diese Metadaten nicht: dort „STL je Farbe (ZIP)“ nehmen, alle STL zusammen laden und bei „Mehrteiliges Objekt?“ <b>Ja</b> wählen, dann je Teil den Extruder setzen (Dateinamen <code>F&lt;slot&gt;_&lt;farbe&gt;.stl</code>).</p>

<h2>11 Bekannte Grenzen</h2>
<ul>
<li>Farbregionen hängen an den Booleans von SMLib: wo SMLib Material verliert (z. B. Vereinigung mit einem Torus, bekannt seit Bericht 1 – das Beispiel nutzt deshalb einen Ring aus einem Zylinder), stimmen die Regionen nicht, die Engine warnt („Summe ≠ Volumen“) oder meldet einen Fehler. Je Farbknoten etwa drei Booleans; die UI fragt die Regionen nur bei offener Farbliste an.</li>
<li>Kanten, die nur auf einer Linie zusammenstoßen, ergeben keinen geschlossenen Körper (beim ersten Buchstaben „D“ gefunden: zwei Balken berührten sich nur an einer Kante, lib3mf meldete das Netz als nicht mannigfaltig; jetzt überlappen sie).</li>
<li><code>color()</code> in 2D-Teilbäumen (unter <code>linear_extrude</code> usw.) färbt Flächen, teilt aber kein Volumen. Material außerhalb der Bauteile fehlt in der Farbliste, wenn es Bauteile gibt.</li>
<li>Filamentfarben in Bambu Studio über den Farbdialog oder von Hand (kein <code>project_settings.config</code>, siehe 5.6); in Bambu Studio und OrcaSlicer nicht im Container geprüft. PrusaSlicer: Komponenten werden eigene Objekte, keine Slots.</li>
<li>Farbe je Fläche in der Ansicht über den Flächen-Tag: zwei inhaltsgleiche Körper mit verschiedenen Farben teilen sich einen Tag und zeigen beide die erste Farbe (bekannte Grenze der Provenienz); „Farbregionen zeigen“ ist davon nicht betroffen.</li>
<li>Farbfehlsichtigkeit: siehe Abschnitt 2 (kleinste ΔE bei Deuteranopie 0,070, Tritanopie 0,031). Weiß auf hellem und Schwarz auf dunklem Hintergrund nur über Schattierung und Kanten unterscheidbar.</li>
<li>Thema „System“ in Tauri hängt am GTK-Farbschema; unter Xvfb ohne Schema ist es hell. Messwerte und Bilder unter Xvfb/SwiftShader ohne GPU.</li>
</ul>

<h2>12 Reproduktion</h2>
<pre>export SMLIB_PREBUILT=$PWD/smlib/build-native CAD_NO_DAEMON=1
cargo build --release -p cad-cli                   # nach: cd web &amp;&amp; pnpm build:tauri
cargo test --workspace --no-fail-fast &gt; reports/tmp/report11/cargo.txt 2&gt;&amp;1
cargo clippy --workspace --all-targets -- -D warnings &gt; reports/tmp/report11/clippy.txt 2&gt;&amp;1
cargo fmt --all --check &gt; reports/tmp/report11/fmt.txt
(cd web &amp;&amp; pnpm typecheck &amp;&amp; pnpm test &gt; ../reports/tmp/report11/vitest.txt &amp;&amp; pnpm e2e)
cp reports/tmp/report2/playwright.json reports/tmp/report11/
node web/e2e-tauri/desktop.e2e.mjs --bin target/release/mcad --out reports/tmp/report11/tauri
CAD=target/release/mcad python3 tests/openscad-compat/compare.py --eval &gt; reports/tmp/report11/compare.txt
/opt/tools/venv-step/bin/python tests/step/verify.py --cad target/release/mcad &gt; reports/tmp/report11/step.txt
CAD=target/release/mcad PY3MF=/pfad/venv/bin/python tools/report/report11-data.sh   # lib3mf, trimesh
node tools/report/report11.mjs
node tools/report/print-pdf.mjs docs/reports/11-farben-filamente/report.html docs/reports/11-farben-filamente/report.pdf "Testbericht 11: Farben, Multi-Filament-3MF, dunkles Thema"</pre>
</main>
</body>
</html>
`;
writeFileSync(join(out, "report.html"), html);
// drop images of earlier runs that the report no longer uses
const used = new Set([...copied, ...readdirSync(imgDir).filter((f) => f.startsWith("cli-"))]);
for (const f of readdirSync(imgDir)) if (!used.has(f)) console.log(`(nicht verwendet: img/${f})`);
console.log(`${relative(root, join(out, "report.html"))}: ${scenarios.length} Szenarien, Tauri ${tauri?.results?.length ?? "–"}, Rust ${cargoSum?.ok ?? "–"}, Farbtests ${colorTests.length}`);
