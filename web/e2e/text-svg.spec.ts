// End-to-end tests for test report 16 (text and SVG import, Plan 17.4/17.5): text in the
// bundled fonts, an SVG of the project files as profile, editing the SVG in the editor moves the
// scene, a font uploaded into the project, the SVG example with its files, SVG warnings with
// their line.

import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { expect, test, type Page } from "@playwright/test";
import { cad, clickLine, hook, loadText, openApp, record16, screenshot16, waitScene } from "./helpers";

test.beforeEach(async ({ page }) => {
  page.on("pageerror", (e) => console.log("pageerror:", e.message));
  await page.addInitScript(() => localStorage.setItem("cad-colors-open", "0"));
});

type ProjectInfo = { name: string; main: string; active: string; paths: string[]; files: Record<string, string> };
type W = {
  __cad: {
    projectInfo(): ProjectInfo;
    openProject(p: { name: string; main: string; files: Record<string, string>; active?: string }): void;
    switchFile(path: string, line?: number): boolean;
    openExample(path: string): boolean;
    store: { getState(): Record<string, unknown> };
  };
};

const info = (page: Page) => page.evaluate(() => (window as unknown as W).__cad.projectInfo());
const bounds = (page: Page) => hook(page, (c) => c.rootBounds());
const width = async (page: Page) => {
  const b = await bounds(page);
  if (!b) throw new Error("no bounds");
  return b.max[0] - b.min[0];
};
const errors = async (page: Page) => (await cad(page)).diagnostics.filter((d) => d.severity === "error");
const warnings = async (page: Page) => (await cad(page)).diagnostics.filter((d) => d.severity === "warning").map((d) => d.message);

const FONTS = join(dirname(fileURLToPath(import.meta.url)), "../../crates/cad-font/fonts");

const LOGO = (w: number) => `<svg xmlns="http://www.w3.org/2000/svg" width="40mm" height="20mm" viewBox="0 0 40 20">
  <rect x="0" y="0" width="${w}" height="20" rx="4"/>
  <circle cx="${w + 8}" cy="10" r="6"/>
</svg>
`;

test("T01 Text in den mitgelieferten Schriften", async ({ page }) => {
  await openApp(page);
  const code = `color("red") linear_extrude(2) text("Größe 7", size = 10, font = "Inter:style=Bold");
color("blue") translate([0, -15, 0]) linear_extrude(2) text("ÄÖÜ äöü ẞ „x“", size = 8, font = "JetBrains Mono");
color("black") translate([0, -30, 0]) linear_extrude(2) text("STENCIL 42", size = 8, font = "Allerta Stencil");
`;
  await loadText(page, code);
  expect(await errors(page)).toEqual([]);
  expect(await warnings(page)).toEqual([]);
  const b = (await bounds(page))!;
  expect(b.max[0] - b.min[0]).toBeGreaterThan(50);
  expect(b.max[2]).toBeCloseTo(2, 5);
  await hook(page, (c) => c.setView("top"));
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
  await clickLine(page, 4);
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
  await screenshot16(page, "t01-text", "full");
  // a character Allerta Stencil lacks: warning, glyph from JetBrains Mono
  await loadText(page, `linear_extrude(1) text("20°C", font = "Allerta Stencil");\n`);
  const w = await warnings(page);
  expect(w.some((m) => m.includes("„°“ fehlen in Allerta Stencil") && m.includes("JetBrains Mono"))).toBe(true);
  record16({
    id: "T01",
    title: "Text in drei Schriften",
    images: [{ name: "t01-text", label: "Inter Bold, JetBrains Mono, Allerta Stencil (Draufsicht)" }],
    code: code.trim(),
    caption: "Umlaute, ẞ und deutsche Anführungszeichen ohne Warnung; Farben wie sonst.",
    checks: ["keine Fehler/Warnungen", "Breite > 50 mm, Höhe 2 mm", "„20°C“ in Allerta Stencil: Warnung, ° aus JetBrains Mono"],
  });
});

test("T02 SVG aus den Projektdateien; SVG ändern → Szene folgt", async ({ page }) => {
  await openApp(page);
  const main = `color("teal") linear_extrude(3) import("logo.svg");\n`;
  const v0 = (await cad(page)).sentVersion;
  await page.evaluate(([m, s]) => (window as unknown as W).__cad.openProject({ name: "logo", main: "main.scad", files: { "main.scad": m, "logo.svg": s } }), [main, LOGO(20)] as const);
  await waitScene(page, { minVersion: v0 + 1 });
  await hook(page, (c) => c.setView("top"));
  expect(await errors(page)).toEqual([]);
  expect(await width(page)).toBeCloseTo(34, 3); // rect 0..20, circle to 34
  await screenshot16(page, "t02-vorher", "full");
  // open the SVG in the editor and change it: the scene follows
  expect(await page.evaluate(() => (window as unknown as W).__cad.switchFile("logo.svg"))).toBe(true);
  const v1 = (await cad(page)).sentVersion;
  await hook(page, (c, t) => c.setText(t as string), LOGO(30));
  await waitScene(page, { minVersion: v1 + 1 });
  expect(await width(page)).toBeCloseTo(44, 3);
  await screenshot16(page, "t02-nachher", "full");
  expect((await info(page)).files["logo.svg"]).toContain('width="30"');
  record16({
    id: "T02",
    title: "SVG als Projektdatei, im Editor geändert",
    images: [
      { name: "t02-vorher", label: "Rechteck 20 mm + Kreis" },
      { name: "t02-nachher", label: "logo.svg im Editor: Rechteck 30 mm" },
    ],
    code: main.trim(),
    caption: "Das SVG ist eine Projektdatei (Tab), wird im Editor geändert; die Szene rechnet neu (Cache-Schlüssel aus dem Inhalt).",
    checks: ["Breite 34 mm", "nach der Änderung 44 mm"],
  });
});

test("T03 Schrift ins Projekt laden", async ({ page }) => {
  await openApp(page);
  await loadText(page, `cube(1);\n`);
  const ttf = readFileSync(join(FONTS, "AllertaStencil-Regular.ttf"));
  await page.getByTestId("file-upload-input").setInputFiles({ name: "schablone.ttf", mimeType: "font/ttf", buffer: ttf });
  await page.waitForFunction(() => (window as unknown as W).__cad.projectInfo().paths.includes("schablone.ttf"));
  const f = (await info(page)).files["schablone.ttf"];
  expect(f.startsWith("data:font/ttf;base64,")).toBe(true);
  await loadText(page, `linear_extrude(2) text("HALLO", font = "schablone.ttf");\n`);
  expect(await errors(page)).toEqual([]);
  expect(await warnings(page)).toEqual([]);
  expect(await width(page)).toBeGreaterThan(30);
  // the font tab does not open in the editor
  expect(await page.evaluate(() => (window as unknown as W).__cad.switchFile("schablone.ttf"))).toBe(false);
  await expect(page.getByTestId("status-notice")).toContainText("Schriftdatei");
  await hook(page, (c) => c.setView("top"));
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
  await screenshot16(page, "t03-schrift", "full");
  record16({
    id: "T03",
    title: "Eigene Schrift (Upload)",
    images: [{ name: "t03-schrift", label: "„⇡ Datei laden“: schablone.ttf, text(…, font = \"schablone.ttf\")" }],
    caption: "Binärdateien liegen im Projekt als data-URL; die Engine dekodiert sie. Der Tab öffnet keinen Editor.",
    checks: ["Datei im Projekt", "keine Warnung", "Tab: Hinweis statt Editor"],
  });
});

test("T04 Beispiel svg_logo mit seinen SVG-Dateien; Warnungen mit Zeile", async ({ page }) => {
  await openApp(page);
  const v0 = (await cad(page)).sentVersion;
  expect(await page.evaluate(() => (window as unknown as W).__cad.openExample("spec/svg_logo.scad"))).toBe(true);
  await waitScene(page, { minVersion: v0 + 1 });
  const p = await info(page);
  expect(p.main).toBe("spec/svg_logo.scad");
  expect(p.paths).toEqual(expect.arrayContaining(["svg/logo.svg", "svg/blatt.svg", "svg/haus.svg", "svg/welle.svg"]));
  expect(await errors(page)).toEqual([]);
  await hook(page, (c) => c.setView("iso"));
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
  await screenshot16(page, "t04-beispiel", "full");
  // unsupported SVG content: a warning with the SVG line
  const v1 = (await cad(page)).sentVersion;
  await page.evaluate(() =>
    (window as unknown as W).__cad.openProject({
      name: "w",
      main: "main.scad",
      files: {
        "main.scad": 'linear_extrude(1) import("w.svg");\n',
        "w.svg": '<svg xmlns="http://www.w3.org/2000/svg">\n<rect width="10" height="10"/>\n<text>x</text>\n</svg>\n',
      },
    }),
  );
  await waitScene(page, { minVersion: v1 + 1 });
  expect((await warnings(page)).some((m) => m.startsWith("w.svg:3:") && m.includes("Text"))).toBe(true);
  record16({
    id: "T04",
    title: "Beispiel mit SVG-Dateien",
    images: [{ name: "t04-beispiel", label: "examples/spec/svg_logo.scad als Projekt mit svg/*.svg" }],
    caption: "Einzelbeispiele, die SVGs aus examples/svg importieren, öffnen als Projekt mit diesen Dateien.",
    checks: ["4 SVG-Dateien im Projekt", "keine Fehler", "<text> im SVG: Warnung „w.svg:3: …Text…“"],
  });
});

test("T05 Messung: Text im Browser (Kernel-Zeit)", async ({ page }) => {
  test.setTimeout(240_000);
  await openApp(page);
  const cases: Array<[string, string]> = [
    ["hello", `linear_extrude(2) text("Hello World", font = "Inter");\n`],
    ["40", `linear_extrude(2) text("The quick brown fox jumps over the lazy d", font = "Inter");\n`],
  ];
  const out: Record<string, unknown> = {};
  for (const [name, code] of cases) {
    await loadText(page, code);
    const st = await page.evaluate(() => {
      const c = (window as unknown as W).__cad.store.getState().complete as { stats: Record<string, number> } | null;
      return c?.stats ?? null;
    });
    out[name] = st;
    expect(await errors(page)).toEqual([]);
  }
  const { mkdirSync, writeFileSync } = await import("node:fs");
  const dir = join(dirname(fileURLToPath(import.meta.url)), "../../reports/tmp/report16");
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, "browser-metrics.json"), JSON.stringify(out, null, 2));
});
