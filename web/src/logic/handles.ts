// Handles (Spec: Griffe und Rückschreiben): which handles the cursor line shows, how they are
// drawn (grip position, guide line, rotation ring) and how a mouse ray maps to a new value.
// Pure functions on protocol types; the viewer draws, the drag controller sends.

import type { Text } from "@codemirror/state";
import type { Handle, Scene, SnapTarget, Vec3 } from "../engine/protocol";
import { sketchesFor } from "./display";
import { gridBox } from "./grids";
import { type Box3, add, apply, applyDir, invertAffine, length, scale, sub } from "./math";
import type { CursorSelection, SceneIndex } from "./sceneIndex";

export type HandleStyle = "literal" | "default" | "ref" | "locked";

/** Display class of a handle: literal (also through a variable), module default, reference only, locked. */
export function handleStyle(h: Handle): HandleStyle {
  if (h.locked) return "locked";
  if (h.via_default) return "default";
  if (h.comps.every((c) => c === null)) return "ref";
  return "literal";
}

/** Only references, no literal: without Alt the handle only snaps (never becomes numbers). */
export function pureRef(h: Handle): boolean {
  return !h.locked && h.has_ref && h.comps.every((c) => c === null);
}

export function isLinear(h: Handle): boolean {
  return h.kind.kind === "radius" || h.kind.kind === "height" || h.kind.kind === "plane" || h.kind.kind === "scale";
}

export function isAngle(h: Handle): boolean {
  return h.kind.kind === "angle";
}

/** German label of a handle kind (tooltips, report). */
export function handleLabel(h: Handle): string {
  switch (h.kind.kind) {
    case "point":
      return "Punkt";
    case "radius":
      return "Radius";
    case "height":
      return "Höhe";
    case "angle":
      return `Drehung ${["x", "y", "z", "Achse"][h.kind.axis] ?? ""}`.trim();
    case "plane":
      return `Ebene ${["x", "y", "z"][h.kind.axis]}`;
    case "scale":
      return h.kind.axis === null ? "Skalierung" : `Skalierung ${["x", "y", "z"][h.kind.axis]}`;
  }
}

/**
 * Handles shown for the cursor: those of the selected statement; for statements without
 * geometry (assignments, grids, anchors) the innermost handle statement containing the
 * cursor that starts on the cursor line (grids: anywhere inside the block).
 */
export function handlesForCursor(scene: Scene, idx: SceneIndex, doc: Text, pos: number, sel: CursorSelection): Handle[] {
  const all = [...scene.handles, ...scene.locked_handles];
  if (sel.stmt !== null) {
    const hs = all.filter((h) => h.stmt === sel.stmt);
    if (hs.length) return hs;
  }
  // Sketch: the cursor line's segment handles, else (on the `sketch` line, a loop, an `if`
  // in it) the handles of all its segments.
  const sketches = sel.nodes.flatMap((n) => sketchesFor(scene, scene.nodes[n]).map((s) => s.idx));
  const ofSketch = all.filter((h) => h.node !== null && sketches.includes(h.node));
  const line = doc.lineAt(Math.min(pos, doc.length));
  const gridStmts = new Set(scene.grids.map((g) => g.stmt));
  let best: number | null = null;
  let size = Infinity;
  for (const h of all) {
    const r = idx.range(h.range);
    if (!(r.from <= pos && pos <= r.to)) continue;
    if (r.from < line.from && !gridStmts.has(h.stmt)) continue;
    if (r.to - r.from < size) {
      size = r.to - r.from;
      best = h.stmt;
    }
  }
  if (best !== null) return all.filter((h) => h.stmt === best);
  // in a sketch: the segments starting on the cursor line (cursor in the indentation), else all
  const onLine = ofSketch.filter((h) => {
    const r = idx.range(h.range);
    return r.from >= line.from && r.from <= line.to;
  });
  return onLine.length ? onLine : ofSketch;
}

/** A 1D handle as a line in context coordinates: grip = base + dir * value. */
export interface HandleLine {
  base: Vec3;
  dir: Vec3;
  value: number;
}

export interface HandleRing {
  center: Vec3;
  /** Unit normal (rotation axis). */
  normal: Vec3;
  /** Unit reference direction (angle 0). */
  u: Vec3;
  /** u rotated by 90° about the normal. */
  v: Vec3;
  radius: number;
  value: number;
}

export interface HandleGeom {
  handle: Handle;
  style: HandleStyle;
  /** Grip position (context coordinates). */
  grip: Vec3;
  line: HandleLine | null;
  ring: HandleRing | null;
}

function cross(a: Vec3, b: Vec3): Vec3 {
  return [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
}

function dot(a: Vec3, b: Vec3): number {
  return a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
}

function norm(a: Vec3): Vec3 {
  const l = length(a);
  return l > 1e-12 ? scale(a, 1 / l) : [0, 0, 0];
}

function anyPerp(n: Vec3): Vec3 {
  const t: Vec3 = Math.abs(n[0]) < 0.9 ? [1, 0, 0] : [0, 1, 0];
  return norm(cross(n, t));
}

function rotateAbout(u: Vec3, n: Vec3, deg: number): Vec3 {
  const a = (deg * Math.PI) / 180;
  const w = cross(n, u);
  return add(scale(u, Math.cos(a)), scale(w, Math.sin(a)));
}

/** Child bounds of a scale node (corner of the hull for the scale grip). */
function scaleCorner(scene: Scene, h: Handle): Vec3 | null {
  if (h.node === null) return null;
  const n = scene.nodes[h.node];
  const c = n?.children[0] !== undefined ? scene.nodes[n.children[0]] : null;
  if (!c?.bounds) return null;
  const m = c.bounds.max;
  return Math.abs(m[0]) + Math.abs(m[1]) + Math.abs(m[2]) > 1e-9 ? m : null;
}

/**
 * Display geometry of a handle. `ringRadius`: size of rotation rings (context units).
 * `hull`: model bounds; grid plane grips sit on an edge of the grid's box (one edge per
 * axis, so the grips of planes at 0 do not coincide).
 */
export function handleGeom(scene: Scene, h: Handle, ringRadius: number, hull: Box3 | null = null): HandleGeom {
  const style = handleStyle(h);
  const f = h.frame;
  if (h.kind.kind === "angle") {
    const axis = h.axis ?? [0, 0, 1];
    const normal = norm(applyDir(f, axis));
    const k = h.kind.axis;
    const u0 = k < 3 ? norm(applyDir(f, [k === 2 ? 1 : 0, k === 0 ? 1 : 0, k === 1 ? 1 : 0])) : anyPerp(normal);
    // Remove any component along the normal (non-orthogonal frames).
    const u = norm(sub(u0, scale(normal, dot(u0, normal))));
    const v = cross(normal, u);
    const center = apply(f, h.base ?? [0, 0, 0]);
    const value = h.value[0] ?? 0;
    const grip = add(center, scale(rotateAbout(u, normal, value), ringRadius));
    return { handle: h, style, grip, line: null, ring: { center, normal, u, v, radius: ringRadius, value } };
  }
  if (isLinear(h) && h.value.length === 1) {
    let baseL: Vec3 = h.base ?? [0, 0, 0];
    let dirL: Vec3 = h.axis ?? [1, 0, 0];
    if (h.kind.kind === "plane") {
      const g = scene.grids.find((x) => x.stmt === h.stmt && x.in_context);
      if (g) {
        const box = gridBox(g, hull);
        const a = h.kind.axis;
        const b = (a + 1) % 3;
        const c = (a + 2) % 3;
        baseL = [0, 0, 0];
        baseL[b] = box.max[b];
        baseL[c] = box.min[c];
      }
    }
    if (h.kind.kind === "scale") {
      // Grip at a corner of the hull of the scaled subtree (Spec: Griff an einer Ecke der Hülle).
      const c = scaleCorner(scene, h);
      const a = h.kind.axis;
      baseL = [0, 0, 0];
      if (a === null) dirL = c ?? [1, 1, 1];
      else {
        dirL = [0, 0, 0];
        dirL[a] = c && Math.abs(c[a]) > 1e-9 ? c[a] : 1;
      }
    }
    const base = apply(f, baseL);
    const dir = applyDir(f, dirL);
    const value = h.value[0];
    return { handle: h, style, grip: add(base, scale(dir, value)), line: { base, dir, value }, ring: null };
  }
  return { handle: h, style, grip: h.world_pos, line: null, ring: null };
}

// ---------------------------------------------------------------- mouse ray → value

export interface Ray {
  origin: Vec3;
  dir: Vec3;
}

/** Intersection of a ray with the plane through `p` with normal `n` (null if parallel or behind). */
export function rayPlane(ray: Ray, p: Vec3, n: Vec3): Vec3 | null {
  const d = dot(ray.dir, n);
  if (Math.abs(d) < 1e-9) return null;
  const t = dot(sub(p, ray.origin), n) / d;
  if (t < 0) return null;
  return add(ray.origin, scale(ray.dir, t));
}

/** Parameter `t` of the point `base + dir * t` closest to the ray. */
export function rayLineParam(ray: Ray, base: Vec3, dir: Vec3): number | null {
  const a = dot(dir, dir);
  const b = dot(dir, ray.dir);
  const c = dot(ray.dir, ray.dir);
  const w = sub(base, ray.origin);
  const d = dot(dir, w);
  const e = dot(ray.dir, w);
  const den = a * c - b * b;
  if (a < 1e-18 || Math.abs(den) < 1e-12 * a * c) return null;
  return (b * e - c * d) / den;
}

/** Angle in degrees of the ray's hit on the ring plane, unwrapped near `prev`. */
export function ringAngle(ray: Ray, ring: HandleRing, prev: number): number | null {
  const q = rayPlane(ray, ring.center, ring.normal) ?? rayPlaneBoth(ray, ring.center, ring.normal);
  if (!q) return null;
  const d = sub(q, ring.center);
  if (length(d) < 1e-9) return null;
  let deg = (Math.atan2(dot(d, ring.v), dot(d, ring.u)) * 180) / Math.PI;
  while (deg - prev > 180) deg -= 360;
  while (deg - prev < -180) deg += 360;
  return deg;
}

function rayPlaneBoth(ray: Ray, p: Vec3, n: Vec3): Vec3 | null {
  const d = dot(ray.dir, n);
  if (Math.abs(d) < 1e-9) return null;
  const t = dot(sub(p, ray.origin), n) / d;
  return add(ray.origin, scale(ray.dir, t));
}

/**
 * Plane for dragging a point handle: the local xy plane for 2D points (unless seen edge-on),
 * otherwise the plane through the grip facing the camera.
 */
export function pointDragPlane(h: Handle, grip: Vec3, viewDir: Vec3): { p: Vec3; n: Vec3 } {
  if (h.value.length === 2) {
    const n = norm(applyDir(h.frame, [0, 0, 1]));
    if (Math.abs(dot(n, norm(viewDir))) > 0.25) return { p: grip, n };
  }
  return { p: grip, n: norm(viewDir) };
}

/** Context point → the handle's local system (what `DragTarget.point` expects). */
export function toLocal(h: Handle, world: Vec3): Vec3 {
  return apply(invertAffine(h.frame), world);
}

// ---------------------------------------------------------------- snapping

export interface ScreenPoint {
  x: number;
  y: number;
}

export interface SnapHit {
  target: SnapTarget;
  index: number;
  dist: number;
}

/**
 * Nearest snap target within `radius` pixels of `mouse`. `screen(i)` gives the screen
 * position of target i (null if not visible).
 */
export function nearestSnap(targets: SnapTarget[], screen: (i: number) => ScreenPoint | null, mouse: ScreenPoint, radius: number): SnapHit | null {
  let best: SnapHit | null = null;
  targets.forEach((t, i) => {
    const s = screen(i);
    if (!s) return;
    const d = Math.hypot(s.x - mouse.x, s.y - mouse.y);
    if (d <= radius && (!best || d < best.dist)) best = { target: t, index: i, dist: d };
  });
  return best;
}

/** Same snap target (for de-duplicating drag messages). */
export function sameSnap(a: SnapTarget | null, b: SnapTarget | null): boolean {
  if (!a || !b) return a === b;
  return a.text === b.text;
}

/**
 * Snap targets of a 1D handle moved onto the drawn handle line (plane grips sit on an edge of
 * the grid box, not on the axis through the origin the engine uses).
 */
export function alignTargets(geom: HandleGeom, targets: SnapTarget[]): SnapTarget[] {
  const h = geom.handle;
  const l = geom.line;
  if (!l || !h.axis || h.kind.kind === "scale") return targets;
  const base = h.base ?? [0, 0, 0];
  const aa = dot(h.axis, h.axis);
  if (aa < 1e-18) return targets;
  return targets.map((t) => {
    const v = dot(sub(t.local, base), h.axis!) / aa;
    const w = add(l.base, scale(l.dir, v));
    return { ...t, world: w, result_world: w };
  });
}
