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

function planAt(name: "skizze" | "skizze-modul", line: number, col = 4) {
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

describe("Skizze unter dem Cursor", () => {
  it("Segmentzeile: Skizze in ihrer Ebene, Segment der Zeile hervorgehoben", () => {
    // line 12: `arc(to = s[bogen, oben], r = b / 2);` in module halter
    const { f, p } = planAt("skizze-modul", 12);
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

  it("Zeile im `hole`-Block: Skizze über die Gruppe gefunden", () => {
    const { f, sel, p } = planAt("skizze-modul", 22);
    expect(f.scene.nodes[sel.nodes[0]].label).toBe("group");
    expect(sketchesFor(f.scene, f.scene.nodes[sel.nodes[0]]).map((n) => n.label)).toEqual(["sketch"]);
    expect(p.sketches[0].segments.filter((g) => g.active).map((g) => g.kind)).toEqual(["line"]);
  });

  it("Bézier: Kontrollpolygon als Hilfslinie", () => {
    const { p } = planAt("skizze", 36);
    const sk = p.sketches[0];
    expect(sk.segments.filter((g) => g.active).map((g) => g.kind)).toEqual(["bezier"]);
    // start, two control points, end
    expect(sk.helpers.some((h) => h.length === 4 && h[1][0] === 6 && h[1][1] === 10)).toBe(true);
  });

  it("Aufruf eines Skizzen-Moduls zeigt die Skizze, ohne hervorgehobenes Segment", () => {
    const { p } = planAt("skizze", 29, 0);
    expect(p.sketches).toHaveLength(1);
    expect(p.sketches[0].segments.some((g) => g.active)).toBe(false);
  });
});

describe("Griffe der Segmente", () => {
  it("Segmentzeile: Griffe dieses Segments; Zeile `sketch`: alle Segmentgriffe", () => {
    const f = fixture("skizze-modul");
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

describe("Module als Namensräume", () => {
  it("Modul unter dem Cursor mit Pfad, Signatur des Kontextmoduls", () => {
    const f = fixture("bibliothek");
    const idx = new SceneIndex(f.scene);
    expect(idx.moduleAt(pos(f.doc, 7, 8))).toBe("profile.stern");
    expect(idx.moduleAt(pos(f.doc, 33, 12))).toBe("teile.platte.lochplatte");
    expect(idx.moduleAt(pos(f.doc, 26, 8))).toBe("teile.schraube");
    expect(idx.moduleAt(pos(f.doc, 22, 4))).toBe("teile");
    const scene = { ...f.scene, context: { ...f.scene.context, module: "teile.platte.lochplatte", path: ["teile", "platte", "lochplatte"] } };
    expect(contextModule(scene)?.params.map((p) => p.name)).toEqual(["n"]);
  });
});
