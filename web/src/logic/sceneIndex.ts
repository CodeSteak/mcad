// Code ↔ view mapping on one scene: which nodes a cursor position or line selects, which
// lines a mesh face belongs to, which module a position lies in. All positions are in the
// *current* editor document; scene ranges are mapped with the scene's `MapPos`.

import type { Text } from "@codemirror/state";
import type { Ref, Scene, SceneNode, SrcRange, Vec3 } from "../engine/protocol";
import { apply, containsPoint, invertAffine, boxDiagonal } from "./math";
import { identityMap, type MapPos, mapRange } from "./positions";

export interface Span {
  from: number;
  to: number;
}

export interface CursorSelection {
  /** Selected nodes (all instances of the chosen statement). */
  nodes: number[];
  /** Statement of the selected nodes. */
  stmt: number | null;
  /** 1-based line of the cursor. */
  line: number;
  /** Indices into `scene.refs` used on the cursor line or by the selected statement. */
  refs: number[];
  /** Grid declared on the cursor line (all its planes are shown). */
  grids: number[];
}

export const EMPTY_SELECTION: CursorSelection = { nodes: [], stmt: null, line: 0, refs: [], grids: [] };

export class SceneIndex {
  readonly scene: Scene;
  readonly map: MapPos;
  private spans: Span[];
  private byStmt = new Map<number, number[]>();
  private tagNodes = new Map<number, number[]>();

  constructor(scene: Scene, map: MapPos = identityMap) {
    this.scene = scene;
    this.map = map;
    this.spans = scene.nodes.map((n) => mapRange(map, n.range.from, n.range.to));
    for (const n of scene.nodes) {
      if (n.stmt === null) continue;
      const l = this.byStmt.get(n.stmt);
      if (l) l.push(n.idx);
      else this.byStmt.set(n.stmt, [n.idx]);
    }
    for (const t of scene.tag_nodes) this.tagNodes.set(t.tag, t.nodes);
  }

  span(node: number): Span {
    return this.spans[node];
  }

  range(r: SrcRange): Span {
    return mapRange(this.map, r.from, r.to);
  }

  node(i: number): SceneNode {
    return this.scene.nodes[i];
  }

  /** Nodes of a statement (several for loops and repeated module calls). */
  nodesOfStmt(stmt: number): number[] {
    return this.byStmt.get(stmt) ?? [];
  }

  /** Nodes that can be selected from the editor (not the synthetic root). */
  private selectable(n: SceneNode): boolean {
    return n.stmt !== null && n.idx !== this.scene.root;
  }

  /**
   * Nodes selected by a cursor at `pos`: among nodes starting on the cursor line the
   * innermost one containing the cursor, else the outermost one starting on the line;
   * without such nodes the innermost node containing the cursor. Returns all nodes of the
   * chosen statement.
   */
  nodesAt(doc: Text, pos: number): { nodes: number[]; stmt: number | null } {
    const line = doc.lineAt(Math.min(pos, doc.length));
    let onLine: SceneNode | null = null;
    let onLineSize = -1;
    let inner: SceneNode | null = null;
    let innerSize = Infinity;
    let containing: SceneNode | null = null;
    let containingSize = Infinity;
    for (const n of this.scene.nodes) {
      if (!this.selectable(n)) continue;
      const s = this.spans[n.idx];
      const size = s.to - s.from;
      const contains = s.from <= pos && pos <= s.to;
      const starts = s.from >= line.from && s.from <= line.to;
      if (starts && contains && size < innerSize) {
        inner = n;
        innerSize = size;
      }
      if (starts && size > onLineSize) {
        onLine = n;
        onLineSize = size;
      }
      if (contains && size < containingSize) {
        containing = n;
        containingSize = size;
      }
    }
    // At the end of the line the whole statement of the line is meant.
    const atEnd = pos >= line.from + line.text.trimEnd().length;
    const chosen = (atEnd ? onLine : inner ?? onLine) ?? containing;
    if (!chosen || chosen.stmt === null) return { nodes: [], stmt: null };
    return { nodes: this.nodesOfStmt(chosen.stmt), stmt: chosen.stmt };
  }

  /** Grid/anchor references used on a line or by a statement. */
  refsFor(doc: Text, line: number, stmt: number | null): number[] {
    if (line < 1 || line > doc.lines) return [];
    const l = doc.line(line);
    const out: number[] = [];
    this.scene.refs.forEach((r, i) => {
      const s = this.range(r.range);
      if ((s.from >= l.from && s.from <= l.to) || (stmt !== null && r.stmt === stmt)) out.push(i);
    });
    return out;
  }

  /** Grids whose declaration starts on the line. */
  gridsOnLine(doc: Text, line: number): number[] {
    if (line < 1 || line > doc.lines) return [];
    const l = doc.line(line);
    const out: number[] = [];
    for (const g of this.scene.grids) {
      const s = this.range(g.range);
      if (s.from <= l.to && s.to >= l.from) out.push(g.id);
    }
    return out;
  }

  select(doc: Text, pos: number): CursorSelection {
    const line = doc.lineAt(Math.min(pos, doc.length)).number;
    const { nodes, stmt } = this.nodesAt(doc, pos);
    return { nodes, stmt, line, refs: this.refsFor(doc, line, stmt), grids: this.gridsOnLine(doc, line) };
  }

  /** Reference expression under `pos` (innermost). */
  refAt(pos: number): number | null {
    let best: number | null = null;
    let size = Infinity;
    this.scene.refs.forEach((r: Ref, i) => {
      const s = this.range(r.range);
      if (s.from <= pos && pos <= s.to && s.to - s.from < size) {
        best = i;
        size = s.to - s.from;
      }
    });
    return best;
  }

  /** Nearest ancestor-or-self that belongs to the context module (module_inst 0). */
  liftToContext(n: number): number {
    let cur = this.scene.nodes[n];
    while (cur.module_inst !== 0 && cur.parent !== null) cur = this.scene.nodes[cur.parent];
    return cur.idx;
  }

  /**
   * Nodes of the context module that created a face with `tag`, hit at `world` (context
   * coordinates). Several nodes with the same tag (same mesh content at different places)
   * are narrowed down by their bounds.
   */
  nodesForTag(tag: number, world?: Vec3): number[] {
    let cands = this.tagNodes.get(tag) ?? [];
    if (world && cands.length > 1) {
      const inside = cands.filter((c) => {
        const n = this.scene.nodes[c];
        if (!n.bounds) return false;
        const local = apply(invertAffine(n.display_world), world);
        const eps = Math.max(1e-3, boxDiagonal(n.bounds) * 1e-3);
        return containsPoint(n.bounds, local, eps);
      });
      if (inside.length) cands = inside;
    }
    const out: number[] = [];
    for (const c of cands) {
      const l = this.liftToContext(c);
      if (l === this.scene.root || this.scene.nodes[l].stmt === null) continue;
      if (!out.includes(l)) out.push(l);
    }
    return out;
  }

  /** Module call node (instance) behind a context node, if it is one. */
  instanceOfNode(n: number): number | null {
    const inst = this.scene.instances.find((i) => i.node === n && i.id !== 0);
    return inst ? inst.id : null;
  }

  /** 1-based lines (current document) where nodes start. */
  linesOf(doc: Text, nodes: number[]): number[] {
    const out = new Set<number>();
    for (const n of nodes) out.add(doc.lineAt(Math.min(this.spans[n].from, doc.length)).number);
    return [...out].sort((a, b) => a - b);
  }

  /** Innermost module definition containing `pos` (its name), `null` at file level. */
  moduleAt(pos: number): string | null {
    let best: string | null = null;
    let size = Infinity;
    for (const m of this.scene.modules) {
      const s = this.range(m.range);
      if (s.from <= pos && pos <= s.to && s.to - s.from < size) {
        best = m.name;
        size = s.to - s.from;
      }
    }
    return best;
  }
}
