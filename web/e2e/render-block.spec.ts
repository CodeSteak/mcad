// End-to-end tests of the studio look, part 2: edge lines on/off, physically based materials
// with reflections of a procedural environment, several lights, and the `render { … }` block of
// the file that the studio panel writes back into the source.

import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { expect, test, type Page } from "@playwright/test";
import { frame, hook, loadText, openApp } from "./helpers";

const here = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(here, "../..");
const example = (name: string) => readFileSync(join(ROOT, "examples", name), "utf8");

interface Drawn {
  on: boolean;
  shadows: boolean;
  ao: boolean;
  ground: boolean;
  edges: boolean;
  edgeLines: number;
  lights: number;
  environment: string | null;
  toneMapping: boolean;
  groundReflection: number;
}
type W = { __cad: { studioInfo(): { settings: Record<string, unknown>; drawn: Drawn | null }; setStudio(p: unknown): void } };

const info = (page: Page) => page.evaluate(() => (window as unknown as W).__cad.studioInfo());
const settingsButton = (page: Page) => page.getByRole("button", { name: "Studio-Einstellungen", exact: true });
const panel = (page: Page) => page.getByRole("dialog", { name: "Studio-Einstellungen" });

async function drawn(page: Page, pred: (d: Drawn) => boolean): Promise<Drawn> {
  await expect.poll(async () => {
    const d = (await info(page)).drawn;
    return d !== null && pred(d);
  }, { timeout: 20_000 }).toBe(true);
  await frame(page);
  return (await info(page)).drawn!;
}

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    if (!sessionStorage.getItem("render-block-spec")) {
      localStorage.removeItem("cad-studio");
      sessionStorage.setItem("render-block-spec", "1");
    }
  });
});

test("R01 edge lines can be hidden in the studio look", async ({ page }) => {
  await openApp(page);
  await loadText(page, example("05-box-lid.mcad"));
  await page.evaluate(() => (window as unknown as W).__cad.setStudio({ on: true }));
  const on = await drawn(page, (d) => d.on && d.edgeLines > 0);
  expect(on.edges).toBe(true);
  await settingsButton(page).click();
  await panel(page).getByRole("checkbox", { name: "Kantenlinien" }).uncheck();
  const off = await drawn(page, (d) => d.on && !d.edges);
  expect(off.edgeLines).toBe(0);
  await panel(page).getByRole("checkbox", { name: "Kantenlinien" }).check();
  await drawn(page, (d) => d.edges && d.edgeLines > 0);
});

const SHOTS = process.env.STUDIO2_SHOTS ?? "";
async function shot(page: Page, name: string, area: "view" | "full" = "view"): Promise<void> {
  if (!SHOTS) return;
  const clip = area === "view" ? await page.locator("section.viewcol").boundingBox() : null;
  await page.screenshot({ path: join(SHOTS, `${name}.png`), ...(clip ? { clip } : {}) });
}

/** The view canvas as a PNG data URL. */
const grab = (page: Page) => page.evaluate(() => (document.querySelector("[data-testid=viewport-canvas]") as HTMLCanvasElement).toDataURL("image/png"));

/** Share of pixels that differ by more than 12 in a channel. */
async function differ(page: Page, a: string, b: string): Promise<number> {
  return page.evaluate(async ([a, b]) => {
    const load = async (u: string) => {
      const img = new Image();
      img.src = u;
      await img.decode();
      const c = document.createElement("canvas");
      c.width = img.width;
      c.height = img.height;
      const ctx = c.getContext("2d")!;
      ctx.drawImage(img, 0, 0);
      return ctx.getImageData(0, 0, c.width, c.height).data;
    };
    const [x, y] = [await load(a), await load(b)];
    let n = 0;
    for (let i = 0; i < x.length; i += 4) if (Math.abs(x[i] - y[i]) > 12 || Math.abs(x[i + 1] - y[i + 1]) > 12 || Math.abs(x[i + 2] - y[i + 2]) > 12) n++;
    return n / (x.length / 4);
  }, [a, b] as const);
}

test("R02 materials reflect a procedural environment; lights can be added", async ({ page }) => {
  test.setTimeout(120_000);
  await openApp(page);
  await loadText(page, example("07-mug.mcad"));
  const set = (p: unknown) => page.evaluate((x) => (window as unknown as W).__cad.setStudio(x), p);
  await set({ on: true, edges: false });
  const d = await drawn(page, (x) => x.on && !x.edges);
  expect(d).toMatchObject({ environment: "studio", toneMapping: true, lights: 1 });
  await shot(page, "ui-plastic");
  const plastic = await grab(page);
  await set({ materials: [{ target: { color: "default" }, preset: "chrome", metalness: null, roughness: null, clearcoat: null }] });
  await frame(page);
  await frame(page);
  await shot(page, "ui-chrome");
  const chrome = await grab(page);
  expect(await differ(page, plastic, chrome)).toBeGreaterThan(0.05);
  // the panel lists the material of the uncoloured material and shows the preset
  await settingsButton(page).click();
  await expect(panel(page).getByTestId("studio-preset-default")).toHaveValue("chrome");
  await panel(page).getByTestId("studio-preset-default").selectOption("aluminium");
  await expect.poll(async () => JSON.stringify((await info(page)).settings.materials)).toContain("aluminium");
  // a second light
  await panel(page).getByTestId("studio-add-light").click();
  await drawn(page, (x) => (x as Drawn & { lights: number }).lights === 2);
  await shot(page, "ui-panel-lights", "full");
  // no environment: no reflections
  await panel(page).getByTestId("studio-environment").selectOption("none");
  await drawn(page, (x) => (x as Drawn & { environment: string | null }).environment === null);
});

const text = (page: Page) => hook(page, (c) => c.text());

test("R03 changes in the studio panel write the render block into the source; undo takes them back", async ({ page }) => {
  test.setTimeout(120_000);
  await openApp(page);
  const src = example("05-box-lid.mcad");
  await loadText(page, src);
  await page.evaluate(() => (window as unknown as W).__cad.setStudio({ on: true }));
  await settingsButton(page).click();
  await expect(panel(page).getByTestId("studio-source")).toContainText("render-Block in die Datei");
  // light direction: a new block at the end of the file, the rest unchanged
  await panel(page).getByRole("slider", { name: "Richtung" }).fill("60");
  await expect.poll(() => text(page), { timeout: 20_000 }).toContain("light key { azimuth = 60; elevation = 50; }");
  let t = await text(page);
  expect(t.startsWith(src)).toBe(true);
  expect(t.slice(src.length)).toBe("\nrender {\n    light key { azimuth = 60; elevation = 50; }\n}\n");
  // the panel now names the block's line
  await expect(panel(page).getByTestId("studio-source")).toContainText("Zeile");
  // edge lines off: a new setting in the block; the light again: in place
  await panel(page).getByRole("checkbox", { name: "Kantenlinien" }).uncheck();
  await expect.poll(() => text(page), { timeout: 20_000 }).toContain("edges = false;");
  await panel(page).getByRole("slider", { name: "Richtung" }).fill("-90");
  await expect.poll(() => text(page), { timeout: 20_000 }).toContain("azimuth = -90;");
  t = await text(page);
  expect(t.match(/light key/g)?.length).toBe(1);
  expect(t.match(/render \{/g)?.length).toBe(1);
  expect(t.slice(src.length)).toBe("\nrender {\n    edges = false;\n    light key { azimuth = -90; elevation = 50; }\n}\n");
  // a material of the uncoloured material
  await panel(page).getByTestId("studio-preset-default").selectOption("chrome");
  await expect.poll(() => text(page), { timeout: 20_000 }).toContain('material "default" { preset = "chrome"; }');
  await shot(page, "ui-writeback", "full");
  // undo in the editor: the last change goes (one undo step per change)
  await page.keyboard.press("Escape");
  await hook(page, (c) => c.focusEditor());
  await page.keyboard.press("Control+z");
  await expect.poll(() => text(page), { timeout: 10_000 }).not.toContain("material");
  expect(await text(page)).toContain("azimuth = -90;");
  // the settings follow the file again (the material is gone)
  await expect.poll(async () => JSON.stringify((await info(page)).settings.materials), { timeout: 20_000 }).toBe("[]");
});

test("R04 a file with a render block: the studio look follows the file", async ({ page }) => {
  test.setTimeout(120_000);
  await openApp(page);
  await loadText(page, example("09-flashlight.mcad"));
  // the block switches the look on and sets lights, edges, materials
  const d = await drawn(page, (x) => x.on && x.lights === 2);
  expect(d).toMatchObject({ edges: false, environment: "studio" });
  const s = (await info(page)).settings as { ambient: number; lights: Array<{ name: string }>; materials: Array<{ target: { color?: string }; preset: string | null }> };
  expect(s.ambient).toBe(0.45);
  expect(s.lights.map((l) => l.name)).toEqual(["key", "rim"]);
  expect(s.materials.find((m) => m.target.color === "silver")?.preset).toBe("aluminium");
  // the mirror image in the ground from the block
  expect((s as unknown as { groundReflection: number }).groundReflection).toBe(0.3);
  expect((await drawn(page, (x) => x.groundReflection > 0)).groundReflection).toBe(0.3);
  await shot(page, "ui-flashlight");
  // typing in the block changes the view's settings
  const t = await text(page);
  await hook(page, (c, x) => c.setText(x as string), t.replace("ambient = 0.45;", "ambient = 0.2;"));
  await expect.poll(async () => ((await info(page)).settings as { ambient: number }).ambient, { timeout: 20_000 }).toBe(0.2);
  await settingsButton(page).click();
  await expect(panel(page).getByTestId("studio-source")).toContainText("Zeile 36");
  await expect(panel(page).getByTestId("studio-preset-silver")).toHaveValue("aluminium");
  await shot(page, "ui-flashlight-panel", "full");
});

test("R05 ground reflection, real metals and brushing; the panel writes the new keys back", async ({ page }) => {
  test.setTimeout(150_000);
  const shaderErrors: string[] = [];
  page.on("console", (m) => {
    if (/Shader Error|shader chunk changed|WebGLProgram/i.test(m.text())) shaderErrors.push(m.text());
  });
  await openApp(page);
  await loadText(page, example("07-mug.mcad"));
  const set = (p: unknown) => page.evaluate((x) => (window as unknown as W).__cad.setStudio(x), p);
  await set({ on: true, edges: false, groundReflection: 0 });
  await drawn(page, (x) => x.on && x.groundReflection === 0);
  const plain = await grab(page);
  await set({ groundReflection: 0.7, groundRoughness: 0 });
  expect((await drawn(page, (x) => x.groundReflection > 0)).groundReflection).toBe(0.7);
  await frame(page);
  await shot(page, "ui-ground-reflection");
  // the mirror image below the mug changes the ground
  expect(await differ(page, plain, await grab(page))).toBeGreaterThan(0.01);
  // presets with their own colour and a brushed one compile and draw
  const m = (preset: string) => [{ target: { color: "default" }, preset, metalness: null, roughness: null, clearcoat: null, anisotropy: null }];
  await set({ materials: m("gold") });
  await frame(page);
  await frame(page);
  const gold = await grab(page);
  await set({ materials: m("metal_brushed") });
  await frame(page);
  await frame(page);
  await shot(page, "ui-brushed");
  expect(await differ(page, gold, await grab(page))).toBeGreaterThan(0.05);
  expect(shaderErrors).toEqual([]);
  // panel → render block
  await settingsButton(page).click();
  await panel(page).getByTestId("studio-groundReflection").fill("0.4");
  await expect.poll(() => text(page), { timeout: 20_000 }).toContain("ground_reflection = 0.4;");
  await panel(page).getByTestId("studio-reflections").uncheck();
  await expect.poll(() => text(page), { timeout: 20_000 }).toContain("reflections = false;");
  await panel(page).getByTestId("studio-anisotropy-default").fill("0.5");
  await expect.poll(() => text(page), { timeout: 20_000 }).toContain("anisotropy = 0.5;");
  await panel(page).getByTestId("studio-environment").selectOption("soft");
  await expect.poll(() => text(page), { timeout: 20_000 }).toContain('environment = "soft";');
  await expect(panel(page).getByTestId("studio-environment").locator("option")).toHaveText(["Studio (dunkel, Softboxen)", "Helles Studio (weich)", "Bedeckt", "Keine (keine Spiegelungen)"]);
  await shot(page, "ui-panel-reflections", "full");
});
