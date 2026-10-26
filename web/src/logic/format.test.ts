import { describe, expect, it } from "vitest";
import { fmtNum, roundTo, stepDecimals, unwrapDeg, valueText } from "./format";

describe("Rounding and number format when writing back", () => {
  it("rounds to the step", () => {
    expect(roundTo(12.4, 1)).toBe(12);
    expect(roundTo(12.6, 1)).toBe(13);
    expect(roundTo(23.3, 0.5)).toBe(23.5);
    expect(roundTo(52, 15)).toBe(45);
    expect(roundTo(53, 15)).toBe(60);
    expect(roundTo(0.1 + 0.2, 0.1)).toBe(0.3);
    expect(roundTo(1.23456, 0)).toBe(1.23456);
  });

  it("shortest form without trailing zeros, never -0", () => {
    expect(fmtNum(12, 1)).toBe("12");
    expect(fmtNum(23.3, 0.5)).toBe("23.5");
    expect(fmtNum(2.46, 0.1)).toBe("2.5");
    expect(fmtNum(-0.2, 1)).toBe("0");
    expect(fmtNum(1 / 3)).toBe("0.333333");
    expect(fmtNum(40)).toBe("40");
    expect(fmtNum(-7.5)).toBe("-7.5");
    expect(fmtNum(Number.NaN)).toBe("0");
    expect(fmtNum(0.25, 0.25)).toBe("0.25");
  });

  it("decimals of a step", () => {
    expect(stepDecimals(1)).toBe(0);
    expect(stepDecimals(0.5)).toBe(1);
    expect(stepDecimals(0.05)).toBe(2);
    expect(stepDecimals(15)).toBe(0);
  });

  it("values as source text", () => {
    expect(valueText(80)).toBe("80");
    expect(valueText(true)).toBe("true");
    expect(valueText([1, 2.5, 0])).toBe("[1, 2.5, 0]");
    expect(valueText("a")).toBe('"a"');
  });

  it("angles stay continuous across ±180°", () => {
    expect(unwrapDeg(-170, 175)).toBe(190);
    expect(unwrapDeg(10, 365)).toBe(370);
    expect(unwrapDeg(45, 30)).toBe(45);
  });
});
