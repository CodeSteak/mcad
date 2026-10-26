// Application runtime: engine client, mesh cache, file actions, module context sync and the
// test hook `window.__cad`.

import type { EditorState } from "@codemirror/state";
import { EngineClient } from "../engine/client";
import type { AssertResult, Edit, ExportFormat } from "../engine/protocol";
import { WorkerTransport } from "../engine/transport";
import { createTauriTransport } from "../engine/tauriTransport";
import { IS_TAURI, desktop } from "../platform/desktop";
import { diskToLoad, initDesktop, onEdit, openDialog, projectLoaded, publishDirty, requestGit, saveProject } from "./desktop";
import type { EditTarget } from "../editor/editGroup";
import { makeState } from "../editor/setup";
import {
  EXAMPLES,
  SHOW_TEST_MODELS,
  type Example,
  type ExampleProject,
  type LoadedProject,
  baseName,
  download,
  exampleData,
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
import { type MessageKey, getLang, resolveLang, setLang, t } from "../i18n";
import { hasFaceColors } from "../logic/colors";
import { jumpUpTarget, moduleSpans } from "../logic/context";
import { baseName as pathBase, diagnosticPath, fileText, isDataFile, isFontFile, isMeshFile, isModelFile, MODEL_EXT, normalizePath, rangePath, stripModelExt, withModelExt } from "../logic/files";
import { modelHull } from "../logic/grids";
import { transformBox } from "../logic/math";
import { VersionMapper, mapRange } from "../logic/positions";
import { isOff } from "../logic/sceneIndex";
import { bridge } from "../state/bridge";
import { type DirHandle, project } from "../state/project";
import { colorsVisible, type ExportChoice, type SideTab, resolveTheme, setExportChoice, setLangPref, setSideTab, setThemePref, store } from "../state/store";
import { MeshCache } from "../view/meshCache";
import type { Viewer } from "../view/viewer";
import { DragController } from "./drag";
import { SketchController } from "./sketch";
import { MeasureController } from "./measure";
import { undo, redo } from "@codemirror/commands";
import { gitMarks } from "../editor/gitGutter";

/** Model shown at the first start (test builds: the spec housing the e2e tests expect). */
export const DEFAULT_EXAMPLE = SHOW_TEST_MODELS ? "spec/gehaeuse.mcad" : "05-box-lid.mcad";

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
    store.setState({ notice: t("notice.notInProject", { path: p }) });
    return false;
  }
  if (isFontFile(p)) {
    store.setState({ notice: t("notice.fontFile", { path: p, name: pathBase(p) }) });
    return false;
  }
  if (isMeshFile(p)) {
    store.setState({ notice: t("notice.meshFile", { path: p, name: pathBase(p) }) });
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

/** Sketch editor (Plan 17.12). */
export const sketch = new SketchController(client, () => sendNow());

/** Measure mode (Plan 18). */
export const measure = new MeasureController(client);

// Sketch editor and measure mode exclude each other.
store.subscribe((s, p) => {
  if (s.sketch && !p.sketch && s.measure) measure.exit();
  if (s.measure && !p.measure && s.sketch) sketch.exit();
});

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
    v.sketch = sketch;
    v.measure = measure;
    v.onOpenLocation = openLocation;
  }
  sketch.setView(v);
  measure.setView(v);
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
      store.setState({ notice: t("notice.openFailed", { error: String(e) }) });
    }
    const ex = EXAMPLES.find((e) => e.path === DEFAULT_EXAMPLE) ?? EXAMPLES[0];
    const name = ex ? ex.name : `unbenannt${MODEL_EXT}`;
    return { name, main: name, active: name, files: { [name]: ex ? ex.text : "cube(10);\n" } };
  }
  const auto = await loadProjectAutosave();
  if (auto && Object.values(auto.files).some((t) => t.trim())) return auto;
  const ex = EXAMPLES.find((e) => e.path === DEFAULT_EXAMPLE) ?? EXAMPLES[0];
  const name = ex ? ex.name : `unbenannt${MODEL_EXT}`;
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
  store.setState({ notice: t("notice.projectOpened", { name: p.name, n: Object.keys(p.files).length, main: p.main }) });
}

/** Opens a folder (File System Access API), else a ZIP file. Desktop app: folder dialog. */
export async function openFolderOrZip(): Promise<void> {
  if (IS_TAURI) return openDialog(true);
  try {
    const w = window as Window & { showDirectoryPicker?: unknown };
    openLoaded(typeof w.showDirectoryPicker === "function" ? await openFolder() : await pickZip());
  } catch (e) {
    store.setState({ notice: t("notice.openFailed", { error: (e as Error).message }) });
  }
}

export async function openZip(): Promise<void> {
  try {
    openLoaded(await pickZip());
  } catch (e) {
    store.setState({ notice: t("notice.openFailed", { error: (e as Error).message }) });
  }
}

/** Downloads the project as a ZIP. */
export function downloadProjectZip(): void {
  flushActive();
  const name = `${stripModelExt(project.name)}.zip`;
  download(new Blob([projectZip(stripModelExt(project.name), project.files()) as BlobPart], { type: "application/zip" }), name);
  store.setState({ notice: t("notice.downloaded", { name }) });
}

/** Adds a file to the project and shows it. */
export function addProjectFile(path: string, text = ""): boolean {
  const p = normalizePath(path.trim());
  if (!p || p.startsWith("..") || p.startsWith("/")) {
    store.setState({ notice: t("notice.badName", { path }) });
    return false;
  }
  const name = withModelExt(p);
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
  // an example importing SVGs of tests/models/svg: a project with those files (paths below tests/models/)
  const data = exampleData(ex.path, ex.text);
  if (Object.keys(data).length) loadProject({ name: ex.name, main: ex.path, files: { [ex.path]: ex.text, ...data } });
  else loadText(ex.name, ex.text);
}

/**
 * Adds files from the computer to the project (UI upload): `.mcad`/`.scad` and SVG as text, fonts and
 * STL as data URL; next to the main file. Existing files of the same name are replaced.
 */
export async function uploadProjectFiles(files: ArrayLike<File>): Promise<void> {
  const dir = project.main.includes("/") ? project.main.slice(0, project.main.lastIndexOf("/") + 1) : "";
  const added: string[] = [];
  for (const f of Array.from(files)) {
    const name = normalizePath(dir + f.name);
    if (!isModelFile(name) && !isDataFile(name)) {
      store.setState({ notice: t("notice.badUpload", { name: f.name }) });
      continue;
    }
    const text = fileText(name, new Uint8Array(await f.arrayBuffer()));
    if (name === project.active && bridge.view) {
      bridge.view.dispatch({ changes: { from: 0, to: bridge.view.state.doc.length, insert: text } });
    } else {
      project.states.delete(name);
      project.texts.set(name, text);
      client.setFile(name, text);
    }
    added.push(name);
  }
  if (!added.length) return;
  publishProject();
  sendNow();
  scheduleSave();
  publishDirty();
  store.setState({ notice: t("notice.added", { names: added.join(", ") }) });
}

export function newFile(): void {
  forgetHandle();
  loadText(`unbenannt${MODEL_EXT}`, `// ${t("file.newModel")}\ncube(10);\n`);
}

export async function openFromDisk(): Promise<void> {
  if (IS_TAURI) return openDialog(false);
  try {
    const f = await openFile();
    if (f) loadText(f.name, f.text);
  } catch (e) {
    store.setState({ notice: t("notice.openFailed", { error: (e as Error).message }) });
  }
}

export async function saveToDisk(saveAs = false): Promise<void> {
  if (IS_TAURI) return saveProject(saveAs);
  try {
    flushActive();
    if (project.dir && !saveAs) {
      const n = await saveToFolder(project.dir, project.files());
      store.setState({ notice: t("notice.savedFolder", { n, dir: project.dir.name }) });
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
    if (name) store.setState({ notice: t("notice.saved", { what: name }) });
  } catch (e) {
    store.setState({ notice: t("notice.saveFailed", { error: (e as Error).message }) });
  }
}

const MIME: Record<ExportFormat, string> = { stl: "model/stl", "3mf": "model/3mf", step: "model/step" };

// ---------------------------------------------------------------- assertions (Plan 21 C)

/** Name of an assertion kind as in the code. */
export function assertName(r: AssertResult): string {
  return `assert_${r.kind}`;
}

/** Violations of the assertions of a scene version as text lines (`Z.3 assert_disjoint: …`). */
export function violationLines(items: AssertResult[]): string[] {
  const out: string[] = [];
  for (const r of items) for (const p of r.pairs) if (!p.ok) out.push(`Z.${r.range.line} ${assertName(r)}: ${p.message}`);
  return out;
}

/** The assertion results of the latest complete scene; waits (up to 60 s) while they are being
 *  checked. `[]` for a model without assertions. */
async function currentAsserts(): Promise<AssertResult[]> {
  const ready = () => {
    const s = store.getState();
    const c = s.complete;
    if (!c) return [] as AssertResult[];
    if (!c.nodes.some((n) => n.label.startsWith("assert_"))) return [];
    return s.asserts && s.asserts.version === c.version ? s.asserts.items : null;
  };
  const now = ready();
  if (now) return now;
  store.setState({ notice: t("notice.checkingAsserts") });
  return new Promise((resolve) => {
    const done = (v: AssertResult[]) => {
      clearTimeout(t);
      unsub();
      resolve(v);
    };
    const unsub = store.subscribe(() => {
      const v = ready();
      if (v) done(v);
    });
    const t = setTimeout(() => done(store.getState().asserts?.items ?? []), 60_000);
  });
}

/** Exports via `run` unless an assertion is violated; then the dialog "Trotzdem exportieren"
 *  (like `mcad export --ignore-asserts`). */
async function guardedExport(what: string, run: () => Promise<void>): Promise<void> {
  const v = violationLines(await currentAsserts());
  if (!v.length) return run();
  store.setState({ exportConfirm: { what, violations: v, run }, notice: t("notice.exportHeld", { n: v.length }) });
}

/** "Trotzdem exportieren" (`true`) or "Abbrechen" in the export dialog. */
export async function confirmExport(go: boolean): Promise<void> {
  const c = store.getState().exportConfirm;
  store.setState({ exportConfirm: null });
  if (!c) return;
  if (go) await c.run();
  else store.setState({ notice: t("notice.exportCancelled") });
}

bridge.onAssertClick = (line) => focusAssertAtLine(line);
bridge.onLocateClick = (line) => focusDiagnosticAtLine(line);

/** Click on a kernel error with a place (`Diagnostic.at`, e.g. operands touching
 *  tangentially) at line `line` of the file in the editor: frame the place in the view. */
export function focusDiagnosticAtLine(line: number): boolean {
  const s = store.getState();
  const active = project.active;
  const d = s.diagnostics.items.find((x) => x.at && x.range.line === line && diagnosticPath(x, project.main) === active);
  if (!d?.at) return false;
  const a = d.at;
  getViewer()?.frameBox([a[0] - 1, a[1] - 1, a[2] - 1], [a[0] + 1, a[1] + 1, a[2] + 1]);
  store.setState({ notice: t("notice.framed", { at: a.map((x) => +x.toFixed(3)).join(", ") }) });
  return true;
}

/** Jumps to an assertion statement and frames its pair `pair` (the intersection body, the
 *  contact or the gap) in the view. */
export function focusAssert(r: AssertResult, pair: number): void {
  const s = store.getState();
  const c = s.complete;
  const a = s.asserts;
  if (!c || !a) return;
  store.setState({ assertFocus: { version: a.version, node: r.node, pair } });
  const path = rangePath(c, r.range, project.main);
  const pos = mapRange(bridge.mapper.mapper(a.version), r.range.from, r.range.to).from;
  openLocation(path, pos);
  const f = r.pairs[pair]?.focus;
  if (f) getViewer()?.frameBox(f.min, f.max);
}

/** The next violated pair after the focused one (status bar button). */
export function focusNextViolation(): void {
  const s = store.getState();
  const items = s.asserts?.items ?? [];
  const all: Array<[AssertResult, number]> = [];
  for (const r of items) r.pairs.forEach((p, i) => !p.ok && all.push([r, i]));
  if (!all.length) return;
  const f = s.assertFocus;
  const at = f ? all.findIndex(([r, i]) => r.node === f.node && i === f.pair) : -1;
  const [r, i] = all[(at + 1) % all.length];
  focusAssert(r, i);
}

/** Click on an assertion warning at line `line` of the main file: focus its first violation. */
export function focusAssertAtLine(line: number): boolean {
  const s = store.getState();
  const r = (s.asserts?.items ?? []).find((x) => x.range.line === line && (x.range.file ?? 0) === 0 && x.pairs.some((p) => !p.ok));
  if (!r) return false;
  focusAssert(r, r.pairs.findIndex((p) => !p.ok));
  return true;
}

export async function exportModel(format: ExportFormat): Promise<void> {
  return guardedExport(format.toUpperCase(), () => exportModelNow(format));
}

async function exportModelNow(format: ExportFormat): Promise<void> {
  store.setState({ notice: t("notice.exporting", { what: format.toUpperCase() }) });
  try {
    const r = await client.exportFile(format);
    const name = `${baseName(store.getState().fileName)}.${format}`;
    download(new Blob([r.bytes as BlobPart], { type: MIME[format] }), name);
    store.setState({ notice: `${t("notice.exported", { name })} (${r.triangles ? t("notice.triangles", { n: r.triangles }) : t("notice.bytes", { n: r.bytes.length })})` });
  } catch (e) {
    store.setState({ notice: t("notice.exportFailed", { error: (e as Error).message }) });
  }
}

/**
 * Multi-colour exports of the model: 3MF with one object per part (material outside the parts:
 * an object "ohne Bauteil"), one volume per colour and filament slots for Bambu Studio /
 * OrcaSlicer, single-colour 3MF, or a ZIP with one STL per colour. The engine picks the parts
 * like `mcad export` (Bericht 12, F4): the files of UI and CLI are the same.
 */
export async function exportColors(kind: "3mf" | "3mf-single" | "stl-zip"): Promise<void> {
  return guardedExport(colorExportLabel(kind), () => exportColorsNow(kind));
}

function colorExportLabel(kind: "3mf" | "3mf-single" | "stl-zip"): string {
  return kind === "3mf" ? t("export.label.3mf") : kind === "3mf-single" ? t("export.label.3mfSingle") : t("export.label.stlColorsShort");
}

async function exportColorsNow(kind: "3mf" | "3mf-single" | "stl-zip"): Promise<void> {
  const st = store.getState();
  const format: ExportFormat = kind === "stl-zip" ? "stl" : "3mf";
  const colors = kind !== "3mf-single";
  const label = colorExportLabel(kind);
  store.setState({ notice: t("notice.exporting", { what: label }) });
  try {
    const r = await client.exportFile(format, undefined, false, colors);
    const base = baseName(st.fileName);
    const name = kind === "stl-zip" ? `${base}-farben.zip` : kind === "3mf" ? `${base}.3mf` : `${base}-einfarbig.3mf`;
    download(new Blob([r.bytes as BlobPart], { type: kind === "stl-zip" ? "application/zip" : MIME["3mf"] }), name);
    const notes = r.notes?.length ? ` – ${r.notes.join("; ")}` : "";
    store.setState({ notice: `${t("notice.exported", { name })} (${t("notice.triangles", { n: r.triangles })})${notes}` });
  } catch (e) {
    store.setState({ notice: t("notice.exportFailed", { error: (e as Error).message }) });
  }
}

// ---------------------------------------------------------------- theme and colour regions

// "System" language: follow a change of the browser language.
if (typeof window !== "undefined") {
  window.addEventListener("languagechange", () => {
    if (store.getState().langPref === "system") setLang(resolveLang("system"));
  });
}

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
  // the list needs volumes; the view draws the regions of a model with colours (Bericht 12)
  const want = c !== null && ((colorsVisible(s) && (c.colors ?? []).length > 0) || (s.showRegions && hasFaceColors(c)));
  if (!want || !c) return;
  // meshes only where the view draws them (a model with colours): none for uncoloured models
  const meshes = s.showRegions && hasFaceColors(c);
  const key = `${c.version}|${meshes}`;
  if (key === regionsAsked) return;
  if (s.regions && s.regions.version === c.version && (s.regions.meshes || !meshes)) return;
  regionsAsked = key;
  if (regionsTimer) clearTimeout(regionsTimer);
  // Small delay: while typing, several scenes follow each other.
  regionsTimer = setTimeout(() => {
    regionsTimer = null;
    const st = store.getState();
    if (st.complete?.version === c.version) client.requestColorRegions(c.version, meshes);
  }, 120);
}

store.subscribe((s, p) => {
  if (s.complete !== p.complete || colorsVisible(s) !== colorsVisible(p) || s.showRegions !== p.showRegions) syncRegions();
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
  return guardedExport(`${format.toUpperCase()} (${parts.join(", ")})`, () => exportPartsNow(format, parts, zip));
}

async function exportPartsNow(format: ExportFormat, parts: string[], zip: boolean): Promise<void> {
  store.setState({ notice: t("notice.exporting", { what: `${format.toUpperCase()} (${parts.join(", ")})` }) });
  try {
    const single = parts.length === 1;
    const r = await client.exportFile(format, parts, zip && !single);
    const base = baseName(store.getState().fileName);
    const name = single ? partFileName(parts[0], format) : zip ? `${base}-teile-${format}.zip` : `${base}-teile.${format}`;
    const type = zip && !single ? "application/zip" : MIME[format];
    download(new Blob([r.bytes as BlobPart], { type }), name);
    store.setState({ notice: `${t("notice.exported", { name })}${r.triangles ? ` (${t("notice.triangles", { n: r.triangles })})` : ""}` });
  } catch (e) {
    store.setState({ notice: t("notice.exportFailed", { error: (e as Error).message }) });
  }
}

/** Shows or hides a part in the view. */
export function togglePart(name: string, visible: boolean): void {
  const h = store.getState().hiddenParts;
  store.setState({ hiddenParts: visible ? h.filter((x) => x !== name) : h.includes(name) ? h : [...h, name] });
}

// ---------------------------------------------------------------- export button

/** Names of the export choices (menu, button text, notices). */
const exportLabel = (key: MessageKey | null, plain: string, short: string) => ({
  get label() {
    return key ? t(key) : plain;
  },
  short,
});

export const EXPORT_LABELS: Record<ExportChoice, { readonly label: string; short: string }> = {
  "3mf": exportLabel("export.label.3mf", "", "3MF"),
  "3mf-single": exportLabel("export.label.3mfSingle", "", "3MF"),
  stl: exportLabel(null, "STL", "STL"),
  "stl-colors": exportLabel("export.label.stlColors", "", "STL"),
  step: exportLabel(null, "STEP", "STEP"),
  "parts-stl": exportLabel("export.label.partsStl", "", "STL"),
  "parts-step": exportLabel("export.label.partsStep", "", "STEP"),
  "parts-3mf": exportLabel("export.label.parts3mf", "", "3MF"),
};

/** Part names of the file context (the exports "je Bauteil"). */
export function partNames(): string[] {
  const s = store.getState();
  const shown = s.complete ?? s.shown;
  return shown && shown.context.module === null ? shown.parts.map((p) => p.name) : [];
}

/** Runs an export of the export button (and remembers the format for the button and Strg+E). */
export async function runExport(c: ExportChoice, remember = true): Promise<void> {
  if (remember) setExportChoice(c);
  switch (c) {
    case "3mf":
      return exportColors("3mf");
    case "3mf-single":
      return exportColors("3mf-single");
    case "stl-colors":
      return exportColors("stl-zip");
    case "stl":
      return exportModel("stl");
    case "step":
      return exportModel("step");
    case "parts-stl":
    case "parts-step":
    case "parts-3mf": {
      const names = partNames();
      if (!names.length) {
        store.setState({ notice: t("parts.none") });
        return;
      }
      return exportParts(c === "parts-stl" ? "stl" : c === "parts-step" ? "step" : "3mf", names, c !== "parts-3mf");
    }
  }
}

// ---------------------------------------------------------------- mode of the view

/** Mode of the 3D view (exactly one): model, section, sketch or measure. */
export type ViewMode = "model" | "section" | "sketch" | "measure";

export function viewMode(s: { sketch: unknown; measure: unknown; section: { on: boolean } }): ViewMode {
  return s.sketch ? "sketch" : s.measure ? "measure" : s.section.on ? "section" : "model";
}

/**
 * Switches the mode. "Modell" ends everything; "Schnitt" ends sketch and measure and turns the
 * section on; "Skizze" and "Messen" keep a section (measuring "Im Schnitt", drawing in a cut
 * view) – after "Fertig" the view is back in "Schnitt".
 */
export function setViewMode(m: ViewMode, sectionOn: (on: boolean) => void): void {
  const s = store.getState();
  if (m === "model" || m === "section") {
    if (s.sketch) sketch.exit();
    if (s.measure) measure.exit();
    sectionOn(m === "section");
    return;
  }
  if (m === "sketch") {
    if (!s.sketch) sketch.enter();
    return;
  }
  if (!s.measure) measure.enter();
}

// The measure mode shows its list in the side panel; the module context its parameters.
let tabBeforeParams: SideTab | null = null;
store.subscribe((s, p) => {
  if (s.measure && !p.measure) setSideTab("measure");
  const mod = s.shown ? s.shown.context.module !== null : false;
  const was = p.shown ? p.shown.context.module !== null : false;
  if (mod && !was) {
    if (s.sideTab !== "params") tabBeforeParams = s.sideTab;
    store.setState({ sideTab: "params", sideOpen: true });
  } else if (!mod && was && s.sideTab === "params") {
    store.setState({ sideTab: tabBeforeParams ?? "parts" });
    tabBeforeParams = null;
  }
});

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
      lang: getLang(),
      langPref: s.langPref,
      colorsOpen: colorsVisible(s),
      sideOpen: s.sideOpen,
      sideTab: s.sideTab,
      exportChoice: s.exportChoice,
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
  /** Adds a file (text; fonts as data URL) to the project as the upload does (tests). */
  addProjectData(path: string, text: string) {
    const f = new File([text], path);
    return uploadProjectFiles([f]);
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
  /** Assertion results (Plan 21 C), the overlay as drawn and the export dialog. */
  asserts() {
    const st = store.getState();
    return {
      version: st.asserts?.version ?? null,
      completeVersion: st.complete?.version ?? null,
      items: (st.asserts?.items ?? []).map((r) => ({
        kind: r.kind,
        line: r.range.line,
        ok: r.ok,
        checked: r.checked,
        bodies: r.bodies,
        pairs: r.pairs.map((p) => ({ ok: p.ok, problem: p.problem, message: p.message, volume: p.volume, distance: p.distance, contact: p.contact, contact_area: p.contact_area, focus: p.focus })),
      })),
      view: viewer?.assertView() ?? null,
      showKeepout: st.showKeepout,
      focus: st.assertFocus,
      exportConfirm: st.exportConfirm ? { what: st.exportConfirm.what, violations: st.exportConfirm.violations } : null,
    };
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
  /** Sketch editor: mode, tool, picked points, the last answer (snap, preview, prompt). */
  sketch() {
    const s = store.getState().sketch;
    if (!s) return null;
    const a = s.answer;
    const snap = a?.snap ? { ...a.snap, screen: viewer?.project(a.snap.point) ?? null } : null;
    return {
      tool: s.tool,
      points: s.points,
      message: s.message,
      actions: s.actions,
      answers: sketch.answers,
      prompt: a?.prompt ?? null,
      sketch: a?.sketch ? { line: a.sketch.line, endLine: a.sketch.end_line } : null,
      node: a?.node ?? null,
      plane: a?.plane ?? null,
      current: a?.current ?? null,
      snap,
      preview: a?.preview.length ?? 0,
      guides: a?.guides.length ?? 0,
      segment: a?.segment ? { line: a.segment.line } : null,
      raster: a?.raster ? { lines: a.raster.lines.length, points: a.raster.points.length, min: a.raster.min, max: a.raster.max } : null,
      drawn: viewer?.lastSketchDraw ?? {},
      rtt: sketch.rtt,
    };
  },
  sketchEnter(tool?: string) {
    return sketch.enter((tool ?? "line") as import("../engine/protocol").SketchTool);
  },
  sketchTool(tool: string) {
    sketch.setTool(tool as import("../engine/protocol").SketchTool);
  },
  sketchExit() {
    sketch.exit();
  },
  /** Measure mode: kind, picks, hover, prompt, section, the measurements and their labels. */
  measure() {
    const st = store.getState();
    const m = st.measure;
    const view = viewer?.measureView() ?? null;
    return {
      active: m !== null,
      kind: m?.kind ?? null,
      inSection: m?.inSection ?? false,
      picks: (m?.picks ?? []).map((p) => ({ kind: p.kind, label: p.label, point: p.point, reference: p.reference, origin: p.origin })),
      hover: m?.hover ? { kind: m.hover.kind, label: m.hover.label, point: m.hover.point, reference: m.hover.reference, origin: m.hover.origin, lines: m.hover.lines.length } : null,
      prompt: m?.prompt ?? null,
      message: m?.message ?? null,
      section: m?.section ?? null,
      answers: measure.answers,
      rtt: measure.rtt,
      selected: st.measureSelected,
      results: st.measurements.map((e) => ({ id: e.id, kind: e.m.kind, value: e.m.value, unit: e.m.unit, label: e.m.label, text: e.m.text, cli: e.m.cli, exact: e.m.exact, values: e.m.values, details: e.m.details, at: e.m.at, version: e.version, lines: e.m.lines.length, guides: e.m.guides.length, origin: e.m.picks.flatMap((p) => p.origin) })),
      view,
    };
  },
  measureEnter(kind?: string, inSection?: boolean) {
    measure.enter((kind ?? "distance") as import("../engine/protocol").MeasureKind, inSection);
  },
  measureKind(kind: string) {
    measure.setKind(kind as import("../engine/protocol").MeasureKind);
  },
  measureExit() {
    measure.exit();
  },
  /** Client position of a context point (measure tests). */
  screenOf(p: [number, number, number]) {
    if (!viewer) return null;
    const q = viewer.project(p);
    const free = document.elementFromPoint(q.x, q.y)?.getAttribute("data-testid") === "viewport-canvas";
    return { ...q, visible: q.visible && free };
  },
  /** Screen position of a point of the sketch plane. */
  sketchScreen(x: number, y: number) {
    return viewer?.sketchScreen([x, y]) ?? null;
  },
  /** Camera position and view direction (sketch plane alignment). */
  camera() {
    if (!viewer) return null;
    const c = viewer.camera.position;
    return { eye: [c.x, c.y, c.z], dir: viewer.viewDir() };
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
      notes: r.notes ?? [],
      objects: [...t.matchAll(/<object id="\d+" type="model" name="([^"]*)">\n   <components>/g)].map((m) => m[1]),
      mColors: count(/<m:color /g),
      items: count(/<item /g),
      components: count(/<component /g),
      modelSettings: t.includes("Metadata/model_settings.config"),
      extruders: [...t.matchAll(/<metadata key="extruder" value="(\d+)"\/>/g)].map((m) => Number(m[1])),
      files: [...t.matchAll(/([A-Za-z0-9_-]+_F\d+_[A-Za-z0-9_-]+\.stl)/g)].map((m) => m[1]).filter((v, i, a) => a.indexOf(v) === i),
    };
  },
  /** Chooses the language (System/English/Deutsch) like the settings. */
  setLangPref(p: "system" | "en" | "de") {
    setLangPref(p);
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
