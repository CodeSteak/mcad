import { describe, expect, it } from "vitest";
import {
  DEFAULT_STUDIO,
  KEY_LIGHT,
  aoRadiusFor,
  colorMatches,
  envRadiance,
  environmentPixels,
  environmentSpec,
  lightDirection,
  materialFor,
  neutralTone,
  resolvePbr,
  sanitizeStudio,
  studioCliArgs,
  studioCliCommand,
  upIrradiance,
  withMaterial,
} from "./studio";

const close = (a: number[], b: number[]) => a.forEach((x, i) => expect(x).toBeCloseTo(b[i], 6));

describe("studio settings", () => {
  it("defaults match mcad render --style studio", () => {
    expect(DEFAULT_STUDIO).toMatchObject({ on: false, lightIntensity: 1, ambient: 0.4, environment: "studio", shadows: true, shadowSoftness: 0.35, ao: true, aoRadius: null, aoStrength: 1, ground: true, edges: true, materials: [] });
    expect(DEFAULT_STUDIO.lights).toEqual([{ name: "key", azimuth: -40, elevation: 50, intensity: 1, color: "#ffffff" }]);
    expect(studioCliArgs({ ...DEFAULT_STUDIO, on: true })).toEqual(["--style", "studio"]);
  });
  it("sanitizes stored values", () => {
    expect(sanitizeStudio(null)).toEqual(DEFAULT_STUDIO);
    expect(sanitizeStudio("x")).toEqual(DEFAULT_STUDIO);
    const s = sanitizeStudio({ on: true, lightElevation: 120, ambient: 0.2, aoRadius: -1, groundColor: "#ABCDEF", background: ["#000000", "nope"], shadows: "yes" });
    expect(s.on).toBe(true);
    expect(s.lights[0].elevation).toBe(50);
    // settings of the first version: one key light
    expect(sanitizeStudio({ lightAzimuth: 60, lightElevation: 25 }).lights).toEqual([{ ...KEY_LIGHT, azimuth: 60, elevation: 25 }]);
    const l = sanitizeStudio({ lights: [{ name: "key", azimuth: 10, elevation: 20, intensity: 9, color: "red" }, { name: "key", azimuth: 1, elevation: 2 }, 5] }).lights;
    expect(l).toEqual([
      { name: "key", azimuth: 10, elevation: 20, intensity: 1, color: "#ffffff" },
      { name: "light2", azimuth: 1, elevation: 2, intensity: 1, color: "#ffffff" },
    ]);
    expect(sanitizeStudio({ environment: "beach" }).environment).toBe("studio");
    expect(sanitizeStudio({ materials: [{ target: { color: "red" }, preset: "chrome", roughness: 3 }, { target: {} }] }).materials).toEqual([
      { target: { color: "red" }, preset: "chrome", metalness: null, roughness: 1, clearcoat: null },
    ]);
    expect(s.ambient).toBe(0.2);
    expect(s.aoRadius).toBeNull();
    expect(s.groundColor).toBe("#abcdef");
    expect(s.background).toBeNull();
    expect(s.shadows).toBe(true);
    expect(sanitizeStudio({ edges: false }).edges).toBe(false);
    expect(sanitizeStudio({ background: ["#ffffff", "#000000"], aoRadius: 2.5 })).toMatchObject({ background: ["#ffffff", "#000000"], aoRadius: 2.5 });
  });
  it("AO radius: explicit or 5 % of the diagonal", () => {
    expect(aoRadiusFor(DEFAULT_STUDIO, 200)).toBeCloseTo(10);
    expect(aoRadiusFor({ ...DEFAULT_STUDIO, aoRadius: 3 }, 200)).toBe(3);
  });
});

describe("light direction relative to the camera (as cad_render::studio)", () => {
  it("azimuth 0 comes from the camera, 90 from its right", () => {
    // camera in front (from −y)
    close(lightDirection([0, -1, 0], [0, 0, 1], 0, 0), [0, -1, 0]);
    close(lightDirection([0, -1, 0], [0, 0, 1], 90, 0), [1, 0, 0]);
    // camera on the right (from +x): azimuth 0 is +x
    close(lightDirection([1, 0, 0], [0, 0, 1], 0, 0), [1, 0, 0]);
    // elevation above the xy plane
    close(lightDirection([0, -1, 0], [0, 0, 1], 0, 90).map((x) => Math.round(x * 1e3) / 1e3), [0, -0.002, 1]);
  });
  it("from straight above the screen's up decides the azimuth", () => {
    const d = lightDirection([0, 0, 1], [0, 1, 0], 0, 45);
    close(d, [0, -Math.SQRT1_2, Math.SQRT1_2]);
  });
});

describe("CLI call", () => {
  it("lists changed settings only", () => {
    const s = { ...DEFAULT_STUDIO, on: true, lights: [{ ...KEY_LIGHT, azimuth: 30 }], shadowSoftness: 0.8, ao: false, aoStrength: 2, groundColor: "#808080", background: ["#ffffff", "#ffffff"] as [string, string] };
    expect(studioCliArgs(s)).toEqual(["--style", "studio", "--light", "30,50", "--shadow-softness", "0.8", "--ao", "off", "--ground-color", "808080", "--background", "ffffff"]);
    expect(studioCliArgs({ ...DEFAULT_STUDIO, shadows: false, ground: false, groundColor: "#808080", background: ["#ffffff", "#000000"] })).toEqual([
      "--style",
      "studio",
      "--shadows",
      "off",
      "--ground",
      "off",
      "--background",
      "ffffff,000000",
    ]);
  });
  it("edge lines off", () => {
    expect(studioCliArgs({ ...DEFAULT_STUDIO, edges: false })).toEqual(["--style", "studio", "--edges", "off"]);
  });
  it("environment, materials and the key light", () => {
    const s = { ...DEFAULT_STUDIO, environment: "overcast" as const, lights: [{ ...KEY_LIGHT, azimuth: 30 }], materials: [{ target: { color: "#aabbcc" }, preset: "chrome", metalness: null, roughness: 0.2, clearcoat: null }, { target: { part: "lid" }, preset: null, metalness: null, roughness: null, clearcoat: null }] };
    expect(studioCliArgs(s)).toEqual(["--style", "studio", "--light", "30,50", "--environment", "overcast", "--material", "aabbcc=chrome,roughness=0.2"]);
  });
  it("full command with camera and theme", () => {
    const c = studioCliCommand({ ...DEFAULT_STUDIO, on: true }, "my box.mcad", "lid", [0, -1, 0], "dark");
    expect(c).toBe("mcad render 'my box.mcad' -o studio.png --module lid --perspective --azimuth 0 --elevation 0 --theme dark --style studio");
  });
});

describe("materials", () => {
  const red = { key: "#c74240", label: "red", isDefault: false };
  const none = { key: "#c9ced6", label: "default", isDefault: true };
  it("presets and single values", () => {
    expect(resolvePbr({ preset: "chrome", metalness: null, roughness: null, clearcoat: null })).toEqual({ metalness: 1, roughness: 0.08, clearcoat: 0 });
    expect(resolvePbr({ preset: null, metalness: 1, roughness: null, clearcoat: null })).toEqual({ metalness: 1, roughness: 0.5, clearcoat: 0 });
    expect(resolvePbr({ preset: "nope", metalness: null, roughness: 2, clearcoat: null })).toEqual({ metalness: 0, roughness: 1, clearcoat: 0 });
  });
  it("colour entries by palette name or hex; default = uncoloured; parts win", () => {
    expect(colorMatches("RED", red)).toBe(true);
    expect(colorMatches("#c74240", red)).toBe(true);
    expect(colorMatches("default", none)).toBe(true);
    expect(colorMatches("default", red)).toBe(false);
    let ms = withMaterial([], { color: "red" }, { preset: "chrome", metalness: null, roughness: null, clearcoat: null });
    ms = withMaterial(ms, { part: "lid" }, { preset: "rubber", metalness: null, roughness: null, clearcoat: null });
    expect(materialFor(ms, red, null).metalness).toBe(1);
    expect(materialFor(ms, red, "lid").roughness).toBe(0.9);
    expect(materialFor(ms, none, "body")).toEqual({ metalness: 0, roughness: 0.5, clearcoat: 0 });
    ms = withMaterial(ms, { color: "Red" }, null);
    expect(ms.length).toBe(1);
  });
});

describe("environment (the same numbers as cad_render::studio, tests/studio.rs environment_numbers)", () => {
  const r4 = (x: number) => Math.round(x * 1e4) / 1e4;
  it("radiance, irradiance and tone mapping", () => {
    const spec = environmentSpec("studio", [0.8, 0.8, 0.8]);
    expect([r4(envRadiance(spec, [0, 0, 1])[0]), r4(envRadiance(spec, [-1, 0, 0])[0]), r4(envRadiance(spec, [0, 0, -1])[0])]).toEqual([1.3248, 0.3607, 0.5692]);
    expect(r4(upIrradiance(spec))).toBe(1.1669);
    expect(neutralTone([0.5, 1, 2]).map(r4)).toEqual([0.3211, 0.5341, 0.96]);
  });
  it("equirectangular pixels follow three.js equirectUv (y of the map = z of the scene is not assumed)", () => {
    const spec = environmentSpec("overcast", [0.5, 0.5, 0.5]);
    const w = 16;
    const px = environmentPixels(spec, w);
    expect(px.length).toBe(w * (w / 2) * 4);
    // the row nearest v = 1 looks along +y of the map's direction vector
    const top = envRadiance(spec, [0, 1, 0])[0];
    expect(px[((w / 2 - 1) * w + 3) * 4]).toBeCloseTo(top, 1);
  });
});
