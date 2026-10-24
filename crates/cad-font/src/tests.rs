use super::*;
use cad_outline::FillRule;

fn lay(font: &str, text: &str, f: impl FnOnce(&mut TextParams)) -> Layout {
    let (font, _) = select(&FontName::parse(font));
    let mut p = TextParams { text: text.into(), ..Default::default() };
    f(&mut p);
    layout(&font, &p).unwrap()
}

fn area(l: &Layout) -> f64 {
    l.glyphs
        .iter()
        .map(|g| cad_outline::region_area(&cad_outline::normalize(&g.contours, FillRule::NonZero), 1e-4))
        .sum()
}

/// OpenSCAD 2021.01 with the same font files (`text(…)` exported as SVG, bounds and area of
/// the polygons; `tools/report/report16-openscad.sh`): (font, text, params, minx, miny, maxx,
/// maxy, area).
#[allow(clippy::type_complexity)]
const OPENSCAD: &[(&str, &str, &str, [f64; 5])] = &[
    ("Inter", "H", "", [1.22069, 0.0, 9.101, 10.1047, 32.0819]),
    ("Inter:style=Bold", "H", "", [0.915497, 0.0, 9.46049, 10.1047, 49.4211]),
    ("JetBrains Mono", "H", "", [1.2916, 0.0, 7.0412, 10.1382, 29.0443]),
    ("Allerta Stencil", "H", "", [1.27489, 0.0, 8.88339, 9.99559, 36.9995]),
    ("Inter", "AV", "", [0.352585, 0.0, 17.6607, 10.1047, 54.7969]),
    ("Inter", "O", "", [0.827393, -0.13559, 9.79279, 10.2404, 33.1630]),
    ("Inter", "Hello World", "", [1.22069, -0.162796, 73.3488, 10.1047, 229.351]),
    ("Inter", "fi", "", [0.13559, 0.0, 7.56322, 10.5794, 29.2161]),
    ("JetBrains Mono", "->", "", [0.985687, 0.486099, 15.4292, 8.67999, 26.0734]),
    ("Inter", "H", "hc vc", [-3.81918, -4.93437, 4.0611, 5.17032, 32.0816]),
    ("Inter", "H", "vt", [1.22069, -9.86874, 9.101, 0.235947, 32.0818]),
    ("Inter", "Hg", "vb", [1.22069, -0.0662384, 17.5261, 13.0359, 66.0707]),
    ("Inter", "Hg", "vc", [1.22069, -6.46625, 17.5261, 6.63594, 66.0709]),
    ("Inter", "HH", "s2", [1.22069, 0.0, 29.2606, 10.1047, 64.1637]),
    ("Inter", "HH", "ttb", [-3.81914, -29.5507, 4.06114, -3.03484, 64.1636]),
    ("Inter", "H", "hr", [-8.85909, 0.0, -0.978775, 10.1047, 32.0820]),
    ("JetBrains Mono", "H", "hc", [-2.77739, 0.0, 2.9722, 10.1382, 29.0443]),
    ("JetBrains Mono", "H", "vt", [1.2916, -9.90625, 7.0412, 0.231949, 29.0443]),
    ("Allerta Stencil", "H", "vt", [1.27489, -9.7625, 8.88339, 0.233093, 36.9994]),
    ("Inter", "H", "size20 vt", [2.44139, -19.7375, 18.202, 0.471893, 128.3275]),
];

fn apply(p: &mut TextParams, flags: &str) {
    for f in flags.split_whitespace() {
        match f {
            "hc" => p.halign = HAlign::Center,
            "hr" => p.halign = HAlign::Right,
            "vc" => p.valign = VAlign::Center,
            "vt" => p.valign = VAlign::Top,
            "vb" => p.valign = VAlign::Bottom,
            "s2" => p.spacing = 2.0,
            "ttb" => p.direction = Some(Dir::Ttb),
            "size20" => p.size = 20.0,
            _ => panic!("{f}"),
        }
    }
}

#[test]
fn hull_and_area_like_openscad() {
    for (font, text, flags, [x0, y0, x1, y1, a]) in OPENSCAD {
        let l = lay(font, text, |p| apply(p, flags));
        let (lo, hi) = l.bounds().unwrap();
        let what = format!("{font} {text:?} {flags}: {lo} {hi}");
        // bounds within 0.01 mm (OpenSCAD writes 6 digits, its metrics are grid-fitted)
        for (got, want) in [(lo.x, x0), (lo.y, y0), (hi.x, x1), (hi.y, y1)] {
            assert!((got - want).abs() < 0.01, "{what}: {got} vs {want}");
        }
        // area within 0.5 % (OpenSCAD flattens the curves)
        let ar = area(&l);
        assert!((ar - a).abs() / a < 0.005, "{what}: area {ar} vs {a}");
    }
}

#[test]
fn glyph_o_has_a_hole() {
    let l = lay("Inter", "O", |_| {});
    assert_eq!(l.glyphs.len(), 1);
    let n = cad_outline::normalize(&l.glyphs[0].contours, FillRule::NonZero);
    assert_eq!(n.len(), 2);
    let areas: Vec<f64> = n.iter().map(|c| c.signed_area(1e-4)).collect();
    assert!(areas.iter().any(|a| *a > 0.0) && areas.iter().any(|a| *a < 0.0), "outer and hole: {areas:?}");
    // TrueType: quadratic curves
    assert!(l.glyphs[0].contours[0].segs.iter().any(|s| matches!(s, Seg::Quad(_))));
}

#[test]
fn kerning_pair_av() {
    let a = lay("Inter", "A", |_| {}).advance.x;
    let v = lay("Inter", "V", |_| {}).advance.x;
    let av = lay("Inter", "AV", |_| {}).advance.x;
    assert!(av < a + v - 0.1, "AV is kerned: {av} < {a} + {v}");
    // monospaced font without kerning
    let a = lay("JetBrains Mono", "A", |_| {}).advance.x;
    let av = lay("JetBrains Mono", "AV", |_| {}).advance.x;
    assert!((av - 2.0 * a).abs() < 1e-9);
}

#[test]
fn align_and_spacing() {
    let base = lay("Inter", "Hg", |_| {});
    let adv = base.advance.x;
    let c = lay("Inter", "Hg", |p| p.halign = HAlign::Center);
    assert!((c.origin.x + adv / 2.0).abs() < 1e-12);
    let r = lay("Inter", "Hg", |p| p.halign = HAlign::Right);
    assert!((r.origin.x + adv).abs() < 1e-12);
    let (lo, hi) = base.bounds().unwrap();
    let t = lay("Inter", "Hg", |p| p.valign = VAlign::Top);
    assert!((t.origin.y + hi.y * OPENSCAD_POSITION_SCALE).abs() < 1e-9);
    let b = lay("Inter", "Hg", |p| p.valign = VAlign::Bottom);
    assert!((b.origin.y + lo.y * OPENSCAD_POSITION_SCALE).abs() < 1e-9);
    let s = lay("Inter", "Hg", |p| p.spacing = 1.5);
    assert!((s.advance.x - 1.5 * adv).abs() < 1e-9);
    let big = lay("Inter", "Hg", |p| p.size = 20.0);
    assert!((big.advance.x - 2.0 * adv).abs() < 1e-9);
    assert!((area(&big) - 4.0 * area(&base)).abs() < 1e-3 * area(&big));
}

#[test]
fn directions() {
    let ltr = lay("Inter", "HI", |_| {});
    let rtl = lay("Inter", "HI", |p| p.direction = Some(Dir::Rtl));
    assert!((ltr.advance.x - rtl.advance.x).abs() < 1e-9);
    // right to left: the first character is placed last (on the right)
    assert!(rtl.glyphs[0].cluster > rtl.glyphs[1].cluster, "visual order");
    let ttb = lay("Inter", "HH", |p| p.direction = Some(Dir::Ttb));
    assert!(ttb.advance.y < 0.0 && ttb.advance.x == 0.0);
    let btt = lay("Inter", "HH", |p| p.direction = Some(Dir::Btt));
    // OpenSCAD 2021.01 gives the same for "HH" bottom to top (h21 in the report)
    let (a, b) = (ttb.bounds().unwrap(), btt.bounds().unwrap());
    assert!(a.0.distance(b.0) < 1e-9 && a.1.distance(b.1) < 1e-9);
    // Hebrew is laid out right to left without a direction
    let he = lay("Inter", "\u{5d0}\u{5d1}", |_| {});
    assert!(he.missing.len() == 2 || he.glyphs.len() == 2 && he.glyphs[0].cluster > he.glyphs[1].cluster);
    // script and language
    assert!(
        layout(
            &Font::builtin(&BUILTIN[0]),
            &TextParams {
                text: "x".into(),
                script: Some("latin".into()),
                language: Some("de".into()),
                ..Default::default()
            }
        )
        .is_ok()
    );
    assert!(
        layout(
            &Font::builtin(&BUILTIN[0]),
            &TextParams { text: "x".into(), script: Some("nope!".into()), ..Default::default() }
        )
        .is_err()
    );
}

#[test]
fn font_names() {
    assert_eq!(FontName::parse("Inter:style=Bold"), FontName { family: "Inter".into(), style: Some("Bold".into()) });
    assert_eq!(builtin(&FontName::parse("inter:style=bold")).unwrap().0.style, "Bold");
    assert_eq!(builtin(&FontName::parse("JetBrainsMono")).unwrap().0.family, "JetBrains Mono");
    let (b, p) = builtin(&FontName::parse("Inter:style=Black")).unwrap();
    assert_eq!((b.style, p), ("Regular", Pick::StyleMissing));
    assert!(builtin(&FontName::parse("Comic Sans")).is_none());
    let names = |n: &str| {
        let (f, p) = select(&FontName::parse(n));
        (f.names().0, f.names().1, p)
    };
    // the stencil font: default, own name, unknown families
    let stencil = |sty: &str, p: Pick| (STENCIL_FAMILY.to_string(), sty.to_string(), p);
    assert_eq!(names(""), stencil("Regular", Pick::Exact));
    assert_eq!(names("cad stencil mono"), stencil("Regular", Pick::Exact));
    assert_eq!(names("CadStencilMono:style=Bold"), stencil("Bold", Pick::Exact));
    assert_eq!(names("cad stencil mono:style=Thin"), stencil("Regular", Pick::StyleMissing));
    assert_eq!(names("Comic Sans"), stencil("Regular", Pick::Fallback));
    assert_eq!(names("Inter:style=Bold"), ("Inter".to_string(), "Bold".to_string(), Pick::Exact));
    assert!(select(&FontName::parse("")).0.is_stencil() && DEFAULT_FAMILY == STENCIL_FAMILY);
    assert!(FontName::is_file("fonts/Meine.TTF") && FontName::is_file("x.otf") && !FontName::is_file("Inter"));
    assert_eq!(builtin_names().len(), BUILTIN.len() + 2);
    assert_eq!(builtin_names()[0], "cad stencil mono:style=Regular");
}

#[test]
fn user_font_data() {
    let data: Arc<[u8]> = BUILTIN[4].data.into();
    let f = Font::from_data(data, 0).unwrap();
    assert_eq!(f.names(), ("Allerta Stencil".to_string(), "Regular".to_string()));
    let l = layout(&f, &TextParams { text: "H".into(), ..Default::default() }).unwrap();
    assert_eq!(l.glyphs.len(), 1);
    assert!(Font::from_data(Arc::from(&b"no font"[..]), 0).is_err());
    // missing glyphs are reported
    let l = lay("Allerta Stencil", "H\u{4e2d}", |_| {});
    assert_eq!(l.missing, vec!['\u{4e2d}']);
}

/// CFF outlines (cubic): the system's Inter OTF if installed.
#[test]
fn cff_outlines_are_cubic() {
    let Ok(data) = std::fs::read("/usr/share/fonts/opentype/inter/Inter-Regular.otf") else { return };
    let f = Font::from_data(data.into(), 0).unwrap();
    let l = layout(&f, &TextParams { text: "O".into(), ..Default::default() }).unwrap();
    assert!(l.glyphs[0].contours.iter().flat_map(|c| &c.segs).any(|s| matches!(s, Seg::Cubic(_))));
    let ttf = lay("Inter", "O", |_| {});
    assert!((area(&l) - area(&ttf)).abs() / area(&ttf) < 0.01, "same design: {} vs {}", area(&l), area(&ttf));
}

/// Every bundled font – the stencil font included – has a real glyph for every character of
/// the required set, except the documented gaps of Allerta Stencil, which come from JetBrains
/// Mono; nothing is `.notdef`.
#[test]
fn required_chars_in_every_font() {
    let fonts = BUILTIN.iter().map(Font::builtin).chain([Font::stencil(false), Font::stencil(true)]);
    for f in fonts {
        let (fam, sty) = f.names();
        let miss: String = missing_chars(&f, REQUIRED_CHARS).into_iter().collect();
        let gaps = if fam == "Allerta Stencil" { ALLERTA_STENCIL_GAPS } else { "" };
        assert_eq!(miss, gaps, "{fam} {sty}");
        let l = layout(&f, &TextParams { text: REQUIRED_CHARS.into(), ..Default::default() }).unwrap();
        assert!(l.missing.is_empty(), "{fam}");
        assert_eq!(l.fallback.iter().collect::<String>(), gaps);
        assert!(l.glyphs.iter().all(|g| g.id != 0));
        // each character alone gives an outline (JetBrains Mono joins some pairs to ligatures)
        for c in REQUIRED_CHARS.chars().filter(|c| *c != ' ') {
            let l = layout(&f, &TextParams { text: c.to_string(), ..Default::default() }).unwrap();
            assert!(l.glyphs.len() == 1 && l.glyphs[0].id != 0, "{fam} {c:?}");
            assert_eq!(l.glyphs[0].fallback, gaps.contains(c), "{fam} {c:?}");
        }
    }
    // the fallback has everything
    assert!(missing_chars(&fallback_font(&Font::builtin(&BUILTIN[0])), REQUIRED_CHARS).is_empty());
}

/// A character the font lacks comes from JetBrains Mono, at its place in the line.
#[test]
fn fallback_to_jetbrains_mono() {
    let (b, _) = builtin(&FontName::parse("Allerta Stencil")).unwrap();
    let f = Font::builtin(b);
    let ch = ['Ж', 'Ω', '→', '∑'].into_iter().find(|c| !missing_chars(&f, &c.to_string()).is_empty()).unwrap();
    let fb = fallback_font(&f);
    assert!(missing_chars(&fb, &ch.to_string()).is_empty(), "JetBrains Mono has {ch}");
    let text = format!("A{ch}B");
    let l = layout(&f, &TextParams { text, ..Default::default() }).unwrap();
    assert_eq!(l.fallback, vec![ch]);
    assert!(l.missing.is_empty());
    assert_eq!(l.glyphs.len(), 3);
    assert!(l.glyphs[1].fallback && !l.glyphs[0].fallback && !l.glyphs[2].fallback);
    // B is placed after the fallback glyph's advance
    let ab = layout(&f, &TextParams { text: "AB".into(), ..Default::default() }).unwrap();
    assert!(l.advance.x > ab.advance.x);
    // a character no font has: .notdef of the font, reported
    let l = layout(&f, &TextParams { text: "A\u{e000}".into(), ..Default::default() }).unwrap();
    assert_eq!(l.missing, vec!['\u{e000}']);
}

// ------------------------------------------------------------------------ cad stencil mono

/// Parameter combinations (stroke, bridge as fractions of `size`) the stencil glyphs are
/// checked with: defaults regular/bold, the limits and some in between.
const STENCIL_PARAMS: &[(f64, f64)] = &[
    (stencil::DEFAULT_STROKE, stencil::DEFAULT_BRIDGE),
    (stencil::DEFAULT_STROKE_BOLD, stencil::DEFAULT_BRIDGE),
    (0.03, stencil::MIN_BRIDGE),
    (0.08, 0.05),
    (0.1, stencil::MAX_BRIDGE),
    (0.17, 0.15),
    (stencil::MAX_STROKE, stencil::MIN_BRIDGE),
    (stencil::MAX_STROKE, stencil::MAX_BRIDGE),
];

/// Every glyph is a valid region without holes (Plan 17.4: no islands): closed loops of lines
/// and arcs, counter-clockwise (no clockwise hole loops), no self-overlaps (a union of the
/// loops gives the same region), no slivers.
#[test]
fn stencil_glyphs_have_no_holes() {
    use cad_kernel::Segment;
    use cad_kernel::region2::{self, Fill, Operand};
    for &(sf, bf) in STENCIL_PARAMS {
        let (w, bw) = (sf * stencil::CAP, bf * stencil::CAP);
        for ch in stencil::CHARS.chars().filter(|c| *c != ' ') {
            let what = format!("{ch:?} stroke {sf} bridge {bf}");
            let loops = stencil::region(ch, w, bw).unwrap();
            assert!(!loops.is_empty(), "{what}: empty");
            for lp in &loops {
                let a = region2::loop_area(lp);
                assert!(a > 0.0, "{what}: hole (loop area {a})");
                // pieces are at least a fifth of a stroke square (dots are one square)
                assert!(a > 0.2 * w * w, "{what}: sliver of area {a}");
                for (i, s) in lp.iter().enumerate() {
                    assert!(!matches!(s, Segment::Nurbs(_)), "{what}: lines and arcs only");
                    let next = &lp[(i + 1) % lp.len()];
                    assert!(s.end().distance(next.start()) < 1e-6, "{what}: loop not closed");
                }
            }
            let total: f64 = loops.iter().map(|l| region2::loop_area(l)).sum();
            let again = region2::combine(&[Operand::new(loops.clone(), Fill::NonZero)], &|i: &[bool]| i[0]);
            let total2: f64 = again.iter().map(|l| region2::loop_area(l)).sum();
            assert!((total - total2).abs() < 1e-6 * total, "{what}: self-overlap {total} vs {total2}");
            assert_eq!(again.len(), loops.len(), "{what}: loops touch or overlap");
        }
    }
}

/// Without bridges (`bridge_width = 0`) the counters stay closed, like a normal font.
#[test]
fn stencil_without_bridges_has_counters() {
    let holes = |ch: char| {
        let r = stencil::region(ch, 100.0, 0.0).unwrap();
        r.iter().filter(|l| cad_kernel::region2::loop_area(l) < 0.0).count()
    };
    assert_eq!((holes('O'), holes('B'), holes('8'), holes('A'), holes('e'), holes('H')), (1, 2, 2, 1, 1, 0));
    // with bridges: O in two halves, B stem and bowls apart
    let pieces = |ch: char| stencil::region(ch, 100.0, 72.0).unwrap().len();
    assert_eq!((pieces('O'), pieces('B'), pieces('H'), pieces('i')), (2, 2, 1, 2));
}

fn stencil_lay(text: &str, f: impl FnOnce(&mut TextParams)) -> Layout {
    lay("", text, f)
}

/// Metrics as OpenSCAD's `size`: cap height = `size`, monospaced advance 600/1000 em with
/// OpenSCAD's 1000/1024 like JetBrains Mono, every glyph inside its cell.
#[test]
fn stencil_metrics_and_monospaced_advance() {
    let size = 10.0;
    let k = size / SIZE_TO_EM / stencil::UPEM;
    let adv = stencil::ADVANCE * k * OPENSCAD_POSITION_SCALE;
    let jb = lay("JetBrains Mono", "H", |_| {}).advance.x;
    assert!((adv - jb).abs() < 1e-9, "advance like JetBrains Mono: {adv} vs {jb}");
    let (lo, hi) = stencil_lay("H", |_| {}).bounds().unwrap();
    assert!(lo.y.abs() < 1e-9 && (hi.y - size).abs() < 1e-9, "cap height = size: {lo} {hi}");
    let (_, hx) = stencil_lay("x", |_| {}).bounds().unwrap();
    assert!((hx.y - stencil::XHEIGHT * k).abs() < 1e-9);
    for ch in stencil::CHARS.chars() {
        let l = stencil_lay(&ch.to_string(), |_| {});
        assert!((l.advance.x - adv).abs() < 1e-9, "{ch:?}: advance {}", l.advance.x);
        if let Some((lo, hi)) = l.bounds() {
            assert!(lo.x >= -1e-9 && hi.x <= stencil::ADVANCE * k + 1e-9, "{ch:?} outside its cell: {lo} {hi}");
            assert!(lo.y >= stencil::DESCENDER * k - 1e-9 && hi.y <= 1.25 * size, "{ch:?}: {lo} {hi}");
        }
    }
    // a line of n characters is n advances long, with spacing
    let line = stencil_lay("Hallo Welt 42", |_| {});
    assert!((line.advance.x - 13.0 * adv).abs() < 1e-9);
    let wide = stencil_lay("Hallo", |p| p.spacing = 1.5);
    assert!((wide.advance.x - 7.5 * adv).abs() < 1e-9);
    assert_eq!(line.glyphs.len(), 11, "no glyph for spaces");
}

/// Areas: exact for H (two stems and a bar), plausible against JetBrains Mono, growing with
/// the stroke width, bridges remove a little.
#[test]
fn stencil_areas() {
    let size = 10.0;
    let k = size / SIZE_TO_EM / stencil::UPEM;
    let w = stencil::DEFAULT_STROKE * stencil::CAP;
    let (l, r) = (60.0 + w / 2.0, 540.0 - w / 2.0);
    let h_exact = (2.0 * w * stencil::CAP + (r - l - w) * w) * k * k;
    let h = area(&stencil_lay("H", |_| {}));
    assert!((h - h_exact).abs() < 1e-9 * h_exact, "H: {h} vs {h_exact}");
    // the alphabet against JetBrains Mono Regular
    let abc = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let (s, j) = (area(&stencil_lay(abc, |_| {})), area(&lay("JetBrains Mono", abc, |_| {})));
    assert!(s / j > 0.9 && s / j < 1.5, "stencil {s} vs JetBrains Mono {j}");
    // o: ring of width w along the centre line, minus two bridges
    let o = area(&stencil_lay("o", |_| {}));
    let rr = (r - l) / 2.0;
    let len = 2.0 * std::f64::consts::PI * rr + 2.0 * ((stencil::XHEIGHT - w) - 2.0 * rr);
    let ring = len * w * k * k;
    let bridges = 2.0 * stencil::DEFAULT_BRIDGE * stencil::CAP * w * k * k;
    assert!((o - (ring - bridges)).abs() < 0.03 * ring, "o: {o} vs {}", ring - bridges);
    // heavier strokes, wider bridges; bold
    let thick = area(&stencil_lay(abc, |p| p.stroke_width = Some(1.8)));
    assert!(thick > 1.15 * s);
    let wide = area(&stencil_lay(abc, |p| p.bridge_width = Some(1.6)));
    assert!(wide < s && wide > 0.85 * s);
    let bold = area(&lay("cad stencil mono:style=Bold", abc, |_| {}));
    let as_bold = area(&stencil_lay(abc, |p| p.stroke_width = Some(stencil::DEFAULT_STROKE_BOLD * size)));
    assert!((bold - as_bold).abs() < 1e-9 * bold && bold > 1.2 * s, "bold {bold} vs {s}");
    // stroke width in mm: the default equals 0.14 × size; it scales with the size
    let explicit = area(&stencil_lay(abc, |p| p.stroke_width = Some(1.4)));
    assert!((explicit - s).abs() < 1e-9 * s);
    let big = area(&stencil_lay(abc, |p| p.size = 20.0));
    assert!((big - 4.0 * s).abs() < 1e-4 * big, "{big} vs 4 × {s}");
    // limits: clamped
    let p = TextParams { stroke_width: Some(9.0), bridge_width: Some(9.0), ..Default::default() };
    let (w_max, b_max) = stencil_widths(false, &p);
    assert!((w_max - stencil::MAX_STROKE * stencil::CAP).abs() < 1e-9);
    assert!((b_max - stencil::MAX_BRIDGE * stencil::CAP).abs() < 1e-9);
    let (_, b_min) = stencil_widths(false, &TextParams { bridge_width: Some(0.01), ..Default::default() });
    assert!((b_min - stencil::MIN_BRIDGE * stencil::CAP).abs() < 1e-9);
    let (_, b0) = stencil_widths(false, &TextParams { bridge_width: Some(0.0), ..Default::default() });
    assert_eq!(b0, 0.0);
}

/// Alignment, directions and the fallback work with the stencil font as with the others.
#[test]
fn stencil_layout_features() {
    let base = stencil_lay("Hg", |_| {});
    let c = stencil_lay("Hg", |p| p.halign = HAlign::Center);
    assert!((c.origin.x + base.advance.x / 2.0).abs() < 1e-12);
    let t = stencil_lay("Hg", |p| p.valign = VAlign::Top);
    let (_, hi) = t.bounds().unwrap();
    assert!(hi.y.abs() < 0.3, "top: {hi}");
    let rtl = stencil_lay("HI", |p| p.direction = Some(Dir::Rtl));
    assert!(
        rtl.glyphs[0].cluster > rtl.glyphs[1].cluster
            && (rtl.advance.x - stencil_lay("HI", |_| {}).advance.x).abs() < 1e-9
    );
    let ttb = stencil_lay("HH", |p| p.direction = Some(Dir::Ttb));
    assert!(ttb.advance.y < 0.0 && ttb.advance.x == 0.0);
    let (lo, hi) = ttb.bounds().unwrap();
    assert!(lo.x < 0.0 && hi.x > 0.0, "centred on the pen: {lo} {hi}");
    // characters the stencil font lacks: JetBrains Mono, reported
    let l = stencil_lay("AЖB", |_| {});
    assert_eq!(l.fallback, vec!['Ж']);
    assert!(l.glyphs[1].fallback && !l.glyphs[0].fallback);
    assert_eq!(missing_chars(&Font::stencil(false), "AЖ€"), vec!['Ж']);
    assert_eq!(fallback_font(&Font::stencil(true)).names(), ("JetBrains Mono".into(), "Bold".into()));
    // an unknown script is an error as with the other fonts
    let p = TextParams { text: "x".into(), script: Some("nope!".into()), ..Default::default() };
    assert!(layout(&Font::stencil(false), &p).is_err());
}
