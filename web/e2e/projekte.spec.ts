// End-to-end tests of the browser UI, Phase D: projects of several files (include/use), file
// tabs, Ctrl+click on include paths, handles writing into included files, hover and click
// across files, parts (visibility, export per part) and indexed named instances `k[i]`.
// Checks run on the UI state (`window.__cad`), the editor text, downloads and pixels; each
// scenario records screenshots and code for test report 6.

import { readFileSync } from "node:fs";
import { expect, test, type Page } from "@playwright/test";
import {
  type HandleView,
  type Pt,
  cad,
  clickLine,
  frame,
  hook,
  layerPixels,
  line,
  lineEl,
  loadText,
  mouseDrag,
  openApp,
  recordD,
  screenshotD,
  waitDragDone,
  waitHandles,
  waitScene,
  waitView,
} from "./helpers";

test.beforeEach(async ({ page }) => {
  page.on("pageerror", (e) => console.log("pageerror:", e.message));
});

type ProjectInfo = { name: string; main: string; active: string; paths: string[]; folder: boolean; files: Record<string, string> };
type PartInfo = { name: string; node: number; line: number; volume: number | null; bounds: { min: number[]; max: number[] } | null; hidden: boolean };
type W = {
  __cad: {
    projectInfo(): ProjectInfo;
    openProject(p: { name: string; main: string; files: Record<string, string>; active?: string }): void;
    switchFile(path: string, line?: number): boolean;
    parts(): PartInfo[];
    text(): string;
    cursorLine(): number;
    store: { getState(): Record<string, unknown> };
  };
};

const info = (page: Page) => page.evaluate(() => (window as unknown as W).__cad.projectInfo());
const parts = (page: Page) => page.evaluate(() => (window as unknown as W).__cad.parts());
const text = (page: Page) => hook(page, (c) => c.text());
const grip = (h: HandleView): Pt => {
  if (!h.screen) throw new Error(`handle ${h.id} not visible`);
  return h.screen;
};
const waitText = (page: Page, pred: string) =>
  page.waitForFunction((src) => new Function("t", `return (${src})(t);`)((window as unknown as W).__cad.text()), pred, { timeout: 30_000, polling: 30 });
const waitActive = (page: Page, path: string) =>
  page.waitForFunction((p) => (window as unknown as W).__cad.projectInfo().active === p, path, { timeout: 20_000, polling: 30 });
const toScreen = async (page: Page, p: number[]): Promise<Pt> => {
  const s = await hook(page, (c, q) => c.project(q as [number, number, number]), p);
  if (!s || !s.visible) throw new Error(`point ${p} not visible`);
  return { x: s.x, y: s.y };
};

const EX = "../examples/projekt-gehaeuse";
const exampleFiles = {
  "gehaeuse.scad": readFileSync(`${EX}/gehaeuse.scad`, "utf8"),
  "konstanten.scad": readFileSync(`${EX}/konstanten.scad`, "utf8"),
  "lib/teile.scad": readFileSync(`${EX}/lib/teile.scad`, "utf8"),
};

/** Opens the example project from the menu and waits for its complete scene. */
async function openExampleProject(page: Page): Promise<void> {
  await page.getByTestId("examples").selectOption("projekt:projekt-gehaeuse");
  await page.waitForFunction(() => (window as unknown as W).__cad.projectInfo().paths.length === 3);
  await waitScene(page, { context: null });
  await page.waitForFunction(() => (window as unknown as W).__cad.parts().length === 2);
}

/** Bytes of a download. */
async function downloaded(page: Page, click: () => Promise<void>): Promise<{ name: string; bytes: Buffer }> {
  const [dl] = await Promise.all([page.waitForEvent("download"), click()]);
  return { name: dl.suggestedFilename(), bytes: readFileSync((await dl.path())!) };
}

/** Names of the entries of a stored ZIP (central directory). */
function zipNames(b: Buffer): string[] {
  const out: string[] = [];
  let p = b.lastIndexOf(Buffer.from([0x50, 0x4b, 0x05, 0x06]));
  const n = b.readUInt16LE(p + 10);
  p = b.readUInt32LE(p + 16);
  for (let k = 0; k < n; k++) {
    const nl = b.readUInt16LE(p + 28);
    const xl = b.readUInt16LE(p + 30);
    const cl = b.readUInt16LE(p + 32);
    out.push(b.subarray(p + 46, p + 46 + nl).toString("utf8"));
    p += 46 + nl + xl + cl;
  }
  return out;
}

// ---------------------------------------------------------------- projects

test("P01 Beispielprojekt öffnen: Dateien als Tabs, Hauptdatei, Bauteile", async ({ page }) => {
  await openApp(page);
  await openExampleProject(page);
  const p = await info(page);
  expect(p.paths).toEqual(["gehaeuse.scad", "konstanten.scad", "lib/teile.scad"]);
  expect(p.main).toBe("gehaeuse.scad");
  expect(p.active).toBe("gehaeuse.scad");
  await expect(page.getByTestId("tab-gehaeuse.scad")).toHaveClass(/active/);
  await expect(page.getByTestId("tab-gehaeuse.scad").locator(".star")).toBeVisible();
  await expect(page.getByTestId("pathbar")).toContainText("gehaeuse.scad");
  const s = await cad(page);
  expect(s.diagnostics.filter((d) => d.severity !== "info")).toEqual([]);
  const files = await page.evaluate(() => ((window as unknown as W).__cad.store.getState().complete as { files: Array<{ path: string; role: string }> }).files);
  expect(files.map((f) => `${f.path}:${f.role}`)).toEqual(["gehaeuse.scad:main", "konstanten.scad:include", "lib/teile.scad:use"]);
  const ps = await parts(page);
  expect(ps.map((x) => x.name)).toEqual(["gehaeuse", "deckel"]);
  expect(ps[1].bounds!.min[1]).toBeCloseTo(50, 5);
  await expect(page.getByTestId("partsbar")).toContainText("gehaeuse");
  await expect(page.getByTestId("partsbar")).toContainText("deckel");
  // The model of both parts is drawn (root mesh).
  const b = (await hook(page, (c) => c.rootBounds()))!;
  expect(b.max[1]).toBeCloseTo(90, 3);
  await screenshotD(page, "p01-projekt");
  recordD({
    id: "P01",
    title: "Beispielprojekt öffnen",
    images: [{ name: "p01-projekt", label: "projekt-gehaeuse/: drei Dateien als Tabs, Hauptdatei ★, Bauteilleiste" }],
    code: exampleFiles["gehaeuse.scad"].trim(),
    caption: "Das Beispielmenü öffnet examples/projekt-gehaeuse/ als Projekt: gehaeuse.scad (Hauptdatei, ★), konstanten.scad (per include) und lib/teile.scad (per use). Die Szene kennt die drei Dateien mit ihrer Rolle; die zwei benannten Instanzen auf oberster Ebene sind die Bauteile.",
    checks: [
      "Tabs gehaeuse.scad (aktiv, ★), konstanten.scad, lib/teile.scad",
      "Scene.files: main, include, use; keine Fehler oder Warnungen",
      "Bauteile gehaeuse und deckel; Hülle des Deckels ab y = 50",
    ],
  });
});

test("P02 Ordner öffnen (File System Access API), ZIP hoch- und herunterladen", async ({ page }) => {
  // A stub folder handle in the page (the native picker cannot be driven by Playwright).
  await page.addInitScript((files: Record<string, string>) => {
    type H = { kind: string; name: string; [k: string]: unknown };
    const written: Record<string, string> = {};
    (window as unknown as { __written: Record<string, string> }).__written = written;
    const dir = (name: string, prefix: string): H => ({
      kind: "directory",
      name,
      async *values() {
        const seen = new Set<string>();
        for (const p of Object.keys(files)) {
          if (!p.startsWith(prefix)) continue;
          const rest = p.slice(prefix.length);
          const seg = rest.split("/")[0];
          if (seen.has(seg)) continue;
          seen.add(seg);
          if (rest.includes("/")) yield dir(seg, `${prefix}${seg}/`);
          else yield file(seg, p);
        }
      },
      async getDirectoryHandle(n: string) {
        return dir(n, `${prefix}${n}/`);
      },
      async getFileHandle(n: string) {
        return file(n, `${prefix}${n}`);
      },
    });
    const file = (name: string, path: string): H => ({
      kind: "file",
      name,
      async getFile() {
        return new File([files[path] ?? ""], name);
      },
      async createWritable() {
        let buf = "";
        return {
          async write(d: string) {
            buf += d;
          },
          async close() {
            written[path] = buf;
          },
        };
      },
    });
    (window as unknown as { showDirectoryPicker: () => Promise<H> }).showDirectoryPicker = async () => dir("mein-projekt", "");
  }, { ...exampleFiles, "notizen.txt": "kein scad" });
  await openApp(page);
  await page.getByTestId("open-folder").click();
  await page.waitForFunction(() => (window as unknown as W).__cad.projectInfo().name === "mein-projekt");
  await waitScene(page, { context: null });
  let p = await info(page);
  expect(p.paths).toEqual(["gehaeuse.scad", "konstanten.scad", "lib/teile.scad"]);
  expect(p.main).toBe("gehaeuse.scad");
  expect(p.folder).toBe(true);
  await expect(page.getByTestId("status-notice")).toContainText("Projekt mein-projekt: 3 Dateien, Hauptdatei gehaeuse.scad");
  // Edit konstanten.scad and save: every file is written back into the folder.
  await page.getByTestId("tab-konstanten.scad").click();
  await waitActive(page, "konstanten.scad");
  await clickLine(page, 2, 9);
  await page.keyboard.press("End");
  await page.keyboard.type(" // breiter");
  await page.keyboard.press("Control+s");
  await page.waitForFunction(() => Object.keys((window as unknown as { __written: Record<string, string> }).__written).length === 3);
  const written = await page.evaluate(() => (window as unknown as { __written: Record<string, string> }).__written);
  expect(written["konstanten.scad"]).toContain("breite = 60; // breiter");
  expect(written["lib/teile.scad"]).toBe(exampleFiles["lib/teile.scad"]);
  await screenshotD(page, "p02-ordner");

  // ZIP download of the project, then upload of that ZIP.
  const z = await downloaded(page, () => page.getByTestId("download-zip").click());
  expect(z.name).toBe("mein-projekt.zip");
  expect(zipNames(z.bytes).sort()).toEqual(["mein-projekt/gehaeuse.scad", "mein-projekt/konstanten.scad", "mein-projekt/lib/teile.scad"]);
  await page.evaluate(() => (window as unknown as W).__cad.openProject({ name: "leer", main: "a.scad", files: { "a.scad": "cube(1);\n" } }));
  await waitScene(page);
  const [chooser] = await Promise.all([page.waitForEvent("filechooser"), page.getByTestId("open-zip").click()]);
  await chooser.setFiles({ name: "mein-projekt.zip", mimeType: "application/zip", buffer: z.bytes });
  await page.waitForFunction(() => (window as unknown as W).__cad.projectInfo().paths.length === 3);
  await waitScene(page, { context: null });
  p = await info(page);
  expect(p.name).toBe("mein-projekt");
  expect(p.main).toBe("gehaeuse.scad");
  expect(p.files["konstanten.scad"]).toContain("// breiter");
  expect((await parts(page)).map((x) => x.name)).toEqual(["gehaeuse", "deckel"]);
  recordD({
    id: "P02",
    title: "Ordner öffnen, speichern, ZIP",
    images: [{ name: "p02-ordner", label: "Ordner „mein-projekt“ geöffnet, konstanten.scad geändert und gespeichert" }],
    caption:
      "„Ordner…“ liest alle .scad-Dateien eines Ordners (File System Access API; im Test ein nachgebildeter Ordner mit einer .txt-Datei, die ignoriert wird) und wählt die Hauptdatei: die Datei der obersten Ebene, die keine andere einbindet. Strg+S schreibt alle Dateien zurück in den Ordner. „ZIP ↓“ lädt das Projekt als ZIP herunter, „ZIP laden…“ lädt dieses ZIP wieder hoch (gemeinsamer Ordner wird entfernt, Hauptdatei wieder gehaeuse.scad).",
    checks: ["3 Dateien, Hauptdatei gehaeuse.scad, notizen.txt ignoriert", "Strg+S schreibt 3 Dateien (konstanten.scad mit Änderung)", "ZIP mit 3 Einträgen; nach dem Hochladen gleiche Dateien und Bauteile"],
  });
});

test("P03 Datei wechseln: Tabs, Kontext aus der Bibliothek, Undo je Datei", async ({ page }) => {
  await openApp(page);
  await openExampleProject(page);
  await page.getByTestId("tab-lib/teile.scad").click();
  await waitActive(page, "lib/teile.scad");
  await expect(lineEl(page, 14)).toHaveText("module gehaeuse(w = 60, d = 40, h = 25, t = 2) {");
  // Cursor in module gehaeuse of the library: the module is the context.
  await clickLine(page, 21, 10);
  let s = await waitScene(page, { context: "gehaeuse" });
  expect(s.path).toEqual(["gehaeuse"]);
  await expect(page.getByTestId("pathbar")).toContainText("gehaeuse");
  await waitView(page, "(s) => s.view.selection.line === 21 && s.view.plan.items.some(i => i.mode === 'full')");
  await screenshotD(page, "p03-bibliothek");
  // Edit in the library, switch away and back: text and undo history stay per file.
  await clickLine(page, 3, 0);
  await page.keyboard.press("End");
  await page.keyboard.type(" // mm");
  await waitText(page, "(t) => t.includes('spiel = 0.2; // mm')");
  await page.getByTestId("tab-gehaeuse.scad").click();
  await waitActive(page, "gehaeuse.scad");
  expect(await text(page)).toBe(exampleFiles["gehaeuse.scad"]);
  s = await waitScene(page, { context: null });
  // Undo in the main file does not touch the library.
  await hook(page, (c) => c.focusEditor());
  await page.keyboard.press("Control+z");
  expect(await text(page)).toBe(exampleFiles["gehaeuse.scad"]);
  await page.getByTestId("tab-lib/teile.scad").click();
  await waitActive(page, "lib/teile.scad");
  expect(await text(page)).toContain("spiel = 0.2; // mm");
  await hook(page, (c) => c.focusEditor());
  await page.keyboard.press("Control+z");
  await waitText(page, `(t) => t === ${JSON.stringify(exampleFiles["lib/teile.scad"])}`);
  recordD({
    id: "P03",
    title: "Datei wechseln",
    images: [{ name: "p03-bibliothek", label: "lib/teile.scad im Editor, Cursor im Modul gehaeuse (Kontext)" }],
    code: exampleFiles["lib/teile.scad"].split("\n").slice(13, 23).join("\n"),
    caption:
      "Ein Klick auf den Tab lib/teile.scad zeigt die Bibliothek; der Cursor im Modul gehaeuse macht es zum Kontext (Pfadleiste „Datei › gehaeuse“), Raster und Griffe gehören zur Bibliothek. Jede Datei behält ihren Text und ihre eigene Undo-Historie: Strg+Z in der Hauptdatei ändert die Bibliothek nicht, Strg+Z in der Bibliothek nimmt dort die Änderung zurück.",
    checks: ["Kontext gehaeuse aus lib/teile.scad", "Änderung bleibt beim Wechsel erhalten", "Undo je Datei"],
  });
});

test("P04 Strg+Klick auf include/use-Pfad öffnet die Datei", async ({ page }) => {
  await openApp(page);
  await openExampleProject(page);
  const at = async (l: number, col: number) => {
    const p = await page.evaluate(([a, b]) => (window as unknown as { __cad: { coordsOf(l: number, c: number): { x: number; y: number } | null } }).__cad.coordsOf(a, b), [l, col] as const);
    if (!p) throw new Error("no coords");
    return p;
  };
  // Line 3: include <konstanten.scad>
  const p3 = await at(3, 14);
  await page.mouse.move(p3.x, p3.y);
  await expect(lineEl(page, 3).locator(".cm-include").first()).toBeVisible();
  await page.keyboard.down("Control");
  await page.mouse.click(p3.x, p3.y);
  await page.keyboard.up("Control");
  await waitActive(page, "konstanten.scad");
  expect((await text(page)).split("\n")[1]).toBe("breite = 60;");
  await expect(lineEl(page, 2).locator(".cm-lineend")).toContainText("= 60");
  await screenshotD(page, "p04-include", "editor");
  // Back, then Ctrl+click on use <lib/teile.scad> (line 4).
  await page.getByTestId("tab-gehaeuse.scad").click();
  await waitActive(page, "gehaeuse.scad");
  const p4 = await at(4, 8);
  await page.keyboard.down("Control");
  await page.mouse.click(p4.x, p4.y);
  await page.keyboard.up("Control");
  await waitActive(page, "lib/teile.scad");
  // A plain click places the cursor only.
  await page.getByTestId("tab-gehaeuse.scad").click();
  await waitActive(page, "gehaeuse.scad");
  const q = await at(3, 14);
  await page.mouse.click(q.x, q.y);
  await frame(page);
  expect((await info(page)).active).toBe("gehaeuse.scad");
  recordD({
    id: "P04",
    title: "Strg+Klick auf include",
    images: [{ name: "p04-include", label: "Nach Strg+Klick auf <konstanten.scad>: die Datei ist offen, Werte am Zeilenende" }],
    code: "include <konstanten.scad>\nuse <lib/teile.scad>",
    caption: "Der Pfad in include <…> und use <…> ist unterstrichen (Titel „Strg+Klick: Datei öffnen“). Strg+Klick öffnet die Datei relativ zur einbindenden Datei; ein einfacher Klick setzt nur den Cursor. In konstanten.scad erscheinen die Werte der Variablen am Zeilenende, weil sie per include in der Dateiauswertung gelten.",
    checks: ["Strg+Klick auf <konstanten.scad> → konstanten.scad aktiv", "Strg+Klick auf <lib/teile.scad> → lib/teile.scad aktiv", "einfacher Klick wechselt nicht"],
  });
});

test("P05 Griff ändert die eingebundene Datei (Variable aus include, Literal in der Bibliothek)", async ({ page }) => {
  await openApp(page);
  const main = "include <mass.scad>\ncube([breite, 10, 2]);\n";
  const mass = "// Maße\nbreite = 40;\n";
  await page.evaluate(([m, k]) => (window as unknown as W).__cad.openProject({ name: "masse", main: "main.scad", files: { "main.scad": m, "mass.scad": k } }), [main, mass] as const);
  await waitScene(page, { context: null });
  await clickLine(page, 2, 0);
  const [h] = await waitHandles(page);
  expect(h.kind).toBe("point");
  await screenshotD(page, "p05-vorher");
  const from = grip(h);
  // Free drag (Alt) locked to x (key X): the literal 40 in mass.scad changes, main.scad stays.
  await page.mouse.move(from.x, from.y);
  await page.keyboard.down("Alt");
  await page.mouse.down();
  await page.keyboard.press("x");
  for (const q of line(from, { x: from.x + 80, y: from.y + 20 }, 12)) {
    await page.mouse.move(q.x, q.y);
    await frame(page);
  }
  await page.mouse.up();
  await page.keyboard.up("Alt");
  await waitDragDone(page);
  let p = await info(page);
  expect(p.active).toBe("main.scad");
  expect(await text(page)).toBe(main);
  expect(p.files["mass.scad"]).toMatch(/^\/\/ Maße\nbreite = \d+;\n$/);
  const newW = Number(p.files["mass.scad"].match(/breite = (\d+)/)![1]);
  expect(newW).not.toBe(40);
  const b = (await hook(page, (c) => c.rootBounds()))!;
  expect(Math.round(b.max[0])).toBe(newW);
  await screenshotD(page, "p05-nachher");
  // The drag is one undo step in mass.scad.
  await page.getByTestId("tab-mass.scad").click();
  await waitActive(page, "mass.scad");
  expect(await text(page)).toBe(`// Maße\nbreite = ${newW};\n`);
  await hook(page, (c) => c.focusEditor());
  await page.keyboard.press("Control+z");
  await waitText(page, `(t) => t === ${JSON.stringify(mass)}`);
  await waitScene(page);
  expect((await hook(page, (c) => c.rootBounds()))!.max[0]).toBeCloseTo(40, 6);

  // Literal in the used library: radius handle of the cylinder in module gehaeuse.
  await openExampleProject(page);
  await page.getByTestId("tab-lib/teile.scad").click();
  await waitActive(page, "lib/teile.scad");
  await clickLine(page, 28, 40);
  await waitScene(page, { context: "gehaeuse" });
  const hs = await waitHandles(page, "(hs) => hs.some(h => h.kind === 'radius')");
  const r = hs.find((x) => x.kind === "radius")!;
  const rp = grip(r);
  await mouseDrag(page, rp, line(rp, { x: rp.x + 60, y: rp.y }, 10), { alt: true });
  await waitDragDone(page);
  p = await info(page);
  const lib = await text(page);
  expect(lib).not.toBe(exampleFiles["lib/teile.scad"]);
  expect(lib.split("\n")[27]).toMatch(/cylinder\(h = 3, r = \d+\);/);
  expect(p.files["gehaeuse.scad"]).toBe(exampleFiles["gehaeuse.scad"]);
  await screenshotD(page, "p05-bibliothek");
  recordD({
    id: "P05",
    title: "Griff ändert die eingebundene Datei",
    images: [
      { name: "p05-vorher", label: "main.scad, Cursor auf cube([breite, 10, 2]): Eckgriff" },
      { name: "p05-nachher", label: `Nach dem Ziehen: mass.scad „breite = ${newW};“, main.scad unverändert` },
    ],
    code: `// main.scad\n${main}// mass.scad\n${mass}`,
    caption: `Der Eckgriff des Quaders geht über die Variable breite auf das Literal in mass.scad. Ziehen schrieb „breite = ${newW};“ in mass.scad (nicht in den Editor der Hauptdatei); die Szene folgte. Das Ziehen ist ein Undo-Schritt in mass.scad. Ebenso ändert der Radiusgriff im Modul gehaeuse von lib/teile.scad das Literal r = 2 in Zeile 28 der Bibliothek, gehaeuse.scad bleibt gleich.`,
    checks: ["Edit mit file = mass.scad, Hauptdatei unverändert", "Hülle = neuer Wert", "Strg+Z in mass.scad → breite = 40", "Radiusgriff in lib/teile.scad schreibt in lib/teile.scad"],
  });
});

test("P06 Hover und Klick über Dateigrenzen, Doppelklick öffnet das Modul der Bibliothek", async ({ page }) => {
  await openApp(page);
  await openExampleProject(page);
  // In konstanten.scad (no geometry): faces of the model come from gehaeuse.scad.
  await page.getByTestId("tab-konstanten.scad").click();
  await waitActive(page, "konstanten.scad");
  await waitScene(page, { context: null });
  const deckel = (await parts(page)).find((x) => x.name === "deckel")!;
  const c = deckel.bounds!;
  const pt = await toScreen(page, [(c.min[0] + c.max[0]) / 2, (c.min[1] + c.max[1]) / 2, c.max[2]]);
  await page.mouse.move(pt.x, pt.y);
  await expect(page.getByTestId("status-hover-file")).toContainText(/Fläche aus gehaeuse\.scad, Zeile [67]$/);
  const hovered = Number((await page.getByTestId("status-hover-file").textContent())!.match(/Zeile (\d+)/)![1]);
  await screenshotD(page, "p06-hover");
  await page.mouse.click(pt.x, pt.y);
  await waitActive(page, "gehaeuse.scad");
  await page.waitForFunction((l) => (window as unknown as W).__cad.cursorLine() === l, hovered);
  // Double-click on the housing (an instance of a module of lib/teile.scad) opens its definition.
  const g = (await parts(page)).find((x) => x.name === "gehaeuse")!;
  const gb = g.bounds!;
  await waitScene(page, { context: null });
  const gp = await toScreen(page, [gb.max[0] - 1, gb.min[1], (gb.min[2] + gb.max[2]) / 2]);
  await page.mouse.dblclick(gp.x, gp.y);
  await waitActive(page, "lib/teile.scad");
  await page.waitForFunction(() => (window as unknown as W).__cad.cursorLine() === 15);
  const s = await waitScene(page, { context: "gehaeuse" });
  expect(s.path).toEqual(["gehaeuse"]);
  await screenshotD(page, "p06-doppelklick");
  recordD({
    id: "P06",
    title: "Hover und Klick über Dateigrenzen",
    images: [
      { name: "p06-hover", label: "konstanten.scad offen, Maus über dem Modell: „Fläche aus gehaeuse.scad, Zeile …“" },
      { name: "p06-doppelklick", label: "Doppelklick auf das Gehäuse: lib/teile.scad, Modul gehaeuse" },
    ],
    caption:
      "Liegt die Zeile einer Fläche in einer anderen Datei als der im Editor, nennt die Statuszeile Datei und Zeile; ein Klick öffnet die Datei an dieser Stelle. Der Doppelklick auf eine Instanz eines Moduls aus einer anderen Datei (gehaeuse aus lib/teile.scad) öffnet diese Datei in der Moduldefinition, der Kontext wird das Modul.",
    checks: ["Hover → status-hover-file mit Datei und Zeile", "Klick → gehaeuse.scad in der Zeile der Fläche", "Doppelklick → lib/teile.scad Zeile 15, Kontext gehaeuse"],
  });
});

test("P07 Bauteilliste: Sichtbarkeit und Export pro Bauteil (STL, 3MF, STEP, ZIP)", async ({ page }) => {
  await openApp(page);
  await openExampleProject(page);
  const ps = await parts(page);
  expect(ps.every((x) => (x.volume ?? 0) > 1000)).toBe(true);
  await expect(page.getByTestId("part-deckel")).toContainText("mm³");
  // Hide the lid: the model is drawn in pieces without it.
  const before = await layerPixels(page, "mode:");
  await page.getByTestId("part-visible-deckel").uncheck();
  let s = await waitView(page, "(s) => s.view.plan.base === null && s.view.plan.items.length > 0");
  expect(s.view!.plan.items.map((i) => i.label)).toEqual(["gehaeuse"]);
  expect((await parts(page)).find((x) => x.name === "deckel")!.hidden).toBe(true);
  await screenshotD(page, "p07-ausgeblendet", "view");
  await page.getByTestId("part-visible-deckel").check();
  s = await waitView(page, "(s) => s.view.plan.base !== null");
  expect(s.view!.plan.base).not.toBeNull();
  expect(before.found).toBeGreaterThan(0);

  // STL of one part (model coordinates: the lid lies at y >= 50).
  const stl = await downloaded(page, () => page.getByTestId("part-stl-deckel").click());
  expect(stl.name).toBe("deckel.stl");
  const n = stl.bytes.readUInt32LE(80);
  expect(stl.bytes.length).toBe(84 + 50 * n);
  let minY = Infinity;
  for (let t = 0; t < n; t++) for (let k = 0; k < 3; k++) minY = Math.min(minY, stl.bytes.readFloatLE(84 + 50 * t + 12 + 12 * k + 4));
  expect(minY).toBeCloseTo(50, 3);
  // All parts: ZIP with one STL each; one 3MF with an object per part.
  const zip = await downloaded(page, () => page.getByTestId("parts-zip").click());
  expect(zip.name).toBe("gehaeuse-teile-stl.zip");
  expect(zipNames(zip.bytes)).toEqual(["gehaeuse.stl", "deckel.stl"]);
  const mf = await downloaded(page, () => page.getByTestId("parts-3mf").click());
  expect(mf.name).toBe("gehaeuse-teile.3mf");
  const xml = mf.bytes.toString("latin1");
  // Multicolour by default (report 11): one build item and one object per part (components
  // over one mesh per colour), names in the model and in Metadata/model_settings.config.
  expect(xml.match(/<item objectid=/g)?.length).toBe(2);
  expect(xml).toContain('name="deckel"');
  expect(xml).toContain("Metadata/model_settings.config");
  // STEP of one part: exact B-rep through the same writer as the whole model.
  const st = await downloaded(page, () => page.getByTestId("part-step-deckel").click());
  expect(st.name).toBe("deckel.step");
  const stepText = st.bytes.toString("utf8");
  expect(stepText.startsWith("ISO-10303-21;")).toBe(true);
  expect(stepText.match(/MANIFOLD_SOLID_BREP\(/g)?.length).toBe(1);
  expect(stepText).toContain("PRODUCT('deckel','deckel'");
  // All parts as STEP files in a ZIP, and the whole model as STEP (toolbar).
  const sz = await downloaded(page, () => page.getByTestId("parts-zip-step").click());
  expect(sz.name).toBe("gehaeuse-teile-step.zip");
  expect(zipNames(sz.bytes)).toEqual(["gehaeuse.step", "deckel.step"]);
  const whole = await downloaded(page, () => page.getByRole("button", { name: "Export STEP" }).click());
  expect(whole.name).toBe("gehaeuse.step");
  const solids = whole.bytes.toString("utf8").match(/MANIFOLD_SOLID_BREP\(/g)?.length ?? 0;
  expect(solids).toBeGreaterThanOrEqual(2);
  await expect(page.getByTestId("status-notice")).toContainText("Exportiert: gehaeuse.step");
  const stepNotice = `deckel.step ${st.bytes.length} Bytes (1 Körper), Modell gehaeuse.step ${whole.bytes.length} Bytes (${solids} Körper)`;
  await screenshotD(page, "p07-bauteile", "view");
  recordD({
    id: "P07",
    title: "Bauteilliste und Export pro Bauteil",
    images: [
      { name: "p07-ausgeblendet", label: "Deckel ausgeblendet: nur das Gehäuse wird gezeichnet" },
      { name: "p07-bauteile", label: "Bauteilleiste mit Volumen und Export-Knöpfen" },
    ],
    caption: `Die Bauteilleiste listet die Bauteile mit Volumen (Hülle als Tooltip). Ohne Haken wird das Bauteil nicht gezeichnet (das Modell wird aus den übrigen Teilbäumen zusammengesetzt). Export: deckel.stl (${n} Dreiecke, Modellkoordinaten), alle Bauteile als ZIP mit einer STL je Bauteil, ein 3MF mit einem Objekt je Bauteil. STEP (exakte B-Rep, AP242, derselbe Writer wie für das ganze Modell): ${stepNotice}; alle Bauteile als ZIP mit einer STEP-Datei je Bauteil.`,
    checks: ["Haken aus → Plan ohne Basis, nur gehaeuse", "deckel.stl gültig, min y = 50", "ZIP: gehaeuse.stl, deckel.stl", "3MF: 2 Objekte mit Namen", "deckel.step: ein Körper, Produkt deckel", "ZIP (STEP): gehaeuse.step, deckel.step", "Export STEP des ganzen Modells"],

  });
});

test("P08 k[i]: Anker der Instanzen in der Schleife sichtbar, Snap schreibt k[2].oben", async ({ page }) => {
  await openApp(page);
  const src = `module halter(h = 8) {
    cylinder(h = h, r = 2);
    anchor oben = [0, 0, h], dir = [0, 0, 1];
}
for (i = [0:3]) translate([i * 15, 0, 0]) k[i]: halter(h = 6 + 2 * i);
translate([0, 20, 0]) sphere(2);
`;
  await loadText(page, src);
  // Cursor outside the module: file context (parts exist only there).
  await clickLine(page, 6, 2);
  await waitScene(page, { context: null });
  const ps = await parts(page);
  expect(ps.map((x) => x.name)).toEqual(["k[0]", "k[1]", "k[2]", "k[3]"]);
  const s = await waitView(page, "(s) => s.view.selection.line === 6 && s.view.plan.anchors >= 4");
  expect(s.view!.plan.anchors).toBe(4);
  const anchors = await page.evaluate(() => ((window as unknown as W).__cad.store.getState().shown as { anchors: Array<{ text: string | null; point: number[] }> }).anchors);
  expect(anchors.map((a) => a.text)).toEqual(["k[0].oben", "k[1].oben", "k[2].oben", "k[3].oben"]);
  const k2 = anchors.find((a) => a.text === "k[2].oben")!;
  expect(k2.point).toEqual([30, 0, 10]);
  const [h] = await waitHandles(page);
  await screenshotD(page, "p08-anker", "view");
  const from = grip(h);
  const to = await toScreen(page, k2.point);
  await mouseDrag(page, from, line(from, to, 16), {
    mid: async () => {
      await page.waitForFunction(() => (window as unknown as { __cad: { drag(): { snap: string | null } | null } }).__cad.drag()?.snap === "k[2].oben");
    },
  });
  await waitDragDone(page);
  const after = await text(page);
  expect(after.split("\n")[5]).toBe("translate(k[2].oben) sphere(2);");
  const nb = await hook(page, (c) => c.nodeBounds(6));
  expect(nb[0].min.map((x) => Math.round(x))).toEqual([28, -2, 8]);
  await screenshotD(page, "p08-snap", "view");
  recordD({
    id: "P08",
    title: "Benannte Instanzen in der Schleife",
    images: [
      { name: "p08-anker", label: "Cursor auf translate: vier Anker k[0].oben … k[3].oben als Rauten" },
      { name: "p08-snap", label: "Griff auf k[2].oben losgelassen" },
    ],
    code: after.trim(),
    caption: "for (i = [0:3]) … k[i]: halter(…) benennt vier Instanzen; ihre Anker sind wie bei benannten Instanzen sichtbar (Rauten) und Snap-Ziele, auf oberster Ebene sind k[0] … k[3] Bauteile. Der translate-Griff auf k[2].oben losgelassen schreibt „translate(k[2].oben)“; die Kugel sitzt bei [30, 0, 10].",
    checks: ["Bauteile k[0] … k[3]", "4 Anker mit Text k[i].oben", "Snap-Text k[2].oben während des Ziehens", "Text translate(k[2].oben) sphere(2);"],
  });
});

test("P09 Fehler in eingebundenen Dateien: fehlende Datei an der Zeile, Syntaxfehler in der Bibliothek", async ({ page }) => {
  await openApp(page);
  await page.evaluate(() =>
    (window as unknown as W).__cad.openProject({
      name: "fehler",
      main: "main.scad",
      files: { "main.scad": "use <lib.scad>\ninclude <fehlt.scad>\nteil();\n", "lib.scad": "module teil() cube(5);\n" },
    }),
  );
  let s = await waitScene(page, { context: null });
  const missing = s.diagnostics.find((d) => d.message.includes("fehlt.scad"))!;
  expect(missing.severity).toBe("error");
  expect(missing.line).toBe(2);
  await expect(lineEl(page, 2)).toHaveClass(/cm-l-error/);
  await expect(page.getByTestId("tab-main.scad").locator(".dot")).toBeVisible();
  // A syntax error in lib.scad: the diagnostic belongs to lib.scad (tab marked), the last
  // scene stays.
  await page.getByTestId("tab-lib.scad").click();
  await waitActive(page, "lib.scad");
  await clickLine(page, 1, 0);
  await page.keyboard.press("End");
  await page.keyboard.type(" sphere(;");
  await page.waitForFunction(() => ((window as unknown as W).__cad.store.getState().diagnostics as { items: Array<{ source: string; file?: string }> }).items.some((d) => d.source === "syntax" && d.file === "lib.scad"));
  await expect(page.getByTestId("tab-lib.scad").locator(".dot")).toBeVisible();
  await expect(lineEl(page, 1)).toHaveClass(/cm-l-error/);
  await screenshotD(page, "p09-fehler");
  s = await cad(page);
  expect(s.shownVersion).not.toBeNull();
  recordD({
    id: "P09",
    title: "Fehler über Dateigrenzen",
    images: [{ name: "p09-fehler", label: "Syntaxfehler in lib.scad: Zeile rot, Tab markiert; fehlende Datei in main.scad" }],
    code: "// main.scad\nuse <lib.scad>\ninclude <fehlt.scad>\nteil();",
    caption: "Eine fehlende Datei ist ein Fehler an der include-Zeile der einbindenden Datei. Diagnosen tragen ihre Datei: der Syntaxfehler in lib.scad erscheint dort (Zeile rot) und markiert den Tab; die Ansicht behält den letzten gültigen Stand.",
    checks: ["„Datei fehlt.scad nicht gefunden“ in main.scad Zeile 2", "Syntaxfehler mit file = lib.scad", "Tabs mit Fehlerpunkt"],
  });
});
