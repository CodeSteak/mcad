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
  metrics: { firstSceneMs: number | null; editToScene: number[]; evalMs: number[] };
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
