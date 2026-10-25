// Desktop app (Tauri): real files, file watcher, git markers, autosave and the control
// requests of `mcad ui …`. In the browser nothing here runs (`IS_TAURI` false).
//
// * Dirty state: `project.saved` holds each file as on disk; a file whose text differs is
//   dirty (tab marker ●, window title).
// * External change (event `file-changed` from the app's watcher; own saves never come back):
//   a clean file is reloaded silently as minimal line changes (cursor and scroll stay) in one
//   undo step of its own; a dirty one gets the bar "Datei wurde extern geändert: Neu laden /
//   Meine behalten / Vergleichen". A deleted or renamed file gets a bar too; saving writes it
//   again.
// * Git: after typing (300 ms), saving, external changes and window focus the app diffs the
//   buffer against HEAD; the gutter shows the hunks (editor/gitGutter.ts).

import { isolateHistory } from "@codemirror/commands";
import { Transaction } from "@codemirror/state";
import { setHunks } from "../editor/gitGutter";
import { definitionTarget } from "../logic/context";
import { lineChanges } from "../logic/textDiff";
import { type DiskProject, IS_TAURI, absPath, desktop, normalizeAbs } from "../platform/desktop";
import { bridge } from "../state/bridge";
import { project } from "../state/project";
import { store } from "../state/store";
import { sectionCli } from "./section";
import { editTarget, getViewer, loadProject, sendNow, switchFile } from "./runtime";

let gitTimer: ReturnType<typeof setTimeout> | null = null;
let autoTimer: ReturnType<typeof setTimeout> | null = null;

/** Relative project path of an absolute path (null: not in the project). */
export function relOf(abs: string): string | null {
  if (project.root === null) return null;
  const a = normalizeAbs(abs);
  for (const p of project.texts.keys()) if (absPath(project.root, p) === a) return p;
  return null;
}

function title(): string {
  const dirty = project.dirty().length > 0;
  return `${dirty ? "● " : ""}${project.root ? project.name : `${project.name} (nicht gespeichert)`} – mcad`;
}

/** Publishes the dirty files (tabs, title). */
export function publishDirty(): void {
  if (!IS_TAURI) return;
  const d = project.dirty();
  const cur = store.getState().dirty;
  if (d.join("\n") !== cur.join("\n")) store.setState({ dirty: d });
  const t = title();
  if (t !== lastTitle) {
    lastTitle = t;
    void desktop.setTitle(t).catch(() => {});
  }
}
let lastTitle = "";

/** Watches the project files (contents as on disk). */
export function watchProject(): void {
  if (!IS_TAURI || project.root === null) {
    if (IS_TAURI) void desktop.watchFiles([]).catch(() => {});
    return;
  }
  const root = project.root;
  const files = [...project.texts.keys()].map((p) => ({ path: absPath(root, p), text: project.saved.get(p) ?? null }));
  void desktop.watchFiles(files).catch(() => {});
}

/** Git hunks of the active file (`refresh`: read HEAD again). */
export function requestGit(refresh = false): void {
  if (!IS_TAURI) return;
  const v = bridge.view;
  if (!v) return;
  if (project.root === null) {
    v.dispatch({ effects: setHunks.of(null) });
    return;
  }
  const file = project.active;
  const text = bridge.text();
  void desktop
    .gitHunks(absPath(project.root, file), text, refresh)
    .then((h) => {
      if (project.active !== file || !bridge.view) return;
      bridge.view.dispatch({ effects: setHunks.of(h) });
      store.setState({ gitVersion: store.getState().gitVersion + 1 });
    })
    .catch(() => {});
}

/** After every edit (desktop): dirty state, git markers (debounced), autosave. */
export function onEdit(): void {
  if (!IS_TAURI) return;
  publishDirty();
  if (gitTimer) clearTimeout(gitTimer);
  gitTimer = setTimeout(() => {
    gitTimer = null;
    requestGit(false);
  }, 300);
  const s = store.getState().settings;
  if (s.autoSave && project.root !== null) {
    if (autoTimer) clearTimeout(autoTimer);
    autoTimer = setTimeout(() => {
      autoTimer = null;
      if (project.dirty().length) void saveProject(false, true);
    }, Math.max(100, s.autoSaveMs));
  }
}

/** A project was loaded (desktop): watch it, markers, title. */
export function projectLoaded(): void {
  if (!IS_TAURI) return;
  store.setState({ external: {}, diffView: null });
  watchProject();
  publishDirty();
  requestGit(true);
  void desktop
    .recent()
    .then((r) => store.setState({ recent: r }))
    .catch(() => {});
}

export function diskToLoad(p: DiskProject) {
  return { project: { name: p.name, main: p.main, active: p.active, files: p.files }, disk: { root: p.root, folder: p.folder } };
}

/** Opens a file or folder (dialog) in the desktop app. */
export async function openDialog(folder: boolean): Promise<void> {
  try {
    const p = await desktop.openDialog(folder);
    if (p) openDisk(p);
  } catch (e) {
    store.setState({ notice: `Öffnen fehlgeschlagen: ${String(e)}` });
  }
}

export function openDisk(p: DiskProject): void {
  const d = diskToLoad(p);
  loadProject(d.project, null, d.disk);
  store.setState({ notice: p.folder ? `Ordner ${p.root}: ${Object.keys(p.files).length} Dateien, Hauptdatei ${p.main}` : `Geöffnet: ${absPath(p.root, p.main)}` });
}

export async function openRecent(path: string): Promise<void> {
  try {
    openDisk(await desktop.openPath(path));
  } catch (e) {
    store.setState({ notice: `Öffnen fehlgeschlagen: ${String(e)}` });
  }
}

function dirname(p: string): string {
  const i = p.replace(/\\/g, "/").lastIndexOf("/");
  return i <= 0 ? "/" : p.slice(0, i);
}

function basename(p: string): string {
  return p.replace(/\\/g, "/").split("/").pop() ?? p;
}

/**
 * Saves the dirty project files (all of them, atomically). Without a location (new file) or
 * with `saveAs`: dialog for the active file; a single-file project then lives there.
 */
export async function saveProject(saveAs = false, auto = false): Promise<void> {
  const v = bridge.view;
  if (!v) return;
  try {
    if (project.root === null || saveAs) {
      const text = bridge.text();
      const path = await desktop.saveAs(basename(project.active), project.root, text);
      if (!path) return;
      if (!project.folder && project.texts.size === 1) {
        // The single file moves: the project follows (new root and name).
        const name = basename(path);
        const files = { [name]: text };
        const keepState = v.state;
        project.reset({ name, main: name, active: name, files }, null, { root: dirname(path), folder: false });
        project.saved.set(name, text);
        v.setState(keepState);
        store.setState({ project: { ...store.getState().project, name, main: name, active: name, paths: [name] }, fileName: name });
        projectLoaded();
      }
      store.setState({ notice: `Gespeichert: ${path}` });
      publishDirty();
      return;
    }
    const root = project.root;
    const dirty = project.dirty();
    const removed = Object.entries(store.getState().external).filter(([, e]) => e.kind === "removed").map(([p]) => p);
    const paths = [...new Set([...dirty, ...removed])];
    if (!paths.length) {
      if (!auto) store.setState({ notice: "Keine Änderungen zu speichern" });
      return;
    }
    const files = paths.map((p) => ({ path: absPath(root, p), text: project.text(p) }));
    const r = await desktop.saveFiles(files);
    for (const f of files) {
      if (!r.saved.some((s) => normalizeAbs(s) === normalizeAbs(f.path))) continue;
      const rel = relOf(f.path);
      if (rel) project.saved.set(rel, f.text);
    }
    const ext = { ...store.getState().external };
    for (const p of paths) delete ext[p];
    store.setState({ external: ext, notice: r.errors.length ? `Speichern fehlgeschlagen: ${r.errors.join("; ")}` : `${auto ? "Automatisch gespeichert" : "Gespeichert"}: ${paths.join(", ")}` });
    publishDirty();
    requestGit(true);
  } catch (e) {
    store.setState({ notice: `Speichern fehlgeschlagen: ${String(e)}` });
  }
}

/** Replaces a file's text with `text` as minimal changes in an undo step of its own. */
function applyText(rel: string, text: string): void {
  const annotations = [Transaction.userEvent.of("input.external"), isolateHistory.of("full")];
  if (rel === project.active && bridge.view) {
    const v = bridge.view;
    const changes = lineChanges(v.state.doc.toString(), text);
    if (changes.length) v.dispatch({ changes, annotations });
    return;
  }
  const t = editTarget(rel);
  if (!t) return;
  const changes = lineChanges(t.state.doc.toString(), text);
  if (changes.length) {
    t.dispatch(t.state.update({ changes, annotations }));
    sendNow();
  }
}

/** Reloads a file from the text on disk (silent reload, "Neu laden"). */
export function reloadFile(rel: string, text: string, quiet = false): void {
  applyText(rel, text);
  project.saved.set(rel, text);
  const ext = { ...store.getState().external };
  delete ext[rel];
  store.setState({ external: ext, ...(quiet ? { notice: `Extern geändert: ${rel} – neu geladen` } : {}) });
  publishDirty();
  if (rel === project.active) requestGit(true);
}

/** "Meine behalten": the disk state is known, the buffer stays (and stays dirty). */
export function keepMine(rel: string): void {
  const e = store.getState().external[rel];
  if (e?.kind === "changed" && e.text !== undefined) project.saved.set(rel, e.text);
  const ext = { ...store.getState().external };
  delete ext[rel];
  store.setState({ external: ext, diffView: null });
  publishDirty();
}

export function compare(rel: string): void {
  const e = store.getState().external[rel];
  if (e?.kind !== "changed" || e.text === undefined) return;
  store.setState({ diffView: { path: rel, disk: e.text, mine: project.text(rel) } });
}

/** Removes a deleted file from the project (or keeps it as unsaved buffer). */
export function closeRemoved(rel: string): void {
  const ext = { ...store.getState().external };
  delete ext[rel];
  store.setState({ external: ext });
  if (rel !== project.main) {
    if (project.active === rel) switchFile(project.main);
    project.texts.delete(rel);
    project.states.delete(rel);
    project.saved.delete(rel);
    store.setState({ project: { ...store.getState().project, paths: project.paths() } });
    sendNow();
  }
  publishDirty();
}

function onChanged(path: string, text: string): void {
  const rel = relOf(path);
  if (!rel) return;
  const cur = project.text(rel);
  if (cur === text) {
    project.saved.set(rel, text);
    wasDirtyBeforeRemoval.delete(rel);
    const ext = { ...store.getState().external };
    delete ext[rel];
    store.setState({ external: ext });
    publishDirty();
    return;
  }
  // A deleted file that comes back: dirty if it had local changes before.
  const known = project.saved.get(rel);
  const dirty = known === undefined ? wasDirtyBeforeRemoval.has(rel) : known !== cur;
  wasDirtyBeforeRemoval.delete(rel);
  if (!dirty) {
    reloadFile(rel, text, true);
    return;
  }
  store.setState({ external: { ...store.getState().external, [rel]: { kind: "changed", text } } });
  publishDirty();
}

/** Files that had local changes when they were deleted (a recreated file asks then). */
const wasDirtyBeforeRemoval = new Set<string>();

function onRemoved(path: string): void {
  const rel = relOf(path);
  if (!rel) return;
  if (project.saved.get(rel) !== project.text(rel)) wasDirtyBeforeRemoval.add(rel);
  project.saved.delete(rel);
  store.setState({ external: { ...store.getState().external, [rel]: { kind: "removed" } } });
  publishDirty();
}

// ---------------------------------------------------------------- `mcad ui …` requests

/** What `mcad ui status` reports for this window. */
export function windowStatus(): unknown {
  const st = store.getState();
  const doc = bridge.doc();
  const cur = Math.min(st.cursor, doc.length);
  const line = doc.lineAt(cur);
  const shown = st.shown;
  const sel = getViewer()?.lastSelection;
  const nodes = shown && sel ? sel.nodes.slice(0, 50).map((n) => ({ idx: n, label: shown.nodes[n]?.label ?? "", line: shown.nodes[n]?.range.line ?? null })) : [];
  const params: Record<string, unknown> = {};
  for (const p of shown?.context.params ?? []) params[p.name] = p.value;
  return {
    label: "main",
    project: { name: project.name, root: project.root, main: project.main, active: project.active, files: project.paths(), folder: project.folder },
    dirty: project.dirty(),
    cursor: { file: project.active, line: line.number, col: cur - line.from + 1 },
    selection: { line: line.number, nodes },
    context: shown?.context.module ?? null,
    params,
    overridden: (shown?.context.params ?? []).filter((p) => p.overridden).map((p) => p.name),
    diagnostics: st.diagnostics.items,
    scene: shown ? { version: shown.version, complete: shown.complete, nodes: shown.nodes.length, parts: (shown.parts ?? []).map((p) => p.name) } : null,
    section: st.section.on ? { point: st.section.point, normal: st.section.normal, flip: st.section.flip, cli: sectionCli() } : null,
    external: Object.keys(st.external),
    engine_error: st.engineError?.message ?? null,
  };
}

function waitFor(pred: () => boolean, ms: number): Promise<boolean> {
  return new Promise((res) => {
    const t0 = performance.now();
    const tick = () => {
      if (pred()) return res(true);
      if (performance.now() - t0 > ms) return res(false);
      setTimeout(tick, 50);
    };
    tick();
  });
}

async function show(body: { file: string; line?: number | null; module?: string | null }): Promise<unknown> {
  let rel = relOf(body.file);
  if (!rel) {
    // Another file: open it (as its own project) – never over unsaved changes.
    const dirty = project.dirty();
    if (dirty.length) throw new Error(`ungespeicherte Änderungen in ${dirty.join(", ")}: Datei nicht geöffnet`);
    openDisk(await desktop.openPath(body.file));
    rel = relOf(body.file);
  } else if (rel !== project.active) switchFile(rel);
  if (body.module) {
    await waitFor(() => {
      const st = store.getState();
      return !!st.scene && st.scene.version === st.sentVersion;
    }, 10_000);
    const sc = store.getState().scene;
    const m = sc?.modules.find(
      (x) => x.name === body.module || [...x.path, x.name].join("::") === body.module || [...x.path, x.name].join(".") === body.module,
    );
    if (!m || !sc) throw new Error(`Modul ${body.module} nicht gefunden`);
    const idx = bridge.index(sc);
    if (!body.line) {
      if (idx.inFile(m.body_range)) bridge.setCursor(definitionTarget(bridge.text(), idx.range(m.body_range)));
      else throw new Error(`Modul ${body.module} liegt in einer anderen Datei`);
    }
  }
  if (body.line) bridge.setCursorLine(body.line);
  await waitFor(() => {
    const s = store.getState();
    return !!s.shown && s.shown.version === s.sentVersion && s.shown.complete && (getViewer()?.viewState().missingMeshes ?? 0) === 0;
  }, 15_000);
  const doc = bridge.doc();
  return { shown: true, file: rel, line: doc.lineAt(Math.min(store.getState().cursor, doc.length)).number, module: store.getState().shown?.context.module ?? null };
}

function screenshot(): unknown {
  const v = getViewer();
  if (!v) throw new Error("keine 3D-Ansicht");
  v.render();
  const c = v.renderer.domElement;
  return { png: c.toDataURL("image/png"), width: c.width, height: c.height };
}

async function handleRequest(id: number, method: string, body: unknown): Promise<void> {
  let result: unknown;
  try {
    if (method === "status") result = windowStatus();
    else if (method === "show") result = await show(body as { file: string; line?: number; module?: string });
    else if (method === "screenshot") result = screenshot();
    else result = { error: `unbekannt: ${method}` };
  } catch (e) {
    result = { error: (e as Error).message ?? String(e) };
  }
  await desktop.uiReply(id, result);
}

let started = false;

/** Registers the desktop listeners (once, from runtime). */
export function initDesktop(): void {
  if (!IS_TAURI || started) return;
  started = true;
  void desktop.onFileChanged((e) => onChanged(e.path, e.text));
  void desktop.onFileRemoved((e) => onRemoved(e.path));
  void desktop.onUiRequest((r) => void handleRequest(r.id, r.method, r.body));
  window.addEventListener("focus", () => requestGit(true));
}
