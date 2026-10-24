// View cube (UI overhaul): the cells of the faces map onto the 26 view directions of a cube
// (6 faces, 12 edges, 8 corners), each face is seen upright from outside.

import { describe, expect, it } from "vitest";
import { FACES, cellDirection } from "./viewCube";

const key = (d: number[]) => d.map((x) => Math.sign(Math.round(x * 1000))).join(",");

describe("Ansichtswürfel", () => {
  it("Flächenmitte = Normale, Ecke vorne rechts oben = (1, −1, 1)/√3", () => {
    for (const f of FACES) expect(cellDirection(f, 1, 1)).toEqual(f.n.map((x) => x + 0));
    const front = FACES.find((f) => f.name === "Vorne")!;
    const c = cellDirection(front, 2, 0);
    const s = 1 / Math.sqrt(3);
    [s, -s, s].forEach((x, k) => expect(c[k]).toBeCloseTo(x, 12));
    // right edge band of the front face: the edge front/right
    const e = cellDirection(front, 2, 1);
    [Math.SQRT1_2, -Math.SQRT1_2, 0].forEach((x, k) => expect(e[k]).toBeCloseTo(x, 12));
  });
  it("54 Felder ergeben die 26 Richtungen; Kanten auf 2, Ecken auf 3 Flächen", () => {
    const count = new Map<string, number>();
    for (const f of FACES) for (let i = 0; i < 3; i++) for (let j = 0; j < 3; j++) {
      const k = key(cellDirection(f, i, j));
      count.set(k, (count.get(k) ?? 0) + 1);
    }
    expect(count.size).toBe(26);
    const by = (n: number) => [...count.entries()].filter(([k]) => k.split(",").filter((x) => x !== "0").length === n).map(([, c]) => c);
    expect(by(1)).toEqual(Array(6).fill(1));
    expect(by(2)).toEqual(Array(12).fill(2));
    expect(by(3)).toEqual(Array(8).fill(3));
  });
  it("Oben von außen: rechts = +x, unten im Bild = vorne (−y)", () => {
    const top = FACES.find((f) => f.name === "Oben")!;
    expect(key(cellDirection(top, 2, 1))).toBe("1,0,1");
    expect(key(cellDirection(top, 1, 2))).toBe("0,-1,1");
  });
});
