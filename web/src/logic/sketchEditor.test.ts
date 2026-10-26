// Sketch editor (Plan 17.12), UI part: step raster, camera onto the plane, snap markers, the
// edits of an answer as one undo step with the cursor after the new code.

import { describe, expect, it } from "vitest";
import { history, undo, undoDepth } from "@codemirror/commands";
import { EditorState, type Transaction } from "@codemirror/state";
import type { Mat4 } from "../engine/protocol";
import { IDENTITY, apply } from "./math";
import { VersionMapper } from "./positions";
import { SKETCH_TOOL_KEYS, pixelSize, planeCamera, rasterLines, rasterSpacing, sketchKeyAction, sketchTransaction, snapMarker } from "./sketchEditor";

describe("Grid of the drawing plane", () => {
  it("spacing: the step, coarsened to at least 12 px", () => {
    expect(rasterSpacing(1, 0.05)).toBe(1);
    expect(rasterSpacing(1, 0.2)).toBe(5);
    expect(rasterSpacing(1, 1)).toBe(20);
    expect(rasterSpacing(0, 0.01)).toBe(1);
  });
  it("lines in both directions, every fifth stronger", () => {
    const l = rasterLines([-2, -1], [10, 4], 2);
    const xs = l.filter((x) => x.a[0] === x.b[0]).map((x) => x.a[0]);
    const ys = l.filter((x) => x.a[1] === x.b[1]).map((x) => x.a[1]);
    expect(xs).toEqual([-2, 0, 2, 4, 6, 8, 10]);
    expect(ys).toEqual([0, 2, 4]);
    expect(l.filter((x) => x.major).map((x) => x.a[0] === x.b[0] ? x.a[0] : x.a[1])).toEqual([0, 10, 0]);
    // too many lines: that axis is left out
    expect(rasterLines([0, 0], [1000, 1], 1, 100).every((x) => x.a[1] === x.b[1])).toBe(true);
  });
});

describe("View perpendicular onto the sketch plane", () => {
  it("xy plane: from the top, target in the middle", () => {
    const c = planeCamera(IDENTITY, [0, 0], [40, 20], 35, 1.5);
    expect(c.target).toEqual([20, 10, 0]);
    const d = [c.eye[0] - c.target[0], c.eye[1] - c.target[1], c.eye[2] - c.target[2]];
    const len = Math.hypot(...d);
    expect(d[2] / len).toBeGreaterThan(0.9999);
    // tilted towards −y: the plane's y is up on the screen
    expect(d[1]).toBeLessThan(0);
    expect(len).toBeGreaterThan(30);
  });
  it("rotated plane (rotate([90, 0, 0])): view along the world's −y", () => {
    // local x → x, y → z, z → −y; origin at z = 5
    const m: Mat4 = [1, 0, 0, 0, 0, 0, 1, 0, 0, -1, 0, 0, 0, 0, 5, 1];
    const c = planeCamera(m, [0, 0], [10, 10], 35, 1);
    expect(c.target).toEqual(apply(m, [5, 5, 0]));
    const d = [c.eye[0] - c.target[0], c.eye[1] - c.target[1], c.eye[2] - c.target[2]];
    expect(d[0]).toBeCloseTo(0, 9);
    expect(d[2]).toBeCloseTo(0, 9);
    expect(d[1]).toBeLessThan(0);
  });
  it("pixel size of the camera", () => {
    expect(pixelSize(90, 100)).toBeCloseTo(0.02, 12);
  });
});

describe("Snap mark per kind", () => {
  it("ring, square, diamond, cross", () => {
    expect(snapMarker("end", [0, 0], 1)).toHaveLength(21);
    expect(snapMarker("grid", [1, 1], 1)).toEqual([[0, 0], [2, 0], [2, 2], [0, 2], [0, 0]]);
    expect(snapMarker("horizontal", [0, 0], 1)[1]).toEqual([0, -1]);
    expect(snapMarker("step", [0, 0], 1)).toHaveLength(5);
  });
});

function editor(doc: string) {
  const ed = {
    state: EditorState.create({ doc, extensions: [history()] }),
    dispatch(tr: Transaction) {
      ed.state = tr.state;
    },
  };
  return ed;
}

describe("Action = one undo step, cursor after the new code", () => {
  const doc = "sketch {\n    start([0, 0]);\n}\n";
  it("insert, cursor, undo", () => {
    const ed = editor(doc);
    const mapper = new VersionMapper(doc.length);
    mapper.mark(1, doc.length);
    const at = doc.indexOf(";") + 1;
    const text = "\n    line(to = [10, 0]);";
    const tr = sketchTransaction(ed.state, mapper, 1, [{ from: at, to: at, text }], [0, text.length])!;
    ed.dispatch(ed.state.update(tr));
    expect(ed.state.doc.toString()).toBe("sketch {\n    start([0, 0]);\n    line(to = [10, 0]);\n}\n");
    expect(ed.state.selection.main.head).toBe(at + text.length);
    expect(undoDepth(ed.state)).toBe(1);
    // the next action is a step of its own
    const at2 = ed.state.selection.main.head;
    const t2 = "\n    close();";
    ed.dispatch(ed.state.update(sketchTransaction(ed.state, mapper, 1, [{ from: at, to: at, text: t2 }], [0, t2.length])!));
    expect(undoDepth(ed.state)).toBe(2);
    expect(at2).toBeGreaterThan(at);
    undo(ed);
    undo(ed);
    expect(ed.state.doc.toString()).toBe(doc);
  });
  it("edits of an unknown version: nothing", () => {
    const ed = editor(doc);
    const mapper = new VersionMapper(doc.length);
    expect(sketchTransaction(ed.state, mapper, 7, [{ from: 0, to: 0, text: "x" }], null)).toBeNull();
  });
});

describe("Shortcuts of the drawing mode (UI overhaul)", () => {
  it("letters choose the tools, Shift+A the arc around the centre", () => {
    const cases: Array<[string, boolean, unknown]> = [
      ["v", false, { tool: "select" }],
      ["L", false, { tool: "line" }],
      ["a", false, { tool: "arc_through" }],
      ["A", true, { tool: "arc_center" }],
      ["t", false, { tool: "arc_tangent" }],
      ["b", false, { tool: "bezier" }],
      ["r", false, { tool: "rect" }],
      ["c", false, { tool: "circle" }],
      ["f", false, { tool: "corner" }],
      ["Enter", false, { command: "close" }],
      ["n", false, { command: "align" }],
      ["x", false, null],
      ["R", true, null],
      ["Escape", false, null],
    ];
    for (const [k, shift, want] of cases) expect(sketchKeyAction(k, shift), `${shift ? "⇧" : ""}${k}`).toEqual(want);
  });
  it("every tool of the bar has a shortcut, none twice", () => {
    const keys = Object.values(SKETCH_TOOL_KEYS);
    expect(keys.length).toBe(9);
    expect(new Set(keys).size).toBe(keys.length);
  });
});
