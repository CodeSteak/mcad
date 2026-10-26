// End-to-end tests of the browser UI, Phase E (test report 7): variable fillet radius
// (`fillet(r = function (p) …)`, `difference(fillet = function (p) …)`) with its inline hint,
// errors of a radius law, shells of rounded bodies, seams and poles that are no edges, and
// surfaces from 2 x 2 / 3 x 3 points. Checks run on the UI state (`window.__cad`) and on canvas
// pixels; each scenario records screenshots and the code excerpt for the report.

import { expect, test, type Page } from "@playwright/test";
import {
  type CadState,
  clickLine,
  hook,
  layerPixels,
  lineEl,
  lines,
  loadText,
  nodesOnLine,
  openApp,
  openExample,
  recordE,
  screenshotE,
  waitScene,
  waitView,
} from "./helpers";

test.beforeEach(async ({ page }) => {
  page.on("pageerror", (e) => console.log("pageerror:", e.message));
});

const text = (page: Page) => hook(page, (c) => c.text());
const lineHint = (page: Page, l: number) => lineEl(page, l).locator(".cm-lineend");
const view = (s: CadState) => s.view!;

async function cursorOn(page: Page, l: number, col: number, context: string | null, pred = "() => true"): Promise<CadState> {
  await clickLine(page, l, col);
  await waitScene(page, { context });
  return waitView(page, `(s) => s.view.selection.line === ${l} && (${pred})(s)`);
}

test("E01 variable radius: hint “r 2–4”, edges orange, result rounded", async ({ page }) => {
  await openApp(page);
  let s = await openExample(page, "spec/variable_radius.mcad");
  expect(s.diagnostics.filter((d) => d.severity !== "info")).toEqual([]);
  const src = await text(page);
  // step law on all twelve edges of the box
  s = await cursorOn(page, 11, 3, null, "(s) => s.view.edgeKinds.selected > 0");
  const [f] = (await nodesOnLine(page, 11)).filter((x) => x.label === "fillet");
  expect(f.status).toBe("ok");
  expect(f.hints).toEqual(["12 Kanten", "r 2–4"]);
  expect(view(s).edgeKinds.selected).toBe(12);
  await expect(lineHint(page, 11)).toContainText("12 Kanten · r 2–4");
  // a rounded result: far more triangles than the 12 of a box
  expect(f.triangles ?? 0).toBeGreaterThan(200);
  const orange = (await layerPixels(page, "edge:selected")).changed;
  expect(orange).toBeGreaterThan(300);
  await screenshotE(page, "e01-stufe");
  // difference(fillet = function): the four new rim edges, r from 1.125 to 1.875
  s = await cursorOn(page, 16, 30, null, "(s) => s.view.edgeKinds.selected > 0");
  const [d] = (await nodesOnLine(page, 16)).filter((x) => x.label === "difference");
  expect(d.status).toBe("ok");
  expect(d.hints).toEqual(["4 Kanten", "r 1.125–1.875"]);
  expect(view(s).edgeKinds.selected).toBe(4);
  await expect(lineHint(page, 16)).toContainText("r 1.125–1.875");
  await screenshotE(page, "e01-tasche");
  // zone block with a lookup law: the four top edges, zone blue as with a constant radius
  s = await cursorOn(page, 22, 30, null, "(s) => s.view.edgeKinds.selected > 0");
  const [z] = (await nodesOnLine(page, 22)).filter((x) => x.label === "fillet");
  expect(z.hints).toEqual(["1 Zone", "4 Kanten", "r 1.5–3.9"]);
  expect(view(s).edgeKinds.selected).toBe(4);
  expect(view(s).plan.items.some((i) => i.mode === "blue" && i.line === 24)).toBe(true);
  await screenshotE(page, "e01-lookup");
  recordE({
    id: "E01",
    title: "Variable radius in the browser",
    images: [
      { name: "e01-stufe", label: "cursor on fillet(r = function (p) p.z < g.mid ? 2 : 4)" },
      { name: "e01-tasche", label: "cursor on difference(fillet = function (p) 1 + p.x / 40)" },
      { name: "e01-lookup", label: "cursor on fillet(r = function (p) lookup(p.x, …)) with a zone" },
    ],
    code: lines(src, 10, 25),
    caption:
      "variable_radius.mcad: the radius is a function of the position. The line end shows the edge count and the radius range of the evaluated stations (“12 Kanten · r 2–4”, “4 Kanten · r 1.125–1.875”, “1 Zone · 4 Kanten · r 1.5–3.9”); selected edges orange, zone blue as with a constant radius.",
    checks: [
      `status ok, hints ${JSON.stringify(f.hints)}, ${JSON.stringify(d.hints)}, ${JSON.stringify(z.hints)}`,
      `drawn selected edges 12 / 4 / 4, pixels of the orange edges (step): ${orange}`,
      `triangles of the rounded box: ${f.triangles}`,
    ],
  });
});

test("E02 variable radius: radius ≤ 0 and a radius too large as with a constant radius", async ({ page }) => {
  await openApp(page);
  const src = "fillet(r = function (p) p.x - 5) {\n    cube(20);\n}\ntranslate([40, 0, 0]) fillet(r = function (p) 8 + p.z / 2) {\n    cube(20);\n}\n";
  await loadText(page, src);
  let s = await cursorOn(page, 1, 3, null, "(s) => s.view.edgeKinds.error > 0");
  await expect(lineEl(page, 1)).toHaveClass(/cm-l-error/);
  await expect(lineHint(page, 1)).toContainText("≤ 0 bei");
  const [a] = (await nodesOnLine(page, 1)).filter((x) => x.label === "fillet");
  expect(a.status).toBe("error");
  expect(a.triangles).toBe(12);
  // the edge with the station where r <= 0 (the other block's failed edges are drawn as well)
  expect(a.messages[0].edges).toBe(1);
  expect(view(s).edgeKinds.error).toBeGreaterThanOrEqual(1);
  await screenshotE(page, "e02-negativ");
  s = await cursorOn(page, 4, 30, null, "(s) => s.view.edgeKinds.error > 0");
  await expect(lineEl(page, 4)).toHaveClass(/cm-l-error/);
  await expect(lineHint(page, 4)).toContainText("Verrundung gescheitert (8–18 mm)");
  const [b] = (await nodesOnLine(page, 4)).filter((x) => x.label === "fillet");
  expect(b.status).toBe("error");
  expect(b.triangles).toBe(12);
  await screenshotE(page, "e02-zu-gross");
  recordE({
    id: "E02",
    title: "Error cases of the variable radius",
    images: [
      { name: "e02-negativ", label: "radius ≤ 0 at an edge" },
      { name: "e02-zu-gross", label: "radius 8–18 at a 20 mm cube" },
    ],
    code: src.trim(),
    caption:
      "Radius ≤ 0 at a station: line red, message with value and position, the edge red, the cube stays without a fillet (12 triangles). Radius too large: kernel message with the radius range, body unchanged – as with a constant radius.",
    checks: [`status error/error, triangles ${a.triangles}/${b.triangles}`, `message: ${a.messages[0]?.message ?? ""}`],
  });
});

test("E03 shell of a rounded body: can_shell.mcad and the three cases from report 4", async ({ page }) => {
  await openApp(page);
  let s = await openExample(page, "spec/can_shell.mcad");
  expect(s.diagnostics.filter((d) => d.severity === "error")).toEqual([]);
  s = await cursorOn(page, 9, 6, "can", "(s) => s.view.openFaces.length > 0");
  const [sh] = (await nodesOnLine(page, 9)).filter((x) => x.label === "shell");
  expect(sh.status).toBe("ok");
  expect(sh.triangles ?? 0).toBeGreaterThan(500);
  expect(view(s).openFaces).toEqual([expect.objectContaining({ faces: 1, engine: 1 })]);
  await expect(lineHint(page, 9)).toContainText("1 Zone · 1 Fläche geöffnet");
  await screenshotE(page, "e03-dose");
  const src =
    "grid g { z: o = 18; }\n" +
    "grid h { z: o = 30; }\n" +
    "shell(1.6) { box([0, 0, 0], [90, 34, 18], $fillet = 3); zone(g.o, d = 1); }\n" +
    "translate([0, 45, 0]) shell(1.6) { box([0, 0, 0], [90, 34, 18], $fillet = 5); zone(g.o, d = 1); }\n" +
    "translate([0, 90, 0]) shell(1.5) { box([0, 0, 0], [50, 35, 30], $fillet = 4); zone(h.o, d = 1); }\n";
  s = await loadText(page, src);
  expect(s.diagnostics.filter((d) => d.severity === "error")).toEqual([]);
  const shells = [];
  for (const l of [3, 4, 5]) {
    const [n] = (await nodesOnLine(page, l)).filter((x) => x.label === "shell");
    expect(n.status).toBe("ok");
    expect(n.openedFaces).toBe(1);
    shells.push(n);
  }
  await cursorOn(page, 4, 30, null, "(s) => s.view.openFaces.length > 0");
  const [mid] = (await nodesOnLine(page, 4)).filter((x) => x.label === "shell");
  expect(mid.triangles ?? 0).toBeGreaterThan(500);
  await screenshotE(page, "e03-faelle");
  recordE({
    id: "E03",
    title: "shell of rounded bodies",
    images: [
      { name: "e03-dose", label: "can_shell.mcad, cursor on shell(wall)" },
      { name: "e03-faelle", label: "the three cases from report 4 (F1)" },
    ],
    code: src.trim(),
    caption:
      "can_shell.mcad is green again: a closed hollow body with an opening at the top, “1 Zone · 1 Fläche geöffnet” (1 zone · 1 face opened). The three cases from report 4 (formerly SM_ERR_NULL_POINTER, “leaves no cavity”, “self-overlapping”) give hollow bodies.",
    checks: [`can: ${sh.triangles} triangles, ${sh.openedFaces} face opened (view and engine)`, `cases: status ${shells.map((x) => x.status).join("/")}, ${shells.map((x) => x.openedFaces).join("/")} face opened each; middle ${mid.triangles} triangles`],
  });
});

test("E04 seam and pole edges do not count: cylinder 2 edges, sphere 0 edges", async ({ page }) => {
  await openApp(page);
  const src = "cylinder(h = 10, r = 5, $chamfer = 1);\ntranslate([20, 0, 0]) sphere(5, $fillet = 1);\ntranslate([40, 0, 0]) fillet(1) { cylinder(h = 10, r = 5); sphere(20); }\n";
  await loadText(page, src);
  let s = await cursorOn(page, 1, 3, null, "(s) => s.view.edgeKinds.selected > 0");
  expect(view(s).edgeKinds.selected).toBe(2);
  await expect(lineHint(page, 1)).toContainText("2 Kanten");
  await screenshotE(page, "e04-zylinder", "view");
  s = await cursorOn(page, 2, 30, null);
  await expect(lineEl(page, 2)).toHaveClass(/cm-l-warning/);
  await expect(lineHint(page, 2)).toContainText("0 Kanten");
  const [sp] = (await nodesOnLine(page, 2)).filter((x) => x.label === "sphere");
  expect(sp.status).toBe("warning");
  expect(view(s).edgeKinds.selected ?? 0).toBe(0);
  s = await cursorOn(page, 3, 30, null, "(s) => s.view.edgeKinds.selected > 0");
  expect(view(s).edgeKinds.selected).toBe(2);
  await expect(lineHint(page, 3)).toContainText("1 Zone · 2 Kanten");
  await screenshotE(page, "e04-zone", "view");
  recordE({
    id: "E04",
    title: "Seam and pole edges",
    images: [
      { name: "e04-zylinder", label: "cylinder(…, $chamfer = 1): top and bottom edge" },
      { name: "e04-zone", label: "fillet(1) with a sphere zone around the cylinder" },
    ],
    code: src.trim(),
    caption:
      "The seam of the cylinder and the seam/poles of the sphere are smooth and never selected: $chamfer on the cylinder “2 Kanten” (2 edges, before 3), $fillet on the sphere “0 Kanten” as a warning without effect (before an error), a zone selection around the whole cylinder also 2 edges.",
    checks: ["drawn edges 2 / 0 / 2", "sphere: status warning, line yellow"],
  });
});

test("E05 surface from 2 × 2 and 3 × 3 points", async ({ page }) => {
  await openApp(page);
  const src =
    "thicken(1) surface(points = [[[0, 0, 0], [20, 0, 6]], [[0, 20, 6], [20, 20, 0]]]);\n" +
    "translate([30, 0, 0]) thicken(1) surface(points = [[[0, 0, 0], [10, 0, 6], [20, 0, 0]], [[0, 10, 6], [10, 10, 9], [20, 10, 6]], [[0, 20, 0], [10, 20, 6], [20, 20, 0]]]);\n";
  const s = await loadText(page, src);
  expect(s.diagnostics.filter((d) => d.severity === "error")).toEqual([]);
  for (const l of [1, 2]) {
    const [n] = (await nodesOnLine(page, l)).filter((x) => x.label === "surface");
    expect(n.status).toBe("ok");
    const [t] = (await nodesOnLine(page, l)).filter((x) => x.label === "thicken");
    expect(t.status).toBe("ok");
  }
  await cursorOn(page, 2, 30, null, "(s) => s.view.plan.items.length > 0");
  const [sq] = (await nodesOnLine(page, 2)).filter((x) => x.label === "thicken");
  expect(sq.triangles ?? 0).toBeGreaterThan(8);
  await screenshotE(page, "e05-surface", "view");
  recordE({
    id: "E05",
    title: "surface from few points",
    images: [{ name: "e05-surface", label: "bilinear (2 × 2) and quadratic (3 × 3)" }],
    code: src.trim(),
    caption: "surface(points) with 2 × 2 (bilinear) and 3 × 3 points (quadratic) is created and can be thickened (before SmApiCreateSurfaceFromOrderedPoints … SM_ERR).",
    checks: ["both surface nodes ok"],
  });
});
