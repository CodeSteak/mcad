import { describe, expect, it } from "vitest";
import { DEFAULT_STUDIO, aoRadiusFor, lightDirection, sanitizeStudio, studioCliArgs, studioCliCommand } from "./studio";

const close = (a: number[], b: number[]) => a.forEach((x, i) => expect(x).toBeCloseTo(b[i], 6));

describe("studio settings", () => {
  it("defaults match mcad render --style studio", () => {
    expect(DEFAULT_STUDIO).toMatchObject({ on: false, lightAzimuth: -40, lightElevation: 50, lightIntensity: 1, ambient: 0.4, shadows: true, shadowSoftness: 0.35, ao: true, aoRadius: null, aoStrength: 1, ground: true, edges: true });
    expect(studioCliArgs({ ...DEFAULT_STUDIO, on: true })).toEqual(["--style", "studio"]);
  });
  it("sanitizes stored values", () => {
    expect(sanitizeStudio(null)).toEqual(DEFAULT_STUDIO);
    expect(sanitizeStudio("x")).toEqual(DEFAULT_STUDIO);
    const s = sanitizeStudio({ on: true, lightElevation: 120, ambient: 0.2, aoRadius: -1, groundColor: "#ABCDEF", background: ["#000000", "nope"], shadows: "yes" });
    expect(s.on).toBe(true);
    expect(s.lightElevation).toBe(50);
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
    const s = { ...DEFAULT_STUDIO, on: true, lightAzimuth: 30, shadowSoftness: 0.8, ao: false, aoStrength: 2, groundColor: "#808080", background: ["#ffffff", "#ffffff"] as [string, string] };
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
  it("full command with camera and theme", () => {
    const c = studioCliCommand({ ...DEFAULT_STUDIO, on: true }, "my box.mcad", "lid", [0, -1, 0], "dark");
    expect(c).toBe("mcad render 'my box.mcad' -o studio.png --module lid --perspective --azimuth 0 --elevation 0 --theme dark --style studio");
  });
});
