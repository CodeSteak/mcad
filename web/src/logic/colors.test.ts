import { describe, expect, it } from "vitest";
import type { Scene } from "../engine/protocol";
import { colorRows, displayHex, grams, rgbaHex, sharedSlots, tagColors, vertexColors } from "./colors";
import { PALETTE } from "./palette";

const rgba = (hex: string): [number, number, number, number] => [0, 2, 4].map((i) => parseInt(hex.slice(1 + i, 3 + i), 16) / 255).concat(1) as [number, number, number, number];

describe("colours", () => {
  it("palette colours get theme variants, others stay", () => {
    const black = PALETTE.find((p) => p.name === "black")!;
    expect(displayHex(rgba(black.hex), "dark")).toBe(black.dark);
    expect(displayHex(rgba(black.hex), "light")).toBe(black.light);
    expect(black.dark).not.toBe(black.hex);
    expect(displayHex(rgba("#123456"), "dark")).toBe("#123456");
    expect(rgbaHex([1, 0.5, 0, 1])).toBe("#ff8000");
    expect(grams(1000)).toBeCloseTo(1.24, 9);
  });

  it("faces take the colour of their tag's node", () => {
    const red = PALETTE.find((p) => p.name === "red")!;
    const scene = {
      nodes: [{ color: null }, { color: rgba(red.hex) }],
      tag_nodes: [
        { tag: 1, nodes: [0] },
        { tag: 2, nodes: [1] },
      ],
    } as unknown as Scene;
    const m = tagColors(scene, "light");
    expect(m.get(2)).toBe(red.light);
    expect(m.has(1)).toBe(false);
    // two triangles with own vertices: the second one red
    const mesh = { positions: new Float32Array(18), indices: new Uint32Array([0, 1, 2, 3, 4, 5]), tri_tags: new Uint32Array([1, 2]) };
    const c = vertexColors(mesh, (t) => m.get(t), "#ffffff");
    expect(c[0]).toBeCloseTo(1, 6);
    expect(c[9]).toBeCloseTo(((0xc7 / 255 + 0.055) / 1.055) ** 2.4, 6);
  });

  it("colour rows with volumes from the regions of the same version", () => {
    const scene = {
      version: 3,
      colors: [
        { key: "#f4f3f0", rgba: rgba("#f4f3f0"), label: "white", source: "palette", filament: 1, explicit: false, ranges: [{ line: 2 }, { line: 2 }], nodes: [1] },
        { key: "#c74240", rgba: rgba("#c74240"), label: "red", source: "palette", filament: 1, explicit: true, ranges: [{ line: 5 }], nodes: [4] },
      ],
    } as unknown as Scene;
    const regions = {
      version: 3,
      meshes: false,
      items: [
        { node: 0, part: "a", total: 30, warning: null, regions: [{ color: "#f4f3f0", filament: 1, volume: 10, mesh: null }, { color: "#c74240", filament: 1, volume: 20, mesh: null }] },
        { node: 1, part: "b", total: 5, warning: null, regions: [{ color: "#c74240", filament: 1, volume: 5, mesh: null }] },
      ],
    };
    const rows = colorRows(scene, regions, "dark", "#888888");
    expect(rows.map((r) => [r.label, r.volume, r.lines])).toEqual([
      ["white", 10, [2]],
      ["red", 25, [5]],
    ]);
    expect(sharedSlots(rows)).toEqual([1]);
    expect(colorRows(scene, { ...regions, version: 2 }, "dark", "#888888")[0].volume).toBeNull();
  });
});
