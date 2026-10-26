// UI language (English, German): typed dictionaries (`en.ts` is the reference, `de.ts` must have
// every key), `t(key, params)` with `{name}` placeholders and plural forms, and `useLang()` for
// React (the app root subscribes, so a switch re-renders the whole tree). The choice ("System",
// English, Deutsch) is kept in localStorage like the theme (state/store.ts: `setLangPref`).
// Texts that come from the engine (diagnostics, measurement texts, prompts) are not translated.

import { useSyncExternalStore } from "react";
import { de } from "./de.ts";
import { en, type MessageKey, type Messages } from "./en.ts";

export type { MessageKey, Messages } from "./en.ts";

export type Lang = "en" | "de";
/** Choice in the settings: follow the system (`navigator.language`) or fixed. */
export type LangPref = "system" | Lang;

export const LANG_PREFS: readonly LangPref[] = ["system", "en", "de"];

export const DICTS: Record<Lang, Messages> = { en, de };

const STORAGE_KEY = "cad-lang";

/** Language of the system: `de…` → German, everything else English. */
export function systemLang(language?: string | null): Lang {
  const l = language ?? (typeof navigator !== "undefined" ? navigator.language : "");
  return (l ?? "").toLowerCase().startsWith("de") ? "de" : "en";
}

export function resolveLang(pref: LangPref, language?: string | null): Lang {
  return pref === "system" ? systemLang(language) : pref;
}

/** The stored choice (default "System"). */
export function loadLangPref(): LangPref {
  try {
    const v = localStorage.getItem(STORAGE_KEY);
    if (v === "system" || v === "en" || v === "de") return v;
  } catch {
    /* no storage */
  }
  return "system";
}

export function saveLangPref(pref: LangPref): void {
  try {
    localStorage.setItem(STORAGE_KEY, pref);
  } catch {
    /* no storage */
  }
}

let current: Lang = resolveLang(loadLangPref());
const listeners = new Set<() => void>();

function applyDocumentLang(): void {
  if (typeof document !== "undefined") document.documentElement.lang = current;
}
applyDocumentLang();

export function getLang(): Lang {
  return current;
}

/** Switches the language (`<html lang>`, listeners, re-render via `useLang`). */
export function setLang(lang: Lang): void {
  if (lang === current) return;
  current = lang;
  applyDocumentLang();
  for (const f of [...listeners]) f();
}

/** Called after every switch; returns the unsubscribe function. */
export function onLangChange(f: () => void): () => void {
  listeners.add(f);
  return () => listeners.delete(f);
}

/** The current language; the component re-renders on a switch. */
export function useLang(): Lang {
  return useSyncExternalStore(onLangChange, getLang, getLang);
}

export type Params = Record<string, string | number>;

/** Replaces `{name}` by `params.name` (unknown names stay as they are). */
export function interpolate(text: string, params?: Params): string {
  if (!params) return text;
  return text.replace(/\{(\w+)\}/g, (all, k: string) => (k in params ? String(params[k]) : all));
}

/** Text of `key` in the current language (plural entries choose by `params.n`). */
export function t(key: MessageKey, params?: Params): string {
  const m = DICTS[current][key] ?? en[key];
  const text = typeof m === "string" ? m : Number(params?.n) === 1 ? m.one : m.other;
  return interpolate(text, params);
}

/**
 * Shortcut as shown in tooltips and menus. Shortcuts are written in the German notation in the
 * code ("Strg+S", "Umschalt", "Entf"); English shows "Ctrl+S", "Shift", "Del".
 */
export function shortcutText(shortcut: string): string {
  if (current === "de") return shortcut;
  return shortcut.replace(/Strg/g, "Ctrl").replace(/Umschalt/g, "Shift").replace(/Entf/g, "Del");
}

/** A record whose values are looked up on every access (module-level tables of UI texts). */
export function lazyRecord<K extends string>(keys: readonly K[], text: (k: K) => string): Record<K, string> {
  const out = {} as Record<K, string>;
  for (const k of keys) Object.defineProperty(out, k, { get: () => text(k), enumerable: true });
  return out;
}
