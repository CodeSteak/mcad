// End-to-end tests of the studio look (test report 22): toggle in the view toolbar, settings
// panel (light, shadows, ambient occlusion, ground, background), settings kept after a reload,
// dark theme, section, "Als mcad render kopieren" and the same look in the CLI, English labels.
// Screenshots and measurements go to reports/tmp/report22/.

import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { expect, test, type Page } from "@playwright/test";
import { type CadState, cad, frame, hook, loadText, mode, openApp, setTheme } from "./helpers";

const here = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(here, "../..");
const OUT = join(ROOT, "reports/tmp/report22");
const example = (name: string) => readFileSync(join(ROOT, "examples", name), "utf8");

interface StudioInfo {
  settings: { on: boolean; ao: boolean; shadows: boolean; ground: boolean; lights: Array<{ azimuth: number; elevation: number }>; shadowSoftness: number; background: [string, string] | null };
  drawn: { on: boolean; shadows: boolean; ao: boolean; ground: boolean; aoRadius: number | null; light: [number, number, number] | null } | null;
}
type W = { __cad: { studioInfo(): StudioInfo; setStudio(p: unknown): void; renderBenchmark(n: number): number | null } };

/** Frames per mode of the frame-time benchmark in S01. */
const BENCH_FRAMES = 8;

const info = (page: Page) => page.evaluate(() => (window as unknown as W).__cad.studioInfo());
const setStudio = async (page: Page, p: Record<string, unknown>) => {
  await page.evaluate((x) => (window as unknown as W).__cad.setStudio(x), p);
  await frame(page);
  await frame(page);
};

async function shot(page: Page, name: string, area: "view" | "full" = "view"): Promise<void> {
  mkdirSync(join(OUT, "img"), { recursive: true });
  const clip = area === "view" ? await page.locator("section.viewcol").boundingBox() : null;
  await page.screenshot({ path: join(OUT, "img", `${name}.png`), ...(clip ? { clip } : {}) });
}

function record(entry: { id: string; title: string; caption: string; images: string[]; checks: string[] }): void {
  mkdirSync(OUT, { recursive: true });
  const f = join(OUT, "scenarios.json");
  const all: Array<{ id: string }> = existsSync(f) ? JSON.parse(readFileSync(f, "utf8")) : [];
  const i = all.findIndex((e) => e.id === entry.id);
  if (i >= 0) all[i] = entry;
  else all.push(entry);
  writeFileSync(f, JSON.stringify(all, null, 2));
}

function metrics(m: Record<string, unknown>): void {
  mkdirSync(OUT, { recursive: true });
  const f = join(OUT, "metrics.json");
  const old = existsSync(f) ? JSON.parse(readFileSync(f, "utf8")) : {};
  writeFileSync(f, JSON.stringify({ ...old, ...m }, null, 2));
}

/** The view canvas as a PNG data URL (the renderer keeps its drawing buffer). */
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

async function studioReady(page: Page, pred: (i: StudioInfo) => boolean = (i) => i.drawn?.on === true): Promise<StudioInfo> {
  await expect.poll(async () => pred(await info(page)), { timeout: 20_000 }).toBe(true);
  await frame(page);
  return info(page);
}

const toggle = (page: Page) => page.getByRole("button", { name: "Studio-Darstellung", exact: true });
const settingsButton = (page: Page) => page.getByRole("button", { name: "Studio-Einstellungen", exact: true });
const panel = (page: Page) => page.getByRole("dialog", { name: "Studio-Einstellungen" });

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    if (!sessionStorage.getItem("studio-spec")) {
      localStorage.removeItem("cad-studio");
      sessionStorage.setItem("studio-spec", "1");
    }
  });
});

test("S01 the toggle switches the view to the studio look (shadows, ambient occlusion, ground)", async ({ page }) => {
  // the frame benchmark is slow on software WebGL (about 1 s per studio frame)
  test.setTimeout(180_000);
  await openApp(page);
  await loadText(page, example("05-box-lid.mcad"));
  await expect(toggle(page)).toHaveAttribute("aria-pressed", "false");
  const before = await grab(page);
  await shot(page, "s01-standard");
  await toggle(page).click();
  const i = await studioReady(page);
  await expect(toggle(page)).toHaveAttribute("aria-pressed", "true");
  expect(i.drawn).toMatchObject({ on: true, shadows: true, ao: true, ground: true });
  expect(i.drawn!.aoRadius!).toBeGreaterThan(0);
  const after = await grab(page);
  const d = await differ(page, before, after);
  expect(d).toBeGreaterThan(0.2);
  await shot(page, "s01-studio");
  await setStudio(page, { on: false });
  const msStd = await page.evaluate((n) => (window as unknown as W).__cad.renderBenchmark(n), BENCH_FRAMES);
  await setStudio(page, { on: true });
  const msStudio = await page.evaluate((n) => (window as unknown as W).__cad.renderBenchmark(n), BENCH_FRAMES);
  await setStudio(page, { ao: false });
  const msNoAo = await page.evaluate((n) => (window as unknown as W).__cad.renderBenchmark(n), BENCH_FRAMES);
  await setStudio(page, { ao: true });
  metrics({ frameMs: { standard: msStd, studio: msStudio, studioNoAo: msNoAo, frames: BENCH_FRAMES, renderer: "Chromium, SwiftShader (software WebGL), 1440x900 window" } });
  record({
    id: "S01",
    title: "Studio look on and off",
    caption: `The button “Studio-Darstellung” (studio look) in the view toolbar switches to key/fill/sky light, soft shadows on the model and the ground, ambient occlusion and a fading ground on a gradient background; ${(d * 100).toFixed(0)} % of the view pixels change. Frame time (software WebGL): standard ${msStd?.toFixed(1)} ms, studio ${msStudio?.toFixed(1)} ms, studio without AO ${msNoAo?.toFixed(1)} ms.`,
    images: ["s01-standard", "s01-studio"],
    checks: [`drawn: ${JSON.stringify(i.drawn)}`, `changed pixels ${(d * 100).toFixed(1)} %`],
  });
});

test("S02 settings panel: light, shadows, ambient occlusion, ground, background; kept after reload", async ({ page }) => {
  await openApp(page);
  await loadText(page, example("02-bracket.mcad"));
  await toggle(page).click();
  await studioReady(page);
  const base = await grab(page);
  await settingsButton(page).click();
  await expect(panel(page)).toBeVisible();
  await shot(page, "s02-panel", "full");
  // ambient occlusion off
  await panel(page).getByRole("checkbox", { name: "Umgebungsverdeckung" }).uncheck();
  let i = await studioReady(page, (x) => x.drawn?.ao === false);
  const noAo = await grab(page);
  const dAo = await differ(page, base, noAo);
  expect(dAo).toBeGreaterThan(0.005);
  // shadows off
  await panel(page).getByRole("checkbox", { name: "Schatten" }).uncheck();
  i = await studioReady(page, (x) => x.drawn?.shadows === false);
  const noShadow = await grab(page);
  const dSh = await differ(page, noAo, noShadow);
  expect(dSh).toBeGreaterThan(0.005);
  // light from the right, low
  await panel(page).getByRole("slider", { name: "Richtung" }).fill("60");
  await panel(page).getByRole("slider", { name: "Höhe" }).fill("25");
  i = await studioReady(page, (x) => x.settings.lights[0].azimuth === 60 && x.settings.lights[0].elevation === 25);
  expect(i.drawn!.light![2]).toBeCloseTo(Math.sin((25 * Math.PI) / 180), 3);
  await page.keyboard.press("Escape");
  // reload: the settings stay
  await page.reload();
  await expect(page.getByTestId("editor").locator(".cm-content")).toBeVisible();
  const after = await info(page);
  expect(after.settings).toMatchObject({ on: true, ao: false, shadows: false, lights: [{ azimuth: 60, elevation: 25 }] });
  // defaults again
  await settingsButton(page).click();
  await panel(page).getByRole("button", { name: "Standardwerte" }).click();
  expect((await info(page)).settings).toMatchObject({ on: true, ao: true, shadows: true, lights: [{ azimuth: -40, elevation: 50 }] });
  record({
    id: "S02",
    title: "Settings panel and persistence",
    caption: `Panel “Studio-Einstellungen”: switching off ambient occlusion changes ${(dAo * 100).toFixed(1)} % of the pixels, switching off shadows another ${(dSh * 100).toFixed(1)} %; light direction 60°/25° is used (light vector z = sin 25°); after a reload the settings are still there (localStorage \`cad-studio\`); “Standardwerte” restores the defaults.`,
    images: ["s02-panel"],
    checks: [`AO off: ${(dAo * 100).toFixed(1)} %`, `shadows off: ${(dSh * 100).toFixed(1)} %`, `after reload: ${JSON.stringify(after.settings)}`],
  });
});

test("S03 variants: light, softness, ground and background, dark theme", async ({ page }) => {
  await openApp(page);
  await loadText(page, example("07-mug.mcad"));
  await setStudio(page, { on: true });
  await studioReady(page);
  await shot(page, "s03-default");
  await setStudio(page, { lights: [{ name: "key", azimuth: 70, elevation: 30, intensity: 1, color: "#ffffff" }], shadowSoftness: 0.9 });
  await studioReady(page, (x) => x.settings.lights[0].azimuth === 70);
  await shot(page, "s03-light-right-soft");
  await setStudio(page, { lights: [{ name: "key", azimuth: -40, elevation: 50, intensity: 1, color: "#ffffff" }], shadowSoftness: 0.35, ground: false, background: ["#ffffff", "#9fb3c8"] });
  const g = await studioReady(page, (x) => x.drawn?.ground === false);
  expect(g.drawn!.ground).toBe(false);
  await shot(page, "s03-no-ground-blue");
  await setStudio(page, { ground: true, background: null });
  await setTheme(page, "dark");
  await frame(page);
  await frame(page);
  await shot(page, "s03-dark");
  record({
    id: "S03",
    title: "Variants",
    caption: "Mug: default settings; light from the right at 30° with soft shadows (0.9); ground off on a white–blue gradient; dark theme (ground and gradient from the theme).",
    images: ["s03-default", "s03-light-right-soft", "s03-no-ground-blue", "s03-dark"],
    checks: ["ground off → drawn.ground false"],
  });
});

test("S04 section and studio look together; other models", async ({ page }) => {
  await openApp(page);
  await loadText(page, example("08-macropad.mcad"));
  await setStudio(page, { on: true });
  await studioReady(page);
  await shot(page, "s04-macropad");
  await loadText(page, example("05-box-lid.mcad"));
  await mode(page, "Schnitt").click();
  const i = await studioReady(page, (x) => x.drawn?.on === true && x.drawn.ao === false);
  expect(i.drawn!.shadows).toBe(true);
  await settingsButton(page).click();
  await expect(panel(page).getByText("Die Umgebungsverdeckung pausiert, solange ein Schnitt gezeigt wird")).toBeVisible();
  await page.keyboard.press("Escape");
  await shot(page, "s04-section");
  await mode(page, "Modell").click();
  await loadText(page, example("04-hook.mcad"));
  await studioReady(page, (x) => x.drawn?.ao === true);
  await shot(page, "s04-hook");
  record({
    id: "S04",
    title: "Section and further models",
    caption: "Macropad (colours per part, keep-out volumes) in the studio look; the box with a section (caps, shadows; ambient occlusion pauses during a section, the panel says so); hook.",
    images: ["s04-macropad", "s04-section", "s04-hook"],
    checks: [`section: drawn ${JSON.stringify(i.drawn)}`],
  });
});

function cadBin(): string | null {
  for (const p of [process.env.MCAD_BIN, join(ROOT, "target/release/mcad"), join(ROOT, "target/debug/mcad")]) if (p && existsSync(p)) return p;
  return null;
}

test("S05 “Als mcad render kopieren” gives the studio call; the CLI renders the same look", async ({ page }) => {
  await openApp(page);
  const text = example("02-bracket.mcad");
  await loadText(page, text);
  await setStudio(page, { on: true, lights: [{ name: "key", azimuth: -30, elevation: 50, intensity: 1, color: "#ffffff" }], shadowSoftness: 0.5 });
  await studioReady(page);
  await settingsButton(page).click();
  await panel(page).getByRole("button", { name: "Als mcad render kopieren" }).click();
  const st = (await cad(page)) as CadState & { lastCli: string };
  const cli = st.lastCli;
  expect(cli).toMatch(/^mcad render \S+ -o studio\.png --perspective --azimuth [\d.]+ --elevation [\d.]+ --style studio --light -30,50 --shadow-softness 0\.5$/);
  await page.keyboard.press("Escape");
  await shot(page, "s05-web");
  const bin = cadBin();
  let ran = false;
  if (bin) {
    const dir = mkdtempSync(join(tmpdir(), "mcad-s05-"));
    const file = cli.split(" ")[2];
    writeFileSync(join(dir, file), text);
    const box = (await page.getByTestId("viewport-canvas").boundingBox())!;
    const r = spawnSync(bin, [...cli.split(" ").slice(1), "--size", `${Math.round(box.width)}x${Math.round(box.height)}`, "--no-axes"], { cwd: dir, encoding: "utf8", env: { ...process.env, MCAD_NO_DAEMON: "1" } });
    expect(r.status, r.stderr).toBe(0);
    mkdirSync(join(OUT, "img"), { recursive: true });
    writeFileSync(join(OUT, "img", "s05-cli.png"), readFileSync(join(dir, "studio.png")));
    ran = true;
  }
  record({
    id: "S05",
    title: "Same look in the CLI",
    caption: `“Als mcad render kopieren” gives \`${cli}\`${ran ? "; the call run with the CLI (size of the view, --no-axes) renders the same view in the CLI's software renderer" : " (no mcad binary: CLI run skipped)"}.`,
    images: ran ? ["s05-web", "s05-cli"] : ["s05-web"],
    checks: [cli],
  });
});

test.describe("English UI", () => {
  test.use({ locale: "en-US" });

  test("S06 English labels of the toggle and the settings panel", async ({ page }) => {
    await openApp(page);
    await loadText(page, example("08-macropad.mcad"));
    const tg = page.getByRole("button", { name: "Studio look", exact: true });
    await tg.click();
    await studioReady(page);
    await page.getByRole("button", { name: "Studio settings", exact: true }).click();
    const p = page.getByRole("dialog", { name: "Studio settings" });
    await expect(p).toBeVisible();
    for (const name of ["Shadows", "Ambient occlusion", "Ground"]) await expect(p.getByRole("checkbox", { name, exact: true })).toBeVisible();
    for (const name of ["Direction", "Height", "Ambient", "Softness"]) await expect(p.getByRole("slider", { name, exact: true })).toBeVisible();
    await expect(p.getByRole("button", { name: "Defaults", exact: true })).toBeVisible();
    await expect(p.getByRole("button", { name: "Copy as mcad render", exact: true })).toBeVisible();
    await shot(page, "s06-panel-en", "full");
    record({
      id: "S06",
      title: "English UI",
      caption: "Browser language English: “Studio look” and “Studio settings” in the view toolbar, the panel with Light (Direction, Height, Intensity, Ambient), Shadows (Softness), Ambient occlusion (Radius, Strength), Ground (colour, Theme), Background (top, bottom, Theme), Defaults and “Copy as mcad render”.",
      images: ["s06-panel-en"],
      checks: ["all labels found by role and English name"],
    });
  });
});
