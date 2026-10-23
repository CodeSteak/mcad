// Files: autosave in IndexedDB, open/save with the File System Access API (Chromium) or
// upload/download, the example menu (files of examples/), downloads of exports. Projects of
// several files: autosave of the whole project, folders (File System Access API), ZIP upload
// and download.

import { dataFileRefs, fileBytes, fileText, fromDataUrl, guessMain, isProjectFile, normalizePath, resolvePath, stripCommonFolder } from "../logic/files";
import type { DirHandle, FileHandleLike, ProjectSnapshot } from "../state/project";
import { readZip, writeZip } from "./zip";

export interface StoredFile {
  name: string;
  text: string;
  savedAt: number;
}

const DB = "controller-cad";
const STORE = "files";
const AUTOSAVE = "autosave";
const PROJECT = "project";

function openVersion(version?: number): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const req = version === undefined ? indexedDB.open(DB) : indexedDB.open(DB, version);
    req.onupgradeneeded = () => {
      if (!req.result.objectStoreNames.contains(STORE)) req.result.createObjectStore(STORE);
    };
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
}

let dbPromise: Promise<IDBDatabase> | null = null;

/** Opens the database; creates the store with a version upgrade if it is missing. */
function openDb(): Promise<IDBDatabase> {
  dbPromise ??= (async () => {
    const db = await openVersion();
    if (db.objectStoreNames.contains(STORE)) return db;
    const v = db.version + 1;
    db.close();
    return openVersion(v);
  })().catch((e) => {
    dbPromise = null;
    throw e;
  });
  return dbPromise;
}

export async function loadAutosave(): Promise<StoredFile | null> {
  try {
    const db = await openDb();
    return await new Promise((resolve, reject) => {
      const r = db.transaction(STORE, "readonly").objectStore(STORE).get(AUTOSAVE);
      r.onsuccess = () => resolve((r.result as StoredFile | undefined) ?? null);
      r.onerror = () => reject(r.error);
    });
  } catch {
    return null;
  }
}

export async function saveAutosave(f: StoredFile): Promise<void> {
  try {
    const db = await openDb();
    await new Promise<void>((resolve, reject) => {
      const tx = db.transaction(STORE, "readwrite");
      tx.objectStore(STORE).put(f, AUTOSAVE);
      tx.oncomplete = () => resolve();
      tx.onerror = () => reject(tx.error);
    });
  } catch {
    /* private mode etc.: no autosave */
  }
}

/** The autosaved project (or the single file of an older autosave as a project). */
export async function loadProjectAutosave(): Promise<ProjectSnapshot | null> {
  try {
    const db = await openDb();
    const p = await new Promise<ProjectSnapshot | null>((resolve, reject) => {
      const r = db.transaction(STORE, "readonly").objectStore(STORE).get(PROJECT);
      r.onsuccess = () => resolve((r.result as ProjectSnapshot | undefined) ?? null);
      r.onerror = () => reject(r.error);
    });
    if (p && p.files && p.files[p.main] !== undefined) return p;
  } catch {
    /* fall back to the single file */
  }
  const f = await loadAutosave();
  if (!f) return null;
  return { name: f.name, main: f.name, active: f.name, files: { [f.name]: f.text }, savedAt: f.savedAt };
}

export async function saveProjectAutosave(p: ProjectSnapshot): Promise<void> {
  try {
    const db = await openDb();
    await new Promise<void>((resolve, reject) => {
      const tx = db.transaction(STORE, "readwrite");
      tx.objectStore(STORE).put(p, PROJECT);
      // The main file also as the single-file autosave (older versions, tests).
      tx.objectStore(STORE).put({ name: p.main, text: p.files[p.main] ?? "", savedAt: p.savedAt }, AUTOSAVE);
      tx.oncomplete = () => resolve();
      tx.onerror = () => reject(tx.error);
    });
  } catch {
    /* no autosave */
  }
}

// ---------------------------------------------------------------- projects

export interface LoadedProject {
  name: string;
  main: string;
  files: Record<string, string>;
  dir: DirHandle | null;
}

/** Largest number of files read from a folder or ZIP. */
const MAX_PROJECT_FILES = 250;

type DirWindow = Window & { showDirectoryPicker?: (o?: unknown) => Promise<DirHandle> };

export function hasDirectoryAccess(): boolean {
  return typeof (window as DirWindow).showDirectoryPicker === "function";
}

/** All .scad files of a folder and the data files (SVG, fonts; recursive, without hidden folders). */
async function readDir(dir: DirHandle, prefix = "", out: Record<string, string> = {}): Promise<Record<string, string>> {
  for await (const h of dir.values()) {
    if (Object.keys(out).length >= MAX_PROJECT_FILES) break;
    if (h.name.startsWith(".")) continue;
    if (h.kind === "directory") await readDir(h, `${prefix}${h.name}/`, out);
    else if (isProjectFile(h.name)) out[`${prefix}${h.name}`] = fileText(h.name, new Uint8Array(await (await h.getFile()).arrayBuffer()));
  }
  return out;
}

/** Opens a folder as a project (File System Access API); `null` if cancelled. */
export async function openFolder(): Promise<LoadedProject | null> {
  const w = window as DirWindow;
  if (!w.showDirectoryPicker) return null;
  let dir: DirHandle;
  try {
    dir = await w.showDirectoryPicker({ mode: "readwrite" });
  } catch (e) {
    if ((e as DOMException).name === "AbortError") return null;
    throw e;
  }
  const files = await readDir(dir);
  const main = guessMain(files);
  if (!main) throw new Error(`Ordner ${dir.name} enthält keine .scad-Datei`);
  return { name: dir.name, main, files, dir };
}

/** Writes all files back into the project folder. */
export async function saveToFolder(dir: DirHandle, files: Record<string, string>): Promise<number> {
  let n = 0;
  for (const [path, text] of Object.entries(files)) {
    const parts = normalizePath(path).split("/");
    if (parts.some((p) => p === ".." || p === "")) continue;
    let d = dir;
    for (const seg of parts.slice(0, -1)) d = await d.getDirectoryHandle(seg, { create: true });
    const fh: FileHandleLike = await d.getFileHandle(parts[parts.length - 1], { create: true });
    const out = await fh.createWritable();
    // binary files (fonts) are data URLs in the project: write their bytes, texts as text
    const bin = fromDataUrl(text);
    await out.write(bin ? new Blob([bin as BlobPart]) : text);
    await out.close();
    n++;
  }
  return n;
}

/** A project from ZIP bytes (.scad files and data files; a common top folder is removed). */
export async function projectFromZip(name: string, bytes: Uint8Array): Promise<LoadedProject> {
  const entries = await readZip(bytes);
  let files: Record<string, string> = {};
  for (const e of entries) {
    if (!isProjectFile(e.name) || e.name.split("/").some((s) => s.startsWith(".") || s === "__MACOSX")) continue;
    files[normalizePath(e.name)] = fileText(e.name, e.data);
    if (Object.keys(files).length >= MAX_PROJECT_FILES) break;
  }
  const stripped = stripCommonFolder(files);
  files = stripped.files;
  const main = guessMain(files);
  if (!main) throw new Error(`${name} enthält keine .scad-Datei`);
  return { name: stripped.folder ?? name.replace(/\.zip$/i, ""), main, files, dir: null };
}

/** Lets the user pick a ZIP file; `null` if cancelled. */
export function pickZip(): Promise<LoadedProject | null> {
  return new Promise((resolve, reject) => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".zip,application/zip";
    input.dataset.testid = "zip-input";
    input.style.display = "none";
    document.body.appendChild(input);
    input.onchange = async () => {
      const f = input.files?.[0];
      input.remove();
      if (!f) return resolve(null);
      try {
        resolve(await projectFromZip(f.name, new Uint8Array(await f.arrayBuffer())));
      } catch (e) {
        reject(e);
      }
    };
    input.click();
  });
}

/** ZIP of the project's files (a top folder named after the project). */
export function projectZip(name: string, files: Record<string, string>): Uint8Array {
  const folder = name.replace(/[^\w.-]+/g, "_") || "projekt";
  return writeZip(Object.keys(files).sort().map((p) => ({ name: `${folder}/${p}`, data: fileBytes(files[p]) })));
}

// ---------------------------------------------------------------- examples

const raw = import.meta.glob("../../../examples/**/*.scad", { query: "?raw", import: "default", eager: true }) as Record<string, string>;
/** SVGs of examples/svg (imported by spec examples). */
const rawSvg = import.meta.glob("../../../examples/**/*.svg", { query: "?raw", import: "default", eager: true }) as Record<string, string>;
const EXAMPLE_DATA: Record<string, string> = Object.fromEntries(Object.entries(rawSvg).map(([p, t]) => [p.replace(/^.*examples\//, ""), t]));

/**
 * Data files an example imports (paths relative to examples/), e.g. `spec/svg_logo.scad` →
 * `{"svg/logo.svg": …}`; empty if it imports none.
 */
export function exampleData(path: string, text: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const rel of dataFileRefs(text)) {
    const p = resolvePath(path, rel);
    if (EXAMPLE_DATA[p] !== undefined) out[p] = EXAMPLE_DATA[p];
  }
  return out;
}

export interface Example {
  group: string;
  name: string;
  path: string;
  text: string;
}

export const EXAMPLES: Example[] = Object.entries(raw)
  .map(([p, text]) => {
    const rel = p.replace(/^.*examples\//, "");
    const [group, ...rest] = rel.split("/");
    return { group, name: rest.join("/"), path: rel, text };
  })
  .sort((a, b) => (a.group === b.group ? a.name.localeCompare(b.name) : a.group === "spec" ? -1 : b.group === "spec" ? 1 : a.group.localeCompare(b.group)));

export const GROUP_LABELS: Record<string, string> = { spec: "Spec-Beispiele", "openscad-compat": "OpenSCAD-Kompatibilität" };

/** Folders of examples/ besides the single-file groups: projects of several files. */
const SINGLE_GROUPS = new Set(["spec", "openscad-compat"]);

export interface ExampleProject {
  /** Folder below examples/. */
  name: string;
  main: string;
  files: Record<string, string>;
}

export const EXAMPLE_PROJECTS: ExampleProject[] = (() => {
  const by = new Map<string, Record<string, string>>();
  for (const e of EXAMPLES) {
    if (SINGLE_GROUPS.has(e.group)) continue;
    const m = by.get(e.group) ?? {};
    m[e.name] = e.text;
    by.set(e.group, m);
  }
  return [...by.entries()].map(([name, files]) => ({ name, files, main: guessMain(files) ?? Object.keys(files)[0] }));
})();

/** Examples shown as single files (not the files of example projects). */
export const SINGLE_EXAMPLES: Example[] = EXAMPLES.filter((e) => SINGLE_GROUPS.has(e.group));


// ---------------------------------------------------------------- open / save

interface FsHandle {
  name: string;
  getFile(): Promise<File>;
  createWritable(): Promise<{ write(d: Blob | string): Promise<void>; close(): Promise<void> }>;
}

type PickerWindow = Window & {
  showOpenFilePicker?: (o?: unknown) => Promise<FsHandle[]>;
  showSaveFilePicker?: (o?: unknown) => Promise<FsHandle>;
};

const pickerTypes = [{ description: "CAD-Quelltext", accept: { "text/plain": [".scad"] } }];

let handle: FsHandle | null = null;

export function hasFileSystemAccess(): boolean {
  return typeof (window as PickerWindow).showOpenFilePicker === "function";
}

/** Opens a file; `null` if cancelled. */
export async function openFile(): Promise<{ name: string; text: string } | null> {
  const w = window as PickerWindow;
  if (w.showOpenFilePicker) {
    try {
      const [h] = await w.showOpenFilePicker({ types: pickerTypes, multiple: false });
      handle = h;
      const f = await h.getFile();
      return { name: f.name, text: await f.text() };
    } catch (e) {
      if ((e as DOMException).name === "AbortError") return null;
      throw e;
    }
  }
  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".scad,text/plain";
    input.onchange = async () => {
      const f = input.files?.[0];
      resolve(f ? { name: f.name, text: await f.text() } : null);
    };
    input.click();
  });
}

/** Saves to the opened file (or asks for one); falls back to a download. Returns the name. */
export async function saveFile(name: string, text: string, saveAs = false): Promise<string | null> {
  const w = window as PickerWindow;
  if (w.showSaveFilePicker) {
    try {
      if (!handle || saveAs) handle = await w.showSaveFilePicker({ suggestedName: name, types: pickerTypes });
      const out = await handle.createWritable();
      await out.write(text);
      await out.close();
      return handle.name;
    } catch (e) {
      if ((e as DOMException).name === "AbortError") return null;
      throw e;
    }
  }
  download(new Blob([text], { type: "text/plain" }), name);
  return name;
}

export function forgetHandle(): void {
  handle = null;
}

export function download(blob: Blob, name: string): void {
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 10_000);
}

export function baseName(name: string): string {
  return name.replace(/\.[^.]*$/, "") || "modell";
}
