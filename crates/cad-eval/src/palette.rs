//! Colour palette of `color("name")` (Plan 16.1). One source of truth: the UI and the CLI
//! renderer use the same table (`web/src/logic/palette.ts` is generated from it, see
//! [`palette_ts`]).
//!
//! * 16 muted colours with English names (case does not matter), chosen so that they stay
//!   apart in OKLab also for deuteranopia and protanopia (Machado et al. 2009) and work on
//!   light and dark backgrounds; `grey`, `aqua` and `fuchsia` are CSS spellings of `gray`,
//!   `cyan` and `magenta`.
//! * Unknown names give a warning and a **stable** random colour: the hue comes from a hash
//!   of the name, lightness and chroma are the palette's medians (it fits in).
//! * Hex (`#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`) and vectors stay exact.
//! * [`display_rgb`]: what the 3D views show for a palette colour in the light or dark
//!   theme (black a bit lighter on dark backgrounds, white a bit darker on light ones).
//!   Exports always use the base values.

/// A palette colour: base value (exports, STEP, 3MF) and display values per theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaletteColor {
    pub name: &'static str,
    pub rgb: [u8; 3],
}

const fn c(name: &'static str, hex: u32) -> PaletteColor {
    PaletteColor { name, rgb: [(hex >> 16) as u8, (hex >> 8) as u8, hex as u8] }
}

/// The palette in display order (hue circle, then neutrals). Values from OKLCH targets
/// (`tools/palette/palette.py` prints the checks of the report).
pub const PALETTE: [PaletteColor; 16] = [
    c("red", 0xc74240),
    c("orange", 0xdc813b),
    c("yellow", 0xf7d974),
    c("lime", 0x91ca55),
    c("green", 0x4ca167),
    c("teal", 0x217274),
    c("cyan", 0x6eccec),
    c("blue", 0x2e65b9),
    c("purple", 0x693d8c),
    c("magenta", 0xcd67b7),
    c("pink", 0xf7b8c6),
    c("brown", 0x6e4223),
    c("black", 0x202225),
    c("gray", 0x7b7d81),
    c("silver", 0xabaeb2),
    c("white", 0xf4f3f0),
];

/// CSS spellings of palette colours.
pub const ALIASES: [(&str, &str); 3] = [("grey", "gray"), ("aqua", "cyan"), ("fuchsia", "magenta")];

/// Colour of material without `color()` in exports and the colour list ("Standard").
pub const DEFAULT_RGB: [u8; 3] = [0xc3, 0xc8, 0xcf];

/// Highest filament slot (`color(…, filament = n)`): Bambu Studio and OrcaSlicer handle 16
/// filaments with four AMS units.
pub const MAX_FILAMENT: u32 = 16;

/// OKLCH lightness and chroma of random colours: medians of the 12 chromatic palette colours.
pub const RANDOM_L: f64 = 0.65;
pub const RANDOM_C: f64 = 0.125;

/// How a colour was given.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorKind {
    /// Palette name (or alias).
    Palette,
    /// Unknown name: random colour from the name's hash.
    Random,
    /// `#…` or `[r, g, b(, a)]`: exact.
    Exact,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedColor {
    pub rgba: [f32; 4],
    pub kind: ColorKind,
    /// Palette name (canonical, lowercase) or the unknown name as written.
    pub name: Option<String>,
}

/// The palette colour of a name (case-insensitive, aliases allowed).
pub fn palette_color(name: &str) -> Option<&'static PaletteColor> {
    let lower = name.trim().to_ascii_lowercase();
    let lower = ALIASES.iter().find(|(a, _)| *a == lower).map_or(lower.as_str(), |(_, n)| n).to_string();
    PALETTE.iter().find(|p| p.name == lower)
}

/// Parses `color("…")`: palette name, `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`, `transparent`
/// or any other name (random colour, [`ColorKind::Random`]). `None` for malformed hex.
pub fn parse_color(s: &str) -> Option<ParsedColor> {
    let t = s.trim();
    if let Some(hex) = t.strip_prefix('#') {
        return parse_hex(hex).map(|rgba| ParsedColor { rgba, kind: ColorKind::Exact, name: None });
    }
    if t.eq_ignore_ascii_case("transparent") {
        return Some(ParsedColor { rgba: [0.0; 4], kind: ColorKind::Exact, name: Some("transparent".into()) });
    }
    if let Some(p) = palette_color(t) {
        return Some(ParsedColor { rgba: rgba_of(p.rgb), kind: ColorKind::Palette, name: Some(p.name.into()) });
    }
    Some(ParsedColor { rgba: random_color(t), kind: ColorKind::Random, name: Some(t.to_string()) })
}

/// `[r, g, b]` bytes → RGBA 0..1 (alpha 1).
pub fn rgba_of(rgb: [u8; 3]) -> [f32; 4] {
    [rgb[0] as f32 / 255.0, rgb[1] as f32 / 255.0, rgb[2] as f32 / 255.0, 1.0]
}

/// RGBA 0..1 → bytes (rounded, clamped).
pub fn rgba_bytes(c: [f32; 4]) -> [u8; 4] {
    c.map(|x| (x.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// `#rrggbb` or `#rrggbbaa` (alpha < 1).
pub fn hex_string(c: [f32; 4]) -> String {
    let b = rgba_bytes(c);
    if b[3] == 255 {
        format!("#{:02x}{:02x}{:02x}", b[0], b[1], b[2])
    } else {
        format!("#{:02x}{:02x}{:02x}{:02x}", b[0], b[1], b[2], b[3])
    }
}

/// The palette name of an exact palette value (`None` for other colours).
pub fn palette_name(c: [f32; 4]) -> Option<&'static str> {
    let b = rgba_bytes(c);
    PALETTE.iter().find(|p| p.rgb == [b[0], b[1], b[2]]).map(|p| p.name)
}

fn parse_hex(h: &str) -> Option<[f32; 4]> {
    let digits: Vec<u8> = h.chars().map(|c| c.to_digit(16).map(|d| d as u8)).collect::<Option<_>>()?;
    let comp = |hi: u8, lo: u8| (hi * 16 + lo) as f32 / 255.0;
    match digits.len() {
        3 | 4 => {
            let mut c = [1.0f32; 4];
            for (i, d) in digits.iter().enumerate() {
                c[i] = comp(*d, *d);
            }
            Some(c)
        }
        6 | 8 => {
            let mut c = [1.0f32; 4];
            for i in 0..digits.len() / 2 {
                c[i] = comp(digits[2 * i], digits[2 * i + 1]);
            }
            Some(c)
        }
        _ => None,
    }
}

/// FNV-1a 64 of the lowercase name (stable across platforms and versions).
fn name_hash(name: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in name.trim().to_ascii_lowercase().bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// Stable random colour of an unknown name: hue from the name's hash, lightness and chroma
/// of the palette ([`RANDOM_L`], [`RANDOM_C`], chroma reduced until it is in sRGB gamut).
pub fn random_color(name: &str) -> [f32; 4] {
    let h = (name_hash(name) % 3600) as f64 / 10.0;
    let mut cc = RANDOM_C;
    loop {
        let lin = oklch_to_linear(RANDOM_L, cc, h);
        if lin.iter().all(|x| (-1e-6..=1.0 + 1e-6).contains(x)) || cc <= 0.0 {
            let b = linear_to_bytes(lin);
            return rgba_of(b);
        }
        cc -= 0.005;
    }
}

// ---------------------------------------------------------------- colour science

/// sRGB byte → linear.
pub fn srgb_to_linear(c: u8) -> f64 {
    let c = c as f64 / 255.0;
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn linear_to_srgb(c: f64) -> f64 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.003_130_8 { 12.92 * c } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}

fn linear_to_bytes(l: [f64; 3]) -> [u8; 3] {
    l.map(|x| (linear_to_srgb(x) * 255.0).round() as u8)
}

/// Linear sRGB → OKLab (Björn Ottosson 2020).
pub fn linear_to_oklab(c: [f64; 3]) -> [f64; 3] {
    let [r, g, b] = c;
    let l = (0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b).cbrt();
    let m = (0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b).cbrt();
    let s = (0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b).cbrt();
    [
        0.210_454_255_3 * l + 0.793_617_785_0 * m - 0.004_072_046_8 * s,
        1.977_998_495_1 * l - 2.428_592_205_0 * m + 0.450_593_709_9 * s,
        0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766_0 * s,
    ]
}

fn oklab_to_linear(lab: [f64; 3]) -> [f64; 3] {
    let [ll, a, b] = lab;
    let l = (ll + 0.396_337_777_4 * a + 0.215_803_757_3 * b).powi(3);
    let m = (ll - 0.105_561_345_8 * a - 0.063_854_172_8 * b).powi(3);
    let s = (ll - 0.089_484_177_5 * a - 1.291_485_548_0 * b).powi(3);
    [
        4.076_741_662_1 * l - 3.307_711_591_3 * m + 0.230_969_929_2 * s,
        -1.268_438_004_6 * l + 2.609_757_401_1 * m - 0.341_319_396_5 * s,
        -0.004_196_086_3 * l - 0.703_418_614_7 * m + 1.707_614_701_0 * s,
    ]
}

fn oklch_to_linear(l: f64, c: f64, h_deg: f64) -> [f64; 3] {
    let h = h_deg.to_radians();
    oklab_to_linear([l, c * h.cos(), c * h.sin()])
}

/// OKLab of sRGB bytes.
pub fn oklab(rgb: [u8; 3]) -> [f64; 3] {
    linear_to_oklab(rgb.map(srgb_to_linear))
}

/// OKLCH (lightness, chroma, hue in degrees) of sRGB bytes.
pub fn oklch(rgb: [u8; 3]) -> [f64; 3] {
    let [l, a, b] = oklab(rgb);
    [l, a.hypot(b), b.atan2(a).to_degrees().rem_euclid(360.0)]
}

/// Euclidean distance in OKLab (ΔE_OK; about 0.02 is just noticeable).
pub fn delta_e(a: [u8; 3], b: [u8; 3]) -> f64 {
    let (x, y) = (oklab(a), oklab(b));
    ((x[0] - y[0]).powi(2) + (x[1] - y[1]).powi(2) + (x[2] - y[2]).powi(2)).sqrt()
}

/// Colour vision deficiency (dichromacy, severity 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cvd {
    Deuteranopia,
    Protanopia,
    Tritanopia,
}

/// Simulated colour for a viewer with `cvd` (Machado, Oliveira, Fernandes 2009, severity
/// 1.0; applied to linear sRGB).
pub fn simulate_cvd(rgb: [u8; 3], cvd: Cvd) -> [u8; 3] {
    let m: [[f64; 3]; 3] = match cvd {
        Cvd::Deuteranopia => {
            [[0.367_322, 0.860_646, -0.227_968], [0.280_085, 0.672_501, 0.047_413], [-0.011_820, 0.042_940, 0.968_881]]
        }
        Cvd::Protanopia => {
            [[0.152_286, 1.052_583, -0.204_868], [0.114_503, 0.786_281, 0.099_216], [-0.003_882, -0.048_116, 1.051_998]]
        }
        Cvd::Tritanopia => {
            [[1.255_528, -0.076_749, -0.178_779], [-0.078_411, 0.930_809, 0.147_602], [0.004_733, 0.691_367, 0.303_900]]
        }
    };
    let l = rgb.map(srgb_to_linear);
    let out = [0, 1, 2].map(|i| (m[i][0] * l[0] + m[i][1] * l[1] + m[i][2] * l[2]).clamp(0.0, 1.0));
    linear_to_bytes(out)
}

/// WCAG 2 relative luminance.
pub fn relative_luminance(rgb: [u8; 3]) -> f64 {
    let [r, g, b] = rgb.map(srgb_to_linear);
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// WCAG 2 contrast ratio (1..21).
pub fn contrast_ratio(a: [u8; 3], b: [u8; 3]) -> f64 {
    let (x, y) = (relative_luminance(a), relative_luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

/// Theme of a 3D view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Theme {
    #[default]
    Light,
    Dark,
}

/// What a view shows for colour `rgb` in `theme`: palette colours get a slightly adjusted
/// lightness (same hue and chroma) so that black stays visible on dark and white on light
/// backgrounds; other colours are shown as they are.
pub fn display_rgb(rgb: [u8; 3], theme: Theme) -> [u8; 3] {
    if !PALETTE.iter().any(|p| p.rgb == rgb) {
        return rgb;
    }
    let [l, c, h] = oklch(rgb);
    let l2 = match theme {
        Theme::Light if l > 0.93 => 0.93,
        Theme::Light => l,
        Theme::Dark if l < 0.36 => 0.36,
        Theme::Dark if l < 0.55 => l + 0.04,
        Theme::Dark => l,
    };
    if (l2 - l).abs() < 1e-9 {
        return rgb;
    }
    linear_to_bytes(oklch_to_linear(l2, c, h))
}

/// Text of `web/src/logic/palette.ts` (generated; `cargo test -p cad-eval --test palette`
/// checks it, `UPDATE_PALETTE_TS=1` rewrites it).
pub fn palette_ts() -> String {
    let hex = |b: [u8; 3]| format!("#{:02x}{:02x}{:02x}", b[0], b[1], b[2]);
    let mut s = String::from(
        "// Generated from crates/cad-eval/src/palette.rs. Do not edit.\n\
         // Regenerate: UPDATE_PALETTE_TS=1 cargo test -p cad-eval --test palette\n\n\
         export interface PaletteColor {\n  name: string;\n  /** Base value (exports). */\n  hex: string;\n  \
         /** Shown in the light theme. */\n  light: string;\n  /** Shown in the dark theme. */\n  dark: string;\n}\n\n\
         export const PALETTE: PaletteColor[] = [\n",
    );
    for p in &PALETTE {
        s.push_str(&format!(
            "  {{ name: \"{}\", hex: \"{}\", light: \"{}\", dark: \"{}\" }},\n",
            p.name,
            hex(p.rgb),
            hex(display_rgb(p.rgb, Theme::Light)),
            hex(display_rgb(p.rgb, Theme::Dark))
        ));
    }
    s.push_str("];\n\n/** CSS spellings of palette colours. */\nexport const ALIASES: Record<string, string> = {\n");
    for (a, n) in ALIASES {
        s.push_str(&format!("  {a}: \"{n}\",\n"));
    }
    s.push_str(&format!(
        "}};\n\n/** Colour of material without `color()` (exports, colour list). */\nexport const DEFAULT_COLOR = \"{}\";\n",
        hex(DEFAULT_RGB)
    ));
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_forms() {
        assert_eq!(parse_color("#f00").unwrap().rgba, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(parse_color("#ff000080").unwrap().rgba[3], 128.0 / 255.0);
        assert!(parse_color("#ff00").is_some());
        assert!(parse_color("#zz0000").is_none());
        assert!(parse_color("#12345").is_none());
    }

    #[test]
    fn names_are_case_insensitive() {
        let a = parse_color("Teal").unwrap();
        assert_eq!(a.kind, ColorKind::Palette);
        assert_eq!(a.rgba, parse_color("teal").unwrap().rgba);
        assert_eq!(parse_color("GREY").unwrap().rgba, parse_color("gray").unwrap().rgba);
        assert_eq!(parse_color("grey").unwrap().name.as_deref(), Some("gray"));
    }
}
