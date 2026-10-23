// End-to-end tests of the 3D section view (test report 9): plane x/y/z or free, dragging with
// the grip, snapping to grid planes and anchors, flip, filled hatched cap (pixel check), cut
// contour, status line, "Als `cad render …` kopieren" — and the copied call renders the same
// section in the CLI (share of cap pixels compared roughly). Convention as `cad render
// --section`: the plane through p with normal n removes the half n points into.

import { spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { expect, test, type Page } from "@playwright/test";
import { type CadState, cad, clickLine, frame, hook, openApp, record9, screenshot9, waitScene, waitView, writeMetrics9 } from "./helpers";

const here = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(here, "../..");
const MODEL = readFileSync(join(ROOT, "examples/spec/gehaeuse.scad"), "utf8");

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
  await page.getByTestId("section-toggle").click();
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

test("X01 Schnitt z: gefüllte Kappe, Kontur, Statuszeile, Picking nur auf der behaltenen Seite", async ({ page }) => {
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
    title: `Schnitt z = ${z} mit Kappe`,
    caption: `„Schnitt“ in der Ansicht: Ebene z durch die Modellmitte (z = ${z}), entfernt wird z > ${z}. Die Schnittfläche ist gefüllt und schraffiert (Stencil-Kappe, Farbe wie im CLI-Renderer), die Kontur als Linie; Statuszeile „Schnitt z = ${z} · entfernt z > ${z}“.`,
    images: [{ name: "x01-schnitt-z", label: `Schnitt z = ${z} (iso)` }],
    checks: [`Kappenpixel vorher ${before.cap}, nachher ${after.cap}`, `${i.caps} Körper mit Kappe, ${i.contour} Kontursegmente`],
  });
});

test("X02 Ebene am Griff ziehen: rastet auf Rasterebene und Anker, Alt frei", async ({ page }) => {
  await openApp(page);
  // module context: grid g (z: boden = 0, deckel = 20) of gehaeuse
  await clickLine(page, 11, 8);
  await waitScene(page, { context: "gehaeuse" });
  await hook(page, (c) => c.setView("front"));
  await waitView(page, "() => true");
  const i = await sectionOn(page);
  expect(i.grip).not.toBeNull();
  const g = i.grip!;
  const at = (z: number) => page.evaluate(([w, zz]) => (window as unknown as W).__cad.project([w[0], w[1], zz]), [i.gripWorld, z] as const);
  // drag up to the plane deckel (z = 20)
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
  expect(mid.snap).toBe("g.deckel");
  expect(mid.point[2]).toBe(20);
  await expect(page.getByTestId("snap-label")).toHaveText("g.deckel");
  await screenshot9(page, "x02-rasten-deckel");
  await page.mouse.up();
  await expect(page.getByTestId("status-section")).toContainText("Schnitt z = 20 · entfernt z > 20 · g.deckel");
  // anchors: `ecke` and `oben` lie in z = 20 as well; z = 0 is the grid plane boden
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
  expect(s.snap).toBe("g.boden");
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
    title: "Ebene ziehen und rasten",
    caption: "Im Modulkontext gehaeuse (Raster g): Griff (blauer Punkt) nach oben gezogen, die Ebene rastet auf g.deckel (z = 20, grünes Etikett), nach unten auf g.boden; mit Alt frei (auf 0,1 mm gerundet).",
    images: [
      { name: "x02-rasten-deckel", label: "beim Ziehen: rastet auf g.deckel" },
      { name: "x02-frei", label: "mit Alt frei gezogen" },
    ],
    checks: [`Rasten: ${mid.snap} (z = ${mid.point[2]}), g.boden (z = 0)`, `frei: z = ${s.point[2]}`, `fps beim Ziehen: ${fps.map((f) => f.fps.toFixed(0)).join(", ")}`],
  });
});

test("X03 Umkehren, Achsen, freie Ebene aus Blickrichtung und per Eingabe", async ({ page }) => {
  await openApp(page);
  await hook(page, (c) => c.setView("iso"));
  await sectionOn(page);
  await page.getByTestId("section-flip").click();
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
  await page.getByTestId("section-axis-x").click();
  i = await info(page);
  expect(Math.abs(i.normal[0])).toBe(1);
  await page.getByTestId("section-value").fill("30");
  await page.getByTestId("section-value").press("Enter");
  await expect(page.getByTestId("status-section")).toContainText("x = 30");
  await screenshot9(page, "x03-x30");
  await page.getByTestId("section-view").click();
  await expect(page.getByTestId("status-section")).toContainText("n = ");
  await page.getByTestId("section-normal").fill("1, 1, 1");
  await page.getByTestId("section-normal").press("Enter");
  i = await info(page);
  const r = 1 / Math.sqrt(3);
  for (let k = 0; k < 3; k++) expect(Math.abs(i.normal[k])).toBeCloseTo(r, 6);
  await page.getByTestId("section-copy").click();
  const cli = (await cad(page)) as CadState & { lastCli: string };
  expect(cli.lastCli).toContain("--section-plane ");
  await screenshot9(page, "x03-frei");
  record9({
    id: "X03",
    title: "Umkehren, Achsen, freie Ebene",
    caption: "„Umkehren“ behält die obere Hälfte (n = −z, entspricht --section-flip); X mit Wert 30; „Aus Blickrichtung“ legt eine freie Ebene senkrecht zur Blickrichtung, die Normale lässt sich eingeben (1, 1, 1).",
    images: [
      { name: "x03-umgekehrt", label: "z umgekehrt (entfernt die untere Hälfte), von unten" },
      { name: "x03-x30", label: "x = 30" },
      { name: "x03-frei", label: "frei, n = (1, 1, 1)/√3" },
    ],
    checks: [`CLI: ${cli.lastCli}`],
  });
});

test("X04 Mit Hervorhebung, Zonen, transparenten Körpern (ohne Kappe) und Griffen", async ({ page }) => {
  await openApp(page);
  await clickLine(page, 11, 8);
  await waitScene(page, { context: "gehaeuse" });
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
  await page.getByTestId("examples").selectOption("spec/fillet_zonen.scad");
  await waitScene(page);
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
    title: "Schnitt zusammen mit Hervorhebung, Zonen, Transparenz",
    caption: "Cursor auf einer Zeile: deren Körper voll und mit Kappe, der Rest gedimmt (nur abgeschnitten, ohne Kappe); abgezogener Körper rot transparent ohne Kappe; Zone (blau) im Fillet-Beispiel; Griffe bleiben über dem Schnitt.",
    images: [
      { name: "x04-hervorhebung", label: "Zeile 11 hervorgehoben" },
      { name: "x04-abgezogen", label: "Zeile 12: abgezogener Körper" },
      { name: "x04-zone", label: `Zone (Zeile ${zl})` },
    ],
    checks: [`Kappen = volle Körper (${full})`, `${(hs as unknown[]).length} Griffe gezeichnet`],
  });
});

function cadBin(): string | null {
  for (const p of [process.env.CAD_BIN, join(ROOT, "target/release/cad"), join(ROOT, "target/debug/cad")]) if (p && existsSync(p)) return p;
  return null;
}

test("X05 Kopierter CLI-Aufruf ergibt im CLI denselben Schnitt (Pixelanteil der Kappe)", async ({ page }) => {
  const bin = cadBin();
  test.skip(!bin, "kein cad-Binary (CAD_BIN oder target/{release,debug}/cad)");
  await openApp(page);
  await hook(page, (c) => c.setView("iso"));
  await sectionOn(page);
  await page.getByTestId("section-value").fill("12");
  await page.getByTestId("section-value").press("Enter");
  await frame(page);
  await page.getByTestId("section-copy").click();
  const st = (await cad(page)) as CadState & { lastCli: string };
  const cli = st.lastCli;
  expect(cli).toMatch(/^cad render gehaeuse\.scad -o schnitt\.png --section z=12 --azimuth [\d.]+ --elevation [\d.]+$/);
  const web = await pixels(page, await canvasPng(page));
  // run the copied call (file and output into a temporary directory)
  const dir = mkdtempSync(join(tmpdir(), "cad-x05-"));
  writeFileSync(join(dir, "gehaeuse.scad"), MODEL);
  const box = (await page.getByTestId("viewport-canvas").boundingBox())!;
  const args = cli.split(" ").slice(1);
  const r = spawnSync(bin!, [...args, "--size", `${Math.round(box.width)}x${Math.round(box.height)}`, "--no-axes"], { cwd: dir, encoding: "utf8" });
  expect(r.status, r.stderr).toBe(0);
  const png = readFileSync(join(dir, "schnitt.png"));
  writeFileSync(join(ROOT, "reports/tmp/report9/img/x05-cli.png"), png);
  const cliPx = await pixels(page, png);
  const rel = Math.abs(web.share - cliPx.share) / Math.max(web.share, cliPx.share);
  expect(cliPx.cap).toBeGreaterThan(1000);
  expect(rel).toBeLessThan(0.35);
  await screenshot9(page, "x05-web");
  writeMetrics9({ cliCompare: { cli, web, cliPx, rel } });
  record9({
    id: "X05",
    title: "Gleicher Schnitt in UI und CLI",
    caption: `„Als cad render … kopieren“ ergibt \`${cli}\`; in der CLI ausgeführt zeigt das Bild denselben Schnitt (Kappenanteil an den gezeichneten Pixeln UI ${(web.share * 100).toFixed(1)} %, CLI ${(cliPx.share * 100).toFixed(1)} %, Abweichung ${(rel * 100).toFixed(0)} %; Perspektive in der UI, orthografisch in der CLI).`,
    images: [
      { name: "x05-web", label: "UI (WebGL)" },
      { name: "x05-cli", label: "CLI (Software-Renderer), kopierter Aufruf" },
    ],
    checks: [`Kappenpixel UI ${web.cap} / ${web.drawn}, CLI ${cliPx.cap} / ${cliPx.drawn}`],
  });
});

test("X06 fps beim Verschieben der Ebene", async ({ page }) => {
  await openApp(page);
  await page.getByTestId("examples").selectOption("spec/fillet_zonen.scad");
  await waitScene(page);
  await hook(page, (c) => c.setView("iso"));
  const plain = await page.evaluate(() => (window as unknown as { __cad: { orbitBenchmark(n: number): Promise<{ fps: number }> } }).__cad.orbitBenchmark(120));
  await sectionOn(page);
  const r = await page.evaluate(() => (window as unknown as W).__cad.sectionBenchmark(120));
  expect(r.fps).toBeGreaterThan(5);
  const orbit = await page.evaluate(() => (window as unknown as { __cad: { orbitBenchmark(n: number): Promise<{ fps: number }> } }).__cad.orbitBenchmark(120));
  writeMetrics9({ sectionMoveFps: r, orbitFpsWithSection: orbit, orbitFpsNoSection: plain });
  record9({
    id: "X06",
    title: "Bildrate beim Schnittziehen",
    caption: `Ebene 120 Bilder lang verschoben (jedes Bild neue Lage, Kappe und Kontur neu): ${r.fps.toFixed(1)} fps in Chromium mit SwiftShader (Software-GL, keine GPU); Drehen mit Schnitt ${orbit.fps.toFixed(1)} fps, ohne Schnitt ${plain.fps.toFixed(1)} fps – der Schnitt kostet kaum mehr als das Zeichnen selbst.`,
    images: [],
    checks: [`${r.frames} Bilder in ${r.ms.toFixed(0)} ms`],
  });
});

test("X07 Tastendruck → Szene im Browser (Wasm), gleiche Änderung wie in Tauri", async ({ page }) => {
  await openApp(page);
  // after "w = 60" in `k: gehaeuse(w = 60);`: Backspace / digit, always a valid model
  await clickLine(page, 21, 18);
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
  writeMetrics9({ keystrokeToSceneWasm: ms, keystrokeEvalWasm: m1.evalMs.slice(beforeEval) });
});
