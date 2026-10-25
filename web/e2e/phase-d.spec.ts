// End-to-end tests of Plan 21, Phase D in Chromium against the production build: the place of
// a failed Boolean (operands touching tangentially) drawn in the view and framed by a click on
// the error at the line end (D1), handles of `slot(p1, p2, r)` (D5). Screenshots for the
// report in reports/tmp/report21/phase-d/img.

import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { expect, test, type Page } from "@playwright/test";
import { cad, clickLine, frame, hook, line, loadText, mouseDrag, openApp, setTheme, waitDragDone, waitHandles, waitScene } from "./helpers";

const here = dirname(fileURLToPath(import.meta.url));
const OUT = join(here, "../../reports/tmp/report21/phase-d");

interface Failures {
  failures: Array<{ node: number; point: [number, number, number] }>;
}

async function shot(page: Page, name: string): Promise<void> {
  mkdirSync(join(OUT, "img"), { recursive: true });
  const clip = await page.locator("section.viewcol").boundingBox();
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
});

// Keyhole dome touching a rib along x = 7, y = 15 (crates/cad-cli/tests/data/dom_rippe.mcad).
const DOM_RIPPE = `cube([30, 30, 2]);
translate([11, 15, 0]) dom();
translate([5, 5, 0]) rippe();

module dom(h = 10) {
    difference() {
        cylinder(h = h, r = 4);
        translate([0, 0, 2]) linear_extrude(h) union() {
            circle(2);
            slot([0, 0], [0, -2.5], 1);
        }
    }
}
module rippe(l = 20, h = 8) cube([2, l, h]);
`;

test("D01 Berührung: Stelle rot markiert, Klick auf die Meldung rahmt sie ein (hell/dunkel)", async ({ page }) => {
  await openApp(page);
  await loadText(page, DOM_RIPPE);
  let s = await waitScene(page, { context: null });
  const d = s.diagnostics.find((x) => x.message.includes("berühren sich tangential"));
  expect(d?.message).toContain("Vereinigung gescheitert: Zeile 2 (dom) und Zeile 3 (rippe) berühren sich tangential bei [7, 15,");
  const plan = s.view!.plan as unknown as Failures;
  expect(plan.failures).toHaveLength(1);
  const [x, y, z] = plan.failures[0].point;
  expect(x).toBeCloseTo(7, 6);
  expect(y).toBeCloseTo(15, 6);
  expect(z).toBeGreaterThan(2);
  expect(z).toBeLessThan(8);
  expect(s.view!.edgeKinds.error ?? 0).toBeGreaterThanOrEqual(1);
  const link = page.locator(".cm-lineend-locate");
  await expect(link).toContainText("berühren sich tangential");
  await expect(link).toHaveAttribute("title", "Klick: Stelle in der Ansicht zeigen");
  await shot(page, "d01-beruehrung-hell");
  mkdirSync(join(OUT, "img"), { recursive: true });
  await page.screenshot({ path: join(OUT, "img", "d01-beruehrung-editor.png") });
  const before = s.view!.camera;
  await link.click();
  await expect.poll(async () => (await cad(page)).view!.camera.target[0]).toBeCloseTo(7, 3);
  s = await cad(page);
  expect(s.view!.camera.target[1]).toBeCloseTo(15, 3);
  expect(s.view!.camera.target[2]).toBeCloseTo(z, 3);
  expect(s.view!.camera.distance).toBeLessThan(before.distance);
  expect(s.notice).toContain("eingerahmt");
  await shot(page, "d01-beruehrung-eingerahmt-hell");
  await setTheme(page, "dark");
  await frame(page);
  await shot(page, "d01-beruehrung-eingerahmt-dunkel");
  // a gap of 0.2 mm: no error, no place
  await loadText(page, DOM_RIPPE.replace("translate([5, 5, 0]) rippe()", "translate([4.8, 5, 0]) rippe()"));
  s = await cad(page);
  expect((s.view!.plan as unknown as Failures).failures).toHaveLength(0);
  expect(s.diagnostics.filter((x) => x.severity === "error")).toHaveLength(0);
  await expect(page.locator(".cm-lineend-locate")).toHaveCount(0);
  note({ id: "D01", message: d?.message, point: plan.failures[0].point, camera: s.view!.camera.target });
});

test("D02 slot(p1, p2, r): Griffe für beide Punkte und den Radius, Radius ziehen", async ({ page }) => {
  await openApp(page);
  await loadText(page, "linear_extrude(2) slot([0, 0], [20, 10], 3);\n");
  await clickLine(page, 1, 20);
  const hs = await waitHandles(page, "(hs) => hs.length >= 3");
  const kinds = hs.map((h) => `${h.kind}#${h.index}`).sort();
  expect(kinds).toEqual(expect.arrayContaining(["point#0", "point#1", "radius#0"]));
  const r = hs.find((h) => h.kind === "radius")!;
  expect(r.value[0]).toBeCloseTo(3, 9);
  await shot(page, "d02-slot-griffe");
  // drag the radius handle outwards (across the slot): the literal 3 grows
  const from = r.screen!;
  const p2 = hs.find((h) => h.kind === "point" && h.index === 1)!.screen!;
  const dir = { x: from.x - p2.x, y: from.y - p2.y };
  const len = Math.hypot(dir.x, dir.y) || 1;
  await mouseDrag(page, from, line(from, { x: from.x + (dir.x / len) * 40, y: from.y + (dir.y / len) * 40 }, 10), { alt: true });
  await waitDragDone(page);
  await waitScene(page);
  const text = await hook(page, (c) => c.text());
  const m = text.match(/slot\(\[0, 0\], \[20, 10\], ([\d.]+)\);/);
  expect(m).not.toBeNull();
  expect(Number(m![1])).toBeGreaterThan(3);
  await shot(page, "d02-slot-radius-gezogen");
  note({ id: "D02", handles: kinds, text });
});
