import { describe, expect, it } from "vitest";
import type { Module, Param } from "../engine/protocol";
import { fixture } from "./__fixtures__/load";
import { contextModule, controlOf, dropOverride, paramRows, resetAll, resetOverride, resetValue, setOverride, sliderRange } from "./params";

const f = fixture("housing-module");
const sig = contextModule(f.scene)!;
const used = f.scene.context.params;

const param = (name: string, ty: Param["ty"], def: Param["default_value"] | null): Param => ({
  name,
  range: { from: 0, to: 0, line: 1, col: 1, end_line: 1, end_col: 1 },
  default_text: def === null ? null : JSON.stringify(def),
  default_range: null,
  default_value: def,
  ty,
});

describe("Parameter bar: rows from the module signature", () => {
  it("context module and rows", () => {
    expect(sig.name).toBe("housing");
    const rows = paramRows(sig, used, undefined);
    expect(rows.map((r) => [r.name, r.control, r.value, r.changed])).toEqual([
      ["w", "number", 40, false],
      ["d", "number", 30, false],
      ["h", "number", 20, false],
      ["t", "number", 3, false],
    ]);
    expect(rows[0].min).toBe(0);
    expect(rows[0].max).toBe(80);
  });

  it("temporary values per module, changed ones marked", () => {
    let ov = setOverride({}, "housing", "w", 60);
    ov = setOverride(ov, "lid", "x", 1);
    const rows = paramRows(sig, used, ov.housing);
    expect(rows[0]).toMatchObject({ value: 60, changed: true, overridden: true });
    expect(rows[1]).toMatchObject({ value: 30, changed: false });
    // Same value as the default: not marked.
    const same = paramRows(sig, used, setOverride({}, "housing", "w", 40).housing);
    expect(same[0].changed).toBe(false);
    expect(ov.lid).toEqual({ x: 1 });
  });

  it("reset to the default, without a default to 0 / false / [0, 0, 0]", () => {
    const ov = setOverride({}, "housing", "w", 60);
    const wp = sig.params[0];
    expect(resetOverride(ov, "housing", wp, "number").housing).toEqual({});
    const n = param("n", "unknown", null);
    const b = param("b", "unknown", null);
    const v = param("v", "unknown", null);
    expect(resetValue(n, controlOf(n, undefined))).toBe(0);
    expect(resetValue(b, controlOf(b, true))).toBe(false);
    expect(resetValue(v, controlOf(v, [1, 2, 3]))).toEqual([0, 0, 0]);
    expect(resetOverride({}, "m", b, "bool").m).toEqual({ b: false });
    expect(resetOverride({}, "m", v, "vector").m).toEqual({ v: [0, 0, 0] });
  });

  it("reset all", () => {
    const m: Module = { ...sig, params: [...sig.params, param("n", "unknown", null)] };
    let ov = setOverride({}, "housing", "w", 60);
    ov = setOverride(ov, "housing", "n", 5);
    const r = resetAll(ov, m, used);
    expect(r.housing).toEqual({ n: 0 });
    const rows = paramRows(m, used, r.housing);
    expect(rows.every((x) => !x.changed)).toBe(true);
  });

  it("after “Use as default” the temporary value is dropped", () => {
    const ov = setOverride(setOverride({}, "housing", "w", 60), "housing", "d", 50);
    expect(dropOverride(ov, "housing", "d").housing).toEqual({ w: 60 });
  });

  it("controls by type", () => {
    expect(controlOf(param("a", "bool", true), true)).toBe("bool");
    expect(controlOf(param("a", "vector", [1, 2, 3]), undefined)).toBe("vector");
    expect(controlOf(param("a", "string", "x"), undefined)).toBe("other");
    expect(controlOf(param("a", "unknown", null), "x")).toBe("other");
    expect(controlOf(param("a", "unknown", null), undefined)).toBe("number");
  });

  it("slider range", () => {
    expect(sliderRange(3)).toEqual({ min: 0, max: 10, step: 1 });
    expect(sliderRange(40)).toEqual({ min: 0, max: 80, step: 1 });
    expect(sliderRange(0.4)).toEqual({ min: 0, max: 10, step: 0.1 });
    expect(sliderRange(-5)).toEqual({ min: -10, max: 10, step: 1 });
  });
});
