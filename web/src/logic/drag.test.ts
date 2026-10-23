import { describe, expect, it } from "vitest";
import type { DragTarget, SnapTarget, Vec3 } from "../engine/protocol";
import { fixture, pos } from "./__fixtures__/load";
import { DragQueue, type DragInput, dragStep, targetKey } from "./drag";
import { handleGeom, handlesForCursor } from "./handles";
import { SceneIndex } from "./sceneIndex";

const f = fixture("griffe");
const idx = new SceneIndex(f.scene);
const handlesAt = (line: number, col = 0) => {
  const p = pos(f.doc, line, col);
  return handlesForCursor(f.scene, idx, f.doc, p, idx.select(f.doc, p));
};

// Orthographic test camera looking down -z: screen = (x * 10, -y * 10).
const project = (p: Vec3) => ({ x: p[0] * 10, y: -p[1] * 10 });
const rayAt = (sx: number, sy: number) => ({ origin: [sx / 10, -sy / 10, 100] as Vec3, dir: [0, 0, -1] as Vec3 });

const gridPoint: SnapTarget = {
  text: "g[rechts, _, deckel]",
  snap: { kind: "grid", grid: 0, planes: ["rechts", "_", "deckel"] },
  local: [40, 0, 20],
  world: [40, 0, 20],
  result_world: [40, 0, 20],
};

function input(geom: ReturnType<typeof handleGeom>, sx: number, sy: number, extra: Partial<DragInput> = {}): DragInput {
  return { geom, ray: rayAt(sx, sy), mouse: { x: sx, y: sy }, viewDir: [0, 0, -1], alt: false, targets: [gridPoint], project, snapRadius: 12, prevAngle: geom.ring?.value ?? 0, stepMm: 1, stepDeg: 15, ...extra };
}

describe("Ein Schritt beim Ziehen", () => {
  const translate = handleGeom(f.scene, handlesAt(7, 0)[0], 10);

  it("Punkt frei: Zahlen in lokalen Koordinaten, Vorschau gerundet", () => {
    const s = dragStep(input(translate, 253, -38));
    expect(s.target?.kind).toBe("point");
    const p = (s.target as Extract<DragTarget, { kind: "point" }>).pos;
    expect(p[0]).toBeCloseTo(25.3);
    expect(p[1]).toBeCloseTo(3.8);
    expect(p[2]).toBeCloseTo(10);
    expect(s.grip).toEqual([25, 4, 10]);
    expect(s.snap).toBeNull();
  });

  it("Punkt nahe einem Rasterpunkt: Referenz", () => {
    const s = dragStep(input(translate, 395, 3));
    expect(s.target).toEqual({ kind: "snap", snap: gridPoint.snap });
    expect(s.snap?.text).toBe("g[rechts, _, deckel]");
    expect(s.grip).toEqual([40, 0, 20]);
  });

  it("Alt: kein Snap, Zahlen", () => {
    const s = dragStep(input(translate, 395, 3, { alt: true }));
    expect(s.target?.kind).toBe("point");
    expect(s.snap).toBeNull();
  });

  it("reine Referenz wird ohne Alt nie zu Zahlen", () => {
    const ref = handleGeom(f.scene, { ...translate.handle, comps: [null, null, null], has_ref: true }, 10);
    expect(dragStep(input(ref, 100, -100)).target).toBeNull();
    expect(dragStep(input(ref, 100, -100, { alt: true })).target?.kind).toBe("point");
    expect(dragStep(input(ref, 398, 0)).target?.kind).toBe("snap");
  });

  it("Höhe: Projektion auf die Griffachse", () => {
    const h = handleGeom(f.scene, handlesAt(9, 0)[0], 10);
    // Seen from the side (ray along +y): screen y ↔ z.
    const side = { ...input(h, 0, 0, { targets: [] }), ray: { origin: [0, -100, 17.4] as Vec3, dir: [0, 1, 0] as Vec3 }, viewDir: [0, 1, 0] as Vec3 };
    const s = dragStep(side);
    expect(s.target).toEqual({ kind: "value", value: expect.closeTo(17.4, 6) });
    expect(s.grip[2]).toBe(17);
  });

  it("Drehring: Winkel, Vorschau auf 15° gerastet", () => {
    const z = handleGeom(f.scene, handlesAt(8, 0)[2], 10);
    const a = (52 * Math.PI) / 180;
    const s = dragStep(input(z, 10 * Math.cos(a) * 10, -10 * Math.sin(a) * 10));
    expect(s.target?.kind).toBe("value");
    expect((s.target as { value: number }).value).toBeCloseTo(52);
    expect(s.angle).toBeCloseTo(52);
    // Preview at 45°.
    expect(s.grip[0]).toBeCloseTo(10 * Math.cos(Math.PI / 4));
    expect(s.grip[1]).toBeCloseTo(10 * Math.sin(Math.PI / 4));
  });
});

describe("Höchstens ein Drag in Flug, letzte Position gewinnt", () => {
  const v = (x: number): DragTarget => ({ kind: "value", value: x });

  it("puffert während eine Antwort aussteht", () => {
    const sent: DragTarget[] = [];
    const q = new DragQueue((t) => sent.push(t));
    expect(q.push(v(1))).toBe(true);
    expect(q.push(v(2))).toBe(false);
    expect(q.push(v(3))).toBe(false);
    expect(q.push(v(4))).toBe(false);
    expect(sent).toEqual([v(1)]);
    expect(q.busy).toBe(true);
    q.answered();
    expect(sent).toEqual([v(1), v(4)]);
    q.answered();
    expect(q.busy).toBe(false);
    expect(q.sent).toBe(2);
    expect(q.dropped).toBe(2);
  });

  it("gleiche Position wird nicht erneut gesendet", () => {
    const sent: DragTarget[] = [];
    const q = new DragQueue((t) => sent.push(t));
    q.push(v(1));
    q.answered();
    expect(q.push(v(1))).toBe(false);
    q.push(v(2));
    q.push(v(1));
    q.answered();
    // The buffered position differs from the one in flight: it follows.
    expect(sent).toEqual([v(1), v(2), v(1)]);
    q.answered();
    q.push(v(1));
    expect(sent).toHaveLength(3);
    expect(q.push(null)).toBe(false);
  });

  it("Schlüssel der Ziele", () => {
    expect(targetKey(v(1))).not.toBe(targetKey(v(1.5)));
    expect(targetKey({ kind: "snap", snap: gridPoint.snap })).toContain("rechts");
    expect(targetKey(null)).toBe("");
  });
});
