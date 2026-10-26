import { describe, expect, it } from "vitest";
import type { Vec3 } from "../engine/protocol";
import { cleanPath, evalSpline, interpolate, splinePoints, sweepPath, sweepStartFrame } from "./curves";

const dist = (a: Vec3, b: Vec3) => Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]);

describe("Paths of sweep", () => {
  it("duplicates are dropped, collinear paths become a segment", () => {
    expect(cleanPath([[0, 0, 0], [0, 0, 0], [5, 0, 0], [10, 0, 0]])).toEqual([[0, 0, 0], [10, 0, 0]]);
    expect(cleanPath([[0, 0, 0], [5, 0, 0], [5, 5, 0]])).toHaveLength(3);
  });

  it("cubic interpolation passes through all points (chord length parameter)", () => {
    const pts: Vec3[] = [[0, 0, 0], [20, 0, 10], [40, 10, 10], [60, 10, 0], [70, 0, 0]];
    const c = interpolate(pts)!;
    expect(c.degree).toBe(3);
    expect(c.ctrl).toHaveLength(5);
    // Parameters by chord length: the curve hits every point at its parameter.
    let total = 0;
    const d = pts.slice(1).map((p, i) => ((total += dist(p, pts[i])), dist(p, pts[i])));
    let u = 0;
    pts.forEach((p, i) => {
      if (i > 0) u += d[i - 1] / total;
      expect(dist(evalSpline(c, Math.min(u, 1)), p)).toBeLessThan(1e-9);
    });
  });

  it("three points: two extra points on the parabola (as in the kernel)", () => {
    const sp = splinePoints([[0, 0, 0], [10, 0, 10], [20, 0, 0]]);
    expect(sp).toHaveLength(5);
    expect(sp[1][2]).toBeGreaterThan(0);
    expect(sp[1][0]).toBeCloseTo(5);
  });

  it("polyline: spline sampled finely, linear unchanged; start tangent", () => {
    const lin = sweepPath([[0, 0, 0], [10, 0, 0], [10, 10, 0]], false)!;
    expect(lin.points).toEqual([[0, 0, 0], [10, 0, 0], [10, 10, 0]]);
    expect(lin.tangent).toEqual([1, 0, 0]);
    const sp = sweepPath([[0, 0, 0], [10, 0, 0], [10, 10, 0]], true)!;
    expect(sp.spline).toBe(true);
    expect(sp.points.length).toBeGreaterThan(30);
    expect(sp.points[0]).toEqual([0, 0, 0]);
    expect(dist(sp.points.at(-1)!, [10, 10, 0])).toBeLessThan(1e-9);
    // The spline bends towards +y right away.
    expect(sp.tangent[1]).toBeLessThan(0);
    expect(Math.hypot(...sp.tangent)).toBeCloseTo(1);
  });

  it("start frame: z along the tangent, x = global X projected, right-handed", () => {
    const m = sweepStartFrame([1, 2, 3], [0, 0, 1]);
    expect(m.slice(0, 3)).toEqual([1, 0, 0]);
    expect(m.slice(8, 11)).toEqual([0, 0, 1]);
    expect(m.slice(12, 15)).toEqual([1, 2, 3]);
    // Tangent along x: x axis falls back to global Y.
    const n = sweepStartFrame([0, 0, 0], [1, 0, 0]);
    expect(n.slice(0, 3)).toEqual([0, 1, 0]);
    expect(n.slice(4, 7)).toEqual([0, 0, 1]);
  });
});
