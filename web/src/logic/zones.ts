// Zone tests on tessellated meshes (Spec: Verrundung und Fase, Zonenblock; shell): which of a
// block's selected edges lie in one particular zone (cursor on a zone shows only its edges)
// and which faces of a shell body are opened. The engine selects with the exact kernel
// classification; the view repeats the test on the zone meshes (closed triangle meshes)
// with a tolerance of the tessellation.

import type { Mat4, Vec3 } from "../engine/protocol";
import { apply } from "./math";

export type SelectMode = "inside" | "touch";

/** A closed triangle mesh in context coordinates with its bounds. */
export class MeshVolume {
  readonly tris: Float64Array;
  readonly min: Vec3 = [Infinity, Infinity, Infinity];
  readonly max: Vec3 = [-Infinity, -Infinity, -Infinity];

  constructor(positions: ArrayLike<number>, indices: ArrayLike<number>, matrix: Mat4) {
    const n = Math.floor(indices.length / 3);
    this.tris = new Float64Array(n * 9);
    for (let t = 0; t < n; t++) {
      for (let k = 0; k < 3; k++) {
        const i = indices[t * 3 + k];
        const p = apply(matrix, [positions[i * 3], positions[i * 3 + 1], positions[i * 3 + 2]]);
        for (let c = 0; c < 3; c++) {
          this.tris[t * 9 + k * 3 + c] = p[c];
          if (p[c] < this.min[c]) this.min[c] = p[c];
          if (p[c] > this.max[c]) this.max[c] = p[c];
        }
      }
    }
  }

  get triangles(): number {
    return this.tris.length / 9;
  }

  /** Inside or within `eps` of the surface. */
  contains(p: Vec3, eps: number): boolean {
    for (let c = 0; c < 3; c++) if (p[c] < this.min[c] - eps || p[c] > this.max[c] + eps) return false;
    if (this.distance(p) <= eps) return true;
    // Majority of three ray parities (robust against rays through edges).
    let votes = 0;
    for (const d of RAYS) if (this.crossings(p, d) % 2 === 1) votes++;
    return votes >= 2;
  }

  /** Shortest distance to the triangles. */
  distance(p: Vec3): number {
    let best = Infinity;
    const t = this.tris;
    for (let i = 0; i < t.length; i += 9) {
      const d = pointTriangle(p, t, i);
      if (d < best) best = d;
    }
    return Math.sqrt(best);
  }

  private crossings(o: Vec3, d: Vec3): number {
    let n = 0;
    const t = this.tris;
    for (let i = 0; i < t.length; i += 9) if (rayHits(o, d, t, i)) n++;
    return n;
  }
}

const RAYS: Vec3[] = [
  [0.5773, 0.5774, 0.5776],
  [-0.6123, 0.3536, 0.7071],
  [0.2673, -0.8018, 0.5345],
];

function rayHits(o: Vec3, d: Vec3, t: Float64Array, i: number): boolean {
  const e1x = t[i + 3] - t[i], e1y = t[i + 4] - t[i + 1], e1z = t[i + 5] - t[i + 2];
  const e2x = t[i + 6] - t[i], e2y = t[i + 7] - t[i + 1], e2z = t[i + 8] - t[i + 2];
  const px = d[1] * e2z - d[2] * e2y, py = d[2] * e2x - d[0] * e2z, pz = d[0] * e2y - d[1] * e2x;
  const det = e1x * px + e1y * py + e1z * pz;
  if (Math.abs(det) < 1e-14) return false;
  const inv = 1 / det;
  const sx = o[0] - t[i], sy = o[1] - t[i + 1], sz = o[2] - t[i + 2];
  const u = (sx * px + sy * py + sz * pz) * inv;
  if (u < 0 || u > 1) return false;
  const qx = sy * e1z - sz * e1y, qy = sz * e1x - sx * e1z, qz = sx * e1y - sy * e1x;
  const v = (d[0] * qx + d[1] * qy + d[2] * qz) * inv;
  if (v < 0 || u + v > 1) return false;
  return (e2x * qx + e2y * qy + e2z * qz) * inv > 1e-12;
}

/** Squared distance from p to triangle i (Ericson, closest point on triangle). */
function pointTriangle(p: Vec3, t: Float64Array, i: number): number {
  const ax = t[i], ay = t[i + 1], az = t[i + 2];
  const abx = t[i + 3] - ax, aby = t[i + 4] - ay, abz = t[i + 5] - az;
  const acx = t[i + 6] - ax, acy = t[i + 7] - ay, acz = t[i + 8] - az;
  const apx = p[0] - ax, apy = p[1] - ay, apz = p[2] - az;
  const d1 = abx * apx + aby * apy + abz * apz;
  const d2 = acx * apx + acy * apy + acz * apz;
  const sq = (x: number, y: number, z: number) => x * x + y * y + z * z;
  if (d1 <= 0 && d2 <= 0) return sq(apx, apy, apz);
  const bpx = p[0] - t[i + 3], bpy = p[1] - t[i + 4], bpz = p[2] - t[i + 5];
  const d3 = abx * bpx + aby * bpy + abz * bpz;
  const d4 = acx * bpx + acy * bpy + acz * bpz;
  if (d3 >= 0 && d4 <= d3) return sq(bpx, bpy, bpz);
  const vc = d1 * d4 - d3 * d2;
  if (vc <= 0 && d1 >= 0 && d3 <= 0) {
    const v = d1 / (d1 - d3);
    return sq(apx - v * abx, apy - v * aby, apz - v * abz);
  }
  const cpx = p[0] - t[i + 6], cpy = p[1] - t[i + 7], cpz = p[2] - t[i + 8];
  const d5 = abx * cpx + aby * cpy + abz * cpz;
  const d6 = acx * cpx + acy * cpy + acz * cpz;
  if (d6 >= 0 && d5 <= d6) return sq(cpx, cpy, cpz);
  const vb = d5 * d2 - d1 * d6;
  if (vb <= 0 && d2 >= 0 && d6 <= 0) {
    const w = d2 / (d2 - d6);
    return sq(apx - w * acx, apy - w * acy, apz - w * acz);
  }
  const va = d3 * d6 - d5 * d4;
  if (va <= 0 && d4 - d3 >= 0 && d5 - d6 >= 0) {
    const w = (d4 - d3) / (d4 - d3 + (d5 - d6));
    const bcx = t[i + 6] - t[i + 3], bcy = t[i + 7] - t[i + 4], bcz = t[i + 8] - t[i + 5];
    return sq(bpx - w * bcx, bpy - w * bcy, bpz - w * bcz);
  }
  const den = 1 / (va + vb + vc);
  const v = vb * den;
  const w = vc * den;
  return sq(apx - abx * v - acx * w, apy - aby * v - acy * w, apz - abz * v - acz * w);
}

/** Points along a polyline: its vertices plus `per` samples on each segment. */
export function samplePolyline(pts: Vec3[], per = 2): Vec3[] {
  const out: Vec3[] = [];
  for (let i = 0; i < pts.length; i++) {
    out.push(pts[i]);
    if (i + 1 < pts.length) {
      for (let k = 1; k <= per; k++) {
        const f = k / (per + 1);
        out.push([pts[i][0] + (pts[i + 1][0] - pts[i][0]) * f, pts[i][1] + (pts[i + 1][1] - pts[i][1]) * f, pts[i][2] + (pts[i + 1][2] - pts[i][2]) * f]);
      }
    }
  }
  return out;
}

/** Zone membership of a point set: all points inside ("inside") or any ("touch"). */
export function inZone(zone: MeshVolume, pts: Vec3[], mode: SelectMode, eps: number): boolean {
  if (!pts.length) return false;
  return mode === "touch" ? pts.some((p) => zone.contains(p, eps)) : pts.every((p) => zone.contains(p, eps));
}

/** Edges (context polylines) that belong to a zone. */
export function edgesInZone(edges: Vec3[][], zone: MeshVolume, mode: SelectMode, eps: number): Vec3[][] {
  return edges.filter((e) => inZone(zone, samplePolyline(e), mode, eps));
}

export interface MeshFaces {
  positions: ArrayLike<number>;
  indices: ArrayLike<number>;
  tri_faces: ArrayLike<number>;
}

/**
 * Faces of a body mesh that lie in one of the zones: face index → its triangles. Vertices of
 * the face (context coordinates) are tested; "inside": all in one zone, "touch": any.
 */
export function facesInZones(mesh: MeshFaces, matrix: Mat4, zones: MeshVolume[], mode: SelectMode, eps: number): Map<number, number[]> {
  const byFace = new Map<number, number[]>();
  const nt = Math.floor(mesh.indices.length / 3);
  for (let t = 0; t < nt; t++) {
    const f = mesh.tri_faces[t];
    const l = byFace.get(f);
    if (l) l.push(t);
    else byFace.set(f, [t]);
  }
  const out = new Map<number, number[]>();
  for (const [f, tris] of byFace) {
    const seen = new Set<number>();
    const pts: Vec3[] = [];
    for (const t of tris) {
      for (let k = 0; k < 3; k++) {
        const i = mesh.indices[t * 3 + k];
        if (seen.has(i)) continue;
        seen.add(i);
        pts.push(apply(matrix, [mesh.positions[i * 3], mesh.positions[i * 3 + 1], mesh.positions[i * 3 + 2]]));
      }
    }
    // Large faces: test a subset of the vertices (and always the extremes of the list).
    const sample = pts.length > 400 ? pts.filter((_, i) => i % Math.ceil(pts.length / 400) === 0) : pts;
    if (zones.some((z) => inZone(z, sample, mode, eps))) out.set(f, tris);
  }
  return out;
}
