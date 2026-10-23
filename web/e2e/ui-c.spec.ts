// End-to-end tests of the browser UI, Phase C: display of fillet/chamfer/shell (zones, selected
// edges, opened faces, empty selection, failure) and of the profile operations (linear_extrude,
// rotate_extrude, loft, sweep, surface, thicken), 2D shapes alone, refitting the view after a
// clear size change, axis-constrained point drags and the new examples. Every check runs on the
// UI state (`window.__cad`) or on pixels of the canvas; each scenario records screenshots and
// the code excerpt for test report 4.

import { expect, test, type Page } from "@playwright/test";
import {
  type CadState,
  type HandleView,
  type Pt,
  cad,
  clickLine,
  colorPixels,
  frame,
  layerPixels,
  hook,
  line,
  lineEl,
  lines,
  loadText,
  mouseDrag,
  nodesOnLine,
  openApp,
  openExample,
  recordC,
  screenshotC,
  waitDragDone,
  waitHandles,
  waitScene,
  waitView,
  writeMetricsC,
} from "./helpers";

test.beforeEach(async ({ page }) => {
  page.on("pageerror", (e) => console.log("pageerror:", e.message));
});

const text = (page: Page) => hook(page, (c) => c.text());
const grip = (h: HandleView): Pt => {
  if (!h.screen) throw new Error(`handle ${h.id} not visible`);
  return h.screen;
};
const waitText = (page: Page, pred: string) =>
  page.waitForFunction((src) => new Function("t", `return (${src})(t);`)((window as unknown as { __cad: { text(): string } }).__cad.text()), pred, { timeout: 30_000, polling: 30 });
const lineHint = (page: Page, l: number) => lineEl(page, l).locator(".cm-lineend");
const view = (s: CadState) => s.view!;
type NodeLite = { status: string; label: string; range: { line: number }; messages: Array<{ message: string }>; blend_suspended?: boolean };
/** Nodes of the last complete scene (store). */
const completeNodes = (page: Page) =>
  page.evaluate(() => (window as unknown as { __cad: { store: { getState(): { complete: { nodes: NodeLite[] } | null } } } }).__cad.store.getState().complete?.nodes ?? []);
/** Display modes of the plan (without the face under a resting mouse). */
const modes = (s: CadState) => view(s).plan.items.filter((i) => i.mode !== "picked").map((i) => `${i.mode}@${i.line}`);

/** Opens an example and puts the cursor on a line; waits until the selection is drawn. */
async function cursorOn(page: Page, l: number, col: number, context: string | null, pred = "() => true"): Promise<CadState> {
  await clickLine(page, l, col);
  await waitScene(page, { context });
  return waitView(page, `(s) => s.view.selection.line === ${l} && (${pred})(s)`);
}

// ---------------------------------------------------------------- fillet, chamfer, shell

test("C01 fillet-Zeile: Zonen blau, ausgewählte Kanten farbig, Anzahl am Zeilenende", async ({ page }) => {
  await openApp(page);
  await openExample(page, "spec/fillet_zonen.scad");
  const s = await cursorOn(page, 12, 10, "block", "(s) => s.view.plan.items.some(i => i.mode === 'blue')");
  expect(modes(s)).toEqual(["full@12", "blue@17", "blue@18"]);
  expect(view(s).edgeKinds.selected).toBe(2);
  await expect(lineHint(page, 12)).toContainText("2 Zonen · 2 Kanten");
  const orange = (await layerPixels(page, "edge:selected")).changed;
  const blue = (await layerPixels(page, "mode:blue")).changed;
  expect(orange).toBeGreaterThan(80);
  expect(blue).toBeGreaterThan(500);
  await screenshotC(page, "c01-fillet");
  recordC({
    id: "C01",
    title: "fillet-Zeile",
    images: [{ name: "c01-fillet", label: "Cursor auf fillet(2)" }],
    code: lines(await text(page), 11, 21),
    caption: "fillet_zonen.scad, Modul block, Cursor auf fillet(2) (Zeile 12): Körper voll, Rest gedimmt; die beiden Zonen (Zylinder um die Rasterlinie g[rechts, *, deckel] und die Kugel über der Bohrung) blau halbtransparent, die zwei ausgewählten Kanten orange; am Zeilenende „2 Zonen · 2 Kanten“.",
    checks: [`Darstellung: ${modes(s).join(", ")}`, `ausgewählte Kanten gezeichnet: ${view(s).edgeKinds.selected}`, `Pixel der Kanten: ${orange}, der Zonen: ${blue} (Differenz mit/ohne Ebene)`],
  });
});

test("C02 Cursor auf einer Zone: nur diese Zone und ihre Kanten", async ({ page }) => {
  await openApp(page);
  await openExample(page, "spec/fillet_zonen.scad");
  let s = await cursorOn(page, 18, 6, "block", "(s) => s.view.zoneEdges !== null");
  expect(modes(s)).toEqual(["blue@18"]);
  expect(view(s).zoneEdges).toMatchObject({ inZone: 1, total: 2 });
  expect(view(s).edgeKinds.selected).toBe(1);
  await screenshotC(page, "c02-zone-kugel");
  s = await cursorOn(page, 17, 6, "block", "(s) => s.view.zoneEdges !== null");
  expect(modes(s)).toEqual(["blue@17"]);
  expect(view(s).zoneEdges).toMatchObject({ inZone: 1, total: 2 });
  await screenshotC(page, "c02-zone-linie");
  recordC({
    id: "C02",
    title: "Cursor auf einer Zone",
    images: [
      { name: "c02-zone-kugel", label: "Zone Kugel (Zeile 18)" },
      { name: "c02-zone-linie", label: "Zone Rasterlinie (Zeile 17)" },
    ],
    code: lines(await text(page), 12, 19),
    caption: "Cursor auf einer Zone des Blocks fillet(2): nur diese Zone ist blau, von den zwei ausgewählten Kanten des Blocks ist nur die Kante in dieser Zone orange (Kugel: obere Bohrungskante; Rasterlinie: obere rechte Kante). Der Griff am Zonenmittelpunkt (translate) snappt auf Raster (Phase B, B10).",
    checks: ["Zone Kugel: 1 von 2 Kanten, nur blue@18", "Zone Rasterlinie: 1 von 2 Kanten, nur blue@17"],
  });
});

test("C03 Leere Auswahl: Zeile gelb, „0 Kanten“", async ({ page }) => {
  await openApp(page);
  const src = "fillet(2) {\n    cube(20);\n    translate([20, 0, 20]) sphere(3);   // berührt nur die Ecke\n}\n";
  await loadText(page, src);
  const s = await cursorOn(page, 1, 3, null, "(s) => s.view.plan.items.length > 0");
  await expect(lineEl(page, 1)).toHaveClass(/cm-l-warning/);
  await expect(lineHint(page, 1)).toContainText("0 Kanten");
  const n = (await nodesOnLine(page, 1)).find((x) => x.label === "fillet")!;
  expect(n.status).toBe("warning");
  expect(view(s).edgeKinds.selected ?? 0).toBe(0);
  expect(s.diagnostics.some((d) => d.severity === "warning" && d.line === 1)).toBe(true);
  await screenshotC(page, "c03-leer", "full");
  recordC({
    id: "C03",
    title: "Leere Auswahl",
    images: [{ name: "c03-leer", label: "Editor und Ansicht" }],
    code: src.trim(),
    caption: "Die Kugel berührt den Würfel nur an der Ecke, keine Kante liegt vollständig in der Zone: Zeile 1 gelb markiert, am Zeilenende „1 Zone · 0 Kanten“ und die Warnung; die Ansicht zeigt den Würfel unverändert (Warnung, kein Fehler).",
    checks: ["Zeilenklasse cm-l-warning", "Knotenstatus warning", "keine ausgewählten Kanten gezeichnet"],
  });
});

test("C04 Verrundung gescheitert: Zeile rot, Kante rot, Körper ohne Verrundung", async ({ page }) => {
  await openApp(page);
  const src = "fillet(25) {\n    cube(20);\n    translate([20, 10, 20]) cube([2, 22, 2], center = true);\n}\n";
  await loadText(page, src);
  const s = await cursorOn(page, 1, 3, null, "(s) => s.view.edgeKinds.error > 0");
  await expect(lineEl(page, 1)).toHaveClass(/cm-l-error/);
  await expect(lineHint(page, 1)).toContainText("Verrundung gescheitert");
  const [f] = (await nodesOnLine(page, 1)).filter((x) => x.label === "fillet");
  expect(f.status).toBe("error");
  expect(f.messages[0].edges).toBe(1);
  // The node shows its input without the operation: the plain cube (12 triangles).
  expect(f.triangles).toBe(12);
  expect(view(s).edgeKinds.error).toBe(1);
  const red = (await layerPixels(page, "edge:error")).changed;
  expect(red).toBeGreaterThan(60);
  await screenshotC(page, "c04-fehler", "full");
  recordC({
    id: "C04",
    title: "Verrundung gescheitert",
    images: [{ name: "c04-fehler", label: "Editor und Ansicht" }],
    code: src.trim(),
    caption: "fillet(25) an einer Kante eines 20-mm-Würfels: Zeile 1 rot mit der Kernel-Meldung am Zeilenende, die betroffene Kante rot; die Ansicht zeigt den Würfel ohne Verrundung (Mesh des Knotens = Mesh des Körpers, 12 Dreiecke).",
    checks: [`Knotenstatus error, 1 betroffene Kante`, `Mesh des fillet-Knotens: ${f.triangles} Dreiecke (unverrundeter Würfel)`, `Pixel der roten Kante: ${red}`],
  });
});

test("C05 difference(fillet = r) und $fillet am Primitiv", async ({ page }) => {
  await openApp(page);
  const src = "difference(fillet = 1) {\n    cube(20);\n    translate([10, 10, -1]) cylinder(h = 30, r = 4);\n}\ntranslate([30, 0, 0]) cube(10, $fillet = 1);\ntranslate([50, 0, 0]) cylinder(h = 10, r = 5, $chamfer = 1);\n";
  await loadText(page, src);
  let s = await cursorOn(page, 1, 3, null, "(s) => s.view.edgeKinds.selected > 0");
  expect(view(s).edgeKinds.selected).toBe(2);
  await expect(lineHint(page, 1)).toContainText("2 Kanten");
  expect(modes(s)).toEqual(["full@1", "red@3"]);
  await screenshotC(page, "c05-difference");
  s = await cursorOn(page, 5, 30, null, "(s) => s.view.edgeKinds.selected > 0");
  expect(view(s).edgeKinds.selected).toBe(12);
  await expect(lineHint(page, 5)).toContainText("12 Kanten");
  await screenshotC(page, "c05-dollar-fillet");
  s = await cursorOn(page, 6, 30, null, "(s) => s.view.edgeKinds.selected > 0");
  const cyl = (await nodesOnLine(page, 6)).find((x) => x.label === "cylinder")!;
  // The engine also counts the seam of the cylinder (3 instead of 2, see report: found bugs).
  expect(view(s).edgeKinds.selected).toBe(cyl.selectedEdges);
  await expect(lineHint(page, 6)).toContainText(`${cyl.selectedEdges} Kanten`);
  recordC({
    id: "C05",
    title: "fillet = an Booleans, $fillet/$chamfer an Primitiven",
    images: [
      { name: "c05-difference", label: "difference(fillet = 1)" },
      { name: "c05-dollar-fillet", label: "cube(10, $fillet = 1)" },
    ],
    code: src.trim(),
    caption: "difference(fillet = 1): die zwei durch die Bohrung neu entstandenen Kanten orange, abgezogener Zylinder rot halbtransparent, „2 Kanten“ am Zeilenende. $fillet = 1 am Würfel: alle 12 eigenen Kanten, „12 Kanten“; $chamfer = 1 am Zylinder: die Kanten, die die Engine meldet (3: oben, unten und die Naht – siehe gefundene Fehler).",
    checks: ["difference: 2 Kanten gezeichnet", "$fillet: 12 Kanten", `$chamfer am Zylinder: ${cyl.selectedEdges} Kanten wie gemeldet`],
  });
});

test("C06 select = \"touch\" und verschachtelte Blöcke", async ({ page }) => {
  await openApp(page);
  await openExample(page, "spec/fillet_zonen.scad");
  // touch: the zone (vertical grid line) only touches four of the edges.
  let s = await cursorOn(page, 23, 30, "block", "(s) => s.view.edgeKinds.selected > 0");
  expect(view(s).edgeKinds.selected).toBe(5);
  await expect(lineHint(page, 23)).toContainText("5 Kanten");
  await screenshotC(page, "c06-touch");
  s = await cursorOn(page, 25, 8, "block", "(s) => s.view.zoneEdges !== null");
  expect(view(s).zoneEdges).toMatchObject({ inZone: 5, total: 5 });
  // Nested: inner block 2 edges, outer block 5 edges (the bottom edges incl. the bore).
  s = await cursorOn(page, 11, 4, "block", "(s) => s.view.edgeKinds.selected > 0");
  expect(view(s).edgeKinds.selected).toBe(5);
  expect(modes(s)).toEqual(["full@11", "blue@20"]);
  await expect(lineHint(page, 11)).toContainText("1 Zone · 5 Kanten");
  await screenshotC(page, "c06-aussen");
  s = await cursorOn(page, 12, 10, "block", "(s) => s.view.edgeKinds.selected > 0");
  expect(view(s).edgeKinds.selected).toBe(2);
  recordC({
    id: "C06",
    title: "select = \"touch\", verschachtelte Blöcke",
    images: [
      { name: "c06-touch", label: "fillet(1.5, select = \"touch\")" },
      { name: "c06-aussen", label: "äußerer Block fillet(1)" },
    ],
    code: lines(await text(page), 11, 26),
    caption: "select = \"touch\": die Zone um die senkrechte Rasterlinie g[links, vorne, *] erfasst die senkrechte Kante und die vier Kanten, die sie an den Enden berühren (auf ganzer Länge), „5 Kanten“; Cursor auf der Zone: alle 5 in dieser Zone. Verschachtelt: der innere Block fillet(2) wählt 2 Kanten, der äußere fillet(1) mit der Zone g.boden die 5 Bodenkanten (vier Außenkanten und die Bohrungskante).",
    checks: ["touch: 5 Kanten, Zone: 5 von 5", "äußerer Block: 5 Kanten, eine Zone (blue@20)", "innerer Block: 2 Kanten"],
  });
});

test("C07 chamfer in allen drei Formen", async ({ page }) => {
  await openApp(page);
  await openExample(page, "spec/fase_chamfer.scad");
  let s = await cursorOn(page, 9, 3, null, "(s) => s.view.edgeKinds.selected > 0");
  expect(modes(s)).toEqual(["full@9", "blue@11"]);
  await expect(lineHint(page, 9)).toContainText("Kanten");
  const top = view(s).edgeKinds.selected;
  expect(top).toBe(4);
  await screenshotC(page, "c07-chamfer");
  s = await cursorOn(page, 14, 25, null, "(s) => s.view.edgeKinds.selected > 0");
  expect(view(s).edgeKinds.selected).toBeGreaterThan(0);
  s = await cursorOn(page, 19, 30, null, "(s) => s.view.edgeKinds.selected > 0");
  expect(view(s).edgeKinds.selected).toBe(12);
  recordC({
    id: "C07",
    title: "chamfer",
    images: [{ name: "c07-chamfer", label: "chamfer(1) mit Zone g.o" }],
    code: lines(await text(page), 9, 19),
    caption: "chamfer(1) { box; zone(g.o) }: die vier oberen Kanten orange, Zone blau; union(chamfer = 0.5) markiert die Schnittkanten der beiden Quader, $chamfer = 1 die 12 Würfelkanten.",
    checks: [`Zonenblock: ${top} Kanten`, "union(chamfer =): Schnittkanten > 0", "$chamfer: 12 Kanten"],
  });
});

test("C08 shell: Zone blau, geöffnete Flächen farbig", async ({ page }) => {
  await openApp(page);
  await openExample(page, "spec/griffschale.scad");
  let s = await cursorOn(page, 12, 10, "griffschale", "(s) => s.view.openFaces.length > 0");
  expect(modes(s)).toEqual(["full@12", "blue@14"]);
  const sh = (await nodesOnLine(page, 12)).find((x) => x.label === "shell")!;
  expect(sh.status).toBe("ok");
  // A real hollow body (the kernel returns an empty solid for some filleted bodies, see report).
  expect(sh.triangles ?? 0).toBeGreaterThan(50);
  expect(view(s).openFaces).toEqual([expect.objectContaining({ faces: 1, engine: 1 })]);
  await expect(lineHint(page, 12)).toContainText("1 Zone · 1 Fläche geöffnet");
  const open = (await layerPixels(page, "open-faces")).changed;
  expect(open).toBeGreaterThan(1000);
  await screenshotC(page, "c08-shell");
  s = await cursorOn(page, 14, 10, "griffschale", "(s) => s.view.openFaces.length > 0");
  expect(modes(s)).toEqual(["blue@14"]);
  expect(view(s).openFaces[0].faces).toBe(1);
  await screenshotC(page, "c08-shell-zone");
  recordC({
    id: "C08",
    title: "shell",
    images: [
      { name: "c08-shell", label: "Cursor auf shell(wand)" },
      { name: "c08-shell-zone", label: "Cursor auf der Zone" },
    ],
    code: lines(await text(page), 11, 17),
    caption: "griffschale.scad (neu), Modul griffschale: Cursor auf shell(wand): Hohlkörper voll, Zone (Platte um g.oben) blau, die geöffnete Oberseite des Körpers orange – in der Ansicht aus den Zonen-Meshes bestimmt (1 Fläche, wie die Engine meldet); „1 Zone · 1 Fläche geöffnet“ am Zeilenende. Cursor auf der Zone: nur diese Zone und ihre Fläche.",
    checks: [`geöffnete Flächen Ansicht/Engine: ${view(s).openFaces[0].faces}/${view(s).openFaces[0].engine}`, `Pixel der geöffneten Fläche: ${open}`],
  });
});

// ---------------------------------------------------------------- profile operations

test("C09 linear_extrude: Profil als Umriss unten, Höhengriff oben", async ({ page }) => {
  await openApp(page);
  const src = "linear_extrude(height = 15, twist = 30)\n    translate([-10, -5]) square([20, 10]);\ntranslate([40, 0, 0]) linear_extrude(8, center = true) circle(6);\n";
  await loadText(page, src);
  const s = await cursorOn(page, 1, 3, null, "(s) => s.view.plan.items.some(i => i.mode === 'profile')");
  expect(modes(s)).toEqual(["full@1", "profile@2"]);
  const hs = await waitHandles(page, "(hs) => hs.some(h => h.kind === 'height')");
  const h = hs.find((x) => x.kind === "height")!;
  expect(h.value).toEqual([15]);
  // Grip on top: above the projected origin on screen.
  const o = (await hook(page, (c) => c.project([0, 0, 0])))!;
  expect(grip(h).y).toBeLessThan(o.y - 40);
  const prof = (await layerPixels(page, "mode:profile")).changed;
  expect(prof).toBeGreaterThan(200);
  await screenshotC(page, "c09-vorher");
  const from = grip(h);
  await mouseDrag(page, from, line(from, { x: from.x, y: from.y - 60 }, 12));
  await waitDragDone(page);
  await page.mouse.move(2, 2);
  const after = await text(page);
  const hNew = Number(after.match(/linear_extrude\(height = (\d+)/)![1]);
  expect(hNew).toBeGreaterThan(15);
  await waitView(page, "(s) => s.view.plan.items.some(i => i.mode === 'profile')");
  await screenshotC(page, "c09-nachher");
  // center = true: the profile sits at -h/2.
  const s2 = await cursorOn(page, 3, 30, null, "(s) => s.view.plan.items.some(i => i.mode === 'profile')");
  expect(modes(s2)).toEqual(["full@3", "profile@3"]);
  recordC({
    id: "C09",
    title: "linear_extrude",
    images: [
      { name: "c09-vorher", label: "vorher" },
      { name: "c09-nachher", label: `Höhe gezogen → ${hNew}` },
    ],
    code: `${src.trim()}\n// nach dem Ziehen:\n${lines(after, 1)}`,
    caption: `Cursor auf linear_extrude: Körper voll, das Profil (Rechteck) als Umriss in Profilfarbe unten in der xy-Ebene, sichtbar durch den Körper; Höhengriff oben auf der Achse. Griff mit der Maus nach oben gezogen: height = 15 → ${hNew}. Bei center = true liegt der Umriss auf −h/2.`,
    checks: ["Darstellung full@1, profile@2", "Höhengriff über dem Ursprung", `Pixel des Profils: ${prof}`],
  });
});

test("C10 rotate_extrude: Profil in der xz-Ebene, Drehachse, Winkelgriff", async ({ page }) => {
  await openApp(page);
  const src = "rotate_extrude(angle = 270)\n    translate([12, 0]) square([4, 20]);\n";
  await loadText(page, src);
  const s = await cursorOn(page, 1, 3, null, "(s) => s.view.plan.guides.axis === 1");
  expect(modes(s)).toEqual(["full@1", "profile@2"]);
  // Axis: a vertical line through the origin (x = y = 0).
  const axis = view(s).plan.guidePoints.find((g) => g.kind === "axis")!;
  expect(axis.points.every((p) => Math.abs(p[0]) < 1e-9 && Math.abs(p[1]) < 1e-9)).toBe(true);
  expect(axis.points[1][2] - axis.points[0][2]).toBeGreaterThan(20);
  const hs = await waitHandles(page, "(hs) => hs.some(h => h.kind === 'angle')");
  const a = hs.find((x) => x.kind === "angle")!;
  expect(a.value).toEqual([270]);
  const axisPx = (await layerPixels(page, "guide:axis")).changed;
  const profPx = (await layerPixels(page, "mode:profile")).changed;
  expect(axisPx).toBeGreaterThan(60);
  expect(profPx).toBeGreaterThan(150);
  await screenshotC(page, "c10-vorher");
  // Drag the angle grip to 180° (rastet in 15°-Schritten).
  const to = (await hook(page, (c, id) => c.handlePoint(id as number, 180), a.id))!;
  await mouseDrag(page, grip(a), line(grip(a), to, 14));
  await waitDragDone(page);
  const after = await text(page);
  expect(after).toMatch(/rotate_extrude\(angle = 180\)/);
  await waitView(page, "(s) => s.view.plan.guides.axis === 1");
  await screenshotC(page, "c10-nachher");
  recordC({
    id: "C10",
    title: "rotate_extrude",
    images: [
      { name: "c10-vorher", label: "angle = 270" },
      { name: "c10-nachher", label: "Winkelgriff auf 180° gezogen" },
    ],
    code: `${src.trim()}\n// nach dem Ziehen:\n${lines(after, 1)}`,
    caption: "Cursor auf rotate_extrude: Körper voll, das Profil (Rechteck x = 12…16, y = 0…20) als Umriss in der xz-Ebene (Profil-y wird z), die Drehachse z als gestrichelte Linie, der Winkelgriff auf dem Ring um z. Ring mit der Maus auf 180° gezogen: angle = 270 → 180 (Raster 15°).",
    checks: ["Darstellung full@1, profile@2", "Achse: Linie x = y = 0 entlang z", "Winkelgriff 270 → 180", `Pixel Achse/Profil: ${axisPx}/${profPx}`],
  });
});

test("C11 loft: alle Profile als Umrisse, Ergebnis halbtransparent", async ({ page }) => {
  await openApp(page);
  await openExample(page, "spec/loft_sweep.scad");
  const s = await cursorOn(page, 9, 2, null, "(s) => s.view.plan.items.some(i => i.mode === 'translucent')");
  expect(modes(s)).toEqual(["translucent@9", "profile@10", "profile@11", "profile@12"]);
  const prof = (await layerPixels(page, "mode:profile")).changed;
  expect(prof).toBeGreaterThan(300);
  await screenshotC(page, "c11-loft");
  recordC({
    id: "C11",
    title: "loft",
    images: [{ name: "c11-loft", label: "Cursor auf loft()" }],
    code: lines(await text(page), 9, 13),
    caption: "loft_sweep.scad, Cursor auf loft(): Ergebnis halbtransparent, alle drei Profile (Kreis unten, Quadrat auf z = 15, Kreis auf z = 30) als Umrisse an ihrer Lage.",
    checks: ["translucent@9, profile@10/11/12", `Pixel der Profile: ${prof}`],
  });
});

test("C12 sweep: Pfad als Kurve, Griffe an Literal-Punkten, Profil am Startpunkt", async ({ page }) => {
  await openApp(page);
  const src = "sweep(path = [[0, 0, 0], [20, 0, 10], [40, 10, 10], [60, 10, 0]], interp = \"spline\")\n    circle(3);\n";
  await loadText(page, src);
  const s = await cursorOn(page, 1, 3, null, "(s) => s.view.plan.guides.path === 1");
  expect(modes(s)).toEqual(["full@1", "profile@2"]);
  const path = view(s).plan.guidePoints.find((g) => g.kind === "path")!;
  // The curve passes through the literal points (spline interpolation).
  for (const q of [[0, 0, 0], [20, 0, 10], [40, 10, 10], [60, 10, 0]]) {
    const d = Math.min(...path.points.map((p) => Math.hypot(p[0] - q[0], p[1] - q[1], p[2] - q[2])));
    expect(d).toBeLessThan(0.6);
  }
  const hs = await waitHandles(page, "(hs) => hs.filter(h => h.kind === 'point').length === 4");
  expect(hs.filter((h) => h.kind === "point").map((h) => h.style)).toEqual(["literal", "literal", "literal", "literal"]);
  const guide = (await layerPixels(page, "guide:path")).changed;
  expect(guide).toBeGreaterThan(150);
  const prof = (await layerPixels(page, "mode:profile")).changed;
  expect(prof).toBeGreaterThan(40);
  await screenshotC(page, "c12-vorher");
  const p2 = hs.find((h) => h.kind === "point" && h.index === 2)!;
  const from = grip(p2);
  await mouseDrag(page, from, line(from, { x: from.x + 10, y: from.y - 50 }, 12), { alt: true });
  await waitDragDone(page);
  await page.mouse.move(2, 2);
  const after = await text(page);
  expect(after).not.toBe(src);
  expect(after).toMatch(/^sweep\(path = \[\[0, 0, 0\], \[20, 0, 10\], \[[-\d]+, [-\d]+, [-\d]+\], \[60, 10, 0\]\]/);
  await waitView(page, "(s) => s.view.plan.guides.path === 1");
  await screenshotC(page, "c12-nachher");
  recordC({
    id: "C12",
    title: "sweep",
    images: [
      { name: "c12-vorher", label: "vorher" },
      { name: "c12-nachher", label: "dritten Pfadpunkt gezogen" },
    ],
    code: `${src.trim()}\n// nach dem Ziehen:\n${lines(after, 1)}`,
    caption: "Cursor auf sweep: Pfad als Kurve (kubischer Spline durch die Punkte wie im Kernel, Sehnenlängen-Parameter), ein Griff an jedem Literal-Punkt, das Profil (Kreis) als Umriss am Startpunkt senkrecht zur Starttangente. Den dritten Punkt mit Alt frei gezogen: nur dieses Literal ändert sich, Kurve und Körper folgen.",
    checks: ["Kurve durch alle 4 Punkte (Abstand < 0,6 mm)", "4 Griffe (Literal)", `Pixel Pfad/Profil: ${guide}/${prof}`],
  });
});

test("C13 surface und thicken: Kontrollnetz mit Griffen, Fläche beidseitig, Fläche als Umriss", async ({ page }) => {
  await openApp(page);
  const src =
    "thicken(2)\n    surface(points = [\n        [[0, 0, 0], [10, 0, 3], [20, 0, 3], [30, 0, 0]],\n        [[0, 10, 3], [10, 10, 10], [20, 10, 10], [30, 10, 3]],\n        [[0, 20, 3], [10, 20, 10], [20, 20, 12], [30, 20, 3]],\n        [[0, 30, 0], [10, 30, 3], [20, 30, 3], [30, 30, 0]]\n    ]);\n";
  await loadText(page, src);
  expect((await cad(page)).diagnostics.filter((d) => d.severity === "error")).toEqual([]);
  let s = await cursorOn(page, 2, 6, null, "(s) => s.view.plan.guides.net > 0");
  expect(view(s).plan.guides.net).toBe(8);
  expect(modes(s)).toEqual(["full@2"]);
  const hs = await waitHandles(page, "(hs) => hs.filter(h => h.kind === 'point').length === 16");
  expect(hs.every((h) => h.style === "literal")).toBe(true);
  await screenshotC(page, "c13-netz");
  // Drag the middle control point up (Alt: free).
  const mid = hs.find((h) => h.index === 5)!;
  const from = grip(mid);
  await mouseDrag(page, from, line(from, { x: from.x, y: from.y - 40 }, 10), { alt: true });
  await waitDragDone(page);
  await page.mouse.move(2, 2);
  const after = await text(page);
  expect(lines(after, 4)).not.toBe("        [[0, 10, 3], [10, 10, 10], [20, 10, 10], [30, 10, 3]],");
  expect(lines(after, 3)).toBe("        [[0, 0, 0], [10, 0, 3], [20, 0, 3], [30, 0, 0]],");
  // thicken: the surface as outline, the result full.
  s = await cursorOn(page, 1, 3, null, "(s) => s.view.plan.items.some(i => i.mode === 'profile')");
  expect(modes(s)).toEqual(["full@1", "profile@2"]);
  await screenshotC(page, "c13-thicken");
  // A surface alone (not in a boolean) is drawn from both sides: seen from above and from
  // below it covers pixels.
  const alone = "surface(points = [for (i = [0:3]) [for (j = [0:3]) [i * 8, j * 8, (i - 1.5) * (j - 1.5)]]]);\n";
  await loadText(page, alone);
  expect((await cad(page)).diagnostics.filter((d) => d.severity === "error")).toEqual([]);
  s = await cursorOn(page, 1, 3, null, "(s) => s.view.plan.guides.net > 0");
  const box = (await page.getByTestId("viewport-canvas").boundingBox())!;
  const cover = async () => box.width * box.height - (await colorPixels(page, "--view-bg", 6));
  await page.evaluate(() => (window as unknown as { __cad: { viewFrom(d: number[]): void } }).__cad.viewFrom([0.3, -0.4, 1]));
  await frame(page);
  const above = await cover();
  await page.evaluate(() => (window as unknown as { __cad: { viewFrom(d: number[]): void } }).__cad.viewFrom([0.3, -0.4, -1]));
  await frame(page);
  const below = await cover();
  await screenshotC(page, "c13-unten");
  expect(above).toBeGreaterThan(5000);
  expect(below).toBeGreaterThan(5000);
  recordC({
    id: "C13",
    title: "surface und thicken",
    images: [
      { name: "c13-netz", label: "Cursor auf surface: Kontrollnetz" },
      { name: "c13-thicken", label: "Cursor auf thicken(2)" },
      { name: "c13-unten", label: "offene Fläche von unten" },
    ],
    code: `${src.trim()}\n// nach dem Ziehen (Zeile 4):\n${lines(after, 4)}\n// Fläche allein:\n${alone.trim()}`,
    caption: "Cursor auf surface (4 × 4): Kontrollnetz (4 Zeilen + 4 Spalten als Linien) mit einem Griff an jedem Kontrollpunkt. Einen inneren Punkt mit Alt nach oben gezogen: nur sein Literal ändert sich. Cursor auf thicken(2): Ergebnis voll, die Fläche als Umriss. Eine offene Fläche allein (berechnete Punkte, gesperrte Griffe) wird beidseitig gerendert: von oben und von unten sichtbar.",
    checks: ["8 Netzlinien, 16 Griffe", "mittlerer Punkt gezogen", "thicken: full@1, profile@2", `bedeckte Pixel der offenen Fläche von oben/unten: ${above}/${below}`],
  });
});

test("C14 2D-Formen allein: Umriss mit Füllung, z-Abweichung als Warnung", async ({ page }) => {
  await openApp(page);
  const src = "grid g {\n    x: a = 0, b = 30;\n    y: v = 0, h = 20;\n    z: u = 0, o = 4;\n}\npolygon([g[a, v, u], g[b, v, u], g[b, h, o], g[a, h, u]]);\ntranslate([40, 0]) circle(8);\n";
  await loadText(page, src);
  const s = await waitView(page, "(s) => s.view.plan.base !== null");
  expect(s.diagnostics.some((d) => d.severity === "warning" && d.line === 6 && /z-Koordinate/.test(d.message))).toBe(true);
  await expect(lineEl(page, 6)).toHaveClass(/cm-l-warning/);
  await expect(lineHint(page, 6)).toContainText("z-Koordinate ignoriert");
  const st = await cursorOn(page, 6, 3, null, "(s) => s.view.plan.items.length > 0");
  expect(view(st).plan.items.every((i) => i.flat)).toBe(true);
  await hook(page, (c) => c.setView("top"));
  await frame(page);
  const fill = await colorPixels(page, "--view-flat", 70);
  expect(fill).toBeGreaterThan(200);
  await screenshotC(page, "c14-2d", "full");
  recordC({
    id: "C14",
    title: "2D-Formen allein",
    images: [{ name: "c14-2d", label: "Editor und Ansicht von oben" }],
    code: src.trim(),
    caption: "Nur 2D-Formen: Umriss mit leichter Füllung in der xy-Ebene (Ansicht von oben). Der Rasterpunkt g[b, h, o] hat z = 4: Zeile 6 gelb mit „z-Koordinate ignoriert: 2D-Formen liegen in der xy-Ebene“ am Zeilenende.",
    checks: ["Warnung in Zeile 6 (Diagnose und Zeilenklasse)", "alle Darstellungen flach", `Pixel Flächenfarbe: ${fill}`],
  });
});

// ---------------------------------------------------------------- view and drags

test("C15 Ansicht nach deutlicher Größenänderung neu einpassen", async ({ page }) => {
  await openApp(page);
  await loadText(page, "cube(10);\n");
  let s = await cad(page);
  const d0 = view(s).camera.distance;
  expect(view(s).camera.autoFit).toBe(true);
  // Camera not moved: a clearly larger model is framed again automatically.
  const v1 = s.sentVersion;
  await hook(page, (c) => c.setText("cube(60);\n"));
  s = await waitScene(page, { minVersion: v1 + 1 });
  s = await waitView(page, `(s) => s.view.camera.distance > ${d0 * 3}`);
  const d1 = view(s).camera.distance;
  await expect(page.getByTestId("refit")).toHaveCount(0);
  await screenshotC(page, "c15-auto");
  // A small change does not move the camera.
  const v2 = s.sentVersion;
  await hook(page, (c) => c.setText("cube(62);\n"));
  s = await waitScene(page, { minVersion: v2 + 1 });
  expect(Math.abs(view(s).camera.distance - d1)).toBeLessThan(1e-6);
  // The user orbits: from then on only a button is offered.
  const box = (await page.getByTestId("viewport-canvas").boundingBox())!;
  const c0 = { x: box.x + box.width * 0.75, y: box.y + box.height * 0.8 };
  await mouseDrag(page, c0, line(c0, { x: c0.x + 60, y: c0.y - 20 }, 6));
  s = await waitView(page, "(s) => s.view.camera.autoFit === false");
  const d2 = view(s).camera.distance;
  const v3 = s.sentVersion;
  await hook(page, (c) => c.setText("cube(300);\n"));
  s = await waitScene(page, { minVersion: v3 + 1 });
  await expect(page.getByTestId("refit")).toBeVisible();
  expect(Math.abs(view(s).camera.distance - d2)).toBeLessThan(1e-6);
  await screenshotC(page, "c15-knopf");
  await page.getByTestId("refit").click();
  s = await waitView(page, `(s) => s.view.camera.distance > ${d2 * 3} && s.view.camera.autoFit`);
  await expect(page.getByTestId("refit")).toHaveCount(0);
  await screenshotC(page, "c15-eingepasst");
  recordC({
    id: "C15",
    title: "Ansicht neu einpassen",
    images: [
      { name: "c15-knopf", label: "Kamera bewegt, Modell 5× größer: Knopf" },
      { name: "c15-eingepasst", label: "nach „Neu einpassen“" },
    ],
    code: "cube(10);  →  cube(60);  →  cube(62);  →  (Drehen)  →  cube(300);",
    caption: `Solange der Nutzer die Kamera nicht bewegt hat, wird nach einer deutlichen Größenänderung (Diagonale × 1,6 oder Mitte um mehr als die halbe Diagonale verschoben) neu eingepasst: cube(10) → cube(60), Kameraabstand ${d0.toFixed(0)} → ${d1.toFixed(0)} mm; cube(62) ändert nichts. Nach dem Drehen mit der Maus bleibt die Kamera bei cube(300) stehen, oben erscheint „Neu einpassen“; ein Klick rahmt das Modell ein.`,
    checks: ["automatisch: Abstand > 3 × vorher", "kleine Änderung: Kamera unverändert", "nach Drehen: Knopf, Kamera unverändert", "Knopf: eingepasst, Knopf weg"],
  });
});

test("C16 Punktgriff achsgebunden ziehen (Umschalttaste, X/Y/Z)", async ({ page }) => {
  await openApp(page);
  const src = "cube([60, 30, 1]);\ntranslate([10, 5.5, 2.25]) cube(6);\n";
  await loadText(page, src);
  await clickLine(page, 2, 3);
  let [h] = await waitHandles(page, "(hs) => hs.some(h => h.kind === 'point')");
  await screenshotC(page, "c16-vorher");
  // Shift: the axis follows the mouse motion; the screen direction of +x.
  const o = (await hook(page, (c) => c.project([10, 5.5, 2.25])))!;
  const px = (await hook(page, (c) => c.project([30, 5.5, 2.25])))!;
  const dir = { x: (px.x - o.x) / 20, y: (px.y - o.y) / 20 };
  const from = grip(h);
  // Mostly along x on screen, with a sideways wobble that would change y/z without the lock.
  const path = Array.from({ length: 14 }, (_, i) => ({ x: from.x + dir.x * (i + 1) * 1.2 + (i % 3) * 2, y: from.y + dir.y * (i + 1) * 1.2 - (i % 2) * 4 }));
  await page.keyboard.down("Shift");
  await mouseDrag(page, from, path, {
    mid: async () => {
      await page.waitForFunction(() => (window as unknown as { __cad: { drag(): { axis: number | null } | null } }).__cad.drag()?.axis === 0);
      await expect(page.getByTestId("status-drag")).toContainText("Achse x");
      await screenshotC(page, "c16-waehrend");
    },
  });
  await page.keyboard.up("Shift");
  await waitDragDone(page);
  const t1 = await text(page);
  const m1 = lines(t1, 2).match(/^translate\(\[([-\d.]+), ([-\d.]+), ([-\d.]+)\]\)/)!;
  expect(Number(m1[1])).toBeGreaterThan(10);
  expect(m1[2]).toBe("5.5");
  expect(m1[3]).toBe("2.25");
  // Key Z during the drag: along z only.
  await clickLine(page, 2, 3);
  [h] = await waitHandles(page, "(hs) => hs.some(h => h.kind === 'point')");
  const f2 = grip(h);
  // Free at first, then Z: from there on only z changes (the lock starts at the current
  // position, the other components keep the values they have now).
  await page.mouse.move(f2.x, f2.y);
  await page.mouse.down();
  for (const p of line(f2, { x: f2.x + 25, y: f2.y + 10 }, 6)) {
    await page.mouse.move(p.x, p.y);
    await frame(page);
  }
  await page.waitForFunction(() => ((window as unknown as { __cad: { drag(): { edits: number } | null } }).__cad.drag()?.edits ?? 0) > 0);
  // Let the last free step land (its scene drawn), then lock to z.
  await page.waitForFunction(() => {
    const s = (window as unknown as { __cad: { state(): CadState } }).__cad.state();
    return !s.pendingSend && s.completeVersion === s.sentVersion;
  });
  const atZ = await text(page);
  await page.keyboard.press("z");
  const f3 = { x: f2.x + 25, y: f2.y + 10 };
  for (const p of line(f3, { x: f3.x + 5, y: f3.y - 170 }, 16)) {
    await page.mouse.move(p.x, p.y);
    await frame(page);
  }
  await page.waitForFunction(() => (window as unknown as { __cad: { drag(): { axis: number | null } | null } }).__cad.drag()?.axis === 2);
  await page.mouse.up();
  await waitDragDone(page);
  const t2 = await text(page);
  const m2 = lines(t2, 2).match(/^translate\(\[([-\d.]+), ([-\d.]+), ([-\d.]+)\]\)/)!;
  const mm = lines(atZ, 2).match(/^translate\(\[([-\d.]+), ([-\d.]+), ([-\d.]+)\]\)/)!;
  // x and y as they were when Z was pressed; z moved up.
  expect(m2[1]).toBe(mm[1]);
  expect(m2[2]).toBe(mm[2]);
  expect(Number(m2[3])).toBeGreaterThan(Number(mm[3]) + 1);
  // The key did not reach the editor.
  expect(t2).not.toMatch(/z/);
  await screenshotC(page, "c16-nachher");
  recordC({
    id: "C16",
    title: "Achsgebundenes Ziehen",
    images: [
      { name: "c16-waehrend", label: "Umschalttaste: Achse x aus der Bewegung" },
      { name: "c16-nachher", label: "erst frei, dann Taste Z" },
    ],
    code: `${src.trim()}\n// Umschalt + Ziehen (schräg, überwiegend x):\n${lines(t1, 2)}\n// erst frei, dann Taste Z:\n${lines(t2, 2)}`,
    caption: "Mit gedrückter Umschalttaste bestimmt die Mausbewegung die Achse (die Achse des lokalen Systems, deren Bildschirmrichtung am besten zur Bewegung passt); der Griff läuft auf einer Linie in Achsfarbe, nur diese Komponente ändert sich – y = 5.5 und z = 2.25 bleiben als Text erhalten (cad-eval schreibt unveränderte Literale nicht neu). Die Tasten X/Y/Z wählen die Achse direkt (erneut drücken: frei), auch mitten im Ziehen: die Achslinie geht dann durch die aktuelle Lage, die übrigen Komponenten behalten ihren Wert; die Tasten erreichen den Editor nicht.",
    checks: [`Umschalt: ${m1.slice(1).join(", ")} (nur x geändert)`, `erst frei, dann Taste Z: ${m2.slice(1).join(", ")} (x, y wie beim Drücken von Z)`],
  });
});

// ---------------------------------------------------------------- examples, measurements

const NEW_EXAMPLES: Array<{ path: string; line: number; col: number; context: string | null; what: string }> = [
  { path: "spec/griffschale.scad", line: 12, col: 10, context: "griffschale", what: "shell mit Zone g.oben, Kabelausgang per difference, Schraubdome mit difference(fillet = 0.5)" },
  { path: "spec/flasche.scad", line: 11, col: 10, context: "flasche", what: "rotate_extrude eines Polygons aus Rasterpunkten, fillet(3) mit zwei Zonen, Teilumlauf 270°" },
  { path: "spec/lueftungsgitter.scad", line: 12, col: 10, context: "gitter", what: "linear_extrude für Rahmen und Schlitze an indizierten Rasterebenen, difference(chamfer = 0.6)" },
  { path: "spec/kabelfuehrung.scad", line: 16, col: 3, context: null, what: "sweep eines U-Profils entlang eines Spline-Pfads aus Rasterpunkten und Literalen, linearer Kanal" },
  { path: "spec/freiformdeckel.scad", line: 11, col: 10, context: "deckel", what: "surface aus einem 4 × 4-Netz (Rasterrand, Wölbung als Literale), thicken(2)" },
];

test("C17 Neue Beispiele im Menü", async ({ page }) => {
  await openApp(page);
  const options = await page.getByTestId("examples").locator("option").allTextContents();
  const caps: string[] = [];
  for (const ex of NEW_EXAMPLES) {
    const name = ex.path.split("/").pop()!;
    expect(options).toContain(name);
    let s = await openExample(page, ex.path);
    expect(s.diagnostics.filter((d) => d.severity === "error")).toEqual([]);
    const bad = await page.evaluate(() => {
      const nodes = (window as unknown as { __cad: { store: { getState(): { complete: { nodes: Array<{ status: string; stmt: number | null; bounds: unknown; label: string }> } } } } }).__cad.store.getState().complete.nodes;
      // Failed nodes, and nodes reported ok without any geometry (empty kernel result).
      return nodes.filter((n) => n.status === "error" || (n.stmt !== null && n.status === "ok" && !n.bounds && n.label !== "group")).map((n) => n.label);
    });
    expect(bad).toEqual([]);
    s = await cursorOn(page, ex.line, ex.col, ex.context, "(s) => s.view.plan.items.length > 0");
    const img = `c17-${name.replace(".scad", "")}`;
    await screenshotC(page, img, "full");
    caps.push(`${name}: ${ex.what} (Cursor Zeile ${ex.line}: ${modes(s).slice(0, 4).join(", ")})`);
    recordC({
      id: `C17-${name.replace(".scad", "")}`,
      title: `Beispiel ${name}`,
      images: [{ name: img, label: `Cursor in Zeile ${ex.line}` }],
      caption: `${ex.what}. Fehlerfrei ausgewertet; Cursor in Zeile ${ex.line}.`,
    });
  }
  recordC({ id: "C17", title: "Neue Beispiele", images: [], caption: caps.join(" · ") });
});

test("C19 Watchdog: hängender Knoten wird gesperrt, Zeile rot, Änderung hebt die Sperre auf", async ({ page }) => {
  page.on("pageerror", (e) => console.log("pageerror:", e.message));
  await page.goto("/?watchdog=1500");
  await expect(page.getByTestId("editor").locator(".cm-content")).toBeVisible();
  await waitScene(page);
  await loadText(page, "cube(5);\n");
  // Five expensive nodes (difference with fillet), one per line: the evaluation takes several
  // steps, so a Progress with the current node arrives before the scene is complete.
  const row = (i: number, w: number, f: number, r: number) =>
    `translate([${i * 40}, 0, 0]) difference(fillet = ${f}) { cube([${w}, 20, 5]); translate([8, 10, -1]) cylinder(h = 7, r = ${r}); translate([20, 10, -1]) cylinder(h = 7, r = ${r}); }`;
  // Distinct values per line: cache keys are content keys, equal nodes share them (and a block).
  const src = [0, 1, 2, 3, 4].map((i) => row(i, 30 + i, +(0.8 + i * 0.05).toFixed(2), +(3 + i * 0.1).toFixed(1))).join("\n") + "\n";
  const v0 = (await cad(page)).sentVersion;
  // Hang the worker as soon as the first Progress with a current node arrives.
  await page.evaluate(() => {
    type W = { __cad: { store: { subscribe(f: (s: { progress: { line: number | null } | null }) => void): () => void }; debugHang(ms: number): void } };
    const c = (window as unknown as W).__cad;
    const off = c.store.subscribe((s) => {
      if (s.progress?.line) {
        off();
        c.debugHang(4000);
      }
    });
  });
  await hook(page, (c, t) => c.setText(t as string), src);
  await page.waitForFunction(() => (window as unknown as { __cad: { state(): CadState } }).__cad.state().restarts === 1, null, { timeout: 30_000 });
  const err = (await cad(page)).engineError!;
  expect(err).toMatch(/Kernel hängt.*Zeile \d+.*Knoten gesperrt/);
  const s = await waitScene(page, { minVersion: v0 + 1 });
  expect(s.blockedKeys).toHaveLength(1);
  const blocked = (await completeNodes(page)).filter((n) => n.status === "error");
  expect(blocked).toHaveLength(1);
  const bl = blocked[0].range.line;
  expect(blocked[0].messages[0].message).toContain("Zeitüberschreitung");
  await expect(lineEl(page, bl)).toHaveClass(/cm-l-error/);
  await expect(lineHint(page, bl)).toContainText("Zeitüberschreitung");
  await expect(page.getByTestId("status-engine-error")).toContainText("Knoten gesperrt");
  await screenshotC(page, "c19-gesperrt", "full");
  // Editing the blocked line changes its keys: it evaluates normally (the key stays in the list).
  const t0 = await text(page);
  const edited = t0.split("\n").map((l, i) => (i === bl - 1 ? row(bl - 1, 33 + bl, 0.7, 2.8) : l)).join("\n");
  const s2 = await loadText(page, edited);
  expect((await completeNodes(page)).filter((n) => n.status === "error")).toEqual([]);
  expect(s2.restarts).toBe(1);
  await expect(lineEl(page, bl)).not.toHaveClass(/cm-l-error/);
  recordC({
    id: "C19",
    title: "Watchdog-Sperre",
    images: [{ name: "c19-gesperrt", label: "nach dem Neustart: gesperrter Knoten rot" }],
    code: `${lines(src, bl).trim()}\n// geändert:\n${lines(edited, bl).trim()}`,
    caption: `Watchdog auf 1,5 s gesetzt (?watchdog=1500), der Worker beim ersten Progress mit __debugHang 4 s blockiert: der Worker wird beendet und neu gestartet, der Schlüssel des Knotens aus Progress.current wird gesperrt; der neue Worker bekommt vor SetSource {type: "BlockKeys", keys: […]}. Der Knoten (${blocked[0].label}) in Zeile ${bl} meldet „Zeitüberschreitung, Knoten gesperrt bis zur nächsten Änderung“ (Zeile rot), der Rest wird ausgewertet. Gesperrt wird der Inhaltsschlüssel: gleiche Knoten in anderen Zeilen wären mitgesperrt. Nach Ändern der Zeile wird sie normal berechnet; die Liste bleibt für weitere Neustarts erhalten (Unit-Test transport.test.ts).`,
    checks: [`Neustarts: 1, gesperrte Schlüssel: ${s.blockedKeys.length}`, `gesperrter Knoten: Zeile ${bl} (${blocked[0].label})`, "nach Änderung: 0 Fehler, kein weiterer Neustart"],
  });
});

test("C20 Entwurfsmodus: Verrundung beim Ziehen ausgesetzt, danach berechnet", async ({ page }) => {
  await openApp(page);
  const src = "difference(fillet = 1) {\n    cube([40, 30, 10]);\n    translate([20, 15, -1]) cylinder(h = 12, r = 5);\n}\n";
  await loadText(page, src);
  await clickLine(page, 3, 6);
  const [h] = await waitHandles(page, "(hs) => hs.some(h => h.kind === 'point')");
  const from = grip(h);
  let draftSeen = false;
  let hint = "";
  await mouseDrag(page, from, line(from, { x: from.x + 40, y: from.y + 10 }, 16), {
    mid: async () => {
      await expect(page.getByTestId("status-draft")).toContainText("Verrundung beim Ziehen ausgesetzt (Zeile 1)", { timeout: 20_000 });
      draftSeen = true;
      hint = (await lineHint(page, 1).textContent()) ?? "";
      await screenshotC(page, "c20-ziehen", "full");
    },
  });
  await waitDragDone(page);
  // After the drag the full evaluation follows: the blend is back, the note is gone.
  await page.waitForFunction(() => {
    const c = (window as unknown as { __cad: { store: { getState(): { complete: { nodes: Array<{ blend_suspended?: boolean }> } | null } } } }).__cad.store.getState().complete;
    return !!c && c.nodes.every((n) => !n.blend_suspended);
  }, null, { timeout: 30_000 });
  await expect(page.getByTestId("status-draft")).toHaveCount(0);
  const d = (await nodesOnLine(page, 1)).find((x) => x.label === "difference")!;
  expect(d.status).toBe("ok");
  expect(draftSeen).toBe(true);
  expect(hint).toContain("Verrundung beim Ziehen ausgesetzt");
  await screenshotC(page, "c20-danach", "full");
  recordC({
    id: "C20",
    title: "Entwurfsmodus beim Ziehen",
    images: [
      { name: "c20-ziehen", label: "während des Ziehens" },
      { name: "c20-danach", label: "nach dem Loslassen" },
    ],
    code: `${src.trim()}\n// gezogen: ${lines(await text(page), 3).trim()}`,
    caption: "Beim Ziehen des Zylinder-Griffs wird die Verrundung von difference(fillet = 1) ausgesetzt (Entwurfsmodus der Engine: Verrundungen, die länger als 25 ms brauchen): die Statuszeile zeigt „Verrundung beim Ziehen ausgesetzt (Zeile 1)“, am Zeilenende steht derselbe Hinweis (NodeReport.blend_suspended, im Szenenknoten als blend_suspended). Nach dem Loslassen wertet die Engine vollständig aus, Hinweis und Meldung verschwinden.",
    checks: ["Statuszeile während des Ziehens: Zeile 1", `Zeilenhinweis: ${hint.trim()}`, "danach: blend_suspended überall false, Knoten ok"],
  });
});

test("C18 Messwerte: Auswertzeit fillet, chamfer, shell, sweep, loft, surface", async ({ page }) => {
  await openApp(page);
  // Each model is evaluated with different parameters (no cache hits); kernel time of the
  // complete scene (stats.kernel_ms) and SetSource → complete scene in the app.
  const models: Array<{ key: string; label: string; src: (k: number) => string }> = [
    { key: "fillet", label: "fillet(r) Zonenblock, 4 Kanten", src: (k) => `grid g { z: o = 15; }\nfillet(${1 + k * 0.25}) {\n  cube([${30 + k}, 20, 15]);\n  zone(g.o, d = 1);\n}\n` },
    { key: "fillet_bool", label: "difference(fillet = r), 4 Bohrungen", src: (k) => `difference(fillet = ${0.8 + k * 0.1}) {\n  cube([${60 + k}, 20, 5]);\n  for (i = [0:3]) translate([10 + i * 13, 10, -1]) cylinder(h = 7, r = 3);\n}\n` },
    { key: "chamfer", label: "chamfer, 12 Kanten ($chamfer)", src: (k) => `cube([${20 + k}, 20, 20], $chamfer = ${1 + k * 0.2});\n` },
    { key: "shell", label: "shell(1.6) eines gefasten Quaders ($chamfer = 4)", src: (k) => `grid g { z: o = 18; }\nshell(1.6) {\n  box([0, 0, 0], [${90 + 2 * k}, 34, 18], $chamfer = 4);\n  zone(g.o, d = 1);\n}\n` },
    { key: "sweep", label: "sweep, Spline-Pfad mit 4 Punkten", src: (k) => `sweep(path = [[0, 0, 0], [20, 0, ${10 + k}], [40, 10, 10], [60, 10, 0]]) circle(3);\n` },
    { key: "loft", label: "loft, 3 Profile", src: (k) => `loft() {\n  circle(${10 + k * 0.5});\n  translate([0, 0, 15]) square(14, center = true);\n  translate([0, 0, 30]) circle(4);\n}\n` },
    { key: "surface", label: "thicken(surface) 4 × 4", src: (k) => `thicken(2) surface(points = [for (i = [0:3]) [for (j = [0:3]) [i * 10, j * 10, ${2 + k} * sin(i * 60) * cos(j * 60)]]]);\n` },
  ];
  const out: Record<string, { label: string; kernel: number[]; total: number[]; median: number; medianTotal: number; nodes: number; failed: number }> = {};
  for (const m of models) {
    const kernel: number[] = [];
    const total: number[] = [];
    let nodes = 0;
    let failed = 0;
    for (let k = 0; k < 5; k++) {
      const v0 = (await cad(page)).sentVersion;
      await hook(page, (c, t) => c.setText(t as string), m.src(k));
      await waitScene(page, { minVersion: v0 + 1 });
      const st = await page.evaluate(() => {
        const s = (window as unknown as { __cad: { store: { getState(): { complete: { stats: { kernel_ms: number; eval_ms: number; nodes: number; cache_hits: number }; nodes: Array<{ status: string; stmt: number | null; bounds: unknown; label: string }> }; metrics: { evalMs: number[] } } } } }).__cad.store.getState();
        // A node reported ok but without geometry (bounds null) counts as failed too.
        const empty = s.complete.nodes.filter((n) => n.status === "ok" && n.stmt !== null && !n.bounds && n.label !== "group").length;
        return { stats: s.complete.stats, errors: s.complete.nodes.filter((n) => n.status === "error").length + empty, evalMs: s.metrics.evalMs.at(-1) ?? null };
      });
      expect(st.errors).toBe(0);
      if (st.errors) failed++;
      kernel.push(st.stats.kernel_ms);
      if (st.evalMs !== null) total.push(st.evalMs);
      nodes = st.stats.nodes;
    }
    const med = (a: number[]) => [...a].sort((x, y) => x - y)[Math.floor(a.length / 2)] ?? 0;
    out[m.key] = { label: m.label, kernel, total, median: med(kernel), medianTotal: med(total), nodes, failed };
  }
  writeMetricsC({ evalTimes: out });
  recordC({
    id: "C18",
    title: "Auswertzeiten",
    images: [],
    caption: Object.values(out)
      .map((o) => `${o.label}: Kernel ${o.median.toFixed(0)} ms (Median aus 5${o.failed ? `, ${o.failed} gescheitert` : ""}), SetSource → Szene ${o.medianTotal.toFixed(0)} ms`)
      .join(" · "),
  });
});
