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
    sceneVersion: number | null;
    complete: boolean;
    context: string | null;
    selection: { nodes: number[]; stmt: number | null; line: number; refs: number[]; grids: number[] };
    hoverNodes: number[];
    plan: {
      base: string | null;
      items: Array<{ node: number; mode: string; label: string; line: number; flat: boolean }>;
      edges: number;
      frames: number;
      anchors: number;
      activeAnchors: number;
      status: string | null;
    };
    grid: { lines: number; dashed: number; points: number; highlight: string[] };
    missingMeshes: number;
    objects: number;
  } | null;
  frames: number;
  restarts: number;
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

export async function openExample(page: Page, path: string, context: string | null = null): Promise<CadState> {
  await page.getByTestId("examples").selectOption(path);
  await page.waitForFunction((p) => (window as unknown as { __cad: { state(): { fileName: string } } }).__cad.state().fileName === p.split("/").pop(), path);
  return waitScene(page, { context });
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

export async function screenshotB(page: Page, name: string): Promise<void> {
  mkdirSync(join(REPORT3_DIR, "img"), { recursive: true });
  await page.screenshot({ path: join(REPORT3_DIR, "img", `${name}.jpg`), type: "jpeg", quality: 82 });
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
  return hook(page, (c) => c.handles());
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
