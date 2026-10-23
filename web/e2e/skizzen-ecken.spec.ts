// End-to-end tests for test report 15 (sketches: corners, offset, stroke; Plan 17.3): the radius
// handle of corner() dragged changes the literal, fillet2d with a zone (zone blue, new corner
// arcs coloured, inline hint), the offset contour dashed over the input contour, the centre
// line of a stroke, an error at the corner line.

import { expect, test, type Page } from "@playwright/test";
import {
  type CadState,
  type HandleView,
  type Pt,
  clickLine,
  hook,
  layerPixels,
  line,
  lineEl,
  loadText,
  mouseDrag,
  openApp,
  record15,
  screenshot15,
  waitDragDone,
  waitHandles,
  waitScene,
  waitView,
} from "./helpers";

test.beforeEach(async ({ page }) => {
  page.on("pageerror", (e) => console.log("pageerror:", e.message));
  await page.addInitScript(() => localStorage.setItem("cad-colors-open", "0"));
});

const text = (page: Page) => hook(page, (c) => c.text());
const grip = (h: HandleView): Pt => {
  if (!h.screen) throw new Error(`handle ${h.id} not visible`);
  return h.screen;
};

async function cursorOn(page: Page, l: number, col: number, pred = "() => true"): Promise<CadState> {
  await clickLine(page, l, col);
  await waitScene(page, { context: null });
  return waitView(page, `(s) => s.view.selection.line === ${l} && (${pred})(s)`);
}

const ECKEN = `linear_extrude(3) sketch {
    start([0, 0]);
    line(to = [40, 0]);
    corner(r = 6);
    line(to = [40, 25]);
    corner(r = 3);
    line(to = [0, 25]);
    corner(chamfer = 4);
    close();
}
`;

test("E01 corner: Radius-Griff ziehen ändert das Literal", async ({ page }) => {
  await openApp(page);
  await loadText(page, ECKEN);
  await cursorOn(page, 4, 6, "(s) => (s.view.plan.sketches ?? [])[0]?.active.length > 0");
  const s = await waitView(page, "(s) => (s.view.plan.sketches ?? [])[0]?.active.some(a => a.kind === 'arc')");
  expect((s.view!.plan.sketches ?? [])[0]?.active.map((a) => `${a.kind}@${a.line}`)).toEqual(["arc@4"]);
  const hs = await waitHandles(page, "(hs) => hs.some(h => h.kind === 'radius')");
  const h = hs.find((x) => x.kind === "radius")!;
  expect(h.value).toEqual([6]);
  const act = await layerPixels(page, "sketch:active");
  expect(act.changed).toBeGreaterThan(20);
  await screenshot15(page, "e01-vorher", "view");
  // drag towards the vertex [40, 0]: smaller radius; away from it: larger
  const from = grip(h);
  await mouseDrag(page, from, line(from, { x: from.x - 40, y: from.y - 40 }, 12), { alt: true });
  await waitDragDone(page);
  await page.mouse.move(2, 2);
  const after = await text(page);
  const m = after.match(/corner\(r = (-?[\d.]+)\);/)!;
  expect(m).not.toBeNull();
  const r = Number(m[1]);
  expect(r).toBeGreaterThan(6);
  // the other lines keep their literals
  expect(after).toContain("corner(r = 3);");
  expect(after).toContain("line(to = [40, 25]);");
  await cursorOn(page, 4, 6, "(s) => (s.view.plan.sketches ?? [])[0]?.active.some(a => a.kind === 'arc')");
  await screenshot15(page, "e01-nachher", "view");
  record15({
    id: "E01",
    title: "Radius-Griff von corner()",
    images: [
      { name: "e01-vorher", label: "Cursor auf corner(r = 6)" },
      { name: "e01-nachher", label: `nach dem Ziehen: corner(r = ${m[1]})` },
    ],
    code: ECKEN.trim(),
    caption:
      "Cursor auf einer corner-Zeile: die Verrundung ist das Segment dieser Zeile (dick in der Akzentfarbe), der Radius-Griff sitzt auf der Bogenmitte und bewegt sich linear mit r vom Eckpunkt weg. Ziehen (Alt: frei) schreibt das Literal; die Nachbarsegmente werden neu gekürzt.",
    checks: [`Griff: radius, Wert 6`, `Literal: 6 → ${m[1]}`, `Pixel des aktiven Bogens: ${act.changed}`, "andere Zeilen unverändert"],
  });
});

const HALTER = `t = 6;
linear_extrude(10) fillet2d(1) {
    fillet2d(5) {
        polygon([[0, 0], [40, 0], [40, t], [t, t], [t, 40], [0, 40]]);
        translate([t, t]) circle(1.5);
    }
}
`;

test("E02 fillet2d mit Zone: Zone blau, neue Eckbögen, Hinweis", async ({ page }) => {
  await openApp(page);
  await loadText(page, HALTER);
  const s = await cursorOn(page, 3, 8, "(s) => s.view.plan.items.some(i => i.mode === 'blue')");
  expect(s.view!.plan.items.filter((i) => i.mode === "blue").map((i) => i.label)).toEqual(["translate"]);
  expect(s.view!.edgeKinds.selected ?? 0).toBeGreaterThan(0);
  await expect(lineEl(page, 3)).toContainText("1 Ecke");
  await expect(lineEl(page, 2)).toContainText("5 Ecken");
  await screenshot15(page, "e02-innen", "view");
  const o = await cursorOn(page, 2, 22, "(s) => (s.view.edgeKinds.selected ?? 0) > 0");
  await screenshot15(page, "e02-aussen", "view");
  record15({
    id: "E02",
    title: "fillet2d mit Zone",
    images: [
      { name: "e02-innen", label: "Cursor auf fillet2d(5): Zone um die Innenecke" },
      { name: "e02-aussen", label: "Cursor auf fillet2d(1): die übrigen fünf Ecken" },
    ],
    code: HALTER.trim(),
    caption:
      "Der innere Block rundet nur die Ecke in der Zone (Kreis um [t, t], blau) mit r = 5; der äußere alle übrigen Ecken mit r = 1 – die Übergänge des großen Bogens sind tangential und zählen nicht. Die neuen Eckbögen sind wie ausgewählte Kanten gefärbt, die Anzahl steht als Inline-Hinweis an der Zeile.",
    checks: ["Zone: 1 (blau)", "Hinweis Zeile 3: 1 Ecke", "Hinweis Zeile 2: 5 Ecken", `ausgewählte Kanten (außen): ${o.view!.edgeKinds.selected}`],
  });
});

const OFFSET = `module platte() difference() {
    square([40, 24]);
    translate([12, 12]) circle(5);
}
linear_extrude(2) offset(r = 2) platte();
translate([0, 40, 0]) linear_extrude(2) offset(delta = -1.5) platte();
`;

test("E03 offset: Offset-Kontur gestrichelt über der Ausgangskontur", async ({ page }) => {
  await openApp(page);
  await loadText(page, OFFSET);
  const s = await cursorOn(page, 5, 22, "(s) => (s.view.plan.guides.offset ?? 0) > 0");
  expect(s.view!.plan.guides.offset).toBe(2); // outer contour and the hole
  expect(s.view!.plan.items.some((i) => i.mode === "profile" && i.label === "platte")).toBe(true);
  const px = await layerPixels(page, "guide:offset");
  expect(px.changed).toBeGreaterThan(50);
  // the offset contour reaches 2 beyond the plate
  const pts = s.view!.plan.guidePoints.filter((g) => g.kind === "offset").flatMap((g) => g.points);
  expect(Math.min(...pts.map((p) => p[0]))).toBeCloseTo(-2, 6);
  expect(Math.max(...pts.map((p) => p[1]))).toBeCloseTo(26, 6);
  await screenshot15(page, "e03-aussen", "view");
  const t = await cursorOn(page, 6, 40, "(s) => (s.view.plan.guides.offset ?? 0) > 0");
  await screenshot15(page, "e03-innen", "view");
  record15({
    id: "E03",
    title: "offset: Darstellung",
    images: [
      { name: "e03-aussen", label: "offset(r = 2): Ecken als Bögen, Loch kleiner" },
      { name: "e03-innen", label: "offset(delta = -1.5): Loch größer, Ecken spitz" },
    ],
    code: OFFSET.trim(),
    caption:
      "Cursor auf offset: das Profil der Kinder als Umriss (Ausgangskontur), darüber die Offset-Kontur gestrichelt in der Akzentfarbe; das Ergebnis selbst als Fläche. Außen werden die Ecken zu exakten Bögen, das Loch wird kleiner; nach innen (delta) bleiben die Ecken spitz, das Loch wächst.",
    checks: [`Offset-Konturen: ${s.view!.plan.guides.offset}`, `Pixel: ${px.changed}`, "Hülle −2 … 42 × −2 … 26", `innen: ${t.view!.plan.guides.offset} Konturen`],
  });
});

const STROKE = `linear_extrude(2) stroke(3, cap = "round") sketch(closed = false) {
    start([0, 0]);
    line(to = [30, 0]);
    arc(to = [45, 15]);
    line(to = [45, 35]);
}
translate([60, 0, 0]) linear_extrude(1) stroke(1.5, cap = "flat", join = "miter") sketch(closed = false) {
    start([0, 0]);
    for (i = [1 : 6]) line(to = [5 * i, i % 2 == 0 ? 0 : 8]);
}
`;

test("E04 stroke: Mittellinie, runde Kappen und Gehrung", async ({ page }) => {
  await openApp(page);
  await loadText(page, STROKE);
  const s = await cursorOn(page, 1, 22, "(s) => (s.view.plan.guides.centerline ?? 0) > 0");
  expect(s.view!.plan.guides.centerline).toBe(1);
  const px = await layerPixels(page, "guide:centerline");
  expect(px.changed).toBeGreaterThan(30);
  await screenshot15(page, "e04-kabel", "view");
  const z = await cursorOn(page, 7, 40, "(s) => (s.view.plan.guides.centerline ?? 0) > 0");
  await screenshot15(page, "e04-zickzack", "view");
  record15({
    id: "E04",
    title: "stroke: Darstellung",
    images: [
      { name: "e04-kabel", label: "stroke(3) eines Pfads aus Gerade, Bogen, Gerade" },
      { name: "e04-zickzack", label: "stroke(1.5, flat, miter) eines Zickzacks" },
    ],
    code: STROKE.trim(),
    caption:
      "Cursor auf stroke: der Pfad (offene Skizze) als Umriss, die Mittellinie gestrichelt, das Ergebnis als Fläche. Der Bogen wird zu einem exakten Ringsektor, die Kappen zu Halbkreisen; der Zickzack bekommt Gehrungen an den Knicken und flache Enden.",
    checks: [`Mittellinien: ${s.view!.plan.guides.centerline}`, `Pixel: ${px.changed}`, `Zickzack: ${z.view!.plan.guides.centerline} Mittellinie`],
  });
});

test("E05 Radius zu groß: Fehler an der corner-Zeile", async ({ page }) => {
  await openApp(page);
  const src = "linear_extrude(2) sketch {\n    start([0, 0]);\n    line(to = [20, 0]);\n    corner(r = 12);\n    line(to = [20, 8]);\n    line(to = [0, 8]);\n    close();\n}\n";
  await loadText(page, src);
  const s = await waitView(page, "(s) => s.diagnostics.some(d => d.severity === 'error')", null, { meshes: false });
  const errs = s.diagnostics.filter((d) => d.severity === "error").map((d) => `${d.line}: ${d.message}`);
  expect(errs).toHaveLength(1);
  expect(errs[0].startsWith("4:") && errs[0].includes("höchstens ≈ 8")).toBe(true);
  await expect(lineEl(page, 4)).toHaveClass(/cm-l-error/);
  await cursorOn(page, 4, 6, "(s) => (s.view.plan.sketches ?? []).length === 1");
  await screenshot15(page, "e05-fehler");
  record15({
    id: "E05",
    title: "Radius passt nicht",
    images: [{ name: "e05-fehler", label: "corner(r = 12) an einer 8 mm langen Kante" }],
    code: src.trim(),
    caption: "Der Radius passt nicht auf die kurze Kante: Fehler an der corner-Zeile mit dem größten passenden Wert; die Skizze bleibt mit der spitzen Ecke sichtbar, der Griff liegt am Eckpunkt.",
    checks: errs,
  });
});
