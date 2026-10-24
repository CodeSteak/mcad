// WCAG 2 contrast of the theme tokens (`styles.css`): text colours on the backgrounds they
// appear on (Plan 16.5: dark theme, AA for text). Used by `contrast.test.ts` and the report.

export type Rgb = [number, number, number];

/** `#rgb`, `#rrggbb`, `rgb(…)`/`rgba(…)` → RGB 0..255 and alpha. */
export function parseCss(c: string): { rgb: Rgb; a: number } | null {
  const s = c.trim().toLowerCase();
  let m = /^#([0-9a-f]{3})$/.exec(s);
  if (m) return { rgb: [...m[1]].map((h) => parseInt(h + h, 16)) as Rgb, a: 1 };
  m = /^#([0-9a-f]{6})([0-9a-f]{2})?$/.exec(s);
  if (m) return { rgb: [0, 2, 4].map((i) => parseInt(m![1].slice(i, i + 2), 16)) as Rgb, a: m[2] ? parseInt(m[2], 16) / 255 : 1 };
  m = /^rgba?\(([^)]+)\)$/.exec(s);
  if (m) {
    const p = m[1].split(/[ ,/]+/).filter(Boolean).map(Number);
    return { rgb: [p[0], p[1], p[2]], a: p.length > 3 ? p[3] : 1 };
  }
  return null;
}

/** `top` with alpha over the opaque `bottom`. */
export function over(top: { rgb: Rgb; a: number }, bottom: Rgb): Rgb {
  return [0, 1, 2].map((i) => top.rgb[i] * top.a + bottom[i] * (1 - top.a)) as Rgb;
}

function lin(c: number): number {
  const x = c / 255;
  return x <= 0.04045 ? x / 12.92 : ((x + 0.055) / 1.055) ** 2.4;
}

export function luminance(c: Rgb): number {
  return 0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2]);
}

export function contrast(a: Rgb, b: Rgb): number {
  const [x, y] = [luminance(a), luminance(b)];
  return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05);
}

/** Token values of one theme block of `styles.css` (`:root[data-theme="…"]`). */
export function themeTokens(css: string, theme: "light" | "dark"): Record<string, string> {
  const out: Record<string, string> = {};
  const re = new RegExp(`:root\\[data-theme="${theme}"\\][^{]*\\{([^}]*)\\}`, "g");
  let m: RegExpExecArray | null;
  while ((m = re.exec(css))) {
    for (const d of m[1].split(";")) {
      const i = d.indexOf(":");
      if (i < 0) continue;
      const k = d.slice(0, i).trim();
      if (k.startsWith("--")) out[k] = d.slice(i + 1).trim();
    }
  }
  return out;
}

/** Text token on background token (both resolved; translucent backgrounds over `base`). */
export interface Pair {
  text: string;
  bg: string;
  /** Background under a translucent `bg`. */
  base?: string;
  /** Minimum ratio: 4.5 (AA text), 3 (AA large text, UI components). */
  min: number;
}

/** The pairs the UI uses: editor, panels, status bar, inline hints, syntax colours. */
export const PAIRS: Pair[] = [
  ...["--text", "--muted"].flatMap((t) => ["--bg", "--panel", "--panel-2"].map((bg) => ({ text: t, bg, min: 4.5 }))),
  { text: "--accent", bg: "--panel", min: 4.5 },
  { text: "--accent", bg: "--panel-2", min: 4.5 },
  { text: "--error", bg: "--panel", min: 4.5 },
  { text: "--warning", bg: "--panel", min: 4.5 },
  { text: "--error", bg: "--error-bg", base: "--panel", min: 4.5 },
  { text: "--warning", bg: "--warning-bg", base: "--panel", min: 4.5 },
  { text: "--text", bg: "--error-bg", base: "--panel", min: 4.5 },
  { text: "--text", bg: "--warning-bg", base: "--panel", min: 4.5 },
  { text: "--text", bg: "--accent-soft", base: "--panel", min: 4.5 },
  { text: "--text", bg: "--viewhover-line", base: "--panel", min: 4.5 },
  ...[
    "--hl-keyword",
    "--hl-number",
    "--hl-string",
    "--hl-comment",
    "--hl-special",
    "--hl-def",
    "--hl-call",
    "--hl-label",
    "--hl-plane",
    "--hl-prop",
    "--hl-modifier",
    "--hl-operator",
  ].map((t) => ({ text: t, bg: "--panel", min: 4.5 })),
  { text: "--hl-comment", bg: "--active-line", base: "--panel", min: 4.5 },
  { text: "--on-accent", bg: "--accent", min: 4.5 },
  { text: "--on-snap", bg: "--view-snap", min: 4.5 },
  { text: "--on-measure", bg: "--view-measure", min: 4.5 },
  { text: "--view-measure", bg: "--view-bg", min: 3 },
  { text: "--view-measure", bg: "--view-model", min: 2 },
  { text: "--text", bg: "--diff-add", base: "--panel", min: 4.5 },
  { text: "--text", bg: "--diff-del", base: "--panel", min: 4.5 },
  { text: "--git-added", bg: "--panel", min: 3 },
  { text: "--git-modified", bg: "--panel", min: 3 },
  { text: "--git-deleted", bg: "--panel", min: 3 },
  { text: "--view-edge", bg: "--view-model", min: 3 },
  { text: "--view-accent", bg: "--view-bg", min: 3 },
  { text: "--view-grid", bg: "--view-bg", min: 3 },
  { text: "--view-anchor", bg: "--view-bg", min: 2 },
  { text: "--view-selected-edge", bg: "--view-model", min: 2 },
  // UI overhaul (report 20): teal accent, buttons, segments, menus, focus ring, view cube
  { text: "--accent", bg: "--bg", min: 4.5 },
  { text: "--accent-text", bg: "--accent-soft", base: "--panel", min: 4.5 },
  { text: "--accent-text", bg: "--accent-soft", base: "--panel-2", min: 4.5 },
  { text: "--text", bg: "--accent-soft", base: "--panel-2", min: 4.5 },
  { text: "--on-accent", bg: "--accent-hover", min: 4.5 },
  { text: "--text", bg: "--seg-on", min: 4.5 },
  { text: "--text", bg: "--ctl-hover", base: "--panel", min: 4.5 },
  { text: "--focus", bg: "--bg", min: 3 },
  { text: "--focus", bg: "--panel", min: 3 },
  { text: "--focus", bg: "--panel-2", min: 3 },
  { text: "--focus", bg: "--view-bg", min: 3 },
  { text: "--view-accent", bg: "--view-model", min: 2 },
  { text: "--text", bg: "--vc-face", min: 4.5 },
  { text: "--axis-x", bg: "--view-bg", min: 3 },
  { text: "--axis-y", bg: "--view-bg", min: 3 },
  { text: "--axis-z", bg: "--view-bg", min: 3 },
];

export interface PairResult extends Pair {
  ratio: number;
  ok: boolean;
}

/** Contrast of every pair in a theme (`null` ratio if a token is missing). */
export function checkTheme(tokens: Record<string, string>): PairResult[] {
  return PAIRS.map((p) => {
    const t = parseCss(tokens[p.text] ?? "");
    const b = parseCss(tokens[p.bg] ?? "");
    const base = p.base ? parseCss(tokens[p.base] ?? "") : null;
    if (!t || !b || (p.base && !base)) return { ...p, ratio: 0, ok: false };
    const bg = b.a < 1 ? over(b, base ? base.rgb : [255, 255, 255]) : b.rgb;
    const fg = t.a < 1 ? over(t, bg) : t.rgb;
    const ratio = contrast(fg, bg);
    return { ...p, ratio, ok: ratio >= p.min };
  });
}
