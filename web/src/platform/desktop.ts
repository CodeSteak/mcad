// Desktop app (Tauri): commands and events of crates/cad-app. In the browser `IS_TAURI` is
// false and nothing here is called.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export const IS_TAURI = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** A project on disk (paths relative to `root`). */
export interface DiskProject {
  root: string;
  name: string;
  main: string;
  active: string;
  files: Record<string, string>;
  folder: boolean;
}

export interface Hunk {
  kind: "added" | "modified" | "deleted";
  /** 1-based lines of the buffer (deleted: removed before `start`). */
  start: number;
  end: number;
  old: string[];
}

export interface RecentEntry {
  path: string;
  folder: boolean;
}

export interface UiRequest {
  id: number;
  method: "status" | "show" | "screenshot";
  body: unknown;
}

export const desktop = {
  initialProject: () => invoke<DiskProject | null>("initial_project"),
  openDialog: (folder: boolean) => invoke<DiskProject | null>("open_dialog", { folder }),
  openPath: (path: string) => invoke<DiskProject>("open_path", { path }),
  saveFiles: (files: Array<{ path: string; text: string }>) => invoke<{ saved: string[]; errors: string[] }>("save_files", { files }),
  saveAs: (name: string, dir: string | null, text: string) => invoke<string | null>("save_as", { name, dir, text }),
  recent: () => invoke<RecentEntry[]>("recent"),
  watchFiles: (files: Array<{ path: string; text: string | null }>) => invoke<void>("watch_files", { files }),
  gitHunks: (path: string, text: string, refresh: boolean) => invoke<Hunk[] | null>("git_hunks", { path, text, refresh }),
  uiReply: (id: number, result: unknown) => invoke<void>("ui_reply", { id, result }),
  setTitle: (title: string) => invoke<void>("set_title", { title }),
  appInfo: () => invoke<{ pid: number; endpoint: string; version: string; started_ms: number }>("app_info"),
  engineStatus: () => invoke<{ ready: boolean; busy: boolean; restarts: number; blocked_keys: string[] }>("engine_status"),
  onFileChanged: (cb: (e: { path: string; text: string }) => void): Promise<UnlistenFn> => listen<{ path: string; text: string }>("file-changed", (e) => cb(e.payload)),
  onFileRemoved: (cb: (e: { path: string }) => void): Promise<UnlistenFn> => listen<{ path: string }>("file-removed", (e) => cb(e.payload)),
  onUiRequest: (cb: (r: UiRequest) => void): Promise<UnlistenFn> => listen<UiRequest>("ui-request", (e) => cb(e.payload)),
};

/** Normalises a `/` path: no `.` segments, `..` folded. */
export function normalizeAbs(p: string): string {
  const abs = p.startsWith("/");
  const out: string[] = [];
  for (const seg of p.replace(/\\/g, "/").split("/")) {
    if (seg === "" || seg === ".") continue;
    if (seg === "..") {
      if (out.length && out[out.length - 1] !== "..") out.pop();
      else if (!abs) out.push("..");
    } else out.push(seg);
  }
  // Windows drive letters stay as they are (`C:/…`).
  return (abs ? "/" : "") + out.join("/");
}

/** Absolute path of a project file. */
export function absPath(root: string, rel: string): string {
  if (rel.startsWith("/") || /^[A-Za-z]:[\\/]/.test(rel)) return normalizeAbs(rel);
  return normalizeAbs(`${root}/${rel}`);
}

/** Path of `abs` relative to `root` (`null` if outside and not reachable with `..`). */
export function relPath(root: string, abs: string): string {
  const r = normalizeAbs(root).split("/").filter(Boolean);
  const a = normalizeAbs(abs).split("/").filter(Boolean);
  let i = 0;
  while (i < r.length && i < a.length && r[i] === a[i]) i++;
  return [...Array(r.length - i).fill(".."), ...a.slice(i)].join("/");
}
