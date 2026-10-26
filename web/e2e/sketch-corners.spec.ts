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

const CORNERS = `linear_extrude(3) sketch {
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

test("E01 corner: dragging the radius handle changes the literal", async ({ page }) => {
  await openApp(page);
  await loadText(page, CORNERS);
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
    title: "Radius handle of corner()",
    images: [
      { name: "e01-vorher", label: "cursor on corner(r = 6)" },
      { name: "e01-nachher", label: `after the drag: corner(r = ${m[1]})` },
    ],
    code: CORNERS.trim(),
    caption:
      "Cursor on a corner line: the fillet is the segment of this line (thick in the accent colour), the radius handle sits at the middle of the arc and moves linearly with r away from the vertex. Dragging (Alt: free) writes the literal; the neighbouring segments are trimmed again.",
    checks: [`handle: radius, value 6`, `literal: 6 → ${m[1]}`, `pixels of the active arc: ${act.changed}`, "other lines unchanged"],
  });
});

const HOLDER = `t = 6;
linear_extrude(10) fillet2d(1) {
    fillet2d(5) {
        polygon([[0, 0], [40, 0], [40, t], [t, t], [t, 40], [0, 40]]);
        translate([t, t]) circle(1.5);
    }
}
`;

test("E02 fillet2d with a zone: zone blue, new corner arcs, hint", async ({ page }) => {
  await openApp(page);
  await loadText(page, HOLDER);
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
    title: "fillet2d with a zone",
    images: [
      { name: "e02-innen", label: "cursor on fillet2d(5): zone around the inner corner" },
      { name: "e02-aussen", label: "cursor on fillet2d(1): the other five corners" },
    ],
    code: HOLDER.trim(),
    caption:
      "The inner block rounds only the corner in the zone (circle around [t, t], blue) with r = 5; the outer one all other corners with r = 1 – the transitions of the large arc are tangent and do not count. The new corner arcs are coloured like selected edges, their count is an inline hint at the line.",
    checks: ["zone: 1 (blue)", "hint line 3: 1 Ecke (corner)", "hint line 2: 5 Ecken (corners)", `selected edges (outer): ${o.view!.edgeKinds.selected}`],
  });
});

const OFFSET = `module plate() difference() {
    square([40, 24]);
    translate([12, 12]) circle(5);
}
linear_extrude(2) offset(r = 2) plate();
translate([0, 40, 0]) linear_extrude(2) offset(delta = -1.5) plate();
`;

test("E03 offset: offset contour dashed over the input contour", async ({ page }) => {
  await openApp(page);
  await loadText(page, OFFSET);
  const s = await cursorOn(page, 5, 22, "(s) => (s.view.plan.guides.offset ?? 0) > 0");
  expect(s.view!.plan.guides.offset).toBe(2); // outer contour and the hole
  expect(s.view!.plan.items.some((i) => i.mode === "profile" && i.label === "plate")).toBe(true);
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
    title: "offset: display",
    images: [
      { name: "e03-aussen", label: "offset(r = 2): corners as arcs, hole smaller" },
      { name: "e03-innen", label: "offset(delta = -1.5): hole larger, corners sharp" },
    ],
    code: OFFSET.trim(),
    caption:
      "Cursor on offset: the profile of the children as an outline (input contour), above it the offset contour dashed in the accent colour; the result itself as a face. Outwards the corners become exact arcs, the hole gets smaller; inwards (delta) the corners stay sharp, the hole grows.",
    checks: [`offset contours: ${s.view!.plan.guides.offset}`, `pixels: ${px.changed}`, "hull −2 … 42 × −2 … 26", `inwards: ${t.view!.plan.guides.offset} contours`],
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

test("E04 stroke: centre line, round caps and miter", async ({ page }) => {
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
    title: "stroke: display",
    images: [
      { name: "e04-kabel", label: "stroke(3) of a path of line, arc, line" },
      { name: "e04-zickzack", label: "stroke(1.5, flat, miter) of a zigzag" },
    ],
    code: STROKE.trim(),
    caption:
      "Cursor on stroke: the path (open sketch) as an outline, the centre line dashed, the result as a face. The arc becomes an exact ring sector, the caps half circles; the zigzag gets miters at the bends and flat ends.",
    checks: [`centre lines: ${s.view!.plan.guides.centerline}`, `pixels: ${px.changed}`, `zigzag: ${z.view!.plan.guides.centerline} centre line`],
  });
});

test("E05 radius too large: error at the corner line", async ({ page }) => {
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
    title: "Radius does not fit",
    images: [{ name: "e05-fehler", label: "corner(r = 12) at an 8 mm long edge" }],
    code: src.trim(),
    caption: "The radius does not fit on the short edge: error at the corner line with the largest value that fits; the sketch stays visible with the sharp corner, the handle lies at the vertex.",
    checks: errs,
  });
});
