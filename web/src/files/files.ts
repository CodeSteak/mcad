// Files: autosave in IndexedDB, open/save with the File System Access API (Chromium) or
// upload/download, the example menu (files of examples/), downloads of exports.

export interface StoredFile {
  name: string;
  text: string;
  savedAt: number;
}

const DB = "controller-cad";
const STORE = "files";
const AUTOSAVE = "autosave";

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

// ---------------------------------------------------------------- examples

const raw = import.meta.glob("../../../examples/**/*.scad", { query: "?raw", import: "default", eager: true }) as Record<string, string>;

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
