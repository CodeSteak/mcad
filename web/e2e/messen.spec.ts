// End-to-end tests for test report 19 (measure mode, Plan 18): every kind (distance with foot
// points and parallel planes, radius of cylinders and fillets, angle of a chamfer, length,
// area, body/part), measuring in the section (contour snap, radius of a section circle, wall
// thickness, region area, centre distance), several measurements, delete, copy as text /
// comment / `mcad measure`, provenance, the dark theme and answer times.

import { expect, test, type Page } from "@playwright/test";
import { hook, loadText, measureBar, measureKind, mode, openApp, openExample, record19, screenshot19, waitScene, writeMetrics19 } from "./helpers";

test.beforeEach(async ({ page }) => {
  page.on("pageerror", (e) => console.log("pageerror:", e.message));
  await page.addInitScript(() => localStorage.setItem("cad-colors-open", "0"));
});

type P3 = [number, number, number];

interface Pick {
  kind: string;
  label: string;
  point: P3;
  reference: string;
  origin: string[];
}

interface MeasureInfo {
  active: boolean;
  kind: string | null;
  inSection: boolean;
  picks: Pick[];
  hover: (Pick & { lines: number }) | null;
  prompt: string | null;
  message: string | null;
  section: { regions: number; contours: number; area: number; exact: boolean; max_dev: number; message?: string } | null;
  answers: number;
  rtt: Array<{ ms: number; click: boolean }>;
  selected: number | null;
  results: Array<{
    id: number;
    kind: string;
    value: number;
    unit: string;
    label: string;
    text: string;
    cli: string[];
    exact: boolean;
    values: Array<{ key: string; value: number }>;
    details: string[];
    at: P3;
    version: number;
    lines: number;
    guides: number;
    origin: string[];
  }>;
  view: { drawn: Record<string, number>; labels: Array<{ id: number; text: string; x: number; y: number; visible: boolean }>; hoverLabel: string | null } | null;
}

type W = {
  __cad: {
    measure(): MeasureInfo;
    measureEnter(kind?: string, inSection?: boolean): void;
    measureKind(kind: string): void;
    measureExit(): void;
    screenOf(p: P3): { x: number; y: number; visible: boolean } | null;
    setSection(s: Record<string, unknown>): void;
    setView(v: string): void;
    viewFrom(d: P3): void;
    contrastOf(a: string, b: string): number | null;
    setThemePref(p: string): void;
    state(): { notice: string | null; lastCli?: string | null };
  };
};

const info = (page: Page) => page.evaluate(() => (window as unknown as W).__cad.measure());

async function waitMeasure(page: Page, pred: string, arg: unknown = null): Promise<MeasureInfo> {
  try {
    await page.waitForFunction(
      ([src, a]) => {
        const m = (window as unknown as W).__cad.measure();
        // eslint-disable-next-line no-new-func
        return new Function("m", "arg", `return (${src})(m, arg);`)(m, a);
      },
      [pred, arg] as const,
      { timeout: 20_000, polling: 50 },
    );
  } catch (e) {
    throw new Error(`${(e as Error).message}\nMessen: ${JSON.stringify(await info(page))}`);
  }
  return info(page);
}

async function scr(page: Page, p: P3): Promise<{ x: number; y: number }> {
  const q = await page.evaluate((pt) => (window as unknown as W).__cad.screenOf(pt), p);
  if (!q || !q.visible) throw new Error(`Punkt [${p.join(", ")}] nicht sichtbar oder verdeckt`);
  return { x: q.x, y: q.y };
}

async function enter(page: Page, kind = "distance"): Promise<MeasureInfo> {
  await mode(page, "Messen").click();
  await waitMeasure(page, "(m) => m.active && m.answers > 0");
  if (kind !== "distance") await measureKind(page, kind).click();
  return waitMeasure(page, "(m, k) => m.kind === k && m.prompt", kind);
}

/** Moves the mouse to a model point and waits for the element under it. */
async function hoverAt(page: Page, p: P3, kind?: string): Promise<MeasureInfo> {
  const q = await scr(page, p);
  const before = (await info(page)).answers;
  await page.mouse.move(q.x + 0.3, q.y + 0.3);
  return waitMeasure(page, "(m, a) => m.answers > a[0] && m.hover !== null && (a[1] === null || m.hover.kind === a[1])", [before, kind ?? null]);
}

/** Clicks model points (hover first, so the element is known). */
async function clickAt(page: Page, pts: Array<{ p: P3; kind?: string }>): Promise<MeasureInfo> {
  for (const { p, kind } of pts) {
    await hoverAt(page, p, kind);
    const q = await scr(page, p);
    const b = await info(page);
    await page.mouse.click(q.x + 0.3, q.y + 0.3);
    // A hover answer still on its way also counts as an answer: wait for the click's own effect
    // (a pick, a measurement or the reason why the element was not accepted).
    await waitMeasure(
      page,
      "(m, a) => m.answers > a[0] && (m.picks.length !== a[1] || m.results.length !== a[2] || m.message !== null)",
      [b.answers, b.picks.length, b.results.length],
    );
  }
  return info(page);
}

const value = (r: MeasureInfo["results"][number], key: string) => r.values.find((v) => v.key === key)?.value;

const BOXES = `grid g {
  x: a = 0, b = 20;
  y: v = 0, h = 10;
  z: u = 0, o = 8;
}
box(g[a, v, u], g[b, h, o]);
translate([30, 0, 0]) cube([10, 10, 12]);
`;

test("M01 Messmodus: Schalter, Kontextleiste, Snap-Anzeige, Esc beendet", async ({ page }) => {
  await openApp(page);
  await loadText(page, BOXES);
  const m = await enter(page);
  expect(m.prompt).toContain("Abstand: erstes Element");
  await expect(measureBar(page)).toBeVisible();
  await expect(page.getByRole("region", { name: "Messungen" })).toBeVisible();
  for (const k of ["distance", "radius", "angle", "length", "area", "body"]) await expect(measureKind(page, k)).toBeEnabled();
  await expect(measureKind(page, "wall")).toBeDisabled();
  const h = await hoverAt(page, [10, 5, 8], "face");
  expect(h.hover!.label).toBe("Fläche · Ebene");
  expect(h.hover!.origin).toContain("Rasterebene g.o");
  await expect(page.getByTestId("measure-hover-label")).toHaveText("Fläche · Ebene");
  await expect(page.getByTestId("status-measure")).toContainText("Fläche · Ebene");
  await screenshot19(page, "m01-flaeche", "view");
  const v = await hoverAt(page, [20, 0, 8], "vertex");
  expect(v.hover!.point).toEqual([20, 0, 8]);
  expect(v.hover!.origin).toContain("Rasterpunkt g[b, v, o]");
  await screenshot19(page, "m01-eckpunkt", "view");
  const mid = await hoverAt(page, [10, 0, 8], "mid");
  expect(mid.hover!.label).toContain("Kantenmitte");
  await page.keyboard.press("Escape");
  await waitMeasure(page, "(m) => !m.active");
  await expect(measureBar(page)).toHaveCount(0);
  record19({
    id: "M01",
    title: "Messmodus ein/aus, Snap",
    images: [
      { name: "m01-flaeche", label: "Maus über der Deckfläche: Fläche hervorgehoben, Beschriftung, Provenienz g.o" },
      { name: "m01-eckpunkt", label: "nahe einer Ecke: Snap auf den Eckpunkt (Rasterpunkt g[b, v, o])" },
    ],
    code: BOXES.trim(),
    caption:
      "„Messen“ in der Ansichtsleiste schaltet den Messmodus ein: die Kontextleiste zeigt die Messarten (Abstand, Radius, Winkel zuerst wie im UI-Konzept, dazu Länge, Fläche, Körper, Wandstärke im Schnitt) und „Im Schnitt“. Unter der Maus fängt die Engine Eckpunkte, Kantenmitten, Kreismitten, Kanten und Flächen (Snap-Radius aus den Einstellungen); Beschriftung und Statuszeile nennen das Element und woher es kommt. Esc beendet den Modus.",
    checks: ["Hover Fläche mit Rasterebene g.o", "Eckpunkt [20, 0, 8] = Rasterpunkt g[b, v, o]", "Kantenmitte", "Esc: Modus aus"],
  });
});

test("M02 Abstand: parallele Flächen (Normalabstand) und Eckpunkte mit Fußpunkten", async ({ page }) => {
  await openApp(page);
  await loadText(page, BOXES);
  await enter(page);
  let m = await clickAt(page, [{ p: [10, 5, 8], kind: "face" }]);
  expect(m.picks.length).toBe(1);
  expect(m.prompt).toContain("zweites");
  m = await clickAt(page, [{ p: [35, 5, 12], kind: "face" }]);
  expect(m.results.length).toBe(1);
  const r = m.results[0];
  expect(r.kind).toBe("distance");
  expect(r.value).toBeCloseTo(4, 12);
  expect(r.exact).toBe(true);
  expect(r.details.join("\n")).toContain("parallele Ebenen: Normalabstand");
  await expect(page.getByTestId(`measure-label-${r.id}`)).toHaveText("4 mm");
  await expect(page.getByTestId(`measure-value-${r.id}`)).toHaveText("4 mm");
  // vertex to vertex
  m = await clickAt(page, [{ p: [0, 0, 8], kind: "vertex" }, { p: [40, 0, 12], kind: "vertex" }]);
  const d = m.results[1];
  expect(d.value).toBeCloseTo(Math.hypot(40, 0, 4), 9);
  expect(value(d, "dx")).toBeCloseTo(40, 9);
  expect(value(d, "dz")).toBeCloseTo(4, 9);
  expect(d.cli).toEqual(["--distance", "vertex@[0,0,8]", "vertex@[40,0,12]"]);
  const v = (await info(page)).view!;
  expect(v.drawn["measure-line"]).toBeGreaterThanOrEqual(2);
  expect(v.labels.length).toBe(2);
  await screenshot19(page, "m02-abstand", "full");
  record19({
    id: "M02",
    title: "Abstand",
    images: [{ name: "m02-abstand", label: "zwei Messungen: Normalabstand der Deckflächen 4 mm, Eckpunkt–Eckpunkt mit Δ" }],
    code: BOXES.trim(),
    caption:
      "Zwei Klicks: kleinster Abstand der Elemente mit Fußpunkten, aus der exakten B-Rep (abwechselnde Projektion auf Ebenen, Zylinder, … in geschlossener Form). Für parallele Ebenen gilt der Normalabstand. Die Maßlinie verbindet die Fußpunkte, die Beschriftung steht in der Mitte; die Liste rechts zeigt Werte, Δx/Δy/Δz und Herkunft.",
    checks: [`Normalabstand ${r.value} mm (exakt)`, `Eckpunkte ${d.value.toFixed(6)} mm, Δx 40, Δz 4`, `mcad measure ${d.cli.join(" ")}`],
  });
});

test("M03 Radius: Zylinder mit Achse, Verrundung, Kreiskante", async ({ page }) => {
  await openApp(page);
  const src = "cylinder(h = 10, r = 5);\ntranslate([20, -5, 0]) cube([20, 10, 8], $fillet = 2);\n";
  await loadText(page, src);
  await enter(page, "radius");
  const c = Math.SQRT1_2;
  // plane: nothing to pick for a radius
  let m = await clickAt(page, [{ p: [5 * c, -5 * c, 5], kind: "face" }]);
  let r = m.results[0];
  expect(r.label).toBe("R 5 · ⌀ 10");
  expect(r.guides).toBeGreaterThanOrEqual(1);
  expect(r.details.join("\n")).toContain("Achse [0, 0, 1]");
  // fillet along the top edge y = −5 (front): cylinder of radius 2
  m = await clickAt(page, [{ p: [30, -5 + 2 - 2 * c, 6 + 2 * c], kind: "face" }]);
  r = m.results[1];
  expect(r.value).toBeCloseTo(2, 9);
  expect(r.label).toBe("R 2 · ⌀ 4");
  // circular edge of the cylinder top
  m = await clickAt(page, [{ p: [5 * c, -5 * c, 10], kind: "edge" }]);
  expect(m.results[2].label).toBe("R 5 · ⌀ 10");
  await screenshot19(page, "m03-radius", "full");
  record19({
    id: "M03",
    title: "Radius und Durchmesser",
    images: [{ name: "m03-radius", label: "Radius des Zylinders (Achse gestrichelt), der Verrundung (R 2) und der Kreiskante" }],
    code: src.trim(),
    caption:
      "Radius einer Kreiskante oder runden Fläche aus der B-Rep: Zylinder, Kegel, Kugel, Torus in geschlossener Form, Verrundungen an Kreiskanten (Rotationsfläche eines Kreisbogens) erkannt, Freiform über die Krümmung. Gezeichnet werden Radiuslinie, Mittelpunkt und Achse.",
    checks: ["Zylinder R 5 · ⌀ 10 mit Achse", "Verrundung R 2", "Kreiskante R 5"],
  });
});

test("M04 Winkel einer Fase und zweier Flächen", async ({ page }) => {
  await openApp(page);
  const src = "cube([20, 10, 8], $chamfer = 2);\n";
  await loadText(page, src);
  await enter(page, "angle");
  let m = await clickAt(page, [{ p: [19, 5, 7], kind: "face" }, { p: [10, 5, 8], kind: "face" }]);
  const a = m.results[0];
  expect(a.unit).toBe("°");
  expect(a.value).toBeCloseTo(45, 9);
  expect(value(a, "opening")).toBeCloseTo(135, 9);
  await expect(page.getByTestId(`measure-label-${a.id}`)).toHaveText("45°");
  m = await clickAt(page, [{ p: [10, 5, 8], kind: "face" }, { p: [20, 5, 4], kind: "face" }]);
  expect(m.results[1].value).toBeCloseTo(90, 9);
  await screenshot19(page, "m04-winkel", "full");
  record19({
    id: "M04",
    title: "Winkel",
    images: [{ name: "m04-winkel", label: "Fase 45° (Öffnungswinkel 135°) und Deckfläche–Seite 90°, mit Winkelbogen" }],
    code: src.trim(),
    caption:
      "Winkel zwischen zwei Kanten oder Flächen: Ebenen über ihre Normalen (angezeigt der Winkel zwischen den Ebenen, dazu der Öffnungswinkel an der Schnittgeraden), Zylinder und Kegel über ihre Achse, Kanten an einer gemeinsamen Ecke von der Ecke weg. Der Bogen liegt an der Schnittgeraden.",
    checks: ["Fase 45°", "Öffnungswinkel 135°", "Deckfläche–Seite 90°"],
  });
});

test("M05 Länge, Fläche, Körper/Bauteil mit Hülle", async ({ page }) => {
  await openApp(page);
  await openExample(page, "spec/gehaeuse.mcad");
  await waitScene(page);
  await enter(page, "length");
  let m = await clickAt(page, [{ p: [60, 15, 20], kind: "edge" }]);
  expect(m.results[0].value).toBeCloseTo(30, 9);
  await measureKind(page, "area").click();
  await waitMeasure(page, "(m) => m.kind === 'area'");
  m = await clickAt(page, [{ p: [60, 15, 10], kind: "face" }]);
  expect(m.results[1].value).toBeCloseTo(600, 9);
  await measureKind(page, "body").click();
  await waitMeasure(page, "(m) => m.kind === 'body'");
  m = await clickAt(page, [{ p: [60, 15, 10], kind: "body" }]);
  const b = m.results[2];
  expect(value(b, "volume")).toBeCloseTo(8460, 0);
  expect([value(b, "size_x"), value(b, "size_y"), value(b, "size_z")]).toEqual([60, 30, 20]);
  expect(b.details[0]).toBe("Bauteil k (Zeile 21)");
  expect(b.guides).toBe(12);
  await screenshot19(page, "m05-koerper", "full");
  record19({
    id: "M05",
    title: "Länge, Fläche, Körper",
    images: [{ name: "m05-koerper", label: "Kantenlänge 30 mm, Fläche 600 mm², Bauteil k: Volumen und Hülle 60 × 30 × 20" }],
    code: "examples/spec/gehaeuse.mcad",
    caption:
      "Länge einer Kante (exakt für Geraden und Bögen, sonst Gauß-Legendre), Fläche einer Fläche (eben exakt über den Rand, gekrümmt über projizierte Unterteilung), Körper: das Bauteil der gewählten Fläche (benannte Instanz k) mit Volumen und Oberfläche aus dem Kernel und der Hülle aus der B-Rep (gestrichelte Box).",
    checks: ["Länge 30 mm", "Fläche 600 mm²", "Bauteil k: 8460 mm³, 60 × 30 × 20"],
  });
});

const PLATE = `difference() {
  cube([40, 20, 6]);
  translate([10, 10, -1]) cylinder(h = 8, r = 2);
  translate([28, 10, -1]) cylinder(h = 8, r = 3);
}
`;

test("M06 Im Schnitt: Kreis der Bohrung, Wandstärke, Fläche, Kreismitten", async ({ page }) => {
  await openApp(page);
  await loadText(page, PLATE);
  await page.evaluate(() => (window as unknown as W).__cad.setSection({ on: true, axis: "z", point: [0, 0, 3], normal: [0, 0, 1], flip: false, snap: null }));
  await page.evaluate(() => (window as unknown as W).__cad.setView("top"));
  await waitScene(page);
  await enter(page, "radius");
  await measureBar(page).getByRole("button", { name: "Im Schnitt" }).click();
  await expect(measureBar(page).getByRole("button", { name: "Im Schnitt" })).toHaveAttribute("aria-pressed", "true");
  let m = await waitMeasure(page, "(m) => m.inSection && m.section && m.section.regions === 1");
  expect(m.section!.exact).toBe(true);
  expect(m.section!.area).toBeCloseTo(800 - 13 * Math.PI, 9);
  await expect(measureKind(page, "wall")).toBeEnabled();
  await expect(measureKind(page, "body")).toBeDisabled();
  m = await clickAt(page, [{ p: [12, 10, 3], kind: "section_edge" }]);
  expect(m.results[0].label).toBe("R 2 · ⌀ 4");
  expect(m.results[0].cli.slice(0, 3)).toEqual(["--section", "z=3", "--radius"]);
  await measureKind(page, "wall").click();
  await waitMeasure(page, "(m) => m.kind === 'wall'");
  m = await clickAt(page, [{ p: [8, 10, 3] }]);
  expect(m.message, JSON.stringify(m)).toBeNull();
  expect(m.results[1].value).toBeCloseTo(8, 2);
  await measureKind(page, "area").click();
  await waitMeasure(page, "(m) => m.kind === 'area'");
  m = await clickAt(page, [{ p: [35, 4, 3], kind: "region" }]);
  expect(m.results[2].value).toBeCloseTo(800 - 13 * Math.PI, 9);
  await measureKind(page, "distance").click();
  await waitMeasure(page, "(m) => m.kind === 'distance'");
  m = await clickAt(page, [{ p: [10, 10, 3], kind: "section_center" }, { p: [28, 10, 3], kind: "section_center" }]);
  expect(m.results[3].value).toBeCloseTo(18, 9);
  await screenshot19(page, "m06-schnitt", "full");
  record19({
    id: "M06",
    title: "Messen im Schnitt",
    images: [{ name: "m06-schnitt", label: "Schnitt z = 3: Radius der Bohrung, Wandstärke 8 mm, Schnittfläche, Abstand der Kreismitten 18 mm" }],
    code: PLATE.trim(),
    caption:
      "„Im Schnitt“ misst in der aktiven Schnittebene: die Konturen kommen aus dem Kernel (Ebene ∩ B-Rep über einen Boolean mit dem behaltenen Halbraum), die Segmente auf Zylindern, Kegeln, Kugeln und Tori in geschlossener Form (hier exakte Kreise). Snap auf Eckpunkte, Mitten und Kreismitten der Kontur; Wandstärke quer zur Kontur; Fläche eines Bereichs (Kontur minus Löcher).",
    checks: ["Schnitt exakt, 1 Bereich", "Bohrung R 2 · ⌀ 4", "Wandstärke 8 mm", `Fläche ${(800 - 13 * Math.PI).toFixed(4)} mm²`, "Kreismitten 18 mm"],
  });
});

test("M07 Mehrere Messungen: Auswahl, Löschen, Kopieren als Text, Kommentar und mcad measure", async ({ page }) => {
  await openApp(page);
  await loadText(page, BOXES);
  await enter(page, "length");
  let m = await clickAt(page, [{ p: [10, 0, 8], kind: "edge" }]);
  m = await clickAt(page, [{ p: [20, 5, 8], kind: "edge" }]);
  m = await clickAt(page, [{ p: [35, 0, 12], kind: "edge" }]);
  expect(m.results.map((r) => r.value)).toEqual([20, 10, 10]);
  const [a, b] = m.results;
  await page.getByTestId(`measure-item-${a.id}`).getByRole("button", { name: "Als mcad measure kopieren" }).click();
  await expect(page.getByTestId("status-notice")).toContainText("mcad measure");
  const cli = await page.evaluate(() => (window as unknown as { __cad: { state(): { lastCli: string | null } } }).__cad.state().lastCli ?? null);
  await page.getByTestId(`measure-item-${a.id}`).getByRole("button", { name: "Als Kommentar kopieren" }).click();
  await expect(page.getByTestId("status-notice")).toContainText("// Messung: Länge");
  await page.getByTestId(`measure-label-${b.id}`).click();
  await waitMeasure(page, "(m, id) => m.selected === id", b.id);
  await screenshot19(page, "m07-liste", "full");
  // Entf deletes the selected one
  await page.keyboard.press("Delete");
  m = await waitMeasure(page, "(m) => m.results.length === 2");
  expect(m.results.map((r) => r.id)).toEqual([a.id, m.results[1].id]);
  await page.getByTestId(`measure-item-${a.id}`).getByRole("button", { name: "Messung löschen" }).click();
  m = await waitMeasure(page, "(m) => m.results.length === 1");
  await page.getByRole("region", { name: "Messungen" }).getByRole("button", { name: "Alle löschen" }).click();
  m = await waitMeasure(page, "(m) => m.results.length === 0");
  expect(m.view!.labels.length).toBe(0);
  record19({
    id: "M07",
    title: "Liste der Messungen",
    images: [{ name: "m07-liste", label: "drei Längen, eine gewählt (Akzentfarbe in Ansicht und Liste)" }],
    code: BOXES.trim(),
    caption:
      "Mehrere Messungen bleiben gleichzeitig sichtbar (auch nach dem Beenden des Modus). Klick auf Beschriftung oder Eintrag wählt eine Messung, Entf oder × löscht sie, „Alle löschen“ alle. Jede Messung lässt sich als Text, als Code-Kommentar (`// Messung: …`) und als `mcad measure`-Aufruf kopieren (wie „Als mcad render kopieren“ beim Schnitt).",
    checks: [`kopiert: ${cli ?? "–"}`, "Kommentar // Messung: Länge …", "Entf löscht die gewählte", "Alle löschen"],
  });
  expect(cli).toMatch(/^mcad measure [\w.-]+\.mcad --length 'edge@\[[0-9.]+,0,8\]'$/);
});

test("M08 Provenienz: Rasterlinie einer Bohrung, Instanz, Anker", async ({ page }) => {
  await openApp(page);
  await openExample(page, "spec/lochplatte.mcad");
  await waitScene(page);
  await page.evaluate(() => (window as unknown as W).__cad.setView("top"));
  await waitScene(page);
  await enter(page, "distance");
  const h = await hoverAt(page, [18, 6, 4], "center");
  expect(h.hover!.origin.join("\n")).toContain("Instanz p: lochplatte");
  await screenshot19(page, "m08-kreismitte", "view");
  const m = await clickAt(page, [{ p: [18, 6, 4], kind: "center" }, { p: [30, 6, 4], kind: "center" }]);
  expect(m.results[0].value).toBeCloseTo(12, 9);
  await expect(page.getByTestId(`measure-origin-${m.results[0].id}`)).toContainText("lochplatte");
  await screenshot19(page, "m08-provenienz", "full");
  record19({
    id: "M08",
    title: "Bezug zur Sprache",
    images: [
      { name: "m08-kreismitte", label: "Kreismitte einer Bohrung (Fase/Verrundung) mit Herkunft" },
      { name: "m08-provenienz", label: "Abstand der Kreismitten = Teilung 12 mm, Herkunft in der Liste" },
    ],
    code: "examples/spec/lochplatte.mcad",
    caption:
      "Zu jedem gewählten Element nennt die Engine die Codezeilen der Flächen (Tags → Knoten), die benannte Instanz (p: lochplatte) und Raster oder Anker, auf denen es liegt: Rasterpunkte und -ebenen, Rasterlinien für Achsen (g[loch[0], mitte, *]), Anker (p.erstes_loch).",
    checks: ["Kreismitte mit Instanz p", "Abstand 12 mm = pitch"],
  });
});

test("M09 Dunkles Thema: Beschriftungen lesbar", async ({ page }) => {
  await openApp(page);
  await page.evaluate(() => (window as unknown as W).__cad.setThemePref("dark"));
  await loadText(page, BOXES);
  await enter(page);
  const m = await clickAt(page, [{ p: [10, 5, 8], kind: "face" }, { p: [35, 5, 12], kind: "face" }]);
  const r = m.results[0];
  await expect(page.getByTestId(`measure-label-${r.id}`)).toBeVisible();
  const ratio = await page.evaluate((id) => (window as unknown as W).__cad.contrastOf(`[data-testid="measure-label-${id}"]`, `[data-testid="measure-label-${id}"]`), r.id);
  expect(ratio).not.toBeNull();
  await screenshot19(page, "m09-dunkel", "full");
  record19({
    id: "M09",
    title: "Dunkles Thema",
    images: [{ name: "m09-dunkel", label: "Messung im dunklen Thema" }],
    code: BOXES.trim(),
    caption: "Maßlinien und Beschriftungen nutzen die Tokens `--view-measure`/`--on-measure` (hell: Dunkelgold auf weißer Schrift, dunkel: Gold mit dunkler Schrift); contrast.test.ts prüft Schrift ≥ 4,5:1 und Linie zum Hintergrund ≥ 3:1.",
    checks: [`Kontrast der Beschriftung ${ratio?.toFixed(2)}:1`],
  });
  expect(ratio!).toBeGreaterThanOrEqual(4.5);
});

test("M10 Antwortzeiten beim Fahren mit der Maus", async ({ page }) => {
  await openApp(page);
  await openExample(page, "spec/lochplatte.mcad");
  await waitScene(page);
  await enter(page);
  const a = await scr(page, [2, 2, 4]);
  const b = await scr(page, [50, 10, 4]);
  for (let i = 0; i <= 40; i++) {
    await page.mouse.move(a.x + ((b.x - a.x) * i) / 40, a.y + ((b.y - a.y) * i) / 40);
    await page.waitForTimeout(16);
  }
  const m = await waitMeasure(page, "(m) => m.answers > 5");
  const hover = m.rtt.filter((x) => !x.click).map((x) => x.ms).sort((x, y) => x - y);
  const med = hover[Math.floor(hover.length / 2)];
  const p95 = hover[Math.floor(hover.length * 0.95)];
  writeMetrics19({ hoverRtt: { n: hover.length, median: med, p95, max: hover[hover.length - 1] } });
  record19({
    id: "M10",
    title: "Antwortzeiten",
    images: [],
    caption: "Mausfahrt über die Lochplatte (41 Positionen, je 16 ms): höchstens eine Anfrage unterwegs, die letzte Position gewinnt. Gemessen von der Anfrage bis zur Antwort im Browser (Worker, Wasm-Kernel, SwiftShader ohne GPU).",
    checks: [`${hover.length} Antworten, Median ${med.toFixed(1)} ms, 95 % ${p95.toFixed(1)} ms`],
  });
  expect(med).toBeLessThan(500);
});

test("M11 Sketch-Editor und Messen schließen sich aus; Modelländerung verwirft die Auswahl", async ({ page }) => {
  await openApp(page);
  await loadText(page, BOXES);
  await enter(page);
  await clickAt(page, [{ p: [10, 5, 8], kind: "face" }]);
  // the model changes: the pick of the old version is dropped
  await hook(page, (c, t) => c.setText(t as string), BOXES.replace("o = 8", "o = 9"));
  await waitMeasure(page, "(m) => m.picks.length === 0");
  await expect(page.getByTestId("status-notice")).toContainText("Modell geändert");
  await waitScene(page);
  await mode(page, "Skizze").click();
  await waitMeasure(page, "(m) => !m.active");
  await mode(page, "Messen").click();
  await waitMeasure(page, "(m) => m.active");
  await page.waitForFunction(() => (window as unknown as { __cad: { sketch(): unknown } }).__cad.sketch() === null);
});
