//! „cad stencil mono“, the default font of `text()` (Plan 17.4, Bericht 17), through the
//! engine with native SMLib: every glyph rendered to a profile has no holes, extrusions have
//! the area of the layout times the height, a typical sign text builds about as fast as with
//! Allerta Stencil.

mod common;

use std::time::Instant;

use cad_eval::{EvalOptions, eval_source};
use cad_ir::Tree;
use common::*;

fn tree(src: &str) -> Tree {
    let out = eval_source(src, &EvalOptions::default());
    let d: Vec<_> = out.diagnostics.iter().collect();
    assert!(d.is_empty(), "{src}: {d:#?}");
    out.tree
}

fn loop_area(lp: &[cad_kernel::Segment]) -> f64 {
    cad_kernel::region2::loop_area(lp)
}

/// Every character of the font (the required set and more), rendered to a profile by the
/// engine: outer loops only, no holes – nothing falls out of a stencil.
#[test]
fn every_glyph_profile_has_no_holes() {
    for (src_font, bold) in [("", false), (", font = \"cad stencil mono:style=Bold\"", true)] {
        for ch in cad_font::stencil::CHARS.chars().filter(|c| *c != ' ') {
            let text = match ch {
                '"' => "\\\"".to_string(),
                '\\' => "\\\\".to_string(),
                c => c.to_string(),
            };
            let t = tree(&format!("text(\"{text}\"{src_font});"));
            let loops = cad_engine::region_2d(&t, t.root.unwrap()).unwrap_or_else(|e| panic!("{ch:?}: {e}"));
            assert!(!loops.is_empty(), "{ch:?}");
            let holes = loops.iter().filter(|l| loop_area(l) < 0.0).count();
            assert_eq!(holes, 0, "{ch:?} (bold {bold}): {holes} holes");
        }
    }
    // without bridges the counters are there (O: one hole)
    let t = tree("text(\"O\", bridge_width = 0);");
    let loops = cad_engine::region_2d(&t, t.root.unwrap()).unwrap();
    assert_eq!(loops.iter().filter(|l| loop_area(l) < 0.0).count(), 1);
}

/// Volume of extruded text = area of the profile × height (exact lines and arcs).
#[test]
fn extruded_text_volume() {
    let src2d = "text(\"Grüße, 42 °C!\", size = 8)";
    let t = tree(&format!("{src2d};"));
    let area: f64 = cad_engine::region_2d(&t, t.root.unwrap()).unwrap().iter().map(|l| loop_area(l)).sum();
    let mut e = engine();
    let ev = e.evaluate(tree(&format!("linear_extrude(1.5) {src2d};")));
    assert_clean(&ev);
    assert_approx("volume", root_volume(&mut e, &ev), 1.5 * area, 1e-6);
    // the same glyphs from the layout
    let lay = cad_font::layout(
        &cad_font::Font::stencil(false),
        &cad_font::TextParams { text: "Grüße, 42 °C!".into(), size: 8.0, ..Default::default() },
    )
    .unwrap();
    let la: f64 = lay
        .glyphs
        .iter()
        .flat_map(|g| g.contours.iter())
        .map(|c| cad_outline::region_area(std::slice::from_ref(c), 1e-5))
        .sum();
    assert_approx("layout area", la, area, 1e-4);
}

/// A typical sign: plate ∪ raised text, 22 characters. The stencil font's glyphs are lines and
/// arcs (Allerta Stencil: B-spline flanks); the kernel needs no more time than for Allerta.
#[test]
fn sign_text_kernel_time_like_allerta() {
    let sign = |font: &str| {
        format!(
            "cube([100, 14, 2]); translate([50, 4, 2]) linear_extrude(1.2) text(\"ZUTRITT NUR MIT BRILLE\", size = 4.2, halign = \"center\"{font});"
        )
    };
    let run = |src: String| {
        let t = tree(&src);
        let t0 = Instant::now();
        let mut e = engine();
        let ev = e.evaluate(t);
        assert_clean(&ev);
        let v = root_volume(&mut e, &ev);
        (t0.elapsed().as_secs_f64(), v)
    };
    let (ts, vs) = run(sign(""));
    let (ta, va) = run(sign(", font = \"Allerta Stencil\""));
    println!("Schild: cad stencil mono {ts:.2} s ({vs:.1} mm³), Allerta Stencil {ta:.2} s ({va:.1} mm³)");
    let plate = 100.0 * 14.0 * 2.0;
    assert!(vs > plate + 10.0 && va > plate + 10.0);
    assert!(ts < 1.5 * ta + 1.0, "stencil {ts:.2} s vs Allerta {ta:.2} s");
}
