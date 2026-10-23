import { describe, expect, it } from "vitest";
import { gridOverlay, modelHull, refHighlight } from "./grids";
import { SceneIndex } from "./sceneIndex";
import { fixture, pos } from "./__fixtures__/load";

describe("Rasteranzeige", () => {
  const f = fixture("lochplatte-modul");
  const idx = new SceneIndex(f.scene);
  const hull = modelHull(f.scene);

  it("nur die von der Zeile verwendeten Ebenen", () => {
    const sel = idx.select(f.doc, pos(f.doc, 12, 10));
    const g = gridOverlay({ scene: f.scene, hull, all: false, refs: sel.refs, grids: sel.grids, hoverRef: null });
    // box(g[links, vorne, boden], g[rechts, hinten, oben]): 6 planes.
    expect(g.lines.map((l) => f.scene.grids[l.grid].planes[l.plane].name).sort()).toEqual(["boden", "hinten", "links", "oben", "rechts", "vorne"]);
    // `rechts = w` and `hinten = 2 * rand` are expressions (dashed).
    expect(g.lines.filter((l) => l.dashed).map((l) => f.scene.grids[l.grid].planes[l.plane].name).sort()).toEqual(["hinten", "rechts"]);
    // Intersections of g (6 × 3 × 2) and l (2 × 2 × 1).
    expect(g.points).toHaveLength(36 + 4);
  });

  it("Schalter „alle Raster“ zeigt alle expliziten Ebenen", () => {
    const g = gridOverlay({ scene: f.scene, hull, all: true, refs: [], grids: [], hoverRef: null });
    expect(g.lines).toHaveLength(11 + 4);
  });

  it("Rasterdeklaration auf der Zeile zeigt ihr Raster", () => {
    const sel = idx.select(f.doc, pos(f.doc, 6, 8));
    const g = gridOverlay({ scene: f.scene, hull, all: false, refs: sel.refs, grids: sel.grids, hoverRef: null });
    expect(new Set(g.lines.map((l) => l.grid))).toEqual(new Set([0]));
    expect(g.lines).toHaveLength(11);
  });

  it("Ebenen liegen am Rand der Hülle (im Rastersystem)", () => {
    const g = gridOverlay({ scene: f.scene, hull, all: true, refs: [], grids: [], hoverRef: null });
    const links = g.lines.find((l) => f.scene.grids[l.grid].planes[l.plane].name === "links")!;
    expect(links.points).toHaveLength(5);
    expect(links.points.every((p) => p[0] === 0)).toBe(true);
    const ys = links.points.map((p) => p[1]);
    expect(Math.min(...ys)).toBeLessThan(0);
    expect(Math.max(...ys)).toBeGreaterThan(12);
  });

  it("Hover über Rasterpunkt, Rasterlinie, Ebene", () => {
    const point = refHighlight(f.scene, f.scene.refs[idx.refAt(pos(f.doc, 12, 20))!], hull);
    expect(point?.kind).toBe("point");
    expect(point?.points).toEqual([[0, 0, 0]]);
    // Grid l is placed at g[rechts, vorne, boden] = [48, 0, 0].
    const lasche = refHighlight(f.scene, f.scene.refs[idx.refAt(pos(f.doc, 25, 25))!], hull);
    expect(lasche?.points).toEqual([[58, 12, 0]]);
    const line = refHighlight(f.scene, { range: f.scene.refs[0].range, stmt: null, target: { kind: "grid", grid: 0, sel: [{ kind: "plane", plane: 5 }, { kind: "wild" }, { kind: "plane", plane: 10 }] } }, hull);
    expect(line?.kind).toBe("line");
    expect(line?.points[0][0]).toBe(48);
    expect(line?.points[0][2]).toBe(4);
    const plane = refHighlight(f.scene, { range: f.scene.refs[0].range, stmt: null, target: { kind: "plane", grid: 0, plane: 10 } }, hull);
    expect(plane?.kind).toBe("plane");
    expect(plane?.points.every((p) => p[2] === 4)).toBe(true);
  });
});
