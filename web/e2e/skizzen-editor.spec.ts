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
  if (!p || !p.visible) throw new Error(`Punkt [${x}, ${y}] nicht sichtbar oder verdeckt`);
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

test("Z01 Zeichenmodus: Schalter, Blick senkrecht auf die Skizzenebene, Esc beendet", async ({ page }) => {
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
    title: "Zeichenmodus ein/aus",
    images: [{ name: "z01-modus", label: "Zeichenmodus: Werkzeugleiste, Raster in der Skizzenebene, Blick von oben" }],
    code: GRID.trim(),
    caption:
      "„Skizze“ in der Ansichtsleiste schaltet den Zeichenmodus ein: die Kamera blickt senkrecht auf die Ebene der Skizze am Cursor, das Raster g und die Schrittweite (1 mm, gröber beim Herauszoomen) liegen in der Ebene, die Statuszeile sagt, was das Werkzeug als Nächstes braucht. Esc beendet den Modus.",
    checks: [`Blickrichtung z = ${cam!.dir[2].toFixed(5)}`, `Rasterlinien ${drawn["sketch-grid"]}, Schrittraster ${drawn["sketch-raster"]}`, "Esc: Modus aus"],
  });
});

test("Z02 Linie in eine neue Skizze, Klick auf den Anfang schließt", async ({ page }) => {
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
    title: "Linie",
    images: [
      { name: "z02-vor-schliessen", label: "vor dem letzten Klick: Snap „Anfang (schließt)“" },
      { name: "z02-linie", label: "nach dem Klick: close(); geschlossene Kontur" },
    ],
    code: t.trim(),
    caption:
      "Ohne sketch-Block am Cursor legt die erste Aktion einen an (am Ende der Datei). Jeder Klick ist eine Aktion: `start(…)` und die erste Linie, dann `line(to = …)` je Punkt; der Klick auf den Anfang der Kontur schreibt `close();`. Freie Punkte werden auf die Schrittweite gerundet.",
    checks: ["neuer sketch-Block", "4 Aktionen", "close() durch Klick auf den Anfang", "keine Fehler"],
  });
});

test("Z03 Rechteck auf Rasterpunkten: Referenzen, Snap-Anzeige", async ({ page }) => {
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
    title: "Rechteck auf Rasterpunkten",
    images: [
      { name: "z03-snap", label: "Snap auf Rasterpunkt: Quadrat und Text g[b, u]" },
      { name: "z03-vorschau", label: "zweite Ecke: Vorschau des Rechtecks" },
      { name: "z03-rechteck", label: "Ergebnis: Referenzen statt Zahlen" },
    ],
    code: t.trim(),
    caption:
      "Snappen auf Rasterpunkte schreibt Referenzen. Beim Rechteck aus zwei Rasterpunkten werden auch die beiden anderen Ecken Rasterpunkte (`g[c, u]`, `g[b, v]`): ändert sich eine Ebene, folgt das Rechteck.",
    checks: ["Snap-Text g[b, u] sichtbar", "Ecken als g[…]", "Vorschau 1 Polylinie"],
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

test("Z04 Kreis", async ({ page }) => {
  await openSketch(page);
  await tool(page, "circle");
  await clicks(page, [[26, 6]]);
  await hoverAt(page, 30, 6);
  await screenshot18(page, "z04-vorschau", "view");
  await clicks(page, [[30, 6]], 1);
  const t = await text(page);
  expect(t).toContain("    line(to = [20, 0]);\n    start([30, 6]);\n    arc(center = [26, 6], angle = 360);\n    close();\n}");
  await screenshot18(page, "z04-kreis", "full");
  record18({ id: "Z04", title: "Kreis", images: [{ name: "z04-vorschau", label: "Vorschau: Kreis um den Mittelpunkt" }, { name: "z04-kreis", label: "Ergebnis" }], code: t.trim(), caption: "Mittelpunkt, dann ein Punkt auf dem Kreis: `start(p); arc(center = c, angle = 360); close();` – eine eigene Kontur nach der offenen.", checks: ["start/arc(center, angle = 360)/close"] });
});

test("Z05 Bogen durch drei Punkte, Z06 Bogen um den Mittelpunkt, Z07 tangential", async ({ page }) => {
  await openSketch(page);
  await tool(page, "arc_through");
  await clicks(page, [[20, 0], [24, 4]]);
  await hoverAt(page, 20, 8);
  await screenshot18(page, "z05-vorschau", "view");
  await clicks(page, [[20, 8]], 1);
  let t = await text(page);
  expect(t).toContain("    line(to = [20, 0]);\n    arc(through = [24, 4], to = [20, 8]);\n}");
  await screenshot18(page, "z05-bogen", "full");
  record18({ id: "Z05", title: "Bogen durch drei Punkte", images: [{ name: "z05-vorschau", label: "Vorschau durch Anfang, Durchgangspunkt und Maus" }, { name: "z05-bogen", label: "Ergebnis" }], code: t.trim(), caption: "Anfang (hier das Ende der Kontur, kein neues start), Punkt auf dem Bogen, Endpunkt: `arc(through = q, to = p)`.", checks: ["arc(through = [24, 4], to = [20, 8])"] });

  // centre arc from the end [20, 8] around [16, 8], 90° counter-clockwise
  await tool(page, "arc_center");
  await clicks(page, [[20, 8], [16, 8]]);
  await hoverAt(page, 16.2, 12);
  await screenshot18(page, "z06-vorschau", "view");
  await clicks(page, [[16.2, 12]], 2);
  t = await text(page);
  expect(t).toContain("    arc(through = [24, 4], to = [20, 8]);\n    arc(center = [16, 8], angle = 90);\n}");
  await screenshot18(page, "z06-bogen", "full");
  record18({ id: "Z06", title: "Bogen um einen Mittelpunkt", images: [{ name: "z06-vorschau", label: "Vorschau mit Radiuslinie" }, { name: "z06-bogen", label: "Ergebnis" }], code: t.trim(), caption: "Anfang, Mittelpunkt, Endwinkel: `arc(center = c, angle = a)`; der Winkel rastet auf die Winkelschrittweite (15°), auf einem gesnappten Punkt entsteht `arc(center = c, to = p)`.", checks: ["arc(center = [16, 8], angle = 90)"] });

  // tangent arc from the end ([16, 12], direction −x)
  await tool(page, "arc_tangent");
  await hoverAt(page, 10, 10);
  await screenshot18(page, "z07-vorschau", "view");
  await clicks(page, [[10, 10]], 3);
  t = await text(page);
  expect(t).toContain("    arc(center = [16, 8], angle = 90);\n    arc(to = [10, 10]);\n}");
  await screenshot18(page, "z07-bogen", "full");
  record18({ id: "Z07", title: "Tangentialer Bogen", images: [{ name: "z07-vorschau", label: "Vorschau tangential am Ende der Kontur" }, { name: "z07-bogen", label: "Ergebnis" }], code: t.trim(), caption: "Ein Klick: der Bogen setzt die Richtung des letzten Segments fort, `arc(to = p)`.", checks: ["arc(to = [10, 10])"] });
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
  record18({ id: "Z08", title: "Bézier", images: [{ name: "z08-vorschau", label: "Vorschau mit Kontrollpolygon" }, { name: "z08-bezier", label: "Ergebnis" }], code: t.trim(), caption: "Anfang, zwei Kontrollpunkte, Ende: kubische Kurve `bezier(c1 = …, c2 = …, to = …)`.", checks: ["bezier(c1 = [24, 0], c2 = [27, 5], to = [27, 10])"] });
});

const RECT = `linear_extrude(2) sketch {
    start([0, 0]);
    line(to = [30, 0]);
    line(to = [30, 20]);
    line(to = [0, 20]);
    close();
}
`;

test("Z09 Ecke runden, Z10 Schließen", async ({ page }) => {
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
  record18({ id: "Z09", title: "Ecke runden", images: [{ name: "z09-vorschau", label: "Ecke gewählt, Vorschau der Rundung" }, { name: "z09-ecke", label: "Ergebnis" }], code: t.trim(), caption: "Ecke wählen (nur Ecken zwischen zwei Segmenten snappen), dann den Radius als Abstand zur Ecke: `corner(r = …)` nach dem Segment, das dort endet; an einer schon gerundeten Ecke ändert sich der Radius.", checks: ["corner(r = 4) nach line(to = [30, 20])"] });

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
  record18({ id: "Z10", title: "Schließen", images: [{ name: "z10-vorher", label: "offene Kontur (Fehler: nicht geschlossen)" }, { name: "z10-nachher", label: "nach „Schließen“: close();" }], code: t.trim(), caption: "„Schließen“ schreibt `close();` für die offene Kontur am Cursor; der Fehler „Kontur nicht geschlossen“ verschwindet.", checks: ["close(); eingefügt", "keine Fehler"] });
});

test("Z11 Snap-Arten: Endpunkt, Mitte, Schnittpunkt, horizontal, Rasterlinie, tangential, Alt frei", async ({ page }) => {
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
  await probe("Endpunkt", 20.3, 4.8, "end");
  await probe("Mitte", 10.2, 5.2, "mid");
  await probe("Schnittpunkt", 20.2, 18.5, "intersection");
  await screenshot18(page, "z11-schnitt", "view");
  await probe("Rasterpunkt", 10.2, 0.2, "grid");
  // after the start point: horizontal to it, and the grid line x = g.b
  await clicks(page, [[0, 25]]);
  const hz = await probe("horizontal", 42.4, 25.2, "horizontal");
  expect(hz.snap!.text).toBe("[42, 25]");
  expect(hz.guides).toBe(1);
  await screenshot18(page, "z11-horizontal", "view");
  const gl = await probe("Rasterlinie", 10.3, -6.4, "grid_line");
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
  const tg = await probe("tangential", 36, 13, "tangent");
  expect(tg.snap!.label).toContain("tangential");
  await screenshot18(page, "z11-tangential", "view");
  await clicks(page, [[36, 13]], 1);
  const t = await text(page);
  expect(t).toMatch(/line\(to = \[30, 15\]\);\n {4}line\(length = \d+\);\n\}/);
  record18({
    id: "Z11",
    title: "Snap-Arten",
    images: [
      { name: "z11-schnitt", label: "Schnittpunkt zweier Linien (Ring)" },
      { name: "z11-horizontal", label: "horizontal zum Anfangspunkt (Raute, Hilfslinie)" },
      { name: "z11-tangential", label: "tangential zum letzten Segment" },
    ],
    code: t.trim(),
    caption:
      "Reihenfolge: Rasterpunkte und Anker (Referenz), Anfang, Ecken, End-, Mittel-, Schnittpunkte und Bogenmitten (Zahlen); sonst tangential (schreibt `line(length = …)`), horizontal/vertikal zum vorigen Punkt und Rasterlinien (`[g.b, 40]`), sonst die Schrittweite. Alt: frei mit Schrittweite. Die Markierung zeigt die Art (Ring Punkt, Quadrat Raster, Raute Richtung, Kreuz Schritt).",
    checks: Object.entries(kinds).map(([k, v]) => `${k} → ${v}`),
  });
});

test("Z12 Undo: jede Aktion ein Schritt", async ({ page }) => {
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
  record18({ id: "Z12", title: "Undo", images: [{ name: "z12-undo", label: "nach Undo, Undo, Redo" }], code: `// nach zwei Aktionen\n${t2.trim()}\n// Strg+Z, Strg+Z, Strg+Umschalt+Z\n${t1.trim()}`, caption: "Jede Aktion ist ein eigener Verlaufsschritt (auch `start` + erste Linie zusammen); Strg+Z/Strg+Umschalt+Z wirken auch mit dem Fokus in der Ansicht.", checks: ["Undo nimmt genau eine Aktion zurück", "Redo stellt sie wieder her"] });
});

test("Z13 Griff ziehen im Zeichenmodus: Rasterreferenz wird umgeschrieben", async ({ page }) => {
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
  record18({ id: "Z13", title: "Griff ziehen", images: [{ name: "z13-vorher", label: "Griffe aller Segmente (Zeichenmodus)" }, { name: "z13-nachher", label: "Punkt auf g[c, v] gezogen" }], code: t.trim(), caption: "Im Zeichenmodus zeigt die Ansicht die Griffe aller Segmente der Skizze. Ziehen nutzt die bestehende Griff-Logik: auf einem Rasterpunkt wird die Referenz umgeschrieben, Literale werden geändert (Alt: Zahlen).", checks: ["g[b, v] → g[c, v]"] });
});

test("Z14 Segment löschen", async ({ page }) => {
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
  record18({ id: "Z14", title: "Segment löschen", images: [{ name: "z14-auswahl", label: "Auswahl: Segment unter der Maus, Klick setzt den Cursor" }, { name: "z14-geloescht", label: "nach Entf: Zeile entfernt, der Nachfolger beginnt am vorigen Punkt" }], code: t.trim(), caption: "Mit dem Werkzeug Auswahl setzt ein Klick auf ein Segment den Cursor in seine Zeile, Entf (oder „Löschen“) entfernt die Anweisung. Das einzige Segment einer Kontur nimmt die Kontur mit, ein `corner()` direkt danach ebenfalls.", checks: ["line(to = [30, 20]) entfernt"] });
});

test("Z15 Skizze unter Transformation und in rotate_extrude", async ({ page }) => {
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
    title: "Skizze unter Transformation, rotate_extrude",
    images: [
      { name: "z15-transformiert", label: "translate + rotate([90, 0, 0]): Ebene senkrecht zur y-Achse" },
      { name: "z15-rotate-extrude", label: "rotate_extrude: Profil in der xz-Ebene" },
    ],
    code: `${t.trim()}\n\n${t2.trim()}`,
    caption:
      "Die Ebene der Skizze kommt von der Engine (`OpInfo.Sketch.plane`): unter Transformationen ihr Knotensystem, im rotate_extrude die xz-Ebene, in der das Profil im Körper liegt, beim zentrierten linear_extrude dessen Boden. Kamera, Raster, Snap und Griffe nutzen dieselbe Ebene; die Werte bleiben Koordinaten der Skizze.",
    checks: [`Blick y = ${cam!.dir[1].toFixed(5)}`, "Snap-Punkt z = 20 (Welt)", `rotate_extrude: Blick y = ${cam2!.dir[1].toFixed(5)}`],
  });
});

test("Z16 Dunkles Thema", async ({ page }) => {
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
  record18({ id: "Z16", title: "Dunkles Thema", images: [{ name: "z16-dunkel", label: "Zeichenmodus im dunklen Thema" }], caption: "Raster, Vorschau, Snap-Markierung und Beschriftung nutzen die Farben des Themas (Snap-Beschriftung wie beim Griff-Snap).", checks: [`Kontrast der Snap-Beschriftung ${contrast!.toFixed(1)}:1`] });
});

test("Z17 Antwortzeiten: Snap und Vorschau beim Bewegen (Skizze mit Schleife, 42 Segmente)", async ({ page }) => {
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
    title: "Antwortzeiten",
    images: [{ name: "z17-zahnscheibe", label: "Zahnriemenscheibe (Skizze mit for-Schleife) im Zeichenmodus" }],
    caption:
      "Jede Mausbewegung schickt den Strahl an die Engine (Worker, Wasm): Snap gegen Rasterpunkte, alle End-, Mittel- und Schnittpunkte der 42 Segmente, Vorschau; höchstens eine Anfrage unterwegs, die letzte Position gewinnt. Gemessen: Anfrage bis Antwort im Store.",
    checks: [`${ms.length} Bewegungen: Median ${med.toFixed(1)} ms, 95 % ${p95.toFixed(1)} ms, max ${ms[ms.length - 1].toFixed(1)} ms`],
  });
});
