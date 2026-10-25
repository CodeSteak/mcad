//! Projects of several files: `include`/`use` (OpenSCAD 2021.01 semantics, see
//! `tests/openscad-compat/projects`), file-aware spans, writing back into included files,
//! parts and indexed named instances `k[i]`.

mod common;
use std::collections::BTreeMap;

use cad_eval::*;
use cad_syntax::Severity;
use common::*;
use glam::DVec3;

fn files(list: &[(&str, &str)]) -> BTreeMap<String, String> {
    list.iter().map(|(p, t)| (p.to_string(), t.to_string())).collect()
}

/// Loads and evaluates `main` (path `main.scad`) with the other files.
fn project(main: &str, others: &[(&str, &str)]) -> (Project, EvalOutput) {
    project_with(main, others, &EvalOptions::default())
}

fn project_with(main: &str, others: &[(&str, &str)], opts: &EvalOptions) -> (Project, EvalOutput) {
    let f = files(others);
    let (p, out) = eval_files("main.scad", main, &f, opts);
    assert!(p.syntax.is_empty(), "syntax errors: {:?}", p.syntax);
    (p, out)
}

fn not_info(out: &EvalOutput) -> Vec<String> {
    out.diagnostics.iter().filter(|d| d.severity != Severity::Info).map(|d| d.message.clone()).collect()
}

#[test]
fn paths_are_relative_to_the_including_file() {
    assert_eq!(resolve_path("main.scad", "lib/a.scad"), "lib/a.scad");
    assert_eq!(resolve_path("lib/a.scad", "b.scad"), "lib/b.scad");
    assert_eq!(resolve_path("lib/a.scad", "../konst.scad"), "konst.scad");
    assert_eq!(resolve_path("lib/a.scad", "./x/../y.scad"), "lib/y.scad");
    assert_eq!(resolve_path("main.scad", "../aussen.scad"), "../aussen.scad");
    assert_eq!(resolve_path("a/b.scad", "/abs/c.scad"), "/abs/c.scad");
    assert_eq!(resolve_path("a\\b.scad", "c\\d.scad"), "a/c/d.scad");
}

#[test]
fn include_inserts_definitions_variables_and_geometry() {
    let (p, out) = project(
        "include <konst.scad>\necho(breite, f(2));\nteil();\ncube(breite);",
        &[(
            "konst.scad",
            "breite = 40;\nfunction f(x) = x * breite;\nmodule teil() sphere(breite);\ncylinder(h = 1, r = 2);",
        )],
    );
    assert_eq!(not_info(&out), Vec::<String>::new());
    assert_eq!(echoes(&out), ["40, 80"]);
    assert_eq!(dump(&out), "group\n  cylinder\n  teil\n    sphere\n  box\n");
    assert_eq!(p.files.len(), 2);
    assert_eq!(p.files[1].role, FileRole::Include);
    // The cylinder's node points into the included file.
    let cyl = nodes_by_label(&out, "cylinder")[0];
    let span = out.tree.node(cyl).span;
    assert_eq!(span.file(), 1);
    assert_eq!(p.text(span), "cylinder(h = 1, r = 2);");
    assert_eq!(p.line(span), 4);
}

#[test]
fn include_assignments_follow_textual_order() {
    // OpenSCAD: the last assignment of the (spliced) file wins, with a warning.
    let (_, out) = project("x = 5;\ninclude <c.scad>\necho(x = x, y = y);", &[("c.scad", "x = 1;\ny = x * 10;")]);
    assert_eq!(echoes(&out), ["x = 1, y = 10"]);
    assert!(
        has_warning(&out, "`x` aus Zeile 1 in `main.scad` wird in Zeile 1 in `c.scad` überschrieben"),
        "{:?}",
        warnings(&out)
    );
    let (_, out) = project("include <c.scad>\nx = 3;\necho(x = x, y = y);", &[("c.scad", "x = 1;\ny = x * 10;")]);
    assert_eq!(echoes(&out), ["x = 3, y = 30"]);
}

#[test]
fn use_imports_modules_and_functions_only() {
    let lib = "module m() echo(\"a\");\nfunction f() = \"a\";\nla = 1;\nfunction getla() = la;\n\
               module showla() echo(la = la, fn = $fn);\ncube(1);\nuse <lib/inner.scad>\nmodule callinner() inner();";
    let (_, out) = project(
        "use <a.scad>\nuse <b.scad>\nm();\necho(f());\necho(getla());\necho(la = la);\nshowla($fn = 5);\ncallinner();\ninner();",
        &[
            ("a.scad", lib),
            ("b.scad", "module m() echo(\"b\");\nfunction f() = \"b\";"),
            ("lib/inner.scad", "module inner() echo(\"inner\", innerv);\ninnerv = 7;"),
        ],
    );
    // Same echo output as OpenSCAD 2021.01: the last `use` wins, library variables are only
    // visible inside the library, `use` is not transitive, no library geometry.
    assert_eq!(echoes(&out), ["\"b\"", "\"b\"", "1", "la = undef", "la = 1, fn = 5", "\"inner\", 7"]);
    assert!(has_warning(&out, "unbekannte Variable `la`"));
    assert!(has_warning(&out, "unbekanntes Modul `inner`"));
    assert!(nodes_by_label(&out, "box").is_empty(), "{}", dump(&out));
}

#[test]
fn own_definitions_win_and_includes_bring_their_uses() {
    let (_, out) = project(
        "use <a.scad>\nmodule m() echo(\"own\");\nm();\nx = 5;\ninclude <c.scad>\necho(x = x, y = y);\ncm();\necho(f());",
        &[
            ("a.scad", "module m() echo(\"a\");\nfunction f() = \"a\";"),
            ("b.scad", "module m() echo(\"b\");\nfunction f() = \"b\";"),
            ("c.scad", "x = 1;\ny = x * 10;\nmodule cm() echo(\"cm\", x);\nuse <b.scad>\ncube(2);"),
        ],
    );
    assert_eq!(echoes(&out), ["\"own\"", "x = 1, y = 10", "\"cm\", 1", "\"b\""]);
    assert_eq!(nodes_by_label(&out, "box").len(), 1);
}

#[test]
fn use_without_geometry_and_used_once() {
    let (p, out) = project(
        "use <lib.scad>\nuse <lib.scad>\nhalter();",
        &[("lib.scad", "module halter(h = 10) cylinder(h = h, r = 2);\ncube(100);\nsphere(5);")],
    );
    assert_eq!(not_info(&out), Vec::<String>::new());
    assert_eq!(dump(&out), "group\n  halter\n    cylinder\n");
    assert_eq!(p.libs.len(), 1);
    assert_eq!(p.files.len(), 2);
    assert_eq!(p.files[1].role, FileRole::Use);
}

#[test]
fn include_cycle_is_an_error() {
    let (p, out) = project(
        "include <a.scad>\ncube(1);",
        &[("a.scad", "include <b.scad>\nz = 1;"), ("b.scad", "include <a.scad>\n")],
    );
    let d = out.diagnostics.iter().find(|d| d.message.contains("zyklisches")).expect("cycle error");
    assert_eq!(d.severity, Severity::Error);
    assert_eq!(d.message, "zyklisches `include`: a.scad → b.scad → a.scad");
    // Reported at the include of `b.scad` (line 1), in that file.
    assert_eq!(p.path_of(d.span), "b.scad");
    assert_eq!(p.line(d.span), 1);
    assert_eq!(p.text(d.span), "<a.scad>");
    // The rest still evaluates.
    assert_eq!(nodes_by_label(&out, "box").len(), 1);
    // Self-include of the main file.
    let (_, out) = project("include <main.scad>\ncube(1);", &[]);
    assert!(has_error(&out, "zyklisches `include`: main.scad → main.scad"));
}

#[test]
fn missing_file_is_an_error_at_the_line() {
    let (p, out) = project("cube(1);\ninclude <fehlt.scad>\nuse <lib/fehlt2.scad>\n", &[]);
    let errs: Vec<_> = out.diagnostics.iter().filter(|d| d.severity == Severity::Error).collect();
    assert_eq!(errs.len(), 2, "{errs:?}");
    assert!(errs[0].message.starts_with("Datei `fehlt.scad` nicht gefunden"));
    assert_eq!(p.line(errs[0].span), 2);
    assert!(errs[1].message.starts_with("Datei `lib/fehlt2.scad` nicht gefunden"));
    assert_eq!(p.line(errs[1].span), 3);
    // Missing file inside an included file: at that file's line.
    let (p, out) = project("include <a.scad>", &[("a.scad", "\n\ninclude <nix.scad>")]);
    let d = out.diagnostics.iter().find(|d| d.severity == Severity::Error).unwrap();
    assert_eq!((p.path_of(d.span), p.line(d.span)), ("a.scad", 3));
}

#[test]
fn diagnostics_of_library_code_know_their_file() {
    let (p, out) =
        project("use <lib/teile.mcad>\nhalter();", &[("lib/teile.mcad", "\nmodule halter() {\n  cube(unbekannt);\n}")]);
    let d = out.diagnostics.iter().find(|d| d.message.contains("unbekannt")).unwrap();
    assert_eq!(p.path_of(d.span), "lib/teile.mcad");
    assert_eq!(p.line(d.span), 3);
    // Syntax errors of other files have their spans in those files too.
    let f = files(&[("x.scad", "cube(;")]);
    let (p, _) = eval_files("main.scad", "include <x.scad>", &f, &EvalOptions::default());
    assert_eq!(p.syntax.len(), 1);
    assert_eq!(p.path_of(p.syntax[0].span), "x.scad");
}

#[test]
fn diamond_include_loads_the_file_twice() {
    let (p, out) = project(
        "include <a.scad>\ninclude <b.scad>",
        &[("a.scad", "include <c.scad>"), ("b.scad", "include <c.scad>"), ("c.scad", "sphere(1);")],
    );
    let paths: Vec<&str> = p.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, ["main.scad", "a.scad", "c.scad", "b.scad", "c.scad"]);
    let spheres = nodes_by_label(&out, "sphere");
    assert_eq!(spheres.len(), 2);
    let (a, b) = (out.tree.node(spheres[0]), out.tree.node(spheres[1]));
    assert_ne!(a.stmt, b.stmt);
    assert_ne!(a.inst_key, b.inst_key);
}

#[test]
fn drag_writes_into_the_included_file() {
    let main = "include <konst.scad>\ncube([breite, 10, 2]);";
    let konst = "// Konstanten\nbreite = 40;\n";
    let (p, out) = project(main, &[("konst.scad", konst)]);
    let h = out.handles.iter().find(|h| h.kind == HandleKind::Point).expect("corner handle");
    assert!(h.via_var);
    let c0 = h.comps[0].expect("literal through variable");
    assert_eq!(c0.span.file(), 1);
    assert_eq!(p.text(c0.span), "40");
    let t = DragTarget::Free(DragValue::Point(DVec3::new(55.0, 10.0, 2.0)));
    let edits = drag_edits(&out, &p, h.id, &t, &DragOptions::default());
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].span.file(), 1);
    assert_eq!(edits[0].text, "55");
    // Apply to the file's text and evaluate again.
    let local = TextEdit { span: edits[0].span.local(), text: edits[0].text.clone() };
    let konst2 = apply_edits(konst, &[local]);
    assert_eq!(konst2, "// Konstanten\nbreite = 55;\n");
    let (_, out2) = project(main, &[("konst.scad", &konst2)]);
    let b = nodes_by_label(&out2, "box")[0];
    let cad_ir::Op::Prim { prim: cad_ir::Prim::Box { max, .. }, .. } = &out2.tree.node(b).op else { panic!() };
    assert_eq!(max.x, 55.0);
}

#[test]
fn module_of_a_used_library_as_context() {
    let lib = "module halter(h = 12) {\n  cylinder(h = h, r = 3);\n  anchor oben = [0, 0, h];\n}";
    let opts =
        EvalOptions { context: Context::Module { name: "halter".into(), overrides: vec![] }, ..Default::default() };
    let (p, out) = project_with("use <lib/halter.mcad>\nhalter();", &[("lib/halter.mcad", lib)], &opts);
    assert_eq!(not_info(&out), Vec::<String>::new());
    assert_eq!(out.context.module.as_deref(), Some("halter"));
    assert_eq!(p.path_of(out.context.def_span.unwrap()), "lib/halter.mcad");
    // Height handle of the cylinder, literal in the default of the library file.
    let h = out.handles.iter().find(|h| h.kind == HandleKind::Height).expect("height handle");
    assert!(h.via_default);
    let e = drag_edits(&out, &p, h.id, &DragTarget::Free(DragValue::Scalar(20.0)), &DragOptions::default());
    assert_eq!(e.len(), 1);
    assert_eq!(p.path_of(e[0].span), "lib/halter.mcad");
    assert_eq!(p.text(e[0].span), "12");
    // Module signatures of library modules are listed (parameter bar).
    let sig = out.modules.iter().find(|m| m.name == "halter").unwrap();
    assert_eq!(sig.params[0].default_text.as_deref(), Some("12"));
    // "Als Standard übernehmen" into the library file.
    let e = set_default_edit_project(&p, "halter", "h", "15").unwrap();
    assert_eq!((p.path_of(e.span), p.text(e.span)), ("lib/halter.mcad", "12"));
}

#[test]
fn parts_are_top_level_named_instances() {
    let src = "module deckel() cube([10, 10, 2]);\nmodule gehaeuse() cube(10);\nmodule schraube() cylinder(h = 5, r = 1);\n\
               deckel: deckel();\ntranslate([0, 0, 20]) gehaeuse: gehaeuse();\nmodule baugruppe() { s: schraube(); }\nb: baugruppe();\ncube(1);";
    let out = clean(src);
    let names: Vec<&str> = out.parts.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["deckel", "gehaeuse", "b"]);
    let g = out.part("gehaeuse").unwrap();
    assert_eq!(out.tree.node(g.node).world.w_axis.z, 20.0);
    assert!(matches!(&out.tree.node(g.node).op, cad_ir::Op::ModuleCall { instance: Some(n), .. } if n == "gehaeuse"));
    // No parts in a module context.
    let out = run_module(src, "baugruppe", vec![]);
    assert!(out.parts.is_empty());
}

#[test]
fn indexed_named_instances_in_loops() {
    let src = "module halter(h = 5) { cube([2, 2, h]); anchor anker = [1, 1, h]; anchor ecke = [2, 2, 0]; }\n\
               for (i = [0:3]) translate([i * 10, 0, 0]) k[i]: halter(h = 5 + i);\n\
               echo(k[2].anker, k[0].ecke);\n\
               translate(k[3].anker) sphere(1);\n\
               translate([0, 0, 0]) cube(1);";
    let out = clean(src);
    assert_eq!(echoes(&out), ["[21, 1, 7], [2, 2, 0]"]);
    let labels: Vec<String> = out.instances.iter().filter_map(|i| i.label.clone()).collect();
    assert_eq!(labels, ["k[0]", "k[1]", "k[2]", "k[3]"]);
    assert_eq!(out.parts.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["k[0]", "k[1]", "k[2]", "k[3]"]);
    // Snap writes `k[2].ecke`.
    let h = out
        .handles
        .iter()
        .find(|h| h.kind == HandleKind::Point && span_text(src, h.span).starts_with("translate([0, 0, 0])"))
        .unwrap();
    assert_eq!(h.instances.len(), 4);
    let t = snap_targets(&out, h.id);
    let k2 = t.iter().find(|t| t.text == "k[2].ecke").expect("snap target k[2].ecke");
    assert!(approx3(k2.world, [22.0, 2.0, 0.0]));
    let e = drag_edits(&out, src, h.id, &DragTarget::Snap(k2.r#ref.clone()), &DragOptions::default());
    let new = apply_edits(src, &e);
    assert!(new.ends_with("translate(k[2].ecke) cube(1);"), "{new}");
    let out2 = clean(&new);
    let b = nodes_by_label(&out2, "box").into_iter().last().unwrap();
    assert!(approx3(out2.tree.node(b).world.w_axis.truncate(), [22.0, 2.0, 0.0]));
}

#[test]
fn indexed_named_instance_errors() {
    let m = "module p() { anchor a = [1, 0, 0]; }\n";
    for (src, msg) in [
        ("for (i = [0:1]) k: p();", "benannte Instanzen sind in for-Schleifen nicht erlaubt (`k[i]: …` verwenden)"),
        ("for (i = [0:1]) k[i / 2]: p();", "Index einer benannten Instanz muss ganzzahlig sein, ist 0.5"),
        ("for (i = [0:1]) k[\"a\"]: p();", "muss ganzzahlig sein"),
        ("for (i = [0:1]) k[0]: p();", "`k[0]` ist in diesem Modul schon deklariert"),
        ("k: p(); for (i = [0:1]) k[i]: p();", "`k` ist in diesem Modul schon deklariert"),
        ("k = 1; k[0]: p();", "schon eine Variable"),
    ] {
        let out = run(&format!("{m}{src}"));
        assert!(has_error(&out, msg), "{src}: {:?}", errors(&out));
    }
    for (src, msg) in [
        ("k[0]: p(); echo(k.a);", "`k` ist eine Liste benannter Instanzen: `k[i].a` verwenden"),
        ("k[0]: p(); echo(k[5].a);", "`k[5]` ist keine benannte Instanz"),
    ] {
        let out = run(&format!("{m}{src}"));
        assert!(has_warning(&out, msg), "{src}: {:?}", warnings(&out));
    }
    // Members of one family may come from several statements; visible after their line.
    let out = clean(&format!("{m}k[0]: p();\ntranslate([5, 0, 0]) k[1]: p();\necho(k[0].a, k[1].a);"));
    assert_eq!(echoes(&out), ["[1, 0, 0], [6, 0, 0]"]);
}

#[test]
fn single_file_eval_reports_includes_as_missing() {
    let out = eval_source("include <x.scad>\ncube(1);", &EvalOptions::default());
    assert!(has_error(&out, "Datei `x.scad` nicht gefunden"));
    assert_eq!(nodes_by_label(&out, "box").len(), 1);
}
