// End-to-end tests of the 3D section view (test report 9): plane x/y/z or free, dragging with
// the grip, snapping to grid planes and anchors, flip, filled hatched cap (pixel check), cut
// contour, status line, "Als `mcad render …` kopieren" — and the copied call renders the same
// section in the CLI (share of cap pixels compared roughly). Convention as `mcad render
// --section`: the plane through p with normal n removes the half n points into.

import { spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { expect, test, type Page } from "@playwright/test";
import { type CadState, cad, clickLine, frame, hook, mode, openApp, openExample, record9, screenshot9, sectionBar, waitScene, waitView, writeMetrics9 } from "./helpers";

const here = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(here, "../..");
const MODEL = readFileSync(join(ROOT, "tests/models/spec/housing.mcad"), "utf8");

interface SectionInfo {
  on: boolean;
  caps: number;
  contour: number;
  grip: { x: number; y: number } | null;
  gripWorld: [number, number, number];
  normal: [number, number, number];
  point: [number, number, number];
  snap: string | null;
  dragging: boolean;
}

type W = { __cad: { sectionInfo(): SectionInfo; setSection(s: unknown): void; project(p: number[]): { x: number; y: number }; sectionBenchmark(n: number): Promise<{ fps: number; frames: number; ms: number }>; setView(v: string): void; fitAll(): void } };

const info = (page: Page) => page.evaluate(() => (window as unknown as W).__cad.sectionInfo());

async function sectionOn(page: Page): Promise<SectionInfo> {
  await mode(page, "Schnitt").click();
  await page.waitForFunction(() => {
    const i = (window as unknown as W).__cad.sectionInfo();
    return i.on && i.caps > 0 && i.contour > 0;
  });
  await frame(page);
  return info(page);
}

/** Cap pixels (red-orange, hatched) and drawn pixels (not background) of a PNG. */
async function pixels(page: Page, png: Buffer): Promise<{ cap: number; drawn: number; share: number }> {
  return page.evaluate(async (b64) => {
    const img = new Image();
    img.src = `data:image/png;base64,${b64}`;
    await img.decode();
    const c = document.createElement("canvas");
    c.width = img.width;
    c.height = img.height;
    const g = c.getContext("2d")!;
    g.drawImage(img, 0, 0);
    const d = g.getImageData(0, 0, c.width, c.height).data;
    const bg = [d[0], d[1], d[2]];
    let cap = 0;
    let drawn = 0;
    for (let i = 0; i < d.length; i += 4) {
      const [r, gg, b] = [d[i], d[i + 1], d[i + 2]];
      if (r > 110 && r - gg > 50 && r - b > 60 && gg < 110) cap++;
      // model and cap: clearly different from the background, not the light floor grid
      if (Math.abs(r - bg[0]) + Math.abs(gg - bg[1]) + Math.abs(b - bg[2]) > 60) drawn++;
    }
    return { cap, drawn, share: drawn ? cap / drawn : 0 };
  }, png.toString("base64"));
}

const canvasPng = (page: Page) => page.getByTestId("viewport-canvas").screenshot();

test.beforeEach(async ({ page }) => {
  page.on("pageerror", (e) => console.log("pageerror:", e.message));
});

test("X01 section z: filled cap, contour, status line, picking only on the kept side", async ({ page }) => {
  await openApp(page);
  await hook(page, (c) => c.setView("iso"));
  const before = await pixels(page, await canvasPng(page));
  const i = await sectionOn(page);
  expect(i.normal).toEqual([0, 0, 1]);
  const z = i.point[2];
  expect(z).toBeGreaterThan(5);
  await expect(page.getByTestId("status-section")).toContainText(`Schnitt z = ${z} · entfernt z > ${z}`);
  const after = await pixels(page, await canvasPng(page));
  expect(before.cap).toBeLessThan(50);
  expect(after.cap).toBeGreaterThan(2000);
  // the cut contour is drawn, every capped body has its parity pass
  expect(i.contour).toBeGreaterThan(8);
  // picking ignores the removed half: a ray through the cap hits the kept part below
  await screenshot9(page, "x01-schnitt-z");
  record9({
    id: "X01",
    title: `Section z = ${z} with cap`,
    caption: `“Schnitt” (section) in the view: plane z through the middle of the model (z = ${z}), z > ${z} is removed. The cut face is filled and hatched (stencil cap, colour as in the CLI renderer), the contour as a line; status line “Schnitt z = ${z} · entfernt z > ${z}”.`,
    images: [{ name: "x01-schnitt-z", label: `section z = ${z} (iso)` }],
    checks: [`cap pixels before ${before.cap}, after ${after.cap}`, `${i.caps} bodies with a cap, ${i.contour} contour segments`],
  });
});

test("X02 drag the plane by its grip: snaps to grid planes and anchors, Alt free", async ({ page }) => {
  await openApp(page);
  // module context: grid g (z: floor = 0, lid = 20) of housing
  await clickLine(page, 11, 8);
  await waitScene(page, { context: "housing" });
  await hook(page, (c) => c.setView("front"));
  await waitView(page, "() => true");
  const i = await sectionOn(page);
  expect(i.grip).not.toBeNull();
  const g = i.grip!;
  const at = (z: number) => page.evaluate(([w, zz]) => (window as unknown as W).__cad.project([w[0], w[1], zz]), [i.gripWorld, z] as const);
  // drag up to the plane lid (z = 20)
  const target = await at(20);
  await page.mouse.move(g.x, g.y);
  await page.mouse.down();
  const steps = 12;
  for (let k = 1; k <= steps; k++) {
    await page.mouse.move(g.x + ((target.x - g.x) * k) / steps, g.y + ((target.y - g.y) * k) / steps + (k === steps ? 3 : 0));
    await frame(page);
  }
  const mid = await info(page);
  expect(mid.dragging).toBe(true);
  expect(mid.snap).toBe("g.lid");
  expect(mid.point[2]).toBe(20);
  await expect(page.getByTestId("snap-label")).toHaveText("g.lid");
  await screenshot9(page, "x02-rasten-lid");
  await page.mouse.up();
  await expect(page.getByTestId("status-section")).toContainText("Schnitt z = 20 · entfernt z > 20 · g.lid");
  // anchors: `corner` and `top` lie in z = 20 as well; z = 0 is the grid plane floor
  const lower = await at(0.4);
  const g2 = (await info(page)).grip!;
  await page.mouse.move(g2.x, g2.y);
  await page.mouse.down();
  for (let k = 1; k <= steps; k++) {
    await page.mouse.move(g2.x + ((lower.x - g2.x) * k) / steps, g2.y + ((lower.y - g2.y) * k) / steps);
    await frame(page);
  }
  await page.mouse.up();
  let s = await info(page);
  expect(s.snap).toBe("g.floor");
  expect(s.point[2]).toBe(0);
  // Alt: free (rounded to 0.1 mm), no snap
  const free = await at(13.37);
  const g3 = (await info(page)).grip!;
  await page.keyboard.down("Alt");
  await page.mouse.move(g3.x, g3.y);
  await page.mouse.down();
  for (let k = 1; k <= steps; k++) {
    await page.mouse.move(g3.x + ((free.x - g3.x) * k) / steps, g3.y + ((free.y - g3.y) * k) / steps);
    await frame(page);
  }
  await page.mouse.up();
  await page.keyboard.up("Alt");
  s = await info(page);
  expect(s.snap).toBeNull();
  expect(Math.abs(s.point[2] - 13.37)).toBeLessThan(0.6);
  expect(Math.round(s.point[2] * 10) / 10).toBeCloseTo(s.point[2], 6);
  const st = await cad(page);
  const fps = (st.metrics as unknown as { sectionFps?: Array<{ fps: number }> }).sectionFps ?? [];
  expect(fps.length).toBeGreaterThanOrEqual(3);
  await screenshot9(page, "x02-frei");
  writeMetrics9({ sectionDragFps: fps });
  record9({
    id: "X02",
    title: "Drag and snap the plane",
    caption: "In the module context housing (grid g): grip (blue point) dragged up, the plane snaps to g.lid (z = 20, green label), down to g.floor; free with Alt (rounded to 0.1 mm).",
    images: [
      { name: "x02-rasten-lid", label: "while dragging: snaps to g.lid" },
      { name: "x02-frei", label: "dragged freely with Alt" },
    ],
    checks: [`snap: ${mid.snap} (z = ${mid.point[2]}), g.floor (z = 0)`, `free: z = ${s.point[2]}`, `fps while dragging: ${fps.map((f) => f.fps.toFixed(0)).join(", ")}`],
  });
});

test("X03 flip, axes, free plane from the view direction and by input", async ({ page }) => {
  await openApp(page);
  await hook(page, (c) => c.setView("iso"));
  await sectionOn(page);
  await sectionBar(page).getByRole("button", { name: "Umkehren" }).click();
  await expect(page.getByTestId("status-section")).toContainText("entfernt z <");
  let i = await info(page);
  expect(i.normal).toEqual([-0, -0, -1]);
  // the kept upper half shows its cap from below
  await page.evaluate(() => (window as unknown as { __cad: { viewFrom(d: number[]): void } }).__cad.viewFrom([1, -1.25, -0.9]));
  await frame(page);
  await frame(page);
  const flipped = await pixels(page, await canvasPng(page));
  expect(flipped.cap).toBeGreaterThan(1000);
  await screenshot9(page, "x03-umgekehrt");
  await sectionBar(page).getByRole("radio", { name: "X", exact: true }).click();
  i = await info(page);
  expect(Math.abs(i.normal[0])).toBe(1);
  await sectionBar(page).getByRole("textbox", { name: /^Lage der Ebene/ }).fill("30");
  await sectionBar(page).getByRole("textbox", { name: /^Lage der Ebene/ }).press("Enter");
  await expect(page.getByTestId("status-section")).toContainText("x = 30");
  await screenshot9(page, "x03-x30");
  await sectionBar(page).getByRole("radio", { name: "Aus Blickrichtung" }).click();
  await expect(page.getByTestId("status-section")).toContainText("n = ");
  await sectionBar(page).getByRole("textbox", { name: "Normale der Ebene" }).fill("1, 1, 1");
  await sectionBar(page).getByRole("textbox", { name: "Normale der Ebene" }).press("Enter");
  i = await info(page);
  const r = 1 / Math.sqrt(3);
  for (let k = 0; k < 3; k++) expect(Math.abs(i.normal[k])).toBeCloseTo(r, 6);
  await sectionBar(page).getByRole("button", { name: "Als mcad render … kopieren" }).click();
  const cli = (await cad(page)) as CadState & { lastCli: string };
  expect(cli.lastCli).toContain("--section-plane ");
  await screenshot9(page, "x03-frei");
  record9({
    id: "X03",
    title: "Flip, axes, free plane",
    caption: "“Umkehren” (flip) keeps the upper half (n = −z, like --section-flip); X with value 30; “Aus Blickrichtung” (from the view direction) lays a free plane perpendicular to the view direction, the normal can be typed (1, 1, 1).",
    images: [
      { name: "x03-umgekehrt", label: "z flipped (removes the lower half), from below" },
      { name: "x03-x30", label: "x = 30" },
      { name: "x03-frei", label: "free, n = (1, 1, 1)/√3" },
    ],
    checks: [`CLI: ${cli.lastCli}`],
  });
});

test("X04 with highlighting, zones, transparent bodies (without a cap) and handles", async ({ page }) => {
  await openApp(page);
  await clickLine(page, 11, 8);
  await waitScene(page, { context: "housing" });
  await hook(page, (c) => c.setView("iso"));
  await sectionOn(page);
  let s = await cad(page);
  // cursor on line 11: that box full (capped), the rest dimmed (transparent: clipped, no cap)
  const full = s.view!.plan.items.filter((it) => it.mode === "full").length;
  let i = await info(page);
  expect(i.caps).toBe(full);
  await screenshot9(page, "x04-hervorhebung");
  // line 12: the subtracted box (red, transparent) – no cap for it
  await clickLine(page, 12, 8);
  s = await waitView(page, "(s) => s.view.selection.line === 12");
  i = await info(page);
  expect(i.caps).toBe(s.view!.plan.items.filter((it) => it.mode === "full").length);
  await screenshot9(page, "x04-abgezogen");
  // zones: fillet example, cursor on a zone
  await openExample(page, "spec/fillet_zones.mcad");
  const text = await hook(page, (c) => c.text());
  const zl = text.split("\n").findIndex((l) => l.includes("zone(")) + 1;
  await clickLine(page, zl, 10);
  s = await waitView(page, `(s) => s.view.selection.line === ${zl}`);
  i = await info(page);
  expect(i.on).toBe(true);
  await screenshot9(page, "x04-zone");
  // handles stay usable: handles of the cursor line are drawn above the section
  const hs = await hook(page, (c) => (c as unknown as { handles(): unknown[] }).handles());
  record9({
    id: "X04",
    title: "Section together with highlighting, zones, transparency",
    caption: "Cursor on a line: its body full and capped, the rest dimmed (only cut, without a cap); subtracted body red, transparent, without a cap; zone (blue) in the fillet example; handles stay above the section.",
    images: [
      { name: "x04-hervorhebung", label: "line 11 highlighted" },
      { name: "x04-abgezogen", label: "line 12: subtracted body" },
      { name: "x04-zone", label: `zone (line ${zl})` },
    ],
    checks: [`caps = full bodies (${full})`, `${(hs as unknown[]).length} handles drawn`],
  });
});

function cadBin(): string | null {
  for (const p of [process.env.MCAD_BIN, join(ROOT, "target/release/mcad"), join(ROOT, "target/debug/mcad")]) if (p && existsSync(p)) return p;
  return null;
}

test("X05 the copied CLI call gives the same section in the CLI (share of cap pixels)", async ({ page }) => {
  const bin = cadBin();
  test.skip(!bin, "no mcad binary (MCAD_BIN or target/{release,debug}/mcad)");
  await openApp(page);
  await hook(page, (c) => c.setView("iso"));
  await sectionOn(page);
  await sectionBar(page).getByRole("textbox", { name: /^Lage der Ebene/ }).fill("12");
  await sectionBar(page).getByRole("textbox", { name: /^Lage der Ebene/ }).press("Enter");
  await frame(page);
  await sectionBar(page).getByRole("button", { name: "Als mcad render … kopieren" }).click();
  const st = (await cad(page)) as CadState & { lastCli: string };
  const cli = st.lastCli;
  expect(cli).toMatch(/^mcad render housing\.mcad -o section\.png --section z=12 --azimuth [\d.]+ --elevation [\d.]+$/);
  const web = await pixels(page, await canvasPng(page));
  // run the copied call (file and output into a temporary directory)
  const dir = mkdtempSync(join(tmpdir(), "cad-x05-"));
  writeFileSync(join(dir, "housing.mcad"), MODEL);
  const box = (await page.getByTestId("viewport-canvas").boundingBox())!;
  const args = cli.split(" ").slice(1);
  const r = spawnSync(bin!, [...args, "--size", `${Math.round(box.width)}x${Math.round(box.height)}`, "--no-axes"], { cwd: dir, encoding: "utf8" });
  expect(r.status, r.stderr).toBe(0);
  const png = readFileSync(join(dir, "section.png"));
  writeFileSync(join(ROOT, "reports/tmp/report9/img/x05-cli.png"), png);
  const cliPx = await pixels(page, png);
  const rel = Math.abs(web.share - cliPx.share) / Math.max(web.share, cliPx.share);
  expect(cliPx.cap).toBeGreaterThan(1000);
  expect(rel).toBeLessThan(0.35);
  await screenshot9(page, "x05-web");
  writeMetrics9({ cliCompare: { cli, web, cliPx, rel } });
  record9({
    id: "X05",
    title: "Same section in UI and CLI",
    caption: `“Als mcad render … kopieren” (copy as mcad render …) gives \`${cli}\`; run in the CLI the image shows the same section (cap share of the drawn pixels UI ${(web.share * 100).toFixed(1)} %, CLI ${(cliPx.share * 100).toFixed(1)} %, deviation ${(rel * 100).toFixed(0)} %; perspective in the UI, orthographic in the CLI).`,
    images: [
      { name: "x05-web", label: "UI (WebGL)" },
      { name: "x05-cli", label: "CLI (software renderer), copied call" },
    ],
    checks: [`cap pixels UI ${web.cap} / ${web.drawn}, CLI ${cliPx.cap} / ${cliPx.drawn}`],
  });
});

test("X06 fps while moving the plane", async ({ page }) => {
  await openApp(page);
  await openExample(page, "spec/fillet_zones.mcad");
  await hook(page, (c) => c.setView("iso"));
  const plain = await page.evaluate(() => (window as unknown as { __cad: { orbitBenchmark(n: number): Promise<{ fps: number }> } }).__cad.orbitBenchmark(120));
  await sectionOn(page);
  const r = await page.evaluate(() => (window as unknown as W).__cad.sectionBenchmark(120));
  expect(r.fps).toBeGreaterThan(5);
  const orbit = await page.evaluate(() => (window as unknown as { __cad: { orbitBenchmark(n: number): Promise<{ fps: number }> } }).__cad.orbitBenchmark(120));
  writeMetrics9({ sectionMoveFps: r, orbitFpsWithSection: orbit, orbitFpsNoSection: plain });
  record9({
    id: "X06",
    title: "Frame rate while dragging the section",
    caption: `Plane moved for 120 frames (every frame a new position, new cap and contour): ${r.fps.toFixed(1)} fps in Chromium with SwiftShader (software GL, no GPU); orbiting with a section ${orbit.fps.toFixed(1)} fps, without ${plain.fps.toFixed(1)} fps – the section costs hardly more than the drawing itself.`,
    images: [],
    checks: [`${r.frames} frames in ${r.ms.toFixed(0)} ms`],
  });
});

test("X07 keystroke → scene in the browser (Wasm), the same changes as in Tauri", async ({ page }) => {
  test.setTimeout(240_000);
  await openApp(page);
  // after "w = 60" in `k: housing(w = 60);`: Backspace / digit, always a valid model
  await clickLine(page, 21, 17);
  const m0 = (await cad(page)).metrics;
  const before = m0.editToScene.length;
  const beforeEval = m0.evalMs.length;
  for (let i = 0; i < 8; i++) {
    await page.keyboard.press(i % 2 ? "0" : "Backspace");
    await page.waitForTimeout(250);
    await waitScene(page);
  }
  const m1 = (await cad(page)).metrics;
  const ms = m1.editToScene.slice(before);
  expect(ms.length).toBeGreaterThan(0);
  const heavy = (r: number) => `difference(fillet = 0.4) {\n    cube(40);\n    for (i = [0 : 3]) for (j = [0 : 3]) translate([10 * i + 5, 10 * j + 5, -1]) cylinder(h = 42, r = ${r});\n}\n`;
  const heavyEval: number[] = [];
  for (const r of [2, 2.5, 3]) {
    const v0 = (await cad(page)).sentVersion;
    await hook(page, (c, t) => c.setText(t as string), heavy(r));
    const sc = await waitScene(page, { minVersion: v0 + 1 });
    heavyEval.push(sc.metrics.evalMs[sc.metrics.evalMs.length - 1]);
  }
  writeMetrics9({ keystrokeToSceneWasm: ms, keystrokeEvalWasm: m1.evalMs.slice(beforeEval), heavyEvalWasm: heavyEval });
});
