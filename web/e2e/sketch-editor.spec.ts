// End-to-end tests for test report 18 (sketch editor, Plan 17.6/17.12): drawing mode with the
// view perpendicular onto the sketch plane, every tool writing code into the sketch block at the
// cursor (or a new one), snapping with a visible marker, undo per action, dragging a handle,
// deleting a segment, sketches under transformations and in rotate_extrude, the dark theme.

import { expect, test, type Page } from "@playwright/test";
import { type Pt, clickLine, hook, line, loadText, mode, mouseDrag, openApp, openExample, record18, screenshot18, sketchBar, sketchTool, waitDragDone, waitHandles, waitScene, waitView, writeMetrics18 } from "./helpers";

test.beforeEach(async ({ page }) => {
  page.on("pageerror", (e) => console.log("pageerror:", e.message));
  await page.addInitScript(() => localStorage.setItem("cad-colors-open", "0"));
});

interface SketchInfo {
  tool: string;
  points: Array<{ p: [number, number]; text: string; kind: string }>;
  message: string | null;
  actions: number;
  answers: number;
  prompt: string | null;
  sketch: { line: number; endLine: number } | null;
  node: number | null;
  plane: number[] | null;
  current: [number, number, number] | null;
  snap: { text: string; kind: string; label: string; point: [number, number, number]; local: [number, number]; screen: { x: number; y: number; visible: boolean } | null } | null;
  preview: number;
  guides: number;
  segment: { line: number } | null;
  raster: { lines: number; points: number; min: [number, number]; max: [number, number] } | null;
  drawn: Record<string, number>;
  rtt: Array<{ ms: number; click: boolean }>;
}

type W = { __cad: { sketch(): SketchInfo | null; sketchScreen(x: number, y: number): { x: number; y: number; visible: boolean } | null; camera(): { eye: number[]; dir: number[] } | null; text(): string; undo(): boolean } };

const info = (page: Page) => page.evaluate(() => (window as unknown as W).__cad.sketch());
const text = (page: Page) => hook(page, (c) => c.text());

async function waitSketch(page: Page, pred: string, arg: unknown = null): Promise<SketchInfo> {
  await page.waitForFunction(
    ([src, a]) => {
      const s = (window as unknown as W).__cad.sketch();
      // eslint-disable-next-line no-new-func
      return s !== null && new Function("s", "arg", `return (${src})(s, arg);`)(s, a);
    },
    [pred, arg] as const,
    { timeout: 20_000, polling: 50 },
  );
  return (await info(page))!;
}

/** Screen position of a point of the sketch plane. */
async function scr(page: Page, x: number, y: number): Promise<Pt> {
  const p = await page.evaluate(([a, b]) => {
    const q = (window as unknown as W).__cad.sketchScreen(a, b);
    // not under a toolbar of the view
    const free = q !== null && document.elementFromPoint(q.x, q.y)?.getAttribute("data-testid") === "viewport-canvas";
    return q && { ...q, visible: q.visible && free };
  }, [x, y] as const);
  if (!p || !p.visible) throw new Error(`point [${x}, ${y}] not visible or hidden`);
  return { x: p.x, y: p.y };
}

async function enter(page: Page): Promise<SketchInfo> {
  await mode(page, "Skizze").click();
  const s = await waitSketch(page, "(s) => s.answers > 0 && s.raster !== null");
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
  return s;
}

async function tool(page: Page, t: string): Promise<void> {
  await sketchTool(page, t);
  await waitSketch(page, "(s, t) => s.tool === t", t);
}

/** Hovers a plane point and waits for the snap there. */
async function hoverAt(page: Page, x: number, y: number, kind?: string): Promise<SketchInfo> {
  const p = await scr(page, x, y);
  const before = (await info(page))!.answers;
  await page.mouse.move(p.x + 0.5, p.y + 0.5);
  return waitSketch(page, "(s, a) => s.answers > a[0] && s.snap !== null && (a[1] === null || s.snap.kind === a[1])", [before, kind ?? null]);
}

/** Clicks plane points; waits until `actions` code actions are written and the scene is there. */
async function clicks(page: Page, pts: Array<[number, number]>, actions?: number): Promise<SketchInfo> {
  for (const [x, y] of pts) {
    const p = await scr(page, x, y);
    const before = (await info(page))!.answers;
    await page.mouse.move(p.x, p.y);
    await page.mouse.click(p.x, p.y);
    await waitSketch(page, "(s, a) => s.answers > a", before);
  }
  if (actions !== undefined) await waitSketch(page, "(s, n) => s.actions >= n", actions);
  await waitScene(page);
  return (await info(page))!;
}

const GRID = `grid g {
    x: a = 0, b = 10, c = 30, d = 40;
    y: u = 0, v = 15, w = 25;
}
linear_extrude(3) sketch {
}
`;

test("Z01 drawing mode: switch, view perpendicular onto the sketch plane, Esc ends it", async ({ page }) => {
  await openApp(page);
  await loadText(page, GRID);
  await clickLine(page, 6, 0);
  await waitScene(page);
  const s = await enter(page);
  expect(s.tool).toBe("line");
  expect(s.sketch?.line).toBe(5);
  expect(s.prompt).toBe("Linie: Anfangspunkt");
  // camera along −z (top view of the xy-plane)
  const cam = await page.evaluate(() => (window as unknown as W).__cad.camera());
  expect(cam!.dir[2]).toBeLessThan(-0.9999);
  await hoverAt(page, 10, 15, "grid");
  const s2 = await waitView(page, "() => true");
  expect(s2).toBeTruthy();
  const drawn = (await info(page))!.drawn;
  expect(drawn["sketch-grid"]).toBeGreaterThan(0);
  expect(drawn["sketch-raster"] ?? 0).toBeGreaterThan(0);
  await expect(sketchBar(page)).toBeVisible();
  await expect(page.getByTestId("status-sketch")).toContainText("Linie: Anfangspunkt");
  await screenshot18(page, "z01-modus", "full");
  await page.keyboard.press("Escape");
  await page.waitForFunction(() => (window as unknown as W).__cad.sketch() === null);
  await expect(sketchBar(page)).toHaveCount(0);
  record18({
    id: "Z01",
    title: "Drawing mode on/off",
    images: [{ name: "z01-modus", label: "drawing mode: toolbar, grid in the sketch plane, view from the top" }],
    code: GRID.trim(),
    caption:
      "“Skizze” (sketch) in the view bar switches the drawing mode on: the camera looks perpendicular onto the plane of the sketch at the cursor, the grid g and the step (1 mm, coarser when zooming out) lie in the plane, the status line says what the tool needs next. Esc ends the mode.",
    checks: [`view direction z = ${cam!.dir[2].toFixed(5)}`, `grid lines ${drawn["sketch-grid"]}, step grid ${drawn["sketch-raster"]}`, "Esc: mode off"],
  });
});

test("Z02 line into a new sketch, a click on the start closes", async ({ page }) => {
  await openApp(page);
  await loadText(page, "cube([30, 20, 2]);\n");
  const s0 = await enter(page);
  expect(s0.sketch).toBeNull();
  await clicks(page, [[0, 0], [20, 0]], 1);
  expect(await text(page)).toBe("cube([30, 20, 2]);\n\nsketch {\n    start([0, 0]);\n    line(to = [20, 0]);\n}\n");
  await clicks(page, [[20, 12]], 2);
  // the chain goes on: the preview from the last point
  const h = await hoverAt(page, 5, 12);
  expect(h.preview).toBe(1);
  expect(h.points.length).toBe(1);
  await clicks(page, [[5, 12]], 3);
  const st = await hoverAt(page, 0, 0, "start");
  expect(st.snap!.label).toContain("Anfang");
  await screenshot18(page, "z02-vor-schliessen", "full");
  await clicks(page, [[0, 0]], 4);
  const t = await text(page);
  expect(t).toBe("cube([30, 20, 2]);\n\nsketch {\n    start([0, 0]);\n    line(to = [20, 0]);\n    line(to = [20, 12]);\n    line(to = [5, 12]);\n    close();\n}\n");
  const s = await waitView(page, "(s) => s.diagnostics.filter(d => d.severity === 'error').length === 0");
  expect(s.diagnostics.filter((d) => d.severity === "error")).toEqual([]);
  await screenshot18(page, "z02-linie", "full");
  record18({
    id: "Z02",
    title: "Line",
    images: [
      { name: "z02-vor-schliessen", label: "before the last click: snap “Anfang (schließt)” (start, closes)" },
      { name: "z02-linie", label: "after the click: close(); closed contour" },
    ],
    code: t.trim(),
    caption:
      "Without a sketch block at the cursor the first action creates one (at the end of the file). Every click is an action: `start(…)` and the first line, then `line(to = …)` per point; the click on the start of the contour writes `close();`. Free points are rounded to the step.",
    checks: ["new sketch block", "4 actions", "close() by a click on the start", "no errors"],
  });
});

test("Z03 rectangle on grid points: references, snap display", async ({ page }) => {
  await openApp(page);
  await loadText(page, GRID);
  await clickLine(page, 6, 0);
  await waitScene(page);
  await enter(page);
  await tool(page, "rect");
  const h = await hoverAt(page, 10.2, 0.2, "grid");
  expect(h.snap!.text).toBe("g[b, u]");
  await expect(page.getByTestId("sketch-snap-label")).toHaveText("g[b, u]");
  await expect(page.getByTestId("sketch-snap-label")).toBeVisible();
  await screenshot18(page, "z03-snap", "view");
  await clicks(page, [[10, 0]]);
  const h2 = await hoverAt(page, 30, 15, "grid");
  expect(h2.preview).toBe(1);
  await screenshot18(page, "z03-vorschau", "view");
  await clicks(page, [[30, 15]], 1);
  const t = await text(page);
  expect(t).toContain("linear_extrude(3) sketch {\n    start(g[b, u]);\n    line(to = g[c, u]);\n    line(to = g[c, v]);\n    line(to = g[b, v]);\n    close();\n}");
  await waitView(page, "(s) => s.view.plan.items.length >= 0");
  await screenshot18(page, "z03-rechteck", "full");
  record18({
    id: "Z03",
    title: "Rectangle on grid points",
    images: [
      { name: "z03-snap", label: "snap to a grid point: square and text g[b, u]" },
      { name: "z03-vorschau", label: "second corner: preview of the rectangle" },
      { name: "z03-rechteck", label: "result: references instead of numbers" },
    ],
    code: t.trim(),
    caption:
      "Snapping to grid points writes references. For a rectangle from two grid points the two other corners become grid points too (`g[c, u]`, `g[b, v]`): when a plane changes, the rectangle follows.",
    checks: ["snap text g[b, u] visible", "corners as g[…]", "preview 1 polyline"],
  });
});

const OPEN = `linear_extrude(2) sketch(closed = false) {
    start([0, 0]);
    line(to = [20, 0]);
}
`;

async function openSketch(page: Page, src = OPEN, lineNo = 3): Promise<void> {
  await openApp(page);
  await loadText(page, src);
  await clickLine(page, lineNo, 6);
  await waitScene(page);
  await enter(page);
}

test("Z04 circle", async ({ page }) => {
  await openSketch(page);
  await tool(page, "circle");
  await clicks(page, [[26, 6]]);
  await hoverAt(page, 30, 6);
  await screenshot18(page, "z04-vorschau", "view");
  await clicks(page, [[30, 6]], 1);
  const t = await text(page);
  expect(t).toContain("    line(to = [20, 0]);\n    start([30, 6]);\n    arc(center = [26, 6], angle = 360);\n    close();\n}");
  await screenshot18(page, "z04-kreis", "full");
  record18({ id: "Z04", title: "Circle", images: [{ name: "z04-vorschau", label: "preview: circle around the centre" }, { name: "z04-kreis", label: "result" }], code: t.trim(), caption: "Centre, then a point on the circle: `start(p); arc(center = c, angle = 360); close();` – a contour of its own after the open one.", checks: ["start/arc(center, angle = 360)/close"] });
});

test("Z05 arc through three points, Z06 arc around the centre, Z07 tangent", async ({ page }) => {
  await openSketch(page);
  await tool(page, "arc_through");
  await clicks(page, [[20, 0], [24, 4]]);
  await hoverAt(page, 20, 8);
  await screenshot18(page, "z05-vorschau", "view");
  await clicks(page, [[20, 8]], 1);
  let t = await text(page);
  expect(t).toContain("    line(to = [20, 0]);\n    arc(through = [24, 4], to = [20, 8]);\n}");
  await screenshot18(page, "z05-bogen", "full");
  record18({ id: "Z05", title: "Arc through three points", images: [{ name: "z05-vorschau", label: "preview through start, through point and mouse" }, { name: "z05-bogen", label: "result" }], code: t.trim(), caption: "Start (here the end of the contour, no new start), a point on the arc, end point: `arc(through = q, to = p)`.", checks: ["arc(through = [24, 4], to = [20, 8])"] });

  // centre arc from the end [20, 8] around [16, 8], 90° counter-clockwise
  await tool(page, "arc_center");
  await clicks(page, [[20, 8], [16, 8]]);
  await hoverAt(page, 16.2, 12);
  await screenshot18(page, "z06-vorschau", "view");
  await clicks(page, [[16.2, 12]], 2);
  t = await text(page);
  expect(t).toContain("    arc(through = [24, 4], to = [20, 8]);\n    arc(center = [16, 8], angle = 90);\n}");
  await screenshot18(page, "z06-bogen", "full");
  record18({ id: "Z06", title: "Arc around a centre", images: [{ name: "z06-vorschau", label: "preview with the radius line" }, { name: "z06-bogen", label: "result" }], code: t.trim(), caption: "Start, centre, end angle: `arc(center = c, angle = a)`; the angle snaps to the angle step (15°), on a snapped point `arc(center = c, to = p)` results.", checks: ["arc(center = [16, 8], angle = 90)"] });

  // tangent arc from the end ([16, 12], direction −x)
  await tool(page, "arc_tangent");
  await hoverAt(page, 10, 10);
  await screenshot18(page, "z07-vorschau", "view");
  await clicks(page, [[10, 10]], 3);
  t = await text(page);
  expect(t).toContain("    arc(center = [16, 8], angle = 90);\n    arc(to = [10, 10]);\n}");
  await screenshot18(page, "z07-bogen", "full");
  record18({ id: "Z07", title: "Tangent arc", images: [{ name: "z07-vorschau", label: "preview tangent at the end of the contour" }, { name: "z07-bogen", label: "result" }], code: t.trim(), caption: "One click: the arc continues the direction of the last segment, `arc(to = p)`.", checks: ["arc(to = [10, 10])"] });
});

test("Z08 Bézier", async ({ page }) => {
  await openSketch(page);
  await tool(page, "bezier");
  await clicks(page, [[20, 0], [24, 0], [27, 5]]);
  await hoverAt(page, 27, 10);
  await screenshot18(page, "z08-vorschau", "view");
  await clicks(page, [[27, 10]], 1);
  const t = await text(page);
  expect(t).toContain("    line(to = [20, 0]);\n    bezier(c1 = [24, 0], c2 = [27, 5], to = [27, 10]);\n}");
  await screenshot18(page, "z08-bezier", "full");
  record18({ id: "Z08", title: "Bézier", images: [{ name: "z08-vorschau", label: "preview with the control polygon" }, { name: "z08-bezier", label: "result" }], code: t.trim(), caption: "Start, two control points, end: cubic curve `bezier(c1 = …, c2 = …, to = …)`.", checks: ["bezier(c1 = [24, 0], c2 = [27, 5], to = [27, 10])"] });
});

const RECT = `linear_extrude(2) sketch {
    start([0, 0]);
    line(to = [30, 0]);
    line(to = [30, 20]);
    line(to = [0, 20]);
    close();
}
`;

test("Z09 round a corner, Z10 close", async ({ page }) => {
  await openSketch(page, RECT, 2);
  await tool(page, "corner");
  const v = await hoverAt(page, 30, 20, "vertex");
  expect(v.snap!.label).toContain("Ecke");
  await clicks(page, [[30, 20]]);
  await hoverAt(page, 30, 16);
  await screenshot18(page, "z09-vorschau", "view");
  await clicks(page, [[30, 16]], 1);
  let t = await text(page);
  expect(t).toContain("    line(to = [30, 20]);\n    corner(r = 4);\n    line(to = [0, 20]);");
  await screenshot18(page, "z09-ecke", "full");
  record18({ id: "Z09", title: "Round a corner", images: [{ name: "z09-vorschau", label: "corner chosen, preview of the rounding" }, { name: "z09-ecke", label: "result" }], code: t.trim(), caption: "Choose a corner (only corners between two segments snap), then the radius as the distance to the corner: `corner(r = …)` after the segment that ends there; at a corner already rounded the radius changes.", checks: ["corner(r = 4) after line(to = [30, 20])"] });

  // close: an open contour (second one) closed with the button
  await page.keyboard.press("Escape");
  await page.waitForFunction(() => (window as unknown as W).__cad.sketch() === null);
  const src2 = "linear_extrude(2) sketch {\n    start([0, 0]);\n    line(to = [30, 0]);\n    line(to = [30, 20]);\n}\n";
  await loadText(page, src2);
  await clickLine(page, 4, 6);
  await waitScene(page);
  await enter(page);
  await screenshot18(page, "z10-vorher", "view");
  const a0 = (await info(page))!.actions;
  await sketchBar(page).getByRole("button", { name: "Schließen", exact: true }).click();
  await waitSketch(page, "(s, n) => s.actions > n", a0);
  await waitScene(page);
  t = await text(page);
  expect(t).toBe("linear_extrude(2) sketch {\n    start([0, 0]);\n    line(to = [30, 0]);\n    line(to = [30, 20]);\n    close();\n}\n");
  const s = await waitView(page, "(s) => s.diagnostics.filter(d => d.severity === 'error').length === 0");
  expect(s.diagnostics.filter((d) => d.severity === "error")).toEqual([]);
  await screenshot18(page, "z10-nachher", "view");
  record18({ id: "Z10", title: "Close", images: [{ name: "z10-vorher", label: "open contour (error: not closed)" }, { name: "z10-nachher", label: "after “Schließen” (close): close();" }], code: t.trim(), caption: "“Schließen” (close) writes `close();` for the open contour at the cursor; the error “Kontur nicht geschlossen” (contour not closed) disappears.", checks: ["close(); inserted", "no errors"] });
});

test("Z11 snap kinds: end point, midpoint, intersection, horizontal, grid line, tangent, Alt free", async ({ page }) => {
  const src = `grid g {
    x: a = 0, b = 10;
    y: u = 0;
}
linear_extrude(2) sketch(closed = false) {
    start([0, 5]);
    line(to = [20, 5]);
    line(to = [20, 25]);
    start([0, 25]);
    line(to = [30, 15]);
}
`;
  await openSketch(page, src, 10);
  const kinds: Record<string, string> = {};
  const probe = async (name: string, x: number, y: number, kind: string) => {
    const s = await hoverAt(page, x, y, kind);
    kinds[name] = `${s.snap!.kind}: ${s.snap!.text}`;
    return s;
  };
  await probe("end point", 20.3, 4.8, "end");
  await probe("midpoint", 10.2, 5.2, "mid");
  await probe("intersection", 20.2, 18.5, "intersection");
  await screenshot18(page, "z11-schnitt", "view");
  await probe("grid point", 10.2, 0.2, "grid");
  // after the start point: horizontal to it, and the grid line x = g.b
  await clicks(page, [[0, 25]]);
  const hz = await probe("horizontal", 42.4, 25.2, "horizontal");
  expect(hz.snap!.text).toBe("[42, 25]");
  expect(hz.guides).toBe(1);
  await screenshot18(page, "z11-horizontal", "view");
  const gl = await probe("grid line", 10.3, -6.4, "grid_line");
  expect(gl.snap!.text).toBe("[g.b, -6]");
  // Alt: free with the step, no snap
  const p = await scr(page, 10.2, 0.2);
  const before = (await info(page))!.answers;
  await page.keyboard.down("Alt");
  await page.mouse.move(p.x + 1, p.y + 1);
  const alt = await waitSketch(page, "(s, a) => s.answers > a && s.snap !== null", before);
  await page.keyboard.up("Alt");
  kinds["Alt"] = `${alt.snap!.kind}: ${alt.snap!.text}`;
  expect(alt.snap!.kind).toBe("step");
  // tangent: line along the end direction of the last line
  await page.keyboard.press("Escape");
  await waitSketch(page, "(s) => s.points.length === 0");
  await clicks(page, [[30, 15]]);
  const tg = await probe("tangent", 36, 13, "tangent");
  expect(tg.snap!.label).toContain("tangential");
  await screenshot18(page, "z11-tangential", "view");
  await clicks(page, [[36, 13]], 1);
  const t = await text(page);
  expect(t).toMatch(/line\(to = \[30, 15\]\);\n {4}line\(length = \d+\);\n\}/);
  record18({
    id: "Z11",
    title: "Snap kinds",
    images: [
      { name: "z11-schnitt", label: "intersection of two lines (ring)" },
      { name: "z11-horizontal", label: "horizontal to the start point (diamond, help line)" },
      { name: "z11-tangential", label: "tangent to the last segment" },
    ],
    code: t.trim(),
    caption:
      "Order: grid points and anchors (reference), start, corners, end, mid and intersection points and arc midpoints (numbers); otherwise tangent (writes `line(length = …)`), horizontal/vertical to the previous point and grid lines (`[g.b, 40]`), otherwise the step. Alt: free with the step. The mark shows the kind (ring point, square grid, diamond direction, cross step).",
    checks: Object.entries(kinds).map(([k, v]) => `${k} → ${v}`),
  });
});

test("Z12 undo: every action one step", async ({ page }) => {
  await openSketch(page);
  const t0 = await text(page);
  await clicks(page, [[20, 0], [20, 10]], 1);
  const t1 = await text(page);
  await clicks(page, [[5, 10]], 2);
  const t2 = await text(page);
  expect(t2).toContain("line(to = [5, 10]);");
  // Ctrl+Z with the focus in the view (not the editor): one action back
  await page.keyboard.press("Control+z");
  await page.waitForFunction((t) => (window as unknown as W).__cad.text() === t, t1);
  await page.keyboard.press("Control+z");
  await page.waitForFunction((t) => (window as unknown as W).__cad.text() === t, t0);
  await page.keyboard.press("Control+Shift+z");
  await page.waitForFunction((t) => (window as unknown as W).__cad.text() === t, t1);
  await waitScene(page);
  await screenshot18(page, "z12-undo", "full");
  record18({ id: "Z12", title: "Undo", images: [{ name: "z12-undo", label: "after undo, undo, redo" }], code: `// after two actions\n${t2.trim()}\n// Ctrl+Z, Ctrl+Z, Ctrl+Shift+Z\n${t1.trim()}`, caption: "Every action is a history step of its own (`start` + first line together, too); Ctrl+Z/Ctrl+Shift+Z work with the focus in the view as well.", checks: ["undo takes back exactly one action", "redo restores it"] });
});

test("Z13 drag a handle in the drawing mode: the grid reference is rewritten", async ({ page }) => {
  const src = `grid g {
    x: a = 0, b = 10, c = 30;
    y: u = 0, v = 15;
}
linear_extrude(2) sketch {
    start(g[a, u]);
    line(to = g[c, u]);
    line(to = g[b, v]);
    close();
}
`;
  await openSketch(page, src, 7);
  await tool(page, "select");
  // all handles of the sketch are shown in the drawing mode
  const hs = await waitHandles(page, "(hs) => hs.filter(h => h.kind === 'point').length >= 3");
  const h = hs.find((x) => x.kind === "point" && x.value[0] === 10 && x.value[1] === 15)!;
  expect(h).toBeTruthy();
  await screenshot18(page, "z13-vorher", "view");
  const from = h.screen!;
  const to = await scr(page, 30, 15);
  await mouseDrag(page, from, line(from, to, 12));
  await waitDragDone(page);
  const t = await text(page);
  expect(t).toContain("    line(to = g[c, v]);");
  await screenshot18(page, "z13-nachher", "view");
  record18({ id: "Z13", title: "Drag a handle", images: [{ name: "z13-vorher", label: "handles of all segments (drawing mode)" }, { name: "z13-nachher", label: "point dragged onto g[c, v]" }], code: t.trim(), caption: "In the drawing mode the view shows the handles of all segments of the sketch. Dragging uses the existing handle logic: on a grid point the reference is rewritten, literals are changed (Alt: numbers).", checks: ["g[b, v] → g[c, v]"] });
});

test("Z14 delete a segment", async ({ page }) => {
  await openSketch(page, RECT, 2);
  await tool(page, "select");
  const p = await scr(page, 30, 10);
  const before = (await info(page))!.answers;
  await page.mouse.move(p.x, p.y);
  await waitSketch(page, "(s, a) => s.answers > a && s.segment !== null", before);
  await page.mouse.click(p.x, p.y);
  await page.waitForFunction(() => (window as unknown as { __cad: { cursorLine(): number } }).__cad.cursorLine() === 4);
  await screenshot18(page, "z14-auswahl", "view");
  const a0 = (await info(page))!.actions;
  await page.keyboard.press("Delete");
  await waitSketch(page, "(s, n) => s.actions > n", a0);
  await waitScene(page);
  const t = await text(page);
  expect(t).toBe("linear_extrude(2) sketch {\n    start([0, 0]);\n    line(to = [30, 0]);\n    line(to = [0, 20]);\n    close();\n}\n");
  await screenshot18(page, "z14-geloescht", "view");
  record18({ id: "Z14", title: "Delete a segment", images: [{ name: "z14-auswahl", label: "select: segment under the mouse, a click sets the cursor" }, { name: "z14-geloescht", label: "after Del: line removed, the successor starts at the previous point" }], code: t.trim(), caption: "With the select tool a click on a segment sets the cursor into its line, Del (or “Löschen”, delete) removes the statement. The only segment of a contour takes the contour with it, a `corner()` right after it too.", checks: ["line(to = [30, 20]) removed"] });
});

test("Z15 sketch under a transform and in rotate_extrude", async ({ page }) => {
  const src = `translate([0, 0, 10]) rotate([90, 0, 0]) linear_extrude(3) sketch {
    start([0, 0]);
    line(to = [20, 0]);
    line(to = [20, 10]);
    close();
}
`;
  await openSketch(page, src, 4);
  const s = (await info(page))!;
  // plane: local y → world z, normal −y; the camera looks along +y
  const cam = await page.evaluate(() => (window as unknown as W).__cad.camera());
  expect(cam!.dir[1]).toBeGreaterThan(0.9999);
  expect(s.plane![6]).toBeCloseTo(1, 9);
  await tool(page, "line");
  const h = await hoverAt(page, 20, 10, "end");
  expect(h.snap!.point[2]).toBeCloseTo(20, 6);
  expect(h.snap!.point[1]).toBeCloseTo(0, 6);
  await clicks(page, [[20, 10], [10, 15]], 1);
  const t = await text(page);
  expect(t).toContain("    line(to = [20, 10]);\n    line(to = [10, 15]);\n    close();");
  await screenshot18(page, "z15-transformiert", "full");
  await page.keyboard.press("Escape");
  await page.keyboard.press("Escape");
  await page.waitForFunction(() => (window as unknown as W).__cad.sketch() === null);

  const rev = `rotate_extrude() sketch {
    start([10, 0]);
    line(to = [20, 0]);
    line(to = [20, 5]);
    close();
}
`;
  await loadText(page, rev);
  await clickLine(page, 4, 6);
  await waitScene(page);
  await enter(page);
  const cam2 = await page.evaluate(() => (window as unknown as W).__cad.camera());
  // xz-plane, normal −y: looking along +y
  expect(cam2!.dir[1]).toBeGreaterThan(0.9999);
  const h2 = await hoverAt(page, 20, 5, "end");
  expect(h2.snap!.point[2]).toBeCloseTo(5, 6);
  await clicks(page, [[20, 5], [15, 8]], 1);
  const t2 = await text(page);
  expect(t2).toContain("    line(to = [20, 5]);\n    line(to = [15, 8]);\n    close();");
  await screenshot18(page, "z15-rotate-extrude", "full");
  record18({
    id: "Z15",
    title: "Sketch under a transform, rotate_extrude",
    images: [
      { name: "z15-transformiert", label: "translate + rotate([90, 0, 0]): plane perpendicular to the y axis" },
      { name: "z15-rotate-extrude", label: "rotate_extrude: profile in the xz plane" },
    ],
    code: `${t.trim()}\n\n${t2.trim()}`,
    caption:
      "The plane of the sketch comes from the engine (`OpInfo.Sketch.plane`): under transforms its node system, in rotate_extrude the xz plane in which the profile lies in the body, with a centred linear_extrude its bottom. Camera, grid, snap and handles use the same plane; the values stay coordinates of the sketch.",
    checks: [`view y = ${cam!.dir[1].toFixed(5)}`, "snap point z = 20 (world)", `rotate_extrude: view y = ${cam2!.dir[1].toFixed(5)}`],
  });
});

test("Z16 dark theme", async ({ page }) => {
  await page.addInitScript(() => localStorage.setItem("cad-theme", "dark"));
  await openApp(page);
  await loadText(page, GRID);
  await clickLine(page, 6, 0);
  await waitScene(page);
  await enter(page);
  await tool(page, "rect");
  await clicks(page, [[0, 0]]);
  await hoverAt(page, 40, 25, "grid");
  await expect(page.getByTestId("sketch-snap-label")).toBeVisible();
  const contrast = await page.evaluate(() => (window as unknown as { __cad: { contrastOf(a: string, b: string): number | null } }).__cad.contrastOf("[data-testid=sketch-snap-label]", "[data-testid=sketch-snap-label]"));
  expect(contrast).toBeGreaterThan(4.5);
  await screenshot18(page, "z16-dunkel", "full");
  record18({ id: "Z16", title: "Dark theme", images: [{ name: "z16-dunkel", label: "drawing mode in the dark theme" }], caption: "Grid, preview, snap mark and label use the colours of the theme (snap label as with the handle snap).", checks: [`contrast of the snap label ${contrast!.toFixed(1)}:1`] });
});

test("Z17 answer times: snap and preview while moving (sketch with a loop, 42 segments)", async ({ page }) => {
  await openApp(page);
  await openExample(page, "spec/sketch_timing_pulley.mcad", null);
  await clickLine(page, 12, 10);
  await waitScene(page, { context: "pulley" });
  await enter(page);
  const s0 = (await info(page))!;
  expect(s0.sketch?.line).toBe(11);
  // a chain of lines from the centre: snap + preview on every move
  await clicks(page, [[0, 0]]);
  const n0 = (await info(page))!.rtt.length;
  const pts: Pt[] = [];
  for (let i = 0; i < 40; i++) {
    const a = (i / 40) * Math.PI * 2;
    pts.push(await scr(page, 5.5 * Math.cos(a), 5.5 * Math.sin(a)));
  }
  for (const p of pts) {
    const before = (await info(page))!.answers;
    await page.mouse.move(p.x, p.y);
    await waitSketch(page, "(s, a) => s.answers > a", before);
  }
  const s = (await info(page))!;
  const ms = s.rtt.slice(n0).filter((r) => !r.click).map((r) => r.ms).sort((a, b) => a - b);
  const med = ms[Math.floor(ms.length / 2)];
  const p95 = ms[Math.min(ms.length - 1, Math.floor(ms.length * 0.95))];
  expect(ms.length).toBeGreaterThanOrEqual(30);
  expect(med).toBeLessThan(150);
  await screenshot18(page, "z17-zahnscheibe", "full");
  writeMetrics18({ hover: { n: ms.length, medianMs: med, p95Ms: p95, maxMs: ms[ms.length - 1], segments: 42 } });
  record18({
    id: "Z17",
    title: "Answer times",
    images: [{ name: "z17-zahnscheibe", label: "timing pulley (sketch with a for loop) in the drawing mode" }],
    caption:
      "Every mouse move sends the ray to the engine (worker, Wasm): snap against grid points, all end, mid and intersection points of the 42 segments, preview; at most one request in flight, the last position wins. Measured: request to answer in the store.",
    checks: [`${ms.length} moves: median ${med.toFixed(1)} ms, 95 % ${p95.toFixed(1)} ms, max ${ms[ms.length - 1].toFixed(1)} ms`],
  });
});
