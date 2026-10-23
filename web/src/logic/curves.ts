// Path curves of `sweep` (Spec: Profiloperationen, "Pfad als Kurve") and the frame the kernel
// places the profile in at the path start. Mirrors the SMLib facade (sc_sweep): duplicate
// points dropped, a collinear path becomes a line, `interp = "spline"` is a cubic B-spline
// interpolating the points with chord-length parameters (Piegl & Tiller A9.1, averaging
// knots); three points get two extra points on the parabola through them first.

import type { Mat4, Vec3 } from "../engine/protocol";
import { add, length, scale, sub } from "./math";

function dot(a: Vec3, b: Vec3): number {
  return a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
}

function cross(a: Vec3, b: Vec3): Vec3 {
  return [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
}

function norm(a: Vec3): Vec3 {
  const l = length(a);
  return l > 1e-15 ? scale(a, 1 / l) : [0, 0, 0];
}

/** Path points as the kernel uses them: consecutive duplicates removed, collinear → 2 points. */
export function cleanPath(path: Vec3[]): Vec3[] {
  const pp: Vec3[] = [];
  for (const p of path) if (!pp.length || length(sub(p, pp[pp.length - 1])) >= 1e-12) pp.push(p);
  if (pp.length < 2) return pp;
  let plen = 0;
  for (let i = 1; i < pp.length; i++) plen += length(sub(pp[i], pp[i - 1]));
  const tol = 1e-9 * Math.max(1, plen);
  const d = norm(sub(pp[pp.length - 1], pp[0]));
  const straight = pp.every((p) => {
    const q = sub(p, pp[0]);
    return length(sub(q, scale(d, dot(q, d)))) <= tol;
  });
  return straight ? [pp[0], pp[pp.length - 1]] : pp;
}

/** Cubic B-spline (clamped) with control points and knots. */
export interface BSpline {
  degree: number;
  knots: number[];
  ctrl: Vec3[];
}

function findSpan(n: number, p: number, u: number, U: number[]): number {
  if (u >= U[n + 1]) return n;
  if (u <= U[p]) return p;
  let lo = p;
  let hi = n + 1;
  let mid = (lo + hi) >> 1;
  while (u < U[mid] || u >= U[mid + 1]) {
    if (u < U[mid]) hi = mid;
    else lo = mid;
    mid = (lo + hi) >> 1;
  }
  return mid;
}

function basis(i: number, u: number, p: number, U: number[]): number[] {
  const N = new Array<number>(p + 1).fill(0);
  const left = new Array<number>(p + 1).fill(0);
  const right = new Array<number>(p + 1).fill(0);
  N[0] = 1;
  for (let j = 1; j <= p; j++) {
    left[j] = u - U[i + 1 - j];
    right[j] = U[i + j] - u;
    let saved = 0;
    for (let r = 0; r < j; r++) {
      const den = right[r + 1] + left[j - r];
      const t = den !== 0 ? N[r] / den : 0;
      N[r] = saved + right[r + 1] * t;
      saved = left[j - r] * t;
    }
    N[j] = saved;
  }
  return N;
}

export function evalSpline(c: BSpline, u: number): Vec3 {
  const n = c.ctrl.length - 1;
  const span = findSpan(n, c.degree, u, c.knots);
  const N = basis(span, u, c.degree, c.knots);
  let p: Vec3 = [0, 0, 0];
  for (let j = 0; j <= c.degree; j++) p = add(p, scale(c.ctrl[span - c.degree + j], N[j]));
  return p;
}

/** Solves A x = b (dense, partial pivoting); b holds vectors. */
function solve(A: number[][], b: Vec3[]): Vec3[] | null {
  const n = A.length;
  const M = A.map((r) => [...r]);
  const B = b.map((v) => [...v] as Vec3);
  for (let k = 0; k < n; k++) {
    let piv = k;
    for (let i = k + 1; i < n; i++) if (Math.abs(M[i][k]) > Math.abs(M[piv][k])) piv = i;
    if (Math.abs(M[piv][k]) < 1e-14) return null;
    [M[k], M[piv]] = [M[piv], M[k]];
    [B[k], B[piv]] = [B[piv], B[k]];
    for (let i = k + 1; i < n; i++) {
      const f = M[i][k] / M[k][k];
      if (f === 0) continue;
      for (let j = k; j < n; j++) M[i][j] -= f * M[k][j];
      B[i] = sub(B[i], scale(B[k], f));
    }
  }
  const x: Vec3[] = new Array(n);
  for (let i = n - 1; i >= 0; i--) {
    let s = B[i];
    for (let j = i + 1; j < n; j++) s = sub(s, scale(x[j], M[i][j]));
    x[i] = scale(s, 1 / M[i][i]);
  }
  return x;
}

/** Cubic interpolation through ≥ 4 points with chord-length parameters. */
export function interpolate(points: Vec3[], degree = 3): BSpline | null {
  const n = points.length - 1;
  const p = Math.min(degree, n);
  if (n < 1) return null;
  const d: number[] = [];
  let total = 0;
  for (let k = 1; k <= n; k++) {
    const l = length(sub(points[k], points[k - 1]));
    d.push(l);
    total += l;
  }
  if (total <= 0) return null;
  const ub = [0];
  for (let k = 1; k < n; k++) ub.push(ub[k - 1] + d[k - 1] / total);
  ub.push(1);
  const U: number[] = [];
  for (let i = 0; i <= p; i++) U.push(0);
  for (let j = 1; j <= n - p; j++) {
    let s = 0;
    for (let i = j; i < j + p; i++) s += ub[i];
    U.push(s / p);
  }
  for (let i = 0; i <= p; i++) U.push(1);
  const A: number[][] = [];
  for (let k = 0; k <= n; k++) {
    const row = new Array<number>(n + 1).fill(0);
    const span = findSpan(n, p, ub[k], U);
    const N = basis(span, ub[k], p, U);
    for (let j = 0; j <= p; j++) row[span - p + j] = N[j];
    A.push(row);
  }
  const ctrl = solve(A, points);
  return ctrl ? { degree: p, knots: U, ctrl } : null;
}

/** The kernel's path points for a spline (three points: two more on the parabola). */
export function splinePoints(pp: Vec3[]): Vec3[] {
  if (pp.length !== 3) return pp;
  const t1 = length(sub(pp[1], pp[0]));
  const t2 = t1 + length(sub(pp[2], pp[1]));
  const q = (t: number): Vec3 => {
    const l0 = ((t - t1) * (t - t2)) / ((0 - t1) * (0 - t2));
    const l1 = ((t - 0) * (t - t2)) / ((t1 - 0) * (t1 - t2));
    const l2 = ((t - 0) * (t - t1)) / ((t2 - 0) * (t2 - t1));
    return add(add(scale(pp[0], l0), scale(pp[1], l1)), scale(pp[2], l2));
  };
  return [pp[0], q(t1 / 2), pp[1], q((t1 + t2) / 2), pp[2]];
}

export interface SweepPath {
  /** Polyline of the path (node system). */
  points: Vec3[];
  /** Unit tangent at the start. */
  tangent: Vec3;
  spline: boolean;
}

/** Path curve of a sweep as a polyline (`samples` per span for splines). */
export function sweepPath(path: Vec3[], spline: boolean, samples = 24): SweepPath | null {
  const pp = cleanPath(path);
  if (pp.length < 2) return null;
  const useSpline = spline && pp.length >= 3;
  if (useSpline) {
    const c = interpolate(splinePoints(pp));
    if (c) {
      const spans = c.ctrl.length - c.degree;
      const n = Math.max(16, spans * samples);
      const pts: Vec3[] = [];
      for (let i = 0; i <= n; i++) pts.push(evalSpline(c, i / n));
      // Start derivative of a clamped B-spline: p / u_{p+1} · (P1 − P0).
      const t = norm(sub(c.ctrl[1], c.ctrl[0]));
      return { points: pts, tangent: length(t) > 0 ? t : norm(sub(pp[1], pp[0])), spline: true };
    }
  }
  return { points: pp, tangent: norm(sub(pp[1], pp[0])), spline: false };
}

/**
 * Frame of the profile at the path start (sc_sweep): origin at the first point, z along the
 * start tangent, x = global X projected into the normal plane (global Y if the tangent is
 * nearly parallel to X), y = z × x.
 */
export function sweepStartFrame(start: Vec3, tangent: Vec3): Mat4 {
  const z = norm(tangent);
  const ref: Vec3 = Math.abs(z[0]) < 0.9 ? [1, 0, 0] : [0, 1, 0];
  const x = norm(sub(ref, scale(z, dot(ref, z))));
  const y = cross(z, x);
  return [x[0], x[1], x[2], 0, y[0], y[1], y[2], 0, z[0], z[1], z[2], 0, start[0], start[1], start[2], 1];
}

/** rotate_extrude: profile xy → xz (x → x, y → z), as the engine maps it. */
export const REVOLVE_PROFILE: Mat4 = [1, 0, 0, 0, 0, 0, 1, 0, 0, -1, 0, 0, 0, 0, 0, 1];
