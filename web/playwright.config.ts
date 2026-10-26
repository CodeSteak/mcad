// End-to-end tests in a real Chromium against the production build (vite build + preview),
// built with VITE_TEST_MODELS=1 so the example menu lists the test models of tests/models/.
// Browsers come from PLAYWRIGHT_BROWSERS_PATH (preinstalled locally; in CI
// `playwright install --with-deps chromium`, see .github/workflows/web.yml).
import { defineConfig, devices, type ReporterDescription } from "@playwright/test";

const reporter: ReporterDescription[] = [["list"], ["json", { outputFile: "../reports/tmp/report2/playwright.json" }]];
// CI (GitHub Actions sets CI): additionally an HTML report in playwright-report/ (artefact on failure).
if (process.env.CI) reporter.push(["html", { open: "never", outputFolder: "playwright-report" }]);

export default defineConfig({
  testDir: "e2e",
  outputDir: "test-results",
  timeout: 90_000,
  expect: { timeout: 20_000 },
  fullyParallel: false,
  workers: 1,
  retries: 0,
  reporter,
  use: {
    baseURL: "http://127.0.0.1:4173",
    viewport: { width: 1440, height: 900 },
    deviceScaleFactor: 1,
    acceptDownloads: true,
    // The specs select by the German names of the UI (language "System" follows the browser);
    // language.spec.ts checks English.
    locale: "de-DE",
    trace: "retain-on-failure",
    launchOptions: { args: ["--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--ignore-gpu-blocklist"] },
  },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"], locale: "de-DE", viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1 } }],
  webServer: {
    command: "VITE_TEST_MODELS=1 pnpm exec vite build && pnpm exec vite preview --port 4173 --strictPort --host 127.0.0.1",
    url: "http://127.0.0.1:4173",
    reuseExistingServer: false,
    timeout: 120_000,
  },
});
