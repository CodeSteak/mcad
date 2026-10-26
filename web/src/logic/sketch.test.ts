// Sketches (Plan 17.1) and modules as namespaces (17.2) in the view logic: the sketch under the
// cursor with the segment of the cursor line, help lines, start marks, the handles of the
// segments, the qualified context module.

import { describe, expect, it } from "vitest";
import type { SrcRange } from "../engine/protocol";
import { plan, sketchesFor } from "./display";
import { handlesForCursor } from "./handles";
import { contextModule } from "./params";
import { SceneIndex } from "./sceneIndex";
import { fixture, pos } from "./__fixtures__/load";

function planAt(name: "sketch" | "sketch-module", line: number, col = 4) {
  const f = fixture(name);
  const idx = new SceneIndex(f.scene);
  const at = pos(f.doc, line, col);
  const sel = idx.select(f.doc, at);
  const l = f.doc.lineAt(at);
  const segmentActive = (r: SrcRange) => {
    const s = idx.range(r);
    return s.from <= l.to && s.to >= l.from;
  };
  return { f, idx, sel, at, p: plan({ scene: f.scene, cursor: sel.nodes, hover: [], segmentActive }) };
}

describe("Sketch under the cursor", () => {
  it("segment line: sketch in its plane, the line's segment highlighted", () => {
    // line 12: `arc(to = s[arc_x, top], r = b / 2);` in module holder
    const { f, p } = planAt("sketch-module", 12);
    expect(p.sketches).toHaveLength(1);
    const sk = p.sketches[0];
    expect(f.scene.nodes[sk.node].label).toBe("sketch");
    expect(sk.closed).toBe(true);
    expect(sk.segments.length).toBe(10);
    const active = sk.segments.filter((g) => g.active);
    expect(active.map((g) => g.kind)).toEqual(["arc"]);
    // the half circle: all points on the circle around (48, 12), radius 12
    for (const q of active[0].points) expect(Math.hypot(q[0] - 48, q[1] - 12)).toBeCloseTo(12, 6);
    // the sketch's mesh is drawn as profile (outline + light fill, through the result)
    expect(p.items.map((i) => `${i.mode}:${f.scene.nodes[i.node].label}`)).toContain("profile:sketch");
    // start marks of the three contours, help lines from the arcs' centres
    expect(sk.starts).toHaveLength(3);
    expect(sk.starts.every((s) => !s.open)).toBe(true);
    expect(sk.starts.filter((s) => s.hole)).toHaveLength(1);
    expect(sk.helpers.length).toBe(5);
  });

  it("line in the `hole` block: sketch found through the group", () => {
    const { f, sel, p } = planAt("sketch-module", 22);
    expect(f.scene.nodes[sel.nodes[0]].label).toBe("group");
    expect(sketchesFor(f.scene, f.scene.nodes[sel.nodes[0]]).map((n) => n.label)).toEqual(["sketch"]);
    expect(p.sketches[0].segments.filter((g) => g.active).map((g) => g.kind)).toEqual(["line"]);
  });

  it("Bézier: control polygon as a help line", () => {
    const { p } = planAt("sketch", 36);
    const sk = p.sketches[0];
    expect(sk.segments.filter((g) => g.active).map((g) => g.kind)).toEqual(["bezier"]);
    // start, two control points, end
    expect(sk.helpers.some((h) => h.length === 4 && h[1][0] === 6 && h[1][1] === 10)).toBe(true);
  });

  it("a call of a sketch module shows the sketch, without a highlighted segment", () => {
    const { p } = planAt("sketch", 29, 0);
    expect(p.sketches).toHaveLength(1);
    expect(p.sketches[0].segments.some((g) => g.active)).toBe(false);
  });
});

describe("Handles of the segments", () => {
  it("segment line: handles of this segment; line `sketch`: all segment handles", () => {
    const f = fixture("sketch-module");
    const idx = new SceneIndex(f.scene);
    const at = (line: number, col = 4) => {
      const p = pos(f.doc, line, col);
      return handlesForCursor(f.scene, idx, f.doc, p, idx.select(f.doc, p)).map((h) => `${h.range.line}:${h.kind.kind}`);
    };
    expect(at(11)).toEqual(["11:point"]);
    expect(at(23)).toEqual(["23:point", "23:radius"]);
    const all = at(5, 30);
    expect(all.length).toBeGreaterThanOrEqual(10);
    // editable and locked (computed values) handles of all segments
    expect(new Set(all.map((h) => h.split(":")[0]))).toEqual(new Set(["10", "11", "12", "13", "16", "17", "18", "21", "22", "23", "24", "25"]));
  });
});

describe("Modules as namespaces", () => {
  it("module under the cursor with its path, signature of the context module", () => {
    const f = fixture("library");
    const idx = new SceneIndex(f.scene);
    expect(idx.moduleAt(pos(f.doc, 7, 8))).toBe("profile.star");
    expect(idx.moduleAt(pos(f.doc, 33, 12))).toBe("parts.plate.hole_plate");
    expect(idx.moduleAt(pos(f.doc, 26, 8))).toBe("parts.screw");
    expect(idx.moduleAt(pos(f.doc, 22, 4))).toBe("parts");
    const scene = { ...f.scene, context: { ...f.scene.context, module: "parts.plate.hole_plate", path: ["parts", "plate", "hole_plate"] } };
    expect(contextModule(scene)?.params.map((p) => p.name)).toEqual(["n"]);
  });
});
