//! `render { … }`: evaluation, defaults, diagnostics.

use cad_eval::{EvalOptions, EvalOutput};
use cad_ir::RenderTarget;

fn run(src: &str) -> EvalOutput {
    let proj =
        cad_eval::Project::load_parsed("main.mcad", src, cad_syntax::parse(src).0, Vec::new(), &cad_eval::NoFiles);
    cad_eval::eval_project(&proj, &EvalOptions::default())
}

fn warnings(out: &EvalOutput) -> Vec<String> {
    out.diagnostics.iter().map(|d| d.message.clone()).collect()
}

#[test]
fn values_lights_and_materials() {
    let src = r##"
az = -30;
module lid() cube([10, 10, 2]);
color("silver") cube(5);
cover: lid();
render {
    style = "studio";
    edges = false;
    ambient = 0.3;
    light_intensity = 1.2;
    environment = "overcast";
    shadows = true;
    shadow_softness = 0.6;
    ao = false;
    ao_radius = 2;
    ao_strength = 1.5;
    ground = true;
    ground_color = "#808080";
    background = ["white", "#c0c0c0"];
    light key { azimuth = az; elevation = 40; }
    light rim { azimuth = 150; intensity = 0.5; color = [1, 0.9, 0.8]; }
    material "silver" { preset = "aluminium"; roughness = 0.25; }
    material "default" { preset = "rubber"; }
    material cover { metalness = 1; clearcoat = 0.4; }
}
"##;
    let out = run(src);
    assert!(out.diagnostics.is_empty(), "{:?}", warnings(&out));
    let r = out.tree.render.as_ref().expect("render block");
    assert_eq!(r.style.as_deref(), Some("studio"));
    assert_eq!((r.edges, r.ambient, r.light_intensity), (Some(false), Some(0.3), Some(1.2)));
    assert_eq!(r.environment.as_deref(), Some("overcast"));
    assert_eq!((r.shadows, r.shadow_softness, r.ao), (Some(true), Some(0.6), Some(false)));
    assert_eq!((r.ao_radius, r.ao_strength, r.ground), (Some(2.0), Some(1.5), Some(true)));
    assert_eq!(r.ground_color.map(cad_eval::palette::hex_string).as_deref(), Some("#808080"));
    let bg: Vec<String> = r.background.as_ref().unwrap().iter().map(|c| cad_eval::palette::hex_string(*c)).collect();
    assert_eq!(bg, ["#f4f3f0", "#c0c0c0"]);
    assert_eq!(r.lights.len(), 2);
    assert_eq!(
        (r.lights[0].name.as_str(), r.lights[0].azimuth, r.lights[0].elevation, r.lights[0].intensity),
        ("key", -30.0, 40.0, 1.0)
    );
    assert_eq!((r.lights[1].azimuth, r.lights[1].elevation, r.lights[1].intensity), (150.0, 50.0, 0.5));
    assert!((r.lights[1].color[2] - 0.8).abs() < 1e-6);
    assert_eq!(r.materials.len(), 3);
    assert!(matches!(&r.materials[0].target, RenderTarget::Color { label, rgba: Some(_) } if label == "silver"));
    assert_eq!((r.materials[0].preset.as_deref(), r.materials[0].roughness), (Some("aluminium"), Some(0.25)));
    assert!(matches!(&r.materials[1].target, RenderTarget::Color { label, rgba: None } if label == "default"));
    assert!(matches!(&r.materials[2].target, RenderTarget::Part(p) if p == "cover"));
    assert_eq!(
        (r.materials[2].metalness, r.materials[2].clearcoat, r.materials[2].preset.as_deref()),
        (Some(1.0), Some(0.4), None)
    );
}

#[test]
fn defaults_when_not_set_and_module_context() {
    let src = "module part() cube(3);\nrender { edges = false; }\npart();";
    let out = run(src);
    let r = out.tree.render.as_ref().unwrap();
    assert_eq!((r.style.as_deref(), r.edges, r.ambient, r.lights.len()), (None, Some(false), None, 0));
    // no block
    assert!(run("cube(1);").tree.render.is_none());
    // module context: the file's block still holds
    let proj =
        cad_eval::Project::load_parsed("main.mcad", src, cad_syntax::parse(src).0, Vec::new(), &cad_eval::NoFiles);
    let opts = EvalOptions {
        context: cad_eval::Context::Module { name: "part".into(), overrides: Vec::new() },
        ..EvalOptions::default()
    };
    let out = cad_eval::eval_project(&proj, &opts);
    assert_eq!(out.tree.render.as_ref().and_then(|r| r.edges), Some(false));
}

#[test]
fn unknown_names_and_bad_values_are_warnings_with_their_place() {
    let src = r#"cube(1);
render {
    egdes = false;
    ambient = 3;
    shadows = "yes";
    environment = "beach";
    ground_color = "notacolour";
    light key { azimuth = 10; height = 4; elevation = 0; }
    light key { azimuth = 20; }
    material "nocolour" { preset = "chrome"; }
    material "red" { preset = "unobtainium"; shine = 1; }
    material lid { preset = "chrome"; }
    edges = true;
    edges = false;
}
module m() render { edges = true; }
m();
render { ao = false; }
"#;
    let out = run(src);
    let w = warnings(&out);
    let has = |s: &str| w.iter().any(|m| m.contains(s));
    for s in [
        "render: unknown setting `egdes` (known: style, edges",
        "render: ambient = 3 is outside 0…1 – ignored",
        "render: shadows expects true or false, not string – ignored",
        "render: environment expects \"studio\", \"soft\", \"overcast\", \"none\" – ignored",
        "render: ground_color expects a colour",
        "light key: unknown setting `height` (known: azimuth, elevation, intensity, color)",
        "render: elevation = 0 is outside 1…90 – ignored",
        "render: light `key` is defined twice (the last one applies)",
        "render: material \"nocolour\": unknown colour",
        "render: preset expects \"plastic\", \"plastic_satin\"",
        "material \"red\": unknown setting `shine`",
        "render: material lid: no part `lid` (parts: –)",
        "render: `edges` is set twice (the last value applies)",
        "render { … } applies only at the top level of the main file – no effect here",
        "only the first render block applies – this one has no effect",
    ] {
        assert!(has(s), "missing {s:?} in {w:#?}");
    }
    assert!(out.diagnostics.iter().all(|d| d.severity == cad_syntax::Severity::Warning), "{w:?}");
    // the place of a bad value: its line
    let li = cad_syntax::LineIndex::new(src);
    let amb = out.diagnostics.iter().find(|d| d.message.contains("ambient = 3")).unwrap();
    assert_eq!(li.line_col(amb.span.start).0 + 1, 4);
    // the valid values stay; the last light `key` and the last `edges` apply
    let r = out.tree.render.as_ref().unwrap();
    assert_eq!((r.ambient, r.shadows, r.environment.as_deref(), r.edges), (None, None, None, Some(false)));
    assert_eq!(r.lights.len(), 1);
    assert_eq!(r.lights[0].azimuth, 20.0);
    assert_eq!(r.materials.len(), 2);
    assert_eq!(r.materials[0].preset, None);
    // the geometry is unaffected: no error
    assert!(out.tree.root.is_some());
}

#[test]
fn reflections_soft_environment_and_anisotropy() {
    let src = r#"
color("silver") cube(5);
render {
    environment = "soft";
    reflections = false;
    ground_reflection = 0.4;
    ground_roughness = 0.1;
    material "silver" { preset = "metal_brushed"; anisotropy = 0.5; }
    material "default" { preset = "titanium"; }
}
"#;
    let out = run(src);
    assert!(out.diagnostics.is_empty(), "{:?}", warnings(&out));
    let r = out.tree.render.as_ref().unwrap();
    assert_eq!(r.environment.as_deref(), Some("soft"));
    assert_eq!((r.reflections, r.ground_reflection, r.ground_roughness), (Some(false), Some(0.4), Some(0.1)));
    assert_eq!((r.materials[0].preset.as_deref(), r.materials[0].anisotropy), (Some("metal_brushed"), Some(0.5)));
    assert_eq!(r.materials[1].preset.as_deref(), Some("titanium"));
    // out of range: warning, ignored
    let out = run("cube(1);\nrender { ground_reflection = 2; material \"default\" { anisotropy = -1; } }");
    let w = warnings(&out);
    assert!(w.iter().any(|m| m.contains("ground_reflection = 2 is outside 0…1")), "{w:?}");
    assert!(w.iter().any(|m| m.contains("anisotropy = -1 is outside 0…1")), "{w:?}");
    let r = out.tree.render.as_ref().unwrap();
    assert_eq!((r.ground_reflection, r.materials[0].anisotropy), (None, None));
}
