// End-to-end tests for test report 17 (Schablonenschrift „cad stencil mono“, Plan 17.4/17.11):
// text() without `font` uses the stencil font in the browser too (Wasm engine), its parameters,
// the example stencil, and the kernel time of a sign text against Allerta Stencil.

import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { expect, test, type Page } from "@playwright/test";
import { cad, hook, loadText, openApp, openExample, record17, screenshot17 } from "./helpers";

test.beforeEach(async ({ page }) => {
  page.on("pageerror", (e) => console.log("pageerror:", e.message));
  await page.addInitScript(() => localStorage.setItem("cad-colors-open", "0"));
});

type W = { __cad: { store: { getState(): Record<string, unknown> } } };

const bounds = async (page: Page) => {
  const b = await hook(page, (c) => c.rootBounds());
  if (!b) throw new Error("no bounds");
  return b;
};
const diagnostics = async (page: Page) => (await cad(page)).diagnostics.map((d) => `${d.severity}: ${d.message}`);
const frames = (page: Page) => page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));

/** em = size / 0.72, advance 600/1000 em, positions × 1000/1024 (like OpenSCAD and JetBrains Mono). */
const advance = (size: number) => ((size / 0.72) * 0.6 * 1000) / 1024;

test("K01 Standardschrift ohne font: Schablonenschrift, Parameter", async ({ page }) => {
  await openApp(page);
  const code = `linear_extrude(1) text("HALLO 08", size = 10);
translate([0, -16, 0]) linear_extrude(1) text("Größe ÄÖÜ", size = 10, stroke_width = 2, bridge_width = 1.5);
`;
  await loadText(page, code);
  expect(await diagnostics(page)).toEqual([]);
  const b = await bounds(page);
  // first line: cap height = size, 8 cells; second line below
  expect(b.max[1]).toBeCloseTo(10, 6);
  expect(b.max[2]).toBeCloseTo(1, 6);
  expect(b.max[0] - b.min[0]).toBeGreaterThan(7 * advance(10));
  expect(b.max[0]).toBeLessThan(9 * advance(10));
  await hook(page, (c) => c.setView("top"));
  await frames(page);
  await screenshot17(page, "k01-standard", "full");
  // limits: warnings at the argument; other fonts ignore the parameters with a warning
  await loadText(page, `text("A", size = 10, stroke_width = 3);\ntext("B", font = "Inter", bridge_width = 1);\n`);
  const d = await diagnostics(page);
  expect(d.some((m) => m.includes("stroke_width") && m.includes("höchstens 2 mm"))).toBe(true);
  expect(d.some((m) => m.includes("wirkt nur") && m.includes("cad stencil mono"))).toBe(true);
  record17({
    id: "K01",
    title: "Standardschrift im Browser",
    images: [{ name: "k01-standard", label: "text() ohne font: cad stencil mono, zweite Zeile mit stroke_width = 2, bridge_width = 1.5" }],
    code: code.trim(),
    caption: "Die Wasm-Engine nutzt dieselbe Schrift wie die CLI: Versalhöhe = size, Vorschub wie JetBrains Mono, Umlaute ohne Rückfall.",
    checks: ["keine Meldungen", "Versalhöhe 10 mm, Höhe 1 mm", "Breite 7–9 Zellen", "Grenzen und fremde Schrift: Warnungen"],
  });
});

test("K02 Beispiel Schablone", async ({ page }) => {
  await openApp(page);
  await openExample(page, "spec/schablone.mcad");
  expect((await cad(page)).diagnostics.filter((x) => x.severity !== "info")).toEqual([]);
  const b = await bounds(page);
  expect(b.max[0] - b.min[0]).toBeCloseTo(120, 6);
  expect(b.max[1] - b.min[1]).toBeCloseTo(44, 6);
  expect(b.max[2] - b.min[2]).toBeCloseTo(1, 6);
  await hook(page, (c) => c.setView("top"));
  await frames(page);
  await screenshot17(page, "k02-schablone", "view");
  record17({
    id: "K02",
    title: "Beispiel Schablone",
    images: [{ name: "k02-schablone", label: "tests/models/spec/schablone.mcad im Browser (Draufsicht)" }],
    caption: "Platte minus Text: die Innenteile von A, 0, 8, B, 9 hängen über die Stege an der Platte.",
    checks: ["keine Fehler/Warnungen", "Hülle 120 × 44 × 1 mm"],
  });
});

test("K03 Messung: Schild im Browser, Schablonenschrift gegen Allerta Stencil", async ({ page }) => {
  test.setTimeout(240_000);
  await openApp(page);
  const sign = (font: string) =>
    `cube([100, 14, 2]); translate([50, 4, 2]) linear_extrude(1.2) text("ZUTRITT NUR MIT BRILLE", size = 4.2, halign = "center"${font});\n`;
  const out: Record<string, unknown> = {};
  for (const [name, code] of [
    ["schild-stencil", sign("")],
    ["schild-allerta", sign(', font = "Allerta Stencil"')],
    ["hello-stencil", `linear_extrude(2) text("Hello World");\n`],
    ["hello-allerta", `linear_extrude(2) text("Hello World", font = "Allerta Stencil");\n`],
  ] as const) {
    await loadText(page, code);
    expect((await cad(page)).diagnostics.filter((x) => x.severity === "error")).toEqual([]);
    out[name] = await page.evaluate(() => ((window as unknown as W).__cad.store.getState().complete as { stats: Record<string, number> } | null)?.stats ?? null);
  }
  const { mkdirSync, writeFileSync } = await import("node:fs");
  const dir = join(dirname(fileURLToPath(import.meta.url)), "../../reports/tmp/report17");
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, "browser-metrics.json"), JSON.stringify(out, null, 2));
});
