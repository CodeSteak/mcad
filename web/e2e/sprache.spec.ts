// UI language: "System" follows the browser language (en-US → English, de-DE → German), the
// choice in the settings switches at once, is kept in localStorage and sets <html lang>.
// (The other specs run with locale de-DE, see playwright.config.ts.)

import { type Page, expect, test } from "@playwright/test";

async function open(page: Page): Promise<void> {
  await page.goto("/");
  await expect(page.getByTestId("editor").locator(".cm-content")).toBeVisible();
}

test.describe("englischer Browser", () => {
  test.use({ locale: "en-US" });

  test("L01 System → Englisch, Umschalten in den Einstellungen, bleibt nach Neuladen", async ({ page }) => {
    await open(page);
    const html = page.locator("html");
    await expect(html).toHaveAttribute("lang", "en");
    const save = page.getByRole("button", { name: "Save", exact: true });
    await expect(save).toBeVisible();
    await expect(save).toHaveAttribute("title", /Ctrl\+S/);
    await expect(page.getByRole("radio", { name: "Model", exact: true })).toBeVisible();
    await expect(page.getByTestId("status-errors")).toHaveText(/^\d+ errors? · \d+ warnings?$/);
    await expect(page.getByRole("textbox", { name: "Source code" })).toBeVisible();
    expect(await page.title()).toBe("mcad – parametric CAD");

    // a menu with a shortcut in English notation
    await page.getByRole("button", { name: "File", exact: true }).click();
    const openItem = page.getByRole("menuitem", { name: "Open …", exact: true });
    await expect(openItem).toBeVisible();
    await expect(openItem).toContainText("Ctrl+O");
    await page.keyboard.press("Escape");

    // settings: English → Deutsch
    await page.getByRole("button", { name: "Settings", exact: true }).click();
    const lang = page.getByRole("dialog", { name: "Settings" }).getByLabel("Language");
    await expect(lang).toHaveValue("system");
    await lang.selectOption("de");
    await expect(html).toHaveAttribute("lang", "de");
    await expect(page.getByRole("button", { name: "Speichern", exact: true })).toHaveAttribute("title", /Strg\+S/);
    await expect(page.getByRole("radio", { name: "Modell", exact: true })).toBeVisible();
    await expect(page.getByRole("dialog", { name: "Einstellungen" }).getByLabel("Sprache")).toHaveValue("de");
    await expect(page.getByTestId("status-errors")).toHaveText(/^\d+ Fehler · \d+ Warnung(en)?$/);
    await expect(page.getByRole("textbox", { name: "Quelltext" })).toBeVisible();
    expect(await page.evaluate(() => localStorage.getItem("cad-lang"))).toBe("de");

    // kept after a reload
    await open(page);
    await expect(html).toHaveAttribute("lang", "de");
    await expect(page.getByRole("button", { name: "Speichern", exact: true })).toBeVisible();

    // back to "System": English again
    await page.getByRole("button", { name: "Einstellungen", exact: true }).click();
    await page.getByRole("dialog", { name: "Einstellungen" }).getByLabel("Sprache").selectOption("system");
    await expect(html).toHaveAttribute("lang", "en");
    await expect(page.getByRole("button", { name: "Save", exact: true })).toBeVisible();
    await expect(page.getByRole("dialog", { name: "Settings" }).getByLabel("Language")).toHaveValue("system");
  });
});

test.describe("deutscher Browser", () => {
  test.use({ locale: "de-DE" });

  test("L02 System → Deutsch, feste Wahl English", async ({ page }) => {
    await open(page);
    await expect(page.locator("html")).toHaveAttribute("lang", "de");
    await expect(page.getByRole("button", { name: "Speichern", exact: true })).toBeVisible();
    await page.getByRole("button", { name: "Einstellungen", exact: true }).click();
    await page.getByRole("dialog", { name: "Einstellungen" }).getByLabel("Sprache").selectOption("en");
    await expect(page.locator("html")).toHaveAttribute("lang", "en");
    await expect(page.getByRole("button", { name: "Save", exact: true })).toBeVisible();
    await expect(page.getByRole("tab", { name: "Parts", exact: true })).toBeVisible();
  });
});
