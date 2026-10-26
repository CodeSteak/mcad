// 2D operations of sketches (Plan 17.3, Testbericht 15) in the view logic: the offset contour
// dashed over the input contour, the centre line of a stroke, the zones and new corner pieces of
// fillet2d, the radius handle of corner().

import { describe, expect, it } from "vitest";
import { plan } from "./display";
import { handlesForCursor } from "./handles";
import { SceneIndex } from "./sceneIndex";
import { fixture, pos } from "./__fixtures__/load";

type Name = "sketch-lid" | "sketch-holder" | "sketch-cable-duct";

function planAt(name: Name, line: number, col = 4) {
  const f = fixture(name);
  const idx = new SceneIndex(f.scene);
  const sel = idx.select(f.doc, pos(f.doc, line, col));
  return { f, idx, sel, p: plan({ scene: f.scene, cursor: sel.nodes, hover: [] }) };
}

describe("offset, stroke, fillet2d unter dem Cursor", () => {
  it("offset: Ausgangskontur als Profil, Offset-Kontur gestrichelt", () => {
    // line 30: `offset(r = -(wall + clearance)) outline();`
    const { f, sel, p } = planAt("sketch-lid", 30, 6);
    const n = f.scene.nodes[sel.nodes[0]];
    expect(n.label).toBe("offset");
    expect(n.op_info).toEqual({ kind: "offset", round: true, d: -2.2, chamfer: false });
    const g = p.guides.filter((x) => x.kind === "offset");
    expect(g).toHaveLength(1);
    expect(g[0].dashed).toBe(true);
    // the offset contour lies 2.2 inside the 80 × 50 outline
    const xs = g[0].points.map((q) => q[0]);
    const ys = g[0].points.map((q) => q[1]);
    expect(Math.min(...xs)).toBeCloseTo(2.2, 6);
    expect(Math.max(...xs)).toBeCloseTo(77.8, 6);
    expect(Math.max(...ys)).toBeCloseTo(47.8, 6);
    // the input (the outline module) drawn as profile
    expect(p.items.some((i) => i.mode === "profile" && f.scene.nodes[i.node].label === "outline")).toBe(true);
  });

  it("stroke: Mittellinie gestrichelt, Pfad als Profil", () => {
    // line 22: `linear_extrude(floor_t) stroke(width, cap = "flat") route();` – the stroke node
    const f = fixture("sketch-cable-duct");
    const stroke = f.scene.nodes.find((n) => n.label === "stroke" && n.range.line === 22)!;
    const p = plan({ scene: f.scene, cursor: [stroke.idx], hover: [] });
    expect(stroke.op_info).toEqual({ kind: "stroke", w: 16, cap: "flat", join: "round" });
    const c = p.guides.filter((x) => x.kind === "centerline");
    expect(c).toHaveLength(1);
    expect(c[0].dashed).toBe(true);
    // the path: from [0, 0] along x, the quarter circle to [60, 20], up, the corner, to [20, 50]
    expect(c[0].points[0][0]).toBeCloseTo(0, 9);
    const last = c[0].points[c[0].points.length - 1];
    expect(last[0]).toBeCloseTo(20, 9);
    expect(last[1]).toBeCloseTo(50, 9);
    // the open path (its sketch) as profile
    expect(p.items.some((i) => i.mode === "profile")).toBe(true);
  });

  it("fillet2d: Zone blau, neue Eckbögen als ausgewählte Kanten, Hinweis", () => {
    // line 19: `fillet2d(r_inner) {` (inner block with the zone)
    const { f, sel, p } = planAt("sketch-holder", 19, 10);
    const n = f.scene.nodes[sel.nodes[0]];
    expect(n.label).toBe("fillet2d");
    expect(n.op_info).toEqual({ kind: "blend2d", chamfer: false, r: 5, select: "inside" });
    expect(n.hints).toEqual(["1 Ecke"]);
    expect(p.items.filter((i) => i.mode === "blue")).toHaveLength(1);
    const sel1 = p.edges.filter((e) => e.kind === "selected");
    expect(sel1).toHaveLength(1);
    // the outer block: the five other corners
    const outer = f.scene.nodes.find((x) => x.label === "fillet2d" && x.range.line === 18)!;
    expect(outer.hints).toEqual(["5 Ecken"]);
    expect(outer.selected_edges).toHaveLength(5);
  });

  it("corner(): Radius-Griff auf der Verrundung", () => {
    // module route as context, line 17: `corner(r = 12);`
    const f = fixture("sketch-path");
    const idx = new SceneIndex(f.scene);
    const at = pos(f.doc, 17, 6);
    const hs = handlesForCursor(f.scene, idx, f.doc, at, idx.select(f.doc, at));
    const r = hs.filter((h) => h.kind.kind === "radius");
    expect(r).toHaveLength(1);
    expect(r[0].value).toEqual([12]);
    // on the rounding: 12 · (√2 − 1) from the vertex [60, 50] towards the inside of the turn
    const d = Math.hypot(r[0].world_pos[0] - 60, r[0].world_pos[1] - 50);
    expect(d).toBeCloseTo(12 * (Math.SQRT2 - 1), 6);
  });
});
