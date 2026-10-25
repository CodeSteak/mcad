// Display states (Spec: UI-Modell, Darstellung): which node meshes are drawn how, given the
// scene, the node(s) under the cursor and the node(s) of the hovered line. Pure function; the
// viewer turns the plan into three.js objects.

import type { Anchor, Mat4, OpInfo, Scene, SceneNode, SrcRange, Vec3 } from "../engine/protocol";
import { REVOLVE_PROFILE, sweepPath, sweepStartFrame } from "./curves";
import { apply, invertAffine, mul } from "./math";
import type { SelectMode } from "./zones";

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
  | "picked"
  /** Profile of a profile operation / surface of `thicken`: outline, visible through the result. */
  | "profile"
  /** Result of a `loft`: semi-transparent so the profiles stay visible. */
  | "translucent"
  /** Keep-out volume of `assert_keepout`: hatched, transparent, dashed outline on top. */
  | "keepout";

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

export type GuideKind =
  /** Rotation axis of `rotate_extrude`. */
  | "axis"
  /** Path of a `sweep`. */
  | "path"
  /** Control net of a `surface`. */
  | "net"
  /** Contour of an `offset` (dashed, over the input contour). */
  | "offset"
  /** Path (centre line) of a `stroke`. */
  | "centerline";

export interface Guide {
  points: Vec3[];
  kind: GuideKind;
  dashed: boolean;
}

/** Cursor on a zone: the block's selected edges that lie in this zone (viewer tests them). */
export interface ZoneEdgeQuery {
  zone: number;
  /** All selected edges of the block (context coordinates). */
  edges: Vec3[][];
  select: SelectMode;
}

/** `shell`: faces of the body that lie in the zones are opened (viewer tests them). */
export interface OpenFaceQuery {
  shell: number;
  body: number;
  /** Placement of the body mesh. */
  matrix: Mat4;
  zones: number[];
  select: SelectMode;
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

/** One segment of a drawn sketch (points in the sketch's plane, its node system). */
export interface SketchSegmentDraw {
  stmt: number;
  kind: string;
  points: Vec3[];
  /** On the cursor line. */
  active: boolean;
  error: boolean;
  /** Closing line added for a contour without `close()`. */
  implicit: boolean;
}

/** A sketch under the cursor (Plan 17.1): drawn in its plane with outline, light fill, the
 *  segment of the cursor line highlighted, help lines and start/end marks. */
export interface SketchDraw {
  node: number;
  /** Sketch plane → context coordinates. */
  matrix: Mat4;
  closed: boolean;
  segments: SketchSegmentDraw[];
  /** Control polygons of Bézier curves; arcs: centre → start / end. */
  helpers: Vec3[][];
  /** Start point and direction of every contour; `open`: the contour is not closed. */
  starts: Array<{ point: Vec3; dir: Vec3; open: boolean; end: Vec3; hole: boolean }>;
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
  /** Guide lines of profile operations and surfaces. */
  guides: Guide[];
  zoneEdges: ZoneEdgeQuery | null;
  openFaces: OpenFaceQuery[];
  sketches: SketchDraw[];
  /** Places of failed operations (`NodeMessage.at`, context coordinates): the operands of a
   *  Boolean touch tangentially there; drawn as red points, edges in `edges` ("error"). */
  failures: Array<{ node: number; point: Vec3 }>;
}

export interface PlanInput {
  scene: Scene;
  cursor: number[];
  hover: number[];
  /** Anchor (inst, index) of a hovered reference expression. */
  hoverAnchor?: { inst: number; anchor: number } | null;
  /** Nodes under the mouse in the view. */
  picked?: number[];
  /** Hidden parts (their `ModuleCall` nodes): the model is drawn without them. */
  hidden?: number[];
  /** Is a segment statement on the cursor line (sketches)? */
  segmentActive?: (range: SrcRange) => boolean;
  /** Draw keep-out volumes (`assert_keepout`; default on). */
  showKeepout?: boolean;
}

/** Is `n` a keep-out volume or inside one? */
export function inKeepout(scene: Scene, n: SceneNode): boolean {
  for (let cur: SceneNode | null = n, d = 0; cur && d < 256; cur = cur.parent !== null ? scene.nodes[cur.parent] : null, d++) {
    if (cur.flags.keepout) return true;
  }
  return false;
}

/** The sketch a node belongs to: itself, an ancestor, or (module calls of a sketch module,
 *  groups) the sketches below it. */
export function sketchesFor(scene: Scene, n: SceneNode): SceneNode[] {
  for (let cur: SceneNode | null = n, d = 0; cur && d < 64; cur = cur.parent !== null ? scene.nodes[cur.parent] : null, d++) {
    if (cur.op_info?.kind === "sketch") return [cur];
  }
  const out: SceneNode[] = [];
  const walk = (c: SceneNode, depth: number) => {
    if (depth > 8 || c.flags.disabled) return;
    if (c.op_info?.kind === "sketch") out.push(c);
    // (profiles of rotate_extrude, loft and sweep are drawn elsewhere than in their own plane)
    else if (c.label !== "rotate_extrude" && c.label !== "loft" && c.label !== "sweep" && !(c.op_info?.kind === "linear_extrude" && c.op_info.center)) {
      for (const k of c.children) walk(scene.nodes[k], depth + 1);
    }
  };
  walk(n, 0);
  return out;
}

/** Where a sketch is drawn: its plane (the xz-plane in a rotate_extrude, the bottom of a
 *  centred linear_extrude; engine `OpInfo.Sketch.plane`), else its node system. */
export function sketchPlane(n: SceneNode): Mat4 {
  const o = n.op_info;
  return o && o.kind === "sketch" && o.plane ? o.plane : n.display_world;
}

function sketchDraw(n: SceneNode, active: (r: SrcRange) => boolean): SketchDraw | null {
  const o = n.op_info;
  if (!o || o.kind !== "sketch") return null;
  const segments: SketchSegmentDraw[] = [];
  const helpers: Vec3[][] = [];
  const starts: SketchDraw["starts"] = [];
  for (const c of o.contours) {
    for (const g of c.segments) {
      segments.push({ stmt: g.stmt, kind: g.kind, points: g.points, active: !g.implicit && active(g.range), error: g.error, implicit: g.implicit });
      if (g.kind === "bezier" && g.ctrl.length >= 3) helpers.push(g.ctrl);
      if (g.kind === "arc" && g.ctrl.length === 1 && g.points.length >= 2) helpers.push([g.points[0], g.ctrl[0], g.points[g.points.length - 1]]);
    }
    const first = c.segments[0]?.points ?? [];
    const last = c.segments[c.segments.length - 1]?.points ?? [];
    if (first.length >= 2) {
      const d = [first[1][0] - first[0][0], first[1][1] - first[0][1], 0] as Vec3;
      starts.push({ point: first[0], dir: d, open: !c.closed, end: last[last.length - 1] ?? first[0], hole: c.hole });
    }
  }
  return { node: n.idx, matrix: sketchPlane(n), closed: o.closed, segments, helpers, starts };
}

/**
 * Nodes that draw the model without the `hidden` subtrees: the root is split through groups
 * and transforms down to the subtrees without hidden nodes (other operations stay whole).
 */
export function visiblePieces(scene: Scene, hidden: number[]): SceneNode[] {
  if (scene.root === null) return [];
  const hide = new Set(hidden);
  const containsHidden = new Map<number, boolean>();
  const contains = (n: SceneNode): boolean => {
    const c = containsHidden.get(n.idx);
    if (c !== undefined) return c;
    const v = hide.has(n.idx) || n.children.some((k) => contains(scene.nodes[k]));
    containsHidden.set(n.idx, v);
    return v;
  };
  const out: SceneNode[] = [];
  const walk = (n: SceneNode, depth: number) => {
    if (hide.has(n.idx) || n.flags.disabled) return;
    if (!contains(n) || depth > 64 || !(PASS.has(n.label) || n.idx === scene.root)) {
      if (hasGeometry(n)) out.push(n);
      return;
    }
    for (const c of visibleChildren(scene, n)) if (!c.flags.background && !c.flags.zone) walk(c, depth + 1);
  };
  walk(scene.nodes[scene.root], 0);
  return out;
}

const TRANSFORMS = new Set(["translate", "rotate", "scale", "mirror", "multmatrix"]);
/** Nodes descended to find the profiles of a loft (their own meshes are flat in their system). */
const PASS = new Set([...TRANSFORMS, "group", "color", "resize"]);
const BLENDS = new Set(["fillet", "chamfer", "shell", "fillet2d", "chamfer2d"]);

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

/** Child system → parent node system (both in context coordinates via `world`). */
function relative(parent: SceneNode, child: SceneNode): Mat4 {
  return mul(invertAffine(parent.world), child.world);
}

/** Profile nodes of a loft: descends through transforms and groups to the shapes. */
export function profileLeaves(scene: Scene, n: SceneNode): SceneNode[] {
  const out: SceneNode[] = [];
  const walk = (c: SceneNode, depth: number) => {
    if (c.flags.background || depth > 64) return;
    const kids = visibleChildren(scene, c);
    if (PASS.has(c.label) && kids.length) for (const k of kids) walk(k, depth + 1);
    else if (hasGeometry(c)) out.push(c);
  };
  for (const c of visibleChildren(scene, n)) walk(c, 0);
  return out;
}

function translation(z: number): Mat4 {
  return [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, z, 1];
}

function opInfo(n: SceneNode): OpInfo | null {
  return n.op_info ?? null;
}

/** Body (first child) and zones of a fillet/chamfer/shell block. */
function blockParts(scene: Scene, n: SceneNode): { body: SceneNode | null; zones: SceneNode[] } {
  const kids = visibleChildren(scene, n);
  return { body: kids.find((c) => !c.flags.zone) ?? null, zones: kids.filter((c) => c.flags.zone && hasGeometry(c)) };
}

function selectOf(n: SceneNode): SelectMode {
  const o = opInfo(n);
  return o && (o.kind === "blend" || o.kind === "shell" || o.kind === "blend2d") ? o.select : "inside";
}

/** Display of profile operations and surfaces under the cursor (Spec: Profiloperationen). */
function profileOp(scene: Scene, n: SceneNode, items: DrawItem[], guides: Guide[]): void {
  const o = opInfo(n);
  if (!o) return;
  const kids = visibleChildren(scene, n).filter(hasGeometry);
  const dw = n.display_world;
  const world = (pts: Vec3[]) => pts.map((p) => apply(dw, p));
  switch (o.kind) {
    case "linear_extrude": {
      // Profile as outline at the bottom (z0), height grip on top (handle).
      const z0 = o.center ? -o.h / 2 : 0;
      for (const c of kids) items.push(item(c, "profile", "Profil unten", mul(dw, mul(translation(z0), relative(n, c)))));
      break;
    }
    case "rotate_extrude": {
      // Profile in the xz plane, rotation axis as a line, angle grip (handle).
      for (const c of kids) items.push(item(c, "profile", "Profil in der xz-Ebene", mul(dw, mul(REVOLVE_PROFILE, relative(n, c)))));
      const b = n.bounds;
      const lo = b ? b.min[2] : 0;
      const hi = b ? b.max[2] : 10;
      const pad = Math.max(2, (hi - lo) * 0.15);
      guides.push({ points: world([[0, 0, lo - pad], [0, 0, hi + pad]]), kind: "axis", dashed: true });
      break;
    }
    case "loft":
      for (const c of profileLeaves(scene, n)) items.push(item(c, "profile", "Profil"));
      break;
    case "sweep": {
      const path = sweepPath(o.path, o.spline);
      if (!path) break;
      guides.push({ points: world(path.points), kind: "path", dashed: false });
      const f = sweepStartFrame(path.points[0], path.tangent);
      for (const c of kids) items.push(item(c, "profile", "Profil am Startpunkt", mul(dw, mul(f, relative(n, c)))));
      break;
    }
    case "surface": {
      const rows = o.rows;
      for (const r of rows) guides.push({ points: world(r), kind: "net", dashed: false });
      const cols = rows[0]?.length ?? 0;
      for (let j = 0; j < cols; j++) guides.push({ points: world(rows.map((r) => r[j])), kind: "net", dashed: false });
      break;
    }
    case "thicken":
      for (const c of kids) items.push(item(c, "profile", "Fläche"));
      break;
    case "offset":
      // Input contour as outline, the offset contour dashed over it (Plan 17.3).
      for (const c of kids) items.push(item(c, "profile", "Ausgangskontur"));
      for (const g of n.guides ?? []) if (g.kind === "result") guides.push({ points: world(g.points), kind: "offset", dashed: true });
      break;
    case "stroke":
      for (const c of kids) items.push(item(c, "profile", "Pfad"));
      for (const g of n.guides ?? []) if (g.kind === "centerline") guides.push({ points: world(g.points), kind: "centerline", dashed: true });
      break;
    case "blend2d":
      // The profile before rounding as outline; the new corner pieces are selected edges.
      for (const c of kids) if (!c.flags.zone) items.push(item(c, "profile", "Profil vor dem Verrunden"));
      break;
    default:
      break;
  }
}

export function plan(input: PlanInput): Plan {
  const { scene } = input;
  const nodes = scene.nodes;
  const items: DrawItem[] = [];
  const edges: EdgeLine[] = [];
  const frames: Frame[] = [];
  const guides: Guide[] = [];
  const openFaces: OpenFaceQuery[] = [];
  let zoneEdges: ZoneEdgeQuery | null = null;
  const sketches: SketchDraw[] = [];
  const activeInst = new Set<number>();
  const root = scene.root !== null ? nodes[scene.root] : null;

  const cursor = input.cursor.map((i) => nodes[i]).filter((n) => n && n.idx !== scene.root);
  const cursorGeo = cursor.filter(hasGeometry);
  const hidden = input.hidden ?? [];
  let base: DrawItem | null = root && hasGeometry(root) ? item(root, cursorGeo.length ? "dim" : "full", "Modell") : null;
  if (base && hidden.length) {
    // Hidden parts: the model in pieces without them.
    const mode = base.mode;
    base = null;
    for (const n of visiblePieces(scene, hidden)) items.push(item(n, mode, "Modell ohne ausgeblendete Bauteile"));
  }


  // Cursor line: the node full, plus what its kind shows.
  for (const n of cursor) {
    if (hasGeometry(n)) {
      const mode: Mode = n.flags.zone ? "blue" : inKeepout(scene, n) ? "keepout" : n.label === "loft" ? "translucent" : n.op_info?.kind === "sketch" ? "profile" : "full";
      items.push(item(n, mode, "Cursor", n.op_info?.kind === "sketch" ? sketchPlane(n) : n.display_world));
    }
    if (n.label === "difference") {
      for (const c of visibleChildren(scene, n)) if (c.subtracted && hasGeometry(c)) items.push(item(c, "red", "abgezogen"));
    } else if (n.label === "intersection") {
      for (const c of visibleChildren(scene, n)) if (hasGeometry(c)) items.push(item(c, "ghost", "Schnittmenge"));
    } else if (BLENDS.has(n.label)) {
      const { body, zones } = blockParts(scene, n);
      for (const c of zones) items.push(item(c, "blue", "Zone"));
      if (n.label === "shell" && body && hasGeometry(body) && zones.length) {
        openFaces.push({ shell: n.idx, body: body.idx, matrix: body.display_world, zones: zones.map((z) => z.idx), select: selectOf(n) });
      }
    } else if (isTransform(n) && !n.flags.zone) {
      // (A zone placed by a transform shows only the zone and its edges.)
      const kids = visibleChildren(scene, n);
      frames.push({ matrix: n.display_world, dashed: true });
      if (kids.length) frames.push({ matrix: kids[0].display_world, dashed: false });
      for (const c of kids) if (hasGeometry(c)) items.push(item(c, "outline", "Ausgangsposition", n.display_world));
    }
    if (n.flags.zone && n.parent !== null) {
      // Cursor on a zone: only this zone and its edges (or opened faces) are highlighted.
      const p = nodes[n.parent];
      if (p.label === "shell") {
        const { body } = blockParts(scene, p);
        if (body && hasGeometry(body) && hasGeometry(n)) {
          openFaces.push({ shell: p.idx, body: body.idx, matrix: body.display_world, zones: [n.idx], select: selectOf(p) });
        }
      } else if (p.selected_edges.length && hasGeometry(n)) {
        const all = polylinesToWorld(p.display_world, p.selected_edges, "selected").map((e) => e.points);
        zoneEdges = { zone: n.idx, edges: all, select: selectOf(p) };
      }
    }
    edges.push(...polylinesToWorld(n.display_world, n.selected_edges, "selected"));
    profileOp(scene, n, items, guides);
    // Sketch under the cursor: in its plane, the segment of the cursor line highlighted.
    for (const sk of sketchesFor(scene, n)) {
      if (sketches.some((d) => d.node === sk.idx)) continue;
      const d = sketchDraw(sk, input.segmentActive ?? (() => false));
      if (d) sketches.push(d);
      if (hasGeometry(sk) && sk.idx !== n.idx) items.push(item(sk, "profile", "Skizze", sketchPlane(sk)));
    }
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
  for (const n of topmost(scene, (n) => n.flags.background)) {
    if (!hasGeometry(n)) continue;
    if (!n.flags.keepout) items.push(item(n, "grey", "%"));
    else if (input.showKeepout !== false) items.push(item(n, "keepout", "Sperrvolumen"));
  }

  // Failed operations: affected edges red, places (operands touching) as points.
  const failures: Plan["failures"] = [];
  for (const n of nodes) {
    for (const m of n.messages) {
      if (m.severity === "error" && m.edges.length) edges.push(...polylinesToWorld(n.display_world, m.edges, "error"));
      if (m.severity === "error" && m.at) failures.push({ node: n.idx, point: apply(n.display_world, m.at) });
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

  return { base, items, edges, frames, anchors, status, guides, zoneEdges, openFaces, sketches, failures };
}

/** Nodes whose meshes a plan needs. */
export function planNodes(p: Plan): number[] {
  const s = new Set<number>();
  if (p.base) s.add(p.base.node);
  for (const i of p.items) s.add(i.node);
  if (p.zoneEdges) s.add(p.zoneEdges.zone);
  for (const q of p.openFaces) {
    s.add(q.body);
    for (const z of q.zones) s.add(z);
  }
  return [...s];
}
