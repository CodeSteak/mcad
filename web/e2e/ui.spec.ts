// End-to-end tests of the browser UI (Phase A) in Chromium against the production build.
// Every scenario records a screenshot and a caption for test report 2. Waiting is done on
// scene versions and view state (window.__cad), never with fixed sleeps.

import { readFileSync } from "node:fs";
import { expect, test } from "@playwright/test";
import { cad, clickLine, exportAs, fileMenu, hoverText, lineEl, openApp, openExample, record, screenshot, setTheme, standardView, waitScene, waitView, writeMetrics } from "./helpers";

test.beforeEach(async ({ page }) => {
  page.on("pageerror", (e) => console.log("pageerror:", e.message));
});

test("S01 Laden: Editor, Ansicht und erste Szene", async ({ page }) => {
  const s = await openApp(page);
  expect(s.fileName).toBe("housing.mcad");
  expect(s.view!.objects).toBeGreaterThan(0);
  expect(s.view!.plan.base).toBe("full");
  // Anchors of the named instance k (diamonds).
  expect(s.view!.plan.anchors).toBe(2);
  await expect(page.getByTestId("status-errors")).toHaveText("0 Fehler · 0 Warnungen");
  await expect(page.getByRole("navigation", { name: "Pfadleiste" })).toHaveText("housing.mcad");
  const nav = await page.evaluate(() => performance.getEntriesByType("navigation")[0]?.startTime ?? 0);
  writeMetrics({ firstSceneMs: s.metrics.firstSceneMs! - nav, firstEvalMs: s.metrics.evalMs[0] });
  await screenshot(page, "s01-laden");
  record({
    id: "S01",
    title: "Laden",
    image: "s01-laden",
    caption: "Startzustand: Spec-Beispiel housing.mcad links im Editor, rechts die Szene mit Boden, Achsenkreuz und den Ankern der benannten Instanz k (Rauten, mit Pfeil bei dir).",
  });
});

test("S02 Beispiel öffnen", async ({ page }) => {
  await openApp(page);
  const v0 = (await cad(page)).shownVersion!;
  const s = await openExample(page, "spec/hole_plate.mcad");
  expect(s.shownVersion).toBeGreaterThan(v0);
  expect(s.fileName).toBe("hole_plate.mcad");
  await expect(lineEl(page, 3)).toContainText("module hole_plate");
  expect(s.view!.objects).toBeGreaterThan(0);
  await screenshot(page, "s02-beispiel");
  record({
    id: "S02",
    title: "Beispiel öffnen",
    image: "s02-beispiel",
    caption: "Beispielmenü → spec/hole_plate.mcad: der Editor lädt die Datei, die Ansicht zeigt nach der Auswertung die Platte mit Bohrungen, Lasche und den Anker p.first_hole.",
  });
});

test("S03 Cursor in Zeile: Knoten voll, Rest gedimmt, Transformation mit Koordinatenkreuzen", async ({ page }) => {
  await openApp(page);
  await clickLine(page, 22, 20);
  let s = await waitView(page, "(s) => s.view.selection.line === 22 && s.view.plan.base === 'dim'");
  expect(s.view!.plan.items.some((i) => i.mode === "full" && i.line === 22)).toBe(true);
  // Cursor on `lid()`: the module call is selected, the rest of the module dimmed.
  await screenshot(page, "s03a-cursor-lid");
  await clickLine(page, 22, 2);
  s = await waitView(page, "(s) => s.view.selection.line === 22 && s.view.plan.frames === 2");
  const labels = s.view!.plan.items.map((i) => `${i.mode}:${i.label}`);
  expect(labels).toContain("full:translate");
  expect(labels).toContain("outline:lid");
  expect(s.view!.plan.base).toBe("dim");
  await screenshot(page, "s03b-cursor-translate");
  // Pixel check: the dimmed base is lighter/transparent compared with the full drawing.
  record({
    id: "S03",
    title: "Cursor in Zeile",
    image: "s03b-cursor-translate",
    caption: "Cursor auf translate(k.corner) in Zeile 22: Teilbaum am Ziel voll, an der Ausgangsposition als Umriss; Koordinatenkreuz vorher gestrichelt, nachher durchgezogen, Pfeil vom alten zum neuen Ursprung; Rest des Modells gedimmt.",
  });
});

test("S04 Pixelprüfung: Dimmen verändert die Darstellung", async ({ page }) => {
  await openApp(page);
  const canvas = page.getByTestId("viewport-canvas");
  const before = await canvas.screenshot();
  await clickLine(page, 22, 20);
  await waitView(page, "(s) => s.view.plan.base === 'dim'");
  const after = await canvas.screenshot();
  expect(Buffer.compare(before, after)).not.toBe(0);
  // Mean brightness of the canvas rises when the big body is dimmed (transparent, light).
  const mean = async (png: Buffer) =>
    page.evaluate(async (b64) => {
      const img = new Image();
      img.src = `data:image/png;base64,${b64}`;
      await img.decode();
      const c = document.createElement("canvas");
      c.width = img.width;
      c.height = img.height;
      const g = c.getContext("2d")!;
      g.drawImage(img, 0, 0);
      const d = g.getImageData(0, 0, c.width, c.height).data;
      let sum = 0;
      for (let i = 0; i < d.length; i += 4) sum += d[i] + d[i + 1] + d[i + 2];
      return sum / (d.length / 4) / 3;
    }, png.toString("base64"));
  const mb = await mean(before);
  const ma = await mean(after);
  expect(ma).toBeGreaterThan(mb + 1);
  record({
    id: "S04",
    title: "Pixelprüfung Dimmen",
    caption: `Mittlere Helligkeit der Ansicht ohne Auswahl ${mb.toFixed(1)}, mit Cursor auf Zeile 22 ${ma.toFixed(1)} (gedimmter Rest ist heller).`,
  });
});

test("S05 Modulkontext, Differenz: abgezogener Körper rot", async ({ page }) => {
  await openApp(page);
  await clickLine(page, 10, 6);
  const s = await waitScene(page, { context: "housing" });
  expect(s.path).toEqual(["housing"]);
  const st = await waitView(page, "(s) => s.view.selection.line === 10 && s.view.plan.items.some(i => i.mode === 'red')");
  const red = st.view!.plan.items.filter((i) => i.mode === "red");
  expect(red.map((i) => i.line)).toEqual([12]);
  await expect(page.getByRole("navigation", { name: "Pfadleiste" })).toContainText("housing.mcad›housing");
  await screenshot(page, "s05-difference");
  record({
    id: "S05",
    title: "Modulkontext und difference",
    image: "s05-difference",
    caption: "Cursor in Zeile 10 (difference) innerhalb von module housing: Kontext wechselt auf das Modul (Pfadleiste „Datei › housing“, eigenes System, Raster g eingeblendet); Ergebnis voll, abgezogene Box aus Zeile 12 rot halbtransparent.",
  });
});

test("S06 Hover über Zeile: Knoten halbtransparent", async ({ page }) => {
  await openApp(page);
  await clickLine(page, 11, 4);
  await waitScene(page, { context: "housing" });
  await lineEl(page, 12).hover({ position: { x: 60, y: 8 } });
  const s = await waitView(page, "(s) => s.view.plan.items.some(i => i.mode === 'hover' && i.line === 12)");
  expect(s.view!.hoverNodes.length).toBeGreaterThan(0);
  await expect(lineEl(page, 12)).toHaveClass(/cm-l-hover/);
  await screenshot(page, "s06-hover-zeile");
  record({
    id: "S06",
    title: "Hover über Zeile",
    image: "s06-hover-zeile",
    caption: "Maus über Zeile 12 (innere Box, wird abgezogen): der Körper erscheint halbtransparent im Gesamtmodell, sichtbar durch das Ergebnis hindurch; Cursor steht auf Zeile 11 (äußere Box voll).",
  });
});

test("S07 Hover über Fläche markiert Zeile, Klick setzt Cursor, Doppelklick springt in Definition", async ({ page }) => {
  await openApp(page);
  const pt = await page.evaluate(() => (window as unknown as { __cad: { findPointForLine(l: number): { x: number; y: number } | null } }).__cad.findPointForLine(22));
  expect(pt).not.toBeNull();
  await page.mouse.move(pt!.x, pt!.y);
  await expect(lineEl(page, 22)).toHaveClass(/cm-l-viewhover/);
  const h = await waitView(page, "(s) => s.view.plan.items.some(i => i.mode === 'picked' && i.line === 22)");
  expect(h.viewHover!.lines).toEqual([22]);
  await screenshot(page, "s07a-hover-flaeche");
  await page.mouse.click(pt!.x, pt!.y);
  const s = await waitView(page, "(s) => s.view.selection.line === 22");
  expect(s.view!.selection.nodes.length).toBeGreaterThan(0);
  // Body of k (line 21): double click opens the definition of housing.
  const pk = await page.evaluate(() => (window as unknown as { __cad: { findPointForLine(l: number): { x: number; y: number } | null } }).__cad.findPointForLine(21));
  expect(pk).not.toBeNull();
  await page.mouse.dblclick(pk!.x, pk!.y);
  const d = await waitScene(page, { context: "housing" });
  expect(d.path).toEqual(["housing"]);
  const line = await page.evaluate(() => {
    const c = (window as unknown as { __cad: { bridge: { doc(): { lineAt(p: number): { number: number } } }; state(): { cursor: number } } }).__cad;
    return c.bridge.doc().lineAt(c.state().cursor).number;
  });
  expect(line).toBeGreaterThanOrEqual(3);
  expect(line).toBeLessThanOrEqual(17);
  await screenshot(page, "s07b-doppelklick");
  record({
    id: "S07",
    title: "Ansicht → Code",
    image: "s07a-hover-flaeche",
    caption: "Maus über der Fläche des Deckels an k.corner: Zeile 22 im Editor wird hervorgehoben (Fläche → tri_tags → tag_nodes → Knoten → Zeile). Klick setzt den Cursor in Zeile 22, Doppelklick auf das Gehäuse springt in module housing.",
  });
});

test("S08 Pfadleiste: Klick springt eine Ebene hoch", async ({ page }) => {
  await openApp(page);
  await clickLine(page, 11, 4);
  await waitScene(page, { context: "housing" });
  await screenshot(page, "s08a-modulkontext");
  await page.getByRole("navigation", { name: "Pfadleiste" }).getByRole("button", { name: "Zur Datei housing.mcad" }).click();
  const s = await waitScene(page, { context: null });
  expect(s.path).toEqual([]);
  expect(s.view!.selection.line).toBe(21);
  await screenshot(page, "s08b-datei");
  record({
    id: "S08",
    title: "Pfadleiste",
    image: "s08a-modulkontext",
    caption: "Modulkontext housing: nur das Modul in seinem eigenen System mit Raster g (Schnittpunkte, verwendete Ebenen der Zeile). Klick auf „Datei“ setzt den Cursor auf den Aufruf k: housing(w = 60) (Zeile 21) und zeigt wieder die ganze Datei.",
  });
});

test("S09 Modifier #, %, *, !", async ({ page }) => {
  await openApp(page);
  let s = await openExample(page, "openscad-compat/24_modifiers.scad");
  const modes = s.view!.plan.items.map((i) => `${i.mode}@${i.line}`);
  expect(modes).toContain("red@4");
  expect(modes).toContain("grey@6");
  await expect(lineEl(page, 7)).toHaveClass(/cm-l-disabled/);
  await expect(lineEl(page, 8)).not.toHaveClass(/cm-l-disabled/);
  await screenshot(page, "s09a-modifier");
  s = await openExample(page, "openscad-compat/25_root_modifier.scad");
  await expect(page.getByTestId("status-bang")).toContainText("Zeile 3");
  expect(s.view!.plan.status).toContain("!");
  await screenshot(page, "s09b-root-modifier");
  record({
    id: "S09",
    title: "Modifier",
    image: "s09a-modifier",
    caption: "24_modifiers.scad: # dauerhaft rot halbtransparent (auch als abgezogener Körper), % grau halbtransparent als Hilfsgeometrie, * unsichtbar und Zeile 7 ausgegraut. Bei 25_root_modifier.scad meldet die Statuszeile den aktiven !-Teilbaum.",
  });
});

test("S10 Syntaxfehler: letzter gültiger Stand bleibt sichtbar", async ({ page }) => {
  await openApp(page);
  const before = await cad(page);
  await lineEl(page, 22).click();
  await page.keyboard.press("End");
  await page.keyboard.insertText(" ]");
  await page.waitForFunction(() => {
    const s = (window as unknown as { __cad: { state(): { diagnostics: Array<{ source: string }>; diagnosticsVersion: number; sentVersion: number; pendingSend: boolean } } }).__cad.state();
    return !s.pendingSend && s.diagnosticsVersion === s.sentVersion && s.diagnostics.some((d) => d.source === "syntax");
  });
  let s = await cad(page);
  expect(s.shownVersion).toBe(before.shownVersion);
  expect(s.view!.objects).toBeGreaterThan(0);
  await expect(page.locator(".cm-d-error").first()).toBeVisible();
  await expect(page.getByTestId("status-errors")).toContainText("1 Fehler");
  await screenshot(page, "s10-syntaxfehler");
  for (let i = 0; i < 2; i++) await page.keyboard.press("Backspace");
  s = await waitScene(page, { minVersion: (before.shownVersion ?? 0) + 1 });
  expect(s.diagnostics.filter((d) => d.severity === "error")).toEqual([]);
  record({
    id: "S10",
    title: "Syntaxfehler",
    image: "s10-syntaxfehler",
    caption: "Eingabe „ ]“ am Ende von Zeile 22: Stelle rot unterstrichen, Meldung am Zeilenende, Statuszeile „1 Fehler“; die Ansicht zeigt weiter den letzten gültigen Stand. Nach dem Löschen wird neu ausgewertet.",
  });
});

test("S11 Gescheiterte Operation: Zeile rot, Meldung am Zeilenende", async ({ page }) => {
  await openApp(page);
  await page.evaluate(() =>
    (window as unknown as { __cad: { setText(t: string): void } }).__cad.setText(
      "difference(fillet = 20) {\n    cube(10);\n    translate([5, 5, -1]) cylinder(h = 12, r = 2);\n}\n",
    ),
  );
  await waitScene(page);
  await expect(lineEl(page, 1)).toHaveClass(/cm-l-error/);
  await expect(lineEl(page, 1).locator(".cm-lineend-error")).toContainText("Verrundung gescheitert");
  await page.getByRole("button", { name: "Einpassen", exact: true }).click();
  await waitView(page, "(s) => s.frames > 0");
  await screenshot(page, "s11-fehler");
  record({
    id: "S11",
    title: "Gescheiterte Operation",
    image: "s11-fehler",
    caption: "difference(fillet = 20) an einem 10-mm-Würfel: Verrundung gescheitert, Zeile 1 rot mit Meldung am Zeilenende; die Ansicht zeigt den Körper ohne Verrundung.",
  });
});

test("S12 Export STL und 3MF", async ({ page }) => {
  await openApp(page);
  const [dl] = await Promise.all([page.waitForEvent("download"), exportAs(page, "STL")]);
  expect(dl.suggestedFilename()).toBe("housing.stl");
  const stl = readFileSync((await dl.path())!);
  const tris = stl.readUInt32LE(80);
  expect(tris).toBeGreaterThan(0);
  expect(stl.length).toBe(84 + 50 * tris);
  const [dl2] = await Promise.all([page.waitForEvent("download"), exportAs(page, "3MF mehrfarbig")]);
  expect(dl2.suggestedFilename()).toBe("housing.3mf");
  const mf = readFileSync((await dl2.path())!);
  expect(mf.subarray(0, 2).toString()).toBe("PK");
  await expect(page.getByTestId("status-notice")).toContainText("Exportiert");
  await screenshot(page, "s12-export");
  record({
    id: "S12",
    title: "Export",
    image: "s12-export",
    caption: `Export STL: housing.stl, ${tris} Dreiecke, ${stl.length} Byte (binär, 84 + 50·n). Export 3MF: housing.3mf (ZIP, ${mf.length} Byte). Die Statuszeile meldet den Export.`,
  });
});

test("S13 Raster: verwendete Ebenen, alle Raster, Hover über Rasterausdruck", async ({ page }) => {
  await openApp(page);
  await openExample(page, "spec/hole_plate.mcad");
  await clickLine(page, 12, 10);
  let s = await waitScene(page, { context: "hole_plate" });
  s = await waitView(page, "(s) => s.view.selection.line === 12 && s.view.grid.lines > 0");
  const used = s.view!.grid.lines;
  expect(used).toBe(6);
  expect(s.view!.grid.points).toBeGreaterThan(30);
  await screenshot(page, "s13a-raster-zeile");
  await page.getByRole("button", { name: "Alle Raster", exact: true }).click();
  await expect(page.getByRole("button", { name: "Alle Raster", exact: true })).toHaveAttribute("aria-pressed", "true");
  s = await waitView(page, "(s, n) => s.view.grid.lines > n", used);
  expect(s.view!.grid.dashed).toBeGreaterThan(0);
  await screenshot(page, "s13b-alle-raster");
  await page.getByRole("button", { name: "Alle Raster", exact: true }).click();
  // Hover over `g[hole[i], mid, floor]` in line 14.
  // Column 24 is inside `g[hole[i], mid, floor]`.
  await hoverText(page, 14, 26);
  s = await waitView(page, "(s) => s.view.grid.highlight.includes('point')");
  await expect(page.locator(".cm-ref-hover").first()).toBeVisible();
  await screenshot(page, "s13c-raster-hover");
  record({
    id: "S13",
    title: "Raster",
    image: "s13b-alle-raster",
    caption: "Modul hole_plate, Schalter „alle Raster“: Ebenen als dünne Linien am Rand der Bauteilhülle (Literal durchgezogen, Ausdruck gestrichelt), Schnittpunkte als Punkte. Ohne Schalter nur die 6 Ebenen der Zeile 12; Hover über g[hole[i], mid, floor] hebt den Punkt hervor.",
  });
});

test("S14 Anker benannter Instanzen", async ({ page }) => {
  await openApp(page);
  const s = await openExample(page, "spec/angle_anchor.mcad");
  // a: bracket(), b: bracket(l = 40): 3 anchors each.
  expect(s.view!.plan.anchors).toBe(6);
  // Hover over the reference `a.base_hole` (line 26) highlights that anchor.
  await clickLine(page, 31, 0);
  await waitView(page, "(s) => s.view.selection.line === 31");
  await hoverText(page, 26, 14);
  await waitView(page, "(s) => s.view.plan.activeAnchors === 1");
  await expect(page.locator(".cm-ref-hover").first()).toHaveText("a.base_hole");
  // Cursor on the named instance a: its anchors are highlighted.
  await page.mouse.move(5, 5);
  await clickLine(page, 23, 4);
  await waitView(page, "(s) => s.view.selection.line === 23 && s.view.plan.activeAnchors === 3");
  await screenshot(page, "s14-anker");
  record({
    id: "S14",
    title: "Anker",
    image: "s14-anker",
    caption: "angle_anchor.mcad: Anker der benannten Instanzen a und b als Rauten, mit Pfeil bei dir. Hover über a.base_hole hebt diesen Anker hervor; Cursor auf a: bracket() hebt die drei Anker der Instanz hervor.",
  });
});

test("S15 Fillet-Zonen blau, ausgewählte Kanten farbig, Inline-Hinweis", async ({ page }) => {
  await openApp(page);
  await openExample(page, "spec/fillet_zones.mcad");
  await clickLine(page, 12, 10);
  await waitScene(page, { context: "block" });
  const s = await waitView(page, "(s) => s.view.selection.line === 12 && s.view.plan.items.some(i => i.mode === 'blue')");
  expect(s.view!.plan.items.filter((i) => i.mode === "blue").length).toBe(2);
  expect(s.view!.plan.edges).toBeGreaterThan(0);
  await expect(lineEl(page, 12).locator(".cm-lineend-hint")).toContainText("2 Kanten");
  await screenshot(page, "s15-fillet");
  record({
    id: "S15",
    title: "Verrundung mit Zonen",
    image: "s15-fillet",
    caption: "fillet_zonen.mcad, Cursor auf fillet(2) in Zeile 12: Zonen (Rasterlinie und Kugel) blau halbtransparent, ausgewählte Kanten orange; Inline-Hinweis „2 Zonen · 2 Kanten“ am Zeilenende.",
  });
});

test("S16 Positionen alter Szenen werden beim Tippen weitergeführt", async ({ page }) => {
  await openApp(page);
  // Insert two lines at the top while the engine is blocked: the cursor still selects nodes
  // of the old scene at their new lines (ChangeSet.mapPos) before the new scene arrives.
  await page.evaluate(() => (window as unknown as { __cad: { debugHang(ms: number): void } }).__cad.debugHang(1500));
  await clickLine(page, 1, 0);
  await page.keyboard.type("// a\n// b\n");
  // The edit is sent after a short debounce; the engine is still blocked.
  await page.waitForFunction(() => !(window as unknown as { __cad: { state(): { pendingSend: boolean } } }).__cad.state().pendingSend);
  const s = await cad(page);
  expect(s.shownVersion).toBeLessThan(s.sentVersion);
  // The old scene's node of `translate(k.corner)` is now on line 24.
  await clickLine(page, 24, 2);
  const v = await waitView(page, "(s) => s.view.selection.line === 24 && s.view.selection.nodes.length > 0", null, { meshes: false });
  expect(v.shownVersion).toBeLessThan(v.sentVersion);
  expect(v.view!.plan.items.some((i) => i.label === "translate" && i.line === 24)).toBe(true);
  const after = await waitScene(page);
  expect(after.view!.selection.line).toBe(24);
  record({
    id: "S16",
    title: "Positionen alter Szenen",
    caption: "Zwei Zeilen oben eingefügt, während die Engine 1,5 s blockiert ist: Der Cursor in Zeile 24 wählt noch in der alten Szene den Knoten translate(k.corner) (früher Zeile 22) – Quellpositionen werden über ChangeSet.mapPos weitergeführt, bis die neue Szene da ist.",
  });
});

test("S17 Watchdog meldet hängende Engine", async ({ page }) => {
  await page.goto("/?watchdog=800");
  await waitScene(page);
  await page.evaluate(() => (window as unknown as { __cad: { debugHang(ms: number): void } }).__cad.debugHang(5000));
  await expect(page.getByTestId("status-engine-error")).toContainText("neu gestartet", { timeout: 10_000 });
  const s = await cad(page);
  expect(s.restarts).toBe(1);
  expect(s.view!.objects).toBeGreaterThan(0);
  await screenshot(page, "s17-watchdog");
  // The next edit evaluates normally.
  await lineEl(page, 28).click();
  await page.keyboard.type("cube(1);");
  await waitScene(page);
  record({
    id: "S17",
    title: "Watchdog",
    image: "s17-watchdog",
    caption: "Engine-Worker 5 s blockiert (Testhilfe, Watchdog 0,8 s): Der Worker wird beendet und neu gestartet, die Statuszeile meldet den Fehler rot; die letzte Szene bleibt sichtbar, die nächste Änderung wird normal ausgewertet.",
  });
});

test("S18 Autospeichern in IndexedDB", async ({ page }) => {
  await openApp(page);
  await lineEl(page, 28).click();
  await page.keyboard.type("cube(3); // autosave-test");
  await waitScene(page);
  // (waitForFunction does not await promises: poll the stored text instead.)
  await expect
    .poll(() => page.evaluate(async () => (await (window as unknown as { __cad: { autosave(): Promise<{ text: string } | null> } }).__cad.autosave())?.text ?? ""), { timeout: 10_000 })
    .toContain("autosave-test");
  await page.reload();
  await waitScene(page);
  await expect(page.locator(".cm-content")).toContainText("autosave-test");
  record({ id: "S18", title: "Autospeichern", caption: "Änderung wird nach 0,4 s in IndexedDB gespeichert; nach dem Neuladen der Seite steht der geänderte Text wieder im Editor." });
});

test("S19 Dunkles Thema und Standardansichten", async ({ page }) => {
  await openApp(page);
  await setTheme(page, "dark");
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  await standardView(page, "Oben");
  await standardView(page, "Iso");
  await clickLine(page, 21, 4);
  await waitView(page, "(s) => s.view.selection.line === 21");
  await screenshot(page, "s19-dunkel");
  record({ id: "S19", title: "Dunkles Thema", image: "s19-dunkel", caption: "Dunkles Thema, Iso-Ansicht, Cursor auf k: housing(w = 60): Instanz voll, Anker der Instanz hervorgehoben, Rest gedimmt." });
});

test("S20 intersection und 2D-Formen", async ({ page }) => {
  await openApp(page);
  await openExample(page, "openscad-compat/05_nested_booleans.scad");
  await clickLine(page, 3, 8);
  let s = await waitView(page, "(s) => s.view.selection.line === 3");
  expect(s.view!.plan.items.map((i) => `${i.mode}@${i.line}`)).toEqual(["full@3", "ghost@4", "ghost@5"]);
  await screenshot(page, "s20a-intersection");
  await openExample(page, "openscad-compat/27_spacers_2d.scad");
  await clickLine(page, 13, 8);
  s = await waitView(page, "(s) => s.view.selection.line === 13 && s.view.plan.items.length > 0");
  expect(s.view!.plan.items.filter((i) => i.mode === "full").every((i) => i.flat)).toBe(true);
  await standardView(page, "Oben");
  await waitView(page, "(s) => s.frames > 0");
  await screenshot(page, "s20b-profil");
  record({
    id: "S20",
    title: "intersection und 2D",
    image: "s20a-intersection",
    caption: "05_nested_booleans.scad, Cursor auf intersection(): Würfel und Kugel halbtransparent, Ergebnis voll. 27_spacers_2d.scad, Cursor auf square([30, 10]): 2D-Form als Umriss mit leichter Füllung in ihrer Ebene (Ansicht von oben).",
  });
});

test("S21 Öffnen und Speichern ohne File System Access API (Hoch- und Herunterladen)", async ({ page }) => {
  await page.addInitScript(() => {
    const w = window as unknown as Record<string, unknown>;
    delete w.showOpenFilePicker;
    delete w.showSaveFilePicker;
  });
  await openApp(page);
  const [chooser] = await Promise.all([page.waitForEvent("filechooser"), fileMenu(page, "Öffnen …")]);
  await chooser.setFiles({ name: "teil.scad", mimeType: "text/plain", buffer: Buffer.from("// hochgeladen\ncube([12, 8, 4]);\n") });
  const s = await waitScene(page);
  expect(s.fileName).toBe("teil.scad");
  await expect(lineEl(page, 2)).toHaveText("cube([12, 8, 4]);");
  const [dl] = await Promise.all([page.waitForEvent("download"), page.getByRole("button", { name: "Speichern", exact: true }).click()]);
  expect(dl.suggestedFilename()).toBe("teil.scad");
  expect(readFileSync((await dl.path())!, "utf8")).toBe("// hochgeladen\ncube([12, 8, 4]);\n");
  await expect(page.getByTestId("status-notice")).toContainText("Gespeichert: teil.scad");
  record({
    id: "S21",
    title: "Öffnen und Speichern",
    caption: "Ohne File System Access API: „Datei › Öffnen …“ lädt eine Datei über den Dateidialog hoch (teil.scad wird ausgewertet), „Speichern“ lädt den Quelltext unverändert herunter. Mit der API (Chromium) werden dieselben Funktionen über showOpenFilePicker/showSaveFilePicker ausgeführt.",
  });
});

test("S22 Messwerte: Tastendruck bis neue Szene, fps beim Drehen", async ({ page }) => {
  await openApp(page);
  await lineEl(page, 21).click();
  // Edit the literal 60 → 61 … 65 and wait for each scene.
  const lat: number[] = [];
  for (let i = 1; i <= 5; i++) {
    await page.keyboard.press("End");
    const before = (await cad(page)).sentVersion;
    const t0 = await page.evaluate(() => performance.now());
    // Replace the digit before ");"
    for (let k = 0; k < 3; k++) await page.keyboard.press("ArrowLeft");
    await page.keyboard.press("Backspace");
    await page.keyboard.type(String(i % 10));
    await waitScene(page, { minVersion: before + 1 });
    lat.push((await page.evaluate(() => performance.now())) - t0);
  }
  const s = await cad(page);
  const fps = await page.evaluate(() => (window as unknown as { __cad: { orbitBenchmark(n: number): Promise<{ fps: number }> } }).__cad.orbitBenchmark(120));
  const median = (a: number[]) => [...a].sort((x, y) => x - y)[Math.floor(a.length / 2)];
  writeMetrics({
    editToSceneMs: s.metrics.editToScene,
    editToSceneMedianMs: median(s.metrics.editToScene),
    keyToSceneE2eMs: lat,
    keyToSceneE2eMedianMs: median(lat),
    evalMsMedian: median(s.metrics.evalMs),
    orbitFps: fps.fps,
  });
  // Guards against a stalled render loop; the value itself goes into the reports. CI runners
  // render with SwiftShader on shared CPUs at about 4–6 fps (locally about 12).
  expect(fps.fps).toBeGreaterThan(process.env.CI ? 2 : 5);
  record({
    id: "S22",
    title: "Messwerte",
    caption: `Fünf Änderungen des Literals in Zeile 21: Tastendruck → neue Szene gezeichnet, Median ${median(s.metrics.editToScene).toFixed(0)} ms (App-Messung). Drehen über 120 Bilder: ${fps.fps.toFixed(0)} fps (SwiftShader, ohne GPU).`,
  });
});
