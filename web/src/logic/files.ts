// Project files (several .scad files): paths, which scene file ids belong to a path, the main
// file of a folder, include/use statements in a text. Pure functions (tested in files.test.ts).

import type { Diagnostic, Scene, SrcRange } from "../engine/protocol";

/** `/` separators, no `.` segments, `..` folded where possible (as the engine does). */
export function normalizePath(p: string): string {
  const s = p.replace(/\\/g, "/");
  const abs = s.startsWith("/");
  const out: string[] = [];
  for (const seg of s.split("/")) {
    if (seg === "" || seg === ".") continue;
    if (seg === "..") {
      if (out.length && out[out.length - 1] !== "..") out.pop();
      else if (!abs) out.push("..");
      continue;
    }
    out.push(seg);
  }
  return (abs ? "/" : "") + out.join("/");
}

/** Path of `rel` (as written in `include <rel>`) seen from the file `from`. */
export function resolvePath(from: string, rel: string): string {
  const r = rel.trim();
  if (r.startsWith("/") || r.startsWith("\\")) return normalizePath(r);
  const f = from.replace(/\\/g, "/");
  const i = f.lastIndexOf("/");
  return normalizePath(i >= 0 ? `${f.slice(0, i)}/${r}` : r);
}

/** Directory part of a path (`""` at the root). */
export function dirName(p: string): string {
  const i = p.lastIndexOf("/");
  return i >= 0 ? p.slice(0, i) : "";
}

export function baseName(p: string): string {
  return p.slice(p.lastIndexOf("/") + 1);
}

/** Scene file ids of a path (a file included twice has two ids). File 0 is the main file. */
export function fileIds(scene: Scene, path: string, main: string): Set<number> {
  const ids = new Set<number>();
  if (!scene.files || scene.files.length === 0) {
    if (path === main) ids.add(0);
    return ids;
  }
  for (const f of scene.files) if (f.path === path) ids.add(f.id);
  return ids;
}

/** Path of the file a scene range is in. */
export function rangePath(scene: Scene, r: SrcRange, main: string): string {
  const id = r.file ?? 0;
  if (id === 0) return main;
  return scene.files?.[id]?.path ?? main;
}

/** Path of the file a diagnostic is in. */
export function diagnosticPath(d: Diagnostic, main: string): string {
  return d.file ?? main;
}

export interface IncludeStmt {
  /** Offsets of `<path>` in the text. */
  from: number;
  to: number;
  path: string;
  isUse: boolean;
}

/** `include <…>` / `use <…>` statements of a text (outside comments). */
export function includeStatements(text: string): IncludeStmt[] {
  const out: IncludeStmt[] = [];
  const re = /\/\/[^\n]*|\/\*[\s\S]*?\*\/|"(?:[^"\\\n]|\\.)*"|\b(include|use)\s*<([^>\n]*)>/g;
  for (let m = re.exec(text); m; m = re.exec(text)) {
    if (!m[1]) continue;
    const lt = m.index + m[0].indexOf("<");
    out.push({ from: lt, to: m.index + m[0].length, path: m[2], isUse: m[1] === "use" });
  }
  return out;
}

/** Include/use statement whose `<path>` contains `pos`. */
export function includeAt(text: string, pos: number): IncludeStmt | null {
  return includeStatements(text).find((s) => s.from <= pos && pos <= s.to) ?? null;
}

/**
 * Main file of a set of files: among the files at the top folder level the one no other file
 * includes or uses (preferring one that includes or uses something); else the first.
 */
export function guessMain(files: Record<string, string>): string | null {
  const paths = Object.keys(files).filter((p) => p.endsWith(".scad")).sort();
  if (!paths.length) return null;
  const referenced = new Set<string>();
  const refers = new Set<string>();
  for (const p of paths) {
    for (const s of includeStatements(files[p])) {
      referenced.add(resolvePath(p, s.path));
      refers.add(p);
    }
  }
  const depth = (p: string) => p.split("/").length;
  const minDepth = Math.min(...paths.map(depth));
  const top = paths.filter((p) => depth(p) === minDepth);
  const free = top.filter((p) => !referenced.has(p));
  return free.find((p) => refers.has(p)) ?? free[0] ?? top[0];
}

/** Strips a common top folder (`projekt/a.scad`, `projekt/lib/b.scad` → `a.scad`, `lib/b.scad`). */
export function stripCommonFolder(files: Record<string, string>): { files: Record<string, string>; folder: string | null } {
  const paths = Object.keys(files);
  if (!paths.length) return { files, folder: null };
  const first = paths[0].split("/")[0];
  if (!paths.every((p) => p.includes("/") && p.split("/")[0] === first)) return { files, folder: null };
  const out: Record<string, string> = {};
  for (const p of paths) out[p.slice(first.length + 1)] = files[p];
  return { files: out, folder: first };
}
