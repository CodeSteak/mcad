//! Text outlines (Plan 17.4): bundled free fonts (SIL OFL), font selection like OpenSCAD
//! (`"Inter:style=Bold"`), shaping with `rustybuzz` (kerning, ligatures, right-to-left and
//! vertical text) and exact glyph contours (TrueType: quadratic, CFF: cubic Bézier curves).
//!
//! Sizes follow OpenSCAD: `size` is 72 % of the em square (OpenSCAD 2021.01 renders the em at
//! `size / 0.72`, so the cap height of most fonts is about `size`); glyph positions and the
//! alignment follow OpenSCAD too, including its 1000/1024 scale ([`OPENSCAD_POSITION_SCALE`]).
//!
//! **Default font:** `"cad stencil mono"` ([`stencil`]), an own monospaced stencil font of lines
//! and arcs with bridges (no holes), used without `font` and for unknown families. Characters a
//! font lacks come from JetBrains Mono ([`fallback_font`]).

use cad_outline::{Contour, Seg};
use glam::DVec2;
use rustybuzz::ttf_parser;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

pub mod stencil;

/// The own stencil font (Plan 17.4, [`stencil`]).
pub const STENCIL_FAMILY: &str = "cad stencil mono";
/// Family used for `font` not given and for unknown families.
pub const DEFAULT_FAMILY: &str = STENCIL_FAMILY;
/// Family of the fallback font for characters a font lacks.
pub const FALLBACK_FAMILY: &str = "JetBrains Mono";

/// em size in mm = `size / SIZE_TO_EM` (OpenSCAD: char size `size` at 100 dpi in 72 dpi points).
pub const SIZE_TO_EM: f64 = 0.72;

/// OpenSCAD (2021.01 and later) places glyphs with the shaper's advances and offsets scaled by
/// 1000/1024 relative to the outlines (a units mix-up in its FreeType renderer: 2.3 % tighter
/// than the font's own spacing), and aligns with the glyph extents scaled the same way. Text
/// here matches it, so that models keep their text width.
pub const OPENSCAD_POSITION_SCALE: f64 = 1000.0 / 1024.0;

/// A bundled font file.
#[derive(Debug)]
pub struct Builtin {
    pub family: &'static str,
    pub style: &'static str,
    pub file: &'static str,
    pub data: &'static [u8],
}

macro_rules! font {
    ($family:expr, $style:expr, $file:expr) => {
        Builtin { family: $family, style: $style, file: $file, data: include_bytes!(concat!("../fonts/", $file)) }
    };
}

/// The bundled fonts (sources and licences: `fonts/README.md`).
pub static BUILTIN: &[Builtin] = &[
    font!("JetBrains Mono", "Regular", "JetBrainsMono-Regular.ttf"),
    font!("JetBrains Mono", "Bold", "JetBrainsMono-Bold.ttf"),
    font!("Inter", "Regular", "Inter-Regular.ttf"),
    font!("Inter", "Bold", "Inter-Bold.ttf"),
    font!("Allerta Stencil", "Regular", "AllertaStencil-Regular.ttf"),
];

/// A font given as `"Family"`, `"Family:style=Bold"` (fontconfig pattern as in OpenSCAD) or a
/// file name ending in `.ttf`/`.otf`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FontName {
    pub family: String,
    pub style: Option<String>,
}

impl FontName {
    pub fn parse(s: &str) -> FontName {
        let mut parts = s.split(':');
        let family = parts.next().unwrap_or("").trim().to_string();
        let mut style = None;
        for p in parts {
            if let Some((k, v)) = p.split_once('=')
                && k.trim().eq_ignore_ascii_case("style")
            {
                style = Some(v.trim().to_string());
            }
        }
        FontName { family, style }
    }

    /// The name is a font file (`font = "schrift.ttf"`).
    pub fn is_file(s: &str) -> bool {
        let l = s.trim().to_ascii_lowercase();
        let base = l.split(':').next().unwrap_or("");
        [".ttf", ".otf", ".ttc", ".otc"].iter().any(|e| base.ends_with(e))
    }
}

fn norm(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace() && *c != '-' && *c != '_').flat_map(char::to_lowercase).collect()
}

/// How a font name was resolved.
#[derive(Debug, Clone, PartialEq)]
pub enum Pick {
    /// Family and style found (or no font given: the default font).
    Exact,
    /// Family found, the style not: the family's regular style.
    StyleMissing,
    /// Unknown family: the default font.
    Fallback,
}

/// A bundled TrueType font by family and style (`None`: no such family). An unknown style
/// gives the family's regular style ([`Pick::StyleMissing`]).
pub fn builtin(name: &FontName) -> Option<(&'static Builtin, Pick)> {
    let fam = norm(&name.family);
    let of_family: Vec<&Builtin> = BUILTIN.iter().filter(|b| norm(b.family) == fam).collect();
    if of_family.is_empty() {
        return None;
    }
    let want = name.style.as_deref().map(norm).unwrap_or_else(|| "regular".into());
    if let Some(b) = of_family.iter().find(|b| norm(b.style) == want) {
        return Some((b, Pick::Exact));
    }
    let regular = of_family.iter().find(|b| b.style == "Regular").unwrap_or(&of_family[0]);
    Some((regular, Pick::StyleMissing))
}

/// The font for a name: the stencil font (no name, its own name with style Regular or Bold,
/// unknown families) or a bundled TrueType font.
pub fn select(name: &FontName) -> (Font, Pick) {
    let fam = norm(&name.family);
    if fam.is_empty() || fam == norm(STENCIL_FAMILY) {
        return match name.style.as_deref().map(norm).as_deref() {
            None | Some("regular") => (Font::stencil(false), Pick::Exact),
            Some("bold") => (Font::stencil(true), Pick::Exact),
            Some(_) => (Font::stencil(false), Pick::StyleMissing),
        };
    }
    match builtin(name) {
        Some((b, pick)) => (Font::builtin(b), pick),
        None => (Font::stencil(false), Pick::Fallback),
    }
}

/// Family and style names of the bundled fonts, e.g. `"Inter:style=Bold"` (the stencil font
/// first).
pub fn builtin_names() -> Vec<String> {
    let mut out: Vec<String> = ["Regular", "Bold"].iter().map(|s| format!("{STENCIL_FAMILY}:style={s}")).collect();
    out.extend(BUILTIN.iter().map(|b| format!("{}:style={}", b.family, b.style)));
    out
}

/// Font data: a bundled font, a user font file or the stencil font (no file).
#[derive(Clone)]
pub enum FontData {
    Static(&'static [u8]),
    Shared(Arc<[u8]>),
    Stencil { bold: bool },
}

impl FontData {
    /// The font file (empty for the stencil font).
    pub fn bytes(&self) -> &[u8] {
        match self {
            FontData::Static(b) => b,
            FontData::Shared(b) => b,
            FontData::Stencil { .. } => &[],
        }
    }
}

/// A font to lay out text with. `key` identifies the data for the glyph cache (content hash).
#[derive(Clone)]
pub struct Font {
    pub data: FontData,
    pub index: u32,
    pub key: u64,
}

impl Font {
    /// „cad stencil mono“, regular or bold (default stroke width).
    pub fn stencil(bold: bool) -> Font {
        Font { data: FontData::Stencil { bold }, index: 0, key: 100 + u64::from(bold) }
    }

    pub fn is_stencil(&self) -> bool {
        matches!(self.data, FontData::Stencil { .. })
    }

    pub fn builtin(b: &'static Builtin) -> Font {
        let i = BUILTIN.iter().position(|x| std::ptr::eq(x, b)).unwrap_or(0) as u64;
        Font { data: FontData::Static(b.data), index: 0, key: i + 1 }
    }

    /// A font file's contents (`index`: face in a collection). Checks that it parses.
    pub fn from_data(data: Arc<[u8]>, index: u32) -> Result<Font, String> {
        ttf_parser::Face::parse(&data, index).map_err(|e| format!("Schriftdatei nicht lesbar ({e})"))?;
        let key = fnv(&data) ^ (index as u64).rotate_left(48) | 1 << 63;
        Ok(Font { data: FontData::Shared(data), index, key })
    }

    /// Family and subfamily (style) names of the face.
    pub fn names(&self) -> (String, String) {
        if let FontData::Stencil { bold } = self.data {
            return (STENCIL_FAMILY.into(), if bold { "Bold" } else { "Regular" }.into());
        }
        let Ok(f) = ttf_parser::Face::parse(self.data.bytes(), self.index) else {
            return (String::new(), String::new());
        };
        let get =
            |id: u16| f.names().into_iter().filter(|n| n.name_id == id).find_map(|n| n.to_string()).unwrap_or_default();
        let fam = get(ttf_parser::name_id::TYPOGRAPHIC_FAMILY);
        let fam = if fam.is_empty() { get(ttf_parser::name_id::FAMILY) } else { fam };
        let sty = get(ttf_parser::name_id::TYPOGRAPHIC_SUBFAMILY);
        let sty = if sty.is_empty() { get(ttf_parser::name_id::SUBFAMILY) } else { sty };
        (fam, sty)
    }
}

fn fnv(b: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for chunk in b.chunks(8) {
        let mut w = [0u8; 8];
        w[..chunk.len()].copy_from_slice(chunk);
        h = (h ^ u64::from_le_bytes(w)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    h ^ b.len() as u64
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HAlign {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VAlign {
    #[default]
    Baseline,
    Bottom,
    Center,
    Top,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Ltr,
    Rtl,
    Ttb,
    Btt,
}

impl Dir {
    pub fn parse(s: &str) -> Option<Dir> {
        Some(match s.to_ascii_lowercase().as_str() {
            "ltr" => Dir::Ltr,
            "rtl" => Dir::Rtl,
            "ttb" => Dir::Ttb,
            "btt" => Dir::Btt,
            _ => return None,
        })
    }

    pub fn vertical(self) -> bool {
        matches!(self, Dir::Ttb | Dir::Btt)
    }
}

/// `text()` parameters besides the font.
#[derive(Debug, Clone)]
pub struct TextParams {
    pub text: String,
    pub size: f64,
    pub halign: HAlign,
    pub valign: VAlign,
    pub spacing: f64,
    /// `None`: guessed from the text (left to right for Latin).
    pub direction: Option<Dir>,
    /// BCP 47 language tag (`"de"`, `"en"`); `None`: not set.
    pub language: Option<String>,
    /// Script as ISO 15924 code (`"Latn"`) or OpenSCAD name (`"latin"`); `None`: guessed.
    pub script: Option<String>,
    /// Stencil font only: stroke width in mm (`None`: [`stencil::DEFAULT_STROKE`] × `size`,
    /// bold [`stencil::DEFAULT_STROKE_BOLD`]); at most [`stencil::MAX_STROKE`] × `size`.
    pub stroke_width: Option<f64>,
    /// Stencil font only: bridge width in mm (`None`: [`stencil::DEFAULT_BRIDGE`] × `size`;
    /// 0: no bridges); at most [`stencil::MAX_BRIDGE`] × `size`.
    pub bridge_width: Option<f64>,
}

impl Default for TextParams {
    fn default() -> Self {
        TextParams {
            text: String::new(),
            size: 10.0,
            halign: HAlign::Left,
            valign: VAlign::Baseline,
            spacing: 1.0,
            direction: None,
            language: None,
            script: None,
            stroke_width: None,
            bridge_width: None,
        }
    }
}

/// One placed glyph: closed contours in mm (as in the font: outer and inner contours in
/// opposite directions; fill with the nonzero rule).
#[derive(Debug, Clone)]
pub struct Glyph {
    pub id: u16,
    /// Byte offset of the glyph's cluster in the text.
    pub cluster: u32,
    /// From the fallback font (the chosen font lacks the character).
    pub fallback: bool,
    pub contours: Vec<Contour>,
}

/// Laid out text.
#[derive(Debug, Clone, Default)]
pub struct Layout {
    pub glyphs: Vec<Glyph>,
    /// Pen advance of the whole text in mm (after alignment the pen started at `origin`).
    pub advance: DVec2,
    pub origin: DVec2,
    /// Characters the font lacks, taken from the fallback font (JetBrains Mono).
    pub fallback: Vec<char>,
    /// Characters no font has a glyph for (drawn as the font's `.notdef`).
    pub missing: Vec<char>,
}

impl Layout {
    pub fn bounds(&self) -> Option<(DVec2, DVec2)> {
        cad_outline::bounds_of(
            self.glyphs.iter().flat_map(|g| g.contours.iter()).filter_map(|c| c.bounds(1e-4)).flat_map(|(a, b)| [a, b]),
        )
    }
}

struct Builder {
    contours: Vec<Contour>,
    cur: Contour,
    p: DVec2,
    start: DVec2,
}

impl Builder {
    fn finish(&mut self) {
        if self.cur.segs.is_empty() {
            return;
        }
        if self.p != self.start {
            self.cur.segs.push(Seg::Line { a: self.p, b: self.start });
        }
        self.cur.closed = true;
        self.contours.push(std::mem::take(&mut self.cur));
    }
}

impl ttf_parser::OutlineBuilder for Builder {
    fn move_to(&mut self, x: f32, y: f32) {
        self.finish();
        self.p = DVec2::new(x as f64, y as f64);
        self.start = self.p;
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let b = DVec2::new(x as f64, y as f64);
        if b != self.p {
            self.cur.segs.push(Seg::Line { a: self.p, b });
        }
        self.p = b;
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let b = DVec2::new(x as f64, y as f64);
        self.cur.segs.push(Seg::Quad([self.p, DVec2::new(x1 as f64, y1 as f64), b]));
        self.p = b;
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let b = DVec2::new(x as f64, y as f64);
        self.cur.segs.push(Seg::Cubic([self.p, DVec2::new(x1 as f64, y1 as f64), DVec2::new(x2 as f64, y2 as f64), b]));
        self.p = b;
    }
    fn close(&mut self) {
        self.finish();
    }
}

/// Glyph outlines in font units by (font key, glyph id).
type GlyphCache = HashMap<(u64, u16), Rc<Vec<Contour>>>;

thread_local! {
    static GLYPHS: RefCell<GlyphCache> = RefCell::new(HashMap::new());
}

/// Outline of a glyph in font units (cached).
fn outline(face: &ttf_parser::Face, key: u64, id: u16) -> Rc<Vec<Contour>> {
    if let Some(c) = GLYPHS.with(|g| g.borrow().get(&(key, id)).cloned()) {
        return c;
    }
    let mut b = Builder { contours: Vec::new(), cur: Contour::default(), p: DVec2::ZERO, start: DVec2::ZERO };
    face.outline_glyph(ttf_parser::GlyphId(id), &mut b);
    b.finish();
    let c = Rc::new(b.contours);
    GLYPHS.with(|g| {
        let mut g = g.borrow_mut();
        if g.len() > 20_000 {
            g.clear();
        }
        g.insert((key, id), c.clone());
    });
    c
}

fn script_of(s: &str) -> Option<rustybuzz::Script> {
    let named = match s.to_ascii_lowercase().as_str() {
        "latin" => "Latn",
        "greek" => "Grek",
        "cyrillic" => "Cyrl",
        "arabic" => "Arab",
        "hebrew" => "Hebr",
        "han" | "chinese" => "Hani",
        "hiragana" => "Hira",
        "katakana" => "Kana",
        "hangul" => "Hang",
        "thai" => "Thai",
        "devanagari" => "Deva",
        _ => s,
    };
    let b = named.as_bytes();
    if b.len() != 4 {
        return None;
    }
    let tag = ttf_parser::Tag::from_bytes(&[b[0].to_ascii_uppercase(), b[1], b[2], b[3]]);
    rustybuzz::Script::from_iso15924_tag(tag)
}

/// Characters every bundled font has a real glyph for (no `.notdef`), the stencil font
/// included (Plan 17.4): letters, digits, German umlauts and ß/ẞ, the usual punctuation and
/// symbols.
pub const REQUIRED_CHARS: &str = concat!(
    "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789",
    "ÄÖÜäöüßẞ",
    " .,;:!?'\"„“‚‘»«()[]{}-–—_/\\&%#+*=<>°²³€@§~^|·•…±×÷µØø",
);

/// Characters of `s` the font has no glyph for.
pub fn missing_chars(font: &Font, s: &str) -> Vec<char> {
    if font.is_stencil() {
        let mut out: Vec<char> = Vec::new();
        for c in s.chars().filter(|c| !stencil::has(*c)) {
            if !out.contains(&c) {
                out.push(c);
            }
        }
        return out;
    }
    let Ok(f) = ttf_parser::Face::parse(font.data.bytes(), font.index) else { return s.chars().collect() };
    let mut out: Vec<char> = Vec::new();
    for c in s.chars() {
        if f.glyph_index(c).is_none() && !out.contains(&c) {
            out.push(c);
        }
    }
    out
}

/// Characters of [`REQUIRED_CHARS`] that Allerta Stencil (Google Fonts) has no glyph for. The
/// font is bundled unchanged (its licence reserves the name „Allerta“ for unmodified
/// versions); these characters come from JetBrains Mono ([`fallback_font`]) with a warning.
pub const ALLERTA_STENCIL_GAPS: &str = "ẞ„‚{}_#°²³§~^·•…±µØø";

/// The fallback font for characters `font` lacks: JetBrains Mono, Bold for bold fonts
/// (OS/2 weight ≥ 600), else Regular. JetBrains Mono covers [`REQUIRED_CHARS`] and much more
/// (Latin, Greek, Cyrillic, arrows, box drawing).
pub fn fallback_font(font: &Font) -> Font {
    let bold = match font.data {
        FontData::Stencil { bold } => bold,
        _ => ttf_parser::Face::parse(font.data.bytes(), font.index).is_ok_and(|f| f.weight().to_number() >= 600),
    };
    let style = if bold { "Bold" } else { "Regular" };
    Font::builtin(BUILTIN.iter().find(|b| b.family == FALLBACK_FAMILY && b.style == style).unwrap())
}

/// Stroke and bridge width of the stencil font in its units for `p` (defaults, limits).
pub fn stencil_widths(bold: bool, p: &TextParams) -> (f64, f64) {
    let unit = p.size / SIZE_TO_EM / stencil::UPEM; // mm per unit
    let def = if bold { stencil::DEFAULT_STROKE_BOLD } else { stencil::DEFAULT_STROKE };
    let w = p.stroke_width.filter(|w| w.is_finite() && *w > 0.0).map_or(def * stencil::CAP, |w| w / unit);
    let b = p
        .bridge_width
        .filter(|b| b.is_finite() && *b >= 0.0)
        .map_or(stencil::DEFAULT_BRIDGE * stencil::CAP, |b| b / unit);
    let b = if b > 0.0 { b.clamp(stencil::MIN_BRIDGE * stencil::CAP, stencil::MAX_BRIDGE * stencil::CAP) } else { 0.0 };
    (w.min(stencil::MAX_STROKE * stencil::CAP), b)
}

/// Combining marks, joiners and variation selectors stay with the preceding character.
fn joins_previous(c: char) -> bool {
    matches!(c as u32, 0x0300..=0x036F | 0x1AB0..=0x1AFF | 0x20D0..=0x20FF | 0xFE00..=0xFE0F | 0xFE20..=0xFE2F | 0x200C | 0x200D)
}

/// Lays out `p.text` in `font`: shaping, glyph outlines scaled to mm, spacing and alignment.
/// Characters the font lacks are taken from JetBrains Mono ([`fallback_font`], reported in
/// [`Layout::fallback`]); characters no font has stay `.notdef` ([`Layout::missing`]).
/// Errors: unreadable font, unknown script or language.
pub fn layout(font: &Font, p: &TextParams) -> Result<Layout, String> {
    // the stencil font has no face: its glyphs are drawn, not shaped
    let stencil_bold = match font.data {
        FontData::Stencil { bold } => Some(bold),
        _ => None,
    };
    let face = match stencil_bold {
        Some(_) => None,
        None => Some(rustybuzz::Face::from_slice(font.data.bytes(), font.index).ok_or("Schriftdatei nicht lesbar")?),
    };
    let has_glyph = |c: char| match &face {
        Some(f) => f.glyph_index(c).is_some(),
        None => stencil::has(c),
    };
    let fb_font = fallback_font(font);
    let use_fb = fb_font.key != font.key;
    let fb_face = rustybuzz::Face::from_slice(fb_font.data.bytes(), 0).ok_or("Ersatzschrift nicht lesbar")?;
    // direction, script and language of the whole text
    let mut probe = rustybuzz::UnicodeBuffer::new();
    probe.push_str(&p.text);
    if let Some(d) = p.direction {
        probe.set_direction(match d {
            Dir::Ltr => rustybuzz::Direction::LeftToRight,
            Dir::Rtl => rustybuzz::Direction::RightToLeft,
            Dir::Ttb => rustybuzz::Direction::TopToBottom,
            Dir::Btt => rustybuzz::Direction::BottomToTop,
        });
    }
    if let Some(s) = &p.script {
        probe.set_script(script_of(s).ok_or_else(|| format!("unbekannte Schrift (script) \"{s}\""))?);
    }
    let lang = match &p.language {
        Some(l) => Some(l.parse::<rustybuzz::Language>().map_err(|_| format!("unbekannte Sprache \"{l}\""))?),
        None => None,
    };
    probe.guess_segment_properties();
    let (dir, script) = (probe.direction(), probe.script());
    let vertical = matches!(dir, rustybuzz::Direction::TopToBottom | rustybuzz::Direction::BottomToTop);
    // runs of characters by font: 0 = the font, 1 = the fallback
    let mut out = Layout::default();
    let mut runs: Vec<(usize, usize, usize)> = Vec::new(); // (font, start, end) byte ranges
    for (i, c) in p.text.char_indices() {
        let mut which = 0;
        if !has_glyph(c) && !c.is_control() {
            if joins_previous(c) && face.is_some() {
                which = runs.last().map_or(0, |r| r.0);
            } else if use_fb && fb_face.glyph_index(c).is_some() {
                which = 1;
                if !out.fallback.contains(&c) {
                    out.fallback.push(c);
                }
            } else if !out.missing.contains(&c) {
                out.missing.push(c);
            }
        }
        match runs.last_mut() {
            Some(r) if r.0 == which => r.2 = i + c.len_utf8(),
            _ => runs.push((which, i, i + c.len_utf8())),
        }
    }
    // visual order of the runs
    if matches!(dir, rustybuzz::Direction::RightToLeft | rustybuzz::Direction::BottomToTop) {
        runs.reverse();
    }
    let mut pen = DVec2::ZERO;
    let reversed = matches!(dir, rustybuzz::Direction::RightToLeft | rustybuzz::Direction::BottomToTop);
    for (which, a, b) in runs {
        if which == 0
            && let Some(bold) = stencil_bold
        {
            let (sw, bw) = stencil_widths(bold, p);
            let k = p.size / SIZE_TO_EM / stencil::UPEM;
            let kp = k * OPENSCAD_POSITION_SCALE;
            let mut chars: Vec<(usize, char)> = p.text[a..b].char_indices().map(|(i, c)| (i + a, c)).collect();
            if reversed {
                chars.reverse();
            }
            for (i, c) in chars {
                let off =
                    if vertical { DVec2::new(-stencil::ADVANCE / 2.0, -stencil::ASCENT) * kp } else { DVec2::ZERO };
                if let (Some(id), Some(outline)) = (stencil::glyph_id(c), stencil::render(c, sw, bw))
                    && !outline.is_empty()
                {
                    let m = glam::DAffine2::from_cols(DVec2::new(k, 0.0), DVec2::new(0.0, k), pen + off);
                    let contours = outline.iter().map(|c| c.transform(&m)).collect();
                    out.glyphs.push(Glyph { id, cluster: i as u32, fallback: false, contours });
                }
                let adv = if vertical {
                    DVec2::new(0.0, -(stencil::ASCENT - stencil::DESCENT))
                } else {
                    DVec2::new(stencil::ADVANCE, 0.0)
                };
                pen += adv * kp * p.spacing;
            }
            continue;
        }
        let (f, key) = match (&face, which) {
            (Some(face), 0) => (face, font.key),
            _ => (&fb_face, fb_font.key),
        };
        let mut buf = rustybuzz::UnicodeBuffer::new();
        buf.push_str(&p.text[a..b]);
        buf.set_direction(dir);
        buf.set_script(script);
        if let Some(l) = &lang {
            buf.set_language(l.clone());
        }
        let shaped = rustybuzz::shape(f, &[], buf);
        let k = p.size / SIZE_TO_EM / f.units_per_em() as f64;
        let kp = k * OPENSCAD_POSITION_SCALE;
        for (info, pos) in shaped.glyph_infos().iter().zip(shaped.glyph_positions()) {
            let id = info.glyph_id as u16;
            let origin = pen + DVec2::new(pos.x_offset as f64, pos.y_offset as f64) * kp;
            let m = glam::DAffine2::from_cols(DVec2::new(k, 0.0), DVec2::new(0.0, k), origin);
            let contours: Vec<Contour> = outline(f, key, id).iter().map(|c| c.transform(&m)).collect();
            if !contours.is_empty() {
                out.glyphs.push(Glyph { id, cluster: info.cluster + a as u32, fallback: which == 1, contours });
            }
            pen += DVec2::new(pos.x_advance as f64, pos.y_advance as f64) * kp * p.spacing;
        }
    }
    out.advance = pen;
    // alignment: horizontally by the advance, vertically by the glyph extents (both as OpenSCAD,
    // the extents scaled like the positions)
    let b = out.bounds();
    let (lo, hi) = b.unwrap_or((DVec2::ZERO, DVec2::ZERO));
    let (lo, hi) = (lo * OPENSCAD_POSITION_SCALE, hi * OPENSCAD_POSITION_SCALE);
    let dx = if vertical {
        match p.halign {
            HAlign::Left => 0.0,
            HAlign::Center => -(lo.x + hi.x) / 2.0,
            HAlign::Right => -hi.x,
        }
    } else {
        let (a0, a1) = (pen.x.min(0.0), pen.x.max(0.0));
        match p.halign {
            HAlign::Left => -a0,
            HAlign::Center => -(a0 + a1) / 2.0,
            HAlign::Right => -a1,
        }
    };
    let dy = match p.valign {
        VAlign::Baseline => 0.0,
        VAlign::Top => -hi.y,
        VAlign::Center => -(lo.y + hi.y) / 2.0,
        VAlign::Bottom => -lo.y,
    };
    let shift = DVec2::new(dx, dy);
    if shift != DVec2::ZERO {
        let m = glam::DAffine2::from_translation(shift);
        for g in &mut out.glyphs {
            g.contours = g.contours.iter().map(|c| c.transform(&m)).collect();
        }
        out.origin = shift;
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
