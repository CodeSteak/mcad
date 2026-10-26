import { describe, expect, it } from "vitest";
import { diffLines, lineChanges } from "./textDiff";

function apply(s: string, cs: Array<{ from: number; to: number; insert: string }>): string {
  let out = "";
  let at = 0;
  for (const c of cs) {
    expect(c.from).toBeGreaterThanOrEqual(at);
    out += s.slice(at, c.from) + c.insert;
    at = c.to;
  }
  return out + s.slice(at);
}

describe("lineChanges", () => {
  const cases: Array<[string, string]> = [
    ["a\nb\nc\n", "a\nB\nc\n"],
    ["a\nb\nc\n", "a\nc\n"],
    ["a\nb\nc\n", "x\na\nb\nc\ny\n"],
    ["a\nb\nc", "a\nb\nc\n"],
    ["a\nb\nc\n", "a\nb\nc"],
    ["", "cube(1);\n"],
    ["cube(1);\n", ""],
    ["l1\nl2\nl3\nl4\nl5\nl6\n", "l1\nL2\nl3\nl4\nL5\nl6\nl7\n"],
  ];
  for (const [a, b] of cases) {
    it(`${JSON.stringify(a)} → ${JSON.stringify(b)}`, () => {
      expect(apply(a, lineChanges(a, b))).toBe(b);
    });
  }
  it("keeps unchanged regions out of the changes (cursor stays)", () => {
    const a = "one\ntwo\nthree\nfour\nfive\n";
    const b = "ONE\ntwo\nthree\nfour\nFIVE\n";
    const cs = lineChanges(a, b);
    expect(cs.length).toBe(2);
    const cursor = a.indexOf("three");
    for (const c of cs) expect(cursor < c.from || cursor >= c.to).toBe(true);
  });
  it("no change for equal texts", () => expect(lineChanges("x", "x")).toEqual([]));
});

describe("diffLines", () => {
  it("marks added and removed lines with line numbers", () => {
    const d = diffLines("a\nb\nc\n", "a\nB\nc\nd\n");
    expect(d.map((x) => x.kind)).toEqual(["same", "del", "add", "same", "add"]);
    expect(d[1]).toEqual({ kind: "del", text: "b", a: 2, b: null });
    expect(d[4]).toEqual({ kind: "add", text: "d", a: null, b: 4 });
  });
});
