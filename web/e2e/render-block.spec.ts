// End-to-end tests of the studio look, part 2: edge lines on/off, physically based materials
// with reflections of a procedural environment, several lights, and the `render { … }` block of
// the file that the studio panel writes back into the source.

import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { expect, test, type Page } from "@playwright/test";
import { frame, loadText, openApp } from "./helpers";

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
