// Application runtime: engine client, mesh cache, file actions, module context sync and the
// test hook `window.__cad`.

import { EngineClient } from "../engine/client";
import type { ExportFormat } from "../engine/protocol";
import { WorkerTransport } from "../engine/transport";
import { replaceDocument } from "../editor/setup";
import { EXAMPLES, type Example, baseName, download, forgetHandle, loadAutosave, openFile, saveAutosave, saveFile } from "../files/files";
import { jumpUpTarget, moduleSpans } from "../logic/context";
import { modelHull } from "../logic/grids";
import { transformBox } from "../logic/math";
import { bridge } from "../state/bridge";
import { store } from "../state/store";
import { MeshCache } from "../view/meshCache";
import type { Viewer } from "../view/viewer";
import { DragController } from "./drag";
import { undo, redo } from "@codemirror/commands";

export const DEFAULT_EXAMPLE = "spec/gehaeuse.scad";

export const meshes = new MeshCache();
const params = new URLSearchParams(location.search);
const watchdogMs = Number(params.get("watchdog")) || undefined;
export const transport = new WorkerTransport({ watchdogMs });
export const client = new EngineClient(transport, meshes);

let viewerRef: Viewer | null = null;
/** Handle drags (Spec: Griffe und Rückschreiben). */
export const drag = new DragController(client, () => sendNow());

// Rounding steps of free drags (Einstellungen).
{
  const s = store.getState().settings;
  client.setDragOptions(s.stepMm, s.stepDeg);
}
store.subscribe((s, p) => {
  if (s.settings !== p.settings) client.setDragOptions(s.settings.stepMm, s.settings.stepDeg);
});

let viewer: Viewer | null = null;
export function setViewer(v: Viewer | null): void {
  viewer = v;
  viewerRef = v;
  if (v) v.drag = drag;
}
export function getViewer(): Viewer | null {
  return viewer;
}

transport.onMessage(() => {
  if (!store.getState().engineReady) store.setState({ engineReady: true });
});

// ---------------------------------------------------------------- source and context

let sendTimer: ReturnType<typeof setTimeout> | null = null;
let saveTimer: ReturnType<typeof setTimeout> | null = null;

/** Editor callback: the document changed. */
let lastEditAt = 0;

export function onDocChange(text: string, _changes: number): void {
  lastEditAt = performance.now();
  if (sendTimer) clearTimeout(sendTimer);
  sendTimer = setTimeout(() => {
    sendTimer = null;
    client.setSource(bridge.text(), lastEditAt);
  }, 8);
  if (saveTimer) clearTimeout(saveTimer);
  saveTimer = setTimeout(() => {
    saveTimer = null;
    void saveAutosave({ name: store.getState().fileName, text, savedAt: Date.now() });
  }, 400);
}

/** Sends the current document immediately (start, file opened). */
export function sendNow(): void {
  if (sendTimer) clearTimeout(sendTimer);
  sendTimer = null;
  client.setSource(bridge.text());
}

/** Module context follows the cursor (Spec: Kontext: aktuelles Modul). */
function syncContext(): void {
  const st = store.getState();
  const scene = st.scene;
  if (!scene || !bridge.view) return;
  const module = bridge.index(scene).moduleAt(st.cursor);
  client.setContext(module, module === null ? {} : st.paramOverrides[module] ?? {});
}

store.subscribe((s, p) => {
  if (s.cursor !== p.cursor || s.scene !== p.scene || s.docRevision !== p.docRevision || s.paramOverrides !== p.paramOverrides) syncContext();
});

/** Path bar: jump to `level` (0 = file). */
export function jumpToLevel(level: number): void {
  const st = store.getState();
  const scene = st.shown ?? st.scene;
  if (!scene) return;
  const idx = bridge.index(scene);
  const pos = jumpUpTarget(bridge.text(), moduleSpans(idx), scene.context.path, level);
  if (pos !== null) bridge.setCursor(pos);
}

// ---------------------------------------------------------------- files

export async function initialDocument(): Promise<{ name: string; text: string }> {
  const auto = await loadAutosave();
  if (auto && auto.text.trim()) return { name: auto.name, text: auto.text };
  const ex = EXAMPLES.find((e) => e.path === DEFAULT_EXAMPLE) ?? EXAMPLES[0];
  return { name: ex ? ex.name : "unbenannt.scad", text: ex ? ex.text : "cube(10);\n" };
}

function loadText(name: string, text: string): void {
  const v = bridge.view;
  if (!v) return;
  store.setState({ fileName: name, notice: null, engineError: null });
  replaceDocument(v, text);
  sendNow();
  store.setState({ loadVersion: store.getState().sentVersion });
  v.focus();
}

export function openExample(ex: Example): void {
  forgetHandle();
  loadText(ex.name, ex.text);
}

export function newFile(): void {
  forgetHandle();
  loadText("unbenannt.scad", "// Neues Modell\ncube(10);\n");
}

export async function openFromDisk(): Promise<void> {
  try {
    const f = await openFile();
    if (f) loadText(f.name, f.text);
  } catch (e) {
    store.setState({ notice: `Öffnen fehlgeschlagen: ${(e as Error).message}` });
  }
}

export async function saveToDisk(saveAs = false): Promise<void> {
  try {
    const name = await saveFile(store.getState().fileName, bridge.text(), saveAs);
    if (name) store.setState({ fileName: name, notice: `Gespeichert: ${name}` });
  } catch (e) {
    store.setState({ notice: `Speichern fehlgeschlagen: ${(e as Error).message}` });
  }
}

export async function exportModel(format: ExportFormat): Promise<void> {
  store.setState({ notice: `Export ${format.toUpperCase()} …` });
  try {
    const r = await client.exportFile(format);
    const type = format === "stl" ? "model/stl" : "model/3mf";
    const name = `${baseName(store.getState().fileName)}.${format}`;
    download(new Blob([r.bytes as BlobPart], { type }), name);
    store.setState({ notice: `Exportiert: ${name} (${r.triangles} Dreiecke)` });
  } catch (e) {
    store.setState({ notice: `Export fehlgeschlagen: ${(e as Error).message}` });
  }
}

// ---------------------------------------------------------------- test hook

declare global {
  interface Window {
    __cad?: unknown;
  }
}

window.__cad = {
  store,
  bridge,
  examples: EXAMPLES.map((e) => e.path),
  state() {
    const s = store.getState();
    return {
      fileName: s.fileName,
      docVersion: s.docVersion,
      sentVersion: s.sentVersion,
      pendingSend: s.pendingSend,
      sceneVersion: s.scene?.version ?? null,
      completeVersion: s.complete?.version ?? null,
      shownVersion: s.shown?.version ?? null,
      shownComplete: s.shown?.complete ?? false,
      shownContext: s.shown?.context.module ?? null,
      context: s.context,
      path: s.shown?.context.path ?? [],
      diagnostics: s.diagnostics.items.map((d) => ({ severity: d.severity, line: d.range.line, message: d.message, source: d.source })),
      diagnosticsVersion: s.diagnostics.version,
      engineError: s.engineError?.message ?? null,
      progress: s.progress,
      cursor: s.cursor,
      viewHover: s.viewHover,
      allGrids: s.allGrids,
      theme: s.theme,
      metrics: s.metrics,
      notice: s.notice,
      meshes: meshes.size(),
      view: viewer?.viewState() ?? null,
      frames: viewer?.frames ?? 0,
      restarts: transport.restarts,
    };
  },
  setCursorLine(line: number, col = 0) {
    bridge.setCursorLine(line, col);
  },
  text() {
    return bridge.text();
  },
  /** Handles drawn for the cursor line, with screen positions. */
  handles() {
    return viewerRef?.handleViews() ?? [];
  },
  /** Screen position on a ring (degrees) or 1D handle line (value). */
  handlePoint(id: number, value: number) {
    return viewerRef?.handlePoint(id, value) ?? null;
  },
  drag() {
    return store.getState().drag;
  },
  undo() {
    const v = bridge.view;
    return v ? undo(v) : false;
  },
  redo() {
    const v = bridge.view;
    return v ? redo(v) : false;
  },
  focusEditor() {
    bridge.view?.focus();
  },
  cursorLine() {
    const d = bridge.doc();
    return d.lineAt(Math.min(store.getState().cursor, d.length)).number;
  },
  /** Bounds of the shown model (context coordinates). */
  rootBounds() {
    const s = store.getState().shown;
    return s ? modelHull(s) : null;
  },
  /** Context bounds of the nodes whose statement starts on a line (current document). */
  nodeBounds(line: number) {
    const s = store.getState().shown;
    if (!s) return [];
    const idx = bridge.index(s);
    const doc = bridge.doc();
    return s.nodes
      .filter((n) => n.stmt !== null && n.bounds && doc.lineAt(Math.min(idx.span(n.idx).from, doc.length)).number === line)
      .map((n) => ({ label: n.label, ...transformBox(n.display_world, { min: n.bounds!.min, max: n.bounds!.max }) }));
  },
  /** Client coordinates of a character (1-based line, 0-based column). */
  coordsOf(line: number, col: number) {
    const v = bridge.view;
    if (!v) return null;
    const l = v.state.doc.line(line);
    const c = v.coordsAtPos(Math.min(l.from + col, l.to));
    return c ? { x: c.left + 2, y: (c.top + c.bottom) / 2 } : null;
  },
  setText(text: string) {
    const v = bridge.view;
    if (v) v.dispatch({ changes: { from: 0, to: v.state.doc.length, insert: text } });
  },
  openExample(path: string) {
    const ex = EXAMPLES.find((e) => e.path === path);
    if (ex) openExample(ex);
    return !!ex;
  },
  project(p: [number, number, number]) {
    return viewer?.project(p) ?? null;
  },
  pickAt(x: number, y: number) {
    const r = viewer?.pick(x, y);
    return r ? { nodes: r.nodes, lines: r.lines, tag: r.tag } : null;
  },
  findPointForLine(line: number) {
    return viewer?.findPointForLine(line) ?? null;
  },
  orbitBenchmark(frames: number) {
    return viewer?.orbitBenchmark(frames) ?? null;
  },
  setView(v: "iso" | "top" | "front" | "right") {
    viewer?.setView(v);
  },
  fitAll() {
    viewer?.fitAll();
  },
  autosave() {
    return loadAutosave();
  },
  /** Blocks the engine worker for `ms` (watchdog test). */
  debugHang(ms: number) {
    transport.send({ type: "__debugHang", ms } as unknown as Parameters<typeof transport.send>[0]);
  },
};
