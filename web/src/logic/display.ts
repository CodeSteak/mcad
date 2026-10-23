// Display states (Spec: UI-Modell, Darstellung): which node meshes are drawn how, given the
// scene, the node(s) under the cursor and the node(s) of the hovered line. Pure function; the
// viewer turns the plan into three.js objects.

import type { Anchor, Mat4, Scene, SceneNode, Vec3 } from "../engine/protocol";
import { apply } from "./math";

export type Mode =
  /** Normal opaque body. */
  | "full"
  /** Rest of the module while a line is selected. */
  | "dim"
  /** Hovered line: semi-transparent, visible through other geometry. */
  | "hover"
  /** `#` modifier and subtracted bodies of a selected `difference`. */
  | "red"
  /** `%` modifier: auxiliary geometry. */
  | "grey"
  /** Zones of fillet/chamfer/shell. */
  | "blue"
  /** Children of a selected `intersection`. */
  | "ghost"
  /** Edges only (source position of a transform, children of a hovered union). */
  | "outline"
  /** Node under the mouse in the view: edges in the accent colour. */
  | "picked";

export interface DrawItem {
  node: number;
  mode: Mode;
  /** Placement of the node's mesh (context coordinates). */
  matrix: Mat4;
  /** 2D node: outline with a light fill. */
  flat: boolean;
  why: string;
}

export type EdgeKind = "selected" | "error";

export interface EdgeLine {
  points: Vec3[];
  kind: EdgeKind;
}

export interface Frame {
  matrix: Mat4;
  dashed: boolean;
}

export interface AnchorMark {
  anchor: Anchor;
  /** Highlighted (hovered reference, selected instance). */
  active: boolean;
}

export interface Plan {
  /** Context root (the whole model of the current module). */
  base: DrawItem | null;
  items: DrawItem[];
  edges: EdgeLine[];
  frames: Frame[];
  anchors: AnchorMark[];
  /** Hint for the status line (`!` modifier). */
  status: string | null;
}

export interface PlanInput {
  scene: Scene;
  cursor: number[];
  hover: number[];
  /** Anchor (inst, index) of a hovered reference expression. */
  hoverAnchor?: { inst: number; anchor: number } | null;
  /** Nodes under the mouse in the view. */
  picked?: number[];
}

const TRANSFORMS = new Set(["translate", "rotate", "scale", "mirror", "multmatrix"]);
const BLENDS = new Set(["fillet", "chamfer", "shell"]);

export function isTransform(n: SceneNode): boolean {
  return TRANSFORMS.has(n.label);
}

export function hasGeometry(n: SceneNode): boolean {
  return n.mesh_key !== null && n.status !== "skipped" && n.dim !== null && !n.flags.disabled;
}

function item(n: SceneNode, mode: Mode, why: string, matrix: Mat4 = n.display_world): DrawItem {
  return { node: n.idx, mode, matrix, flat: n.dim === "2d", why };
}

function polylinesToWorld(m: Mat4, lines: Vec3[][], kind: EdgeKind): EdgeLine[] {
  return lines.filter((l) => l.length > 1).map((l) => ({ points: l.map((p) => apply(m, p)), kind }));
}

/** Children shown with a node (not disabled). */
function visibleChildren(scene: Scene, n: SceneNode): SceneNode[] {
  return n.children.map((c) => scene.nodes[c]).filter((c) => !c.flags.disabled);
}

/** Module instance reached from a node through transforms (`translate(…) modul();`). */
export function instanceBelow(scene: Scene, n: SceneNode): number | null {
  let cur: SceneNode | undefined = n;
  for (let depth = 0; cur && depth < 32; depth++) {
    const inst = scene.instances.find((i) => i.id !== 0 && i.node === cur!.idx);
    if (inst) return inst.id;
    if (!isTransform(cur) || cur.children.length !== 1) return null;
    cur = scene.nodes[cur.children[0]];
  }
  return null;
}

/** Topmost nodes with a flag (descendants are part of their mesh). */
function topmost(scene: Scene, pred: (n: SceneNode) => boolean): SceneNode[] {
  const out: SceneNode[] = [];
  for (const n of scene.nodes) {
    if (!pred(n)) continue;
    let p = n.parent;
    let covered = false;
    while (p !== null) {
      if (pred(scene.nodes[p])) {
        covered = true;
        break;
      }
      p = scene.nodes[p].parent;
    }
    if (!covered) out.push(n);
  }
  return out;
}

export function plan(input: PlanInput): Plan {
  const { scene } = input;
  const nodes = scene.nodes;
  const items: DrawItem[] = [];
  const edges: EdgeLine[] = [];
  const frames: Frame[] = [];
  const activeInst = new Set<number>();
  const root = scene.root !== null ? nodes[scene.root] : null;

  const cursor = input.cursor.map((i) => nodes[i]).filter((n) => n && n.idx !== scene.root);
  const cursorGeo = cursor.filter(hasGeometry);
  const base: DrawItem | null = root && hasGeometry(root) ? item(root, cursorGeo.length ? "dim" : "full", "Modell") : null;

  // Cursor line: the node full, plus what its kind shows.
  for (const n of cursor) {
    if (hasGeometry(n)) items.push(item(n, n.flags.zone ? "blue" : "full", "Cursor"));
    if (n.label === "difference") {
      for (const c of visibleChildren(scene, n)) if (c.subtracted && hasGeometry(c)) items.push(item(c, "red", "abgezogen"));
    } else if (n.label === "intersection") {
      for (const c of visibleChildren(scene, n)) if (hasGeometry(c)) items.push(item(c, "ghost", "Schnittmenge"));
    } else if (BLENDS.has(n.label)) {
      for (const c of visibleChildren(scene, n)) if (c.flags.zone && hasGeometry(c)) items.push(item(c, "blue", "Zone"));
    } else if (isTransform(n)) {
      const kids = visibleChildren(scene, n);
      frames.push({ matrix: n.display_world, dashed: true });
      if (kids.length) frames.push({ matrix: kids[0].display_world, dashed: false });
      for (const c of kids) if (hasGeometry(c)) items.push(item(c, "outline", "Ausgangsposition", n.display_world));
    }
    if (n.flags.zone && n.parent !== null) {
      // Cursor on a zone: the body of the block dimmed is the base; the zone is blue.
      const p = nodes[n.parent];
      edges.push(...polylinesToWorld(p.display_world, p.selected_edges, "selected"));
    }
    edges.push(...polylinesToWorld(n.display_world, n.selected_edges, "selected"));
    const inst = instanceBelow(scene, n);
    if (inst !== null) activeInst.add(inst);
  }

  // Hovered line: semi-transparent in the whole model, also when subtracted.
  for (const i of input.hover) {
    const n = nodes[i];
    if (!n || n.idx === scene.root) continue;
    if (hasGeometry(n)) items.push(item(n, "hover", "Hover"));
    if (n.label === "union") {
      for (const c of visibleChildren(scene, n)) if (hasGeometry(c)) items.push(item(c, "outline", "Kind"));
    }
  }

  for (const i of input.picked ?? []) {
    const n = nodes[i];
    if (n && hasGeometry(n)) items.push(item(n, "picked", "Maus in der Ansicht"));
  }

  // Modifiers stay visible regardless of the cursor.
  for (const n of topmost(scene, (n) => n.flags.highlight)) if (hasGeometry(n)) items.push(item(n, "red", "#"));
  for (const n of topmost(scene, (n) => n.flags.background)) if (hasGeometry(n)) items.push(item(n, "grey", "%"));

  // Failed operations: affected edges red.
  for (const n of nodes) {
    for (const m of n.messages) {
      if (m.severity === "error" && m.edges.length) edges.push(...polylinesToWorld(n.display_world, m.edges, "error"));
    }
  }

  // Anchors: named instances, the context module's own anchors, the selected instance.
  const named = new Set(scene.instances.filter((i) => i.label !== null).map((i) => i.id));
  const ha = input.hoverAnchor ?? null;
  const anchors: AnchorMark[] = [];
  for (const a of scene.anchors) {
    const hovered = ha !== null && ha.inst === a.inst && ha.anchor === a.index;
    if (named.has(a.inst) || a.inst === 0 || activeInst.has(a.inst) || hovered) {
      anchors.push({ anchor: a, active: hovered || activeInst.has(a.inst) });
    }
  }

  let status: string | null = null;
  if (scene.root_is_subtree || scene.context.root_modifier !== null) {
    const r = scene.context.root_modifier !== null ? nodes[scene.context.root_modifier] : root;
    status = `Nur Teilbaum mit ! angezeigt${r ? ` (Zeile ${r.range.line})` : ""}`;
  }

  return { base, items, edges, frames, anchors, status };
}

/** Nodes whose meshes a plan needs. */
export function planNodes(p: Plan): number[] {
  const s = new Set<number>();
  if (p.base) s.add(p.base.node);
  for (const i of p.items) s.add(i.node);
  return [...s];
}
