// End-to-end tests of the browser UI (Phase A) in Chromium against the production build.
// Every scenario records a screenshot and a caption for test report 2. Waiting is done on
// scene versions and view state (window.__cad), never with fixed sleeps.

import { readFileSync } from "node:fs";
import { expect, test } from "@playwright/test";
import { cad, clickLine, exportAs, fileMenu, hoverText, lineEl, openApp, openExample, record, screenshot, setTheme, standardView, waitScene, waitView, writeMetrics } from "./helpers";

test.beforeEach(async ({ page }) => {
  page.on("pageerror", (e) => console.log("pageerror:", e.message));
});

test("S01 loading: editor, view and first scene", async ({ page }) => {
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
    title: "Loading",
    image: "s01-laden",
    caption: "Initial state: spec example housing.mcad on the left in the editor, on the right the scene with the floor, the axis cross and the anchors of the named instance k (diamonds, with an arrow for dir).",
  });
});

test("S02 open an example", async ({ page }) => {
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
    title: "Open an example",
    image: "s02-beispiel",
    caption: "Example menu → spec/hole_plate.mcad: the editor loads the file, after the evaluation the view shows the plate with bores, the tab and the anchor p.first_hole.",
  });
});

test("S03 cursor in a line: node full, the rest dimmed, transform with coordinate crosses", async ({ page }) => {
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
    title: "Cursor in a line",
    image: "s03b-cursor-translate",
    caption: "Cursor on translate(k.corner) in line 22: subtree full at the target, as an outline at the start position; coordinate cross before dashed, after solid, an arrow from the old to the new origin; the rest of the model dimmed.",
  });
});

test("S04 pixel check: dimming changes the display", async ({ page }) => {
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
    title: "Pixel check dimming",
    caption: `Mean brightness of the view without a selection ${mb.toFixed(1)}, with the cursor on line 22 ${ma.toFixed(1)} (the dimmed rest is lighter).`,
  });
});

test("S05 module context, difference: subtracted body red", async ({ page }) => {
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
    title: "Module context and difference",
    image: "s05-difference",
    caption: "Cursor in line 10 (difference) inside module housing: the context switches to the module (path bar “Datei › housing”, its own system, grid g shown); result full, the subtracted box of line 12 red, semi-transparent.",
  });
});

test("S06 hover over a line: node semi-transparent", async ({ page }) => {
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
    title: "Hover over a line",
    image: "s06-hover-zeile",
    caption: "Mouse over line 12 (inner box, subtracted): the body appears semi-transparent in the whole model, visible through the result; the cursor is on line 11 (outer box full).",
  });
});

test("S07 hover over a face marks the line, a click sets the cursor, a double click jumps into the definition", async ({ page }) => {
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
    title: "View → code",
    image: "s07a-hover-flaeche",
    caption: "Mouse over the face of the lid at k.corner: line 22 is highlighted in the editor (face → tri_tags → tag_nodes → node → line). A click sets the cursor into line 22, a double click on the housing jumps into module housing.",
  });
});

test("S08 path bar: a click jumps one level up", async ({ page }) => {
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
    title: "Path bar",
    image: "s08a-modulkontext",
    caption: "Module context housing: only the module in its own system with grid g (intersections, planes used by the line). A click on “Datei” (file) sets the cursor on the call k: housing(w = 60) (line 21) and shows the whole file again.",
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
    title: "Modifiers",
    image: "s09a-modifier",
    caption: "24_modifiers.scad: # permanently red semi-transparent (also as a subtracted body), % grey semi-transparent as helper geometry, * invisible and line 7 greyed out. With 25_root_modifier.scad the status line reports the active !-subtree.",
  });
});

test("S10 syntax error: the last valid state stays visible", async ({ page }) => {
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
    title: "Syntax error",
    image: "s10-syntaxfehler",
    caption: "Input “ ]” at the end of line 22: place underlined red, message at the line end, status line “1 Fehler” (1 error); the view keeps showing the last valid state. After deleting it the model is evaluated again.",
  });
});

test("S11 failed operation: line red, message at the line end", async ({ page }) => {
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
    title: "Failed operation",
    image: "s11-fehler",
    caption: "difference(fillet = 20) on a 10 mm cube: the fillet fails, line 1 red with the message at the line end; the view shows the body without the fillet.",
  });
});

test("S12 export STL and 3MF", async ({ page }) => {
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
    caption: `Export STL: housing.stl, ${tris} triangles, ${stl.length} bytes (binary, 84 + 50·n). Export 3MF: housing.3mf (ZIP, ${mf.length} bytes). The status line reports the export.`,
  });
});

test("S13 grids: used planes, all grids, hover over a grid expression", async ({ page }) => {
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
    title: "Grids",
    image: "s13b-alle-raster",
    caption: "Module hole_plate, switch “alle Raster” (all grids): planes as thin lines at the edge of the part's hull (literal solid, expression dashed), intersections as points. Without the switch only the 6 planes of line 12; hover over g[hole[i], mid, floor] highlights the point.",
  });
});

test("S14 anchors of named instances", async ({ page }) => {
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
    title: "Anchors",
    image: "s14-anker",
    caption: "angle_anchor.mcad: anchors of the named instances a and b as diamonds, with an arrow for dir. Hover over a.base_hole highlights this anchor; the cursor on a: bracket() highlights the three anchors of the instance.",
  });
});

test("S15 fillet zones blue, selected edges coloured, inline hint", async ({ page }) => {
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
    title: "Fillet with zones",
    image: "s15-fillet",
    caption: "fillet_zones.mcad, cursor on fillet(2) in line 12: zones (grid line and sphere) blue semi-transparent, selected edges orange; inline hint “2 Zonen · 2 Kanten” (2 zones · 2 edges) at the line end.",
  });
});

test("S16 positions of old scenes are carried along while typing", async ({ page }) => {
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
    title: "Positions of old scenes",
    caption: "Two lines inserted at the top while the engine is blocked for 1.5 s: the cursor in line 24 still selects the node translate(k.corner) (formerly line 22) in the old scene – source positions are carried along with ChangeSet.mapPos until the new scene arrives.",
  });
});

test("S17 the watchdog reports a hanging engine", async ({ page }) => {
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
    caption: "Engine worker blocked for 5 s (test helper, watchdog 0.8 s): the worker is terminated and started again, the status line reports the error in red; the last scene stays visible, the next change is evaluated normally.",
  });
});

test("S18 autosave in IndexedDB", async ({ page }) => {
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
  record({ id: "S18", title: "Autosave", caption: "A change is saved in IndexedDB after 0.4 s; after reloading the page the changed text is in the editor again." });
});

test("S19 dark theme and standard views", async ({ page }) => {
  await openApp(page);
  await setTheme(page, "dark");
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  await standardView(page, "Oben");
  await standardView(page, "Iso");
  await clickLine(page, 21, 4);
  await waitView(page, "(s) => s.view.selection.line === 21");
  await screenshot(page, "s19-dunkel");
  record({ id: "S19", title: "Dark theme", image: "s19-dunkel", caption: "Dark theme, iso view, cursor on k: housing(w = 60): instance full, the anchors of the instance highlighted, the rest dimmed." });
});

test("S20 intersection and 2D shapes", async ({ page }) => {
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
    title: "intersection and 2D",
    image: "s20a-intersection",
    caption: "05_nested_booleans.scad, cursor on intersection(): cube and sphere semi-transparent, result full. 27_spacers_2d.scad, cursor on square([30, 10]): 2D shape as an outline with a light fill in its plane (view from the top).",
  });
});

test("S21 open and save without the File System Access API (upload and download)", async ({ page }) => {
  await page.addInitScript(() => {
    const w = window as unknown as Record<string, unknown>;
    delete w.showOpenFilePicker;
    delete w.showSaveFilePicker;
  });
  await openApp(page);
  const [chooser] = await Promise.all([page.waitForEvent("filechooser"), fileMenu(page, "Öffnen …")]);
  await chooser.setFiles({ name: "part.scad", mimeType: "text/plain", buffer: Buffer.from("// uploaded\ncube([12, 8, 4]);\n") });
  const s = await waitScene(page);
  expect(s.fileName).toBe("part.scad");
  await expect(lineEl(page, 2)).toHaveText("cube([12, 8, 4]);");
  const [dl] = await Promise.all([page.waitForEvent("download"), page.getByRole("button", { name: "Speichern", exact: true }).click()]);
  expect(dl.suggestedFilename()).toBe("part.scad");
  expect(readFileSync((await dl.path())!, "utf8")).toBe("// uploaded\ncube([12, 8, 4]);\n");
  await expect(page.getByTestId("status-notice")).toContainText("Gespeichert: part.scad");
  record({
    id: "S21",
    title: "Open and save",
    caption: "Without the File System Access API: “Datei › Öffnen …” (file › open) uploads a file through the file dialog (part.scad is evaluated), “Speichern” (save) downloads the source unchanged. With the API (Chromium) the same functions run through showOpenFilePicker/showSaveFilePicker.",
  });
});

test("S22 measurements: keystroke to new scene, fps while orbiting", async ({ page }) => {
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
    title: "Measurements",
    caption: `Five changes of the literal in line 21: keystroke → new scene drawn, median ${median(s.metrics.editToScene).toFixed(0)} ms (app measurement). Orbiting for 120 frames: ${fps.fps.toFixed(0)} fps (SwiftShader, no GPU).`,
  });
});
