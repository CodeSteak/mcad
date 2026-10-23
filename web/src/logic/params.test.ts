import { describe, expect, it } from "vitest";
import type { Module, Param } from "../engine/protocol";
import { fixture } from "./__fixtures__/load";
import { contextModule, controlOf, dropOverride, paramRows, resetAll, resetOverride, resetValue, setOverride, sliderRange } from "./params";

const f = fixture("gehaeuse-modul");
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

describe("Parameterleiste: Zeilen aus der Modulsignatur", () => {
  it("Kontextmodul und Zeilen", () => {
    expect(sig.name).toBe("gehaeuse");
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

  it("temporäre Werte pro Modul, geändert markiert", () => {
    let ov = setOverride({}, "gehaeuse", "w", 60);
    ov = setOverride(ov, "deckel", "x", 1);
    const rows = paramRows(sig, used, ov.gehaeuse);
    expect(rows[0]).toMatchObject({ value: 60, changed: true, overridden: true });
    expect(rows[1]).toMatchObject({ value: 30, changed: false });
    // Same value as the default: not marked.
    const same = paramRows(sig, used, setOverride({}, "gehaeuse", "w", 40).gehaeuse);
    expect(same[0].changed).toBe(false);
    expect(ov.deckel).toEqual({ x: 1 });
  });

  it("Reset auf den Standardwert, ohne Standardwert auf 0 / false / [0, 0, 0]", () => {
    const ov = setOverride({}, "gehaeuse", "w", 60);
    const wp = sig.params[0];
    expect(resetOverride(ov, "gehaeuse", wp, "number").gehaeuse).toEqual({});
    const n = param("n", "unknown", null);
    const b = param("b", "unknown", null);
    const v = param("v", "unknown", null);
    expect(resetValue(n, controlOf(n, undefined))).toBe(0);
    expect(resetValue(b, controlOf(b, true))).toBe(false);
    expect(resetValue(v, controlOf(v, [1, 2, 3]))).toEqual([0, 0, 0]);
    expect(resetOverride({}, "m", b, "bool").m).toEqual({ b: false });
    expect(resetOverride({}, "m", v, "vector").m).toEqual({ v: [0, 0, 0] });
  });

  it("alle zurücksetzen", () => {
    const m: Module = { ...sig, params: [...sig.params, param("n", "unknown", null)] };
    let ov = setOverride({}, "gehaeuse", "w", 60);
    ov = setOverride(ov, "gehaeuse", "n", 5);
    const r = resetAll(ov, m, used);
    expect(r.gehaeuse).toEqual({ n: 0 });
    const rows = paramRows(m, used, r.gehaeuse);
    expect(rows.every((x) => !x.changed)).toBe(true);
  });

  it("nach „Als Standard übernehmen“ fällt der temporäre Wert weg", () => {
    const ov = setOverride(setOverride({}, "gehaeuse", "w", 60), "gehaeuse", "d", 50);
    expect(dropOverride(ov, "gehaeuse", "d").gehaeuse).toEqual({ w: 60 });
  });

  it("Bedienelemente nach Typ", () => {
    expect(controlOf(param("a", "bool", true), true)).toBe("bool");
    expect(controlOf(param("a", "vector", [1, 2, 3]), undefined)).toBe("vector");
    expect(controlOf(param("a", "string", "x"), undefined)).toBe("other");
    expect(controlOf(param("a", "unknown", null), "x")).toBe("other");
    expect(controlOf(param("a", "unknown", null), undefined)).toBe("number");
  });

  it("Schieberegler-Bereich", () => {
    expect(sliderRange(3)).toEqual({ min: 0, max: 10, step: 1 });
    expect(sliderRange(40)).toEqual({ min: 0, max: 80, step: 1 });
    expect(sliderRange(0.4)).toEqual({ min: 0, max: 10, step: 0.1 });
    expect(sliderRange(-5)).toEqual({ min: -10, max: 10, step: 1 });
  });
});
