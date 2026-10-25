// End-to-end tests of the collision asserts (Plan 21, Phase C) in Chromium against the
// production build: keep-out volume display and toggle, violations (red intersection body,
// status bar, jump and framing), export dialog "Trotzdem exportieren", assert_touches (contact
// face, gap), plus one scenario each for hull/minkowski (Phase A, needs `sc_offset` in
// smlib.wasm) and the STL import (Phase B). Screenshots for the report in
// reports/tmp/report21/phase-c/img.

import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { expect, test, type Page } from "@playwright/test";
import { cad, loadText, openApp, openExample, setTheme, waitScene } from "./helpers";

const here = dirname(fileURLToPath(import.meta.url));
const OUT = join(here, "../../reports/tmp/report21/phase-c");

interface AssertsInfo {
  version: number | null;
  completeVersion: number | null;
  items: Array<{ kind: string; line: number; ok: boolean; checked: boolean; bodies: number; pairs: Array<{ ok: boolean; problem: string | null; message: string; volume: number | null; distance: number | null; contact: string | null; contact_area: number | null }> }>;
  view: { drawn: Record<string, number>; labels: Array<{ text: string; bad: boolean; visible: boolean }>; keepout: number } | null;
  showKeepout: boolean;
  focus: { node: number; pair: number } | null;
  exportConfirm: { what: string; violations: string[] } | null;
}

const asserts = (page: Page) => page.evaluate(() => (window as unknown as { __cad: { asserts(): AssertsInfo } }).__cad.asserts());

/** Waits until the assertion results of the shown scene are there (and drawn). */
async function waitAsserts(page: Page): Promise<AssertsInfo> {
  await page.waitForFunction(
    () => {
      const a = (window as unknown as { __cad: { asserts(): AssertsInfo; state(): { shownVersion: number | null } } }).__cad;
      const s = a.asserts();
      return s.version !== null && s.version === s.completeVersion && s.version === a.state().shownVersion && s.view !== null;
    },
    null,
    { timeout: 60_000, polling: 50 },
  );
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
  return asserts(page);
}

async function shot(page: Page, name: string, area: "view" | "full" = "view"): Promise<void> {
  mkdirSync(join(OUT, "img"), { recursive: true });
  const clip = area === "view" ? await page.locator("section.viewcol").boundingBox() : null;
  await page.screenshot({ path: join(OUT, "img", `${name}.png`), ...(clip ? { clip } : {}) });
}

const results: Array<Record<string, unknown>> = [];
function note(r: Record<string, unknown>): void {
  results.push(r);
  mkdirSync(OUT, { recursive: true });
  writeFileSync(join(OUT, "e2e-values.json"), JSON.stringify(results, null, 2));
}

test.beforeEach(async ({ page }) => {
  page.on("pageerror", (e) => console.log("pageerror:", e.message));
  await page.addInitScript(() => {
    try {
      // once per test (a reload within the test keeps the choice)
      if (!sessionStorage.getItem("e2e-keepout")) {
        localStorage.removeItem("cad-keepout");
        sessionStorage.setItem("e2e-keepout", "1");
      }
    } catch {
      /* no storage */
    }
  });
});

const KEEPOUT_BAD = `cube([40, 30, 3]);
translate([0, 0, 3]) cube([12, 30, 6]);
assert_keepout(clearance = 0.5) translate([10, 10, 2]) cube([10, 10, 5]);
`;

const TOUCHES = `assert_touches() { cube(10); translate([10, 2, 0]) cube([10, 5, 5]); }
assert_touches() { translate([0, 20, 0]) cube(10); translate([10.2, 20, 0]) cube(10); }
`;

test("C01 Sperrvolumen aus STL: durchscheinend, nicht wählbar, Asserts ok (hell/dunkel)", async ({ page }) => {
  await openApp(page);
  await openExample(page, "spec/asserts.scad");
  const a = await waitAsserts(page);
  expect(a.items.map((r) => r.kind)).toEqual(["touches", "intersect", "disjoint", "keepout"]);
  expect(a.items.every((r) => r.ok && r.checked)).toBe(true);
  expect(a.view?.keepout).toBe(1);
  const s = await cad(page);
  const ko = s.view!.plan.items.find((i) => i.mode === "keepout");
  expect(ko).toBeTruthy();
  // the keep-out volume is no pickable body: a click on its centre picks the case (or nothing)
  const p = await page.evaluate(() => (window as unknown as { __cad: { project(p: number[]): { x: number; y: number; visible: boolean } } }).__cad.project([35, 17, 11]));
  const pick = await page.evaluate(([x, y]) => (window as unknown as { __cad: { pickAt(x: number, y: number): { nodes: number[] } | null } }).__cad.pickAt(x, y), [p.x, p.y]);
  expect(pick?.nodes ?? []).not.toContain(ko!.node);
  // the face contact of the lid as a label
  expect(a.view!.labels.some((l) => l.text === "Kontakt 384 mm²" && !l.bad)).toBe(true);
  await shot(page, "c01-keepout-ok-hell");
  await setTheme(page, "dark");
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
  await shot(page, "c01-keepout-ok-dunkel");
  note({ id: "C01", keepout: a.view?.keepout, labels: a.view?.labels, kinds: a.items.map((r) => r.kind) });
});

test("C02 Verletzung: roter Schnittkörper, Statusleiste springt und rahmt ein (hell/dunkel)", async ({ page }) => {
  await openApp(page);
  await loadText(page, KEEPOUT_BAD);
  const a = await waitAsserts(page);
  const pr = a.items[0].pairs[0];
  expect(a.items[0].ok).toBe(false);
  expect(pr.problem).toBe("overlap");
  // plate 10 × 10 × 1 plus the block 2 × 10 × 4 (z 3…7)
  expect(pr.volume!).toBeCloseTo(100 + 2 * 10 * 4, 6);
  expect(a.view!.drawn["assert-overlap"]).toBeGreaterThanOrEqual(1);
  expect(a.view!.labels.some((l) => l.bad && l.text === "Schnitt 180 mm³")).toBe(true);
  await expect(page.getByTestId("status-asserts")).toHaveText("1 Assert-Verletzung");
  // the warning at the end of the editor line
  await expect(page.locator(".cm-lineend-assert")).toContainText("assert_keepout: ");
  await shot(page, "c02-keepout-verletzt-hell");
  const before = (await cad(page)).view!.camera;
  await page.getByTestId("status-asserts").click();
  await page.waitForFunction(() => (window as unknown as { __cad: { asserts(): AssertsInfo } }).__cad.asserts().focus !== null);
  const s = await cad(page);
  expect(await page.evaluate(() => (window as unknown as { __cad: { cursorLine(): number } }).__cad.cursorLine())).toBe(3);
  expect(s.view!.camera.distance).toBeLessThan(before.distance);
  // framed on the intersection (centre of x 10…20, y 10…20, z 2…7)
  expect(s.view!.camera.target[0]).toBeCloseTo(15, 3);
  expect(s.view!.camera.target[2]).toBeCloseTo(4.5, 3);
  await shot(page, "c02-keepout-verletzt-eingerahmt-hell");
  // click on the warning in the editor line: the same
  await page.evaluate(() => (window as unknown as { __cad: { fitAll(): void } }).__cad.fitAll());
  await page.locator(".cm-lineend-assert").click();
  await expect.poll(async () => (await cad(page)).view!.camera.target[0]).toBeCloseTo(15, 3);
  await setTheme(page, "dark");
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
  await shot(page, "c02-keepout-verletzt-eingerahmt-dunkel");
  note({ id: "C02", volume: pr.volume, expected: 180, message: pr.message, labels: a.view!.labels, target: s.view!.camera.target });
});

test("C03 Sperrvolumen ein-/ausblenden, Zustand bleibt nach dem Neuladen", async ({ page }) => {
  await openApp(page);
  await openExample(page, "spec/asserts.scad");
  await waitAsserts(page);
  const btn = page.getByRole("button", { name: "Sperrvolumen" });
  await expect(btn).toHaveAttribute("aria-pressed", "true");
  await btn.click();
  await expect.poll(async () => (await asserts(page)).view?.keepout).toBe(0);
  expect((await cad(page)).view!.plan.items.some((i) => i.mode === "keepout")).toBe(false);
  await shot(page, "c03-keepout-ausgeblendet");
  // kept in localStorage (the init script only clears it before the first load)
  const stored = await page.evaluate(() => localStorage.getItem("cad-keepout"));
  expect(stored).toBe("0");
  await page.reload();
  await expect(page.getByTestId("editor").locator(".cm-content")).toBeVisible();
  await openExample(page, "spec/asserts.scad");
  await waitAsserts(page);
  await expect(btn).toHaveAttribute("aria-pressed", "false");
  expect((await asserts(page)).view?.keepout).toBe(0);
  await btn.click();
  await expect.poll(async () => (await asserts(page)).view?.keepout).toBe(1);
  note({ id: "C03", stored });
});

test("C04 Export mit Verletzung: Rückfrage, Abbrechen, Trotzdem exportieren", async ({ page }) => {
  await openApp(page);
  await loadText(page, KEEPOUT_BAD);
  await waitAsserts(page);
  await page.keyboard.press("Control+e");
  const dlg = page.getByRole("alertdialog", { name: "Export trotz Assert-Verletzung?" });
  await expect(dlg).toBeVisible();
  await expect(dlg).toContainText("Z.3 assert_keepout: ");
  await shot(page, "c04-export-dialog-hell", "full");
  await dlg.getByRole("button", { name: "Abbrechen" }).click();
  await expect(dlg).toBeHidden();
  await expect(page.getByTestId("status-notice")).toHaveText("Export abgebrochen (Assert-Verletzung)");
  await page.keyboard.press("Control+e");
  await expect(dlg).toBeVisible();
  const [dl] = await Promise.all([page.waitForEvent("download"), dlg.getByRole("button", { name: "Trotzdem exportieren" }).click()]);
  expect(dl.suggestedFilename()).toMatch(/\.3mf$/);
  await setTheme(page, "dark");
  await page.keyboard.press("Control+e");
  await expect(dlg).toBeVisible();
  await shot(page, "c04-export-dialog-dunkel", "full");
  await page.keyboard.press("Escape");
  await expect(dlg).toBeHidden();
  // without violations: no question
  await loadText(page, "cube(5);\nassert_keepout() translate([10, 0, 0]) cube(2);\n");
  await waitAsserts(page);
  const [dl2] = await Promise.all([page.waitForEvent("download"), page.keyboard.press("Control+e")]);
  expect(dl2.suggestedFilename()).toMatch(/\.3mf$/);
  note({ id: "C04", file: dl.suggestedFilename() });
});

test("C05 assert_touches: Kontaktfläche grün, Spalt als Maßlinie (hell/dunkel)", async ({ page }) => {
  await openApp(page);
  await loadText(page, TOUCHES);
  const a = await waitAsserts(page);
  const [face, gap] = a.items;
  expect(face.ok).toBe(true);
  expect(face.pairs[0].contact).toBe("face");
  expect(face.pairs[0].contact_area!).toBeCloseTo(25, 9);
  expect(gap.ok).toBe(false);
  expect(gap.pairs[0].problem).toBe("gap");
  expect(gap.pairs[0].distance!).toBeCloseTo(0.2, 5);
  expect(a.view!.drawn["assert-contact"]).toBeGreaterThanOrEqual(1);
  expect(a.view!.drawn["assert-gap"]).toBe(1);
  expect(a.view!.labels.map((l) => l.text).sort()).toEqual(["Kontakt 25 mm²", "Spalt 0.2 mm"]);
  await shot(page, "c05-touches-hell");
  await setTheme(page, "dark");
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
  await shot(page, "c05-touches-dunkel");
  note({ id: "C05", area: face.pairs[0].contact_area, gap: gap.pairs[0].distance, labels: a.view!.labels });
});

test("C06 hull/minkowski im Browser (Phase A, sc_offset)", async ({ page }) => {
  await openApp(page);
  await openExample(page, "spec/hull_minkowski.scad");
  const s = await cad(page);
  expect(s.diagnostics.filter((d) => d.severity !== "info")).toEqual([]);
  // minkowski() { cube([20, 20, 10]); sphere(2); } at [0, 50, 0]: 24 × 24 × 14
  const b = await page.evaluate(() => (window as unknown as { __cad: { nodeBounds(l: number): Array<{ label: string; min: number[]; max: number[] }> } }).__cad.nodeBounds(23));
  const mk = b.find((x) => x.label === "minkowski") ?? b[0];
  expect(mk.max[0] - mk.min[0]).toBeCloseTo(24, 4);
  expect(mk.max[2] - mk.min[2]).toBeCloseTo(14, 4);
  await shot(page, "c06-hull-minkowski");
  note({ id: "C06", minkowski: mk });
});

test("C07 STL-Import im Browser (Phase B)", async ({ page }) => {
  await openApp(page);
  await openExample(page, "spec/stl_import.scad");
  const s = await waitScene(page);
  expect(s.diagnostics.filter((d) => d.severity === "error")).toEqual([]);
  const b = await page.evaluate(() => (window as unknown as { __cad: { rootBounds(): { min: number[]; max: number[] } | null } }).__cad.rootBounds());
  // B-rep bracket 0…20 and the mirrored mesh −25…−5 in x, 15 high
  expect(b!.min[0]).toBeCloseTo(-25, 4);
  expect(b!.max[0]).toBeCloseTo(20, 4);
  expect(b!.max[2]).toBeCloseTo(15, 4);
  await shot(page, "c07-stl-import");
  note({ id: "C07", bounds: b });
});
