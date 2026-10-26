// Colours, filaments and the dark theme (test report 11): colour list with slots, volumes and
// masses; changing a slot writes `filament = n`; multi-colour exports; colour regions in the
// view; dark theme (computed styles, pixels) and "System".

import { readFileSync } from "node:fs";
import { type Page, expect, test } from "@playwright/test";
import { cad, clickLine, loadText, menuItem, openApp, openExample, record11, screenshot11, setTheme, sideTab, waitScene, waitView, writeMetrics11 } from "./helpers";

type Hook = { __cad: { text(): string; state(): { colors: Array<{ label: string; filament: number; explicit: boolean; key: string; source: string }> } } };

const EXAMPLE = "spec/multicolor.mcad";

// Region meshes, canvas screenshots and pixel counts take a while with SwiftShader.
test.setTimeout(180_000);

async function text(page: Page): Promise<string> {
  return page.evaluate(() => (window as unknown as Hook).__cad.text());
}

/** RGB of a computed CSS colour property of the first element matching `sel`. */
async function cssColor(page: Page, sel: string, prop: string): Promise<[number, number, number]> {
  return page.evaluate(
    ([s, p]) => {
      const el = document.querySelector(s)!;
      const v = getComputedStyle(el).getPropertyValue(p);
      const c = document.createElement("canvas").getContext("2d")!;
      c.fillStyle = v;
      const h = c.fillStyle as string;
      if (h.startsWith("#")) return [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16)) as [number, number, number];
      const m = /rgba?\(([^)]+)\)/.exec(h)!;
      return m[1].split(",").slice(0, 3).map(Number) as [number, number, number];
    },
    [sel, prop] as const,
  );
}

function lum(c: [number, number, number]): number {
  const l = (x: number) => {
    const v = x / 255;
    return v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * l(c[0]) + 0.7152 * l(c[1]) + 0.0722 * l(c[2]);
}

function ratio(a: [number, number, number], b: [number, number, number]): number {
  const [x, y] = [lum(a), lum(b)];
  return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05);
}

/** Contrast of text on its (opaque) panel background. */
async function textContrast(page: Page, textSel: string, bgSel: string): Promise<number> {
  return ratio(await cssColor(page, textSel, "color"), await cssColor(page, bgSel, "background-color"));
}

/** Canvas pixels within `tol` of a colour, and the mean RGB of the four corners. */
async function canvasPixels(page: Page, hex: string, tol = 22): Promise<{ count: number; corner: [number, number, number] }> {
  const png = await page.getByTestId("viewport-canvas").screenshot();
  return page.evaluate(
    async ([b64, hx, t]) => {
      const rgb = [1, 3, 5].map((i) => parseInt((hx as string).slice(i, i + 2), 16));
      const img = new Image();
      img.src = `data:image/png;base64,${b64}`;
      await img.decode();
      const c = document.createElement("canvas");
      c.width = img.width;
      c.height = img.height;
      const g = c.getContext("2d")!;
      g.drawImage(img, 0, 0);
      const d = g.getImageData(0, 0, c.width, c.height).data;
      let n = 0;
      for (let i = 0; i < d.length; i += 4) {
        if (Math.abs(d[i] - rgb[0]) <= (t as number) && Math.abs(d[i + 1] - rgb[1]) <= (t as number) && Math.abs(d[i + 2] - rgb[2]) <= (t as number)) n++;
      }
      const px = (x: number, y: number) => {
        const i = (y * c.width + x) * 4;
        return [d[i], d[i + 1], d[i + 2]];
      };
      const cs = [px(4, 4), px(c.width - 5, 4), px(4, c.height - 5), px(c.width - 5, c.height - 5)];
      const corner = [0, 1, 2].map((k) => cs.reduce((a, p) => a + p[k], 0) / 4) as [number, number, number];
      return { count: n, corner };
    },
    [png.toString("base64"), hex, tol] as const,
  );
}

const colorsRegion = (page: Page) => page.getByRole("region", { name: "Farben und Filamente" });

async function openModel(page: Page): Promise<void> {
  await openApp(page);
  await openExample(page, EXAMPLE);
  await sideTab(page, "Farben").click();
  await expect(page.getByRole("region", { name: "Farben und Filamente" })).toBeVisible();
  // volumes arrive with the colour regions
  await expect(page.getByTestId("color-volume-white")).not.toContainText("…");
}

test("F01 colour list: palette, slots, volumes and masses, colour per face", async ({ page }) => {
  await page.emulateMedia({ colorScheme: "light" });
  await openModel(page);
  const s = await cad(page);
  expect(s.colors.map((c) => [c.label, c.filament, c.explicit])).toEqual([
    ["white", 1, false],
    ["teal", 2, false],
    ["red", 3, true],
    ["blue", 4, false],
    ["yellow", 5, false],
  ]);
  await expect(page.getByRole("combobox", { name: "Filament für red" })).toHaveValue("3");
  await expect(page.getByRole("combobox", { name: "Filament für teal" })).toHaveValue("auto");
  await expect(page.getByRole("combobox", { name: "Filament für teal" }).locator("option").first()).toHaveText("2 (auto)");
  // volumes add up to the part volumes
  const regions = s.regions!;
  expect(regions.items.map((x) => x.part)).toEqual(["door_sign", "figure"]);
  for (const set of regions.items) {
    const sum = set.regions.reduce((a, r) => a + r.volume, 0);
    expect(Math.abs(sum - set.total)).toBeLessThan(1e-6 * sum);
  }
  await expect(page.getByTestId("color-volume-red")).toContainText("208.8 mm³");
  await expect(page.getByTestId("color-volume-red")).toContainText("0.26 g");
  // swatches: palette values (light theme)
  expect(await cssColor(page, "[data-testid=color-swatch-red]", "background-color")).toEqual([0xc7, 0x42, 0x40]);
  expect(await cssColor(page, "[data-testid=color-swatch-teal]", "background-color")).toEqual([0x21, 0x72, 0x74]);
  // colours per face in the 3D view
  expect(s.view!.faceColors).toBeGreaterThan(4);
  const red = await canvasPixels(page, "#c74240", 40);
  const teal = await canvasPixels(page, "#217274", 40);
  expect(red.count).toBeGreaterThan(200);
  expect(teal.count).toBeGreaterThan(200);
  await screenshot11(page, "f01-farbliste-hell");
  await screenshot11(page, "f01-panel-hell", "colors");
  writeMetrics11({ f01: { redPixels: red.count, tealPixels: teal.count, regions: regions.items } });
  record11({
    id: "F01",
    title: "Colour list with slots, volumes, masses",
    caption: "Example multicolor.mcad: five colours in the order of their first appearance, red fixed with filament = 3, the others automatic (1, 2, 4, 5). Volume per colour from the colour regions of both parts, mass for PLA 1.24 g/cm³. In the 3D view every face has the colour of its color() call.",
    images: [
      { name: "f01-farbliste-hell", label: "light theme, colour list on the right" },
      { name: "f01-panel-hell", label: "colour list" },
    ],
    checks: [
      "labels and slots white 1, teal 2, red 3 (explicit), blue 4, yellow 5",
      "sum of the regions = volume per part",
      "red 208.8 mm³ / 0.26 g; swatches with palette values",
      `${red.count} red and ${teal.count} teal pixels in the view`,
    ],
  });
});

test("F02 changing a slot writes filament = n into the code", async ({ page }) => {
  await openModel(page);
  const before = await text(page);
  expect(before).toContain('color("teal") difference()');
  await page.getByRole("combobox", { name: "Filament für teal" }).selectOption("5");
  await expect.poll(() => text(page)).toContain('color("teal", filament = 5) difference()');
  await waitScene(page);
  await expect(page.getByRole("combobox", { name: "Filament für teal" })).toHaveValue("5");
  const s = await cad(page);
  const slots = Object.fromEntries(s.colors.map((c) => [c.label, c.filament]));
  // teal 5 fixed, blue moves to the lowest free slot
  expect(slots).toEqual({ white: 1, teal: 5, red: 3, blue: 2, yellow: 4 });
  await screenshot11(page, "f02-slot-geaendert", "full");
  // back to automatic: the argument is removed again
  await page.getByRole("combobox", { name: "Filament für teal" }).selectOption("auto");
  await expect.poll(() => text(page)).toBe(before);
  await waitScene(page);
  await expect(page.getByRole("combobox", { name: "Filament für teal" })).toHaveValue("auto");
  // the default material (no colour) has no slot choice
  await loadText(page, 'cube(10);\ncolor("red") translate([0, 0, 10]) cube(3);\n');
  await expect(page.getByRole("combobox", { name: "Filament für Standard" })).toBeDisabled();
  record11({
    id: "F02",
    title: "Slot by drop-down",
    caption: "Slot of teal to 5: the editor gets color(\"teal\", filament = 5) (text edit through the engine, undoable), blue moves to the free slot 2. “automatisch” (automatic) removes the argument again. Material without color() is always filament 1.",
    images: [{ name: "f02-slot-geaendert", label: "teal on slot 5, code changed" }],
    checks: ["the text contains color(\"teal\", filament = 5)", "slots then white 1, teal 5, red 3, blue 2, yellow 4", "automatic: text as before again", "default entry without a choice"],
  });
});

test("F03 multi-colour export: 3MF and STL per colour", async ({ page }) => {
  await openModel(page);
  const [dl] = await Promise.all([page.waitForEvent("download"), colorsRegion(page).getByRole("button", { name: "3MF mehrfarbig" }).click()]);
  expect(dl.suggestedFilename()).toBe("multicolor.3mf");
  const bytes = readFileSync((await dl.path())!);
  const t = bytes.toString("utf8");
  expect(bytes.subarray(0, 2).toString()).toBe("PK");
  // Report 12: no colour group – Bambu Studio keeps the slots without its colour dialog
  expect(t).not.toContain("<m:colorgroup");
  expect((t.match(/<m:color /g) ?? []).length).toBe(0);
  expect((t.match(/<item /g) ?? []).length).toBe(2);
  expect((t.match(/<component /g) ?? []).length).toBe(6);
  expect(t).toContain("Metadata/model_settings.config");
  expect(t).toContain('<metadata key="name" value="door_sign – red"/>\n      <metadata key="extruder" value="3"/>');
  const [dz] = await Promise.all([page.waitForEvent("download"), colorsRegion(page).getByRole("button", { name: "STL je Farbe (ZIP)" }).click()]);
  expect(dz.suggestedFilename()).toBe("multicolor-farben.zip");
  const z = readFileSync((await dz.path())!).toString("latin1");
  for (const f of ["door_sign_F1_white.stl", "door_sign_F2_teal.stl", "door_sign_F3_red.stl", "figure_F4_blue.stl", "figure_F1_white.stl", "figure_F5_yellow.stl"]) expect(z).toContain(f);
  const [ds] = await Promise.all([page.waitForEvent("download"), colorsRegion(page).getByRole("button", { name: "3MF einfarbig" }).click()]);
  expect(readFileSync((await ds.path())!).toString("latin1")).not.toContain("colorgroup");
  record11({
    id: "F03",
    title: "Export for Bambu Studio / OrcaSlicer",
    caption: "“3MF mehrfarbig” (multi-colour): one object per part (door_sign, figure), in it one part per colour without an offset, colour group with five colours, Metadata/model_settings.config with an extruder per part. “STL je Farbe” (STL per colour): ZIP with six files F<slot>_<colour>.stl. “3MF einfarbig” (single colour) without colours.",
    images: [],
    checks: ["2 build items, 6 components, no colour group (report 12)", "door_sign – red → extruder 3", "ZIP with 6 STL (part_F<slot>_<colour>.stl)", "single colour without colorgroup"],
  });
});

test("F04 show colour regions", async ({ page }) => {
  await openModel(page);
  await page.getByRole("checkbox", { name: "Farbregionen zeigen" }).check();
  const s = await waitView(page, "(s) => s.view.regionsDrawn === 6");
  expect(s.regions!.meshes).toBe(true);
  const yellow = await canvasPixels(page, "#f7d974", 40);
  expect(yellow.count).toBeGreaterThan(100);
  // a click on a region body selects its line like a normal face
  await screenshot11(page, "f04-farbregionen", "view");
  await page.getByRole("checkbox", { name: "Farbregionen zeigen" }).uncheck();
  await waitView(page, "(s) => s.view.regionsDrawn === 0");
  record11({
    id: "F04",
    title: "Colour regions in the view",
    caption: "“Farbregionen zeigen” (show colour regions): instead of the colour per face the bodies per colour, as they are exported (six bodies: three per part).",
    images: [{ name: "f04-farbregionen", label: "colour regions" }],
    checks: ["6 region bodies drawn", `${yellow.count} yellow pixels`, "off: colour per face again"],
  });
});

test("F05 dark theme: contrast, view, palette", async ({ page }) => {
  await page.emulateMedia({ colorScheme: "light" });
  await openModel(page);
  await setTheme(page, "dark");
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  await waitView(page, "(s) => s.theme === 'dark'");
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
  const checks: Record<string, number> = {
    "text on background": await textContrast(page, "body", "body"),
    "editor text": await textContrast(page, ".cm-content", ".editor .cm-editor"),
    "line numbers": await textContrast(page, ".cm-gutters", ".editor .cm-editor"),
    "status bar": await textContrast(page, "[data-testid=status-progress]", ".statusbar"),
    "colour list": await textContrast(page, "[data-testid=colors-panel]", ".sidepanel"),
    "header bar": await textContrast(page, ".hd .ui-btn--quiet", ".hd"),
    "save (primary)": await textContrast(page, "[data-testid=save]", "[data-testid=save]"),
    "toolbar": await textContrast(page, ".vtoolbar .ui-seg-item.is-on", ".vtoolbar .ui-seg-item.is-on"),
  };
  for (const [k, v] of Object.entries(checks)) expect(v, k).toBeGreaterThanOrEqual(4.5);
  // dark 3D view, palette colours in their dark variants
  const px = await canvasPixels(page, "#217274");
  expect(lum(px.corner)).toBeLessThan(0.02);
  expect(await cssColor(page, "[data-testid=color-swatch-teal]", "background-color")).toEqual([0x30, 0x7e, 0x80]);
  const tealDark = await canvasPixels(page, "#307e80", 40);
  expect(tealDark.count).toBeGreaterThan(200);
  // search panel of the editor follows the theme
  // (cursor on line 1: file context; a line in a module would switch the context)
  await clickLine(page, 1, 0);
  await page.evaluate(() => (window as unknown as { __cad: { focusEditor(): void } }).__cad.focusEditor());
  await page.keyboard.press("Control+f");
  await expect(page.locator(".cm-panels")).toBeVisible();
  const panelBg = await cssColor(page, ".cm-panels", "background-color");
  expect(lum(panelBg)).toBeLessThan(0.05);
  await screenshot11(page, "f05-dunkel-suche", "editor");
  await page.keyboard.press("Escape");
  await screenshot11(page, "f05-farbliste-dunkel");
  await screenshot11(page, "f05-panel-dunkel", "colors");
  writeMetrics11({ f05: { contrast: checks, viewCorner: px.corner, tealDarkPixels: tealDark.count } });
  record11({
    id: "F05",
    title: "Dark theme",
    caption: "Theme “Dunkel” (dark): panels, editor (syntax, line numbers, search bar), status bar and colour list dark, contrast of the texts ≥ 4.5 (computed styles); 3D view with a dark background, palette colours in their dark variant (teal #307e80 instead of #217274), the export uses the base values.",
    images: [
      { name: "f05-farbliste-dunkel", label: "dark theme" },
      { name: "f05-panel-dunkel", label: "colour list dark" },
      { name: "f05-dunkel-suche", label: "editor with the search bar" },
    ],
    checks: Object.entries(checks).map(([k, v]) => `${k}: ${v.toFixed(2)}`),
  });
});

test("F06 theme System follows prefers-color-scheme", async ({ page }) => {
  await page.emulateMedia({ colorScheme: "dark" });
  await page.goto("/");
  await page.evaluate(() => localStorage.removeItem("cad-theme"));
  await page.reload();
  await waitScene(page);
  await page.getByRole("button", { name: "Farbschema", exact: true }).click();
  await expect(menuItem(page, "System")).toHaveAttribute("aria-checked", "true");
  await page.keyboard.press("Escape");
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  await page.emulateMedia({ colorScheme: "light" });
  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
  await page.emulateMedia({ colorScheme: "dark" });
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  // a fixed choice ignores the system
  await setTheme(page, "light");
  await page.emulateMedia({ colorScheme: "dark" });
  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
  await page.reload();
  await waitScene(page);
  await page.getByRole("button", { name: "Farbschema", exact: true }).click();
  await expect(menuItem(page, "Hell")).toHaveAttribute("aria-checked", "true");
  await page.keyboard.press("Escape");
  await setTheme(page, "system");
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  record11({
    id: "F06",
    title: "Theme “System”",
    caption: "The default is “System”: the theme follows prefers-color-scheme, also when it switches while running; “Hell”/“Dunkel” (light/dark) are fixed and survive a reload.",
    images: [],
    checks: ["System + dark → dark, then light → light, dark → dark", "fixed light stays with a dark system", "the choice survives a reload"],
  });
});

test("F07 unknown colour name: warning and a stable random colour", async ({ page }) => {
  await openApp(page);
  await loadText(page, 'color("lightblue") cube(10);\ncolor("Teal") translate([12, 0, 0]) cube(10);\n');
  let s = await cad(page);
  const warn = s.diagnostics.find((d) => d.severity === "warning");
  expect(warn?.message).toContain("Farbe `lightblue` unbekannt – Zufallsfarbe");
  expect(warn?.line).toBe(1);
  expect(s.colors.map((c) => [c.label, c.source])).toEqual([
    ["lightblue", "random"],
    ["teal", "palette"],
  ]);
  const key = s.colors[0].key;
  // the list is the tab "Farben" of the side panel; the panel folds away and comes back
  await sideTab(page, "Farben").click();
  await expect(page.getByRole("region", { name: "Farben und Filamente" })).toBeVisible();
  await page.getByRole("button", { name: "Seitenpanel ausblenden" }).click();
  await expect(page.getByRole("region", { name: "Farben und Filamente" })).toBeHidden();
  await page.getByRole("button", { name: "Seitenpanel einblenden" }).click();
  await expect(page.getByRole("region", { name: "Farben und Filamente" })).toBeVisible();
  await expect(page.getByTestId("color-row-lightblue")).toContainText("zufällig");
  // stable: same name, same colour after another evaluation
  await loadText(page, 'cube(1);\ncolor("LightBlue") cube(10);\n');
  s = await cad(page);
  expect(s.colors.find((c) => c.source === "random")?.key).toBe(key);
  await screenshot11(page, "f07-zufallsfarbe");
  record11({
    id: "F07",
    title: "Unknown colour",
    caption: "color(\"lightblue\"): warning at the argument “Farbe `lightblue` unbekannt – Zufallsfarbe (Palette: …)” (unknown colour – random colour), the colour comes from the hash of the name (the same for LightBlue, the same lightness and saturation as the palette); “Teal” (case does not matter) is the palette colour.",
    images: [{ name: "f07-zufallsfarbe", label: "warning and colour list" }],
    checks: [`random colour ${key} for lightblue and LightBlue`, "warning on line 1", "Teal = palette"],
  });
});
