import { describe, expect, it } from "vitest";
import type { RenderBlock, SrcRange } from "../engine/protocol";
import { blockChanges, blockKey, settingsFromBlock } from "./renderBlock";
import { DEFAULT_STUDIO, KEY_LIGHT, type StudioSettings } from "./studio";

const R: SrcRange = { from: 0, to: 1, line: 3, col: 1, end_line: 3, end_col: 2 };
const empty: RenderBlock = {
  range: R,
  style: null,
  edges: null,
  environment: null,
  ambient: null,
  light_intensity: null,
  shadows: null,
  shadow_softness: null,
  ao: null,
  ao_radius: null,
  ao_strength: null,
  ground: null,
  ground_color: null,
  background: null,
  lights: [],
  materials: [],
};

describe("settings from a render block", () => {
  it("unset values are the defaults; on stays", () => {
    expect(settingsFromBlock(empty, true)).toEqual({ ...DEFAULT_STUDIO, on: true });
  });
  it("values, lights, materials, one background colour", () => {
    const b: RenderBlock = {
      ...empty,
      edges: false,
      ambient: 0.3,
      environment: "overcast",
      ground_color: "#808080",
      background: ["#FFFFFF"],
      lights: [{ name: "rim", azimuth: 150, elevation: 25, intensity: 0.5, color: "#ffe0c0", range: R }],
      materials: [{ target: { color: "red" }, preset: "chrome", metalness: null, roughness: 0.2, clearcoat: null, range: R }],
    };
    const s = settingsFromBlock(b, false);
    expect(s).toMatchObject({ edges: false, ambient: 0.3, environment: "overcast", groundColor: "#808080", background: ["#ffffff", "#ffffff"] });
    expect(s.lights).toEqual([{ name: "rim", azimuth: 150, elevation: 25, intensity: 0.5, color: "#ffe0c0" }]);
    expect(s.materials).toEqual([{ target: { color: "red" }, preset: "chrome", metalness: null, roughness: 0.2, clearcoat: null }]);
    // the key ignores the place
    expect(blockKey({ ...b, range: { ...R, line: 9 } })).toBe(blockKey(b));
    expect(blockKey({ ...b, ambient: 0.4 })).not.toBe(blockKey(b));
    expect(blockKey(null)).toBeNull();
  });
});

describe("changes written into the block", () => {
  const s0: StudioSettings = { ...DEFAULT_STUDIO, on: true };
  it("a new block gets everything that differs from the defaults", () => {
    const next = { ...s0, edges: false, ambient: 0.25, groundColor: "#808080" };
    expect(blockChanges(s0, next, false)).toEqual([
      { kind: "set", key: "edges", value: false },
      { kind: "set", key: "ambient", value: 0.25 },
      { kind: "set", key: "ground_color", value: "#808080" },
    ]);
    // the default key light needs no entry; a moved one is written with its direction
    expect(blockChanges(s0, { ...s0, lights: [{ ...KEY_LIGHT, azimuth: 60 }] }, false)).toEqual([
      { kind: "light", name: "key", key: "azimuth", value: 60 },
      { kind: "light", name: "key", key: "elevation", value: 50 },
    ]);
  });
  it("with a block only what changed", () => {
    const prev = { ...s0, edges: false, ambient: 0.25 };
    expect(blockChanges(prev, { ...prev, ambient: 0.30000000000000004 }, true)).toEqual([{ kind: "set", key: "ambient", value: 0.3 }]);
    expect(blockChanges(prev, { ...prev, aoRadius: 2.5 }, true)).toEqual([{ kind: "set", key: "ao_radius", value: 2.5 }]);
    expect(blockChanges({ ...prev, aoRadius: 2.5 }, prev, true)).toEqual([{ kind: "set", key: "ao_radius", value: null }]);
    expect(blockChanges(prev, { ...prev, background: ["#ffffff", "#000000"] }, true)).toEqual([{ kind: "set", key: "background", value: ["#ffffff", "#000000"] }]);
    expect(blockChanges(prev, { ...prev, background: ["#ffffff", "#ffffff"] }, true)).toEqual([{ kind: "set", key: "background", value: "#ffffff" }]);
    expect(blockChanges(prev, prev, true)).toEqual([]);
  });
  it("lights: changed values, new and removed lights", () => {
    const rim = { name: "rim", azimuth: 150, elevation: 25, intensity: 0.5, color: "#ffffff" };
    const prev = { ...s0, lights: [KEY_LIGHT, rim] };
    expect(blockChanges(prev, { ...prev, lights: [{ ...KEY_LIGHT, color: "#ffe0c0" }, rim] }, true)).toEqual([{ kind: "light", name: "key", key: "color", value: "#ffe0c0" }]);
    expect(blockChanges(prev, { ...prev, lights: [KEY_LIGHT] }, true)).toEqual([{ kind: "remove_light", name: "rim" }]);
    // from the (maybe implicit) default key light: all lights are written
    expect(blockChanges({ ...s0 }, prev, true)).toEqual([
      { kind: "light", name: "key", key: "azimuth", value: -40 },
      { kind: "light", name: "key", key: "elevation", value: 50 },
      { kind: "light", name: "rim", key: "azimuth", value: 150 },
      { kind: "light", name: "rim", key: "elevation", value: 25 },
      { kind: "light", name: "rim", key: "intensity", value: 0.5 },
    ]);
  });
  it("materials: new entry, changed value, removed entry", () => {
    const m = { target: { color: "red" }, preset: "chrome", metalness: null, roughness: null, clearcoat: null };
    expect(blockChanges(s0, { ...s0, materials: [m] }, true)).toEqual([{ kind: "material", target: { color: "red" }, key: "preset", value: "chrome" }]);
    const prev = { ...s0, materials: [m] };
    expect(blockChanges(prev, { ...s0, materials: [{ ...m, roughness: 0.3 }] }, true)).toEqual([{ kind: "material", target: { color: "red" }, key: "roughness", value: 0.3 }]);
    expect(blockChanges(prev, s0, true)).toEqual([{ kind: "remove_material", target: { color: "red" } }]);
    const lid = { target: { part: "lid" }, preset: null, metalness: 1, roughness: null, clearcoat: null };
    expect(blockChanges(s0, { ...s0, materials: [lid] }, true)).toEqual([{ kind: "material", target: { part: "lid" }, key: "metalness", value: 1 }]);
  });
});
