// End-to-end tests of the browser UI, Phase B (handles, snapping, parameter bar) in Chromium
// against the production build. Real mouse events on the canvas; waiting is done on scene
// versions, drag state and editor text (window.__cad), never with fixed sleeps. Every scenario
// records screenshots before/after and the code excerpt for test report 3.

import { expect, test, type Page } from "@playwright/test";
import {
  type HandleView,
  type Pt,
  cad,
  clickLine,
  frame,
  hook,
  line,
  lines,
  loadText,
  mouseDrag,
  openApp,
  openExample,
  recordB,
  screenshotB,
  waitDragDone,
  waitHandles,
  waitScene,
  writeMetricsB,
} from "./helpers";

test.beforeEach(async ({ page }) => {
  page.on("pageerror", (e) => console.log("pageerror:", e.message));
});

const text = (page: Page) => hook(page, (c) => c.text());
const project = async (page: Page, p: [number, number, number]): Promise<Pt> => {
  const s = await hook(page, (c, q) => c.project(q as [number, number, number]), p);
  if (!s || !s.visible) throw new Error(`point ${p} not visible`);
  return { x: s.x, y: s.y };
};
const grip = (h: HandleView): Pt => {
  if (!h.screen) throw new Error(`handle ${h.id} not visible`);
  return h.screen;
};
const handlePoint = async (page: Page, id: number, v: number): Promise<Pt> => {
  const p = await hook(page, (c, a) => c.handlePoint((a as number[])[0], (a as number[])[1]), [id, v]);
  if (!p) throw new Error("no handle point");
  return p;
};
const waitText = (page: Page, pred: string) =>
  page.waitForFunction((src) => new Function("t", `return (${src})(t);`)((window as unknown as { __cad: { text(): string } }).__cad.text()), pred, { timeout: 30_000, polling: 30 });

const GRID = `grid g {
    x: links = 0, rechts = 40;
    y: vorne = 0, hinten = 30;
    z: boden = 0, deckel = 20;
}
cube([40, 30, 2]);
translate([5, 5, 2]) cube(4);
`;

test("B01 cube-Ecke ziehen: Literal live, Szene folgt, ein Undo", async ({ page }) => {
  await openApp(page);
  const src = "cube([20, 20, 20]);\n";
  await loadText(page, src);
  await clickLine(page, 1, 0);
  const [h] = await waitHandles(page);
  expect(h.kind).toBe("point");
  expect(h.style).toBe("literal");
  await screenshotB(page, "b01-vorher");
  const v0 = (await cad(page)).shownVersion!;
  const b0 = (await hook(page, (c) => c.rootBounds()))!;
  const from = grip(h);
  await mouseDrag(page, from, line(from, { x: from.x + 70, y: from.y - 40 }, 14), {
    mid: async () => {
      // Live: the literal changed and a newer scene is drawn while the button is still down.
      await waitText(page, `(t) => t !== ${JSON.stringify(src)}`);
      await page.waitForFunction((v) => (window as unknown as { __cad: { state(): { shownVersion: number } } }).__cad.state().shownVersion > v, v0);
      const d = await hook(page, (c) => c.drag());
      expect(d).not.toBeNull();
      expect(d!.edits).toBeGreaterThan(0);
      await screenshotB(page, "b01-waehrend");
    },
  });
  await waitDragDone(page);
  const after = await text(page);
  expect(after).toMatch(/^cube\(\[\d+, \d+, \d+\]\);\n$/);
  expect(after).not.toBe(src);
  const nums = after.match(/\d+/g)!.map(Number);
  const b1 = (await hook(page, (c) => c.rootBounds()))!;
  expect(b1.max.map((x) => Math.round(x))).toEqual(nums);
  expect(b1.max).not.toEqual(b0.max);
  await screenshotB(page, "b01-nachher");
  // Measurements of a light model (UI and worker overhead without much kernel work).
  await page.waitForFunction(() => (window as unknown as { __cad: { state(): { metrics: { dragEndToFine: number[] } } } }).__cad.state().metrics.dragEndToFine.length > 0);
  const mm = (await cad(page)).metrics;
  const lat = mm.dragToScene.filter((x) => x.coarse).map((x) => x.ms).sort((x, y) => x - y);
  const fps0 = mm.dragFps.at(-1)!;
  writeMetricsB({
    dragCube: {
      model: "cube([20, 20, 20])",
      coarse: { n: lat.length, median: lat[Math.floor(lat.length / 2)], p95: lat[Math.min(lat.length - 1, Math.floor(lat.length * 0.95))], max: lat.at(-1) },
      fine: { last: mm.dragEndToFine.at(-1) },
      fps: fps0.fps,
      maxGapMs: fps0.maxGapMs,
      sent: fps0.sent,
      dropped: fps0.dropped,
    },
  });
  // One undo step restores the text from before the drag.
  await hook(page, (c) => c.focusEditor());
  await page.keyboard.press("Control+z");
  expect(await text(page)).toBe(src);
  const s = await waitScene(page);
  expect((await hook(page, (c) => c.rootBounds()))!.max).toEqual([20, 20, 20]);
  // Redo (Ctrl+Y: the test browser reports Windows, where CodeMirror binds redo to Mod-y).
  await page.keyboard.press("Control+y");
  expect(await text(page)).toBe(after);
  recordB({
    id: "B01",
    title: "cube-Ecke ziehen",
    before: "b01-vorher",
    after: "b01-nachher",
    codeBefore: src.trim(),
    codeAfter: after.trim(),
    caption: `Griff an der Ecke gegenüber dem Ursprung mit der Maus gezogen: das Literal änderte sich live (${s.metrics.dragToScene.length} Zwischenstände mit grober Tessellierung), die Szene folgte. Ein Strg+Z stellte „${src.trim()}“ wieder her, Strg+Y (Wiederholen) wieder „${after.trim()}“.`,
  });
});

test("B02 translate-Griff auf Rasterpunkt loslassen schreibt g[…]", async ({ page }) => {
  await openApp(page);
  await loadText(page, GRID);
  await clickLine(page, 7, 0);
  // the handles of line 7 (not the grid's of line 1 from before the cursor moved)
  const [h] = await waitHandles(page, "(hs) => hs.length > 0 && hs.every((h) => h.line === 7)");
  expect(h.value).toEqual([5, 5, 2]);
  await screenshotB(page, "b02-vorher");
  const from = grip(h);
  const to = await project(page, [40, 30, 20]);
  await mouseDrag(page, from, line(from, to, 16), {
    mid: async () => {
      await page.waitForFunction(() => (window as unknown as { __cad: { drag(): { snap: string | null } | null } }).__cad.drag()?.snap === "g[rechts, hinten, deckel]");
      await expect(page.getByTestId("snap-label")).toBeVisible();
      await expect(page.getByTestId("snap-label")).toHaveText("g[rechts, hinten, deckel]");
      await expect(page.getByTestId("status-drag")).toContainText("Snap: g[rechts, hinten, deckel]");
      await waitText(page, `(t) => t.includes("translate(g[rechts, hinten, deckel])")`);
      await screenshotB(page, "b02-waehrend");
    },
  });
  await waitDragDone(page);
  const after = await text(page);
  expect(lines(after, 7)).toBe("translate(g[rechts, hinten, deckel]) cube(4);");
  // The cube now sits at the grid point.
  const nb = await hook(page, (c) => c.nodeBounds(7));
  expect(nb.some((b) => Math.abs(b.min[0] - 40) < 1e-6 && Math.abs(b.min[1] - 30) < 1e-6 && Math.abs(b.min[2] - 20) < 1e-6)).toBe(true);
  await screenshotB(page, "b02-nachher");
  recordB({
    id: "B02",
    title: "Snap auf Rasterpunkt",
    before: "b02-vorher",
    after: "b02-waehrend",
    codeBefore: lines(GRID, 7),
    codeAfter: lines(after, 7),
    caption: "translate-Griff (Ziel des Pfeils) zum Schnittpunkt rechts/hinten/deckel gezogen: Ziel im Fangradius hervorgehoben und beschriftet, beim Loslassen steht die Referenz g[rechts, hinten, deckel] im Code (keine Koordinaten).",
  });
});

test("B03 Snap auf Anker schreibt k.ecke", async ({ page }) => {
  await openApp(page);
  const src = `module teil() {
    anchor ecke = [30, 20, 10];
    cube([30, 20, 10]);
}
k: teil();
translate([0, 0, 25]) sphere(3);
`;
  await loadText(page, src);
  await clickLine(page, 6, 0);
  const [h] = await waitHandles(page);
  await screenshotB(page, "b03-vorher");
  const from = grip(h);
  const to = await project(page, [30, 20, 10]);
  await mouseDrag(page, from, line(from, to, 14), {
    mid: async () => {
      await page.waitForFunction(() => (window as unknown as { __cad: { drag(): { snap: string | null } | null } }).__cad.drag()?.snap === "k.ecke");
      await screenshotB(page, "b03-waehrend");
    },
  });
  await waitDragDone(page);
  const after = await text(page);
  expect(lines(after, 6)).toBe("translate(k.ecke) sphere(3);");
  await screenshotB(page, "b03-nachher");
  recordB({
    id: "B03",
    title: "Snap auf Anker",
    before: "b03-vorher",
    after: "b03-waehrend",
    codeBefore: lines(src, 6),
    codeAfter: lines(after, 6),
    caption: "Griff des translate-Ziels auf die Raute des Ankers ecke der benannten Instanz k gezogen: Beschriftung „k.ecke“, beim Loslassen wird k.ecke geschrieben.",
  });
});

test("B04 Alt-Ziehen: frei, Zahlen auf die Schrittweite gerundet", async ({ page }) => {
  await openApp(page);
  await loadText(page, GRID);
  await clickLine(page, 7, 0);
  let [h] = await waitHandles(page);
  const from = grip(h);
  // Close to the grid point, but with Alt: no snapping.
  const near = await project(page, [40, 30, 2]);
  await mouseDrag(page, from, line(from, { x: near.x - 3, y: near.y + 2 }, 12), {
    alt: true,
    mid: async () => {
      const d = await hook(page, (c) => c.drag());
      expect(d!.alt).toBe(true);
      expect(d!.snap).toBeNull();
      await expect(page.getByTestId("status-drag")).toContainText("Alt: frei");
      await screenshotB(page, "b04-waehrend");
    },
  });
  await waitDragDone(page);
  let after = await text(page);
  const m = lines(after, 7).match(/^translate\(\[(-?[\d.]+), (-?[\d.]+), (-?[\d.]+)\]\) cube\(4\);$/);
  expect(m, lines(after, 7)).not.toBeNull();
  const v = m!.slice(1).map(Number);
  expect(v.every((x) => Number.isInteger(x))).toBe(true);
  expect(v[0]).toBeGreaterThan(20);
  const first = lines(after, 7);
  // Step 0.5 mm in the settings.
  await page.getByRole("button", { name: "Einstellungen", exact: true }).click();
  await page.getByRole("dialog", { name: "Einstellungen" }).getByRole("spinbutton", { name: "Schrittweite Länge" }).fill("0.5");
  await page.getByRole("button", { name: "Einstellungen", exact: true }).click();
  await clickLine(page, 7, 0);
  [h] = await waitHandles(page);
  const f2 = grip(h);
  await mouseDrag(page, f2, line(f2, { x: f2.x - 37, y: f2.y + 23 }, 10), { alt: true });
  await waitDragDone(page);
  after = await text(page);
  const m2 = lines(after, 7).match(/^translate\(\[(-?[\d.]+), (-?[\d.]+), (-?[\d.]+)\]\) cube\(4\);$/);
  expect(m2).not.toBeNull();
  expect(m2!.slice(1).map(Number).every((x) => Number.isInteger(x * 2))).toBe(true);
  await screenshotB(page, "b04-nachher");
  recordB({
    id: "B04",
    title: "Alt-Ziehen",
    before: "b04-waehrend",
    after: "b04-nachher",
    codeBefore: lines(GRID, 7),
    codeAfter: `${first}\n${lines(after, 7)}`,
    caption: `Mit gedrückter Alt-Taste nahe an einen Rasterpunkt gezogen: kein Snap, es werden Zahlen geschrieben, gerundet auf 1 mm (${first}). Danach Schrittweite in den Einstellungen auf 0,5 mm gesetzt: ${lines(after, 7)}.`,
  });
});

test("B05 Rasterebene mit Literal ziehen", async ({ page }) => {
  await openApp(page);
  await loadText(page, GRID);
  await clickLine(page, 2, 8);
  const hs = await waitHandles(page, "(hs) => hs.filter(h => h.kind === 'plane').length === 6");
  const r = hs.find((h) => h.kind === "plane" && h.axis === 0 && h.value[0] === 40)!;
  await screenshotB(page, "b05-vorher");
  const from = grip(r);
  const to = await handlePoint(page, r.id, 52);
  await mouseDrag(page, from, line(from, to, 12), {
    mid: async () => {
      await waitText(page, `(t) => !t.includes("rechts = 40;")`);
    },
  });
  await waitDragDone(page);
  const after = await text(page);
  const m = lines(after, 2).match(/rechts = (\d+);/);
  expect(m).not.toBeNull();
  expect(Number(m![1])).toBeGreaterThanOrEqual(49);
  expect(Number(m![1])).toBeLessThanOrEqual(55);
  // The plane handle is at the new value.
  const hs2 = await waitHandles(page, `(hs) => hs.some(h => h.kind === 'plane' && h.axis === 0 && h.value[0] === ${m![1]})`);
  expect(hs2.length).toBe(6);
  await screenshotB(page, "b05-nachher");
  recordB({
    id: "B05",
    title: "Rasterebene ziehen",
    before: "b05-vorher",
    after: "b05-nachher",
    codeBefore: lines(GRID, 2),
    codeAfter: lines(after, 2),
    caption: "Cursor im Rasterblock: alle Ebenen mit Literal als durchgezogene Linie mit Griff. Der Griff der Ebene rechts wurde entlang der x-Achse gezogen; das Literal folgt, Linie und Rasterpunkte wandern mit.",
  });
});

test("B06 Höhe snappt auf Rasterebene derselben Achse (g.deckel)", async ({ page }) => {
  await openApp(page);
  const src = `grid g {
    z: boden = 0, deckel = 20;
}
cube([30, 30, 1]);
cylinder(h = 12, r = 4);
`;
  await loadText(page, src);
  await clickLine(page, 5, 0);
  const hs = await waitHandles(page);
  const h = hs.find((x) => x.kind === "height")!;
  await screenshotB(page, "b06-vorher");
  const from = grip(h);
  const to = await handlePoint(page, h.id, 20);
  await mouseDrag(page, from, line(from, to, 10), {
    mid: async () => {
      await page.waitForFunction(() => (window as unknown as { __cad: { drag(): { snap: string | null } | null } }).__cad.drag()?.snap === "g.deckel");
      await screenshotB(page, "b06-waehrend");
    },
  });
  await waitDragDone(page);
  const after = await text(page);
  expect(lines(after, 5)).toBe("cylinder(h = g.deckel, r = 4);");
  const nb = await hook(page, (c) => c.nodeBounds(5));
  expect(nb[0].max[2]).toBeCloseTo(20, 3);
  recordB({
    id: "B06",
    title: "1D-Griff snappt auf Ebene",
    before: "b06-vorher",
    after: "b06-waehrend",
    codeBefore: lines(src, 5),
    codeAfter: lines(after, 5),
    caption: "Höhengriff des Zylinders an die Rasterebene deckel gezogen: Ebenen derselben Achse sind Snap-Ziele, geschrieben wird g.deckel (neu in cad-eval: SnapRef::Plane).",
  });
});

test("B07 rotate-Ring rastet in 15°", async ({ page }) => {
  await openApp(page);
  const src = "rotate([0, 0, 10]) cube([30, 6, 6]);\n";
  await loadText(page, src);
  await clickLine(page, 1, 0);
  const hs = await waitHandles(page, "(hs) => hs.filter(h => h.kind === 'angle').length === 3");
  const z = hs.find((h) => h.kind === "angle" && h.axis === 2)!;
  expect(z.value).toEqual([10]);
  await screenshotB(page, "b07-vorher");
  const from = grip(z);
  const path: Pt[] = [];
  for (let a = 12; a <= 52; a += 2) path.push(await handlePoint(page, z.id, a));
  const seen = new Set<string>();
  await mouseDrag(page, from, path, {
    mid: async () => {
      await waitText(page, `(t) => t.startsWith("rotate([0, 0, 45])")`);
      await screenshotB(page, "b07-waehrend");
    },
  });
  seen.add(lines(await text(page), 1));
  await waitDragDone(page);
  const after = await text(page);
  expect(lines(after, 1)).toBe("rotate([0, 0, 45]) cube([30, 6, 6]);");
  const hs2 = await waitHandles(page, "(hs) => hs.some(h => h.kind === 'angle' && h.axis === 2 && h.value[0] === 45)");
  expect(hs2.length).toBe(3);
  await screenshotB(page, "b07-nachher");
  recordB({
    id: "B07",
    title: "Drehring rastet in 15°",
    before: "b07-vorher",
    after: "b07-nachher",
    codeBefore: lines(src, 1),
    codeAfter: lines(after, 1),
    caption: "Drehringe pro Achse; der Griff am z-Ring wurde von 10° bis 52° gezogen. Geschrieben werden nur Vielfache von 15° (Schrittweite für Winkel), am Ende 45°.",
  });
});

test("B08 gesperrter Griff bei berechnetem Wert", async ({ page }) => {
  await openApp(page);
  const src = `a = 3;
translate([a + 1, 0, 0] * 4) cube(5);
cube([30, 10, 1]);
`;
  await loadText(page, src);
  await clickLine(page, 2, 0);
  const [h] = await waitHandles(page);
  expect(h.style).toBe("locked");
  expect(h.value).toEqual([16, 0, 0]);
  const from = grip(h);
  await page.mouse.move(from.x, from.y);
  await expect(page.getByTestId("viewport-canvas")).toHaveAttribute("title", /gesperrt/);
  await screenshotB(page, "b08-vorher");
  await mouseDrag(page, from, line(from, { x: from.x + 80, y: from.y - 30 }, 8));
  expect(await hook(page, (c) => c.drag())).toBeNull();
  expect(await text(page)).toBe(src);
  await expect(page.getByTestId("status-notice")).toContainText("gesperrt");
  recordB({
    id: "B08",
    title: "Gesperrter Griff",
    before: "b08-vorher",
    codeBefore: lines(src, 2),
    codeAfter: lines(src, 2),
    caption: "Berechneter Wert [a + 1, 0, 0] * 4: der Griff wird angezeigt (grauer Ring), ist aber gesperrt. Ziehen ändert nichts, die Statuszeile meldet „berechneter Wert – gesperrt“.",
  });
});

test("B09 Variable mit Literal: Definition ändert sich, alle Verwendungen bewegen sich", async ({ page }) => {
  await openApp(page);
  const src = `p = [10, 0, 5];
translate(p) cube(4);
translate(p) sphere(3);
cube([40, 20, 1]);
`;
  await loadText(page, src);
  await clickLine(page, 2, 0);
  const [h] = await waitHandles(page);
  expect(h.style).toBe("literal");
  const c0 = (await hook(page, (c) => c.nodeBounds(2))).find((b) => b.label === "translate")!;
  const s0 = (await hook(page, (c) => c.nodeBounds(3))).find((b) => b.label === "translate")!;
  await screenshotB(page, "b09-vorher");
  const from = grip(h);
  await mouseDrag(page, from, line(from, { x: from.x + 90, y: from.y + 10 }, 12));
  await waitDragDone(page);
  const after = await text(page);
  expect(lines(after, 1)).toMatch(/^p = \[-?\d+, -?\d+, -?\d+\];$/);
  expect(lines(after, 1)).not.toBe("p = [10, 0, 5];");
  expect(lines(after, 2, 3)).toBe("translate(p) cube(4);\ntranslate(p) sphere(3);");
  const p = lines(after, 1).match(/-?\d+/g)!.map(Number);
  const c1 = (await hook(page, (c) => c.nodeBounds(2))).find((b) => b.label === "translate")!;
  const s1 = (await hook(page, (c) => c.nodeBounds(3))).find((b) => b.label === "translate")!;
  const d = [p[0] - 10, p[1], p[2] - 5];
  for (let k = 0; k < 3; k++) {
    expect(c1.min[k] - c0.min[k]).toBeCloseTo(d[k], 3);
    expect(s1.min[k] - s0.min[k]).toBeCloseTo(d[k], 3);
  }
  await screenshotB(page, "b09-nachher");
  recordB({
    id: "B09",
    title: "Variable mit Literal",
    before: "b09-vorher",
    after: "b09-nachher",
    codeBefore: lines(src, 1, 3),
    codeAfter: lines(after, 1, 3),
    caption: "translate(p) in Zeile 2 gezogen: geändert wird die Definition von p in Zeile 1, Würfel und Kugel (Zeilen 2 und 3) verschieben sich um denselben Betrag.",
  });
});

test("B10 Zonenmittelpunkt über translate snappt auf Raster", async ({ page }) => {
  await openApp(page);
  const src = `grid g {
    x: links = 0, rechts = 30;
    y: vorne = 0, hinten = 20;
    z: boden = 0, deckel = 15;
}
fillet(2) {
    box(g[links, vorne, boden], g[rechts, hinten, deckel]);
    translate([10, 10, 15]) sphere(4);
}
`;
  await loadText(page, src);
  await clickLine(page, 8, 4);
  // The handles of the previous cursor line (grid g, line 1) can still be shown for a moment.
  const [h] = await waitHandles(page, "(hs) => hs.length > 0 && hs.every((x) => x.line === 8)");
  await screenshotB(page, "b10-vorher");
  const from = grip(h);
  const to = await project(page, [30, 20, 15]);
  await mouseDrag(page, from, line(from, to, 12));
  await waitDragDone(page);
  const after = await text(page);
  expect(lines(after, 8)).toBe("    translate(g[rechts, hinten, deckel]) sphere(4);");
  await screenshotB(page, "b10-nachher");
  recordB({
    id: "B10",
    title: "Zonenmittelpunkt",
    before: "b10-vorher",
    after: "b10-nachher",
    codeBefore: lines(src, 8),
    codeAfter: lines(after, 8),
    caption: "Cursor auf der Zone (Kugel) im fillet-Block: Griff am Zonenmittelpunkt (translate), auf die Ecke rechts/hinten/deckel gezogen. Die Zone wandert, die ausgewählten Kanten folgen.",
  });
});

test("B11 Parameterleiste: Vorschau, Reset, Als Standard übernehmen", async ({ page }) => {
  await openApp(page);
  const src0 = await text(page);
  await clickLine(page, 10, 6);
  await waitScene(page, { context: "housing" });
  const bar = page.getByRole("region", { name: "Parameterleiste" });
  await expect(bar).toBeVisible();
  await expect(bar.getByTestId("param-w")).toBeVisible();
  await expect(bar.getByTestId("param-t")).toBeVisible();
  await screenshotB(page, "b11-vorher");
  // Number field: preview only.
  await bar.getByRole("textbox", { name: "w Wert" }).fill("60");
  await page.waitForFunction(() => {
    const b = (window as unknown as { __cad: { rootBounds(): { min: number[]; max: number[] } | null } }).__cad.rootBounds();
    return b !== null && Math.abs(b.max[0] - b.min[0] - 60) < 1e-6;
  });
  await waitScene(page, { context: "housing" });
  expect(await text(page)).toBe(src0);
  await expect(bar.getByTestId("param-w")).toHaveClass(/changed/);
  await screenshotB(page, "b11-vorschau");
  // Reset.
  await bar.getByRole("button", { name: "w zurücksetzen" }).click();
  await page.waitForFunction(() => {
    const b = (window as unknown as { __cad: { rootBounds(): { min: number[]; max: number[] } | null } }).__cad.rootBounds();
    return b !== null && Math.abs(b.max[0] - b.min[0] - 40) < 1e-6;
  });
  await expect(bar.getByTestId("param-w")).not.toHaveClass(/changed/);
  // Slider for h with the mouse.
  const sl = bar.getByRole("slider", { name: "h Schieberegler" });
  const box = (await sl.boundingBox())!;
  await page.mouse.move(box.x + box.width * 0.5, box.y + box.height / 2);
  await page.mouse.down();
  for (const f of [0.55, 0.6, 0.65, 0.7, 0.75]) {
    await page.mouse.move(box.x + box.width * f, box.y + box.height / 2);
    await frame(page);
  }
  await page.mouse.up();
  await page.waitForFunction(() => {
    const b = (window as unknown as { __cad: { rootBounds(): { min: number[]; max: number[] } | null } }).__cad.rootBounds();
    return b !== null && b.max[2] - b.min[2] > 25;
  });
  const hval = Number(await bar.getByRole("textbox", { name: "h Wert" }).inputValue());
  expect(hval).toBeGreaterThan(25);
  expect(await text(page)).toBe(src0);
  // Values per module stay for the session: leave the module and come back.
  await bar.getByRole("textbox", { name: "d Wert" }).fill("50");
  await clickLine(page, 21, 0);
  await waitScene(page, { context: null });
  await expect(page.getByRole("region", { name: "Parameterleiste" })).toHaveCount(0);
  await clickLine(page, 10, 6);
  await waitScene(page, { context: "housing" });
  await expect(page.getByRole("textbox", { name: "d Wert" })).toHaveValue("50");
  await expect(page.getByTestId("param-d")).toHaveClass(/changed/);
  // "Als Standard übernehmen" writes the default into the module definition.
  await page.getByTestId("param-d").getByRole("button", { name: "Als Standard übernehmen" }).click();
  await waitText(page, `(t) => t.includes("module housing(w = 40, d = 50, h = 20, t = 3)")`);
  await expect(page.getByTestId("param-d")).not.toHaveClass(/changed/);
  await waitScene(page, { context: "housing" });
  const after = await text(page);
  await screenshotB(page, "b11-nachher");
  // "Alle zurücksetzen": h back to its default.
  await page.getByRole("button", { name: "Alle zurücksetzen" }).click();
  await page.waitForFunction(() => {
    const b = (window as unknown as { __cad: { rootBounds(): { min: number[]; max: number[] } | null } }).__cad.rootBounds();
    return b !== null && Math.abs(b.max[2] - b.min[2] - 20) < 1e-6;
  });
  recordB({
    id: "B11",
    title: "Parameterleiste",
    before: "b11-vorschau",
    after: "b11-nachher",
    codeBefore: lines(src0, 3),
    codeAfter: lines(after, 3),
    caption: `Cursor in module housing: Leiste mit w, d, h, t (Eingabefeld + Schieberegler, Reset ↺). w = 60 änderte nur die Vorschau (Code unverändert, Zeile markiert), Reset stellte 40 her; h per Schieberegler auf ${hval}. d = 50 blieb beim Verlassen und Wiederbetreten des Moduls erhalten; „Als Standard übernehmen“ schrieb d = 50 in die Moduldefinition.`,
  });
});

test("B12 Inline-Schalter, kein Schieberegler", async ({ page }) => {
  await openApp(page);
  // Number literals have no inline slider.
  await loadText(page, "t = 3;\ncube([t * 5, 10, 10]);\n");
  await waitScene(page);
  await expect(page.getByTestId("inline-slider")).toHaveCount(0);
  // Bool switch.
  const src2 = "hohl = true;\nif (hohl) cube(10); else sphere(5);\n";
  await loadText(page, src2);
  await screenshotB(page, "b12-vorher", "top");
  await page.getByRole("checkbox", { name: "Wert umschalten" }).click();
  await waitText(page, `(t) => t.startsWith("hohl = false;")`);
  await waitScene(page);
  const nb = await hook(page, (c) => c.nodeBounds(2));
  expect(nb.some((b) => b.label === "sphere")).toBe(true);
  await hook(page, (c) => c.fitAll());
  await frame(page);
  await screenshotB(page, "b12-nachher", "top");
  await hook(page, (c) => c.focusEditor());
  await page.keyboard.press("Control+z");
  expect(await text(page)).toBe(src2);
  recordB({
    id: "B12",
    title: "Inline-Schalter",
    before: "b12-vorher",
    after: "b12-nachher",
    codeBefore: "hohl = true;",
    codeAfter: "hohl = false;",
    caption: "Bool-Literal einer Zuweisung: Schalter am Zeilenende, Klick schreibt false; die Szene wechselt auf den else-Zweig. Zahl-Literale haben keinen Inline-Schieberegler (nur in der Parameterleiste).",
  });
});

test("B13 Strg+Klick auf Rasterausdruck springt zur Rasterdeklaration", async ({ page }) => {
  await openApp(page);
  // The references of a module are known once the cursor is in it (module context).
  const src0 = await text(page);
  await clickLine(page, 11, 0);
  await waitScene(page, { context: "housing" });
  const p = await page.evaluate(() => (window as unknown as { __cad: { coordsOf(l: number, c: number): { x: number; y: number } | null } }).__cad.coordsOf(11, 14));
  await page.mouse.move(p!.x, p!.y);
  await page.keyboard.down("Control");
  await page.mouse.click(p!.x, p!.y);
  await page.keyboard.up("Control");
  await page.waitForFunction(() => (window as unknown as { __cad: { cursorLine(): number } }).__cad.cursorLine() === 4);
  // Plain click keeps placing the cursor.
  await page.mouse.click(p!.x, p!.y);
  await page.waitForFunction(() => (window as unknown as { __cad: { cursorLine(): number } }).__cad.cursorLine() === 11);
  recordB({
    id: "B13",
    title: "Sprung zur Rasterdeklaration",
    codeBefore: `11: ${lines(src0, 11).trim()}`,
    codeAfter: `4: ${lines(src0, 4).trim()}   ← Cursor`,
    caption: "Strg+Klick auf g[left, front, floor] (Zeile 11) setzt den Cursor auf die Deklaration grid g (Zeile 4); ein normaler Klick setzt weiter nur den Cursor.",
  });
});

test("B14 Messwerte: Latenz Ziehen → Szene (grob/fein), fps beim Ziehen", async ({ page }) => {
  await openApp(page);
  await openExample(page, "spec/hole_plate.mcad");
  // Module context: drag the plate thickness plane `top = t` (default of t) up and down.
  await clickLine(page, 8, 12);
  await waitScene(page, { context: "hole_plate" });
  await hook(page, (c) => c.setView("iso"));
  const hs = await waitHandles(page, "(hs) => hs.some(h => h.kind === 'plane' && h.axis === 2 && h.value[0] === 4)");
  const top = hs.find((h) => h.kind === "plane" && h.axis === 2 && h.value[0] === 4)!;
  expect(top.style).toBe("default");
  const before = await cad(page);
  const n0 = before.metrics.dragToScene.length;
  const from = grip(top);
  const path: Pt[] = [];
  for (let i = 0; i <= 30; i++) path.push(await handlePoint(page, top.id, 4 + (i * 6) / 30));
  for (let i = 30; i >= 0; i--) path.push(await handlePoint(page, top.id, 4 + (i * 6) / 30));
  for (let i = 0; i <= 15; i++) path.push(await handlePoint(page, top.id, 4 + (i * 4) / 15));
  await mouseDrag(page, from, path, { mid: async () => screenshotB(page, "b14-waehrend") });
  const s = await waitDragDone(page);
  await page.waitForFunction(() => (window as unknown as { __cad: { state(): { metrics: { dragEndToFine: number[] } } } }).__cad.state().metrics.dragEndToFine.length > 0);
  const m = (await cad(page)).metrics;
  const lat = m.dragToScene.slice(n0);
  const coarse = lat.filter((x) => x.coarse).map((x) => x.ms);
  const fine = m.dragEndToFine;
  const fps = m.dragFps.at(-1)!;
  const sorted = [...coarse].sort((a, b) => a - b);
  const med = sorted[Math.floor(sorted.length / 2)];
  const p95 = sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * 0.95))];
  const after = await text(page);
  expect(lines(after, 3)).toMatch(/module hole_plate\(n = 4, pitch = 12, t = \d+, margin = 6\)/);
  expect(coarse.length).toBeGreaterThan(5);
  writeMetricsB({
    drag: {
      model: "spec/hole_plate.mcad, Modul hole_plate, Ebene top = t (Standardwert)",
      moves: path.length,
      sent: fps.sent,
      dropped: fps.dropped,
      coarse: { n: coarse.length, median: med, p95, max: sorted.at(-1) },
      fine: { n: fine.length, last: fine.at(-1) },
      fps: fps.fps,
      maxGapMs: fps.maxGapMs,
      durationMs: fps.ms,
      evalMs: s.metrics.evalMs.slice(-20),
    },
  });
  await screenshotB(page, "b14-nachher");
  recordB({
    id: "B14",
    title: "Messwerte beim Ziehen",
    before: "b14-waehrend",
    after: "b14-nachher",
    codeBefore: "module hole_plate(n = 4, pitch = 12, t = 4, margin = 6) {",
    codeAfter: lines(after, 3),
    caption: `hole_plate (Differenz mit fillet = 1, vier Bohrungen): Ebene top = t (Standardwert, andere Farbe) ${path.length}-mal bewegt, ein Schritt pro Frame. Ziehen → Szene grob: Median ${med.toFixed(0)} ms, p95 ${p95.toFixed(0)} ms (${coarse.length} Szenen); Loslassen → feine Szene ${fine.at(-1)!.toFixed(0)} ms; ${fps.fps.toFixed(0)} fps während des Ziehens, ${fps.sent} Drag-Nachrichten, ${fps.dropped} übersprungen.`,
  });
});
