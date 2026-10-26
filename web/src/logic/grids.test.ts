import { describe, expect, it } from "vitest";
import { gridOverlay, modelHull, refHighlight } from "./grids";
import { SceneIndex } from "./sceneIndex";
import { fixture, pos } from "./__fixtures__/load";

describe("Grid display", () => {
  const f = fixture("hole-plate-module");
  const idx = new SceneIndex(f.scene);
  const hull = modelHull(f.scene);

  it("only the planes used by the line", () => {
    const sel = idx.select(f.doc, pos(f.doc, 12, 10));
    const g = gridOverlay({ scene: f.scene, hull, all: false, refs: sel.refs, grids: sel.grids, hoverRef: null });
    // box(g[left, front, floor], g[right, back, top]): 6 planes.
    expect(g.lines.map((l) => f.scene.grids[l.grid].planes[l.plane].name).sort()).toEqual(["back", "floor", "front", "left", "right", "top"]);
    // `right = w` and `back = 2 * margin` are expressions (dashed).
    expect(g.lines.filter((l) => l.dashed).map((l) => f.scene.grids[l.grid].planes[l.plane].name).sort()).toEqual(["back", "right"]);
    // Intersections of g (6 × 3 × 2) and l (2 × 2 × 1).
    expect(g.points).toHaveLength(36 + 4);
  });

  it("switch “all grids” shows all explicit planes", () => {
    const g = gridOverlay({ scene: f.scene, hull, all: true, refs: [], grids: [], hoverRef: null });
    expect(g.lines).toHaveLength(11 + 4);
  });

  it("a grid declaration on the line shows its grid", () => {
    const sel = idx.select(f.doc, pos(f.doc, 6, 8));
    const g = gridOverlay({ scene: f.scene, hull, all: false, refs: sel.refs, grids: sel.grids, hoverRef: null });
    expect(new Set(g.lines.map((l) => l.grid))).toEqual(new Set([0]));
    expect(g.lines).toHaveLength(11);
  });

  it("planes lie at the edge of the hull (in the grid system)", () => {
    const g = gridOverlay({ scene: f.scene, hull, all: true, refs: [], grids: [], hoverRef: null });
    const left = g.lines.find((l) => f.scene.grids[l.grid].planes[l.plane].name === "left")!;
    expect(left.points).toHaveLength(5);
    expect(left.points.every((p) => p[0] === 0)).toBe(true);
    const ys = left.points.map((p) => p[1]);
    expect(Math.min(...ys)).toBeLessThan(0);
    expect(Math.max(...ys)).toBeGreaterThan(12);
  });

  it("hover over grid point, grid line, plane", () => {
    const point = refHighlight(f.scene, f.scene.refs[idx.refAt(pos(f.doc, 12, 20))!], hull);
    expect(point?.kind).toBe("point");
    expect(point?.points).toEqual([[0, 0, 0]]);
    // Grid l is placed at g[right, front, floor] = [48, 0, 0].
    const tab = refHighlight(f.scene, f.scene.refs[idx.refAt(pos(f.doc, 25, 21))!], hull);
    expect(tab?.points).toEqual([[58, 12, 0]]);
    const line = refHighlight(f.scene, { range: f.scene.refs[0].range, stmt: null, target: { kind: "grid", grid: 0, sel: [{ kind: "plane", plane: 5 }, { kind: "wild" }, { kind: "plane", plane: 10 }] } }, hull);
    expect(line?.kind).toBe("line");
    expect(line?.points[0][0]).toBe(48);
    expect(line?.points[0][2]).toBe(4);
    const plane = refHighlight(f.scene, { range: f.scene.refs[0].range, stmt: null, target: { kind: "plane", grid: 0, plane: 10 } }, hull);
    expect(plane?.kind).toBe("plane");
    expect(plane?.points.every((p) => p[2] === 4)).toBe(true);
  });
});
