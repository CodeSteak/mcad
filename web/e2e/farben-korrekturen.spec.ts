// Corrections of the colour functions (test report 12, findings F1–F9 of the independent
// review in tests/farben-pruefung/): the view shows each instance in its own colour and the
// colour regions as printed, the slot choice of a parameter colour writes a filament table,
// material outside parts is exported, no colour group in the 3MF.

import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { type Page, expect, test } from "@playwright/test";
import { cad, hook, loadText, openApp, record12, screenshot12, sideTab, waitScene, waitView } from "./helpers";

test.setTimeout(180_000);

const CASES = join(dirname(fileURLToPath(import.meta.url)), "../../tests/farben-pruefung/cases");
// a comment first: the cursor (line 1) selects no statement, the whole model is shown full
const caseText = (name: string) => `// ${name}\n${readFileSync(join(CASES, name), "utf8")}`;

type Summary = { colorGroup: boolean; mColors: number; items: number; objects: string[]; notes: string[]; extruders: number[] };

async function text(page: Page): Promise<string> {
  return page.evaluate(() => (window as unknown as { __cad: { text(): string } }).__cad.text());
}

/** Canvas pixels whose hue is within 12° of the colour's (chroma > 0.06, any shading). */
async function huePixels(page: Page, hex: string): Promise<number> {
  const png = await page.getByTestId("viewport-canvas").screenshot();
  return page.evaluate(
    async ([b64, hx]) => {
      const img = new Image();
      img.src = `data:image/png;base64,${b64}`;
      await img.decode();
      const c = document.createElement("canvas");
      c.width = img.width;
      c.height = img.height;
      const g = c.getContext("2d")!;
      g.drawImage(img, 0, 0);
      const d = g.getImageData(0, 0, c.width, c.height).data;
      const lin = (v: number) => (v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4);
      const lch = (r: number, gg: number, b: number) => {
        const [R, G, B] = [r, gg, b].map((x) => lin(x / 255));
        const l = Math.cbrt(0.4122214708 * R + 0.5363325363 * G + 0.0514459929 * B);
        const m = Math.cbrt(0.2119034982 * R + 0.6806995451 * G + 0.1073969566 * B);
        const s = Math.cbrt(0.0883024619 * R + 0.2817188376 * G + 0.6299787005 * B);
        const a = 1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s;
        const bb = 0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s;
        return [Math.hypot(a, bb), ((Math.atan2(bb, a) * 180) / Math.PI + 360) % 360];
      };
      const h = hx as string;
      const [, h0] = lch(parseInt(h.slice(1, 3), 16), parseInt(h.slice(3, 5), 16), parseInt(h.slice(5, 7), 16));
      let n = 0;
      for (let i = 0; i < d.length; i += 4) {
        const [ch, hh] = lch(d[i], d[i + 1], d[i + 2]);
        const dh = Math.abs(hh - h0);
        if (ch > 0.06 && Math.min(dh, 360 - dh) < 12) n++;
      }
      return n;
    },
    [png.toString("base64"), hex] as const,
  );
}

async function openWithList(page: Page): Promise<void> {
  await openApp(page);
  await sideTab(page, "Farben").click();
}

test("G01 F1: gleiche Geometrie, verschiedene Farben – Regionen und Farbe je Fläche", async ({ page }) => {
  await openWithList(page);
  await loadText(page, caseText("pal2.scad"));
  // default: the colour regions (as printed) replace the faces once they are computed
  await waitView(page, "(s) => s.view.regionsDrawn === 3");
  const reg = { red: await huePixels(page, "#c74240"), blue: await huePixels(page, "#2e65b9"), green: await huePixels(page, "#4ca167") };
  for (const [k, v] of Object.entries(reg)) expect(v, k).toBeGreaterThan(300);
  await screenshot12(page, "g01-regionen");
  // colours per face (regions off): each cube its own colour (before: three red cubes)
  await page.getByRole("checkbox", { name: "Farbregionen zeigen" }).uncheck();
  await waitView(page, "(s) => s.view.regionsDrawn === 0");
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
  const faces = { red: await huePixels(page, "#c74240"), blue: await huePixels(page, "#2e65b9"), green: await huePixels(page, "#4ca167") };
  for (const [k, v] of Object.entries(faces)) expect(v, k).toBeGreaterThan(300);
  await screenshot12(page, "g01-flaechen");
  // a loop over palette names with equal tiles
  await page.getByRole("checkbox", { name: "Farbregionen zeigen" }).check();
  await loadText(page, caseText("palette.scad"));
  await waitView(page, "(s) => s.view.regionsDrawn === 16");
  const teal = await huePixels(page, "#217274");
  const magenta = await huePixels(page, "#cd67b7");
  expect(teal).toBeGreaterThan(100);
  expect(magenta).toBeGreaterThan(100);
  await screenshot12(page, "g01-palette");
  record12({
    id: "G01",
    title: "F1: gleiche Geometrie in verschiedenen Farben",
    caption:
      "pal2.scad (drei gleiche Würfel rot, blau, grün) und palette.scad (16 gleiche Kacheln in einer Schleife): vorher alle in der ersten Farbe. Jetzt zeigt die Ansicht die Farbregionen (die Körper des 3MF), und auch die Farbe je Fläche nimmt die Farbe der eigenen Instanz.",
    images: [
      { name: "g01-regionen", label: "pal2: Farbregionen (Standard)" },
      { name: "g01-flaechen", label: "pal2: Farbe je Fläche (Regionen aus)" },
      { name: "g01-palette", label: "palette.scad: 16 Kacheln" },
    ],
    checks: [
      `Regionen: ${reg.red} rote, ${reg.blue} blaue, ${reg.green} grüne Pixel`,
      `je Fläche: ${faces.red} rote, ${faces.blue} blaue, ${faces.green} grüne Pixel`,
      `Palette: ${teal} petrolfarbene, ${magenta} magentafarbene Pixel, 16 Regionen`,
    ],
  });
});

test("G02 F2: Slot einer Parameterfarbe schreibt filament(…) statt filament = n", async ({ page }) => {
  await openWithList(page);
  const src = caseText("slot_mod.scad");
  await loadText(page, src);
  await expect(page.getByRole("region", { name: "Farben und Filamente" })).toBeVisible();
  await page.getByRole("combobox", { name: "Filament für red" }).selectOption("4");
  await expect.poll(() => text(page)).toContain('filament("red", 4);');
  expect(await text(page)).not.toContain("filament = 4");
  await waitScene(page);
  await expect(page.getByRole("combobox", { name: "Filament für red" })).toHaveValue("4");
  const s = await cad(page);
  expect(s.colors.map((c) => [c.label, c.filament, c.explicit])).toEqual([
    ["red", 4, true],
    ["blue", 1, false],
    ["green", 2, false],
  ]);
  await screenshot12(page, "g02-tabelle", "full");
  await page.getByRole("combobox", { name: "Filament für red" }).selectOption("auto");
  await expect.poll(() => text(page)).toBe(src);
  record12({
    id: "G02",
    title: "F2: Slot einer Farbe aus einem Parameter",
    caption:
      "slot_mod.scad: module bar(c) color(c) …; Slot 4 für red schrieb vorher filament = 4 an color(c) – alle drei Farben bekamen Slot 4. Jetzt schreibt die UI filament(\"red\", 4); in die Datei; blue und green bleiben automatisch. „automatisch“ entfernt die Zeile wieder.",
    images: [{ name: "g02-tabelle", label: "filament(\"red\", 4); im Code, Farbliste" }],
    checks: ['Text enthält filament("red", 4); und kein filament = 4', "Slots red 4 (fest), blue 1, green 2", "automatisch: Text wie vorher"],
  });
});

test("G03 F3: Ansicht wie Druck – Überlappung, Verrundung, 2D-Farbe", async ({ page }) => {
  await openWithList(page);
  await loadText(page, caseText("r_fillet2.scad"));
  await waitView(page, "(s) => s.view.regionsDrawn === 2");
  const red = await huePixels(page, "#c74240");
  expect(red).toBeGreaterThan(2000);
  await screenshot12(page, "g03-verrundung");
  await loadText(page, caseText("r_overlap3.scad"));
  await waitView(page, "(s) => s.view.regionsDrawn === 2");
  const blue = await huePixels(page, "#2e65b9");
  const red2 = await huePixels(page, "#c74240");
  expect(blue).toBeGreaterThan(red2);
  await screenshot12(page, "g03-ueberlappung");
  await loadText(page, caseText("r_extrude.scad"));
  await waitView(page, "(s) => s.view.regionsDrawn === 3");
  const st = await cad(page);
  const w = st.diagnostics.find((d) => d.line === 4 && d.severity === "warning");
  expect(w?.message).toContain("2D-Profil");
  expect(st.colors.map((c) => c.label)).toEqual(["Standard", "red", "blue"]);
  await screenshot12(page, "g03-extrusion", "full");
  record12({
    id: "G03",
    title: "F3: Ansicht = Druck",
    caption:
      "r_fillet2.scad: die Verrundung zwischen rot und blau war grau gezeigt und rot exportiert, jetzt rot gezeigt. r_overlap3.scad: die spätere Farbe (blau) besitzt die Überlappung, auch in der Ansicht. r_extrude.scad: color() im 2D-Profil färbt die Extrusion (rot, blau); gemischte 2D-Farben unter einer Extrusion werden nicht getrennt – Warnung, Standard.",
    images: [
      { name: "g03-verrundung", label: "Verrundung an der Farbgrenze" },
      { name: "g03-ueberlappung", label: "Überlappung: blau gewinnt" },
      { name: "g03-extrusion", label: "Extrusion mit 2D-Farbe, Warnung Zeile 3" },
    ],
    checks: [`${red} rote Pixel an Platte und Verrundung`, `Überlappung: ${blue} blaue > ${red2} rote Pixel`, "Warnung „Farben im 2D-Profil …“ in Zeile 3, Liste Standard, red, blue"],
  });
});

test("G04 F4/Bambu: Material außerhalb von Bauteilen, 3MF ohne Farbgruppe", async ({ page }) => {
  await openWithList(page);
  await loadText(page, '// Bauteil und Material ohne Bauteil\nmodule a() color("white") cube([20, 10, 2]);\nteil: a();\ntranslate([0, 20, 0]) color("yellow") cube(5);\n');
  await expect(page.getByTestId("color-volume-yellow")).toContainText("125 mm³");
  const sum = await hook(page, (c) => (c as unknown as { exportSummary(f: string, colors?: boolean): Promise<Summary> }).exportSummary("3mf", true));
  expect(sum.objects).toEqual(["teil", "ohne Bauteil"]);
  expect(sum.items).toBe(2);
  expect(sum.colorGroup).toBe(false);
  expect(sum.mColors).toBe(0);
  expect(sum.notes.join(" ")).toContain("ohne Farbdialog");
  // the button: the notice names the objects
  const [dl] = await Promise.all([page.waitForEvent("download"), page.getByRole("region", { name: "Farben und Filamente" }).getByRole("button", { name: "3MF mehrfarbig" }).click()]);
  expect(readFileSync((await dl.path())!).toString("utf8")).toContain('name="ohne Bauteil – yellow"');
  await expect(page.getByTestId("status-notice")).toContainText("ohne Bauteil");
  await screenshot12(page, "g04-export", "full");
  record12({
    id: "G04",
    title: "F4: Material ohne Bauteil, 3MF für Bambu Studio",
    caption:
      "Ein Bauteil teil und ein gelber Würfel ohne Bauteil: vorher 0 mm³ in der Liste und im UI-Export weggelassen. Jetzt 125 mm³, eigenes Objekt „ohne Bauteil“, gleiche Datei wie cad export. Das 3MF hat keine Farbgruppe mehr: Bambu Studio öffnet keinen Farbdialog und übernimmt die Slots aus model_settings.config.",
    images: [{ name: "g04-export", label: "Export-Meldung mit Objekten" }],
    checks: [`Objekte ${sum.objects.join(", ")}`, "keine m:colorgroup, keine m:color", `Hinweise: ${sum.notes.join("; ")}`],
  });
});
