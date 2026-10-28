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
