#!/usr/bin/env node
// Test report 20 (UI overhaul): the same states of the browser UI as screenshots, once
// for the old UI (label "vorher" = before, build of the commit before the overhaul) and once for
// the new one ("nachher" = after; the labels are directory names read by report20.mjs). The script only uses the test hooks (`window.__cad`) and region selectors that
// exist in both versions, so the pairs show exactly the same model, theme and mode.
//
//   (cd web && pnpm exec vite build && pnpm exec vite preview --port 4180 --strictPort --host 127.0.0.1) &
//   node tools/report/report20-shots.mjs --label vorher --url http://127.0.0.1:4180
//
// Output (gitignored): reports/tmp/report20/<label>/*.png and shots.json (region sizes, overflow
// of the bars: scrollWidth/clientWidth).
import { chromium } from "playwright";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const argv = process.argv.slice(2);
const opt = (n, d) => (argv.includes(n) ? argv[argv.indexOf(n) + 1] : d);
const LABEL = opt("--label", "nachher");
const URL = opt("--url", "http://127.0.0.1:4180");
const OUT = join(here, "../../reports/tmp/report20", LABEL);
mkdirSync(OUT, { recursive: true });

const browser = await chromium.launch({ args: ["--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--ignore-gpu-blocklist"] });
const info = {};

async function ready(page) {
  await page.waitForFunction(() => {
    const c = window.__cad;
    if (!c) return false;
    const s = c.state();
    return s.view && s.shownComplete && !s.pendingSend && s.shownVersion === s.sentVersion && s.view.missingMeshes === 0;
  }, null, { timeout: 90_000, polling: 100 });
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
  await page.waitForTimeout(250);
}

async function open(page, ex) {
  await page.evaluate((p) => window.__cad.openExample(p), ex);
  await page.waitForFunction((p) => window.__cad.state().fileName === p.split("/").pop(), ex);
  await ready(page);
}

/** The example project (several files, parts): old UI per select, new UI per menu. */
async function openProject(page) {
  const sel = page.locator("select[data-testid=examples]");
  if (await sel.count()) await sel.selectOption("projekt:projekt-gehaeuse");
  else {
    // the new UI follows the browser language (English in Playwright); German names for older builds
    await page.getByRole("button", { name: /^(Examples|Beispiele)$/ }).click();
    await page.getByRole("menuitem", { name: /^(Projects \(several files\)|Projekte \(mehrere Dateien\))$/ }).click();
    await page.getByRole("menuitem", { name: /projekt-gehaeuse/ }).click();
  }
  await page.waitForFunction(() => window.__cad.state().fileName === "gehaeuse.scad");
  await ready(page);
}

async function shot(page, name, selector) {
  const file = join(OUT, `${name}.png`);
  if (!selector) {
    await page.screenshot({ path: file });
    return;
  }
  const loc = page.locator(selector).first();
  if (!(await loc.count())) {
    console.log(`  (${name}: ${selector} not present)`);
    return;
  }
  await loc.screenshot({ path: file });
}

/** Width check of the bars: content wider than the box = cut off or scrolled. */
async function overflow(page) {
  return page.evaluate(() =>
    [...document.querySelectorAll("header, .toolbar, .filebar, .viewtools, .vtoolbar, .contextbar, .sectionbar, .sketchbar, .measurebar, .statusbar")].map((e) => ({
      cls: e.className,
      scroll: e.scrollWidth,
      client: e.clientWidth,
      height: e.clientHeight,
    })),
  );
}

const HEADER = "header";
const VIEW = "section.viewcol";
const EDITOR = "section.editorpane";

for (const theme of ["light", "dark"]) {
  for (const width of [1280, 1440, 1920]) {
    const page = await browser.newPage({ viewport: { width, height: 900 }, deviceScaleFactor: 1 });
    await page.goto(URL);
    await page.evaluate((t) => window.__cad.setThemePref(t), theme);
    await ready(page);
    await open(page, "spec/mehrfarbig.scad");
    const t = theme === "light" ? "hell" : "dunkel";
    await shot(page, `kopf-${width}-${t}`, HEADER);
    info[`${width}-${t}`] = await overflow(page);
    if (width !== 1440) {
      if (width === 1920) await shot(page, `gesamt-${width}-${t}`);
      await page.close();
      continue;
    }
    await shot(page, `gesamt-${t}`);
    await shot(page, `ansicht-modell-${t}`, VIEW);
    // colours (old: panel on its own; new: tab of the side panel)
    await page.evaluate(() => {
      const st = window.__cad.store.getState();
      if ("sideTab" in st) window.__cad.store.setState({ sideOpen: true, sideTab: "colors" });
      else window.__cad.store.setState({ colorsOpen: true });
    });
    await page.waitForTimeout(400);
    await shot(page, `farben-${t}`, "[data-testid=colors-panel]");
    // section
    await page.evaluate(() => {
      const b = window.__cad.rootBounds();
      const z = b ? Math.round(((b.min[2] + b.max[2]) / 2) * 10) / 10 : 5;
      window.__cad.setSection({ on: true, axis: "z", point: [0, 0, z], normal: [0, 0, 1], flip: false });
    });
    await page.waitForTimeout(600);
    await shot(page, `ansicht-schnitt-${t}`, VIEW);
    await page.evaluate(() => window.__cad.setSection({ on: false }));
    // measure
    await page.evaluate(() => window.__cad.measureEnter("distance"));
    await page.waitForTimeout(500);
    await shot(page, `ansicht-messen-${t}`, VIEW);
    await page.evaluate(() => window.__cad.measureExit());
    // sketch
    await open(page, "spec/skizze_deckel.scad");
    await page.evaluate(() => window.__cad.setCursorLine(14));
    await page.waitForTimeout(300);
    await page.evaluate(() => window.__cad.sketchEnter("line"));
    await page.waitForFunction(() => window.__cad.sketch().answer !== null || window.__cad.sketch().answers > 0, null, { timeout: 20_000 }).catch(() => {});
    await page.waitForTimeout(800);
    await shot(page, `ansicht-skizze-${t}`, VIEW);
    info[`skizze-${t}`] = await overflow(page);
    await page.evaluate(() => window.__cad.sketchExit());
    // project with several files and parts
    await openProject(page);
    await shot(page, `editor-${t}`, EDITOR);
    await shot(page, `projekt-${t}`);
    await page.evaluate(() => {
      const st = window.__cad.store.getState();
      if ("sideTab" in st) window.__cad.store.setState({ sideOpen: true, sideTab: "parts" });
    });
    await page.waitForTimeout(300);
    await shot(page, `bauteile-${t}`, "[data-testid=partsbar]");
    await page.close();
  }
}
writeFileSync(join(OUT, "shots.json"), JSON.stringify(info, null, 2));
await browser.close();
console.log(`${OUT}: done`);
