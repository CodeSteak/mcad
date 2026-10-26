// End-to-end tests for test report 14 (sketches and namespaces, Plan 17.1/17.2): the sketch
// under the cursor in its plane with the segment of the cursor line highlighted, help lines of
// Bézier curves, dragging a segment's end point changes its literal, errors per segment line,
// an open sketch as sweep path, and the path bar of a nested module.

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
  record14,
  screenshot14,
  waitDragDone,
  waitHandles,
  waitScene,
  waitView,
} from "./helpers";

test.beforeEach(async ({ page }) => {
  page.on("pageerror", (e) => console.log("pageerror:", e.message));
  // the colours panel closed: more room for the view in the screenshots
  await page.addInitScript(() => localStorage.setItem("cad-colors-open", "0"));
});

const text = (page: Page) => hook(page, (c) => c.text());
const grip = (h: HandleView): Pt => {
  if (!h.screen) throw new Error(`handle ${h.id} not visible`);
  return h.screen;
};
const sketches = (s: CadState) => s.view!.plan.sketches ?? [];

async function cursorOn(page: Page, l: number, col: number, context: string | null, pred = "() => true"): Promise<CadState> {
  await clickLine(page, l, col);
  await waitScene(page, { context });
  return waitView(page, `(s) => s.view.selection.line === ${l} && (${pred})(s)`);
}

const PROFILE = `linear_extrude(5) sketch {
    start([0, 0]);
    line(to = [40, 0]);
    arc(to = [50, 10], r = 10);
    line(by = [0, 20]);
    bezier(c1 = [45, 40], to = [20, 35]);
    close();
    hole {
        start([12, 12]);
        arc(to = [22, 12], r = 5);
        arc(to = [12, 12], r = 5);
    }
}
`;

test("S01 segment line: sketch in its plane, segment highlighted", async ({ page }) => {
  await openApp(page);
  await loadText(page, PROFILE);
  const s = await cursorOn(page, 4, 6, null, "(s) => (s.view.plan.sketches ?? []).length === 1");
  const sk = sketches(s)[0];
  expect(sk.closed).toBe(true);
  expect(sk.segments).toBe(7);
  expect(sk.active.map((a) => `${a.kind}@${a.line}`)).toEqual(["arc@4"]);
  // sketch drawn as profile (outline, light fill), the active segment thick in the accent colour
  expect(s.view!.plan.items.some((i) => i.mode === "profile" && i.label === "sketch")).toBe(true);
  const act = await layerPixels(page, "sketch:active");
  expect(act.found).toBeGreaterThan(0);
  expect(act.changed).toBeGreaterThan(30);
  const start = await layerPixels(page, "sketch:start");
  expect(start.found).toBe(2);
  await screenshot14(page, "s01-bogen", "view");
  // another line: the hole's arc
  const s2 = await cursorOn(page, 10, 10, null, "(s) => (s.view.plan.sketches ?? [])[0]?.active.length > 0");
  expect(sketches(s2)[0].active.map((a) => `${a.kind}@${a.line}`)).toEqual(["arc@10"]);
  await screenshot14(page, "s01-loch", "view");
  record14({
    id: "S01",
    title: "Sketch under the cursor",
    images: [
      { name: "s01-bogen", label: "cursor on arc(…) (line 4)" },
      { name: "s01-loch", label: "cursor in the hole block (line 10)" },
    ],
    code: PROFILE.trim(),
    caption:
      "Cursor on a segment line: the sketch is drawn in its plane – outline with a light fill (the engine's mesh, mode “profile”, visible through the body), the segments as lines, the line's segment thick in the accent colour; the start point of every contour as a ring with a direction arrow, arc centres as dashed help lines.",
    checks: [`active segment: arc (line 4), pixels ${act.changed}`, `start marks: ${start.found}`, "line 10: arc in the hole active"],
  });
});

test("S02 Bézier: control polygon, handle at the control point", async ({ page }) => {
  await openApp(page);
  await loadText(page, PROFILE);
  const s = await cursorOn(page, 6, 6, null, "(s) => (s.view.plan.sketches ?? [])[0]?.active.length > 0");
  const sk = sketches(s)[0];
  expect(sk.active.map((a) => a.kind)).toEqual(["bezier"]);
  expect(sk.helpers).toBeGreaterThanOrEqual(4);
  const hs = await waitHandles(page, "(hs) => hs.filter(h => h.kind === 'point').length === 2");
  expect(hs.map((h) => h.value)).toEqual(expect.arrayContaining([[20, 35], [45, 40]]));
  const help = await layerPixels(page, "sketch:help");
  expect(help.changed).toBeGreaterThan(10);
  await screenshot14(page, "s02-bezier");
  record14({
    id: "S02",
    title: "Bézier segment",
    images: [{ name: "s02-bezier", label: "cursor on bezier(c1 = …, to = …)" }],
    code: "bezier(c1 = [45, 40], to = [20, 35]);",
    caption: "Quadratic Bézier curve: control polygon (start – c1 – end) dashed as a help line, handles at the end point and at the control point.",
    checks: [`help lines: ${sk.helpers}`, `pixels of the help lines: ${help.changed}`, "2 point handles (to, c1)"],
  });
});

test("S03 dragging a handle changes the literal of the end point", async ({ page }) => {
  await openApp(page);
  await loadText(page, PROFILE);
  await cursorOn(page, 3, 6, null, "(s) => (s.view.plan.sketches ?? [])[0]?.active.length > 0");
  const hs = await waitHandles(page, "(hs) => hs.some(h => h.kind === 'point' && h.value[0] === 40)");
  const h = hs.find((x) => x.kind === "point" && x.value[0] === 40)!;
  await screenshot14(page, "s03-vorher", "view");
  const from = grip(h);
  await mouseDrag(page, from, line(from, { x: from.x + 70, y: from.y + 25 }, 14), { alt: true });
  await waitDragDone(page);
  await page.mouse.move(2, 2);
  const after = await text(page);
  const m = after.match(/line\(to = \[(-?[\d.]+), (-?[\d.]+)\]\);/)!;
  expect(m).not.toBeNull();
  expect(Number(m[1])).not.toBe(40);
  // the next segment starts at the new end point: the arc keeps its literals
  expect(after).toContain("arc(to = [50, 10], r = 10);");
  const s = await waitView(page, "(s) => (s.view.plan.sketches ?? []).length === 1");
  expect(sketches(s)[0].active.map((a) => a.kind)).toEqual(["line"]);
  await screenshot14(page, "s03-nachher", "view");
  record14({
    id: "S03",
    title: "Drag the end point",
    images: [
      { name: "s03-vorher", label: "before: line(to = [40, 0])" },
      { name: "s03-nachher", label: `after: line(to = [${m[1]}, ${m[2]}])` },
    ],
    code: `// before\nline(to = [40, 0]);\n// after the drag (Alt, free)\n${m[0]}`,
    caption: "The point handle at the end of the straight line dragged with the mouse (Alt: free, grid 1 mm): the literal in the line changes, the sketch follows live; the following arc starts at the new point.",
    checks: [`literal: [40, 0] → [${m[1]}, ${m[2]}]`, "arc line unchanged"],
  });
});

test("S04 errors per segment line", async ({ page }) => {
  await openApp(page);
  const src = "linear_extrude(2) sketch {\n    start([0, 0]);\n    line(to = [30, 0]);\n    arc(to = [30, 20], r = 5);\n    line(to = [0, 20]);\n}\n";
  await loadText(page, src);
  const s = await waitView(page, "(s) => s.diagnostics.some(d => d.severity === 'error')", null, { meshes: false });
  const errs = s.diagnostics.filter((d) => d.severity === "error").map((d) => `${d.line}: ${d.message}`);
  expect(errs.some((e) => e.startsWith("4:") && e.includes("halbe Sehne"))).toBe(true);
  expect(errs.some((e) => e.startsWith("5:") && e.includes("nicht geschlossen"))).toBe(true);
  await expect(lineEl(page, 4)).toHaveClass(/cm-l-error/);
  await expect(lineEl(page, 5)).toHaveClass(/cm-l-error/);
  await cursorOn(page, 5, 6, null, "(s) => (s.view.plan.sketches ?? []).length === 1");
  await screenshot14(page, "s04-fehler");
  record14({
    id: "S04",
    title: "Errors per segment line",
    images: [{ name: "s04-fehler", label: "impossible arc, contour not closed" }],
    code: src.trim(),
    caption: "An arc with a radius smaller than half the chord and a contour without close(): both errors are at their segment line (red, message at the line end); the sketch stays visible with the missing closing line (red, dashed).",
    checks: errs,
  });
});

test("S05 open sketch as a sweep path, namespace in the path bar", async ({ page }) => {
  await openApp(page);
  const src = `module parts {
    module pipe(r = 4) {
        sweep() {
            sketch(closed = false) {
                start([0, 0]);
                line(to = [30, 0]);
                arc(to = [45, 15]);
                line(by = [0, 20]);
            }
            circle(r);
        }
    }
}
parts.pipe(r = 3);
`;
  await loadText(page, src);
  const s = await cursorOn(page, 7, 18, "parts.pipe", "(s) => (s.view.plan.sketches ?? []).length === 1");
  const sk = sketches(s)[0];
  expect(sk.closed).toBe(false);
  expect(sk.open).toBe(1);
  expect(sk.active.map((a) => a.kind)).toEqual(["arc"]);
  expect(s.path).toEqual(["parts", "pipe"]);
  await expect(page.getByRole("navigation", { name: "Pfadleiste" })).toContainText("parts");
  await expect(page.getByRole("navigation", { name: "Pfadleiste" })).toContainText("pipe");
  const end = await layerPixels(page, "sketch:end");
  expect(end.found).toBe(1);
  await screenshot14(page, "s05-pfad", "view");
  // the call from outside with a dot: the tube in the file context
  const f = await cursorOn(page, 14, 2, null, "(s) => s.view.plan.items.some(i => i.label === 'pipe')");
  expect(f.view!.plan.items.find((i) => i.label === "pipe")!.mode).toBe("full");
  await screenshot14(page, "s05-aufruf", "view");
  record14({
    id: "S05",
    title: "Sweep path and namespace",
    images: [
      { name: "s05-pfad", label: "context parts › pipe, cursor on the arc of the path" },
      { name: "s05-aufruf", label: "call parts.pipe(r = 3) in the file" },
    ],
    code: src.trim(),
    caption:
      "An open sketch (closed = false) is the path of the sweep: without a fill, start as a ring, end as a square; the circle runs along line – tangent arc – line (one curve chain). The module pipe is in the namespace parts: the path bar shows “parts › pipe”, from outside it is called as parts.pipe(r = 3).",
    checks: [`path bar: ${s.path.join(" › ")}`, "open contour: 1, end mark drawn", "call in the file shown full"],
  });
});
