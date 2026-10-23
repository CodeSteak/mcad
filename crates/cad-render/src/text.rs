//! Text and 2D drawing on finished images: labels, titles, contact sheets.

use crate::font::{self, Font};
use crate::{Image, to_u8};

/// Font size of [`draw_text`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextSize {
    /// 7×14 px cells (labels).
    Small,
    /// 10×19 px cells (titles).
    Large,
}

fn font(size: TextSize) -> &'static Font {
    match size {
        TextSize::Small => &font::SMALL,
        TextSize::Large => &font::LARGE,
    }
}

fn glyph(f: &Font, c: char) -> Option<&'static str> {
    let i = font::CHARS.chars().position(|x| x == c)?;
    f.glyphs.get(i).copied()
}

/// Width and height of `text` in pixels.
pub fn text_size(text: &str, size: TextSize) -> (u32, u32) {
    let f = font(size);
    (text.chars().count() as u32 * f.w, f.h)
}

impl Image {
    /// Blends `c` (alpha in `c[3]`) over pixel (x, y); outside pixels are ignored.
    pub fn blend_pixel(&mut self, x: i64, y: i64, c: [f32; 4]) {
        if x < 0 || y < 0 || x >= self.width as i64 || y >= self.height as i64 {
            return;
        }
        let i = ((y as u32 * self.width + x as u32) * 4) as usize;
        let a = c[3].clamp(0.0, 1.0);
        for (px, ck) in self.rgba[i..i + 3].iter_mut().zip(c) {
            let d = *px as f32 / 255.0;
            *px = ((d * (1.0 - a) + ck * a).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        }
        self.rgba[i + 3] = 255;
    }

    /// Fills a rectangle (alpha blended).
    pub fn fill_rect(&mut self, x: i64, y: i64, w: u32, h: u32, c: [f32; 4]) {
        for yy in y..y + h as i64 {
            for xx in x..x + w as i64 {
                self.blend_pixel(xx, yy, c);
            }
        }
    }

    /// Draws `text` with its top left corner at (x, y). Characters missing from the font are
    /// drawn as `?`.
    pub fn draw_text(&mut self, x: i64, y: i64, text: &str, size: TextSize, color: [f32; 4]) {
        let f = font(size);
        for (k, c) in text.chars().enumerate() {
            let Some(g) = glyph(f, c).or_else(|| glyph(f, '?')) else { continue };
            let gx = x + (k as u32 * f.w) as i64;
            for (i, d) in g.bytes().enumerate() {
                let a = match d {
                    b'0'..=b'9' => d - b'0',
                    b'a'..=b'f' => d - b'a' + 10,
                    _ => 0,
                };
                if a == 0 {
                    continue;
                }
                let (px, py) = ((i as u32 % f.w) as i64, (i as u32 / f.w) as i64);
                self.blend_pixel(gx + px, y + py, [color[0], color[1], color[2], color[3] * a as f32 / 15.0]);
            }
        }
    }

    /// Text on a light box (labels, titles); returns the box (x, y, w, h).
    pub fn draw_label(&mut self, x: i64, y: i64, text: &str, size: TextSize, color: [f32; 4]) -> (i64, i64, u32, u32) {
        self.draw_label_on(x, y, text, size, color, [1.0, 1.0, 1.0, 0.78])
    }

    /// [`Image::draw_label`] on a box of colour `bg`.
    pub fn draw_label_on(
        &mut self,
        x: i64,
        y: i64,
        text: &str,
        size: TextSize,
        color: [f32; 4],
        bg: [f32; 4],
    ) -> (i64, i64, u32, u32) {
        let (w, h) = text_size(text, size);
        let pad = if size == TextSize::Large { 4 } else { 2 };
        let bx = (x - pad, y - pad / 2, w + 2 * pad as u32, h + pad as u32);
        self.fill_rect(bx.0, bx.1, bx.2, bx.3, bg);
        self.draw_text(x, y, text, size, color);
        bx
    }
}

/// Tiles of equal size in a grid of `cols` columns with `gap` pixels between them.
pub fn contact_sheet(tiles: &[Image], cols: usize, gap: u32, background: [f32; 4]) -> Image {
    let cols = cols.clamp(1, tiles.len().max(1));
    let rows = tiles.len().div_ceil(cols).max(1);
    let tw = tiles.iter().map(|t| t.width).max().unwrap_or(1);
    let th = tiles.iter().map(|t| t.height).max().unwrap_or(1);
    let w = cols as u32 * tw + (cols as u32 + 1) * gap;
    let h = rows as u32 * th + (rows as u32 + 1) * gap;
    let bg = to_u8([background[0] * 0.9, background[1] * 0.9, background[2] * 0.9, 1.0]);
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for _ in 0..w * h {
        rgba.extend_from_slice(&bg);
    }
    let mut img = Image { width: w, height: h, rgba };
    for (k, t) in tiles.iter().enumerate() {
        let (c, r) = ((k % cols) as u32, (k / cols) as u32);
        let (ox, oy) = (gap + c * (tw + gap), gap + r * (th + gap));
        for y in 0..t.height {
            let src = (y * t.width * 4) as usize;
            let dst = (((oy + y) * w + ox) * 4) as usize;
            img.rgba[dst..dst + (t.width * 4) as usize].copy_from_slice(&t.rgba[src..src + (t.width * 4) as usize]);
        }
    }
    img
}

/// Position of tile `k` in a [`contact_sheet`]: (x, y).
pub fn tile_origin(k: usize, cols: usize, tile: (u32, u32), gap: u32) -> (u32, u32) {
    let cols = cols.max(1);
    let (c, r) = ((k % cols) as u32, (k / cols) as u32);
    (gap + c * (tile.0 + gap), gap + r * (tile.1 + gap))
}
