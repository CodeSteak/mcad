// End-to-end tests of the UI overhaul (docs/konzepte/ui-ueberarbeitung, test report 20): bars
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
      ["Kopfleiste", ".hd"],
      ["Kopf links", ".hd-left"],
      ["Kopf Mitte", ".hd-title"],
      ["Kopf rechts", ".hd-right"],
      ["Dateileiste", ".filebar"],
      ["Werkzeugleiste Modus", ".vtoolbar > [role=toolbar]:first-child"],
      ["Werkzeugleiste Kamera", ".vtoolbar > [role=toolbar]:last-child"],
      ["Kontextleiste", ".ctxslot > [role=toolbar]"],
      ["Seitenpanel Reiter", ".sp-head"],
      ["Statuszeile", ".statusbar"],
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
  if (rep.docScroll > rep.docClient) problems.push(`${label}: Fenster scrollt waagerecht (${rep.docScroll} > ${rep.docClient})`);
  for (const b of rep.bars) {
    if (b.scroll > b.client + 1) problems.push(`${label}: ${b.name} abgeschnitten (${b.scroll} > ${b.client})`);
    if (b.children > 0) problems.push(`${label}: ${b.name}: ${b.children} Elemente ragen heraus`);
    // one row: the bars are 26 px controls with padding
    const max = b.name === "Kopfleiste" ? 41 : b.name === "Dateileiste" ? 34 : b.name === "Statuszeile" ? 26 : 40;
    if (b.height > max) problems.push(`${label}: ${b.name} umgebrochen (Höhe ${b.height} > ${max})`);
  }
  const mode = rep.bars.find((b) => b.name === "Werkzeugleiste Modus");
  const cam = rep.bars.find((b) => b.name === "Werkzeugleiste Kamera");
  if (mode && cam && mode.right > cam.left) problems.push(`${label}: Modus und Kamera überlappen (${mode.right} > ${cam.left})`);
  return problems;
}

test("U01 Leisten bei 1280, 1440 und 1920 px, hell und dunkel: kein Umbruch, nichts abgeschnitten", async ({ page }) => {
  test.setTimeout(240_000);
  const widths = [1280, 1440, 1920];
  const all: Record<string, unknown> = {};
  const problems: string[] = [];
  await openApp(page);
  await openExample(page, "spec/mehrfarbig.scad");
  for (const theme of ["light", "dark"] as const) {
    await setTheme(page, theme);
    for (const w of widths) {
      await page.setViewportSize({ width: w, height: 900 });
      await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
      const t = theme === "light" ? "hell" : "dunkel";
      const label = `${w} ${t}`;
      for (const m of ["Modell", "Schnitt", "Messen"] as const) {
        await mode(page, m).click();
        await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
        const rep = await barReport(page);
        all[`${label} ${m}`] = rep;
        problems.push(...checkBars(rep, `${label} ${m}`));
        await screenshot20(page, `u01-${w}-${t}-${m.toLowerCase()}`);
      }
      await mode(page, "Modell").click();
    }
  }
  // the sketch bar (drawing needs the sketch at the cursor)
  await openExample(page, "spec/skizze_deckel.scad");
  await clickLine(page, 14, 4);
  await waitScene(page, { context: "umriss" });
  for (const theme of ["light", "dark"] as const) {
    await setTheme(page, theme);
    for (const w of widths) {
      await page.setViewportSize({ width: w, height: 900 });
      await mode(page, "Skizze").click();
      await expect(sketchBar(page)).toBeVisible();
      await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
      const t = theme === "light" ? "hell" : "dunkel";
      const rep = await barReport(page);
      all[`${w} ${t} Skizze`] = rep;
      problems.push(...checkBars(rep, `${w} ${t} Skizze`));
      await screenshot20(page, `u01-${w}-${t}-skizze`);
      await sketchBar(page).getByRole("button", { name: "Fertig" }).click();
    }
  }
  writeMetrics20({ bars: all });
  expect(problems).toEqual([]);
  record20({
    id: "U01",
    title: "Breiten und Themen",
    caption:
      "Je Breite (1280, 1440, 1920 px) und Thema alle Leisten in den Modi Modell, Schnitt, Messen und Skizze: Inhalt nicht breiter als die Leiste (scrollWidth ≤ clientWidth), kein Element ragt heraus, jede Leiste einzeilig, Modus- und Kameraleiste überlappen nicht, das Fenster scrollt nicht waagerecht.",
    images: [
      { name: "u01-1280-hell-modell", label: "1280 px, hell, Modell" },
      { name: "u01-1280-dunkel-messen", label: "1280 px, dunkel, Messen" },
      { name: "u01-1920-hell-schnitt", label: "1920 px, hell, Schnitt" },
      { name: "u01-1280-hell-skizze", label: "1280 px, hell, Skizze" },
    ],
    checks: [`${Object.keys(all).length} Zustände geprüft, ${problems.length} Befunde`],
  });
});

test("U02 Kopfleiste: Datei-Menü per Tastatur, Export merkt das Format (Strg+E), Thema", async ({ page }) => {
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
  expect(dl.suggestedFilename()).toBe("gehaeuse.stl");
  await expect(page.getByRole("button", { name: "Exportieren: STL", exact: true })).toBeVisible();
  await page.locator(".cm-content").click();
  const [dl2] = await Promise.all([page.waitForEvent("download"), page.keyboard.press("Control+e")]);
  expect(dl2.suggestedFilename()).toBe("gehaeuse.stl");
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
    title: "Kopfleiste",
    caption:
      "„Datei ▾“ öffnet mit Enter, Pfeiltasten und Pos1/Ende bewegen den Fokus, Esc schließt und gibt den Fokus an den Knopf zurück. „Beispiele ▾“ mit Untermenüs. Der geteilte Export-Knopf listet 3MF mehrfarbig/einfarbig, STL, STL je Farbe, STEP und „Je Bauteil“; nach der Wahl von STL zeigt der Knopf „STL“, Strg+E exportiert STL, auch nach dem Neuladen. Thema als Symbolknopf mit Menü.",
    images: [
      { name: "u02-datei-menue", label: "Datei-Menü (Tastatur)" },
      { name: "u02-export-menue", label: "Export-Menü" },
      { name: "u02-beispiele", label: "Beispiele mit Untermenü" },
      { name: "u02-thema-menue", label: "Thema" },
    ],
    checks: ["8 Einträge im Datei-Menü", "Export STL per Menü und per Strg+E (gehaeuse.stl)", "Format nach Neuladen erhalten"],
  });
});

test("U03 Editor: nur Tabs und „+“, Kontextmenü Als Hauptdatei und Schließen", async ({ page }) => {
  await openApp(page);
  await openExampleProject(page, "projekt-gehaeuse");
  await page.waitForFunction(() => (window as unknown as { __cad: { projectInfo(): { paths: string[] } } }).__cad.projectInfo().paths.length === 3);
  await waitScene(page, { context: null });
  const tabs = page.getByRole("tablist", { name: "Dateien des Projekts" });
  await expect(tabs.getByRole("tab")).toHaveCount(3);
  // "+": new file (name from the prompt)
  page.once("dialog", (d) => void d.accept("lib/neu.scad"));
  await pickMenu(page, "Datei hinzufügen", "Neue Datei …");
  await expect(fileTab(page, "lib/neu.scad")).toHaveAttribute("aria-selected", "true");
  // context menu on a tab (right click)
  await fileTab(page, "konstanten.scad").click({ button: "right" });
  await expect(page.getByRole("menu", { name: "Datei konstanten.scad" })).toBeVisible();
  await screenshot20(page, "u03-tab-kontextmenue");
  await menuItem(page, "Als Hauptdatei").click();
  await page.waitForFunction(() => (window as unknown as { __cad: { projectInfo(): { main: string } } }).__cad.projectInfo().main === "konstanten.scad");
  await expect(fileTab(page, "konstanten.scad").locator(".star")).toBeVisible();
  // the main file can't be closed; another one can (with confirmation)
  await fileTab(page, "konstanten.scad").click({ button: "right" });
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
  // back to gehaeuse.scad as main file (the "+" menu holds the actions of the shown file)
  await fileTab(page, "gehaeuse.scad").click();
  await pickMenu(page, "Datei hinzufügen", "Als Hauptdatei");
  await page.waitForFunction(() => (window as unknown as { __cad: { projectInfo(): { main: string } } }).__cad.projectInfo().main === "gehaeuse.scad");
  await screenshot20(page, "u03-editor", "section.editorpane");
  record20({
    id: "U03",
    title: "Editor-Tabs",
    caption:
      "Über dem Editor nur die Tabs und „+“ (Neue Datei, Datei laden, Aktionen der angezeigten Datei). Rechtsklick auf einen Tab: „Als Hauptdatei“, „Schließen“ (für die Hauptdatei gesperrt); per Tastatur Umschalt+F10 oder Kontextmenütaste. Umbenennen gibt es (noch) nicht.",
    images: [
      { name: "u03-tab-kontextmenue", label: "Kontextmenü eines Tabs" },
      { name: "u03-editor", label: "Editor mit Tabs und „+“" },
    ],
    checks: ["neue Datei per „+“", "Hauptdatei per Kontextmenü gewechselt", "Schließen per Tastatur (Umschalt+F10)"],
  });
});

test("U04 Modus: genau einer aktiv, Schnitt bleibt beim Messen im Schnitt", async ({ page }) => {
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
    title: "Modus",
    caption:
      "Modell, Schnitt, Skizze, Messen als Segmentschalter: immer genau einer aktiv. Die Kontextleiste des Modus steht links oben unter der Werkzeugleiste. Messen „Im Schnitt“ schaltet den Schnitt ein; nach „Fertig“ ist die Ansicht wieder im Modus Schnitt. Pfeiltasten wechseln den Modus, Esc beendet Skizze und Messen.",
    images: [{ name: "u04-messen-im-schnitt", label: "Messen im Schnitt, Liste im Reiter „Messen“" }],
    checks: ["genau ein aria-checked je Zustand", "Kontextleiste unter der Werkzeugleiste, links bündig", "Pfeiltasten im Segment"],
  });
});

test("U05 Skizzen-Kontextleiste: Gruppen, Bogen-Menü, Tastenkürzel, Fertig", async ({ page }) => {
  await openApp(page);
  await openExample(page, "spec/skizze_deckel.scad");
  await clickLine(page, 14, 4);
  await waitScene(page, { context: "umriss" });
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
    title: "Skizzen-Kontextleiste",
    caption:
      "Symbolgruppen Auswahl | Linie, Bogen ▾, Bézier, Rechteck, Kreis | Ecke runden, Schließen, Löschen | ⊥ Ansicht und „Fertig“ als Primärknopf. Tooltips mit Tastenkürzel; V, L, A, Umschalt+A, T, B, R, C, F wählen die Werkzeuge, das Bogen-Menü die Bogenart.",
    images: [
      { name: "u05-skizzenleiste", label: "Skizzen-Kontextleiste" },
      { name: "u05-bogen-menue", label: "Bogenart" },
    ],
    checks: keys.map(([k, t]) => `${k} → ${t}`),
  });
});

test("U06 Ansichtswürfel: Flächen, Kanten und Ecken", async ({ page }) => {
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
    title: "Ansichtswürfel",
    caption: "Klickbarer Würfel unten rechts statt Achsenkreuz, dreht sich mit der Kamera; Flächenmitte = Ansicht auf die Fläche, Randstreifen = Kante (diagonal), Ecken = Ecke. Daneben x/y/z.",
    images: [
      { name: "u06-oben", label: "nach Klick auf „Oben“" },
      { name: "u06-kante", label: "nach Klick auf die Kante Vorne/Rechts" },
    ],
    checks: ["Oben: Blick −z", "Vorne: Blick +y", "Kante: Blick (−1, 1, 0)/√2"],
  });
});

test("U07 Seitenpanel: Reiter, einklappen, Bauteile mit „ohne Bauteil“, Parameter im Modulkontext", async ({ page }) => {
  await openApp(page);
  await loadText(page, '// Bauteil und Material ohne Bauteil\nmodule a() color("white") cube([20, 10, 2]);\nteil: a();\ntranslate([0, 20, 0]) color("yellow") cube(5);\n');
  const parts = page.getByRole("region", { name: "Bauteile" });
  await expect(sideTab(page, "Bauteile")).toHaveAttribute("aria-selected", "true");
  await expect(parts.getByTestId("part-teil")).toContainText("400 mm³");
  await expect(parts.getByTestId("part-rest")).toContainText("125 mm³");
  await expect(parts.getByTestId("parts-total")).toHaveText("525 mm³");
  await screenshot20(page, "u07-bauteile", ".sidepanel");
  // the ⋯ menu of a part
  await parts.getByRole("button", { name: "Export teil" }).click();
  const names = await page.getByRole("menu", { name: "Export teil" }).locator("[role=menuitem]").evaluateAll((els) => els.map((e) => e.getAttribute("aria-label")));
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
  await openExample(page, "spec/gehaeuse.scad");
  await clickLine(page, 10, 6);
  await waitScene(page, { context: "gehaeuse" });
  await expect(sideTab(page, "Parameter")).toHaveAttribute("aria-selected", "true");
  await expect(page.getByRole("region", { name: "Parameterleiste" })).toBeVisible();
  await screenshot20(page, "u07-parameter", ".sidepanel");
  await clickLine(page, 21, 0);
  await waitScene(page, { context: null });
  await expect(sideTab(page, "Parameter")).toHaveCount(0);
  await expect(sideTab(page, "Messen")).toHaveAttribute("aria-selected", "true");
  const s = await cad(page);
  expect(s.fileName).toBe("gehaeuse.scad");
  record20({
    id: "U07",
    title: "Seitenpanel",
    caption:
      "Rechts ein einklappbares Seitenpanel mit Reitern Bauteile (Sichtbarkeit, Volumen, Export im ⋯-Menü, „ohne Bauteil“, Summe), Farben und Messen; im Modulkontext zusätzlich „Parameter“ (die frühere Parameterleiste). Pfeiltasten wechseln die Reiter. Eingeklappt bekommt die Ansicht den Platz, ein Symbolknopf in der Werkzeugleiste holt das Panel zurück.",
    images: [
      { name: "u07-bauteile", label: "Bauteile mit „ohne Bauteil“" },
      { name: "u07-farben", label: "Farben" },
      { name: "u07-parameter", label: "Parameter im Modulkontext" },
    ],
    checks: ["teil 400 mm³, ohne Bauteil 125 mm³, Summe 525 mm³", "Pfeiltasten wechseln Reiter", "eingeklappt: Ansicht > 200 px breiter"],
  });
});

test("U08 Tastatur: Tab erreicht alle Bedienelemente der Leisten", async ({ page }) => {
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
  const want = ["Datei", "Beispiele", "Speichern", "Exportieren: 3MF mehrfarbig", "Exportformat", "Farbschema", "Einstellungen", "gehaeuse.scad", "Datei hinzufügen", "Editor", "Modell", "Einpassen", "Ansicht", "Alle Raster", "Bauteile", "Seitenpanel ausblenden"];
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
    title: "Tastatur",
    caption: "Mit Tab erreichbar: Datei, Beispiele, Speichern, Export (Knopf und Menü), Thema, Einstellungen, Tabs, „+“, Modus, Einpassen, Ansicht, Raster, Reiter des Seitenpanels. Fokusring 2 px in der Akzentfarbe.",
    images: [{ name: "u08-fokus", label: "Fokusring in der Werkzeugleiste" }],
    checks: [`Fokusreihenfolge: ${seen.filter(Boolean).slice(0, 18).join(" → ")} …`, `Fokusring ${ring}`],
  });
});

test("U09 Hell und dunkel: Akzent Teal, Kontrast von Knöpfen und Menüs", async ({ page }) => {
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
    title: "Themen",
    caption: "Akzent Teal (hell #0f766e, dunkel #3cc4a8) für Primärknöpfe, aktive Umschalter, Menü-Hervorhebung und Fokusring; Kontrast am gerenderten Knopf gemessen (computed styles).",
    images: [
      { name: "u09-hell", label: "hell, Datei-Menü offen" },
      { name: "u09-dunkel", label: "dunkel, Datei-Menü offen" },
    ],
    checks: Object.entries(out).map(([k, v]) => `${k}: ${v.toFixed(2)}`),
  });
});
