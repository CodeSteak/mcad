//! Colour palette (Plan 16.1): names, warning and stable random colour for unknown names,
//! exact hex/vectors, `filament = n`, distinctness (also for colour vision deficiencies) and
//! the generated TypeScript table.

mod common;

use std::path::PathBuf;

use cad_eval::palette::{
    self, ColorKind, Cvd, PALETTE, Theme, contrast_ratio, delta_e, display_rgb, oklch, parse_color, random_color,
    rgba_bytes, simulate_cvd,
};
use cad_ir::Op;
use common::*;

/// `web/src/logic/palette.ts` must match the Rust table.
/// Regenerate with `UPDATE_PALETTE_TS=1 cargo test -p cad-eval --test palette`.
#[test]
fn palette_ts_is_current() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../web/src/logic/palette.ts");
    let want = palette::palette_ts();
    if std::env::var_os("UPDATE_PALETTE_TS").is_some() {
        std::fs::write(&path, &want).unwrap();
        return;
    }
    let got = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        got == want,
        "{} is out of date; run UPDATE_PALETTE_TS=1 cargo test -p cad-eval --test palette",
        path.display()
    );
}

fn colors(src: &str) -> Vec<(Op, Option<[f32; 4]>)> {
    let out = run(src);
    out.tree.nodes.iter().filter(|n| matches!(n.op, Op::Color { .. })).map(|n| (n.op.clone(), n.color)).collect()
}

fn rgba(op: &Op) -> [f32; 4] {
    match op {
        Op::Color { rgba, .. } => *rgba,
        _ => unreachable!(),
    }
}

#[test]
fn palette_names_case_insensitive() {
    let out =
        clean("color(\"Teal\") cube(1); color(\"RED\") cube(1); color(\"grey\") cube(1); color(\"white\") cube(1);");
    let c: Vec<_> = out
        .tree
        .nodes
        .iter()
        .filter_map(|n| match &n.op {
            Op::Color { rgba, name, filament } => Some((rgba_bytes(*rgba), name.clone(), *filament)),
            _ => None,
        })
        .collect();
    assert_eq!(c[0], ([0x21, 0x72, 0x74, 255], Some("teal".into()), None));
    assert_eq!(c[1].0, [0xc7, 0x42, 0x40, 255]);
    assert_eq!(c[2].1.as_deref(), Some("gray"));
    assert_eq!(c[3].0, [0xf4, 0xf3, 0xf0, 255]);
}

#[test]
fn unknown_name_warns_and_gets_a_stable_random_colour() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let src = "color(\"lightgoldenrodyellow\") cube(1);\ncolor(\"Rot\") cube(1);";
    let out = run(src);
    let w = warnings(&out);
    assert_eq!(w.len(), 2, "{w:?}");
    assert!(
        w[0].starts_with("Farbe `lightgoldenrodyellow` unbekannt – Zufallsfarbe (Palette: red, orange"),
        "{}",
        w[0]
    );
    assert!(w[1].contains("`Rot`"), "no German aliases: {}", w[1]);
    // The warning sits at the argument.
    let d = &out.diagnostics[0];
    assert_eq!(&src[d.span.start as usize..d.span.end as usize], "\"lightgoldenrodyellow\"");
    // Stable: same name (any case) → same colour, across evaluations; recorded values.
    let a = rgba(&colors(src)[0].0);
    assert_eq!(a, rgba(&colors("color(\"LightGoldenrodYellow\") cube(1);")[0].0));
    assert_eq!(a, random_color("lightgoldenrodyellow"));
    assert_eq!(palette::hex_string(random_color("lightgoldenrodyellow")), "#a991ef");
    assert_eq!(palette::hex_string(random_color("rot")), "#a9342e");
    // Different names give different colours in the muted range, away from the palette.
    let mut seen: Vec<[u8; 3]> = Vec::new();
    for n in ["salmon", "navy", "gold", "olive", "maroon", "lavender", "coral", "khaki"] {
        let b = rgba_bytes(random_color(n));
        let rgb = [b[0], b[1], b[2]];
        let [l, c, _] = oklch(rgb);
        assert!((palette::RANDOM_L.0 - 0.01..=palette::RANDOM_L.1 + 0.01).contains(&l), "{n}: L {l}");
        assert!(c <= palette::RANDOM_C.1 + 0.005, "{n}: C {c}");
        for p in PALETTE.iter().map(|p| p.rgb).chain([palette::DEFAULT_RGB]) {
            assert!(delta_e(rgb, p) >= palette::RANDOM_MIN_PALETTE, "{n} too close to {p:?}");
        }
        assert!(!seen.contains(&rgb), "{n}");
        seen.push(rgb);
        assert_eq!(parse_color(n).unwrap().kind, ColorKind::Random);
    }
}

/// Bericht 12, F5: random colours of different names in one model keep a distance to each
/// other and to the palette (before: only the hue varied, `baz`/`violet` and `x`/`rose` were
/// equal and merged into one entry and slot).
#[test]
fn random_colours_do_not_collide_in_a_model() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/farben-pruefung/cases/randnames.scad");
    let src = std::fs::read_to_string(path).unwrap();
    let out = run(&src);
    let mut by_name: Vec<(String, [u8; 3])> = Vec::new();
    for n in &out.tree.nodes {
        if let Op::Color { rgba, name: Some(name), .. } = &n.op {
            let b = rgba_bytes(*rgba);
            let key = name.to_ascii_lowercase();
            match by_name.iter().find(|(k, _)| *k == key) {
                Some((_, c)) => assert_eq!(*c, [b[0], b[1], b[2]], "{name}: same name, same colour"),
                None => by_name.push((key, [b[0], b[1], b[2]])),
            }
        }
    }
    let random: Vec<_> = by_name.iter().filter(|(k, _)| palette::palette_color(k).is_none()).collect();
    assert_eq!(random.len(), 34, "{by_name:?}");
    for (i, (a, ca)) in random.iter().enumerate() {
        for p in PALETTE.iter().map(|p| p.rgb).chain([palette::DEFAULT_RGB]) {
            assert!(delta_e(*ca, p) >= palette::RANDOM_MIN_PALETTE - 1e-9, "{a} near palette {p:?}");
        }
        for (b, cb) in &random[i + 1..] {
            assert!(delta_e(*ca, *cb) >= palette::RANDOM_MIN_OTHER - 1e-9, "{a} / {b}: {}", delta_e(*ca, *cb));
        }
    }
    // the same names in another order: still distinct (values may move, never merge)
    let rev: String = src.lines().rev().collect::<Vec<_>>().join("\n");
    let out = run(&rev);
    let mut keys: Vec<[u8; 4]> = out
        .tree
        .nodes
        .iter()
        .filter_map(|n| match &n.op {
            Op::Color { rgba, name: Some(n), .. } if palette::palette_color(n).is_none() => Some(rgba_bytes(*rgba)),
            _ => None,
        })
        .collect();
    keys.sort_unstable();
    keys.dedup();
    assert_eq!(keys.len(), 34);
}

#[test]
fn hex_vectors_and_alpha_stay_exact() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let c = colors(
        "color(\"#12ab34\") cube(1); color(\"#abc\") cube(1); color(\"#11223380\") cube(1); color([0.1, 0.2, 0.3]) cube(1); color(\"teal\", alpha = 0.5) cube(1); color([1, 0, 0, 0.25]) cube(1);",
    );
    let b: Vec<[u8; 4]> = c.iter().map(|(o, _)| rgba_bytes(rgba(o))).collect();
    assert_eq!(b[0], [0x12, 0xab, 0x34, 255]);
    assert_eq!(b[1], [0xaa, 0xbb, 0xcc, 255]);
    assert_eq!(b[2], [0x11, 0x22, 0x33, 0x80]);
    assert_eq!(rgba(&c[3].0), [0.1, 0.2, 0.3, 1.0]);
    assert_eq!(b[4], [0x21, 0x72, 0x74, 128]);
    assert_eq!(rgba(&c[5].0), [1.0, 0.0, 0.0, 0.25]);
    let out = run("color(\"#zz\") cube(1);");
    assert!(has_warning(&out, "ist keine Farbe"));
}

#[test]
fn filament_parameter() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let out = run(
        "color(\"teal\", filament = 2) cube(1);\ncolor(\"red\") cube(1);\ncolor(\"teal\", filament = 3) cube(2);\ncolor(\"blue\", filament = 0) cube(1);\ncolor(\"blue\", filament = 1.5) cube(1);\ncolor(\"teal\", 1, 2) cube(3);",
    );
    let f: Vec<Option<u32>> = out
        .tree
        .nodes
        .iter()
        .filter_map(|n| match &n.op {
            Op::Color { filament, .. } => Some(*filament),
            _ => None,
        })
        .collect();
    assert_eq!(f, [Some(2), None, None, None, None, None]);
    let w = warnings(&out);
    assert!(w.iter().any(|m| m.contains("schon Filament 2") && m.contains("filament = 3")), "{w:?}");
    assert_eq!(w.iter().filter(|m| m.contains("ganze Zahl von 1 bis 16")).count(), 2, "{w:?}");
    // filament is not positional (OpenSCAD: color(c, alpha))
    assert!(w.iter().any(|m| m.contains("zu viele Argumente")), "{w:?}");
}

#[test]
fn children_inherit_the_colour() {
    let out = clean("color(\"blue\") { cube(1); color(\"pink\") sphere(1); }");
    let blue = palette::rgba_of(palette::palette_color("blue").unwrap().rgb);
    let pink = palette::rgba_of(palette::palette_color("pink").unwrap().rgb);
    let prims: Vec<_> = out.tree.nodes.iter().filter(|n| matches!(n.op, Op::Prim { .. })).map(|n| n.color).collect();
    assert_eq!(prims, [Some(blue), Some(pink)]);
}

fn min_pair(f: impl Fn([u8; 3]) -> [u8; 3]) -> (f64, &'static str, &'static str) {
    let mut best = (f64::INFINITY, "", "");
    for (i, a) in PALETTE.iter().enumerate() {
        for b in &PALETTE[i + 1..] {
            let d = delta_e(f(a.rgb), f(b.rgb));
            if d < best.0 {
                best = (d, a.name, b.name);
            }
        }
    }
    best
}

/// The palette stays apart in OKLab, also for simulated colour vision deficiencies. Minimum
/// ΔE_OK (about 0.02 is just noticeable): normal 0.107 (cyan/silver), deuteranopia 0.070
/// (red/green), protanopia 0.052 (orange/green), tritanopia 0.030 (teal/blue).
#[test]
fn palette_is_distinguishable() {
    let normal = min_pair(|c| c);
    let deut = min_pair(|c| simulate_cvd(c, Cvd::Deuteranopia));
    let prot = min_pair(|c| simulate_cvd(c, Cvd::Protanopia));
    let trit = min_pair(|c| simulate_cvd(c, Cvd::Tritanopia));
    println!("normal {normal:?}\ndeuteranopia {deut:?}\nprotanopia {prot:?}\ntritanopia {trit:?}");
    assert!(normal.0 >= 0.10, "{normal:?}");
    assert!(deut.0 >= 0.065, "{deut:?}");
    assert!(prot.0 >= 0.05, "{prot:?}");
    assert!(trit.0 >= 0.025, "{trit:?}");
    // muted: no pure colours, chroma at most 0.18
    for p in &PALETTE {
        let [_, c, _] = oklch(p.rgb);
        assert!(c <= 0.18, "{} too saturated: {c}", p.name);
        assert!(!p.rgb.iter().all(|&x| x == 0 || x == 255), "{} is a pure colour", p.name);
    }
}

/// Display variants: same hue, lightness adjusted only where needed; every palette colour
/// stands out from the 3D view background of its theme (contrast ≥ 1.25 with the shaded
/// faces, see `web/src/styles.css`: --view-bg).
#[test]
fn display_variants() {
    let light_bg = [0xe4, 0xe8, 0xed];
    let dark_bg = [0x16, 0x19, 0x1d];
    for p in &PALETTE {
        let l = display_rgb(p.rgb, Theme::Light);
        let d = display_rgb(p.rgb, Theme::Dark);
        let (h0, hl, hd) = (oklch(p.rgb)[2], oklch(l)[2], oklch(d)[2]);
        if oklch(p.rgb)[1] > 0.03 {
            assert!((h0 - hl).abs() < 2.0 && (h0 - hd).abs() < 2.0, "{}: hue {h0} {hl} {hd}", p.name);
        }
        assert!(oklch(d)[0] >= 0.355, "{} too dark for the dark theme", p.name);
        assert!(oklch(l)[0] <= 0.931, "{} too light for the light theme", p.name);
        let best_light = contrast_ratio(l, light_bg).max(contrast_ratio(scale(l, 0.7), light_bg));
        let best_dark = contrast_ratio(d, dark_bg);
        assert!(best_light >= 1.25, "{}: light {best_light}", p.name);
        assert!(best_dark >= 1.25, "{}: dark {best_dark}", p.name);
    }
    // non-palette colours are shown as they are
    assert_eq!(display_rgb([1, 2, 3], Theme::Dark), [1, 2, 3]);
}

/// A face lit at an angle (Lambert factor `k`).
fn scale(c: [u8; 3], k: f64) -> [u8; 3] {
    c.map(|x| (x as f64 * k).round() as u8)
}

/// Bericht 12, F2: `filament(c, n);` – the slot table of the model (top level, evaluated
/// first); invalid slots warn once; a user module named `filament` keeps working.
#[test]
fn filament_declarations() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let out = run("color(\"red\", filament = 2) cube(1);\nfilament(\"red\", 4);\nfilament(\"#2e65b9\", 17);\n");
    assert_eq!(out.tree.filaments.len(), 1);
    assert_eq!((out.tree.filaments[0].slot, out.tree.filaments[0].name.as_deref()), (4, Some("red")));
    let w = warnings(&out);
    assert_eq!(w.len(), 2, "{w:?}");
    assert!(w.iter().any(|x| x.contains("Slot muss eine ganze Zahl von 1 bis 16 sein")), "{w:?}");
    assert!(w.iter().any(|x| x.contains("Farbe hat schon Filament 4")), "the table counts first: {w:?}");
    let out = run("module filament(a, b) cube(b);\nfilament(\"red\", 3);\n");
    assert!(out.tree.filaments.is_empty());
    assert!(warnings(&out).is_empty(), "{:?}", warnings(&out));
    assert_eq!(out.tree.nodes.iter().filter(|n| matches!(n.op, Op::Prim { .. })).count(), 1);
}

#[test]
fn unknown_name_warns_in_english() {
    let out = run("color(\"lightgoldenrodyellow\") cube(1);");
    let w = warnings(&out);
    assert!(
        w[0].starts_with("colour `lightgoldenrodyellow` unknown – random colour (palette: red, orange"),
        "{}",
        w[0]
    );
}
