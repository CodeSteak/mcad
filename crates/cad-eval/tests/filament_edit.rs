//! Writing a filament slot into the code (colour list in the UI, Plan 16.3).

use cad_eval::palette::{parse_color, rgba_bytes};
use cad_ir::Op;

fn filament_change(src: &str, color: &str, filament: Option<u32>) -> String {
    let proj =
        cad_eval::Project::load_parsed("main.scad", src, cad_syntax::parse(src).0, Vec::new(), &cad_eval::NoFiles);
    let out = cad_eval::eval_project(&proj, &Default::default());
    let want = rgba_bytes(parse_color(color).unwrap().rgba);
    let spans: Vec<cad_syntax::Span> = out
        .tree
        .nodes
        .iter()
        .filter(|n| matches!(&n.op, Op::Color { rgba, .. } if rgba_bytes(*rgba) == want))
        .map(|n| n.span)
        .collect();
    let edits = cad_eval::filament_edits(&proj, &spans, filament);
    cad_eval::apply_edits(src, &edits)
}

#[test]
fn slot_is_added_to_the_first_statement() {
    let src = "color(\"red\") cube(1);\nfor (i = [0:2]) translate([i * 2, 0, 0]) color(\"red\") cube(1);";
    assert_eq!(
        filament_change(src, "red", Some(3)),
        "color(\"red\", filament = 3) cube(1);\nfor (i = [0:2]) translate([i * 2, 0, 0]) color(\"red\") cube(1);"
    );
}

#[test]
fn existing_slots_are_replaced_or_removed() {
    let src = "module m() color(\"teal\", filament = 2) cube(1);\nm();\ncolor(\"#217274\", filament = 4) sphere(1);";
    assert_eq!(
        filament_change(src, "teal", Some(5)),
        "module m() color(\"teal\", filament = 5) cube(1);\nm();\ncolor(\"#217274\", filament = 5) sphere(1);"
    );
    assert_eq!(
        filament_change(src, "teal", None),
        "module m() color(\"teal\") cube(1);\nm();\ncolor(\"#217274\") sphere(1);"
    );
    assert_eq!(
        filament_change("color(filament = 2, c = \"red\") cube(1);", "red", None),
        "color(c = \"red\") cube(1);"
    );
}

#[test]
fn unchanged_value_gives_no_edit() {
    let src = "color(\"red\", filament = 2) cube(1);";
    assert_eq!(filament_change(src, "red", Some(2)), src);
    assert_eq!(filament_change("color(\"red\") cube(1);", "red", None), "color(\"red\") cube(1);");
}
