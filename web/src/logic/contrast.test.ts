import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { checkTheme, contrast, parseCss, themeTokens } from "./contrast";

const css = readFileSync(join(dirname(fileURLToPath(import.meta.url)), "../styles.css"), "utf8");

describe("theme contrast (WCAG 2)", () => {
  it("parses colours", () => {
    expect(parseCss("#fff")?.rgb).toEqual([255, 255, 255]);
    expect(parseCss("rgba(10, 20, 30, 0.5)")).toEqual({ rgb: [10, 20, 30], a: 0.5 });
    expect(contrast([0, 0, 0], [255, 255, 255])).toBeCloseTo(21, 5);
  });
  for (const theme of ["light", "dark"] as const) {
    it(`${theme}: text AA, UI parts 3:1`, () => {
      const res = checkTheme(themeTokens(css, theme));
      const bad = res.filter((r) => !r.ok).map((r) => `${r.text} on ${r.bg}: ${r.ratio.toFixed(2)} < ${r.min}`);
      if (process.env.SHOW_CONTRAST) console.log(theme + "\n" + res.map((r) => `${r.text}/${r.bg} ${r.ratio.toFixed(2)}`).join("\n"));
      expect(bad).toEqual([]);
    });
  }
});
