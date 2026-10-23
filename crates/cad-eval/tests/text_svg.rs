//! `import("….svg")` and `text()` (Plan 17.4/17.5, Bericht 16): profiles, data files of the
//! project, warnings with SVG lines, fonts.

use std::collections::BTreeMap;

use cad_eval::*;
use cad_ir::{Op, Profile, Seg};
use cad_syntax::Severity;

fn files(list: &[(&str, &str)]) -> BTreeMap<String, String> {
    list.iter().map(|(p, t)| (p.to_string(), t.to_string())).collect()
}

fn run(main: &str, others: &[(&str, &str)]) -> (Project, EvalOutput) {
    let f = files(others);
    eval_files("main.scad", main, &f, &EvalOptions::default())
}

fn msgs(out: &EvalOutput, sev: Severity) -> Vec<String> {
    out.diagnostics.iter().filter(|d| d.severity == sev).map(|d| d.message.clone()).collect()
}

/// Closed sketch profiles of the tree (contour count each).
fn sketches(out: &EvalOutput) -> Vec<(usize, bool)> {
    out.tree
        .nodes
        .iter()
        .filter_map(|n| match &n.op {
            Op::Profile(Profile::Sketch { contours, closed }) => Some((contours.len(), *closed)),
            _ => None,
        })
        .collect()
}

const LOGO: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="40mm" height="20mm" viewBox="0 0 40 20">
  <rect x="0" y="0" width="40" height="20" rx="3" fill-rule="evenodd"/>
  <circle cx="10" cy="10" r="5"/>
  <text x="20" y="10">A</text>
</svg>
"#;

#[test]
fn import_svg_gives_sketch_profiles() {
    let (p, out) = run("linear_extrude(2) import(\"logo.svg\");", &[("logo.svg", LOGO)]);
    assert!(msgs(&out, Severity::Error).is_empty(), "{:?}", out.diagnostics);
    assert!(p.assets.contains_key("logo.svg"));
    assert_eq!(sketches(&out), vec![(1, true), (1, true)], "one profile per element, under a group");
    // the text element is reported with its line in the SVG
    let w = msgs(&out, Severity::Warning);
    assert!(w.iter().any(|m| m.starts_with("logo.svg:4:") && m.contains("Text")), "{w:?}");
    // circle stays exact
    let circle = out.tree.nodes.iter().find_map(|n| match &n.op {
        Op::Profile(Profile::Sketch { contours, .. })
            if contours[0].segs.iter().all(|s| matches!(s, Seg::Arc { .. })) =>
        {
            Some(contours[0].clone())
        }
        _ => None,
    });
    assert!(circle.is_some(), "circle as arcs");
}

#[test]
fn import_relative_to_the_file_and_errors() {
    let lib = "module logo() import(\"bilder/logo.svg\");";
    let (_, out) = run("use <lib/teile.scad>\nlogo();", &[("lib/teile.scad", lib), ("lib/bilder/logo.svg", LOGO)]);
    assert!(msgs(&out, Severity::Error).is_empty(), "{:?}", out.diagnostics);
    assert_eq!(sketches(&out).len(), 2);
    // missing file
    let (_, out) = run("import(\"fehlt.svg\");", &[]);
    assert!(msgs(&out, Severity::Error).iter().any(|m| m.contains("fehlt.svg") && m.contains("nicht gefunden")));
    // name computed at run time: not loaded
    let (_, out) = run("n = \"lo\"; import(str(n, \"go.sv\", \"g\"));", &[("logo.svg", LOGO)]);
    assert!(msgs(&out, Severity::Error).iter().any(|m| m.contains("nicht geladen")), "{:?}", out.diagnostics);
    // other formats
    let (_, out) = run("import(\"teil.stl\");", &[]);
    assert!(msgs(&out, Severity::Error).iter().any(|m| m.contains("nur SVG")));
    // broken SVG and unknown id
    let (_, out) = run("import(\"x.svg\");", &[("x.svg", "<svg")]);
    assert!(msgs(&out, Severity::Error).iter().any(|m| m.contains("x.svg: SVG nicht lesbar")));
    let (_, out) = run("import(\"logo.svg\", id = \"nix\");", &[("logo.svg", LOGO)]);
    assert!(msgs(&out, Severity::Error).iter().any(|m| m.contains("nix")));
    // a name passed as a parameter is found too
    let (_, out) = run("module m(f = \"logo.svg\") import(f);\nm();", &[("logo.svg", LOGO)]);
    assert!(msgs(&out, Severity::Error).is_empty(), "{:?}", out.diagnostics);
}

#[test]
fn import_open_paths_are_a_path() {
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg"><path d="M0 0 L10 0 L10 10"/></svg>"#;
    let (_, out) = run("import(\"p.svg\");", &[("p.svg", svg)]);
    assert_eq!(sketches(&out), vec![(1, false)]);
    assert!(msgs(&out, Severity::Warning).iter().any(|m| m.contains("offener Pfad")));
}

#[test]
fn changed_svg_changes_the_tree() {
    let a = r#"<svg xmlns="http://www.w3.org/2000/svg"><rect width="10" height="10"/></svg>"#;
    let b = r#"<svg xmlns="http://www.w3.org/2000/svg"><rect width="20" height="10"/></svg>"#;
    let (_, o1) = run("import(\"r.svg\");", &[("r.svg", a)]);
    let (_, o2) = run("import(\"r.svg\");", &[("r.svg", b)]);
    let ops = |o: &EvalOutput| format!("{:?}", o.tree.nodes.iter().map(|n| &n.op).collect::<Vec<_>>());
    assert_ne!(ops(&o1), ops(&o2));
}

#[test]
fn text_profiles_and_fonts() {
    let (_, out) = run("linear_extrude(1) text(\"Ob\", size = 10, font = \"Inter\");", &[]);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    // one profile per glyph; O and b have a hole each
    assert_eq!(sketches(&out), vec![(2, true), (2, true)]);
    let (_, out) = run("text(\"A\");", &[]);
    assert_eq!(sketches(&out), vec![(2, true)], "default font, single glyph without group");
    // fonts: style missing, unknown family, reserved stencil name
    let (_, out) = run("text(\"A\", font = \"Inter:style=Thin\");", &[]);
    assert!(msgs(&out, Severity::Warning).iter().any(|m| m.contains("Thin")));
    let (_, out) = run("text(\"A\", font = \"Comic Sans\");", &[]);
    assert!(msgs(&out, Severity::Warning).iter().any(|m| m.contains("Comic Sans") && m.contains("JetBrains Mono")));
    let (_, out) = run("text(\"A\", font = \"cad stencil mono\");", &[]);
    assert!(msgs(&out, Severity::Info).iter().any(|m| m.contains("Schablonenschrift")));
    // parameters
    let (_, out) = run("text(\"A\", halign = \"mitte\");", &[]);
    assert!(msgs(&out, Severity::Warning).iter().any(|m| m.contains("halign")));
    let (_, out) = run("text(\"\");", &[]);
    assert!(sketches(&out).is_empty() && out.diagnostics.is_empty());
    let (_, out) = run("text(\"a\u{4e2d}\");", &[]);
    assert!(msgs(&out, Severity::Warning).iter().any(|m| m.contains("fehlen") && m.contains("Platzhalter")));
    // a character the font lacks comes from JetBrains Mono (warning at the statement)
    let (_, out) = run("text(\"Maß: 20°\", font = \"Allerta Stencil\");", &[]);
    let w = msgs(&out, Severity::Warning);
    assert!(w.iter().any(|m| m.contains("„°“ fehlen in Allerta Stencil") && m.contains("JetBrains Mono")), "{w:?}");
    assert_eq!(sketches(&out).len(), 7, "M a ß : 2 0 ° (no glyph for the space)");
    // umlauts and punctuation of the bundled fonts need no fallback
    let (_, out) = run("text(\"Größe – „Äpfel“ ÖÜäöüẞ …\", font = \"Inter\");", &[]);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
}

#[test]
fn text_with_a_font_file() {
    // binary files of text projects are data URLs
    let ttf = cad_font::BUILTIN.iter().find(|b| b.family == "Allerta Stencil").unwrap().data;
    let url = data_url("font/ttf", ttf);
    let (p, out) = run("text(\"H\", font = \"fonts/allerta.ttf\");", &[("fonts/allerta.ttf", &url)]);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    assert_eq!(p.assets["fonts/allerta.ttf"].0.as_ref().unwrap().len(), ttf.len());
    assert_eq!(sketches(&out), vec![(2, true)], "stencil H: two parts");
    let (_, out) = run("text(\"H\", font = \"kaputt.ttf\");", &[("kaputt.ttf", "keine Schrift")]);
    assert!(msgs(&out, Severity::Error).iter().any(|m| m.contains("nicht lesbar")), "{:?}", out.diagnostics);
}

#[test]
fn data_urls_roundtrip() {
    for n in 0..20usize {
        let b: Vec<u8> = (0..n as u8).map(|i| i.wrapping_mul(37)).collect();
        let u = data_url("application/octet-stream", &b);
        assert_eq!(decode_data_url(&u).unwrap().unwrap(), b);
    }
    assert!(decode_data_url("<svg/>").is_none());
}
