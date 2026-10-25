// The open project: several files (path → text), the main file and the file shown in the
// editor. The active file's text lives in the editor view; every other file keeps its text and,
// once it was opened, its EditorState (own undo history).

import type { EditorState } from "@codemirror/state";
import { MODEL_EXT } from "../logic/files";

export interface ProjectSnapshot {
  name: string;
  main: string;
  active: string;
  files: Record<string, string>;
  savedAt: number;
}

/** Directory handle of the File System Access API (only the parts used here). */
export interface DirHandle {
  kind: "directory";
  name: string;
  values(): AsyncIterable<DirHandle | FileHandleLike>;
  getDirectoryHandle(name: string, o?: { create?: boolean }): Promise<DirHandle>;
  getFileHandle(name: string, o?: { create?: boolean }): Promise<FileHandleLike>;
}

export interface FileHandleLike {
  kind: "file";
  name: string;
  getFile(): Promise<File>;
  createWritable(): Promise<{ write(d: Blob | string): Promise<void>; close(): Promise<void> }>;
}

class Project {
  name = "Projekt";
  main = `unbenannt${MODEL_EXT}`;
  active = `unbenannt${MODEL_EXT}`;
  /** Texts of all files; the active file's entry may be stale (the editor has it). */
  texts = new Map<string, string>();
  /** Saved editor states of files that are not shown (undo history per file). */
  states = new Map<string, EditorState>();
  /** Folder the project was opened from (File System Access API), for saving back. */
  dir: DirHandle | null = null;
  /** Desktop app: absolute directory the paths are relative to (`null`: not on disk yet). */
  root: string | null = null;
  /** Desktop app: opened as a folder (all model files of the folder). */
  folder = false;
  /** Desktop app: file contents as on disk (loaded or saved last); a file is dirty if its text differs. */
  saved = new Map<string, string>();
  /** Reads the active file's current text from the editor. */
  activeText: () => string | null = () => null;

  paths(): string[] {
    return [...this.texts.keys()].sort((a, b) => (a === this.main ? -1 : b === this.main ? 1 : a.localeCompare(b)));
  }

  has(path: string): boolean {
    return this.texts.has(path);
  }

  text(path: string): string {
    if (path === this.active) {
      const t = this.activeText();
      if (t !== null) return t;
    }
    const s = this.states.get(path);
    if (s) return s.doc.toString();
    return this.texts.get(path) ?? "";
  }

  mainText(): string {
    return this.text(this.main);
  }

  /** All files with current texts. */
  files(): Record<string, string> {
    const out: Record<string, string> = {};
    for (const p of this.texts.keys()) out[p] = this.text(p);
    return out;
  }

  /** Files besides the main file (for `SetFiles`). */
  others(): Record<string, string> {
    const out: Record<string, string> = {};
    for (const p of this.texts.keys()) if (p !== this.main) out[p] = this.text(p);
    return out;
  }

  snapshot(): ProjectSnapshot {
    return { name: this.name, main: this.main, active: this.active, files: this.files(), savedAt: Date.now() };
  }

  /** Files whose text differs from the disk (desktop app; files never saved count). */
  dirty(): string[] {
    if (this.root === null) return [];
    return [...this.texts.keys()].filter((p) => this.saved.get(p) !== this.text(p));
  }

  /** Replaces everything (editor states are dropped). */
  reset(s: { name: string; main: string; active?: string; files: Record<string, string> }, dir: DirHandle | null = null, disk: { root: string; folder: boolean } | null = null): void {
    this.root = disk?.root ?? null;
    this.folder = disk?.folder ?? false;
    this.saved = new Map(disk ? Object.entries(s.files) : []);
    this.name = s.name;
    this.main = s.main;
    this.active = s.active && s.files[s.active] !== undefined ? s.active : s.main;
    this.texts = new Map(Object.entries(s.files));
    if (!this.texts.has(this.main)) this.texts.set(this.main, "");
    this.states.clear();
    this.dir = dir;
  }
}

export const project = new Project();
