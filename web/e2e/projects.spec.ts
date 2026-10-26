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
  exportAs,
  fileMenu,
  fileTab,
  pickMenu,
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

const EX = "../tests/models/project-housing";
const exampleFiles = {
  "housing.mcad": readFileSync(`${EX}/housing.mcad`, "utf8"),
  "constants.mcad": readFileSync(`${EX}/constants.mcad`, "utf8"),
  "lib/parts.mcad": readFileSync(`${EX}/lib/parts.mcad`, "utf8"),
};

/** Opens the example project from the menu and waits for its complete scene. */
async function openExampleProject(page: Page): Promise<void> {
  await pickMenu(page, "Beispiele", "Projekte (mehrere Dateien)", "project-housing/");
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

test("P01 open the example project: files as tabs, main file, parts", async ({ page }) => {
  await openApp(page);
  await openExampleProject(page);
  const p = await info(page);
  expect(p.paths).toEqual(["housing.mcad", "constants.mcad", "lib/parts.mcad"]);
  expect(p.main).toBe("housing.mcad");
  expect(p.active).toBe("housing.mcad");
  await expect(fileTab(page, "housing.mcad")).toHaveClass(/active/);
  await expect(fileTab(page, "housing.mcad").locator(".star")).toBeVisible();
  await expect(page.getByRole("navigation", { name: "Pfadleiste" })).toContainText("housing.mcad");
  const s = await cad(page);
  expect(s.diagnostics.filter((d) => d.severity !== "info")).toEqual([]);
  const files = await page.evaluate(() => ((window as unknown as W).__cad.store.getState().complete as { files: Array<{ path: string; role: string }> }).files);
  expect(files.map((f) => `${f.path}:${f.role}`)).toEqual(["housing.mcad:main", "constants.mcad:include", "lib/parts.mcad:use"]);
  const ps = await parts(page);
  expect(ps.map((x) => x.name)).toEqual(["housing", "lid"]);
  expect(ps[1].bounds!.min[1]).toBeCloseTo(50, 5);
  await expect(page.getByRole("region", { name: "Bauteile" })).toContainText("housing");
  await expect(page.getByRole("region", { name: "Bauteile" })).toContainText("lid");
  // The model of both parts is drawn (root mesh).
  const b = (await hook(page, (c) => c.rootBounds()))!;
  expect(b.max[1]).toBeCloseTo(90, 3);
  await screenshotD(page, "p01-projekt");
  recordD({
    id: "P01",
    title: "Open the example project",
    images: [{ name: "p01-projekt", label: "project-housing/: three files as tabs, main file ★, parts bar" }],
    code: exampleFiles["housing.mcad"].trim(),
    caption: "The example menu opens tests/models/project-housing/ as a project: housing.mcad (main file, ★), constants.mcad (by include) and lib/parts.mcad (by use). The scene knows the three files with their role; the two named instances at the top level are the parts.",
    checks: [
      "Tabs housing.mcad (aktiv, ★), constants.mcad, lib/parts.mcad",
      "Scene.files: main, include, use; no errors or warnings",
      "parts housing and lid; hull of the lid from y = 50",
    ],
  });
});

test("P02 open a folder (File System Access API), upload and download a ZIP", async ({ page }) => {
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
    (window as unknown as { showDirectoryPicker: () => Promise<H> }).showDirectoryPicker = async () => dir("my-project", "");
  }, { ...exampleFiles, "notes.txt": "not scad" });
  await openApp(page);
  await fileMenu(page, "Ordner öffnen …");
  await page.waitForFunction(() => (window as unknown as W).__cad.projectInfo().name === "my-project");
  await waitScene(page, { context: null });
  let p = await info(page);
  expect(p.paths).toEqual(["housing.mcad", "constants.mcad", "lib/parts.mcad"]);
  expect(p.main).toBe("housing.mcad");
  expect(p.folder).toBe(true);
  await expect(page.getByTestId("status-notice")).toContainText("Projekt my-project: 3 Dateien, Hauptdatei housing.mcad");
  // Edit constants.mcad and save: every file is written back into the folder.
  await fileTab(page, "constants.mcad").click();
  await waitActive(page, "constants.mcad");
  await clickLine(page, 2, 9);
  await page.keyboard.press("End");
  await page.keyboard.type(" // wider");
  await page.keyboard.press("Control+s");
  await page.waitForFunction(() => Object.keys((window as unknown as { __written: Record<string, string> }).__written).length === 3);
  const written = await page.evaluate(() => (window as unknown as { __written: Record<string, string> }).__written);
  expect(written["constants.mcad"]).toContain("width = 60; // wider");
  expect(written["lib/parts.mcad"]).toBe(exampleFiles["lib/parts.mcad"]);
  await screenshotD(page, "p02-ordner");

  // ZIP download of the project, then upload of that ZIP.
  const z = await downloaded(page, () => fileMenu(page, "ZIP speichern"));
  expect(z.name).toBe("my-project.zip");
  expect(zipNames(z.bytes).sort()).toEqual(["my-project/constants.mcad", "my-project/housing.mcad", "my-project/lib/parts.mcad"]);
  await page.evaluate(() => (window as unknown as W).__cad.openProject({ name: "leer", main: "a.scad", files: { "a.scad": "cube(1);\n" } }));
  await waitScene(page);
  const [chooser] = await Promise.all([page.waitForEvent("filechooser"), fileMenu(page, "ZIP laden …")]);
  await chooser.setFiles({ name: "my-project.zip", mimeType: "application/zip", buffer: z.bytes });
  await page.waitForFunction(() => (window as unknown as W).__cad.projectInfo().paths.length === 3);
  await waitScene(page, { context: null });
  p = await info(page);
  expect(p.name).toBe("my-project");
  expect(p.main).toBe("housing.mcad");
  expect(p.files["constants.mcad"]).toContain("// wider");
  expect((await parts(page)).map((x) => x.name)).toEqual(["housing", "lid"]);
  recordD({
    id: "P02",
    title: "Open a folder, save, ZIP",
    images: [{ name: "p02-ordner", label: "folder “my-project” opened, constants.mcad changed and saved" }],
    caption:
      "“Datei › Ordner öffnen …” (file › open folder) reads all .scad files of a folder (File System Access API; in the test a stub folder with a .txt file that is ignored) and chooses the main file: the top-level file that no other file includes. Ctrl+S writes all files back into the folder. “Datei › ZIP speichern” (save ZIP) downloads the project as a ZIP, “Datei › ZIP laden …” (load ZIP) uploads this ZIP again (the common folder is removed, main file housing.mcad again).",
    checks: ["3 files, main file housing.mcad, notes.txt ignored", "Ctrl+S writes 3 files (constants.mcad with the change)", "ZIP with 3 entries; after the upload the same files and parts"],
  });
});

test("P03 switch files: tabs, context from the library, undo per file", async ({ page }) => {
  await openApp(page);
  await openExampleProject(page);
  await fileTab(page, "lib/parts.mcad").click();
  await waitActive(page, "lib/parts.mcad");
  await expect(lineEl(page, 14)).toHaveText("module housing(w = 60, d = 40, h = 25, t = 2) {");
  // Cursor in module housing of the library: the module is the context.
  await clickLine(page, 21, 10);
  let s = await waitScene(page, { context: "housing" });
  expect(s.path).toEqual(["housing"]);
  await expect(page.getByRole("navigation", { name: "Pfadleiste" })).toContainText("housing");
  await waitView(page, "(s) => s.view.selection.line === 21 && s.view.plan.items.some(i => i.mode === 'full')");
  await screenshotD(page, "p03-bibliothek");
  // Edit in the library, switch away and back: text and undo history stay per file.
  await clickLine(page, 3, 0);
  await page.keyboard.press("End");
  await page.keyboard.type(" // mm");
  await waitText(page, "(t) => t.includes('clearance = 0.2; // mm')");
  await fileTab(page, "housing.mcad").click();
  await waitActive(page, "housing.mcad");
  expect(await text(page)).toBe(exampleFiles["housing.mcad"]);
  s = await waitScene(page, { context: null });
  // Undo in the main file does not touch the library.
  await hook(page, (c) => c.focusEditor());
  await page.keyboard.press("Control+z");
  expect(await text(page)).toBe(exampleFiles["housing.mcad"]);
  await fileTab(page, "lib/parts.mcad").click();
  await waitActive(page, "lib/parts.mcad");
  expect(await text(page)).toContain("clearance = 0.2; // mm");
  await hook(page, (c) => c.focusEditor());
  await page.keyboard.press("Control+z");
  await waitText(page, `(t) => t === ${JSON.stringify(exampleFiles["lib/parts.mcad"])}`);
  recordD({
    id: "P03",
    title: "Switch files",
    images: [{ name: "p03-bibliothek", label: "lib/parts.mcad in the editor, cursor in the module housing (context)" }],
    code: exampleFiles["lib/parts.mcad"].split("\n").slice(13, 23).join("\n"),
    caption:
      "A click on the tab lib/parts.mcad shows the library; the cursor in the module housing makes it the context (path bar “Datei › housing”), grids and handles belong to the library. Every file keeps its text and its own undo history: Ctrl+Z in the main file does not change the library, Ctrl+Z in the library takes back the change there.",
    checks: ["context housing from lib/parts.mcad", "the change survives the switch", "undo per file"],
  });
});

test("P04 Ctrl+click on an include/use path opens the file", async ({ page }) => {
  await openApp(page);
  await openExampleProject(page);
  const at = async (l: number, col: number) => {
    const p = await page.evaluate(([a, b]) => (window as unknown as { __cad: { coordsOf(l: number, c: number): { x: number; y: number } | null } }).__cad.coordsOf(a, b), [l, col] as const);
    if (!p) throw new Error("no coords");
    return p;
  };
  // Line 3: include <constants.mcad>
  const p3 = await at(3, 14);
  await page.mouse.move(p3.x, p3.y);
  await expect(lineEl(page, 3).locator(".cm-include").first()).toBeVisible();
  await page.keyboard.down("Control");
  await page.mouse.click(p3.x, p3.y);
  await page.keyboard.up("Control");
  await waitActive(page, "constants.mcad");
  expect((await text(page)).split("\n")[1]).toBe("width = 60;");
  await expect(lineEl(page, 2).locator(".cm-lineend")).toContainText("= 60");
  await screenshotD(page, "p04-include", "editor");
  // Back, then Ctrl+click on use <lib/parts.mcad> (line 4).
  await fileTab(page, "housing.mcad").click();
  await waitActive(page, "housing.mcad");
  const p4 = await at(4, 8);
  await page.keyboard.down("Control");
  await page.mouse.click(p4.x, p4.y);
  await page.keyboard.up("Control");
  await waitActive(page, "lib/parts.mcad");
  // A plain click places the cursor only.
  await fileTab(page, "housing.mcad").click();
  await waitActive(page, "housing.mcad");
  const q = await at(3, 14);
  await page.mouse.click(q.x, q.y);
  await frame(page);
  expect((await info(page)).active).toBe("housing.mcad");
  recordD({
    id: "P04",
    title: "Ctrl+click on include",
    images: [{ name: "p04-include", label: "after Ctrl+click on <constants.mcad>: the file is open, values at the line end" }],
    code: "include <constants.mcad>\nuse <lib/parts.mcad>",
    caption: "The path in include <…> and use <…> is underlined (title “Strg+Klick: Datei öffnen”, Ctrl+click: open file). Ctrl+click opens the file relative to the including file; a plain click only sets the cursor. In constants.mcad the values of the variables appear at the line end because they hold in the file evaluation by include.",
    checks: ["Ctrl+click on <constants.mcad> → constants.mcad active", "Ctrl+click on <lib/parts.mcad> → lib/parts.mcad active", "a plain click does not switch"],
  });
});

test("P05 a handle changes the included file (variable from include, literal in the library)", async ({ page }) => {
  await openApp(page);
  const main = "include <dims.scad>\ncube([width, 10, 2]);\n";
  const dims = "// Dimensions\nwidth = 40;\n";
  await page.evaluate(([m, k]) => (window as unknown as W).__cad.openProject({ name: "dims", main: "main.scad", files: { "main.scad": m, "dims.scad": k } }), [main, dims] as const);
  await waitScene(page, { context: null });
  await clickLine(page, 2, 0);
  const [h] = await waitHandles(page);
  expect(h.kind).toBe("point");
  await screenshotD(page, "p05-vorher");
  const from = grip(h);
  // Free drag (Alt) locked to x (key X): the literal 40 in dims.scad changes, main.scad stays.
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
  expect(p.files["dims.scad"]).toMatch(/^\/\/ Dimensions\nwidth = \d+;\n$/);
  const newW = Number(p.files["dims.scad"].match(/width = (\d+)/)![1]);
  expect(newW).not.toBe(40);
  const b = (await hook(page, (c) => c.rootBounds()))!;
  expect(Math.round(b.max[0])).toBe(newW);
  await screenshotD(page, "p05-nachher");
  // The drag is one undo step in dims.scad.
  await fileTab(page, "dims.scad").click();
  await waitActive(page, "dims.scad");
  expect(await text(page)).toBe(`// Dimensions\nwidth = ${newW};\n`);
  await hook(page, (c) => c.focusEditor());
  await page.keyboard.press("Control+z");
  await waitText(page, `(t) => t === ${JSON.stringify(dims)}`);
  await waitScene(page);
  expect((await hook(page, (c) => c.rootBounds()))!.max[0]).toBeCloseTo(40, 6);

  // Literal in the used library: radius handle of the cylinder in module housing.
  await openExampleProject(page);
  await fileTab(page, "lib/parts.mcad").click();
  await waitActive(page, "lib/parts.mcad");
  await clickLine(page, 28, 40);
  await waitScene(page, { context: "housing" });
  const hs = await waitHandles(page, "(hs) => hs.some(h => h.kind === 'radius')");
  const r = hs.find((x) => x.kind === "radius")!;
  const rp = grip(r);
  await mouseDrag(page, rp, line(rp, { x: rp.x + 60, y: rp.y }, 10), { alt: true });
  await waitDragDone(page);
  p = await info(page);
  const lib = await text(page);
  expect(lib).not.toBe(exampleFiles["lib/parts.mcad"]);
  expect(lib.split("\n")[27]).toMatch(/cylinder\(h = 3, r = \d+\);/);
  expect(p.files["housing.mcad"]).toBe(exampleFiles["housing.mcad"]);
  await screenshotD(page, "p05-bibliothek");
  recordD({
    id: "P05",
    title: "A handle changes the included file",
    images: [
      { name: "p05-vorher", label: "main.scad, cursor on cube([width, 10, 2]): corner handle" },
      { name: "p05-nachher", label: `after the drag: dims.scad “width = ${newW};”, main.scad unchanged` },
    ],
    code: `// main.scad\n${main}// dims.scad\n${dims}`,
    caption: `The corner handle of the box goes through the variable width to the literal in dims.scad. Dragging wrote “width = ${newW};” into dims.scad (not into the editor of the main file); the scene followed. The drag is one undo step in dims.scad. Likewise the radius handle in the module housing of lib/parts.mcad changes the literal r = 2 on line 28 of the library, housing.mcad stays the same.`,
    checks: ["edit with file = dims.scad, main file unchanged", "hull = new value", "Ctrl+Z in dims.scad → width = 40", "radius handle in lib/parts.mcad writes into lib/parts.mcad"],
  });
});

test("P06 hover and click across file boundaries, a double click opens the library's module", async ({ page }) => {
  await openApp(page);
  await openExampleProject(page);
  // In constants.mcad (no geometry): faces of the model come from housing.mcad.
  await fileTab(page, "constants.mcad").click();
  await waitActive(page, "constants.mcad");
  await waitScene(page, { context: null });
  const lid = (await parts(page)).find((x) => x.name === "lid")!;
  const c = lid.bounds!;
  const pt = await toScreen(page, [(c.min[0] + c.max[0]) / 2, (c.min[1] + c.max[1]) / 2, c.max[2]]);
  await page.mouse.move(pt.x, pt.y);
  await expect(page.getByTestId("status-hover-file")).toContainText(/Fläche aus housing\.mcad, Zeile [67]$/);
  const hovered = Number((await page.getByTestId("status-hover-file").textContent())!.match(/Zeile (\d+)/)![1]);
  await screenshotD(page, "p06-hover");
  await page.mouse.click(pt.x, pt.y);
  await waitActive(page, "housing.mcad");
  await page.waitForFunction((l) => (window as unknown as W).__cad.cursorLine() === l, hovered);
  // Double-click on the housing (an instance of a module of lib/parts.mcad) opens its definition.
  const g = (await parts(page)).find((x) => x.name === "housing")!;
  const gb = g.bounds!;
  await waitScene(page, { context: null });
  const gp = await toScreen(page, [gb.max[0] - 1, gb.min[1], (gb.min[2] + gb.max[2]) / 2]);
  await page.mouse.dblclick(gp.x, gp.y);
  await waitActive(page, "lib/parts.mcad");
  await page.waitForFunction(() => (window as unknown as W).__cad.cursorLine() === 15);
  const s = await waitScene(page, { context: "housing" });
  expect(s.path).toEqual(["housing"]);
  await screenshotD(page, "p06-doppelklick");
  recordD({
    id: "P06",
    title: "Hover and click across file boundaries",
    images: [
      { name: "p06-hover", label: "constants.mcad open, mouse over the model: “Fläche aus housing.mcad, Zeile …” (face from …, line …)" },
      { name: "p06-doppelklick", label: "double click on the housing: lib/parts.mcad, module housing" },
    ],
    caption:
      "If the line of a face is in another file than the one in the editor, the status line names file and line; a click opens the file at this place. A double click on an instance of a module from another file (housing from lib/parts.mcad) opens this file at the module definition, the context becomes the module.",
    checks: ["hover → status-hover-file with file and line", "click → housing.mcad at the line of the face", "double click → lib/parts.mcad line 15, context housing"],
  });
});

test("P07 parts list: visibility and export per part (STL, 3MF, STEP, ZIP)", async ({ page }) => {
  await openApp(page);
  await openExampleProject(page);
  const ps = await parts(page);
  expect(ps.every((x) => (x.volume ?? 0) > 1000)).toBe(true);
  await expect(page.getByTestId("part-lid")).toContainText("mm³");
  // Hide the lid: the model is drawn in pieces without it.
  const before = await layerPixels(page, "mode:");
  await page.getByRole("checkbox", { name: "lid sichtbar" }).uncheck();
  let s = await waitView(page, "(s) => s.view.plan.base === null && s.view.plan.items.length > 0");
  expect(s.view!.plan.items.map((i) => i.label)).toEqual(["housing"]);
  expect((await parts(page)).find((x) => x.name === "lid")!.hidden).toBe(true);
  await screenshotD(page, "p07-ausgeblendet", "view");
  await page.getByRole("checkbox", { name: "lid sichtbar" }).check();
  s = await waitView(page, "(s) => s.view.plan.base !== null");
  expect(s.view!.plan.base).not.toBeNull();
  expect(before.found).toBeGreaterThan(0);

  // STL of one part (model coordinates: the lid lies at y >= 50).
  const stl = await downloaded(page, () => pickMenu(page, "Export lid", "STL"));
  expect(stl.name).toBe("lid.stl");
  const n = stl.bytes.readUInt32LE(80);
  expect(stl.bytes.length).toBe(84 + 50 * n);
  let minY = Infinity;
  for (let t = 0; t < n; t++) for (let k = 0; k < 3; k++) minY = Math.min(minY, stl.bytes.readFloatLE(84 + 50 * t + 12 + 12 * k + 4));
  expect(minY).toBeCloseTo(50, 3);
  // All parts: ZIP with one STL each; one 3MF with an object per part.
  const zip = await downloaded(page, () => pickMenu(page, "Export aller Bauteile", "ZIP (STL)"));
  expect(zip.name).toBe("housing-parts-stl.zip");
  expect(zipNames(zip.bytes)).toEqual(["housing.stl", "lid.stl"]);
  const mf = await downloaded(page, () => pickMenu(page, "Export aller Bauteile", "3MF (ein Objekt je Bauteil)"));
  expect(mf.name).toBe("housing-parts.3mf");
  const xml = mf.bytes.toString("latin1");
  // Multicolour by default (report 11): one build item and one object per part (components
  // over one mesh per colour), names in the model and in Metadata/model_settings.config.
  expect(xml.match(/<item objectid=/g)?.length).toBe(2);
  expect(xml).toContain('name="lid"');
  expect(xml).toContain("Metadata/model_settings.config");
  // STEP of one part: exact B-rep through the same writer as the whole model.
  const st = await downloaded(page, () => pickMenu(page, "Export lid", "STEP"));
  expect(st.name).toBe("lid.step");
  const stepText = st.bytes.toString("utf8");
  expect(stepText.startsWith("ISO-10303-21;")).toBe(true);
  expect(stepText.match(/MANIFOLD_SOLID_BREP\(/g)?.length).toBe(1);
  expect(stepText).toContain("PRODUCT('lid','lid'");
  // All parts as STEP files in a ZIP, and the whole model as STEP (toolbar).
  const sz = await downloaded(page, () => pickMenu(page, "Export aller Bauteile", "ZIP (STEP)"));
  expect(sz.name).toBe("housing-parts-step.zip");
  expect(zipNames(sz.bytes)).toEqual(["housing.step", "lid.step"]);
  const whole = await downloaded(page, () => exportAs(page, "STEP"));
  expect(whole.name).toBe("housing.step");
  const solids = whole.bytes.toString("utf8").match(/MANIFOLD_SOLID_BREP\(/g)?.length ?? 0;
  expect(solids).toBeGreaterThanOrEqual(2);
  await expect(page.getByTestId("status-notice")).toContainText("Exportiert: housing.step");
  const stepNotice = `lid.step ${st.bytes.length} bytes (1 body), model housing.step ${whole.bytes.length} bytes (${solids} bodies)`;
  await screenshotD(page, "p07-bauteile", "view");
  recordD({
    id: "P07",
    title: "Parts list and export per part",
    images: [
      { name: "p07-ausgeblendet", label: "lid hidden: only the housing is drawn" },
      { name: "p07-bauteile", label: "parts bar with volumes and export buttons" },
    ],
    caption: `The parts bar lists the parts with their volumes (hull as a tooltip). Without the tick the part is not drawn (the model is assembled from the other subtrees). Export: lid.stl (${n} triangles, model coordinates), all parts as a ZIP with one STL per part, a 3MF with one object per part. STEP (exact B-rep, AP242, the same writer as for the whole model): ${stepNotice}; all parts as a ZIP with one STEP file per part.`,
    checks: ["tick off → plan without base, only housing", "lid.stl valid, min y = 50", "ZIP: housing.stl, lid.stl", "3MF: 2 objects with names", "lid.step: one body, product lid", "ZIP (STEP): housing.step, lid.step", "export STEP of the whole model"],

  });
});

test("P08 k[i]: anchors of the instances in the loop visible, snap writes k[2].top", async ({ page }) => {
  await openApp(page);
  const src = `module holder(h = 8) {
    cylinder(h = h, r = 2);
    anchor top = [0, 0, h], dir = [0, 0, 1];
}
for (i = [0:3]) translate([i * 15, 0, 0]) k[i]: holder(h = 6 + 2 * i);
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
  expect(anchors.map((a) => a.text)).toEqual(["k[0].top", "k[1].top", "k[2].top", "k[3].top"]);
  const k2 = anchors.find((a) => a.text === "k[2].top")!;
  expect(k2.point).toEqual([30, 0, 10]);
  const [h] = await waitHandles(page);
  await screenshotD(page, "p08-anker", "view");
  const from = grip(h);
  const to = await toScreen(page, k2.point);
  await mouseDrag(page, from, line(from, to, 16), {
    mid: async () => {
      await page.waitForFunction(() => (window as unknown as { __cad: { drag(): { snap: string | null } | null } }).__cad.drag()?.snap === "k[2].top");
    },
  });
  await waitDragDone(page);
  const after = await text(page);
  expect(after.split("\n")[5]).toBe("translate(k[2].top) sphere(2);");
  const nb = await hook(page, (c) => c.nodeBounds(6));
  expect(nb[0].min.map((x) => Math.round(x))).toEqual([28, -2, 8]);
  await screenshotD(page, "p08-snap", "view");
  recordD({
    id: "P08",
    title: "Named instances in a loop",
    images: [
      { name: "p08-anker", label: "cursor on translate: four anchors k[0].top … k[3].top as diamonds" },
      { name: "p08-snap", label: "handle released on k[2].top" },
    ],
    code: after.trim(),
    caption: "for (i = [0:3]) … k[i]: holder(…) names four instances; their anchors are visible like those of named instances (diamonds) and are snap targets, at the top level k[0] … k[3] are parts. The translate handle released on k[2].top writes “translate(k[2].top)”; the sphere sits at [30, 0, 10].",
    checks: ["parts k[0] … k[3]", "4 anchors with text k[i].top", "snap text k[2].top while dragging", "text translate(k[2].top) sphere(2);"],
  });
});

test("P09 errors in included files: missing file at its line, syntax error in the library", async ({ page }) => {
  await openApp(page);
  await page.evaluate(() =>
    (window as unknown as W).__cad.openProject({
      name: "errors",
      main: "main.scad",
      files: { "main.scad": "use <lib.scad>\ninclude <missing.scad>\npart();\n", "lib.scad": "module part() cube(5);\n" },
    }),
  );
  let s = await waitScene(page, { context: null });
  const missing = s.diagnostics.find((d) => d.message.includes("missing.scad"))!;
  expect(missing.severity).toBe("error");
  expect(missing.line).toBe(2);
  await expect(lineEl(page, 2)).toHaveClass(/cm-l-error/);
  await expect(fileTab(page, "main.scad").locator(".dot")).toBeVisible();
  // A syntax error in lib.scad: the diagnostic belongs to lib.scad (tab marked), the last
  // scene stays.
  await fileTab(page, "lib.scad").click();
  await waitActive(page, "lib.scad");
  await clickLine(page, 1, 0);
  await page.keyboard.press("End");
  await page.keyboard.type(" sphere(;");
  await page.waitForFunction(() => ((window as unknown as W).__cad.store.getState().diagnostics as { items: Array<{ source: string; file?: string }> }).items.some((d) => d.source === "syntax" && d.file === "lib.scad"));
  await expect(fileTab(page, "lib.scad").locator(".dot")).toBeVisible();
  await expect(lineEl(page, 1)).toHaveClass(/cm-l-error/);
  await screenshotD(page, "p09-fehler");
  s = await cad(page);
  expect(s.shownVersion).not.toBeNull();
  recordD({
    id: "P09",
    title: "Errors across file boundaries",
    images: [{ name: "p09-fehler", label: "syntax error in lib.scad: line red, tab marked; missing file in main.scad" }],
    code: "// main.scad\nuse <lib.scad>\ninclude <missing.scad>\npart();",
    caption: "A missing file is an error at the include line of the including file. Diagnostics carry their file: the syntax error in lib.scad appears there (line red) and marks the tab; the view keeps the last valid state.",
    checks: ["“Datei missing.scad nicht gefunden” (file not found) in main.scad line 2", "syntax error with file = lib.scad", "tabs with an error dot"],
  });
});
