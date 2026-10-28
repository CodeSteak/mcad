//! Writing studio settings into the `render { … }` block (UI write-back): small edits.

use cad_eval::{EditTarget, RenderEdit, apply_edits, render_edits};

fn edit(src: &str, changes: &[RenderEdit]) -> String {
    let proj =
        cad_eval::Project::load_parsed("main.mcad", src, cad_syntax::parse(src).0, Vec::new(), &cad_eval::NoFiles);
    let edits = render_edits(&proj, changes).unwrap();
    let out = apply_edits(src, &edits);
    // the result parses and evaluates without new diagnostics
    let (_, d) = cad_syntax::parse(&out);
    assert!(d.is_empty(), "{out}\n{d:?}");
    let p2 =
        cad_eval::Project::load_parsed("main.mcad", &out, cad_syntax::parse(&out).0, Vec::new(), &cad_eval::NoFiles);
    let ev = cad_eval::eval_project(&p2, &Default::default());
    assert!(
        ev.diagnostics.iter().all(|d| !d.message.starts_with("render") || d.message.contains("no part")),
        "{out}\n{:?}",
        ev.diagnostics
    );
    out
}

fn set(k: &str, v: &str) -> RenderEdit {
    RenderEdit::Set { key: k.into(), value: Some(v.into()) }
}

fn light(n: &str, k: &str, v: &str) -> RenderEdit {
    RenderEdit::Light { name: n.into(), key: k.into(), value: Some(v.into()) }
}

fn mat(c: &str, k: &str, v: &str) -> RenderEdit {
    RenderEdit::Material { target: EditTarget::Color(c.into()), key: k.into(), value: Some(v.into()) }
}

#[test]
fn a_new_block_goes_at_the_end() {
    let src = "// box\ncube(10);\n";
    let out = edit(
        src,
        &[
            set("edges", "false"),
            light("key", "azimuth", "30"),
            light("key", "elevation", "40"),
            mat("red", "preset", "\"chrome\""),
        ],
    );
    assert_eq!(
        out,
        "// box\ncube(10);\n\nrender {\n    edges = false;\n    light key { azimuth = 30; elevation = 40; }\n    material \"red\" { preset = \"chrome\"; }\n}\n"
    );
    // no newline at the end of the file
    assert_eq!(edit("cube(1);", &[set("ao", "false")]), "cube(1);\n\nrender {\n    ao = false;\n}\n");
    // only removals: nothing to write
    assert_eq!(edit("cube(1);\n", &[RenderEdit::Set { key: "ao".into(), value: None }]), "cube(1);\n");
}

const BLOCK: &str = r#"cube(10);

render {
    // look of the box
    edges = false;   // no lines
    ambient = 0.4;
    light key { azimuth = -40; elevation = 50; }
    material "silver" { preset = "aluminium"; }
}
// end
"#;

#[test]
fn values_change_in_place_and_comments_stay() {
    let out = edit(BLOCK, &[set("ambient", "0.25"), set("edges", "true"), light("key", "azimuth", "15")]);
    assert_eq!(
        out,
        BLOCK
            .replace("ambient = 0.4", "ambient = 0.25")
            .replace("edges = false", "edges = true")
            .replace("azimuth = -40", "azimuth = 15")
    );
    // unchanged values: no edit
    let proj =
        cad_eval::Project::load_parsed("main.mcad", BLOCK, cad_syntax::parse(BLOCK).0, Vec::new(), &cad_eval::NoFiles);
    assert!(render_edits(&proj, &[set("ambient", "0.4")]).unwrap().is_empty());
}

#[test]
fn new_settings_lights_and_materials_go_after_their_kind() {
    let out = edit(
        BLOCK,
        &[
            set("shadow_softness", "0.6"),
            light("key", "intensity", "1.2"),
            light("rim", "azimuth", "150"),
            light("rim", "color", "\"#ffe0c0\""),
            mat("#abaeb2", "roughness", "0.3"),
            mat("red", "preset", "\"rubber\""),
            RenderEdit::Material {
                target: EditTarget::Part("lid".into()),
                key: "metalness".into(),
                value: Some("1".into()),
            },
        ],
    );
    assert_eq!(
        out,
        r##"cube(10);

render {
    // look of the box
    edges = false;   // no lines
    ambient = 0.4;
    shadow_softness = 0.6;
    light key { azimuth = -40; elevation = 50; intensity = 1.2; }
    light rim { azimuth = 150; color = "#ffe0c0"; }
    material "silver" { preset = "aluminium"; roughness = 0.3; }
    material "red" { preset = "rubber"; }
    material lid { metalness = 1; }
}
// end
"##
    );
}

#[test]
fn removals_take_their_line() {
    let out = edit(
        BLOCK,
        &[
            RenderEdit::Set { key: "edges".into(), value: None },
            RenderEdit::RemoveLight { name: "key".into() },
            RenderEdit::RemoveMaterial { target: EditTarget::Color("silver".into()) },
        ],
    );
    assert_eq!(out, "cube(10);\n\nrender {\n    // look of the box\n    ambient = 0.4;\n}\n// end\n");
    // a value in a light
    let out = edit(BLOCK, &[RenderEdit::Light { name: "key".into(), key: "elevation".into(), value: None }]);
    assert!(out.contains("light key { azimuth = -40; }"), "{out}");
}

#[test]
fn one_line_and_empty_blocks() {
    assert_eq!(edit("render { edges = false; }\n", &[set("ao", "false")]), "render { edges = false; ao = false; }\n");
    assert_eq!(edit("render {}\n", &[set("ao", "false")]), "render { ao = false; }\n");
    assert_eq!(
        edit("render { }\n", &[set("ao", "false"), set("edges", "true")]),
        "render { ao = false; edges = true; }\n"
    );
    assert_eq!(edit("render {\n}\n", &[light("key", "azimuth", "5")]), "render {\n    light key { azimuth = 5; }\n}\n");
    // multi-line light entry
    let src = "render {\n  light key {\n    azimuth = 5;\n  }\n}\n";
    assert_eq!(
        edit(src, &[light("key", "elevation", "30")]),
        "render {\n  light key {\n    azimuth = 5;\n    elevation = 30;\n  }\n}\n"
    );
}

#[test]
fn expressions_are_replaced_and_bad_names_refused() {
    let src = "az = 10;\nrender { light key { azimuth = az * 2; } }\n";
    assert_eq!(edit(src, &[light("key", "azimuth", "45")]), "az = 10;\nrender { light key { azimuth = 45; } }\n");
    let proj =
        cad_eval::Project::load_parsed("main.mcad", src, cad_syntax::parse(src).0, Vec::new(), &cad_eval::NoFiles);
    assert!(render_edits(&proj, &[set("shine", "1")]).unwrap_err().contains("unknown setting `shine`"));
    assert!(render_edits(&proj, &[light("key", "height", "1")]).is_err());
    assert!(render_edits(&proj, &[light("my light", "azimuth", "1")]).is_err());
}
