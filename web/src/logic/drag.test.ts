import { describe, expect, it } from "vitest";
import type { DragTarget, SnapTarget, Vec3 } from "../engine/protocol";
import { fixture, pos } from "./__fixtures__/load";
import { DragQueue, type DragInput, axisFromMotion, axisLine, dragStep, lockableAxes, targetKey } from "./drag";
import { handleGeom, handlesForCursor } from "./handles";
import { SceneIndex } from "./sceneIndex";

const f = fixture("handles");
const idx = new SceneIndex(f.scene);
const handlesAt = (line: number, col = 0) => {
  const p = pos(f.doc, line, col);
  return handlesForCursor(f.scene, idx, f.doc, p, idx.select(f.doc, p));
};

// Orthographic test camera looking down -z: screen = (x * 10, -y * 10).
const project = (p: Vec3) => ({ x: p[0] * 10, y: -p[1] * 10 });
const rayAt = (sx: number, sy: number) => ({ origin: [sx / 10, -sy / 10, 100] as Vec3, dir: [0, 0, -1] as Vec3 });

const gridPoint: SnapTarget = {
  text: "g[right, _, lid]",
  snap: { kind: "grid", grid: 0, planes: ["right", "_", "lid"] },
  local: [40, 0, 20],
  world: [40, 0, 20],
  result_world: [40, 0, 20],
};

function input(geom: ReturnType<typeof handleGeom>, sx: number, sy: number, extra: Partial<DragInput> = {}): DragInput {
  return { geom, ray: rayAt(sx, sy), mouse: { x: sx, y: sy }, viewDir: [0, 0, -1], alt: false, targets: [gridPoint], project, snapRadius: 12, prevAngle: geom.ring?.value ?? 0, stepMm: 1, stepDeg: 15, ...extra };
}

describe("One drag step", () => {
  const translate = handleGeom(f.scene, handlesAt(7, 0)[0], 10);

  it("free point: numbers in local coordinates, preview rounded", () => {
    const s = dragStep(input(translate, 253, -38));
    expect(s.target?.kind).toBe("point");
    const p = (s.target as Extract<DragTarget, { kind: "point" }>).pos;
    expect(p[0]).toBeCloseTo(25.3);
    expect(p[1]).toBeCloseTo(3.8);
    expect(p[2]).toBeCloseTo(10);
    expect(s.grip).toEqual([25, 4, 10]);
    expect(s.snap).toBeNull();
  });

  it("point near a grid point: reference", () => {
    const s = dragStep(input(translate, 395, 3));
    expect(s.target).toEqual({ kind: "snap", snap: gridPoint.snap });
    expect(s.snap?.text).toBe("g[right, _, lid]");
    expect(s.grip).toEqual([40, 0, 20]);
  });

  it("Alt: no snap, numbers", () => {
    const s = dragStep(input(translate, 395, 3, { alt: true }));
    expect(s.target?.kind).toBe("point");
    expect(s.snap).toBeNull();
  });

  it("a pure reference never becomes numbers without Alt", () => {
    const ref = handleGeom(f.scene, { ...translate.handle, comps: [null, null, null], has_ref: true }, 10);
    expect(dragStep(input(ref, 100, -100)).target).toBeNull();
    expect(dragStep(input(ref, 100, -100, { alt: true })).target?.kind).toBe("point");
    expect(dragStep(input(ref, 398, 0)).target?.kind).toBe("snap");
  });

  it("height: projection onto the handle axis", () => {
    const h = handleGeom(f.scene, handlesAt(9, 0)[0], 10);
    // Seen from the side (ray along +y): screen y ↔ z.
    const side = { ...input(h, 0, 0, { targets: [] }), ray: { origin: [0, -100, 17.4] as Vec3, dir: [0, 1, 0] as Vec3 }, viewDir: [0, 1, 0] as Vec3 };
    const s = dragStep(side);
    expect(s.target).toEqual({ kind: "value", value: expect.closeTo(17.4, 6) });
    expect(s.grip[2]).toBe(17);
  });

  it("rotation ring: angle, preview snapped to 15°", () => {
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

describe("Axis-locked drag", () => {
  const translate = handleGeom(f.scene, handlesAt(7, 0)[0], 10);
  const h = translate.handle;
  const origin = [...h.pos] as Vec3;

  it("only the chosen component changes", () => {
    const s = dragStep(input(translate, 253, -38, { axis: { axis: 0, origin }, targets: [] }));
    const p = (s.target as Extract<DragTarget, { kind: "point" }>).pos;
    expect(p[0]).toBeCloseTo(25.3);
    expect(p[1]).toBe(origin[1]);
    expect(p[2]).toBe(origin[2]);
    expect(s.grip[1]).toBeCloseTo(origin[1]);
    // Along y (screen up = +y in the test camera).
    const t = dragStep(input(translate, 253, -38, { axis: { axis: 1, origin }, targets: [] }));
    const q = (t.target as Extract<DragTarget, { kind: "point" }>).pos;
    expect(q[0]).toBe(origin[0]);
    expect(q[1]).toBeCloseTo(3.8);
  });

  it("only grid points on the axis are snap targets", () => {
    const onAxis: SnapTarget = { ...gridPoint, text: "g[right, …]", local: [40, origin[1], origin[2]], world: [40, origin[1], origin[2]], result_world: [40, origin[1], origin[2]] };
    const off: SnapTarget = { ...gridPoint, local: [40, origin[1] + 5, origin[2]], world: [40, origin[1] + 5, origin[2]], result_world: [40, origin[1] + 5, origin[2]] };
    const near = { x: 400, y: -origin[1] * 10 };
    let s = dragStep(input(translate, near.x, near.y, { axis: { axis: 0, origin }, targets: [off, onAxis] }));
    expect(s.snap?.text).toBe("g[right, …]");
    s = dragStep(input(translate, near.x, near.y - 50, { axis: { axis: 0, origin }, targets: [off] }));
    expect(s.snap).toBeNull();
    expect(s.target?.kind).toBe("point");
  });

  it("axis from the mouse motion; 2D points only x/y", () => {
    expect(lockableAxes(h)).toEqual([0, 1, 2]);
    expect(axisFromMotion(h, origin, { x: 0, y: 0 }, { x: 3, y: 1 }, project)).toBeNull();
    expect(axisFromMotion(h, origin, { x: 0, y: 0 }, { x: 40, y: 6 }, project)).toBe(0);
    expect(axisFromMotion(h, origin, { x: 0, y: 0 }, { x: -5, y: -40 }, project)).toBe(1);
    expect(lockableAxes({ ...h, value: [1, 2] })).toEqual([0, 1]);
    expect(lockableAxes({ ...h, kind: { kind: "height" } })).toEqual([]);
    const l = axisLine(h, { axis: 2, origin }, 5);
    expect(l.to[2] - l.from[2]).toBeCloseTo(10);
  });
});

describe("At most one drag in flight, the last position wins", () => {
  const v = (x: number): DragTarget => ({ kind: "value", value: x });

  it("buffers while an answer is pending", () => {
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

  it("the same position is not sent again", () => {
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

  it("keys of the targets", () => {
    expect(targetKey(v(1))).not.toBe(targetKey(v(1.5)));
    expect(targetKey({ kind: "snap", snap: gridPoint.snap })).toContain("right");
    expect(targetKey(null)).toBe("");
  });
});
