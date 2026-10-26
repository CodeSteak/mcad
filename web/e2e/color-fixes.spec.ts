// Corrections of the colour functions (test report 12, findings F1–F9 of the independent
// review in tests/color-review/): the view shows each instance in its own colour and the
// colour regions as printed, the slot choice of a parameter colour writes a filament table,
// material outside parts is exported, no colour group in the 3MF.

import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { type Page, expect, test } from "@playwright/test";
import { cad, hook, loadText, openApp, record12, screenshot12, sideTab, waitScene, waitView } from "./helpers";

test.setTimeout(180_000);

const CASES = join(dirname(fileURLToPath(import.meta.url)), "../../tests/color-review/cases");
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

test("G01 F1: equal geometry, different colours – regions and colour per face", async ({ page }) => {
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
    title: "F1: equal geometry in different colours",
    caption:
      "pal2.scad (three equal cubes red, blue, green) and palette.scad (16 equal tiles in a loop): before, all in the first colour. Now the view shows the colour regions (the bodies of the 3MF), and the colour per face takes the colour of its own instance too.",
    images: [
      { name: "g01-regionen", label: "pal2: colour regions (default)" },
      { name: "g01-flaechen", label: "pal2: colour per face (regions off)" },
      { name: "g01-palette", label: "palette.scad: 16 tiles" },
    ],
    checks: [
      `regions: ${reg.red} red, ${reg.blue} blue, ${reg.green} green pixels`,
      `per face: ${faces.red} red, ${faces.blue} blue, ${faces.green} green pixels`,
      `palette: ${teal} teal, ${magenta} magenta pixels, 16 regions`,
    ],
  });
});

test("G02 F2: the slot of a parameter colour writes filament(…) instead of filament = n", async ({ page }) => {
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
    title: "F2: slot of a colour from a parameter",
    caption:
      "slot_mod.scad: module bar(c) color(c) …; slot 4 for red used to write filament = 4 at color(c) – all three colours got slot 4. Now the UI writes filament(\"red\", 4); into the file; blue and green stay automatic. “automatisch” (automatic) removes the line again.",
    images: [{ name: "g02-tabelle", label: "filament(\"red\", 4); in the code, colour list" }],
    checks: ['the text contains filament("red", 4); and no filament = 4', "slots red 4 (fixed), blue 1, green 2", "automatic: text as before"],
  });
});

test("G03 F3: view as printed – overlap, fillet, 2D colour", async ({ page }) => {
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
  // the label of material without color() is fixed ("default"); the colour list shows it translated
  expect(st.colors.map((c) => c.label)).toEqual(["default", "red", "blue"]);
  await expect(page.getByTestId("color-row-default")).toContainText("Standard");
  await screenshot12(page, "g03-extrusion", "full");
  record12({
    id: "G03",
    title: "F3: view = print",
    caption:
      "r_fillet2.scad: the fillet between red and blue was shown grey and exported red, now shown red. r_overlap3.scad: the later colour (blue) owns the overlap, in the view too. r_extrude.scad: color() in the 2D profile colours the extrusion (red, blue); mixed 2D colours under one extrusion are not separated – warning, default.",
    images: [
      { name: "g03-verrundung", label: "fillet at the colour boundary" },
      { name: "g03-ueberlappung", label: "overlap: blue wins" },
      { name: "g03-extrusion", label: "extrusion with a 2D colour, warning line 3" },
    ],
    checks: [`${red} red pixels on plate and fillet`, `overlap: ${blue} blue > ${red2} red pixels`, "warning “Farben im 2D-Profil …” (colours in the 2D profile) on line 3, list Standard (default), red, blue"],
  });
});

test("G04 F4/Bambu: material outside parts, 3MF without a colour group", async ({ page }) => {
  await openWithList(page);
  await loadText(page, '// part and material without a part\nmodule a() color("white") cube([20, 10, 2]);\npart: a();\ntranslate([0, 20, 0]) color("yellow") cube(5);\n');
  await expect(page.getByTestId("color-volume-yellow")).toContainText("125 mm³");
  const sum = await hook(page, (c) => (c as unknown as { exportSummary(f: string, colors?: boolean): Promise<Summary> }).exportSummary("3mf", true));
  expect(sum.objects).toEqual(["part", "no part"]);
  expect(sum.items).toBe(2);
  expect(sum.colorGroup).toBe(false);
  expect(sum.mColors).toBe(0);
  expect(sum.notes.join(" ")).toContain("ohne Farbdialog");
  // the button: the notice names the objects
  const [dl] = await Promise.all([page.waitForEvent("download"), page.getByRole("region", { name: "Farben und Filamente" }).getByRole("button", { name: "3MF mehrfarbig" }).click()]);
  expect(readFileSync((await dl.path())!).toString("utf8")).toContain('name="no part – yellow"');
  await expect(page.getByTestId("status-notice")).toContainText("no part");
  await screenshot12(page, "g04-export", "full");
  record12({
    id: "G04",
    title: "F4: material without a part, 3MF for Bambu Studio",
    caption:
      "A part part and a yellow cube without a part: before 0 mm³ in the list and left out of the UI export. Now 125 mm³, its own object “no part” (fixed name), the same file as mcad export. The 3MF has no colour group any more: Bambu Studio opens no colour dialog and takes the slots from model_settings.config.",
    images: [{ name: "g04-export", label: "export notice with the objects" }],
    checks: [`objects ${sum.objects.join(", ")}`, "no m:colorgroup, no m:color", `notes: ${sum.notes.join("; ")}`],
  });
});
