import { describe, expect, it } from "vitest";
import type { Handle, SnapTarget } from "../engine/protocol";
import { fixture, pos } from "./__fixtures__/load";
import { handleGeom, handleStyle, handlesForCursor, nearestSnap, pointDragPlane, pureRef, rayLineParam, rayPlane, ringAngle, toLocal } from "./handles";
import { SceneIndex } from "./sceneIndex";

const f = fixture("griffe");
const idx = new SceneIndex(f.scene);
const at = (line: number, col = 0) => {
  const p = pos(f.doc, line, col);
  return handlesForCursor(f.scene, idx, f.doc, p, idx.select(f.doc, p));
};
const kinds = (hs: Handle[]) => hs.map((h) => `${h.kind.kind}${"axis" in h.kind && h.kind.axis !== null ? h.kind.axis : ""}:${h.index}`);
const close = (a: number[], b: number[], eps = 1e-6) => a.every((x, i) => Math.abs(x - b[i]) < eps);

describe("Griffe der Zeile unter dem Cursor", () => {
  it("Anweisung mit Geometrie: Griffe der gewählten Anweisung", () => {
    expect(kinds(at(7, 0))).toEqual(["point:0"]);
    // Cursor on `cube` in the same line: the cube corner.
    const c = at(7, 16);
    expect(c.map((h) => h.value)).toEqual([[10, 10, 10]]);
    expect(kinds(at(8, 0))).toEqual(["angle0:0", "angle1:1", "angle2:2"]);
    expect(kinds(at(9, 0))).toEqual(["height:0", "radius:0"]);
    expect(kinds(at(11, 0))).toEqual(["scale:0"]);
  });

  it("Zeilen ohne Geometrie: Variable, Rasterblock", () => {
    expect(kinds(at(2, 0))).toEqual(["point:0"]);
    // Anywhere inside the grid block: all literal planes.
    expect(kinds(at(4, 8))).toEqual(["plane0:0", "plane0:1", "plane2:3", "plane2:4"]);
    expect(kinds(at(5, 2))).toHaveLength(4);
    expect(at(12, 0)).toEqual([]);
  });

  it("Stil: Literal, Variable, gesperrt", () => {
    const t = at(7, 0)[0];
    expect(t.via_var).toBe(true);
    expect(handleStyle(t)).toBe("literal");
    const locked = at(10, 0);
    expect(locked).toHaveLength(1);
    expect(locked[0].locked).toBe(true);
    expect(handleStyle(locked[0])).toBe("locked");
    expect(pureRef(locked[0])).toBe(false);
    const ref = { ...t, comps: [null, null, null], has_ref: true };
    expect(handleStyle(ref)).toBe("ref");
    expect(pureRef(ref)).toBe(true);
    expect(handleStyle({ ...t, via_default: true })).toBe("default");
  });
});

describe("Darstellung der Griffe", () => {
  it("Drehring: Griff beim aktuellen Winkel", () => {
    const z = at(8, 0)[2];
    const g = handleGeom(f.scene, z, 10);
    expect(g.ring).not.toBeNull();
    expect(close(g.ring!.normal, [0, 0, 1])).toBe(true);
    expect(close(g.grip, [10 * Math.cos(Math.PI / 6), 10 * Math.sin(Math.PI / 6), 0])).toBe(true);
    // x ring: rotation axis x, reference direction y.
    const x = handleGeom(f.scene, at(8, 0)[0], 10);
    expect(Math.abs(Math.abs(x.ring!.normal[0]) - Math.cos(Math.PI / 6)) < 1e-6).toBe(true);
  });

  it("Höhe und Radius als Linie, Skalierung an der Hüllenecke", () => {
    const [h, r] = at(9, 0);
    const gh = handleGeom(f.scene, h, 10);
    expect(close(gh.grip, [0, 0, 12])).toBe(true);
    expect(close(gh.line!.dir, [0, 0, 1])).toBe(true);
    const gr = handleGeom(f.scene, r, 10);
    expect(close(gr.grip, [4, 0, 0])).toBe(true);
    const gs = handleGeom(f.scene, at(11, 0)[0], 10);
    // Child cube(5): corner [5, 5, 5], factor 2 → [10, 10, 10].
    expect(close(gs.grip, [10, 10, 10])).toBe(true);
    expect(gs.line!.value).toBe(2);
  });

  it("Punktgriff am Ziel", () => {
    const g = handleGeom(f.scene, at(7, 0)[0], 10);
    expect(g.grip).toEqual([20, 0, 10]);
    expect(g.line).toBeNull();
  });
});

describe("Maus → Wert", () => {
  it("Strahl gegen Ebene und Linie", () => {
    const ray = { origin: [5, -10, 6] as [number, number, number], dir: [0, 1, 0] as [number, number, number] };
    expect(rayLineParam(ray, [0, 0, 0], [0, 0, 1])).toBeCloseTo(6);
    expect(rayLineParam(ray, [0, 0, 0], [0, 0, 2])).toBeCloseTo(3);
    expect(rayLineParam(ray, [0, 0, 0], [0, 1, 0])).toBeNull();
    expect(rayPlane(ray, [0, 2, 0], [0, 1, 0])).toEqual([5, 2, 6]);
    expect(rayPlane(ray, [0, -20, 0], [0, 1, 0])).toBeNull();
  });

  it("Winkel auf dem Ring, stetig fortgesetzt", () => {
    const g = handleGeom(f.scene, at(8, 0)[2], 10);
    const a = (deg: number) => (deg * Math.PI) / 180;
    const ray = (deg: number) => ({ origin: [10 * Math.cos(a(deg)), 10 * Math.sin(a(deg)), 10] as [number, number, number], dir: [0, 0, -1] as [number, number, number] });
    expect(ringAngle(ray(60), g.ring!, 30)).toBeCloseTo(60);
    expect(ringAngle(ray(-170), g.ring!, 175)).toBeCloseTo(190);
  });

  it("Ziehebene: 2D-Punkte in ihrer xy-Ebene, sonst zur Kamera", () => {
    const h = at(7, 0)[0];
    expect(pointDragPlane(h, [1, 2, 3], [0, 0, -1]).n).toEqual([0, 0, -1]);
    const h2 = { ...h, value: [1, 2] };
    expect(pointDragPlane(h2, [1, 2, 0], [0.3, 0, -1]).n).toEqual([0, 0, 1]);
    // Seen edge-on: camera plane.
    expect(pointDragPlane(h2, [1, 2, 0], [1, 0, 0]).n).toEqual([1, 0, 0]);
    expect(toLocal(h, [3, 4, 5])).toEqual([3, 4, 5]);
  });
});

describe("Snap im Bildschirmraum", () => {
  const t = (text: string, x: number): SnapTarget => ({ text, snap: { kind: "anchor", inst: 1, name: text }, local: [x, 0, 0], world: [x, 0, 0], result_world: [x, 0, 0] });
  const targets = [t("a", 0), t("b", 10), t("c", 20)];
  const screen = (i: number) => ({ x: targets[i].world[0] * 10, y: 100 });

  it("nächstes Ziel im Fangradius", () => {
    expect(nearestSnap(targets, screen, { x: 96, y: 104 }, 12)?.target.text).toBe("b");
    expect(nearestSnap(targets, screen, { x: 150, y: 100 }, 12)).toBeNull();
    expect(nearestSnap(targets, screen, { x: 150, y: 100 }, 60)?.target.text).toBe("b");
  });

  it("unsichtbare Ziele zählen nicht", () => {
    expect(nearestSnap(targets, (i) => (i === 1 ? null : screen(i)), { x: 100, y: 100 }, 12)).toBeNull();
  });
});
