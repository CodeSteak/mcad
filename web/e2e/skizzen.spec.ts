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

const PROFIL = `linear_extrude(5) sketch {
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

test("S01 Segmentzeile: Skizze in ihrer Ebene, Segment hervorgehoben", async ({ page }) => {
  await openApp(page);
  await loadText(page, PROFIL);
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
    title: "Skizze unter dem Cursor",
    images: [
      { name: "s01-bogen", label: "Cursor auf arc(…) (Zeile 4)" },
      { name: "s01-loch", label: "Cursor im hole-Block (Zeile 10)" },
    ],
    code: PROFIL.trim(),
    caption:
      "Cursor auf einer Segmentzeile: die Skizze wird in ihrer Ebene gezeichnet – Umriss mit leichter Füllung (Netz der Engine, Modus „profile“, durch den Körper sichtbar), die Segmente als Linien, das Segment der Zeile dick in der Akzentfarbe; Startpunkt jeder Kontur als Ring mit Richtungspfeil, Bogenmittelpunkte als gestrichelte Hilfslinien.",
    checks: [`aktives Segment: arc (Zeile 4), Pixel ${act.changed}`, `Startmarken: ${start.found}`, "Zeile 10: Bogen im Loch aktiv"],
  });
});

test("S02 Bézier: Kontrollpolygon, Griff am Kontrollpunkt", async ({ page }) => {
  await openApp(page);
  await loadText(page, PROFIL);
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
    title: "Bézier-Segment",
    images: [{ name: "s02-bezier", label: "Cursor auf bezier(c1 = …, to = …)" }],
    code: "bezier(c1 = [45, 40], to = [20, 35]);",
    caption: "Quadratische Bézier-Kurve: Kontrollpolygon (Anfang – c1 – Ende) gestrichelt als Hilfslinie, Griffe am Endpunkt und am Kontrollpunkt.",
    checks: [`Hilfslinien: ${sk.helpers}`, `Pixel der Hilfslinien: ${help.changed}`, "2 Punktgriffe (to, c1)"],
  });
});

test("S03 Griff ziehen ändert das Literal des Endpunkts", async ({ page }) => {
  await openApp(page);
  await loadText(page, PROFIL);
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
    title: "Endpunkt ziehen",
    images: [
      { name: "s03-vorher", label: "vorher: line(to = [40, 0])" },
      { name: "s03-nachher", label: `nachher: line(to = [${m[1]}, ${m[2]}])` },
    ],
    code: `// vorher\nline(to = [40, 0]);\n// nach dem Ziehen (Alt, frei)\n${m[0]}`,
    caption: "Der Punktgriff am Ende der Geraden mit der Maus gezogen (Alt: frei, Raster 1 mm): das Literal in der Zeile ändert sich, die Skizze folgt live; der folgende Bogen beginnt am neuen Punkt.",
    checks: [`Literal: [40, 0] → [${m[1]}, ${m[2]}]`, "arc-Zeile unverändert"],
  });
});

test("S04 Fehler je Segmentzeile", async ({ page }) => {
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
    title: "Fehler je Segmentzeile",
    images: [{ name: "s04-fehler", label: "unmöglicher Bogen, Kontur nicht geschlossen" }],
    code: src.trim(),
    caption: "Ein Bogen mit Radius kleiner als die halbe Sehne und eine Kontur ohne close(): beide Fehler stehen an ihrer Segmentzeile (rot, Meldung am Zeilenende); die Skizze bleibt mit der fehlenden Schlusslinie (rot gestrichelt) sichtbar.",
    checks: errs,
  });
});

test("S05 Offene Skizze als sweep-Pfad, Namensraum in der Pfadleiste", async ({ page }) => {
  await openApp(page);
  const src = `module teile {
    module rohr(r = 4) {
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
teile.rohr(r = 3);
`;
  await loadText(page, src);
  const s = await cursorOn(page, 7, 18, "teile.rohr", "(s) => (s.view.plan.sketches ?? []).length === 1");
  const sk = sketches(s)[0];
  expect(sk.closed).toBe(false);
  expect(sk.open).toBe(1);
  expect(sk.active.map((a) => a.kind)).toEqual(["arc"]);
  expect(s.path).toEqual(["teile", "rohr"]);
  await expect(page.getByTestId("pathbar")).toContainText("teile");
  await expect(page.getByTestId("pathbar")).toContainText("rohr");
  const end = await layerPixels(page, "sketch:end");
  expect(end.found).toBe(1);
  await screenshot14(page, "s05-pfad", "view");
  // the call from outside with a dot: the tube in the file context
  const f = await cursorOn(page, 14, 2, null, "(s) => s.view.plan.items.some(i => i.label === 'rohr')");
  expect(f.view!.plan.items.find((i) => i.label === "rohr")!.mode).toBe("full");
  await screenshot14(page, "s05-aufruf", "view");
  record14({
    id: "S05",
    title: "Sweep-Pfad und Namensraum",
    images: [
      { name: "s05-pfad", label: "Kontext teile › rohr, Cursor auf dem Bogen des Pfads" },
      { name: "s05-aufruf", label: "Aufruf teile.rohr(r = 3) in der Datei" },
    ],
    code: src.trim(),
    caption:
      "Eine offene Skizze (closed = false) ist der Pfad des sweep: ohne Füllung, Start als Ring, Ende als Quadrat; der Kreis läuft entlang Gerade – tangentialem Bogen – Gerade (ein Kurvenzug). Das Modul rohr steht im Namensraum teile: die Pfadleiste zeigt „teile › rohr“, von außen wird es als teile.rohr(r = 3) aufgerufen.",
    checks: [`Pfadleiste: ${s.path.join(" › ")}`, "offene Kontur: 1, Endmarke gezeichnet", "Aufruf in der Datei voll dargestellt"],
  });
});
