// UI language: dictionaries complete and consistent, placeholders, plural forms, the choice
// "System" from navigator.language.

import { afterEach, describe, expect, it } from "vitest";
import { DICTS, getLang, interpolate, lazyRecord, onLangChange, resolveLang, setLang, shortcutText, systemLang, t } from "./index";
import { en } from "./en";
import { de } from "./de";

const placeholders = (v: string | { one: string; other: string }) => {
  const texts = typeof v === "string" ? [v] : [v.one, v.other];
  return [...new Set(texts.flatMap((s) => [...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1])))].sort();
};

afterEach(() => setLang("de"));

describe("Wörterbücher", () => {
  it("de hat genau die Schlüssel von en", () => {
    expect(Object.keys(de).sort()).toEqual(Object.keys(en).sort());
  });
  it("gleiche Platzhalter und Pluralformen in beiden Sprachen, keine leeren Texte", () => {
    for (const k of Object.keys(en) as Array<keyof typeof en>) {
      const a = en[k];
      const b = de[k];
      expect(typeof b, k).toBe(typeof a);
      expect(placeholders(b), k).toEqual(placeholders(a));
      if (typeof a === "string" && a === "") continue; // sentence pieces may be empty in one language
      expect(typeof a === "string" ? a.trim() : a.one.trim(), k).not.toBe("");
    }
  });
});

describe("t", () => {
  it("setzt Parameter ein, unbekannte bleiben stehen", () => {
    setLang("en");
    expect(t("tab.closeConfirm", { path: "lib/a.mcad" })).toBe("Remove lib/a.mcad from the project?");
    expect(interpolate("{a} und {b}", { a: 1 })).toBe("1 und {b}");
    setLang("de");
    expect(t("tab.closeConfirm", { path: "lib/a.mcad" })).toBe("lib/a.mcad aus dem Projekt entfernen?");
  });
  it("Pluralformen nach n", () => {
    setLang("en");
    expect(t("status.warnings", { n: 1 })).toBe("1 warning");
    expect(t("status.warnings", { n: 0 })).toBe("0 warnings");
    setLang("de");
    expect(t("status.warnings", { n: 1 })).toBe("1 Warnung");
    expect(t("status.warnings", { n: 3 })).toBe("3 Warnungen");
  });
  it("Umschalten benachrichtigt und ändert Tabellen mit Gettern", () => {
    const names = lazyRecord(["save"] as const, () => t("header.save"));
    let calls = 0;
    const off = onLangChange(() => calls++);
    setLang("en");
    expect(getLang()).toBe("en");
    expect(names.save).toBe("Save");
    setLang("en");
    setLang("de");
    expect(names.save).toBe("Speichern");
    off();
    setLang("en");
    expect(calls).toBe(2);
  });
  it("Tastenkürzel in der Sprache der Oberfläche", () => {
    setLang("en");
    expect(shortcutText("Strg+⇧+S")).toBe("Ctrl+⇧+S");
    expect(shortcutText("Entf")).toBe("Del");
    setLang("de");
    expect(shortcutText("Strg+E")).toBe("Strg+E");
  });
});

describe("Systemsprache", () => {
  it("de… → Deutsch, sonst Englisch", () => {
    expect(systemLang("de-DE")).toBe("de");
    expect(systemLang("de")).toBe("de");
    expect(systemLang("DE-at")).toBe("de");
    expect(systemLang("en-US")).toBe("en");
    expect(systemLang("fr-FR")).toBe("en");
    expect(systemLang("")).toBe("en");
  });
  it("feste Wahl schlägt das System", () => {
    expect(resolveLang("system", "de-CH")).toBe("de");
    expect(resolveLang("system", "en-GB")).toBe("en");
    expect(resolveLang("en", "de-DE")).toBe("en");
    expect(resolveLang("de", "en-US")).toBe("de");
  });
  it("beide Sprachen sind registriert", () => {
    expect(Object.keys(DICTS).sort()).toEqual(["de", "en"]);
  });
});
