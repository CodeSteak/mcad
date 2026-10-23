// Module context (Spec: Kontext: aktuelles Modul): path bar targets and definition jumps.

import type { Scene } from "../engine/protocol";
import type { SceneIndex, Span } from "./sceneIndex";

export interface ModuleSpan extends Span {
  name: string;
  body: Span;
  /** Enclosing modules, outermost first. */
  path: string[];
}

export function moduleSpans(idx: SceneIndex): ModuleSpan[] {
  return idx.scene.modules.map((m) => ({ name: m.name, ...idx.range(m.range), body: idx.range(m.body_range), path: m.path }));
}

/** Path bar segments: "Datei" then the modules of the context. */
export function pathSegments(scene: Scene | null, fileName: string): string[] {
  return [fileName || "Datei", ...(scene?.context.path ?? [])];
}

function escapeRe(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/** Positions inside comments and strings (calls there do not count). */
function commentMask(text: string): Uint8Array {
  const m = new Uint8Array(text.length + 1);
  let i = 0;
  while (i < text.length) {
    const c = text[i];
    const n = text[i + 1];
    if (c === "/" && n === "/") {
      const e = text.indexOf("\n", i);
      const end = e < 0 ? text.length : e;
      m.fill(1, i, end);
      i = end;
    } else if (c === "/" && n === "*") {
      const e = text.indexOf("*/", i + 2);
      const end = e < 0 ? text.length : e + 2;
      m.fill(1, i, end);
      i = end;
    } else if (c === '"') {
      let j = i + 1;
      while (j < text.length && text[j] !== '"' && text[j] !== "\n") j += text[j] === "\\" ? 2 : 1;
      m.fill(1, i, j + 1);
      i = j + 1;
    } else i++;
  }
  return m;
}

/**
 * Where the cursor goes when the path bar jumps to `level` (0 = file, k = k-th module of the
 * path): the first call of the module one level below inside that scope, else just after
 * that module's definition.
 */
export function jumpUpTarget(text: string, modules: ModuleSpan[], path: string[], level: number): number | null {
  if (level < 0 || level >= path.length) return null;
  const childName = path[level];
  const child = modules.find((m) => m.name === childName && m.path.join("/") === path.slice(0, level).join("/"));
  const scope: Span = level === 0 ? { from: 0, to: text.length } : modules.find((m) => m.name === path[level - 1])?.body ?? { from: 0, to: text.length };
  // Module definitions nested directly in the scope are not part of it.
  const excluded = modules.filter((m) => m.path.length === level && m.from >= scope.from && m.to <= scope.to);
  const mask = commentMask(text);
  const re = new RegExp(`(?<![\\w$.])${escapeRe(childName)}\\s*\\(`, "g");
  re.lastIndex = scope.from;
  for (let m = re.exec(text); m && m.index < scope.to; m = re.exec(text)) {
    const at = m.index;
    if (mask[at]) continue;
    if (excluded.some((e) => at >= e.from && at < e.to)) continue;
    if (/\bmodule\s*$/.test(text.slice(Math.max(0, at - 20), at))) continue;
    return at;
  }
  return child ? Math.min(child.to + 1, text.length) : null;
}

/** Position inside the definition of a module (start of its body, after `{`). */
export function definitionTarget(text: string, body: Span): number {
  let p = body.from;
  if (text[p] === "{") p++;
  // First non-blank position on the next line, if the body starts with a line break.
  const nl = text.indexOf("\n", p);
  if (nl >= 0 && nl < body.to && /^\s*$/.test(text.slice(p, nl))) {
    p = nl + 1;
    while (p < body.to && (text[p] === " " || text[p] === "\t")) p++;
  }
  return p;
}
