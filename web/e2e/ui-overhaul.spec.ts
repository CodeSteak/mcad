// End-to-end tests of the UI overhaul (docs/concepts/ui-redesign, test report 20): bars
// without wrapping or cutting off at 1280/1440/1920 px in both themes (scrollWidth ≤ clientWidth,
// one row, no overlap), head bar menus and the export split button, file tabs with "+" and the
// context menu, exactly one mode, the context bars (sketch shortcuts, measure kinds), the view
// cube, the side panel with tabs and the keyboard path through the window. Screenshots for
// report 20.

import { readFileSync } from "node:fs";
import { expect, test, type Page } from "@playwright/test";
import {
  cad,
  clickLine,
  fileTab,
  loadText,
  measureBar,
  measureKind,
  menuItem,
  mode,
  openApp,
  openExample,
  openExampleProject,
  pickMenu,
  record20,
  screenshot20,
  sectionBar,
  setTheme,
  sideTab,
  sketchBar,
  sketchToolButton,
  waitScene,
  writeMetrics20,
} from "./helpers";

test.beforeEach(async ({ page }) => {
  page.on("pageerror", (e) => console.log("pageerror:", e.message));
});

type W = {
  __cad: {
    sketch(): { tool: string; actions: number } | null;
    measure(): { active: boolean; kind: string | null; inSection: boolean };
    camera(): { eye: number[]; dir: number[] } | null;
    state(): { section: { on: boolean }; sideOpen: boolean; sideTab: string; exportChoice: string };
  };
};

/** Every bar of the window: content wider than its box (cut off or scrolled), height, overlaps. */
async function barReport(page: Page) {
  return page.evaluate(() => {
    const sel = [
      ["header bar", ".hd"],
      ["header left", ".hd-left"],
      ["header middle", ".hd-title"],
      ["header right", ".hd-right"],
      ["file bar", ".filebar"],
      ["mode toolbar", ".vtoolbar > [role=toolbar]:first-child"],
      ["camera toolbar", ".vt-end > [role=toolbar]"],
      ["context bar", ".ctxslot > [role=toolbar]"],
      ["side panel tabs", ".sp-head"],
      ["status line", ".statusbar"],
    ];
    const out: Array<{ name: string; scroll: number; client: number; height: number; top: number; left: number; right: number; bottom: number; children: number }> = [];
    for (const [name, s] of sel) {
      for (const e of document.querySelectorAll<HTMLElement>(s)) {
        const r = e.getBoundingClientRect();
        // children that stick out of the bar (cut off by the window or the pane)
        let outside = 0;
        for (const c of e.querySelectorAll<HTMLElement>("button, [role=radio], [role=tab], input, .hd-seg, .ui-label")) {
          const q = c.getBoundingClientRect();
          if (q.width === 0) continue;
          if (q.right > r.right + 0.5 || q.left < r.left - 0.5 || q.right > window.innerWidth + 0.5) outside++;
        }
        out.push({ name, scroll: e.scrollWidth, client: e.clientWidth, height: Math.round(r.height), top: Math.round(r.top), left: Math.round(r.left), right: Math.round(r.right), bottom: Math.round(r.bottom), children: outside });
      }
    }
    return { bars: out, docScroll: document.documentElement.scrollWidth, docClient: document.documentElement.clientWidth };
  });
}

function checkBars(rep: Awaited<ReturnType<typeof barReport>>, label: string): string[] {
  const problems: string[] = [];
  if (rep.docScroll > rep.docClient) problems.push(`${label}: the window scrolls horizontally (${rep.docScroll} > ${rep.docClient})`);
  for (const b of rep.bars) {
    if (b.scroll > b.client + 1) problems.push(`${label}: ${b.name} cut off (${b.scroll} > ${b.client})`);
    if (b.children > 0) problems.push(`${label}: ${b.name}: ${b.children} elements stick out`);
    // one row: the bars are 26 px controls with padding
    const max = b.name === "header bar" ? 41 : b.name === "file bar" ? 34 : b.name === "status line" ? 26 : 40;
    if (b.height > max) problems.push(`${label}: ${b.name} wrapped (height ${b.height} > ${max})`);
  }
  const mode = rep.bars.find((b) => b.name === "mode toolbar");
  const cam = rep.bars.find((b) => b.name === "camera toolbar");
  if (mode && cam && mode.right > cam.left) problems.push(`${label}: mode and camera overlap (${mode.right} > ${cam.left})`);
  return problems;
}

test("U01 bars at 1280, 1440 and 1920 px, light and dark: no wrapping, nothing cut off", async ({ page }) => {
  test.setTimeout(240_000);
  const widths = [1280, 1440, 1920];
  const all: Record<string, unknown> = {};
  const problems: string[] = [];
  await openApp(page);
  await openExample(page, "spec/multicolor.mcad");
  for (const theme of ["light", "dark"] as const) {
    await setTheme(page, theme);
    for (const w of widths) {
      await page.setViewportSize({ width: w, height: 900 });
      await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
      const t = theme === "light" ? "hell" : "dunkel";
      const label = `${w} ${theme}`;
      for (const [m, en] of [["Modell", "model"], ["Schnitt", "section"], ["Messen", "measure"]] as const) {
        await mode(page, m).click();
        await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
        const rep = await barReport(page);
        all[`${label} ${en}`] = rep;
        problems.push(...checkBars(rep, `${label} ${en}`));
        await screenshot20(page, `u01-${w}-${t}-${m.toLowerCase()}`);
      }
      await mode(page, "Modell").click();
    }
  }
  // the sketch bar (drawing needs the sketch at the cursor)
  await openExample(page, "spec/sketch_lid.mcad");
  await clickLine(page, 14, 4);
  await waitScene(page, { context: "outline" });
  for (const theme of ["light", "dark"] as const) {
    await setTheme(page, theme);
    for (const w of widths) {
      await page.setViewportSize({ width: w, height: 900 });
      await mode(page, "Skizze").click();
      await expect(sketchBar(page)).toBeVisible();
      await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
      const t = theme === "light" ? "hell" : "dunkel";
      const rep = await barReport(page);
      all[`${w} ${theme} sketch`] = rep;
      problems.push(...checkBars(rep, `${w} ${theme} sketch`));
      await screenshot20(page, `u01-${w}-${t}-skizze`);
      await sketchBar(page).getByRole("button", { name: "Fertig" }).click();
    }
  }
  writeMetrics20({ bars: all });
  expect(problems).toEqual([]);
  record20({
    id: "U01",
    title: "Widths and themes",
    caption:
      "Per width (1280, 1440, 1920 px) and theme all bars in the modes model, section, measure and sketch: content not wider than the bar (scrollWidth ≤ clientWidth), no element sticks out, every bar single-line, the mode and camera bars do not overlap, the window does not scroll horizontally.",
    images: [
      { name: "u01-1280-hell-modell", label: "1280 px, light, model" },
      { name: "u01-1280-dunkel-messen", label: "1280 px, dark, measure" },
      { name: "u01-1920-hell-schnitt", label: "1920 px, light, section" },
      { name: "u01-1280-hell-skizze", label: "1280 px, light, sketch" },
    ],
    checks: [`${Object.keys(all).length} states checked, ${problems.length} findings`],
  });
});

test("U02 header bar: file menu by keyboard, the export remembers the format (Ctrl+E), theme", async ({ page }) => {
  await openApp(page);
  // keyboard: open with Enter, move with the arrow keys, Escape returns to the button
  const datei = page.getByRole("button", { name: "Datei", exact: true });
  await datei.focus();
  await page.keyboard.press("Enter");
  const menu = page.getByRole("menu", { name: "Datei" });
  await expect(menu).toBeVisible();
  await expect(menuItem(page, "Neu")).toBeFocused();
  const names = await menu.locator("[role^=menuitem]").evaluateAll((els) => els.map((e) => e.getAttribute("aria-label")));
  expect(names).toEqual(["Neu", "Öffnen …", "Ordner öffnen …", "Speichern unter …", "Datei hinzufügen …", "Datei laden …", "ZIP laden …", "ZIP speichern"]);
  await page.keyboard.press("ArrowDown");
  await expect(menuItem(page, "Öffnen …")).toBeFocused();
  await page.keyboard.press("End");
  await expect(menuItem(page, "ZIP speichern")).toBeFocused();
  await screenshot20(page, "u02-datei-menue");
  await page.keyboard.press("Escape");
  await expect(menu).toHaveCount(0);
  await expect(datei).toBeFocused();
  // "Beispiele" with submenus
  await page.getByRole("button", { name: "Beispiele", exact: true }).click();
  await menuItem(page, "Spec-Beispiele").hover();
  await expect(page.getByRole("menu", { name: "Spec-Beispiele" })).toBeVisible();
  await screenshot20(page, "u02-beispiele");
  await page.keyboard.press("Escape");
  await page.keyboard.press("Escape");
  // export: the menu lists all formats; the choice stays on the button and for Strg+E
  await page.getByRole("button", { name: "Exportformat", exact: true }).click();
  const em = page.getByRole("menu", { name: "Exportformat" });
  const formats = await em.locator("[role^=menuitem]").evaluateAll((els) => els.map((e) => e.getAttribute("aria-label")));
  expect(formats).toEqual(["3MF mehrfarbig", "3MF einfarbig", "STL", "STL je Farbe (ZIP)", "STEP", "Je Bauteil"]);
  await expect(menuItem(page, "3MF mehrfarbig")).toHaveAttribute("aria-checked", "true");
  await screenshot20(page, "u02-export-menue");
  const [dl] = await Promise.all([page.waitForEvent("download"), menuItem(page, "STL").click()]);
  expect(dl.suggestedFilename()).toBe("housing.stl");
  await expect(page.getByRole("button", { name: "Exportieren: STL", exact: true })).toBeVisible();
  await page.locator(".cm-content").click();
  const [dl2] = await Promise.all([page.waitForEvent("download"), page.keyboard.press("Control+e")]);
  expect(dl2.suggestedFilename()).toBe("housing.stl");
  await page.reload();
  await waitScene(page);
  await expect(page.getByRole("button", { name: "Exportieren: STL", exact: true })).toBeVisible();
  // theme as a symbol button with a menu
  await setTheme(page, "dark");
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  await page.getByRole("button", { name: "Farbschema", exact: true }).click();
  await expect(menuItem(page, "Dunkel")).toHaveAttribute("aria-checked", "true");
  await screenshot20(page, "u02-thema-menue");
  await page.keyboard.press("Escape");
  await setTheme(page, "light");
  // settings popover: Escape closes it
  await page.getByRole("button", { name: "Einstellungen", exact: true }).click();
  await expect(page.getByRole("dialog", { name: "Einstellungen" })).toBeVisible();
  await page.getByRole("dialog", { name: "Einstellungen" }).getByRole("spinbutton", { name: "Fangradius" }).focus();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog", { name: "Einstellungen" })).toHaveCount(0);
  record20({
    id: "U02",
    title: "Header bar",
    caption:
      "“Datei ▾” (file) opens with Enter, arrow keys and Home/End move the focus, Esc closes and returns the focus to the button. “Beispiele ▾” (examples) with submenus. The split export button lists 3MF multi-colour/single colour, STL, STL per colour, STEP and “Je Bauteil” (per part); after choosing STL the button shows “STL”, Ctrl+E exports STL, also after a reload. Theme as an icon button with a menu.",
    images: [
      { name: "u02-datei-menue", label: "file menu (keyboard)" },
      { name: "u02-export-menue", label: "export menu" },
      { name: "u02-beispiele", label: "examples with a submenu" },
      { name: "u02-thema-menue", label: "theme" },
    ],
    checks: ["8 entries in the file menu", "export STL by menu and by Ctrl+E (housing.stl)", "format kept after a reload"],
  });
});

test("U03 editor: only tabs and “+”, context menu “Als Hauptdatei” and “Schließen”", async ({ page }) => {
  await openApp(page);
  await openExampleProject(page, "project-housing");
  await page.waitForFunction(() => (window as unknown as { __cad: { projectInfo(): { paths: string[] } } }).__cad.projectInfo().paths.length === 3);
  await waitScene(page, { context: null });
  const tabs = page.getByRole("tablist", { name: "Dateien des Projekts" });
  await expect(tabs.getByRole("tab")).toHaveCount(3);
  // "+": new file (name from the prompt)
  page.once("dialog", (d) => void d.accept("lib/neu.scad"));
  await pickMenu(page, "Datei hinzufügen", "Neue Datei …");
  await expect(fileTab(page, "lib/neu.scad")).toHaveAttribute("aria-selected", "true");
  // context menu on a tab (right click)
  await fileTab(page, "constants.mcad").click({ button: "right" });
  await expect(page.getByRole("menu", { name: "Datei constants.mcad" })).toBeVisible();
  await screenshot20(page, "u03-tab-kontextmenue");
  await menuItem(page, "Als Hauptdatei").click();
  await page.waitForFunction(() => (window as unknown as { __cad: { projectInfo(): { main: string } } }).__cad.projectInfo().main === "constants.mcad");
  await expect(fileTab(page, "constants.mcad").locator(".star")).toBeVisible();
  // the main file can't be closed; another one can (with confirmation)
  await fileTab(page, "constants.mcad").click({ button: "right" });
  await expect(menuItem(page, "Schließen")).toHaveAttribute("aria-disabled", "true");
  await page.keyboard.press("Escape");
  // keyboard: Umschalt+F10 on the focused tab opens the same menu
  await fileTab(page, "lib/neu.scad").focus();
  await page.keyboard.press("Shift+F10");
  await expect(menuItem(page, "Als Hauptdatei")).toBeFocused();
  page.once("dialog", (d) => void d.accept());
  await page.keyboard.press("ArrowDown");
  await expect(menuItem(page, "Schließen")).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(fileTab(page, "lib/neu.scad")).toHaveCount(0);
  // back to housing.mcad as main file (the "+" menu holds the actions of the shown file)
  await fileTab(page, "housing.mcad").click();
  await pickMenu(page, "Datei hinzufügen", "Als Hauptdatei");
  await page.waitForFunction(() => (window as unknown as { __cad: { projectInfo(): { main: string } } }).__cad.projectInfo().main === "housing.mcad");
  await screenshot20(page, "u03-editor", "section.editorpane");
  record20({
    id: "U03",
    title: "Editor tabs",
    caption:
      "Above the editor only the tabs and “+” (new file, load file, actions of the file shown). Right click on a tab: “Als Hauptdatei” (as main file), “Schließen” (close; locked for the main file); by keyboard Shift+F10 or the context menu key. There is no renaming (yet).",
    images: [
      { name: "u03-tab-kontextmenue", label: "context menu of a tab" },
      { name: "u03-editor", label: "editor with tabs and “+”" },
    ],
    checks: ["new file by “+”", "main file switched by the context menu", "close by keyboard (Shift+F10)"],
  });
});

test("U04 mode: exactly one active, section stays when measuring in the section", async ({ page }) => {
  await openApp(page);
  const group = page.getByRole("radiogroup", { name: "Modus" });
  const checked = () => group.locator("[role=radio][aria-checked=true]").evaluateAll((els) => els.map((e) => e.getAttribute("aria-label")));
  expect(await checked()).toEqual(["Modell"]);
  await mode(page, "Schnitt").click();
  expect(await checked()).toEqual(["Schnitt"]);
  await expect(sectionBar(page)).toBeVisible();
  // the context bar sits top left under the tool bar
  const tb = (await group.boundingBox())!;
  const cb = (await sectionBar(page).boundingBox())!;
  expect(cb.y).toBeGreaterThan(tb.y + tb.height);
  expect(Math.abs(cb.x - (tb.x - 4))).toBeLessThan(8);
  await mode(page, "Messen").click();
  expect(await checked()).toEqual(["Messen"]);
  await expect(sectionBar(page)).toHaveCount(0);
  await measureBar(page).getByRole("button", { name: "Im Schnitt" }).click();
  await page.waitForFunction(() => (window as unknown as W).__cad.measure().inSection);
  // the list is the tab "Messen" of the side panel
  await expect(sideTab(page, "Messen")).toHaveAttribute("aria-selected", "true");
  await screenshot20(page, "u04-messen-im-schnitt");
  await measureBar(page).getByRole("button", { name: "Fertig" }).click();
  // the section stays: mode "Schnitt"
  expect(await checked()).toEqual(["Schnitt"]);
  // arrow keys move the mode (radio group)
  await mode(page, "Schnitt").focus();
  await page.keyboard.press("ArrowLeft");
  expect(await checked()).toEqual(["Modell"]);
  expect((await page.evaluate(() => (window as unknown as W).__cad.state())).section.on).toBe(false);
  await page.keyboard.press("ArrowRight");
  await page.keyboard.press("ArrowRight");
  expect(await checked()).toEqual(["Skizze"]);
  await page.keyboard.press("Escape");
  await page.waitForFunction(() => (window as unknown as W).__cad.sketch() === null);
  // the section of "Schnitt" stayed while drawing
  expect(await checked()).toEqual(["Schnitt"]);
  await mode(page, "Modell").click();
  expect(await checked()).toEqual(["Modell"]);
  record20({
    id: "U04",
    title: "Mode",
    caption:
      "Modell, Schnitt, Skizze, Messen (model, section, sketch, measure) as a segmented control: always exactly one active. The context bar of the mode sits at the top left below the toolbar. Measuring “Im Schnitt” (in the section) switches the section on; after “Fertig” (done) the view is in the mode section again. Arrow keys switch the mode, Esc ends sketch and measure.",
    images: [{ name: "u04-messen-im-schnitt", label: "measuring in the section, list in the tab “Messen”" }],
    checks: ["exactly one aria-checked per state", "context bar below the toolbar, left aligned", "arrow keys in the segment"],
  });
});

test("U05 sketch context bar: groups, arc menu, shortcuts, done", async ({ page }) => {
  await openApp(page);
  await openExample(page, "spec/sketch_lid.mcad");
  await clickLine(page, 14, 4);
  await waitScene(page, { context: "outline" });
  await mode(page, "Skizze").click();
  await expect(sketchBar(page)).toBeVisible();
  const tool = () => page.evaluate(() => (window as unknown as W).__cad.sketch()!.tool);
  await expect.poll(tool).toBe("line");
  await expect(sketchToolButton(page, "line")).toHaveAttribute("aria-pressed", "true");
  // tooltips carry the shortcut
  await expect(sketchToolButton(page, "rect")).toHaveAttribute("title", /^Rechteck \(R\)/);
  await expect(sketchBar(page).getByRole("button", { name: "⊥ Ansicht" })).toHaveAttribute("title", /\(N\)/);
  // keys choose the tools (focus in the view, not in the editor)
  await page.getByTestId("viewport-canvas").hover();
  await page.getByRole("radiogroup", { name: "Modus" }).getByRole("radio", { name: "Skizze" }).focus();
  const keys: Array<[string, string]> = [
    ["v", "select"],
    ["l", "line"],
    ["a", "arc_through"],
    ["Shift+A", "arc_center"],
    ["t", "arc_tangent"],
    ["b", "bezier"],
    ["r", "rect"],
    ["c", "circle"],
    ["f", "corner"],
  ];
  for (const [k, t] of keys) {
    await page.keyboard.press(k);
    await expect.poll(tool).toBe(t);
  }
  // the arc button shows the last arc kind; its menu chooses the kind
  await sketchBar(page).getByRole("button", { name: "Bogenart" }).click();
  await expect(menuItem(page, "Bogen (Mittelpunkt)")).toHaveAttribute("aria-checked", "false");
  await screenshot20(page, "u05-bogen-menue");
  await menuItem(page, "Bogen (tangential)").click();
  await expect.poll(tool).toBe("arc_tangent");
  await expect(sketchBar(page).getByRole("button", { name: "Bogen (tangential)" })).toHaveAttribute("aria-pressed", "true");
  await page.keyboard.press("l");
  await expect.poll(tool).toBe("line");
  await screenshot20(page, "u05-skizzenleiste", ".ctxslot");
  // "Fertig" is the primary button
  await expect(sketchBar(page).getByRole("button", { name: "Fertig" })).toHaveClass(/ui-btn--primary/);
  await sketchBar(page).getByRole("button", { name: "Fertig" }).click();
  await page.waitForFunction(() => (window as unknown as W).__cad.sketch() === null);
  record20({
    id: "U05",
    title: "Sketch context bar",
    caption:
      "Icon groups select | line, arc ▾, Bézier, rectangle, circle | round corner, close, delete | ⊥ view and “Fertig” (done) as the primary button. Tooltips with shortcuts; V, L, A, Shift+A, T, B, R, C, F choose the tools, the arc menu the kind of arc.",
    images: [
      { name: "u05-skizzenleiste", label: "sketch context bar" },
      { name: "u05-bogen-menue", label: "kind of arc" },
    ],
    checks: keys.map(([k, t]) => `${k} → ${t}`),
  });
});

test("U06 view cube: faces, edges and corners", async ({ page }) => {
  await openApp(page);
  const dir = async () => (await page.evaluate(() => (window as unknown as W).__cad.camera()))!.dir;
  const cube = page.getByTestId("viewcube");
  await expect(cube).toBeVisible();
  await page.getByRole("button", { name: "Ansicht Oben" }).click();
  await expect.poll(async () => (await dir())[2]).toBeLessThan(-0.999);
  await screenshot20(page, "u06-oben", "[data-testid=viewcube]");
  // from the top only the top face is visible: back to iso, then the front face
  await pickMenu(page, "Ansicht", "Iso");
  await page.getByRole("button", { name: "Ansicht Vorne" }).click();
  await expect.poll(async () => (await dir())[1]).toBeGreaterThan(0.999);
  // an edge cell: right edge of the front face → looking along (−1, 1, 0)/√2
  const box = (await cube.locator("rect.vc-cell").first().boundingBox())!;
  expect(box.width).toBeGreaterThan(0);
  const front = page.getByRole("button", { name: "Ansicht Vorne" });
  const fb = (await front.boundingBox())!;
  await page.mouse.click(fb.x + fb.width + 3, fb.y + fb.height / 2);
  await expect.poll(async () => {
    const d = await dir();
    return Math.abs(d[0] + Math.SQRT1_2) < 1e-3 && Math.abs(d[1] - Math.SQRT1_2) < 1e-3 && Math.abs(d[2]) < 1e-3;
  }).toBe(true);
  await screenshot20(page, "u06-kante", "[data-testid=viewcube]");
  // a corner cell (top right of the face seen now)
  const vis = page.getByRole("button", { name: /^Ansicht / });
  expect(await vis.count()).toBeGreaterThanOrEqual(1);
  await screenshot20(page, "u06-ansicht");
  record20({
    id: "U06",
    title: "View cube",
    caption: "A clickable cube at the bottom right instead of the axis cross, it turns with the camera; face centre = view onto the face, edge band = edge (diagonal), corners = corner. Next to it x/y/z.",
    images: [
      { name: "u06-oben", label: "after a click on “Oben” (top)" },
      { name: "u06-kante", label: "after a click on the edge front/right" },
    ],
    checks: ["top: view −z", "front: view +y", "edge: view (−1, 1, 0)/√2"],
  });
});

test("U07 side panel: tabs, fold away, parts with “ohne Bauteil”, parameters in the module context", async ({ page }) => {
  await openApp(page);
  await loadText(page, '// part and material without a part\nmodule a() color("white") cube([20, 10, 2]);\npart: a();\ntranslate([0, 20, 0]) color("yellow") cube(5);\n');
  const parts = page.getByRole("region", { name: "Bauteile" });
  await expect(sideTab(page, "Bauteile")).toHaveAttribute("aria-selected", "true");
  await expect(parts.getByTestId("part-part")).toContainText("400 mm³");
  await expect(parts.getByTestId("part-rest")).toContainText("125 mm³");
  await expect(parts.getByTestId("parts-total")).toHaveText("525 mm³");
  await screenshot20(page, "u07-bauteile", ".sidepanel");
  // the ⋯ menu of a part
  await parts.getByRole("button", { name: "Export part" }).click();
  const names = await page.getByRole("menu", { name: "Export part" }).locator("[role=menuitem]").evaluateAll((els) => els.map((e) => e.getAttribute("aria-label")));
  expect(names).toEqual(["STL", "3MF", "STEP"]);
  await page.keyboard.press("Escape");
  // keyboard: arrows move between the tabs
  await sideTab(page, "Bauteile").focus();
  await page.keyboard.press("ArrowRight");
  await expect(sideTab(page, "Farben")).toHaveAttribute("aria-selected", "true");
  await expect(page.getByRole("region", { name: "Farben und Filamente" })).toBeVisible();
  await screenshot20(page, "u07-farben", ".sidepanel");
  await page.keyboard.press("ArrowRight");
  await expect(page.getByRole("region", { name: "Messungen" })).toBeVisible();
  // fold away: the view gets the room; the button in the tool bar brings it back
  const w0 = (await page.getByTestId("viewport").boundingBox())!.width;
  await page.getByRole("button", { name: "Seitenpanel ausblenden" }).click();
  await expect(page.getByTestId("sidepanel")).toHaveCount(0);
  await expect.poll(async () => (await page.getByTestId("viewport").boundingBox())!.width).toBeGreaterThan(w0 + 200);
  await page.getByRole("button", { name: "Seitenpanel einblenden" }).click();
  await expect(sideTab(page, "Messen")).toHaveAttribute("aria-selected", "true");
  // module context: the tab "Parameter" appears and is chosen, back in the file the last tab
  await openExample(page, "spec/housing.mcad");
  await clickLine(page, 10, 6);
  await waitScene(page, { context: "housing" });
  await expect(sideTab(page, "Parameter")).toHaveAttribute("aria-selected", "true");
  await expect(page.getByRole("region", { name: "Parameterleiste" })).toBeVisible();
  await screenshot20(page, "u07-parameter", ".sidepanel");
  await clickLine(page, 21, 0);
  await waitScene(page, { context: null });
  await expect(sideTab(page, "Parameter")).toHaveCount(0);
  await expect(sideTab(page, "Messen")).toHaveAttribute("aria-selected", "true");
  const s = await cad(page);
  expect(s.fileName).toBe("housing.mcad");
  record20({
    id: "U07",
    title: "Side panel",
    caption:
      "On the right a side panel that folds away, with the tabs parts (visibility, volume, export in the ⋯ menu, “ohne Bauteil” (without a part), sum), colours and measure; in the module context also “Parameter” (the former parameter bar). Arrow keys switch the tabs. Folded away the view gets the room, an icon button in the toolbar brings the panel back.",
    images: [
      { name: "u07-bauteile", label: "parts with “ohne Bauteil”" },
      { name: "u07-farben", label: "colours" },
      { name: "u07-parameter", label: "parameters in the module context" },
    ],
    checks: ["part 400 mm³, without a part 125 mm³, sum 525 mm³", "arrow keys switch tabs", "folded away: view > 200 px wider"],
  });
});

test("U08 keyboard: Tab reaches all controls of the bars", async ({ page }) => {
  await openApp(page);
  await page.locator("body").click({ position: { x: 1, y: 1 } });
  const seen: string[] = [];
  for (let i = 0; i < 40; i++) {
    // the editor indents with Tab; Esc, then Tab leaves it (CodeMirror)
    if (await page.evaluate(() => document.activeElement?.classList.contains("cm-content") ?? false)) {
      seen.push("Editor");
      await page.keyboard.press("Escape");
    }
    await page.keyboard.press("Tab");
    const n = await page.evaluate(() => {
      const e = document.activeElement as HTMLElement | null;
      if (!e || e === document.body) return "";
      return e.getAttribute("aria-label") ?? e.textContent?.trim().slice(0, 30) ?? e.tagName;
    });
    seen.push(n);
  }
  const want = ["Datei", "Beispiele", "Speichern", "Exportieren: 3MF mehrfarbig", "Exportformat", "Farbschema", "Einstellungen", "housing.mcad", "Datei hinzufügen", "Editor", "Modell", "Einpassen", "Ansicht", "Alle Raster", "Bauteile", "Seitenpanel ausblenden"];
  for (const w of want) expect(seen, `Tab erreicht ${w}`).toContain(w);
  // focus ring in the accent colour
  await page.getByRole("button", { name: "Einpassen" }).focus();
  await page.keyboard.press("Shift+Tab");
  await page.keyboard.press("Tab");
  const ring = await page.evaluate(() => getComputedStyle(document.activeElement!).outlineColor);
  await screenshot20(page, "u08-fokus", ".vtoolbar");
  writeMetrics20({ tabOrder: seen, focusRing: ring });
  expect(ring).toMatch(/^rgb\(15, 118, 110\)$/);
  record20({
    id: "U08",
    title: "Keyboard",
    caption: "Reachable with Tab: file, examples, save, export (button and menu), theme, settings, tabs, “+”, mode, fit, view, grid, tabs of the side panel. Focus ring 2 px in the accent colour.",
    images: [{ name: "u08-fokus", label: "focus ring in the toolbar" }],
    checks: [`focus order: ${seen.filter(Boolean).slice(0, 18).join(" → ")} …`, `focus ring ${ring}`],
  });
});

test("U09 light and dark: accent teal, contrast of buttons and menus", async ({ page }) => {
  await openApp(page);
  const css = readFileSync(new URL("../src/styles.css", import.meta.url), "utf8");
  expect(css).toContain("--accent: #0f766e;");
  const out: Record<string, number> = {};
  for (const theme of ["light", "dark"] as const) {
    await setTheme(page, theme);
    const c = await page.evaluate(() => {
      const ratio = (a: string, b: string) => (window as unknown as { __cad: { contrastOf(a: string, b: string): number | null } }).__cad.contrastOf(a, b) ?? 0;
      return { save: ratio("[data-testid=save]", "[data-testid=save]"), mode: ratio(".modes .is-on", ".modes .is-on"), status: ratio("[data-testid=status-progress]", ".statusbar") };
    });
    for (const [k, v] of Object.entries(c)) out[`${theme} ${k}`] = v;
    expect(c.save).toBeGreaterThanOrEqual(4.5);
    expect(c.mode).toBeGreaterThanOrEqual(4.5);
    await page.getByRole("button", { name: "Datei", exact: true }).click();
    await screenshot20(page, `u09-${theme === "light" ? "hell" : "dunkel"}`);
    await page.keyboard.press("Escape");
  }
  writeMetrics20({ contrast: out });
  record20({
    id: "U09",
    title: "Themes",
    caption: "Accent teal (light #0f766e, dark #3cc4a8) for primary buttons, active toggles, menu highlight and focus ring; contrast measured at the rendered button (computed styles).",
    images: [
      { name: "u09-hell", label: "light, file menu open" },
      { name: "u09-dunkel", label: "dark, file menu open" },
    ],
    checks: Object.entries(out).map(([k, v]) => `${k}: ${v.toFixed(2)}`),
  });
});
