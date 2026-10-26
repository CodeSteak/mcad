import { describe, expect, it } from "vitest";
import type { Measurement } from "../engine/protocol";
import { MEASURE_KINDS, accuracyText, fitKind, kindAvailable, listText, measureCli, measureComment, sectionText, shellQuote } from "./measure";

function m(over: Partial<Measurement> = {}): Measurement {
  return {
    kind: "distance",
    title: "Abstand",
    value: 8,
    unit: "mm",
    label: "8 mm",
    values: [],
    lines: [],
    guides: [],
    points: [],
    at: [0, 0, 0],
    details: [],
    picks: [],
    exact: true,
    text: "Abstand Fläche · Ebene – Fläche · Ebene: 8 mm\n  parallele Ebenen: Normalabstand\n  von [10, 5, 8] bis [10, 5, 0] · Δ [0, 0, -8]",
    cli: ["--distance", "face@[10,5,8]", "face@[10,5,0]"],
    ...over,
  };
}

describe("measure mode (Plan 18)", () => {
  it("offers the kinds by mode", () => {
    expect(MEASURE_KINDS.slice(0, 3).map((k) => k.label)).toEqual(["Abstand", "Radius", "Winkel"]);
    expect(kindAvailable("wall", false)).toBe(false);
    expect(kindAvailable("wall", true)).toBe(true);
    expect(kindAvailable("body", true)).toBe(false);
    expect(kindAvailable("radius", true)).toBe(true);
    expect(fitKind("wall", false)).toBe("distance");
    expect(fitKind("body", false)).toBe("body");
    // shortcuts are unique
    expect(new Set(MEASURE_KINDS.map((k) => k.key)).size).toBe(MEASURE_KINDS.length);
  });

  it("quotes shell arguments", () => {
    expect(shellQuote("face@[10,5,8]")).toBe("'face@[10,5,8]'");
    expect(shellQuote("--section")).toBe("--section");
    expect(shellQuote("z=10")).toBe("z=10");
    expect(shellQuote("my part.scad")).toBe("'my part.scad'");
    expect(shellQuote("it's")).toBe("'it'\\''s'");
  });

  it("copies as mcad measure, comment and text", () => {
    expect(measureCli(m(), "main.scad")).toBe("mcad measure main.scad --distance 'face@[10,5,8]' 'face@[10,5,0]'");
    expect(measureCli(m({ cli: ["--module", "housing", "--section", "z=10", "--wall", "[0,15,10]"] }), "/home/x/a b.scad")).toBe(
      "mcad measure '/home/x/a b.scad' --module housing --section z=10 --wall '[0,15,10]'",
    );
    const c = measureComment(m());
    expect(c.split("\n")).toEqual([
      "// Messung: Abstand Fläche · Ebene – Fläche · Ebene: 8 mm",
      "//   parallele Ebenen: Normalabstand",
      "//   von [10, 5, 8] bis [10, 5, 0] · Δ [0, 0, -8]",
    ]);
    expect(listText([m(), m({ text: "Radius: 2 mm" })])).toBe(`${m().text}\n\nRadius: 2 mm`);
  });

  it("describes accuracy and the section plane", () => {
    expect(accuracyText(m())).toBe("exakt");
    expect(accuracyText(m({ exact: false, tol: 0.02 }))).toBe("± 0.02 mm");
    expect(accuracyText(m({ exact: false, tol: 2e-7 }))).toBe("± 2.0e-7 mm");
    expect(sectionText(m())).toBeNull();
    expect(sectionText(m({ section: { point: [0, 0, 10], normal: [0, 0, 1] } }))).toBe("z = 10");
    expect(sectionText(m({ section: { point: [1, 1, 1], normal: [0.6, 0, 0.8] } }))).toBe("freie Ebene");
  });
});
