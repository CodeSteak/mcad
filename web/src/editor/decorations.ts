// Editor decorations from engine data (Spec: Fehler und Warnungen, Inline-Hinweise,
// Modifier): diagnostics underlined, error lines red, warning lines yellow, messages and
// value hints at the line end, `*` statements greyed out, lines of the face under the mouse
// in the view highlighted, hovered grid expressions marked.

import { syntaxTree } from "@codemirror/language";
import type { EditorState, Range, Text } from "@codemirror/state";
import { Decoration, type DecorationSet, WidgetType } from "@codemirror/view";
import type { Diagnostic, Scene } from "../engine/protocol";
import { diagnosticPath, fileIds, includeStatements } from "../logic/files";
import { mapRange, type MapPos, type VersionMapper } from "../logic/positions";
import { disabledStatements } from "./lang";
import { type LiteralControl, LiteralControlWidget, literalAt } from "./literalControls";

export interface DecorationInput {
  scene: Scene | null;
  complete: Scene | null;
  diagnostics: { version: number; items: Diagnostic[] };
  mapper: VersionMapper;
  engineError: { node: number | null; version: number | null; message: string } | null;
  viewHoverLines: number[];
  hoverLine: number | null;
  hoverRef: { from: number; to: number } | null;
  /** Project file in the editor and the main file (default: a single file). */
  activeFile?: string;
  mainFile?: string;
}

type SegKind = "error" | "warning" | "info" | "hint" | "echo";

interface Seg {
  kind: SegKind;
  text: string;
  /** The diagnostic has a place (`at`): a click frames it in the view. */
  locate?: boolean;
}

class LineEndWidget extends WidgetType {
  constructor(readonly segs: Seg[]) {
    super();
  }
  eq(o: LineEndWidget): boolean {
    return o.segs.length === this.segs.length && o.segs.every((s, i) => s.kind === this.segs[i].kind && s.text === this.segs[i].text && !!s.locate === !!this.segs[i].locate);
  }
  toDOM(): HTMLElement {
    const wrap = document.createElement("span");
    wrap.className = "cm-lineend";
    for (const s of this.segs) {
      const e = document.createElement("span");
      e.className = `cm-lineend-${s.kind}${s.kind === "warning" && /^assert_(keepout|disjoint|intersect|touches): /.test(s.text) ? " cm-lineend-assert" : ""}`;
      if (e.classList.contains("cm-lineend-assert")) e.title = "Klick: Verletzung in der Ansicht zeigen";
      if (s.locate) {
        e.classList.add("cm-lineend-locate");
        e.title = "Klick: Stelle in der Ansicht zeigen";
      }
      e.textContent = s.text;
      wrap.appendChild(e);
    }
    return wrap;
  }
  ignoreEvent(): boolean {
    return false;
  }
}

/** Per-line classes and line-end segments, before conversion to decorations. */
export interface LineInfo {
  classes: Set<string>;
  segs: Seg[];
}

function lineInfo(map: Map<number, LineInfo>, line: number): LineInfo {
  let l = map.get(line);
  if (!l) {
    l = { classes: new Set(), segs: [] };
    map.set(line, l);
  }
  return l;
}

function lineOf(doc: Text, pos: number): number {
  return doc.lineAt(Math.max(0, Math.min(pos, doc.length))).number;
}

/** Computes line classes and line-end segments (exported for tests). */
export function collectLines(doc: Text, input: DecorationInput, disabled: Array<{ from: number; to: number }>): {
  lines: Map<number, LineInfo>;
  marks: Array<{ from: number; to: number; cls: string }>;
  controls: Map<number, LiteralControl>;
} {
  const lines = new Map<number, LineInfo>();
  const controls = new Map<number, LiteralControl>();
  const marks: Array<{ from: number; to: number; cls: string }> = [];
  const clampMark = (from: number, to: number, cls: string) => {
    let f = Math.max(0, Math.min(from, doc.length));
    let t = Math.max(f, Math.min(to, doc.length));
    if (f === t) {
      const l = doc.lineAt(f);
      if (t < l.to) t++;
      else if (f > l.from) f--;
    }
    if (f < t) marks.push({ from: f, to: t, cls });
  };

  // `*` statements: greyed out.
  for (const d of disabled) {
    for (let l = lineOf(doc, d.from); l <= lineOf(doc, d.to); l++) lineInfo(lines, l).classes.add("cm-l-disabled");
  }

  // Only what belongs to the file in the editor.
  const main = input.mainFile ?? "";
  const active = input.activeFile ?? main;
  const idsCache = new Map<Scene, Set<number>>();
  const single = input.activeFile === undefined;
  const inFile = (s: Scene, file: number | undefined) => {
    if (single) return (file ?? 0) === 0;
    let ids = idsCache.get(s);
    if (!ids) {
      ids = fileIds(s, active, main);
      idsCache.set(s, ids);
    }
    return ids.has(file ?? 0);
  };

  // Diagnostics (own version).
  const dmap = input.mapper.mapper(input.diagnostics.version);
  for (const d of input.diagnostics.items) {
    if (single ? d.file !== undefined : diagnosticPath(d, main) !== active) continue;

    const r = mapRange(dmap, d.range.from, d.range.to);
    const line = lineOf(doc, r.from);
    const li = lineInfo(lines, line);
    if (d.severity === "error") li.classes.add("cm-l-error");
    else if (d.severity === "warning") li.classes.add("cm-l-warning");
    li.segs.push({ kind: d.severity, text: d.message, locate: d.at !== undefined });
    if (d.source !== "kernel" && d.severity !== "info") clampMark(r.from, r.to, d.severity === "error" ? "cm-d-error" : "cm-d-warning");
  }

  // Value hints of assignments (latest scene).
  if (input.scene) {
    const m = input.mapper.mapper(input.scene.version);
    for (const h of input.scene.hints) {
      if (!h.in_context || !inFile(input.scene, h.range.file)) continue;
      const r = mapRange(m, h.range.from, h.range.to);
      const line = lineOf(doc, r.to);
      lineInfo(lines, line).segs.push({ kind: "hint", text: `= ${h.text}` });
      // Bool literal → switch (Spec: Werttypen); number literals have no inline slider.
      if (h.literal && typeof h.value === "boolean" && !controls.has(line)) {
        const lr = mapRange(m, h.literal.from, h.literal.to);
        const c: LiteralControl = { from: lr.from, to: lr.to, kind: "bool", value: h.value };
        if (literalAt(doc, c) && lineOf(doc, lr.from) === line) controls.set(line, c);
      }
    }
    for (const e of input.scene.echoes) {
      if (!e.in_context || !inFile(input.scene, e.range.file)) continue;
      const r = mapRange(m, e.range.from, e.range.to);
      lineInfo(lines, lineOf(doc, r.to)).segs.push({ kind: "echo", text: `ECHO: ${e.text}` });
    }
  }

  // Node hints ("4 Kanten"), failed operations, disabled nodes (latest complete scene).
  const c = input.complete;
  if (c) {
    const m: MapPos = input.mapper.mapper(c.version);
    const seen = new Map<number, Set<string>>();
    for (const n of c.nodes) {
      if (n.stmt === null || !inFile(c, n.range.file)) continue;
      const r = mapRange(m, n.range.from, n.range.to);
      const line = lineOf(doc, r.from);
      if (n.flags.disabled) for (let l = line; l <= lineOf(doc, r.to); l++) lineInfo(lines, l).classes.add("cm-l-disabled");
      if (n.module_inst !== 0) continue;
      if (n.status === "error") lineInfo(lines, line).classes.add("cm-l-error");
      else if (n.status === "warning") lineInfo(lines, line).classes.add("cm-l-warning");
      if (n.hints.length) {
        const s = seen.get(line) ?? new Set<string>();
        const text = n.hints.join(" · ");
        if (!s.has(text)) {
          s.add(text);
          seen.set(line, s);
          lineInfo(lines, line).segs.push({ kind: "hint", text });
        }
      }
    }
    // Watchdog / engine error at a node.
    const ee = input.engineError;
    if (ee && ee.node !== null) {
      const n = c.nodes[ee.node];
      if (n && inFile(c, n.range.file)) {

        const line = lineOf(doc, mapRange(m, n.range.from, n.range.to).from);
        const li = lineInfo(lines, line);
        li.classes.add("cm-l-error");
        li.segs.unshift({ kind: "error", text: ee.message });
      }
    }
  }

  for (const l of input.viewHoverLines) if (l >= 1 && l <= doc.lines) lineInfo(lines, l).classes.add("cm-l-viewhover");
  if (input.hoverLine !== null && input.hoverLine >= 1 && input.hoverLine <= doc.lines) lineInfo(lines, input.hoverLine).classes.add("cm-l-hover");
  if (input.hoverRef) clampMark(input.hoverRef.from, input.hoverRef.to, "cm-ref-hover");
  return { lines, marks, controls };
}

export function buildDecorations(state: EditorState, input: DecorationInput): DecorationSet {
  const doc = state.doc;
  const disabled = disabledStatements(syntaxTree(state), doc.toString());
  const { lines, marks, controls } = collectLines(doc, input, disabled);
  const ranges: Range<Decoration>[] = [];
  for (const [line, info] of lines) {
    const l = doc.line(line);
    if (info.classes.size) ranges.push(Decoration.line({ class: [...info.classes].join(" ") }).range(l.from));
    if (info.segs.length) ranges.push(Decoration.widget({ widget: new LineEndWidget(info.segs), side: 1 }).range(l.to));
    const c = controls.get(line);
    if (c) ranges.push(Decoration.widget({ widget: new LiteralControlWidget(c), side: 2 }).range(l.to));
  }
  for (const m of marks) {
    const attributes = m.cls === "cm-ref-hover" ? { title: "Strg+Klick: zur Deklaration springen" } : undefined;
    ranges.push(Decoration.mark({ class: m.cls, attributes }).range(m.from, m.to));
  }
  // Paths of include/use: Ctrl+click opens the file.
  for (const inc of includeStatements(doc.toString())) {
    if (inc.to > inc.from) ranges.push(Decoration.mark({ class: "cm-include", attributes: { title: "Strg+Klick: Datei öffnen" } }).range(inc.from, inc.to));
  }
  return Decoration.set(ranges, true);
}
