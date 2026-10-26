// End-to-end tests for test report 16 (text and SVG import, Plan 17.4/17.5): text in the
// bundled fonts, an SVG of the project files as profile, editing the SVG in the editor moves the
// scene, a font uploaded into the project, the SVG example with its files, SVG warnings with
// their line.

import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { expect, test, type Page } from "@playwright/test";
import { cad, clickLine, fileMenu, hook, loadText, openApp, record16, screenshot16, waitScene } from "./helpers";

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

test("T01 text in the bundled fonts", async ({ page }) => {
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
    title: "Text in three fonts",
    images: [{ name: "t01-text", label: "Inter Bold, JetBrains Mono, Allerta Stencil (top view)" }],
    code: code.trim(),
    caption: "Umlauts, ẞ and German quotation marks without a warning; colours as usual.",
    checks: ["no errors/warnings", "width > 50 mm, height 2 mm", "“20°C” in Allerta Stencil: warning, ° from JetBrains Mono"],
  });
});

test("T02 SVG from the project files; change the SVG → the scene follows", async ({ page }) => {
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
    title: "SVG as a project file, changed in the editor",
    images: [
      { name: "t02-vorher", label: "rectangle 20 mm + circle" },
      { name: "t02-nachher", label: "logo.svg in the editor: rectangle 30 mm" },
    ],
    code: main.trim(),
    caption: "The SVG is a project file (tab), changed in the editor; the scene is evaluated again (cache key from the content).",
    checks: ["width 34 mm", "after the change 44 mm"],
  });
});

test("T03 load a font into the project", async ({ page }) => {
  await openApp(page);
  await loadText(page, `cube(1);\n`);
  const ttf = readFileSync(join(FONTS, "AllertaStencil-Regular.ttf"));
  // "Datei › Datei laden …" (also in the "+" menu of the tabs) opens the file dialog
  const [chooser] = await Promise.all([page.waitForEvent("filechooser"), fileMenu(page, "Datei laden …")]);
  await chooser.setFiles({ name: "stencil.ttf", mimeType: "font/ttf", buffer: ttf });
  await page.waitForFunction(() => (window as unknown as W).__cad.projectInfo().paths.includes("stencil.ttf"));
  const f = (await info(page)).files["stencil.ttf"];
  expect(f.startsWith("data:font/ttf;base64,")).toBe(true);
  await loadText(page, `linear_extrude(2) text("HELLO", font = "stencil.ttf");\n`);
  expect(await errors(page)).toEqual([]);
  expect(await warnings(page)).toEqual([]);
  expect(await width(page)).toBeGreaterThan(30);
  // the font tab does not open in the editor
  expect(await page.evaluate(() => (window as unknown as W).__cad.switchFile("stencil.ttf"))).toBe(false);
  await expect(page.getByTestId("status-notice")).toContainText("Schriftdatei");
  await hook(page, (c) => c.setView("top"));
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
  await screenshot16(page, "t03-schrift", "full");
  record16({
    id: "T03",
    title: "Own font (upload)",
    images: [{ name: "t03-schrift", label: "“⇡ Datei laden” (load file): stencil.ttf, text(…, font = \"stencil.ttf\")" }],
    caption: "Binary files are kept in the project as data URLs; the engine decodes them. The tab opens no editor.",
    checks: ["file in the project", "no warning", "tab: a notice instead of an editor"],
  });
});

test("T04 example svg_logo with its SVG files; warnings with their line", async ({ page }) => {
  await openApp(page);
  const v0 = (await cad(page)).sentVersion;
  expect(await page.evaluate(() => (window as unknown as W).__cad.openExample("spec/svg_logo.mcad"))).toBe(true);
  await waitScene(page, { minVersion: v0 + 1 });
  const p = await info(page);
  expect(p.main).toBe("spec/svg_logo.mcad");
  expect(p.paths).toEqual(expect.arrayContaining(["svg/logo.svg", "svg/leaf.svg", "svg/house.svg", "svg/wave.svg"]));
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
    title: "Example with SVG files",
    images: [{ name: "t04-beispiel", label: "tests/models/spec/svg_logo.mcad as a project with svg/*.svg" }],
    caption: "Single examples that import SVGs from tests/models/svg open as a project with these files.",
    checks: ["4 SVG files in the project", "no errors", "<text> in the SVG: warning “w.svg:3: …Text…”"],
  });
});

test("T05 measurement: text in the browser (kernel time)", async ({ page }) => {
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
