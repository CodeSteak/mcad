// Application runtime: engine client, mesh cache, file actions, module context sync and the
// test hook `window.__cad`.

import type { EditorState } from "@codemirror/state";
import { EngineClient } from "../engine/client";
import type { Edit, ExportFormat } from "../engine/protocol";
import { WorkerTransport } from "../engine/transport";
import { createTauriTransport } from "../engine/tauriTransport";
import { IS_TAURI, desktop } from "../platform/desktop";
import { diskToLoad, initDesktop, onEdit, openDialog, projectLoaded, publishDirty, requestGit, saveProject } from "./desktop";
import type { EditTarget } from "../editor/editGroup";
import { makeState } from "../editor/setup";
import {
  EXAMPLES,
  type Example,
  type ExampleProject,
  type LoadedProject,
  baseName,
  download,
  forgetHandle,
  loadAutosave,
  loadProjectAutosave,
  openFile,
  openFolder,
  pickZip,
  projectZip,
  saveFile,
  saveProjectAutosave,
  saveToFolder,
} from "../files/files";
import { jumpUpTarget, moduleSpans } from "../logic/context";
import { baseName as pathBase, normalizePath } from "../logic/files";
import { modelHull } from "../logic/grids";
import { transformBox } from "../logic/math";
import { VersionMapper } from "../logic/positions";
import { isOff } from "../logic/sceneIndex";
import { bridge } from "../state/bridge";
import { type DirHandle, project } from "../state/project";
import { resolveTheme, setThemePref, store } from "../state/store";
import { MeshCache } from "../view/meshCache";
import type { Viewer } from "../view/viewer";
import { DragController } from "./drag";
import { undo, redo } from "@codemirror/commands";
import { gitMarks } from "../editor/gitGutter";

export const DEFAULT_EXAMPLE = "spec/gehaeuse.scad";

export const meshes = new MeshCache();
const params = new URLSearchParams(location.search);
const watchdogMs = Number(params.get("watchdog")) || undefined;
// Browser: engine in a Web Worker (Wasm). Desktop app: the native engine process (Tauri IPC).
export const transport = IS_TAURI ? createTauriTransport() : new WorkerTransport({ watchdogMs });
export const client = new EngineClient(transport, meshes);

let viewerRef: Viewer | null = null;
// ---------------------------------------------------------------- project files

project.activeText = () => (bridge.view ? bridge.text() : null);

/** Publishes the project's file list etc. to the store (tabs, path bar). */
function publishProject(): void {
  store.setState({
    project: { name: project.name, main: project.main, active: project.active, paths: project.paths(), folder: project.dir !== null || project.folder },
    fileName: pathBase(project.main),
  });
}

/** Editor state of a project file that is not shown (created on first use). */
function fileState(path: string): EditorState {
  let s = project.states.get(path);
  if (!s) {
    s = makeState(project.texts.get(path) ?? "");
    project.states.set(path, s);
  }
  return s;
}

/** Edit target of a project file: the editor for the shown file, its saved state otherwise. */
export function editTarget(path: string): EditTarget | null {
  if (path === project.active) return bridge.view;
  if (!project.has(path)) return null;
  return {
    get state() {
      return fileState(path);
    },
    dispatch(tr) {
      project.states.set(path, tr.state);
      if (tr.docChanged) {
        const text = tr.state.doc.toString();
        project.texts.set(path, text);
        client.setFile(path, text);
        scheduleSave();
      }
    },
  };
}

/** File an engine edit applies to. */
export function editFile(e: Edit): string {
  return e.file ? normalizePath(e.file) : project.main;
}

/** The mapper follows the editor document: after switching files, known versions start there. */
function resetMapper(): void {
  const len = bridge.doc().length;
  bridge.mapper = new VersionMapper(len);
  const st = store.getState();
  for (const v of [st.scene?.version, st.complete?.version, st.shown?.version, st.diagnostics.version, st.sentVersion]) {
    if (v !== undefined && v > 0) bridge.mapper.mark(v, len);
  }
}

/** Shows a project file in the editor (keeps the other file's state and undo history). */
export function switchFile(path: string, pos?: number): boolean {
  const p = normalizePath(path);
  const v = bridge.view;
  if (!v || !project.has(p)) {
    store.setState({ notice: `Datei ${p} gehört nicht zum Projekt` });
    return false;
  }
  if (p !== project.active) {
    flushActive();
    project.states.set(project.active, v.state);
    project.texts.set(project.active, v.state.doc.toString());
    const next = fileState(p);
    project.states.delete(p);
    project.active = p;
    v.setState(next);
    resetMapper();
    const st = store.getState();
    store.setState({ docRevision: st.docRevision + 1, cursor: next.selection.main.head, editorHover: null, viewHover: null, viewHoverOther: null });
    publishProject();
    scheduleSave();
    requestGit(false);
  }
  if (pos !== undefined) bridge.setCursor(pos);
  else v.focus();
  return true;
}

/** Handle drags (Spec: Griffe und Rückschreiben). */
export const drag = new DragController(client, () => sendNow(), { target: editTarget, fileOf: editFile, active: () => project.active });

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
  if (v) {
    v.drag = drag;
    v.onOpenLocation = openLocation;
  }
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

/** The file in the editor is not the main file: the engine gets its text with `SetFile`. */
function flushActive(): void {
  if (project.active !== project.main && bridge.view) {
    const t = bridge.text();
    if (project.texts.get(project.active) !== t) {
      project.texts.set(project.active, t);
      client.setFile(project.active, t);
    }
  }
}

function scheduleSave(): void {
  // Desktop app: files live on disk (no IndexedDB); dirty state, git markers, autosave.
  if (IS_TAURI) return onEdit();
  if (saveTimer) clearTimeout(saveTimer);
  saveTimer = setTimeout(() => {
    saveTimer = null;
    void saveProjectAutosave(project.snapshot());
  }, 400);
}

export function onDocChange(_text: string, _changes: number): void {
  lastEditAt = performance.now();
  if (sendTimer) clearTimeout(sendTimer);
  sendTimer = setTimeout(() => {
    sendTimer = null;
    flushActive();
    client.setSource(bridge.mainText(), lastEditAt);
  }, 8);
  scheduleSave();
}

/** Sends the current document immediately (start, file opened). */
export function sendNow(): void {
  if (sendTimer) clearTimeout(sendTimer);
  sendTimer = null;
  flushActive();
  client.setSource(bridge.mainText());
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

/** The project to start with: the autosaved one, else the default example. */
export async function initialProject(): Promise<{ name: string; main: string; active: string; files: Record<string, string>; disk?: { root: string; folder: boolean } }> {
  if (IS_TAURI) {
    // Desktop app: the file or folder from the command line, else the example as a new file.
    try {
      const p = await desktop.initialProject();
      if (p) {
        const d = diskToLoad(p);
        return { ...d.project, active: d.project.active, disk: d.disk };
      }
    } catch (e) {
      store.setState({ notice: `Öffnen fehlgeschlagen: ${String(e)}` });
    }
    const ex = EXAMPLES.find((e) => e.path === DEFAULT_EXAMPLE) ?? EXAMPLES[0];
    const name = ex ? ex.name : "unbenannt.scad";
    return { name, main: name, active: name, files: { [name]: ex ? ex.text : "cube(10);\n" } };
  }
  const auto = await loadProjectAutosave();
  if (auto && Object.values(auto.files).some((t) => t.trim())) return auto;
  const ex = EXAMPLES.find((e) => e.path === DEFAULT_EXAMPLE) ?? EXAMPLES[0];
  const name = ex ? ex.name : "unbenannt.scad";
  return { name, main: name, active: name, files: { [name]: ex ? ex.text : "cube(10);\n" } };
}

/** Takes over a project after the editor exists (start). */
export function startProject(p: { name: string; main: string; active?: string; files: Record<string, string>; disk?: { root: string; folder: boolean } }): void {
  project.reset(p, null, p.disk ?? null);
  publishProject();
  client.setFiles(project.others(), project.main);
  initDesktop();
  projectLoaded();
}

/**
 * Opens a project: every file, the main file in the editor (or `active`). The editor's undo
 * history starts fresh.
 */
export function loadProject(p: { name: string; main: string; active?: string; files: Record<string, string> }, dir: DirHandle | null = null, disk: { root: string; folder: boolean } | null = null): void {
  const v = bridge.view;
  if (!v) return;
  project.reset(p, dir, disk);
  v.setState(makeState(project.texts.get(project.active) ?? ""));
  resetMapper();
  const st = store.getState();
  store.setState({ notice: null, engineError: null, hiddenParts: [], viewHoverOther: null, docRevision: st.docRevision + 1, cursor: 0 });
  publishProject();
  client.setFiles(project.others(), project.main);
  sendNow();
  store.setState({ loadVersion: store.getState().sentVersion });
  scheduleSave();
  projectLoaded();
  v.focus();
}

function loadText(name: string, text: string): void {
  loadProject({ name, main: name, files: { [name]: text } });
}

export function openExampleProject(ex: ExampleProject): void {
  forgetHandle();
  loadProject({ name: ex.name, main: ex.main, files: { ...ex.files } });
}

function openLoaded(p: LoadedProject | null): void {
  if (!p) return;
  forgetHandle();
  loadProject({ name: p.name, main: p.main, files: p.files }, p.dir);
  store.setState({ notice: `Projekt ${p.name}: ${Object.keys(p.files).length} Dateien, Hauptdatei ${p.main}` });
}

/** Opens a folder (File System Access API), else a ZIP file. Desktop app: folder dialog. */
export async function openFolderOrZip(): Promise<void> {
  if (IS_TAURI) return openDialog(true);
  try {
    const w = window as Window & { showDirectoryPicker?: unknown };
    openLoaded(typeof w.showDirectoryPicker === "function" ? await openFolder() : await pickZip());
  } catch (e) {
    store.setState({ notice: `Öffnen fehlgeschlagen: ${(e as Error).message}` });
  }
}

export async function openZip(): Promise<void> {
  try {
    openLoaded(await pickZip());
  } catch (e) {
    store.setState({ notice: `Öffnen fehlgeschlagen: ${(e as Error).message}` });
  }
}

/** Downloads the project as a ZIP. */
export function downloadProjectZip(): void {
  flushActive();
  const name = `${project.name.replace(/\.scad$/, "")}.zip`;
  download(new Blob([projectZip(project.name.replace(/\.scad$/, ""), project.files()) as BlobPart], { type: "application/zip" }), name);
  store.setState({ notice: `Heruntergeladen: ${name}` });
}

/** Adds a file to the project and shows it. */
export function addProjectFile(path: string, text = ""): boolean {
  const p = normalizePath(path.trim());
  if (!p || p.startsWith("..") || p.startsWith("/")) {
    store.setState({ notice: `Ungültiger Dateiname: ${path}` });
    return false;
  }
  const name = p.endsWith(".scad") ? p : `${p}.scad`;
  if (project.has(name)) return switchFile(name);
  project.texts.set(name, text);
  client.setFile(name, text);
  publishProject();
  switchFile(name);
  sendNow();
  publishDirty();
  return true;
}

/** Removes a file from the project (not the main file). */
export function removeProjectFile(path: string): void {
  if (path === project.main || !project.has(path)) return;
  if (path === project.active) switchFile(project.main);
  project.texts.delete(path);
  project.states.delete(path);
  client.setFile(path, null);
  publishProject();
  sendNow();
  scheduleSave();
}

/** Makes a project file the main file (what is evaluated). */
export function setMainFile(path: string): void {
  if (!project.has(path) || path === project.main) return;
  flushActive();
  project.texts.set(project.active, bridge.text());
  project.main = path;
  store.setState({ hiddenParts: [] });
  publishProject();
  client.setFiles(project.others(), project.main);
  sendNow();
  store.setState({ loadVersion: store.getState().sentVersion });
  scheduleSave();
}

/** Opens a position in a project file (scene range of another file, e.g. a face's line). */
export function openLocation(path: string, pos: number): void {
  if (path === project.active) bridge.setCursor(pos);
  else switchFile(path, pos);
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
  if (IS_TAURI) return openDialog(false);
  try {
    const f = await openFile();
    if (f) loadText(f.name, f.text);
  } catch (e) {
    store.setState({ notice: `Öffnen fehlgeschlagen: ${(e as Error).message}` });
  }
}

export async function saveToDisk(saveAs = false): Promise<void> {
  if (IS_TAURI) return saveProject(saveAs);
  try {
    flushActive();
    if (project.dir && !saveAs) {
      const n = await saveToFolder(project.dir, project.files());
      store.setState({ notice: `Gespeichert: ${n} Dateien in ${project.dir.name}` });
      return;
    }
    if (project.texts.size > 1 && !saveAs) {
      downloadProjectZip();
      return;
    }
    const name = await saveFile(pathBase(project.active), bridge.text(), saveAs);
    if (name && project.texts.size === 1) {
      // A single file renamed by "Speichern unter": the project follows.
      if (name !== project.main) {
        project.texts.delete(project.main);
        project.states.delete(project.main);
        project.main = project.active = name;
        project.texts.set(name, bridge.text());
        project.name = name;
        client.setFiles({}, name);
        publishProject();
      }
    }
    if (name) store.setState({ notice: `Gespeichert: ${name}` });
  } catch (e) {
    store.setState({ notice: `Speichern fehlgeschlagen: ${(e as Error).message}` });
  }
}

const MIME: Record<ExportFormat, string> = { stl: "model/stl", "3mf": "model/3mf", step: "model/step" };

export async function exportModel(format: ExportFormat): Promise<void> {
  store.setState({ notice: `Export ${format.toUpperCase()} …` });
  try {
    const r = await client.exportFile(format);
    const name = `${baseName(store.getState().fileName)}.${format}`;
    download(new Blob([r.bytes as BlobPart], { type: MIME[format] }), name);
    store.setState({ notice: `Exportiert: ${name} (${r.triangles ? `${r.triangles} Dreiecke` : `${r.bytes.length} Bytes`})` });
  } catch (e) {
    store.setState({ notice: `Export fehlgeschlagen: ${(e as Error).message}` });
  }
}

/**
 * Multi-colour exports of the model (or the parts, if any): 3MF with one volume per colour
 * and filament slots for Bambu Studio / OrcaSlicer, single-colour 3MF, or a ZIP with one STL
 * per colour.
 */
export async function exportColors(kind: "3mf" | "3mf-single" | "stl-zip"): Promise<void> {
  const st = store.getState();
  const parts = (st.complete?.context.module === null ? st.complete?.parts ?? [] : []).map((p) => p.name);
  const format: ExportFormat = kind === "stl-zip" ? "stl" : "3mf";
  const colors = kind !== "3mf-single";
  const label = kind === "3mf" ? "3MF mehrfarbig" : kind === "3mf-single" ? "3MF einfarbig" : "STL je Farbe";
  store.setState({ notice: `Export ${label} …` });
  try {
    const r = await client.exportFile(format, parts.length ? parts : undefined, false, colors);
    const base = baseName(st.fileName);
    const name = kind === "stl-zip" ? `${base}-farben.zip` : kind === "3mf" ? `${base}.3mf` : `${base}-einfarbig.3mf`;
    download(new Blob([r.bytes as BlobPart], { type: kind === "stl-zip" ? "application/zip" : MIME["3mf"] }), name);
    store.setState({ notice: `Exportiert: ${name} (${r.triangles} Dreiecke)` });
  } catch (e) {
    store.setState({ notice: `Export fehlgeschlagen: ${(e as Error).message}` });
  }
}

// ---------------------------------------------------------------- theme and colour regions

if (typeof matchMedia !== "undefined") {
  // "System": follow dark mode changes of the OS (browser and desktop webview).
  matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
    const s = store.getState();
    if (s.themePref === "system") store.setState({ theme: resolveTheme("system") });
  });
}

let regionsAsked = "";
let regionsTimer: ReturnType<typeof setTimeout> | null = null;

/** Asks the engine for the colour regions of the complete scene (colour list or region view). */
function syncRegions(): void {
  const s = store.getState();
  const c = s.complete;
  const want = (s.colorsOpen || s.showRegions) && c !== null && (c.colors ?? []).length > 0;
  if (!want || !c) return;
  const key = `${c.version}|${s.showRegions}`;
  if (key === regionsAsked) return;
  if (s.regions && s.regions.version === c.version && (s.regions.meshes || !s.showRegions)) return;
  regionsAsked = key;
  if (regionsTimer) clearTimeout(regionsTimer);
  // Small delay: while typing, several scenes follow each other.
  regionsTimer = setTimeout(() => {
    regionsTimer = null;
    const st = store.getState();
    if (st.complete?.version === c.version) client.requestColorRegions(c.version, st.showRegions);
  }, 120);
}

store.subscribe((s, p) => {
  if (s.complete !== p.complete || s.colorsOpen !== p.colorsOpen || s.showRegions !== p.showRegions) syncRegions();
});

/** File name of a part (`k[2]` → `k_2.stl`, as the engine names ZIP entries). */
export function partFileName(name: string, ext: string): string {
  const s = name.replace(/[^A-Za-z0-9_.-]/g, "_").replace(/_+$/, "") || "teil";
  return `${s}.${ext}`;
}

/**
 * Exports parts: one part → one file; several → one file per part as a ZIP (`zip`), or one 3MF
 * with an object per part.
 */
export async function exportParts(format: ExportFormat, parts: string[], zip: boolean): Promise<void> {
  if (!parts.length) return;
  store.setState({ notice: `Export ${format.toUpperCase()} (${parts.join(", ")}) …` });
  try {
    const single = parts.length === 1;
    const r = await client.exportFile(format, parts, zip && !single);
    const base = baseName(store.getState().fileName);
    const name = single ? partFileName(parts[0], format) : zip ? `${base}-teile-${format}.zip` : `${base}-teile.${format}`;
    const type = zip && !single ? "application/zip" : MIME[format];
    download(new Blob([r.bytes as BlobPart], { type }), name);
    store.setState({ notice: `Exportiert: ${name}${r.triangles ? ` (${r.triangles} Dreiecke)` : ""}` });
  } catch (e) {
    store.setState({ notice: `Export fehlgeschlagen: ${(e as Error).message}` });
  }
}

/** Shows or hides a part in the view. */
export function togglePart(name: string, visible: boolean): void {
  const h = store.getState().hiddenParts;
  store.setState({ hiddenParts: visible ? h.filter((x) => x !== name) : h.includes(name) ? h : [...h, name] });
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
      themePref: s.themePref,
      colorsOpen: s.colorsOpen,
      showRegions: s.showRegions,
      regions: s.regions ? { version: s.regions.version, meshes: s.regions.meshes, items: s.regions.items.map((x) => ({ part: x.part, total: x.total, warning: x.warning, regions: x.regions.map((r) => ({ color: r.color, filament: r.filament, volume: r.volume })) })) } : null,
      colors: (s.complete?.colors ?? []).map((c) => ({ key: c.key, label: c.label, source: c.source, filament: c.filament, explicit: c.explicit, lines: c.ranges.map((r) => r.line) })),
      metrics: s.metrics,
      notice: s.notice,
      meshes: meshes.size(),
      view: viewer?.viewState() ?? null,
      frames: viewer?.frames ?? 0,
      restarts: transport.restarts,
      blockedKeys: [...transport.blockedKeys],
      desktop: IS_TAURI,
      dirty: s.dirty,
      external: Object.fromEntries(Object.entries(s.external).map(([k, v]) => [k, v.kind])),
      section: s.section,
      lastCli: s.lastCli,
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
      .filter((n) => n.stmt !== null && n.bounds && !isOff(idx.span(n.idx)) && doc.lineAt(Math.min(idx.span(n.idx).from, doc.length)).number === line)
      .map((n) => ({ label: n.label, ...transformBox(n.display_world, { min: n.bounds!.min, max: n.bounds!.max }) }));
  },
  /** Nodes whose statement starts on a line (current document) with their engine data. */
  nodesOnLine(line: number) {
    const s = store.getState().shown;
    if (!s) return [];
    const idx = bridge.index(s);
    const doc = bridge.doc();
    return s.nodes
      .filter((n) => n.stmt !== null && !isOff(idx.span(n.idx)) && doc.lineAt(Math.min(idx.span(n.idx).from, doc.length)).number === line)
      .map((n) => {
        const m = meshes.get(n.mesh_key)?.data;
        return {
          idx: n.idx,
          label: n.label,
          status: n.status,
          hints: n.hints,
          messages: n.messages.map((x) => ({ severity: x.severity, message: x.message, edges: x.edges.length })),
          selectedEdges: n.selected_edges.length,
          openedFaces: n.opened_faces,
          meshKey: n.mesh_key,
          triangles: m ? m.indices.length / 3 : null,
          op: n.op_info?.kind ?? null,
          zone: n.flags.zone,
        };
      });
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
  /** Shows/hides drawn objects by name prefix (pixel checks); returns the number found. */
  setLayerVisible(prefix: string, visible: boolean) {
    return viewer?.setLayerVisible(prefix, visible) ?? 0;
  },
  /** Camera looking along `-dir` (from the direction `dir`), then framed (tests). */
  viewFrom(dir: [number, number, number]) {
    viewer?.viewFrom(dir);
  },
  autosave() {
    return loadAutosave();
  },
  /** The open project: file list, main and active file. */
  projectInfo() {
    return { ...store.getState().project, files: project.files() };
  },
  /** Opens a project (tests). */
  openProject(p: { name: string; main: string; files: Record<string, string>; active?: string }) {
    loadProject(p);
  },
  switchFile(path: string, line?: number) {
    const ok = switchFile(path);
    if (ok && line !== undefined) bridge.setCursorLine(line);
    return ok;
  },
  /** Parts of the latest complete scene. */
  parts() {
    const c = store.getState().complete;
    return (c?.parts ?? []).map((p) => ({ name: p.name, node: p.node, line: p.range.line, volume: p.volume, bounds: p.bounds, hidden: store.getState().hiddenParts.includes(p.name) }));
  },
  autosaveProject() {
    return loadProjectAutosave();
  },

  /** Blocks the engine worker for `ms` (watchdog test). */
  debugHang(ms: number) {
    if (transport instanceof WorkerTransport) transport.send({ type: "__debugHang", ms } as unknown as Parameters<typeof transport.send>[0]);
    else transport.debugHang(ms);
  },
  /** Section view as drawn. */
  sectionInfo() {
    return viewer?.sectionInfo() ?? null;
  },
  setSection(s: Partial<import("../logic/section").SectionState>) {
    store.setState({ section: { ...store.getState().section, ...s } });
  },
  sectionBenchmark(frames: number) {
    return viewer?.sectionBenchmark(frames) ?? null;
  },
  /** Git markers of the editor (desktop app). */
  gitMarks() {
    const v = bridge.view;
    return v ? gitMarks(v.state) : null;
  },
  /** Project on disk (desktop app). */
  disk() {
    return { root: project.root, folder: project.folder, dirty: project.dirty(), saved: Object.fromEntries(project.saved) };
  },
  /** Engine bytes received over the Tauri channel. */
  transportBytes() {
    return "bytes" in transport ? (transport as { bytes: number }).bytes : null;
  },
  async appInfo() {
    return IS_TAURI ? desktop.appInfo() : null;
  },
  /**
   * Export without download (tests, also in the desktop app): bytes, triangles and the
   * strings a multi-colour package must contain.
   */
  async exportSummary(format: ExportFormat, colors?: boolean, parts?: string[]) {
    const r = await client.exportFile(format, parts, false, colors);
    const t = new TextDecoder().decode(r.bytes);
    const count = (re: RegExp) => (t.match(re) ?? []).length;
    return {
      bytes: r.bytes.length,
      triangles: r.triangles,
      colors: r.colors ?? false,
      colorGroup: t.includes("<m:colorgroup"),
      mColors: count(/<m:color /g),
      items: count(/<item /g),
      components: count(/<component /g),
      modelSettings: t.includes("Metadata/model_settings.config"),
      extruders: [...t.matchAll(/<metadata key="extruder" value="(\d+)"\/>/g)].map((m) => Number(m[1])),
      files: [...t.matchAll(/([A-Za-z0-9_-]+_F\d+_[A-Za-z0-9_-]+\.stl)/g)].map((m) => m[1]).filter((v, i, a) => a.indexOf(v) === i),
    };
  },
  /** Chooses the theme (System/Hell/Dunkel) like the toolbar. */
  setThemePref(p: "system" | "light" | "dark") {
    setThemePref(p);
  },
  /** Contrast of text and background of the first element matching a selector (computed styles). */
  contrastOf(textSel: string, bgSel: string) {
    const parse = (v: string) => {
      const c = document.createElement("canvas").getContext("2d")!;
      c.fillStyle = v;
      const h = c.fillStyle as string;
      if (h.startsWith("#")) return [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16));
      return (/rgba?\(([^)]+)\)/.exec(h)?.[1] ?? "0,0,0").split(",").slice(0, 3).map(Number);
    };
    const fg = document.querySelector(textSel);
    const bg = document.querySelector(bgSel);
    if (!fg || !bg) return null;
    const a = parse(getComputedStyle(fg).color);
    const b = parse(getComputedStyle(bg).backgroundColor);
    const lin = (x: number) => (x / 255 <= 0.04045 ? x / 255 / 12.92 : ((x / 255 + 0.055) / 1.055) ** 2.4);
    const L = (c: number[]) => 0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2]);
    const [x, y] = [L(a), L(b)];
    return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05);
  },
  /** Does the system prefer a dark theme (`prefers-color-scheme`)? */
  systemDark() {
    return typeof matchMedia !== "undefined" && matchMedia("(prefers-color-scheme: dark)").matches;
  },
};
