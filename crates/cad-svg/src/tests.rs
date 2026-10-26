use super::*;
use std::f64::consts::PI;

fn doc(attrs: &str, body: &str) -> String {
    format!("<svg xmlns=\"http://www.w3.org/2000/svg\" {attrs}>\n{body}\n</svg>\n")
}

fn area(s: &Svg) -> f64 {
    s.shapes.iter().map(|sh| cad_outline::region_area(&sh.normalized(), 1e-5)).sum()
}

fn bounds(s: &Svg) -> (DVec2, DVec2) {
    s.bounds().expect("geometry")
}

fn near(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

/// Reference values from OpenSCAD 2021.01 (`import()` exported as SVG): with a viewBox
/// OpenSCAD and the import agree.
#[test]
fn units_viewbox_like_openscad() {
    let rect = r#"<rect x="10" y="5" width="20" height="10"/>"#;
    // width/height in mm, viewBox 2:1 → 0.5 mm per user unit; y flipped about the height
    let s = parse(&doc(r#"width="100mm" height="50mm" viewBox="0 0 200 100""#, rect), &Options::default()).unwrap();
    let (lo, hi) = bounds(&s);
    assert!(near(lo.x, 5.0, 1e-9) && near(hi.x, 15.0, 1e-9));
    assert!(near(lo.y, 42.5, 1e-9) && near(hi.y, 47.5, 1e-9), "{lo} {hi}");
    // px are CSS pixels (96 dpi): OpenSCAD e3
    let s = parse(&doc(r#"width="100px" height="50px" viewBox="0 0 100 50""#, rect), &Options::default()).unwrap();
    let (lo, hi) = bounds(&s);
    assert!(near(lo.x, 2.64583, 1e-5) && near(hi.x, 7.9375, 1e-5));
    assert!(near(lo.y, 9.26041, 1e-5) && near(hi.y, 11.9062, 1e-4), "{lo} {hi}");
    // unitless numbers follow dpi: OpenSCAD e2 (dpi 72)
    let o72 = Options { dpi: 72.0, ..Default::default() };
    let s = parse(&doc(r#"width="100" height="50" viewBox="0 0 100 50""#, rect), &o72).unwrap();
    let (lo, hi) = bounds(&s);
    assert!(near(lo.x, 3.52777, 1e-5) && near(hi.x, 10.5833, 1e-4));
    assert!(near(lo.y, 12.3472, 1e-4) && near(hi.y, 15.875, 1e-4));
    // viewBox only: width = viewBox width in pixels (OpenSCAD e4)
    let s = parse(&doc(r#"viewBox="0 0 100 50""#, rect), &o72).unwrap();
    assert!(near(bounds(&s).0.x, 3.52777, 1e-5));
    // viewBox origin: x as OpenSCAD (e10); OpenSCAD 2021.01 adds the y offset instead of
    // subtracting it (10.58…14.11), the SVG rule gives 14.11…17.64
    let s = parse(&doc(r#"width="100" height="50" viewBox="10 5 100 50""#, rect), &o72).unwrap();
    let (lo, hi) = bounds(&s);
    assert!(near(lo.x, 0.0, 1e-9) && near(hi.x, 7.05554, 1e-4), "{lo} {hi}");
    assert!(near(lo.y, 14.1111, 1e-4) && near(hi.y, 17.6389, 1e-4), "{lo} {hi}");
    // inches
    let s = parse(&doc(r#"width="1in" height="0.5in" viewBox="0 0 100 50""#, rect), &o72).unwrap();
    assert!(near(bounds(&s).0.x, 2.54, 1e-9) && near(bounds(&s).1.y, 11.43, 1e-9));
    // cm and pt
    assert!(near(length_mm("10cm", 1.0).unwrap(), 100.0, 1e-12));
    assert!(near(length_mm("72pt", 1.0).unwrap(), 25.4, 1e-12));
    assert!(near(length_mm("6pc", 1.0).unwrap(), 25.4, 1e-12));
    assert!(length_mm("50%", 1.0).is_none());
}

#[test]
fn units_without_viewbox_are_pixels() {
    let rect = r#"<rect x="0" y="0" width="96" height="48"/>"#;
    let s = parse(&doc(r#"width="96" height="48""#, rect), &Options::default()).unwrap();
    let (lo, hi) = bounds(&s);
    assert!(near(hi.x - lo.x, 25.4, 1e-9) && near(hi.y - lo.y, 12.7, 1e-9));
    assert!(near(lo.y, 0.0, 1e-9), "the rect fills the document: origin at the lower left");
    // scale and dpi
    let s = parse(&doc(r#"width="96" height="48""#, rect), &Options { dpi: 25.4, scale: 2.0, ..Default::default() })
        .unwrap();
    assert!(near(bounds(&s).1.x, 192.0, 1e-9));
    // center
    let s = parse(
        &doc("", r#"<rect x="10" y="10" width="20" height="10"/>"#),
        &Options { center: true, ..Default::default() },
    )
    .unwrap();
    let (lo, hi) = bounds(&s);
    assert!(near(lo.x, -hi.x, 1e-9) && near(lo.y, -hi.y, 1e-9));
}

#[test]
fn shapes_exact() {
    let o = Options { dpi: 25.4, ..Default::default() }; // 1 user unit = 1 mm
    let s = parse(&doc("", r#"<circle cx="10" cy="10" r="5"/>"#), &o).unwrap();
    assert!(s.shapes[0].contours[0].segs.iter().all(|g| matches!(g, Seg::Arc { .. })), "circle as arcs");
    assert!(near(area(&s), PI * 25.0, 1e-3));
    let s = parse(&doc("", r#"<ellipse cx="10" cy="10" rx="5" ry="2"/>"#), &o).unwrap();
    assert!(s.shapes[0].contours[0].segs.iter().all(|g| matches!(g, Seg::Ellipse { .. })));
    assert!(near(area(&s), PI * 10.0, 1e-3));
    let s = parse(&doc("", r#"<rect width="20" height="10" rx="2"/>"#), &o).unwrap();
    assert!(near(area(&s), 200.0 - (4.0 - PI) * 4.0, 1e-3), "{}", area(&s));
    assert!(s.shapes[0].contours[0].segs.iter().any(|g| matches!(g, Seg::Arc { .. })), "rx = ry: circular corners");
    let s = parse(&doc("", r#"<rect width="20" height="10" rx="4" ry="2"/>"#), &o).unwrap();
    assert!(near(area(&s), 200.0 - (4.0 - PI) * 8.0, 1e-3));
    let s = parse(&doc("", r#"<rect width="20" height="10" rx="40"/>"#), &o).unwrap();
    assert!(near(area(&s), 200.0 - (4.0 - PI) * 50.0, 1e-3), "rx = ry = 40 clamped to 10 and 5");
    let s = parse(&doc("", r#"<polygon points="0,0 10,0 10,10"/>"#), &o).unwrap();
    assert!(near(area(&s), 50.0, 1e-9));
}

#[test]
fn transforms() {
    let o = Options { dpi: 25.4, ..Default::default() };
    let c = r#"<circle cx="0" cy="0" r="1"/>"#;
    // translate + non-uniform scale: an ellipse
    let s = parse(&doc("", &format!(r#"<g transform="translate(10,20) scale(3,2)">{c}</g>"#)), &o).unwrap();
    let (lo, hi) = bounds(&s);
    assert!(near(lo.x, 7.0, 1e-3) && near(hi.x, 13.0, 1e-3));
    assert!(near(lo.y, -22.0, 1e-3) && near(hi.y, -18.0, 1e-3), "{lo} {hi}");
    assert!(near(area(&s), 6.0 * PI, 1e-3));
    // rotate about a point, uniform scale: stays a circle
    let s = parse(&doc("", &format!(r#"<g transform="rotate(90 5 0) scale(2)">{c}</g>"#)), &o).unwrap();
    assert!(s.shapes[0].contours[0].segs.iter().all(|g| matches!(g, Seg::Arc { .. })));
    let (lo, hi) = bounds(&s);
    assert!(near((lo.x + hi.x) / 2.0, 5.0, 1e-3) && near((lo.y + hi.y) / 2.0, 5.0, 1e-3), "{lo} {hi}");
    // matrix and skew
    let m = parse_transform("matrix(1 2 3 4 5 6)").unwrap();
    assert_eq!(m.transform_point2(DVec2::new(1.0, 1.0)), DVec2::new(9.0, 12.0));
    let k = parse_transform("skewX(45)").unwrap();
    assert!(k.transform_point2(DVec2::new(0.0, 1.0)).distance(DVec2::new(1.0, 1.0)) < 1e-12);
    let k = parse_transform("skewY(45)").unwrap();
    assert!(k.transform_point2(DVec2::new(1.0, 0.0)).distance(DVec2::new(1.0, 1.0)) < 1e-12);
    // list: right to left
    let t = parse_transform("translate(10) scale(2)").unwrap();
    assert_eq!(t.transform_point2(DVec2::new(1.0, 0.0)), DVec2::new(12.0, 0.0));
    assert!(parse_transform("rotate(1 2)").is_err());
    // a skewed square keeps its area
    let s = parse(&doc("", r#"<rect width="10" height="10" transform="skewX(30)"/>"#), &o).unwrap();
    assert!(near(area(&s), 100.0, 1e-9));
    // nested transforms multiply
    let s = parse(
        &doc("", r#"<g transform="scale(2)"><g transform="translate(1,0)"><rect width="1" height="1"/></g></g>"#),
        &o,
    )
    .unwrap();
    let (lo, hi) = bounds(&s);
    assert!(near(lo.x, 2.0, 1e-12) && near(hi.x, 4.0, 1e-12));
}

#[test]
fn fill_rules() {
    let o = Options { dpi: 25.4, ..Default::default() };
    let d = "M0 0 L10 0 L10 10 L0 10 Z M2 2 L8 2 L8 8 L2 8 Z";
    let eo = parse(&doc("", &format!(r#"<path d="{d}" fill-rule="evenodd"/>"#)), &o).unwrap();
    assert!(near(area(&eo), 64.0, 1e-9));
    let nz = parse(&doc("", &format!(r#"<path d="{d}"/>"#)), &o).unwrap();
    assert!(near(area(&nz), 100.0, 1e-9), "nonzero: same direction fills");
    let st = parse(&doc("", &format!(r#"<g style="fill-rule:evenodd"><path d="{d}"/></g>"#)), &o).unwrap();
    assert!(near(area(&st), 64.0, 1e-9), "inherited from style");
    // inner square reversed: a hole under nonzero too
    let rev = parse(&doc("", r#"<path d="M0 0 L10 0 L10 10 L0 10 Z M2 2 L2 8 L8 8 L8 2 Z"/>"#), &o).unwrap();
    assert!(near(area(&rev), 64.0, 1e-9));
}

#[test]
fn hidden_and_unfilled() {
    let o = Options { dpi: 25.4, ..Default::default() };
    let body = r#"<rect width="1" height="1" display="none"/>
<g style="display:none"><rect width="5" height="5"/></g>
<rect width="2" height="2" fill="none" stroke="black"/>
<rect width="3" height="3" visibility="hidden"/>
<rect width="4" height="4"/>"#;
    let s = parse(&doc("", body), &o).unwrap();
    assert_eq!(s.shapes.len(), 1);
    assert!(near(area(&s), 16.0, 1e-9));
    assert!(s.warnings.iter().any(|w| w.msg.contains("fill=\"none\"") && w.line == 4), "{:?}", s.warnings);
    assert!(s.warnings.iter().any(|w| w.msg.contains("stroke")));
}

#[test]
fn open_paths_are_paths() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let o = Options { dpi: 25.4, ..Default::default() };
    let body = r#"<line x1="0" y1="0" x2="10" y2="0"/>
<polyline points="0,0 5,5 10,0"/>
<path d="M0 0 L5 5"/>
<path d="M0 0 L5 0 L5 5 L0 0"/>"#;
    let s = parse(&doc("", body), &o).unwrap();
    assert_eq!(s.paths.len(), 3);
    assert_eq!(s.shapes.len(), 1, "a path ending at its start is closed");
    assert!(s.warnings.iter().any(|w| w.msg.contains("offener Pfad") && w.line == 2), "{:?}", s.warnings);
}

#[test]
fn unsupported_warns_with_line() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let body = r##"<style>.a { fill: red }</style>
<text x="0" y="10">Hallo</text>
<image href="a.png" width="10" height="10"/>
<use href="#r"/>
<rect class="a" width="1" height="1" clip-path="url(#c)"/>
<rect width="1" height="1" mask="url(#m)" fill="url(#grad)"/>
<foo/>"##;
    let s = parse(&doc("", body), &Options::default()).unwrap();
    let has = |part: &str, line: u32| s.warnings.iter().any(|w| w.msg.contains(part) && w.line == line);
    assert!(has("Stylesheet", 2), "{:?}", s.warnings);
    assert!(has("Text", 3));
    assert!(has("Bilder", 4));
    assert!(has("use", 5));
    assert!(has("CSS-Klassen", 6));
    assert!(has("clip-path", 6));
    assert!(has("mask", 7));
    assert!(has("Verlauf", 7));
    assert!(has("`foo`", 8));
    assert_eq!(s.shapes.len(), 2, "the rects are still imported");
}

#[test]
fn select_by_id() {
    let body =
        r#"<g id="a"><rect width="1" height="1"/></g><g id="b"><rect x="5" width="2" height="2"/><text>x</text></g>"#;
    let o = Options { dpi: 25.4, id: Some("b".into()), ..Default::default() };
    let s = parse(&doc("", body), &o).unwrap();
    assert_eq!(s.shapes.len(), 1);
    assert!(near(area(&s), 4.0, 1e-9));
    assert_eq!(s.warnings.len(), 1);
    let e = parse(&doc("", body), &Options { id: Some("zz".into()), ..Default::default() }).unwrap_err();
    assert!(e.contains("zz"));
}

#[test]
fn errors() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    assert!(parse("<svg", &Options::default()).unwrap_err().contains("nicht lesbar"));
    assert!(parse("<html/>", &Options::default()).unwrap_err().contains("kein SVG"));
    let s = parse(&doc("", r#"<path d="M0 0 L10 0 L10 10 Z L x"/>"#), &Options::default()).unwrap();
    assert!(s.warnings.iter().any(|w| w.msg.contains("Pfaddaten fehlerhaft")));
    assert_eq!(s.shapes.len(), 1, "the part before the error stays");
    // Inkscape namespaces and metadata are ignored silently
    let ink = r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:sodipodi="http://sodipodi.sourceforge.net/DTD/sodipodi-0.dtd">
<sodipodi:namedview/><metadata/><defs><linearGradient id="g"/></defs><rect width="1" height="1"/></svg>"#;
    let s = parse(ink, &Options::default()).unwrap();
    assert!(s.warnings.is_empty(), "{:?}", s.warnings);
}

#[test]
fn messages_english() {
    let _en = cad_i18n::scoped(cad_i18n::Lang::En);
    assert!(parse("<svg", &Options::default()).unwrap_err().starts_with("SVG not readable"));
    assert_eq!(parse("<html/>", &Options::default()).unwrap_err(), "not an SVG document (root element `html`)");
    let s = parse(&doc("", r#"<path d="M0 0 L10 0 L10 10 Z L x"/>"#), &Options::default()).unwrap();
    assert!(s.warnings.iter().any(|w| w.msg.starts_with("`path`: path data invalid")), "{:?}", s.warnings);
    let s = parse(&doc("", r#"<path d="M0 0 L5 5"/><image href="a.png"/>"#), &Options::default()).unwrap();
    let msgs: Vec<&str> = s.warnings.iter().map(|w| w.msg.as_str()).collect();
    assert!(msgs.contains(&"`path`: open path is not filled (taken as a path only; Z closes it)"), "{msgs:?}");
    assert!(msgs.contains(&"`image`: images not supported"), "{msgs:?}");
}

#[test]
fn nested_svg_viewport() {
    let o = Options { dpi: 25.4, ..Default::default() };
    let body = r#"<svg x="10" y="0" width="20" height="20" viewBox="0 0 10 10"><rect width="10" height="10"/></svg>"#;
    let s = parse(&doc("", body), &o).unwrap();
    let (lo, hi) = bounds(&s);
    assert!(near(lo.x, 10.0, 1e-9) && near(hi.x, 30.0, 1e-9));
}
