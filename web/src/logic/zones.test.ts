import { describe, expect, it } from "vitest";
import type { Mat4, Vec3 } from "../engine/protocol";
import { IDENTITY } from "./math";
import { MeshVolume, edgesInZone, facesInZones, samplePolyline } from "./zones";
import { needsRefit } from "./fit";

/** Closed box mesh (12 triangles, one face index per side). */
function boxMesh(min: Vec3, max: Vec3) {
  const [x0, y0, z0] = min;
  const [x1, y1, z1] = max;
  const positions = [x0, y0, z0, x1, y0, z0, x1, y1, z0, x0, y1, z0, x0, y0, z1, x1, y0, z1, x1, y1, z1, x0, y1, z1];
  const quads = [
    [0, 3, 2, 1], // bottom
    [4, 5, 6, 7], // top
    [0, 1, 5, 4],
    [1, 2, 6, 5],
    [2, 3, 7, 6],
    [3, 0, 4, 7],
  ];
  const indices: number[] = [];
  const tri_faces: number[] = [];
  quads.forEach((q, f) => {
    indices.push(q[0], q[1], q[2], q[0], q[2], q[3]);
    tri_faces.push(f, f);
  });
  return { positions, indices, tri_faces };
}

const T = (x: number, y: number, z: number): Mat4 => [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, x, y, z, 1];

describe("Zones on meshes", () => {
  const zone = boxMesh([-1, -1, 9], [21, 21, 11]); // plate around z = 10
  const vol = new MeshVolume(zone.positions, zone.indices, IDENTITY);

  it("point inside, outside, on the boundary (tolerance)", () => {
    expect(vol.triangles).toBe(12);
    expect(vol.contains([5, 5, 10], 1e-6)).toBe(true);
    expect(vol.contains([5, 5, 12], 1e-6)).toBe(false);
    expect(vol.contains([5, 5, 11.0005], 1e-3)).toBe(true);
    expect(vol.contains([30, 5, 10], 1e-3)).toBe(false);
    // Placement by a matrix.
    const moved = new MeshVolume(zone.positions, zone.indices, T(100, 0, 0));
    expect(moved.contains([105, 5, 10], 1e-6)).toBe(true);
    expect(moved.contains([5, 5, 10], 1e-6)).toBe(false);
  });

  it("edges: inside = completely, touch = touches", () => {
    const edges: Vec3[][] = [
      [[0, 0, 10], [20, 0, 10]], // in the plate
      [[0, 0, 0], [0, 0, 10]], // ends in the plate
      [[0, 0, 0], [20, 0, 0]], // outside
    ];
    expect(edgesInZone(edges, vol, "inside", 1e-3)).toEqual([edges[0]]);
    expect(edgesInZone(edges, vol, "touch", 1e-3)).toEqual([edges[0], edges[1]]);
    expect(samplePolyline([[0, 0, 0], [3, 0, 0]], 2)).toEqual([[0, 0, 0], [1, 0, 0], [2, 0, 0], [3, 0, 0]]);
  });

  it("opened faces: only the face completely in the zone", () => {
    const body = boxMesh([0, 0, 0], [20, 20, 10]);
    const faces = facesInZones(body, IDENTITY, [vol], "inside", 1e-3);
    expect([...faces.keys()]).toEqual([1]);
    expect(faces.get(1)).toEqual([2, 3]);
    // touch: the four sides touch the plate too.
    expect([...facesInZones(body, IDENTITY, [vol], "touch", 1e-3).keys()].sort()).toEqual([1, 2, 3, 4, 5]);
  });
});

describe("Refit", () => {
  const b = (min: Vec3, max: Vec3) => ({ min, max });
  it("a clear change of size or position", () => {
    expect(needsRefit(b([0, 0, 0], [10, 10, 10]), b([0, 0, 0], [11, 11, 11]))).toBe(false);
    expect(needsRefit(b([0, 0, 0], [10, 10, 10]), b([0, 0, 0], [20, 20, 20]))).toBe(true);
    expect(needsRefit(b([0, 0, 0], [30, 30, 30]), b([0, 0, 0], [10, 10, 10]))).toBe(true);
    expect(needsRefit(b([0, 0, 0], [10, 10, 10]), b([20, 0, 0], [30, 10, 10]))).toBe(true);
    expect(needsRefit(b([0, 0, 0], [10, 10, 10]), b([2, 0, 0], [12, 10, 10]))).toBe(false);
  });
});
