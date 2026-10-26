import { describe, expect, it } from "vitest";
import { axisSection, cliCamera, cliCommand, cliSection, effectiveNormal, freeSection, keeps, offset, withOffset } from "./section";

describe("section convention (as mcad render --section)", () => {
  it("x=10 removes x > 10", () => {
    const s = axisSection("x", 10);
    expect(keeps(s, [9, 0, 0])).toBe(true);
    expect(keeps(s, [10, 5, 5])).toBe(true);
    expect(keeps(s, [11, 0, 0])).toBe(false);
    expect(cliSection(s)).toBe("--section x=10");
  });
  it("flip keeps the other half", () => {
    const s = axisSection("z", 4.5, true);
    expect(effectiveNormal(s)).toEqual([-0, -0, -1]);
    expect(keeps(s, [0, 0, 5])).toBe(true);
    expect(keeps(s, [0, 0, 4])).toBe(false);
    expect(cliSection(s)).toBe("--section z=4.5 --section-flip");
  });
  it("free plane gives --section-plane with the effective normal", () => {
    const s = freeSection([1, 2, 3], [0, 3, 4]);
    expect(cliSection(s)).toBe("--section-plane 1,2,3,0,0.6,0.8");
    expect(cliSection({ ...s, flip: true })).toBe("--section-plane 1,2,3,0,-0.6,-0.8");
    // a free plane that happens to be axis-aligned is written as an axis section
    expect(cliSection(freeSection([0, 7, 0], [0, -1, 0]))).toBe("--section y=7 --section-flip");
  });
  it("moving along the normal", () => {
    const s = withOffset(axisSection("y", 0), 12.25, "g.back");
    expect(s.point).toEqual([0, 12.25, 0]);
    expect(s.snap).toBe("g.back");
    const f = withOffset(freeSection([1, 1, 1], [0, 0, 2]), 5);
    expect(f.point).toEqual([1, 1, 5]);
    expect(offset(f)).toBe(5);
  });
  it("camera and full command", () => {
    expect(cliCamera([0, -1, 0])).toEqual({ azimuth: 0, elevation: 0 });
    expect(cliCamera([1, 0, 0])).toEqual({ azimuth: 90, elevation: 0 });
    expect(cliCamera([0, 0, 1]).elevation).toBe(90);
    const c = cliCommand(axisSection("z", 10), "/tmp/my part.scad", "housing", [1, -1, 0]);
    expect(c).toBe("mcad render '/tmp/my part.scad' -o section.png --module housing --section z=10 --azimuth 45 --elevation 0");
  });
});
