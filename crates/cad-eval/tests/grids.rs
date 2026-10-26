//! Grids, anchors, named instances: values, placement, errors, visibility.

mod common;
use cad_eval::value::Sel;
use cad_eval::{Origin, RefTarget};
use cad_ir::{Op, Prim};
use common::*;
use glam::{DMat4, DVec3};

fn box_of(out: &cad_eval::EvalOutput, i: usize) -> (DVec3, DVec3) {
    let b = nodes_by_label(out, "box")[i];
    match &out.tree.node(b).op {
        Op::Prim { prim: Prim::Box { min, max }, .. } => (*min, *max),
        _ => unreachable!(),
    }
}

#[test]
fn declaration_planes_and_values() {
    let src = "
        t = 2;
        grid g {
            x: b = 10, a = 0, loch = [for (i = [0:2]) 5 + i * 8];
            y: v = t * 2;
        }
        echo(g.a, g.b + 1, g.loch, g.loch[1] * 2, g.v);
        cube(g[b, v, _] + [0, 0, 1]);
    ";
    let out = clean(src);
    assert_eq!(echoes(&out), ["0, 11, [5, 13, 21], 26, 4"]);
    let g = out.grid_by_name("g").unwrap();
    let names: Vec<&str> = g.planes.iter().map(|p| p.name.as_str()).collect();
    // Sorted by axis, then value; missing z gets the implicit `_`.
    assert_eq!(names, ["a", "loch[0]", "b", "loch[1]", "loch[2]", "v", "_"]);
    assert!(g.planes[6].implicit);
    assert!(g.in_context);
    let b = g.planes.iter().find(|p| p.name == "b").unwrap();
    assert_eq!(span_text(src, b.literal.unwrap()), "10");
    let v = g.planes.iter().find(|p| p.name == "v").unwrap();
    assert_eq!(v.literal, None);
    assert_eq!(v.origin, Origin::Computed);
    assert_eq!(box_of(&out, 0), (DVec3::ZERO, DVec3::new(10.0, 4.0, 1.0)));
}

#[test]
fn two_items_for_2d_grids() {
    let out =
        clean("grid g { x: a = 0, b = 10; y: c = 0, d = 5; } polygon([g[a, c], g[b, c], g[b, d]]); echo(g[b, d]);");
    assert_eq!(echoes(&out), ["[10, 5, 0]"]);
}

#[test]
fn grid_errors() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let cases: &[(&str, &str)] = &[
        ("grid g { x: a = 0; y: a = 1; }", "Ebenenname `a` in Raster `g` schon vergeben"),
        ("grid g { x: a = 0; } cube(g[a, b, _]);", "hat keine Ebene `b`"),
        ("grid g { x: a = 0; y: b = 0; } cube(g[b, a, _]);", "Ebene `b` liegt auf Achse y"),
        ("grid g { x: a = 0; y: b = 0; z: c = 0; } cube(g[a, b]);", "braucht eine Ebene pro Achse"),
        ("grid g { x: a = 0; y: b = 0; z: c = 0; } cube(g[a, b, _]);", "`_` steht nur für eine fehlende Achse"),
        ("grid g { x: l = [1, 2]; } cube(g[l, _, _]);", "`l` ist eine Ebenenliste"),
        ("grid g { x: l = [1, 2]; } cube(g[l[5], _, _]);", "Ebenenindex 5 außerhalb"),
        ("grid g { x: a = \"s\"; }", "muss Zahl oder Zahlenliste sein"),
        ("grid g { x: a = 0; } grid g { x: b = 0; }", "schon deklariert"),
        ("g = 1; grid g { x: a = 0; }", "schon eine Variable"),
        ("grid g { x: a = 0; } x = g[*, *, *];", "wählt das ganze Raster"),
        ("scale([1, 2, 1]) grid g { x: a = 0; }", "ungleichmäßiger Skalierung"),
        ("for (i = [0:1]) grid g { x: a = i; }", "in for-Schleifen nicht erlaubt"),
        ("grid g { x: a = 0; } echo(g.zz);", "hat keine Ebene `zz`"),
        ("grid g { x: a = 0; } cube(g[a, \"q\", _]);", "Ebenenname oder Zahl erwartet"),
        ("x = [1, 2][*];", "nur in Rasterzugriffen erlaubt"),
        ("grid g { x: _ = 0; }", "reserviert"),
    ];
    for (src, msg) in cases {
        let out = run(src);
        assert!(has_error(&out, msg), "{src}: expected error containing {msg:?}, got {:?}", out.diagnostics);
    }
}

#[test]
fn lines_and_planes_are_not_values() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let out = run(
        "grid g { x: a = 1; y: b = 2; z: c = 3; } l = g[a, *, c]; echo(l + 1); translate(g[*, *, c]) cube(1); echo(g.c + 1, g[*, *, c] == g[*, *, c]);",
    );
    assert!(has_error(&out, "Typfehler: undefinierte Operation (Rasterlinie + Zahl)"));
    assert!(has_error(&out, "`v` erwartet Zahl oder Punkt, ist Rasterebene"));
    assert_eq!(echoes(&out).last().unwrap(), "4, true");
    let out = run("module p() {} k: p(); echo(k + 1); grid g { x: a = 1; } cube(g);");
    assert!(has_error(&out, "(Instanz + Zahl)"));
    assert!(has_error(&out, "ist Raster; Rasterpunkt"));
}

#[test]
fn placement_by_transforms() {
    let src = "
        grid g { x: l = 0, r = 40; y: v = 0, h = 30; z: b = 0, t = 3; }
        translate(g[r, v, t]) rotate([0, 0, 90]) grid r2 { x: a = 0, e = 10; y: c = 5; z: d = 0; }
        echo(r2[e, c, d]);
        translate(r2[e, c, d]) cube(1);
        mirror([1, 0, 0]) grid m { x: p = 5; }
        echo(m[p, _, _], m.p);
        scale(2) grid s { x: q = 3; }
        echo(s[q, _, _]);
    ";
    let out = clean(src);
    // r2's x axis points along the module's y axis.
    assert_eq!(echoes(&out), ["[35, 10, 3]", "[-5, 0, 0], 5", "[6, 0, 0]"]);
    let r2 = out.grid_by_name("r2").unwrap();
    assert!(approx3(r2.frame_module.transform_point3(DVec3::new(10.0, 0.0, 0.0)), [40.0, 10.0, 3.0]));
    assert_eq!(r2.frame, r2.frame_module);
    // A grid point is interpreted in the local system of the using statement.
    let c = nodes_by_label(&out, "box")[0];
    assert!(approx3(out.tree.node(c).world.transform_point3(DVec3::ZERO), [35.0, 10.0, 3.0]));
}

#[test]
fn grid_inside_module_frames() {
    let src = "
        module m() { translate([5, 0, 0]) grid g { x: a = 1; } echo(g[a, _, _]); }
        translate([100, 0, 0]) m();
    ";
    let out = clean(src);
    assert_eq!(echoes(&out), ["[6, 0, 0]"]);
    let g = out.grid_by_name("g").unwrap();
    assert!(!g.in_context);
    assert_eq!(g.module_inst, 1);
    assert_eq!(g.frame_module, DMat4::from_translation(DVec3::new(5.0, 0.0, 0.0)));
    assert_eq!(g.frame, DMat4::from_translation(DVec3::new(105.0, 0.0, 0.0)));
}

#[test]
fn mirror_at_grid_plane() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let out = clean("grid g { x: m = 10; } mirror(g.m) cube(1); mirror(g[m, *, *]) cube(1);");
    let ms: Vec<DMat4> =
        out.tree.nodes.iter().filter_map(|n| if let Op::Transform { m, .. } = n.op { Some(m) } else { None }).collect();
    for m in ms {
        assert!(approx3(m.transform_point3(DVec3::new(12.0, 1.0, 2.0)), [8.0, 1.0, 2.0]));
    }
    let out = run("grid g { x: m = 10; y: n = 0; } mirror(g[m, *, _]) cube(1);");
    assert!(has_error(&out, "braucht eine Ebene, keine Rasterlinie"));
}

#[test]
fn visibility_of_grids() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    // Visible from the declaration to the end of the module, also in nested blocks.
    let ok = "module m() { translate([1, 0, 0]) { grid g { x: a = 1; } } if (true) { translate(g[a, _, _]) cube(1); } } m();";
    clean(ok);
    // Not before the declaration.
    let out = run("cube(g[a, _, _]); grid g { x: a = 1; }");
    assert!(has_warning(&out, "unbekannte Variable `g`"));
    let out = run("p = g.a; grid g { x: a = 1; }");
    assert!(has_warning(&out, "unbekannte Variable `g`"));
    // Not in called modules.
    let out = run("module inner() cube(g[a, _, _]); grid g { x: a = 1; } inner();");
    assert!(has_warning(&out, "unbekannte Variable `g`"));
    // Not in nested module definitions either.
    let out = run("module outer() { grid g { x: a = 1; } module inner() echo(g.a); inner(); } outer();");
    assert!(has_warning(&out, "unbekannte Variable `g`"));
    // Not in the caller.
    let out = run("module m() { grid g { x: a = 1; } } m(); echo(g.a);");
    assert!(has_warning(&out, "unbekannte Variable `g`"));
    // Children written in the caller see the caller's grids.
    let out = clean("module wrap() children(); grid g { x: a = 1; } wrap() echo(g.a);");
    assert_eq!(echoes(&out), ["1"]);
    // Grid names shadow file variables inside modules.
    let out = clean("g = 5; module m() { grid g { x: a = 2; } echo(g.a); } m(); echo(g);");
    assert_eq!(echoes(&out), ["2", "5"]);
}

#[test]
fn grid_refs_for_hover() {
    let src = "grid g { x: a = 1; y: b = 2; } cube(g[a, b, _]); zone(g[a, *, _], d = 1); echo(g.b);";
    let out = run(src);
    let texts: Vec<&str> = out.refs.iter().map(|r| span_text(src, r.span)).collect();
    assert_eq!(texts, ["g[a, b, _]", "g[a, *, _]", "g.b"]);
    assert!(matches!(out.refs[1].target, RefTarget::Grid { grid: 0, sel: [Sel::Plane(_), Sel::Wild, Sel::Plane(_)] }));
    assert!(matches!(out.refs[2].target, RefTarget::Plane { grid: 0, .. }));
}

#[test]
fn anchors_from_outside() {
    let src = "
        module part() {
            translate([10, 0, 0]) { anchor a = [1, 2, 3]; }
            anchor up = [0, 0, 5], dir = [0, 0, 2];
            cube(1);
        }
        k: part();
        translate([0, 100, 0]) rotate([0, 0, 90]) j: part();
        echo(k.a, k.up, dir(k.up), j.up, dir(j.up), j.a);
        translate(k.up) rotate(to = dir(k.up)) cube(1);
    ";
    let out = clean(src);
    assert_eq!(echoes(&out), ["[11, 2, 3], [0, 0, 5], [0, 0, 1], [0, 100, 5], [0, 0, 1], [-2, 111, 3]"]);
    let j = out.instances.iter().find(|i| i.label.as_deref() == Some("j")).unwrap();
    assert!(approx3(j.matrix.transform_point3(DVec3::X), [0.0, 101.0, 0.0]));
    assert!(span_text(src, j.visible.unwrap()).starts_with("\n        echo"));
}

#[test]
fn anchor_errors() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let cases: &[(&str, &str)] = &[
        ("module p() { anchor a = [0, 0, 0]; } k: p(); echo(dir(k.a));", "hat keine Richtung"),
        ("module p() { anchor a = [0, 0, 0]; } k: p(); echo(k.b);", "hat keinen Anker `b`"),
        ("echo(dir([1, 2, 3]));", "dir() erwartet einen Anker"),
        ("module p() { anchor a = 5; } p();", "Anker braucht einen Punkt"),
        ("module p() { anchor a = [0,0,0]; anchor a = [1,0,0]; } p();", "Anker `a` ist schon deklariert"),
        ("module p() {} for (i = [0:1]) k: p();", "in for-Schleifen nicht erlaubt"),
        ("k: cube(1);", "nur Aufrufe eigener Module sind benennbar"),
        ("module p() {} k: p(); k: p();", "schon deklariert"),
        ("module p() { for (i = [0:1]) { anchor a = [i, 0, 0]; } } p();", "Anker sind in for-Schleifen nicht erlaubt"),
    ];
    for (src, msg) in cases {
        let out = run(src);
        assert!(has_error(&out, msg), "{src}: expected error containing {msg:?}, got {:?}", out.diagnostics);
    }
}

#[test]
fn named_instance_visibility() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let out = run("echo(k.a); module p() { anchor a = [1, 0, 0]; } k: p();");
    assert!(has_warning(&out, "unbekannte Variable `k`"));
    let out = run("module p() { anchor a = [1, 0, 0]; } module q() echo(k.a); k: p(); q();");
    assert!(has_warning(&out, "unbekannte Variable `k`"));
    // A named instance is normal geometry in the tree.
    let out = clean("module p() cube(1); k: p();");
    assert_eq!(dump(&out), "group\n  k:p\n    box\n");
}

#[test]
fn grid_at_anchor_of_instance() {
    let src = "
        module p() { anchor top = [0, 0, 10]; cube(10); }
        k: p();
        translate(k.top) grid g { x: a = 0, b = 5; }
        echo(g[b, _, _]);
    ";
    let out = clean(src);
    assert_eq!(echoes(&out), ["[5, 0, 10]"]);
}

#[test]
fn transforms_of_calls_move_anchors_and_grids() {
    let src = "module m() { grid g { x: a = 1; } anchor e = g[a, _, _]; } translate([0, 0, 7]) k: m(); echo(k.e);";
    let out = clean(src);
    assert_eq!(echoes(&out), ["[1, 0, 7]"]);
    let g = out.grid_by_name("g").unwrap();
    assert_eq!(g.frame, DMat4::from_translation(DVec3::new(0.0, 0.0, 7.0)));
    let k = &out.instances[1];
    assert_eq!(k.world, DMat4::from_translation(DVec3::new(0.0, 0.0, 7.0)));
}

#[test]
fn grid_errors_in_english() {
    let cases: &[(&str, &str)] = &[
        ("grid g { x: a = 0; y: a = 1; }", "plane name `a` already used in grid `g`"),
        ("grid g { x: a = 0; } cube(g[a, b, _]);", "grid `g` has no plane `b`"),
        ("grid g { x: l = [1, 2]; } cube(g[l[5], _, _]);", "plane index 5 out of range"),
        ("for (i = [0:1]) grid g { x: a = i; }", "grids are not allowed in for loops"),
    ];
    for (src, msg) in cases {
        let out = run(src);
        assert!(has_error(&out, msg), "{src}: expected error containing {msg:?}, got {:?}", out.diagnostics);
    }
}
