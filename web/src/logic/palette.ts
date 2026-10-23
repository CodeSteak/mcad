// Generated from crates/cad-eval/src/palette.rs. Do not edit.
// Regenerate: UPDATE_PALETTE_TS=1 cargo test -p cad-eval --test palette

export interface PaletteColor {
  name: string;
  /** Base value (exports). */
  hex: string;
  /** Shown in the light theme. */
  light: string;
  /** Shown in the dark theme. */
  dark: string;
}

export const PALETTE: PaletteColor[] = [
  { name: "red", hex: "#c74240", light: "#c74240", dark: "#c74240" },
  { name: "orange", hex: "#dc813b", light: "#dc813b", dark: "#dc813b" },
  { name: "yellow", hex: "#f7d974", light: "#f7d974", dark: "#f7d974" },
  { name: "lime", hex: "#91ca55", light: "#91ca55", dark: "#91ca55" },
  { name: "green", hex: "#4ca167", light: "#4ca167", dark: "#4ca167" },
  { name: "teal", hex: "#217274", light: "#217274", dark: "#307e80" },
  { name: "cyan", hex: "#6eccec", light: "#6eccec", dark: "#6eccec" },
  { name: "blue", hex: "#2e65b9", light: "#2e65b9", dark: "#3a71c6" },
  { name: "purple", hex: "#693d8c", light: "#693d8c", dark: "#754999" },
  { name: "magenta", hex: "#cd67b7", light: "#cd67b7", dark: "#cd67b7" },
  { name: "pink", hex: "#f7b8c6", light: "#f7b8c6", dark: "#f7b8c6" },
  { name: "brown", hex: "#6e4223", light: "#6e4223", dark: "#7a4d2e" },
  { name: "black", hex: "#202225", light: "#202225", dark: "#3b3d41" },
  { name: "gray", hex: "#7b7d81", light: "#7b7d81", dark: "#7b7d81" },
  { name: "silver", hex: "#abaeb2", light: "#abaeb2", dark: "#abaeb2" },
  { name: "white", hex: "#f4f3f0", light: "#e9e8e5", dark: "#f4f3f0" },
];

/** CSS spellings of palette colours. */
export const ALIASES: Record<string, string> = {
  grey: "gray",
  aqua: "cyan",
  fuchsia: "magenta",
};

/** Colour of material without `color()` (exports, colour list). */
export const DEFAULT_COLOR = "#c3c8cf";
