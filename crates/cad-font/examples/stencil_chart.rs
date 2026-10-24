//! Character chart of „cad stencil mono“ as SVG (Bericht 17): every glyph in its cell with the
//! base line, x-height, cap height and the advance, plus one line per glyph on stdout (pieces,
//! holes, area in units²).
//!
//!   cargo run -p cad-font --example stencil_chart -- chart.svg [stroke bridge columns chars]
//!
//! `stroke` and `bridge` are fractions of the cap height (`size`), defaults 0.14 and 0.1.

use cad_font::stencil::{self, ADVANCE, ASCENDER, CAP, CHARS, DESCENDER, XHEIGHT};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out = args.get(1).cloned().unwrap_or_else(|| "stencil-chart.svg".into());
    let sw = args.get(2).and_then(|s| s.parse::<f64>().ok()).unwrap_or(stencil::DEFAULT_STROKE) * CAP;
    let bw = args.get(3).and_then(|s| s.parse::<f64>().ok()).unwrap_or(stencil::DEFAULT_BRIDGE) * CAP;
    let cols = args.get(4).and_then(|s| s.parse::<usize>().ok()).unwrap_or(16);
    let chars: Vec<char> = args
        .get(5)
        .filter(|s| !s.is_empty())
        .map_or_else(|| CHARS.chars().filter(|c| *c != ' ').collect(), |s| s.chars().collect());
    let (cw, ch) = (ADVANCE + 80.0, 1400.0);
    let rows = chars.len().div_ceil(cols);
    let (wpx, hpx) = (cols as f64 * cw, rows as f64 * ch);
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {wpx} {hpx}\" width=\"{}\" height=\"{}\">\n<rect width=\"100%\" height=\"100%\" fill=\"white\"/>\n",
        wpx / 4.0,
        hpx / 4.0
    );
    for (i, c) in chars.iter().enumerate() {
        let (ox, oy) = ((i % cols) as f64 * cw + 40.0, (i / cols) as f64 * ch + 1050.0);
        // cell, metric lines
        svg += &format!(
            "<rect x=\"{ox}\" y=\"{}\" width=\"{ADVANCE}\" height=\"{}\" fill=\"#f4f6fa\" stroke=\"#c8cfdc\" stroke-width=\"3\"/>\n",
            oy - ASCENDER - 60.0,
            ASCENDER - DESCENDER + 120.0
        );
        for (y, col) in [(0.0, "#7a8aa8"), (XHEIGHT, "#d0d7e4"), (CAP, "#d0d7e4"), (DESCENDER, "#e2e6ee")] {
            svg += &format!(
                "<line x1=\"{ox}\" x2=\"{}\" y1=\"{}\" y2=\"{}\" stroke=\"{col}\" stroke-width=\"3\"/>\n",
                ox + ADVANCE,
                oy - y,
                oy - y
            );
        }
        let Some(region) = stencil::region(*c, sw, bw) else { continue };
        let mut d = String::new();
        let (mut holes, mut pieces, mut area) = (0, 0, 0.0);
        for lp in &region {
            let a = cad_kernel::region2::loop_area(lp);
            area += a;
            if a < 0.0 {
                holes += 1;
            } else {
                pieces += 1;
            }
            let mut first = true;
            for s in lp {
                for p in s.sample(0.5) {
                    d += &format!("{}{:.1},{:.1} ", if first { "M" } else { "L" }, ox + p.x, oy - p.y);
                    first = false;
                }
            }
            d += "Z ";
        }
        let fill = if holes > 0 { "#c0392b" } else { "#1d2433" };
        svg += &format!("<path d=\"{d}\" fill=\"{fill}\" fill-rule=\"nonzero\"/>\n");
        let label = match c {
            '&' => "&amp;".to_string(),
            '<' => "&lt;".to_string(),
            '>' => "&gt;".to_string(),
            c => c.to_string(),
        };
        svg += &format!(
            "<text x=\"{}\" y=\"{}\" font-family=\"sans-serif\" font-size=\"90\" fill=\"#5a6478\">{label} U+{:04X}</text>\n",
            ox + 10.0,
            oy + 300.0,
            *c as u32
        );
        println!("{c}\tU+{:04X}\tpieces {pieces}\tholes {holes}\tarea {area:.0}", *c as u32);
    }
    svg += "</svg>\n";
    std::fs::write(&out, svg).expect("write");
    eprintln!("{out}: {} glyphs, stroke {sw:.1}, bridge {bw:.1} units", chars.len());
}
