// Screenshots of the browser build on solidcore (test report 23: pure Rust, one Wasm module):
// three examples in the full window, with the evaluation time the UI measured. Runs only with
// REPORT23=1 (the macropad takes about a minute in software WebGL); output in reports/tmp/report23/ui/.

import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { expect, test } from "@playwright/test";
import { cad, frame, loadText, openApp } from "./helpers";

const here = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(here, "../..");
const OUT = join(ROOT, "reports/tmp/report23/ui");
const example = (name: string) =>
  readFileSync(join(ROOT, "examples", name), "utf8");

test.skip(!process.env.REPORT23, "report 23 only (REPORT23=1)");

test("R23 examples in the browser on solidcore", async ({ page }) => {
  test.setTimeout(600_000);
  mkdirSync(OUT, { recursive: true });
  await openApp(page);
  const times: Record<string, number | null> = {};
  for (const [i, name] of [
    "08-macropad",
    "07-mug",
    "09-flashlight",
  ].entries()) {
    await loadText(page, example(`${name}.mcad`));
    const s = await cad(page);
    expect(s.engineError).toBeNull();
    expect(s.diagnostics.filter((d) => d.severity === "error")).toEqual([]);
    times[name] = s.metrics.evalMs.at(-1) ?? null;
    await frame(page);
    await frame(page);
    await page.screenshot({ path: join(OUT, `${i + 1}_${name}.png`) });
  }
  writeFileSync(join(OUT, "times.json"), JSON.stringify(times, null, 1));
});
