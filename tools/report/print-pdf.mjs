// Prints an HTML report to PDF (A4, backgrounds on) with Playwright's Chromium.
//
//   cd tools/report && npm i playwright@1.56.1 --no-save   (browsers: PLAYWRIGHT_BROWSERS_PATH)
//   node tools/report/print-pdf.mjs <report.html> <report.pdf> [footer title]
import { chromium } from "playwright";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { statSync } from "node:fs";

const [src, dst, title = "Test report 1: kernel, language, engine, CLI"] = process.argv.slice(2);
const esc = (t) => t.replace(/&/g, "&amp;").replace(/</g, "&lt;");
if (!src || !dst) {
  console.error("usage: node print-pdf.mjs <report.html> <report.pdf>");
  process.exit(2);
}
const browser = await chromium.launch();
try {
  const page = await browser.newPage();
  await page.goto(pathToFileURL(resolve(src)).href, { waitUntil: "load" });
  await page.evaluate(() => Promise.all([...document.images].map((i) => (i.complete ? null : new Promise((r) => (i.onload = i.onerror = r))))));
  await page.pdf({
    path: resolve(dst),
    format: "A4",
    printBackground: true,
    preferCSSPageSize: true,
    displayHeaderFooter: true,
    headerTemplate: "<span></span>",
    footerTemplate:
      "<div style='font-size:7.5pt;color:#5b6270;width:100%;padding:0 13mm;display:flex;justify-content:space-between;'>" +
      `<span>${esc(title)}</span>` + "<span>Page <span class='pageNumber'></span> / <span class='totalPages'></span></span></div>",
  });
  console.log(`${dst}: ${statSync(resolve(dst)).size} B, Chromium ${browser.version()}`);
} finally {
  await browser.close();
}
