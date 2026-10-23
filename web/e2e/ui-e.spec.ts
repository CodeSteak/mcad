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

test("E01 Variabler Radius: Hinweis „r 2–4“, Kanten orange, Ergebnis verrundet", async ({ page }) => {
  await openApp(page);
  let s = await openExample(page, "spec/variabler_radius.scad");
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
    title: "Variabler Radius im Browser",
    images: [
      { name: "e01-stufe", label: "Cursor auf fillet(r = function (p) p.z < g.mitte ? 2 : 4)" },
      { name: "e01-tasche", label: "Cursor auf difference(fillet = function (p) 1 + p.x / 40)" },
      { name: "e01-lookup", label: "Cursor auf fillet(r = function (p) lookup(p.x, …)) mit Zone" },
    ],
    code: lines(src, 10, 25),
    caption:
      "variabler_radius.scad: der Radius ist eine Funktion der Position. Am Zeilenende stehen Kantenzahl und Radiusbereich der ausgewerteten Stützpunkte („12 Kanten · r 2–4“, „4 Kanten · r 1.125–1.875“, „1 Zone · 4 Kanten · r 1.5–3.9“); ausgewählte Kanten orange, Zone blau wie beim konstanten Radius.",
    checks: [
      `Status ok, Hinweise ${JSON.stringify(f.hints)}, ${JSON.stringify(d.hints)}, ${JSON.stringify(z.hints)}`,
      `gezeichnete ausgewählte Kanten 12 / 4 / 4, Pixel der orangen Kanten (Stufe): ${orange}`,
      `Dreiecke der verrundeten Box: ${f.triangles}`,
    ],
  });
});

test("E02 Variabler Radius: Radius ≤ 0 und zu großer Radius wie beim konstanten Radius", async ({ page }) => {
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
    title: "Fehlerfälle des variablen Radius",
    images: [
      { name: "e02-negativ", label: "Radius ≤ 0 an einer Kante" },
      { name: "e02-zu-gross", label: "Radius 8–18 an einem 20-mm-Würfel" },
    ],
    code: src.trim(),
    caption:
      "Radius ≤ 0 an einem Stützpunkt: Zeile rot, Meldung mit Wert und Position, die Kante rot, der Würfel bleibt ohne Verrundung (12 Dreiecke). Zu großer Radius: Kernel-Meldung mit dem Radiusbereich, Körper unverändert – wie beim konstanten Radius.",
    checks: [`Status error/error, Dreiecke ${a.triangles}/${b.triangles}`, `Meldung: ${a.messages[0]?.message ?? ""}`],
  });
});

test("E03 shell eines verrundeten Körpers: dose_shell.scad und die drei Fälle aus Bericht 4", async ({ page }) => {
  await openApp(page);
  let s = await openExample(page, "spec/dose_shell.scad");
  expect(s.diagnostics.filter((d) => d.severity === "error")).toEqual([]);
  s = await cursorOn(page, 9, 6, "dose", "(s) => s.view.openFaces.length > 0");
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
    title: "shell verrundeter Körper",
    images: [
      { name: "e03-dose", label: "dose_shell.scad, Cursor auf shell(wand)" },
      { name: "e03-faelle", label: "die drei Fälle aus Bericht 4 (F1)" },
    ],
    code: src.trim(),
    caption:
      "dose_shell.scad ist wieder grün: ein geschlossener Hohlkörper mit Öffnung oben, „1 Zone · 1 Fläche geöffnet“. Die drei Fälle aus Bericht 4 (früher SM_ERR_NULL_POINTER, „leaves no cavity“, „self-overlapping“) ergeben Hohlkörper.",
    checks: [`dose: ${sh.triangles} Dreiecke, ${sh.openedFaces} Fläche geöffnet (Ansicht und Engine)`, `Fälle: Status ${shells.map((x) => x.status).join("/")}, je ${shells.map((x) => x.openedFaces).join("/")} Fläche geöffnet; Mitte ${mid.triangles} Dreiecke`],
  });
});

test("E04 Naht- und Polkanten zählen nicht: Zylinder 2 Kanten, Kugel 0 Kanten", async ({ page }) => {
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
    title: "Naht- und Polkanten",
    images: [
      { name: "e04-zylinder", label: "cylinder(…, $chamfer = 1): Deckel- und Bodenkante" },
      { name: "e04-zone", label: "fillet(1) mit Kugelzone um den Zylinder" },
    ],
    code: src.trim(),
    caption:
      "Die Naht des Zylinders und die Naht/Pole der Kugel sind glatt und werden nie ausgewählt: $chamfer am Zylinder „2 Kanten“ (vorher 3), $fillet an der Kugel „0 Kanten“ als Warnung ohne Wirkung (vorher Fehler), Zonenauswahl um den ganzen Zylinder ebenfalls 2 Kanten.",
    checks: ["gezeichnete Kanten 2 / 0 / 2", "Kugel: Status warning, Zeile gelb"],
  });
});

test("E05 surface aus 2 × 2 und 3 × 3 Punkten", async ({ page }) => {
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
    title: "surface aus wenigen Punkten",
    images: [{ name: "e05-surface", label: "bilinear (2 × 2) und quadratisch (3 × 3)" }],
    code: src.trim(),
    caption: "surface(points) mit 2 × 2 (bilinear) und 3 × 3 Punkten (quadratisch) wird erzeugt und lässt sich aufdicken (vorher SmApiCreateSurfaceFromOrderedPoints … SM_ERR).",
    checks: ["beide surface-Knoten ok"],
  });
});
