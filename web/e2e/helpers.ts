import { mkdirSync, writeFileSync, readFileSync, existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { type Page, expect } from "@playwright/test";

const here = dirname(fileURLToPath(import.meta.url));
/** Screenshots and measurements for test report 2 (tools/report/report2-ui.mjs). */
export const REPORT_DIR = join(here, "../../reports/tmp/report2");

export interface CadState {
  fileName: string;
  docVersion: number;
  sentVersion: number;
  pendingSend: boolean;
  sceneVersion: number | null;
  completeVersion: number | null;
  shownVersion: number | null;
  shownComplete: boolean;
  shownContext: string | null;
  context: string | null;
  path: string[];
  diagnostics: Array<{ severity: string; line: number; message: string; source: string }>;
  diagnosticsVersion: number;
  engineError: string | null;
  cursor: number;
  viewHover: { lines: number[]; nodes: number[] } | null;
  allGrids: boolean;
  theme: string;
  themePref: string;
  colorsOpen: boolean;
  showRegions: boolean;
  regions: { version: number; meshes: boolean; items: Array<{ part: string | null; total: number; warning: string | null; regions: Array<{ color: string; filament: number; volume: number }> }> } | null;
  colors: Array<{ key: string; label: string; source: string; filament: number; explicit: boolean; lines: number[] }>;
  metrics: {
    firstSceneMs: number | null;
    editToScene: number[];
    evalMs: number[];
    dragToScene: Array<{ ms: number; coarse: boolean }>;
    dragEndToFine: number[];
    dragFps: Array<{ fps: number; frames: number; ms: number; maxGapMs: number; sent: number; dropped: number }>;
  };
  notice: string | null;
  meshes: { count: number; bytes: number };
  view: {
    regionsDrawn: number;
    faceColors: number;
    sceneVersion: number | null;
    complete: boolean;
    context: string | null;
    selection: { nodes: number[]; stmt: number | null; line: number; refs: number[]; grids: number[] };
    hoverNodes: number[];
    plan: {
      base: string | null;
      items: Array<{ node: number; mode: string; label: string; line: number; flat: boolean; why: string }>;
      edges: number;
      frames: number;
      anchors: number;
      activeAnchors: number;
      status: string | null;
      guides: Record<string, number>;
      guidePoints: Array<{ kind: string; points: Array<[number, number, number]> }>;
      sketches?: Array<{ node: number; closed: boolean; segments: number; active: Array<{ stmt: number; kind: string; line: number }>; errors: number; helpers: number; open: number }>;
    };
    grid: { lines: number; dashed: number; points: number; highlight: string[] };
    missingMeshes: number;
    objects: number;
    edgeKinds: Record<string, number>;
    zoneEdges: { zone: number; inZone: number; total: number } | null;
    openFaces: Array<{ shell: number; faces: number; engine: number }>;
    camera: { autoFit: boolean; refitHint: boolean; distance: number; target: [number, number, number] };
  } | null;
  frames: number;
  restarts: number;
  /** Cache keys of nodes that hung (watchdog), sent to every new worker. */
  blockedKeys: string[];
}

export async function cad(page: Page): Promise<CadState> {
  return page.evaluate(() => (window as unknown as { __cad: { state(): CadState } }).__cad.state());
}

/** Waits until the view shows the complete scene of the current document and all meshes. */
export async function waitScene(page: Page, opts: { context?: string | null; minVersion?: number } = {}): Promise<CadState> {
  await withState(page, () => page.waitForFunction(
    ([ctx, minV]) => {
      const c = (window as unknown as { __cad?: { state(): CadState } }).__cad;
      if (!c) return false;
      const s = c.state();
      if (!s.view || s.shownVersion === null || !s.shownComplete) return false;
      if (s.pendingSend || s.shownVersion !== s.sentVersion) return false;
      if (minV !== null && s.shownVersion < (minV as number)) return false;
      if (ctx !== "__any" && s.shownContext !== ctx) return false;
      if (s.view.missingMeshes > 0) return false;
      return true;
    },
    [opts.context === undefined ? "__any" : opts.context, opts.minVersion ?? null] as const,
    { timeout: 60_000, polling: 50 },
  ));
  // One more frame so the drawn state matches.
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
  return cad(page);
}

/** Waits until the view state satisfies a predicate (evaluated in the page). */
export async function waitView(page: Page, fn: string, arg: unknown = null, opts: { meshes?: boolean } = {}): Promise<CadState> {
  const needMeshes = opts.meshes ?? true;
  await withState(page, () => page.waitForFunction(
    ([src, a, need]) => {
      const c = (window as unknown as { __cad?: { state(): CadState } }).__cad;
      if (!c) return false;
      const s = c.state();
      if (!s.view || (need && s.view.missingMeshes > 0)) return false;
      // eslint-disable-next-line no-new-func
      return new Function("s", "arg", `return (${src})(s, arg);`)(s, a);
    },
    [fn, arg, needMeshes] as const,
    { timeout: 30_000, polling: 50 },
  ));
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
  return cad(page);
}

/** Runs a wait; on failure the error carries the UI state (diagnosis without retries). */
async function withState<T>(page: Page, f: () => Promise<T>): Promise<T> {
  try {
    return await f();
  } catch (e) {
    const s = await cad(page).catch(() => null);
    const v = s?.view;
    const brief = s && { ...s, view: v && { ...v, plan: { ...v.plan, items: v.plan.items.slice(0, 12) } } };
    throw new Error(`${(e as Error).message}\nZustand: ${JSON.stringify(brief)}`);
  }
}

export async function openApp(page: Page): Promise<CadState> {
  await page.goto("/");
  await expect(page.getByTestId("editor").locator(".cm-content")).toBeVisible();
  return waitScene(page);
}

// ---------------------------------------------------------------- UI by roles and names
// (UI overhaul, report 20: the tests find controls like a user – by role and accessible name –
// not by test ids; test ids remain only on displays: status texts, panels, labels in the view.)

/** A menu entry of the open menus (menuitem, menuitemradio or menuitemcheckbox) by its name. */
export function menuItem(page: Page, name: string | RegExp) {
  const o = typeof name === "string" ? { name, exact: true } : { name };
  return page.getByRole("menuitem", o).or(page.getByRole("menuitemradio", o)).or(page.getByRole("menuitemcheckbox", o));
}

/** Opens the menu of a button and chooses the entries in order (submenus first). */
export async function pickMenu(page: Page, button: string, ...items: Array<string | RegExp>): Promise<void> {
  await page.getByRole("button", { name: button, exact: true }).click();
  for (const it of items) await menuItem(page, it).click();
}

/** Mode of the 3D view: "Modell", "Schnitt", "Skizze", "Messen". */
export function mode(page: Page, name: "Modell" | "Schnitt" | "Skizze" | "Messen") {
  return page.getByRole("radiogroup", { name: "Modus" }).getByRole("radio", { name, exact: true });
}

/** Tab of the side panel: "Bauteile", "Farben", "Messen", "Parameter". */
export function sideTab(page: Page, name: "Bauteile" | "Farben" | "Messen" | "Parameter") {
  return page.getByRole("tab", { name, exact: true });
}

export async function setTheme(page: Page, t: "light" | "dark" | "system"): Promise<void> {
  await pickMenu(page, "Farbschema", { light: "Hell", dark: "Dunkel", system: "System" }[t]);
}

/** Standard view from the menu "Ansicht" (Iso, Oben, Vorne, Rechts, Hinten, Links, Unten). */
export async function standardView(page: Page, name: string): Promise<void> {
  await pickMenu(page, "Ansicht", name);
}

/** Export from the menu of the export button ("3MF mehrfarbig", "STL", "STEP", …). */
export async function exportAs(page: Page, name: string, ...sub: string[]): Promise<void> {
  await pickMenu(page, "Exportformat", name, ...sub);
}

/** Entry of the menu "Datei". */
export async function fileMenu(page: Page, ...items: string[]): Promise<void> {
  await pickMenu(page, "Datei", ...items);
}

/** File tab of the project. */
export function fileTab(page: Page, path: string) {
  return page.getByRole("tablist", { name: "Dateien des Projekts" }).getByRole("tab", { name: path, exact: true });
}

/** Context menu of a file tab: "Als Hauptdatei", "Schließen". */
export async function tabAction(page: Page, path: string, action: "Als Hauptdatei" | "Schließen"): Promise<void> {
  await fileTab(page, path).click({ button: "right" });
  await menuItem(page, action).click();
}

const SKETCH_NAMES: Record<string, string> = {
  select: "Auswahl",
  line: "Linie",
  arc_through: "Bogen (3 Punkte)",
  arc_center: "Bogen (Mittelpunkt)",
  arc_tangent: "Bogen (tangential)",
  bezier: "Bézier",
  rect: "Rechteck",
  circle: "Kreis",
  corner: "Ecke runden",
};

/** Context bar of the sketch editor. */
export function sketchBar(page: Page) {
  return page.getByRole("toolbar", { name: "Skizze" });
}

/** Chooses a drawing tool in the context bar (arc kinds from the menu "Bogenart"). */
export async function sketchTool(page: Page, tool: string): Promise<void> {
  const name = SKETCH_NAMES[tool] ?? tool;
  if (tool.startsWith("arc_")) {
    await sketchBar(page).getByRole("button", { name: "Bogenart", exact: true }).click();
    await menuItem(page, name).click();
  } else await sketchBar(page).getByRole("button", { name, exact: true }).click();
}

/** Tool button of the sketch bar (pressed state). */
export function sketchToolButton(page: Page, tool: string) {
  return sketchBar(page).getByRole("button", { name: SKETCH_NAMES[tool] ?? tool, exact: true });
}

const MEASURE_NAMES: Record<string, string> = { distance: "Abstand", radius: "Radius", angle: "Winkel", length: "Länge", area: "Fläche", body: "Körper", wall: "Wandstärke" };

/** Context bar of the measure mode. */
export function measureBar(page: Page) {
  return page.getByRole("toolbar", { name: "Messen" });
}

/** Measure kind (radio of the group "Messart"). */
export function measureKind(page: Page, kind: string) {
  return page.getByRole("radiogroup", { name: "Messart" }).getByRole("radio", { name: MEASURE_NAMES[kind] ?? kind, exact: true });
}

/** Context bar of the section view. */
export function sectionBar(page: Page) {
  return page.getByRole("toolbar", { name: "Schnitt" });
}

const GROUPS: Record<string, string> = { spec: "Spec-Beispiele", "openscad-compat": "OpenSCAD-Kompatibilität" };

export async function openExample(page: Page, path: string, context: string | null = null): Promise<CadState> {
  const [group, ...rest] = path.split("/");
  await pickMenu(page, "Beispiele", GROUPS[group] ?? group, rest.join("/"));
  await page.waitForFunction((p) => (window as unknown as { __cad: { state(): { fileName: string } } }).__cad.state().fileName === p.split("/").pop(), path);
  return waitScene(page, { context });
}

/** Example project (folder below examples/) from the menu "Beispiele". */
export async function openExampleProject(page: Page, name: string): Promise<void> {
  await pickMenu(page, "Beispiele", "Projekte (mehrere Dateien)", `${name}/`);
}

/** Clicks into an editor line (1-based) at a column. */
export async function clickLine(page: Page, line: number, col = 0): Promise<void> {
  await page.evaluate(([l, c]) => (window as unknown as { __cad: { setCursorLine(l: number, c: number): void } }).__cad.setCursorLine(l, c), [line, col] as const);
}

/** Moves the mouse over a character of the editor (1-based line, 0-based column). */
export async function hoverText(page: Page, line: number, col: number): Promise<void> {
  const p = await page.evaluate(([l, c]) => (window as unknown as { __cad: { coordsOf(l: number, c: number): { x: number; y: number } | null } }).__cad.coordsOf(l, c), [line, col] as const);
  if (!p) throw new Error(`no coordinates for ${line}:${col}`);
  await page.mouse.move(p.x, p.y);
}

/** Editor line element (1-based; the example files fit into the viewport). */
export function lineEl(page: Page, line: number) {
  return page.locator(".cm-content .cm-line").nth(line - 1);
}

export function shot(name: string): string {
  mkdirSync(join(REPORT_DIR, "img"), { recursive: true });
  return join(REPORT_DIR, "img", `${name}.jpg`);
}

export async function screenshot(page: Page, name: string): Promise<void> {
  await page.screenshot({ path: shot(name), type: "jpeg", quality: 82 });
}

/** Records a scenario (title, result, caption) for the report. */
export function record(entry: { id: string; title: string; caption: string; image?: string; details?: string }): void {
  mkdirSync(REPORT_DIR, { recursive: true });
  const f = join(REPORT_DIR, "scenarios.json");
  const all: Array<Record<string, unknown>> = existsSync(f) ? JSON.parse(readFileSync(f, "utf8")) : [];
  const i = all.findIndex((e) => e.id === entry.id);
  if (i >= 0) all[i] = entry;
  else all.push(entry);
  writeFileSync(f, JSON.stringify(all, null, 2));
}

export function writeMetrics(m: Record<string, unknown>): void {
  mkdirSync(REPORT_DIR, { recursive: true });
  const f = join(REPORT_DIR, "metrics.json");
  const old = existsSync(f) ? JSON.parse(readFileSync(f, "utf8")) : {};
  writeFileSync(f, JSON.stringify({ ...old, ...m }, null, 2));
}

// ---------------------------------------------------------------- Phase B (test report 3)

/** Screenshots and measurements for test report 3 (tools/report/report3-ui.mjs). */
export const REPORT3_DIR = join(here, "../../reports/tmp/report3");

/**
 * Screenshot for report 3: by default only the view column (path bar, 3D view, parameter bar);
 * the code is shown as text next to it in the report. `top`: the first editor lines (inline
 * controls), `full`: the whole window.
 */
export async function screenshotB(page: Page, name: string, area: "view" | "top" | "full" = "view"): Promise<void> {
  mkdirSync(join(REPORT3_DIR, "img"), { recursive: true });
  const vp = page.viewportSize() ?? { width: 1440, height: 900 };
  const clip = area === "view" ? await page.locator("section.viewcol").boundingBox() : area === "top" ? { x: 0, y: 38, width: Math.min(vp.width, 664), height: 110 } : null;
  await page.screenshot({ path: join(REPORT3_DIR, "img", `${name}.jpg`), type: "jpeg", quality: 82, ...(clip ? { clip } : {}) });
}

export interface ScenarioB {
  id: string;
  title: string;
  caption: string;
  before?: string;
  after?: string;
  /** Source excerpt before and after the interaction. */
  codeBefore?: string;
  codeAfter?: string;
  details?: string;
}

export function recordB(entry: ScenarioB): void {
  mkdirSync(REPORT3_DIR, { recursive: true });
  const f = join(REPORT3_DIR, "scenarios.json");
  const all: ScenarioB[] = existsSync(f) ? JSON.parse(readFileSync(f, "utf8")) : [];
  const i = all.findIndex((e) => e.id === entry.id);
  if (i >= 0) all[i] = entry;
  else all.push(entry);
  writeFileSync(f, JSON.stringify(all, null, 2));
}

export function writeMetricsB(m: Record<string, unknown>): void {
  mkdirSync(REPORT3_DIR, { recursive: true });
  const f = join(REPORT3_DIR, "metrics.json");
  const old = existsSync(f) ? JSON.parse(readFileSync(f, "utf8")) : {};
  writeFileSync(f, JSON.stringify({ ...old, ...m }, null, 2));
}

export interface HandleView {
  id: number;
  kind: string;
  axis: number | null;
  index: number;
  style: string;
  line: number;
  value: number[];
  screen: { x: number; y: number } | null;
  label: string;
}

type Hook = {
  handles(): HandleView[];
  handlePoint(id: number, v: number): { x: number; y: number } | null;
  text(): string;
  drag(): { handle: number; snap: string | null; alt: boolean; edits: number } | null;
  project(p: [number, number, number]): { x: number; y: number; visible: boolean } | null;
  rootBounds(): { min: number[]; max: number[] } | null;
  nodeBounds(line: number): Array<{ label: string; min: number[]; max: number[] }>;
  cursorLine(): number;
  setText(t: string): void;
  fitAll(): void;
  setView(v: string): void;
  focusEditor(): void;
};

export function hook<T>(page: Page, f: (c: Hook, arg: unknown) => T, arg: unknown = null): Promise<T> {
  return page.evaluate(
    ([src, a]) => {
      const c = (window as unknown as { __cad: Hook }).__cad;
      // eslint-disable-next-line no-new-func
      return new Function("c", "a", `return (${src})(c, a);`)(c, a);
    },
    [f.toString(), arg] as const,
  ) as Promise<T>;
}

/** Replaces the document, waits for its scene and frames it in the iso view. */
export async function loadText(page: Page, text: string): Promise<CadState> {
  const v0 = (await cad(page)).sentVersion;
  await hook(page, (c, t) => c.setText(t as string), text);
  await waitScene(page, { minVersion: v0 + 1 });
  await hook(page, (c) => c.setView("iso"));
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
  return cad(page);
}

/** Waits until the cursor line shows handles; returns them. */
export async function waitHandles(page: Page, pred: string = "(hs) => hs.length > 0"): Promise<HandleView[]> {
  await withState(page, () =>
    page.waitForFunction(
      (src) => {
        const c = (window as unknown as { __cad?: Hook }).__cad;
        if (!c) return false;
        // eslint-disable-next-line no-new-func
        return new Function("hs", `return (${src})(hs);`)(c.handles());
      },
      pred,
      { timeout: 30_000, polling: 50 },
    ),
  );
  // The handles of the previous cursor line stay drawn until the next frame rebuilds the
  // overlay, and a refit (viewport resized) can still move them: answer once two reads a few
  // frames apart agree and still satisfy the predicate.
  let hs = await hook(page, (c) => c.handles());
  for (let i = 0; i < 40; i++) {
    await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(() => requestAnimationFrame(r)))));
    const next = await hook(page, (c) => c.handles());
    // eslint-disable-next-line no-new-func
    const ok = (await page.evaluate(([src, h]) => new Function("hs", `return (${src})(hs);`)(h), [pred, next] as const)) as boolean;
    if (ok && JSON.stringify(next) === JSON.stringify(hs)) return next;
    hs = next;
  }
  return hs;
}

export async function frame(page: Page): Promise<void> {
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => r(null))));
}

export interface Pt {
  x: number;
  y: number;
}

/**
 * Real mouse drag on the canvas: press at `from`, move through `path` (one move per animation
 * frame, like a user), optionally run `mid` before releasing.
 */
export async function mouseDrag(page: Page, from: Pt, path: Pt[], opts: { alt?: boolean; mid?: () => Promise<void> } = {}): Promise<void> {
  await page.mouse.move(from.x, from.y);
  if (opts.alt) await page.keyboard.down("Alt");
  await page.mouse.down();
  for (const p of path) {
    await page.mouse.move(p.x, p.y);
    await frame(page);
  }
  if (opts.mid) await opts.mid();
  await page.mouse.up();
  if (opts.alt) await page.keyboard.up("Alt");
}

/** Points from a to b (excluding a). */
export function line(a: Pt, b: Pt, steps: number): Pt[] {
  return Array.from({ length: steps }, (_, i) => ({ x: a.x + ((b.x - a.x) * (i + 1)) / steps, y: a.y + ((b.y - a.y) * (i + 1)) / steps }));
}

/** Waits until no drag is active and the scene of the current document is drawn. */
export async function waitDragDone(page: Page): Promise<CadState> {
  await page.waitForFunction(() => (window as unknown as { __cad: Hook }).__cad.drag() === null, null, { timeout: 30_000, polling: 50 });
  return waitScene(page);
}

/** Lines `from`..`to` (1-based) of the editor text. */
export function lines(text: string, from: number, to = from): string {
  return text.split("\n").slice(from - 1, to).join("\n");
}

// ---------------------------------------------------------------- Phase C (test report 4)

/** Screenshots and measurements for test report 4 (tools/report/report4.mjs). */
export const REPORT4_DIR = join(here, "../../reports/tmp/report4");

/** Screenshot of the view column (default), the editor or the whole window for report 4. */
export async function screenshotC(page: Page, name: string, area: "view" | "full" | "editor" = "view"): Promise<void> {
  mkdirSync(join(REPORT4_DIR, "img"), { recursive: true });
  const clip = area === "view" ? await page.locator("section.viewcol").boundingBox() : area === "editor" ? await page.locator("section.editorpane").boundingBox() : null;
  await page.screenshot({ path: join(REPORT4_DIR, "img", `${name}.jpg`), type: "jpeg", quality: 82, ...(clip ? { clip } : {}) });
}

export interface ScenarioC {
  id: string;
  title: string;
  caption: string;
  /** Screenshots with a label each. */
  images: Array<{ name: string; label: string }>;
  /** Code excerpt shown next to the screenshots. */
  code?: string;
  /** Checked conditions (state or pixels). */
  checks?: string[];
}

export function recordC(entry: ScenarioC): void {
  mkdirSync(REPORT4_DIR, { recursive: true });
  const f = join(REPORT4_DIR, "scenarios.json");
  const all: ScenarioC[] = existsSync(f) ? JSON.parse(readFileSync(f, "utf8")) : [];
  const i = all.findIndex((e) => e.id === entry.id);
  if (i >= 0) all[i] = entry;
  else all.push(entry);
  writeFileSync(f, JSON.stringify(all, null, 2));
}

export function writeMetricsC(m: Record<string, unknown>): void {
  mkdirSync(REPORT4_DIR, { recursive: true });
  const f = join(REPORT4_DIR, "metrics.json");
  const old = existsSync(f) ? JSON.parse(readFileSync(f, "utf8")) : {};
  writeFileSync(f, JSON.stringify({ ...old, ...m }, null, 2));
}

/**
 * Pixels of the 3D view whose colour is within `tol` (max channel difference) of a CSS
 * variable's colour (e.g. `--view-selected-edge`), optionally only inside a client rectangle.
 */
export async function colorPixels(page: Page, cssVar: string, tol = 28, rect?: { x: number; y: number; width: number; height: number }): Promise<number> {
  const canvas = page.getByTestId("viewport-canvas");
  const box = (await canvas.boundingBox())!;
  const png = await canvas.screenshot();
  return page.evaluate(
    async ([b64, name, t, r, bx, by]) => {
      const col = getComputedStyle(document.documentElement).getPropertyValue(name as string).trim();
      const c0 = document.createElement("canvas").getContext("2d")!;
      c0.fillStyle = col;
      const hex = c0.fillStyle as string;
      const rgb = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16));
      const img = new Image();
      img.src = `data:image/png;base64,${b64}`;
      await img.decode();
      const c = document.createElement("canvas");
      c.width = img.width;
      c.height = img.height;
      const g = c.getContext("2d")!;
      g.drawImage(img, 0, 0);
      const rr = r as { x: number; y: number; width: number; height: number } | null;
      const x0 = rr ? Math.max(0, Math.floor(rr.x - (bx as number))) : 0;
      const y0 = rr ? Math.max(0, Math.floor(rr.y - (by as number))) : 0;
      const w = rr ? Math.min(c.width - x0, Math.ceil(rr.width)) : c.width;
      const h = rr ? Math.min(c.height - y0, Math.ceil(rr.height)) : c.height;
      if (w <= 0 || h <= 0) return 0;
      const d = g.getImageData(x0, y0, w, h).data;
      let n = 0;
      for (let i = 0; i < d.length; i += 4) {
        if (Math.abs(d[i] - rgb[0]) <= (t as number) && Math.abs(d[i + 1] - rgb[1]) <= (t as number) && Math.abs(d[i + 2] - rgb[2]) <= (t as number)) n++;
      }
      return n;
    },
    [png.toString("base64"), cssVar, tol, rect ?? null, box.x, box.y] as const,
  );
}

/**
 * Pixel check of one drawn layer: canvas pixels that change (max channel difference > `tol`)
 * when the objects whose name starts with `prefix` (`open-faces`, `guide:path`,
 * `edge:selected`, `mode:profile`, …) are hidden. `found`: number of such objects.
 */
export async function layerPixels(page: Page, prefix: string, tol = 24): Promise<{ found: number; changed: number }> {
  const canvas = page.getByTestId("viewport-canvas");
  type W = { __cad: { setLayerVisible(p: string, v: boolean): number } };
  const shown = await canvas.screenshot();
  const found = await page.evaluate((p) => (window as unknown as W).__cad.setLayerVisible(p, false), prefix);
  const hidden = await canvas.screenshot();
  await page.evaluate((p) => (window as unknown as W).__cad.setLayerVisible(p, true), prefix);
  const changed = await page.evaluate(
    async ([a, b, t]) => {
      const load = async (b64: string) => {
        const img = new Image();
        img.src = `data:image/png;base64,${b64}`;
        await img.decode();
        const c = document.createElement("canvas");
        c.width = img.width;
        c.height = img.height;
        const g = c.getContext("2d")!;
        g.drawImage(img, 0, 0);
        return g.getImageData(0, 0, c.width, c.height).data;
      };
      const da = await load(a as string);
      const db = await load(b as string);
      let n = 0;
      for (let i = 0; i < da.length; i += 4) {
        if (Math.max(Math.abs(da[i] - db[i]), Math.abs(da[i + 1] - db[i + 1]), Math.abs(da[i + 2] - db[i + 2])) > (t as number)) n++;
      }
      return n;
    },
    [shown.toString("base64"), hidden.toString("base64"), tol] as const,
  );
  return { found, changed };
}

export interface NodeInfo {
  idx: number;
  label: string;
  status: string;
  hints: string[];
  messages: Array<{ severity: string; message: string; edges: number }>;
  selectedEdges: number;
  openedFaces: number;
  meshKey: string | null;
  triangles: number | null;
  op: string | null;
  zone: boolean;
}

/** Nodes whose statement starts on a line (current document) with their engine data. */
export function nodesOnLine(page: Page, line: number): Promise<NodeInfo[]> {
  return page.evaluate((l) => (window as unknown as { __cad: { nodesOnLine(l: number): NodeInfo[] } }).__cad.nodesOnLine(l), line);
}

// ---------------------------------------------------------------- Phase D (test report 6)

/** Screenshots and measurements for test report 6 (tools/report/report6.mjs). */
export const REPORT6_DIR = join(here, "../../reports/tmp/report6");

/** Screenshot for report 6: the whole window (default), the view column or the editor pane. */
export async function screenshotD(page: Page, name: string, area: "view" | "full" | "editor" = "full"): Promise<void> {
  mkdirSync(join(REPORT6_DIR, "img"), { recursive: true });
  const clip = area === "view" ? await page.locator("section.viewcol").boundingBox() : area === "editor" ? await page.locator("section.editorpane").boundingBox() : null;
  await page.screenshot({ path: join(REPORT6_DIR, "img", `${name}.jpg`), type: "jpeg", quality: 80, ...(clip ? { clip } : {}) });
}

export function recordD(entry: ScenarioC): void {
  mkdirSync(REPORT6_DIR, { recursive: true });
  const f = join(REPORT6_DIR, "scenarios.json");
  const all: ScenarioC[] = existsSync(f) ? JSON.parse(readFileSync(f, "utf8")) : [];
  const i = all.findIndex((e) => e.id === entry.id);
  if (i >= 0) all[i] = entry;
  else all.push(entry);
  writeFileSync(f, JSON.stringify(all, null, 2));
}

// ---------------------------------------------------------------- Phase E (test report 7)

/** Screenshots and scenario data for test report 7 (tools/report/report7.mjs). */
export const REPORT7_DIR = join(here, "../../reports/tmp/report7");

/** Screenshot for report 7: the whole window (default), the view column or the editor pane. */
export async function screenshotE(page: Page, name: string, area: "view" | "full" | "editor" = "full"): Promise<void> {
  mkdirSync(join(REPORT7_DIR, "img"), { recursive: true });
  const clip = area === "view" ? await page.locator("section.viewcol").boundingBox() : area === "editor" ? await page.locator("section.editorpane").boundingBox() : null;
  await page.screenshot({ path: join(REPORT7_DIR, "img", `${name}.jpg`), type: "jpeg", quality: 80, ...(clip ? { clip } : {}) });
}

export function recordE(entry: ScenarioC): void {
  mkdirSync(REPORT7_DIR, { recursive: true });
  const f = join(REPORT7_DIR, "scenarios.json");
  const all: ScenarioC[] = existsSync(f) ? JSON.parse(readFileSync(f, "utf8")) : [];
  const i = all.findIndex((e) => e.id === entry.id);
  if (i >= 0) all[i] = entry;
  else all.push(entry);
  writeFileSync(f, JSON.stringify(all, null, 2));
}

// ---------------------------------------------------------------- section view (test report 9)

/** Screenshots and scenario data for test report 9 (tools/report/report9.mjs). */
export const REPORT9_DIR = join(here, "../../reports/tmp/report9");

/** Screenshot for report 9: the view column (default), the whole window or the editor pane. */
export async function screenshot9(page: Page, name: string, area: "view" | "full" | "editor" = "view"): Promise<void> {
  mkdirSync(join(REPORT9_DIR, "img"), { recursive: true });
  const clip = area === "view" ? await page.locator("section.viewcol").boundingBox() : area === "editor" ? await page.locator("section.editorpane").boundingBox() : null;
  await page.screenshot({ path: join(REPORT9_DIR, "img", `${name}.jpg`), type: "jpeg", quality: 82, ...(clip ? { clip } : {}) });
}

export function record9(entry: ScenarioC): void {
  mkdirSync(REPORT9_DIR, { recursive: true });
  const f = join(REPORT9_DIR, "scenarios.json");
  const all: ScenarioC[] = existsSync(f) ? JSON.parse(readFileSync(f, "utf8")) : [];
  const i = all.findIndex((e) => e.id === entry.id);
  if (i >= 0) all[i] = entry;
  else all.push(entry);
  writeFileSync(f, JSON.stringify(all, null, 2));
}

export function writeMetrics9(m: Record<string, unknown>): void {
  mkdirSync(REPORT9_DIR, { recursive: true });
  const f = join(REPORT9_DIR, "metrics.json");
  const old = existsSync(f) ? JSON.parse(readFileSync(f, "utf8")) : {};
  writeFileSync(f, JSON.stringify({ ...old, ...m }, null, 2));
}

// ---------------------------------------------------------------- colours, filaments, dark theme (test report 11)

/** Screenshots and scenario data for test report 11 (tools/report/report11.mjs). */
export const REPORT11_DIR = join(here, "../../reports/tmp/report11");

/** Screenshot for report 11: the whole window (default), the view column or the editor pane. */
export async function screenshot11(page: Page, name: string, area: "view" | "full" | "editor" | "colors" = "full"): Promise<void> {
  mkdirSync(join(REPORT11_DIR, "img"), { recursive: true });
  const sel = { view: "section.viewcol", editor: "section.editorpane", colors: "[data-testid=colors-panel]" } as const;
  const clip = area === "full" ? null : await page.locator(sel[area]).boundingBox();
  await page.screenshot({ path: join(REPORT11_DIR, "img", `${name}.jpg`), type: "jpeg", quality: 85, ...(clip ? { clip } : {}) });
}

export function record11(entry: ScenarioC): void {
  mkdirSync(REPORT11_DIR, { recursive: true });
  const f = join(REPORT11_DIR, "scenarios.json");
  const all: ScenarioC[] = existsSync(f) ? JSON.parse(readFileSync(f, "utf8")) : [];
  const i = all.findIndex((e) => e.id === entry.id);
  if (i >= 0) all[i] = entry;
  else all.push(entry);
  writeFileSync(f, JSON.stringify(all, null, 2));
}

export function writeMetrics11(m: Record<string, unknown>): void {
  mkdirSync(REPORT11_DIR, { recursive: true });
  const f = join(REPORT11_DIR, "metrics.json");
  const old = existsSync(f) ? JSON.parse(readFileSync(f, "utf8")) : {};
  writeFileSync(f, JSON.stringify({ ...old, ...m }, null, 2));
}

// ---------------------------------------------------------------- colour fixes (test report 12)

export const REPORT12_DIR = join(here, "../../reports/tmp/report12");

export async function screenshot12(page: Page, name: string, area: "view" | "full" | "editor" | "colors" = "view"): Promise<void> {
  mkdirSync(join(REPORT12_DIR, "img"), { recursive: true });
  const sel = { view: "section.viewcol", editor: "section.editorpane", colors: "[data-testid=colors-panel]" } as const;
  const clip = area === "full" ? null : await page.locator(sel[area]).boundingBox();
  await page.screenshot({ path: join(REPORT12_DIR, "img", `${name}.jpg`), type: "jpeg", quality: 85, ...(clip ? { clip } : {}) });
}

export function record12(entry: ScenarioC): void {
  mkdirSync(REPORT12_DIR, { recursive: true });
  const f = join(REPORT12_DIR, "scenarios.json");
  const all: ScenarioC[] = existsSync(f) ? JSON.parse(readFileSync(f, "utf8")) : [];
  const i = all.findIndex((e) => e.id === entry.id);
  if (i >= 0) all[i] = entry;
  else all.push(entry);
  writeFileSync(f, JSON.stringify(all, null, 2));
}

// ---------------------------------------------------------------- sketches, namespaces (test report 14)

export const REPORT14_DIR = join(here, "../../reports/tmp/report14");

export async function screenshot14(page: Page, name: string, area: "view" | "full" | "editor" = "full"): Promise<void> {
  mkdirSync(join(REPORT14_DIR, "img"), { recursive: true });
  const sel = { view: "section.viewcol", editor: "section.editorpane" } as const;
  const clip = area === "full" ? null : await page.locator(sel[area]).boundingBox();
  await page.screenshot({ path: join(REPORT14_DIR, "img", `${name}.jpg`), type: "jpeg", quality: 85, ...(clip ? { clip } : {}) });
}

export function record14(entry: ScenarioC): void {
  mkdirSync(REPORT14_DIR, { recursive: true });
  const f = join(REPORT14_DIR, "scenarios.json");
  const all: ScenarioC[] = existsSync(f) ? JSON.parse(readFileSync(f, "utf8")) : [];
  const i = all.findIndex((e) => e.id === entry.id);
  if (i >= 0) all[i] = entry;
  else all.push(entry);
  writeFileSync(f, JSON.stringify(all, null, 2));
}

// ---------------------------------------------------------------- corners, offset, stroke (test report 15)

export const REPORT15_DIR = join(here, "../../reports/tmp/report15");

export async function screenshot15(page: Page, name: string, area: "view" | "full" | "editor" = "full"): Promise<void> {
  mkdirSync(join(REPORT15_DIR, "img"), { recursive: true });
  const sel = { view: "section.viewcol", editor: "section.editorpane" } as const;
  const clip = area === "full" ? null : await page.locator(sel[area]).boundingBox();
  await page.screenshot({ path: join(REPORT15_DIR, "img", `${name}.jpg`), type: "jpeg", quality: 85, ...(clip ? { clip } : {}) });
}

export function record15(entry: ScenarioC): void {
  mkdirSync(REPORT15_DIR, { recursive: true });
  const f = join(REPORT15_DIR, "scenarios.json");
  const all: ScenarioC[] = existsSync(f) ? JSON.parse(readFileSync(f, "utf8")) : [];
  const i = all.findIndex((e) => e.id === entry.id);
  if (i >= 0) all[i] = entry;
  else all.push(entry);
  writeFileSync(f, JSON.stringify(all, null, 2));
}

// ---------------------------------------------------------------- text, SVG import (test report 16)

export const REPORT16_DIR = join(here, "../../reports/tmp/report16");

export async function screenshot16(page: Page, name: string, area: "view" | "full" | "editor" = "full"): Promise<void> {
  mkdirSync(join(REPORT16_DIR, "img"), { recursive: true });
  const sel = { view: "section.viewcol", editor: "section.editorpane" } as const;
  const clip = area === "full" ? null : await page.locator(sel[area]).boundingBox();
  await page.screenshot({ path: join(REPORT16_DIR, "img", `${name}.jpg`), type: "jpeg", quality: 85, ...(clip ? { clip } : {}) });
}

export function record16(entry: ScenarioC): void {
  mkdirSync(REPORT16_DIR, { recursive: true });
  const f = join(REPORT16_DIR, "scenarios.json");
  const all: ScenarioC[] = existsSync(f) ? JSON.parse(readFileSync(f, "utf8")) : [];
  const i = all.findIndex((e) => e.id === entry.id);
  if (i >= 0) all[i] = entry;
  else all.push(entry);
  writeFileSync(f, JSON.stringify(all, null, 2));
}

// ---------------------------------------------------------------- stencil font (test report 17)

export const REPORT17_DIR = join(here, "../../reports/tmp/report17");

export async function screenshot17(page: Page, name: string, area: "view" | "full" | "editor" = "full"): Promise<void> {
  mkdirSync(join(REPORT17_DIR, "img"), { recursive: true });
  const sel = { view: "section.viewcol", editor: "section.editorpane" } as const;
  const clip = area === "full" ? null : await page.locator(sel[area]).boundingBox();
  await page.screenshot({ path: join(REPORT17_DIR, "img", `${name}.jpg`), type: "jpeg", quality: 85, ...(clip ? { clip } : {}) });
}

export function record17(entry: ScenarioC): void {
  mkdirSync(REPORT17_DIR, { recursive: true });
  const f = join(REPORT17_DIR, "scenarios.json");
  const all: ScenarioC[] = existsSync(f) ? JSON.parse(readFileSync(f, "utf8")) : [];
  const i = all.findIndex((e) => e.id === entry.id);
  if (i >= 0) all[i] = entry;
  else all.push(entry);
  writeFileSync(f, JSON.stringify(all, null, 2));
}

// ---------------------------------------------------------------- sketch editor (test report 18)

export const REPORT18_DIR = join(here, "../../reports/tmp/report18");

export async function screenshot18(page: Page, name: string, area: "view" | "full" | "editor" = "full"): Promise<void> {
  mkdirSync(join(REPORT18_DIR, "img"), { recursive: true });
  const sel = { view: "section.viewcol", editor: "section.editorpane" } as const;
  const clip = area === "full" ? null : await page.locator(sel[area]).boundingBox();
  await page.screenshot({ path: join(REPORT18_DIR, "img", `${name}.jpg`), type: "jpeg", quality: 85, ...(clip ? { clip } : {}) });
}

export function record18(entry: ScenarioC): void {
  mkdirSync(REPORT18_DIR, { recursive: true });
  const f = join(REPORT18_DIR, "scenarios.json");
  const all: ScenarioC[] = existsSync(f) ? JSON.parse(readFileSync(f, "utf8")) : [];
  const i = all.findIndex((e) => e.id === entry.id);
  if (i >= 0) all[i] = entry;
  else all.push(entry);
  writeFileSync(f, JSON.stringify(all, null, 2));
}

export function writeMetrics18(m: Record<string, unknown>): void {
  mkdirSync(REPORT18_DIR, { recursive: true });
  const f = join(REPORT18_DIR, "metrics.json");
  const old = existsSync(f) ? JSON.parse(readFileSync(f, "utf8")) : {};
  writeFileSync(f, JSON.stringify({ ...old, ...m }, null, 2));
}

// ---------------------------------------------------------------- measuring (test report 19)

export const REPORT19_DIR = join(here, "../../reports/tmp/report19");

export async function screenshot19(page: Page, name: string, area: "view" | "full" | "editor" = "full"): Promise<void> {
  mkdirSync(join(REPORT19_DIR, "img"), { recursive: true });
  const sel = { view: "section.viewcol", editor: "section.editorpane" } as const;
  const clip = area === "full" ? null : await page.locator(sel[area]).boundingBox();
  await page.screenshot({ path: join(REPORT19_DIR, "img", `${name}.jpg`), type: "jpeg", quality: 85, ...(clip ? { clip } : {}) });
}

export function record19(entry: ScenarioC): void {
  mkdirSync(REPORT19_DIR, { recursive: true });
  const f = join(REPORT19_DIR, "scenarios.json");
  const all: ScenarioC[] = existsSync(f) ? JSON.parse(readFileSync(f, "utf8")) : [];
  const i = all.findIndex((e) => e.id === entry.id);
  if (i >= 0) all[i] = entry;
  else all.push(entry);
  writeFileSync(f, JSON.stringify(all, null, 2));
}

export function writeMetrics19(m: Record<string, unknown>): void {
  mkdirSync(REPORT19_DIR, { recursive: true });
  const f = join(REPORT19_DIR, "metrics.json");
  const old = existsSync(f) ? JSON.parse(readFileSync(f, "utf8")) : {};
  writeFileSync(f, JSON.stringify({ ...old, ...m }, null, 2));
}

// ---------------------------------------------------------------- UI overhaul (test report 20)

/** Screenshots and measurements for test report 20 (tools/report/report20.mjs). */
export const REPORT20_DIR = join(here, "../../reports/tmp/report20");

/** Screenshot for report 20: whole window or the first element of a selector. */
export async function screenshot20(page: Page, name: string, area: "full" | string = "full"): Promise<void> {
  mkdirSync(join(REPORT20_DIR, "img"), { recursive: true });
  const path = join(REPORT20_DIR, "img", `${name}.png`);
  if (area === "full") await page.screenshot({ path });
  else await page.locator(area).first().screenshot({ path });
}

export function record20(entry: ScenarioC): void {
  mkdirSync(REPORT20_DIR, { recursive: true });
  const f = join(REPORT20_DIR, "scenarios.json");
  const all: ScenarioC[] = existsSync(f) ? JSON.parse(readFileSync(f, "utf8")) : [];
  const i = all.findIndex((e) => e.id === entry.id);
  if (i >= 0) all[i] = entry;
  else all.push(entry);
  writeFileSync(f, JSON.stringify(all, null, 2));
}

export function writeMetrics20(m: Record<string, unknown>): void {
  mkdirSync(REPORT20_DIR, { recursive: true });
  const f = join(REPORT20_DIR, "metrics.json");
  const old = existsSync(f) ? JSON.parse(readFileSync(f, "utf8")) : {};
  writeFileSync(f, JSON.stringify({ ...old, ...m }, null, 2));
}
