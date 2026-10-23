// Grid display (Spec: Raster, Darstellung): planes as thin lines at the boundary of the part's
// hull, intersections as small points; literal planes solid, expression planes dashed. By
// default only the planes the current line uses; a switch shows all. Hovering a grid
// expression highlights its point, line or plane.

import type { Grid, Plane, Ref, Scene, Vec3 } from "../engine/protocol";
import { type Box3, apply, boxDiagonal, emptyBox, expandPoint, invertAffine, isEmpty, transformBox } from "./math";

export interface GridLine {
  grid: number;
  plane: number;
  /** Closed polyline (context coordinates). */
  points: Vec3[];
  dashed: boolean;
  /** Plane used by the current line. */
  used: boolean;
}

export interface GridHighlight {
  kind: "point" | "line" | "plane";
  /** point: one point; line: two; plane: closed rectangle (5 points). */
  points: Vec3[];
}

export interface GridOverlay {
  lines: GridLine[];
  points: Vec3[];
  highlight: GridHighlight[];
}

export interface GridInput {
  scene: Scene;
  /** Bounds of the shown model in context coordinates. */
  hull: Box3 | null;
  /** Show all planes of all grids of the module. */
  all: boolean;
  /** References used by the current line (indices into `scene.refs`). */
  refs: number[];
  /** Grids declared on the current line (all their planes are shown). */
  grids: number[];
  /** Hovered reference expression (index into `scene.refs`). */
  hoverRef: number | null;
}

const MAX_POINTS = 4000;

/** Hull of a grid in its own system: model bounds ∪ plane extents, padded. */
export function gridBox(g: Grid, hull: Box3 | null): Box3 {
  const b = emptyBox();
  if (hull && !isEmpty(hull)) {
    const local = transformBox(invertAffine(g.frame), hull);
    expandPoint(b, local.min);
    expandPoint(b, local.max);
  }
  for (const p of g.planes) {
    const q: Vec3 = [NaN, NaN, NaN];
    for (let k = 0; k < 3; k++) q[k] = k === p.axis ? p.value : isEmpty(b) ? 0 : b.min[k];
    expandPoint(b, q);
  }
  if (isEmpty(b)) return { min: [0, 0, 0], max: [0, 0, 0] };
  const pad = Math.max(1, boxDiagonal(b) * 0.04);
  return {
    min: [b.min[0] - pad, b.min[1] - pad, b.min[2] - pad],
    max: [b.max[0] + pad, b.max[1] + pad, b.max[2] + pad],
  };
}

function planeRect(axis: number, value: number, box: Box3): Vec3[] {
  const [b, c] = [(axis + 1) % 3, (axis + 2) % 3];
  const pt = (vb: number, vc: number): Vec3 => {
    const p: Vec3 = [0, 0, 0];
    p[axis] = value;
    p[b] = vb;
    p[c] = vc;
    return p;
  };
  return [pt(box.min[b], box.min[c]), pt(box.max[b], box.min[c]), pt(box.max[b], box.max[c]), pt(box.min[b], box.max[c]), pt(box.min[b], box.min[c])];
}

/** Planes a reference uses (indices into `grid.planes`). */
export function refPlanes(r: Ref): { grid: number; planes: number[] } | null {
  const t = r.target;
  if (t.kind === "plane") return { grid: t.grid, planes: [t.plane] };
  if (t.kind === "grid") {
    const planes: number[] = [];
    for (const s of t.sel) if (s.kind === "plane") planes.push(s.plane);
    return { grid: t.grid, planes };
  }
  return null;
}

/** Point, line or plane of a hovered grid reference (context coordinates). */
export function refHighlight(scene: Scene, r: Ref, hull: Box3 | null): GridHighlight | null {
  const t = r.target;
  if (t.kind === "anchor") return null;
  const g = scene.grids.find((x) => x.id === t.grid);
  if (!g) return null;
  const box = gridBox(g, hull);
  const vals: Array<number | null> = [null, null, null];
  if (t.kind === "plane") {
    const p = g.planes[t.plane];
    if (!p) return null;
    return { kind: "plane", points: planeRect(p.axis, p.value, box).map((q) => apply(g.frame, q)) };
  }
  t.sel.forEach((s, k) => {
    if (s.kind === "plane") vals[k] = g.planes[s.plane]?.value ?? 0;
    else if (s.kind === "coord") vals[k] = s.value;
  });
  const wild = vals.flatMap((v, k) => (v === null ? [k] : []));
  if (wild.length === 0) return { kind: "point", points: [apply(g.frame, vals as Vec3)] };
  if (wild.length === 1) {
    const a: Vec3 = vals.map((v) => v ?? 0) as Vec3;
    const b: Vec3 = [...a];
    a[wild[0]] = box.min[wild[0]];
    b[wild[0]] = box.max[wild[0]];
    return { kind: "line", points: [apply(g.frame, a), apply(g.frame, b)] };
  }
  if (wild.length === 2) {
    const axis = [0, 1, 2].find((k) => !wild.includes(k))!;
    return { kind: "plane", points: planeRect(axis, vals[axis]!, box).map((q) => apply(g.frame, q)) };
  }
  return null;
}

function axisPlanes(g: Grid, axis: number): Plane[] {
  return g.planes.filter((p) => p.axis === axis);
}

export function gridOverlay(input: GridInput): GridOverlay {
  const { scene, hull } = input;
  const used = new Map<number, Set<number>>();
  for (const i of input.refs) {
    const r = scene.refs[i];
    const rp = r && refPlanes(r);
    if (!rp) continue;
    const s = used.get(rp.grid) ?? new Set<number>();
    rp.planes.forEach((p) => s.add(p));
    used.set(rp.grid, s);
  }
  const lines: GridLine[] = [];
  const points: Vec3[] = [];
  for (const g of scene.grids) {
    if (!g.in_context) continue;
    const box = gridBox(g, hull);
    const showAll = input.all || input.grids.includes(g.id);
    const u = used.get(g.id);
    g.planes.forEach((p, idx) => {
      if (p.implicit) return;
      const isUsed = u?.has(idx) ?? false;
      if (!showAll && !isUsed) return;
      lines.push({
        grid: g.id,
        plane: idx,
        points: planeRect(p.axis, p.value, box).map((q) => apply(g.frame, q)),
        dashed: p.literal === null,
        used: isUsed,
      });
    });
    const xs = axisPlanes(g, 0);
    const ys = axisPlanes(g, 1);
    const zs = axisPlanes(g, 2);
    const count = Math.max(1, xs.length) * Math.max(1, ys.length) * Math.max(1, zs.length);
    if (points.length + count > MAX_POINTS) continue;
    const vx = xs.length ? xs.map((p) => p.value) : [0];
    const vy = ys.length ? ys.map((p) => p.value) : [0];
    const vz = zs.length ? zs.map((p) => p.value) : [0];
    for (const x of vx) for (const y of vy) for (const z of vz) points.push(apply(g.frame, [x, y, z]));
  }
  const highlight: GridHighlight[] = [];
  if (input.hoverRef !== null && scene.refs[input.hoverRef]) {
    const h = refHighlight(scene, scene.refs[input.hoverRef], hull);
    if (h) highlight.push(h);
  }
  return { lines, points, highlight };
}

/** Hull of the shown model (root bounds in context coordinates). */
export function modelHull(scene: Scene): Box3 | null {
  if (scene.root === null) return null;
  const r = scene.nodes[scene.root];
  if (!r.bounds) return null;
  return transformBox(r.display_world, { min: r.bounds.min, max: r.bounds.max });
}

/**
 * Box to frame in the view: the model hull plus the grid points of the context module's grids
 * and the anchors (snap targets should be on screen).
 */
export function viewBox(scene: Scene, hull: Box3 | null): Box3 {
  const b = emptyBox();
  if (hull && !isEmpty(hull)) {
    expandPoint(b, hull.min);
    expandPoint(b, hull.max);
  }
  for (const g of scene.grids) {
    if (!g.in_context) continue;
    const ext: Array<[number, number]> = [0, 1, 2].map((k) => {
      const vs = g.planes.filter((p) => p.axis === k).map((p) => p.value);
      return vs.length ? [Math.min(...vs), Math.max(...vs)] : [0, 0];
    }) as Array<[number, number]>;
    for (let i = 0; i < 8; i++) expandPoint(b, apply(g.frame, [ext[0][i & 1], ext[1][(i >> 1) & 1], ext[2][(i >> 2) & 1]]));
  }
  for (const a of scene.anchors) expandPoint(b, a.point);
  return b;
}
