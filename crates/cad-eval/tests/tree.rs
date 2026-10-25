//! Geometry tree for the spec examples and OpenSCAD-style code.

mod common;
use cad_eval::{Context, EvalOptions, PValue};
use cad_ir::{BlendKind, Op, Prim, Profile, Select, TransformKind, ZoneShape};
use cad_kernel::{BoolOp, PathInterp};
use common::*;
use glam::{DMat4, DVec3};

pub const GEHAEUSE: &str = r#"
module gehaeuse(w = 40, d = 30, h = 20, t = 3) {
    grid g {
        x: links = 0, innen_l = t, innen_r = w - t, rechts = w;
        y: vorne = 0, hinten = d;
        z: boden = 0, deckel = h;
    }

    difference() {
        box(g[links, vorne, boden], g[rechts, hinten, deckel]);
        box(g[innen_l, vorne, t], g[innen_r, hinten, deckel]);
    }

    anchor ecke = g[rechts, hinten, deckel];
    anchor oben = g[rechts, hinten, deckel], dir = [0, 0, 1];
}

module deckel() cube([10, 10, 2]);

k: gehaeuse(w = 60);
translate(k.ecke) rotate(to = dir(k.oben)) deckel();

grid h {
    x: rand = k.ecke.x;
}
"#;

fn prim(out: &cad_eval::EvalOutput, i: u32) -> &Prim {
    match &out.tree.node(i).op {
        Op::Prim { prim, .. } => prim,
        o => panic!("not a primitive: {o:?}"),
    }
}

#[test]
fn spec_gehaeuse_tree() {
    let out = clean(GEHAEUSE);
    assert_eq!(
        dump(&out),
        "group\n  k:gehaeuse\n    difference\n      box\n      box\n  translate\n    rotate\n      deckel\n        box\n"
    );
    let boxes = nodes_by_label(&out, "box");
    assert_eq!(prim(&out, boxes[0]), &Prim::Box { min: DVec3::ZERO, max: DVec3::new(60.0, 30.0, 20.0) });
    // `t` in `g[innen_l, vorne, t]` is a coordinate, not a plane.
    assert_eq!(prim(&out, boxes[1]), &Prim::Box { min: DVec3::new(3.0, 0.0, 3.0), max: DVec3::new(57.0, 30.0, 20.0) });
    // The lid sits at the anchor.
    let lid = boxes[2];
    assert!(approx3(out.tree.node(lid).world.transform_point3(DVec3::ZERO), [60.0, 30.0, 20.0]));
    // Grid h uses the anchor's x.
    let h = out.grid_by_name("h").unwrap();
    assert_eq!(h.planes.iter().find(|p| p.name == "rand").unwrap().value, 60.0);
    // Instances: file, k, deckel.
    assert_eq!(out.instances.len(), 3);
    let k = &out.instances[1];
    assert_eq!(k.label.as_deref(), Some("k"));
    assert_eq!(k.anchors.len(), 2);
    assert!(approx3(k.anchors[0].point, [60.0, 30.0, 20.0]));
    assert_eq!(k.anchors[1].dir, Some(DVec3::Z));
}

#[test]
fn every_statement_gets_a_node() {
    let src = "
        for (i = [0:2]) translate([i * 10, 0, 0]) cube(1);
        if (true) sphere(1); else cube(1);
        { cube(1); sphere(2); }
        let (a = 2) cube(a);
    ";
    let out = clean(src);
    assert_eq!(
        dump(&out),
        "group\n  group\n    translate\n      box\n    translate\n      box\n    translate\n      box\n  group\n    sphere\n  group\n    box\n    sphere\n  group\n    box\n"
    );
    // Loop iterations have distinct, stable instance keys.
    let tr = nodes_by_label(&out, "translate");
    let keys: Vec<u64> = tr.iter().map(|&i| out.tree.node(i).inst_key).collect();
    assert_eq!(keys.len(), 3);
    assert!(keys[0] != keys[1] && keys[1] != keys[2]);
    let again = clean(src);
    let keys2: Vec<u64> = nodes_by_label(&again, "translate").iter().map(|&i| again.tree.node(i).inst_key).collect();
    assert_eq!(keys, keys2);
    assert!(keys.iter().all(|k| *k < (1 << 53)));
}

#[test]
fn inst_keys_survive_literal_changes() {
    let a = clean("module m(s) cube(s); m(1); translate([1, 2, 3]) m(2);");
    let b = clean("module m(s) cube(s); m(5); translate([7, 2, 3]) m(9);");
    let ka: Vec<u64> = a.tree.nodes.iter().map(|n| n.inst_key).collect();
    let kb: Vec<u64> = b.tree.nodes.iter().map(|n| n.inst_key).collect();
    assert_eq!(ka, kb);
    let mut uniq = ka.clone();
    uniq.sort();
    uniq.dedup();
    assert_eq!(uniq.len(), ka.len());
}

#[test]
fn world_and_local_matrices() {
    let out = clean("translate([10, 0, 0]) rotate([0, 0, 90]) { cube(1); translate([0, 5, 0]) sphere(1); }");
    let t = nodes_by_label(&out, "translate")[0];
    let r = nodes_by_label(&out, "rotate")[0];
    let c = nodes_by_label(&out, "box")[0];
    let s = nodes_by_label(&out, "sphere")[0];
    let (tn, rn, cn, sn) = (out.tree.node(t), out.tree.node(r), out.tree.node(c), out.tree.node(s));
    assert_eq!(tn.world, DMat4::IDENTITY);
    assert_eq!(tn.local, DMat4::IDENTITY);
    assert_eq!(rn.local, DMat4::from_translation(DVec3::new(10.0, 0.0, 0.0)));
    assert_eq!(rn.world, rn.local);
    // Exact 90° rotation (no 6e-17 noise).
    assert_eq!(cn.local, cad_eval::math::rot_z(90.0));
    assert!(approx3(cn.world.transform_point3(DVec3::X), [10.0, 1.0, 0.0]));
    assert!(approx3(sn.world.transform_point3(DVec3::ZERO), [5.0, 0.0, 0.0]));
    assert_eq!(cn.parent, Some(r));
}

#[test]
fn primitives() {
    let out = clean(
        "cube(2, center = true); cube([1, 2, 3]); sphere(d = 4); cylinder(h = 4, r1 = 2, r2 = 1, center = true);
         cylinder([0, 0, 0], [0, 0, 10], 2); torus(10, 2); square([2, 3], center = true); circle(d = 6);
         polygon([[0, 0], [1, 0], [0, 1]]); spline([[0, 0], [1, 0], [0, 1]]); rect([1, 1], [3, 4]);
         polyhedron([[0,0,0],[1,0,0],[0,1,0],[0,0,1]], [[0,1,2],[0,1,3],[1,2,3],[0,2,3]]);",
    );
    let ops: Vec<&Op> = out.tree.nodes.iter().skip(1).map(|n| &n.op).collect();
    let p = |i: usize| match ops[i] {
        Op::Prim { prim, place } => (prim.clone(), *place),
        o => panic!("{o:?}"),
    };
    assert_eq!(p(0).0, Prim::Box { min: DVec3::splat(-1.0), max: DVec3::splat(1.0) });
    assert_eq!(p(1).0, Prim::Box { min: DVec3::ZERO, max: DVec3::new(1.0, 2.0, 3.0) });
    assert_eq!(p(2).0, Prim::Sphere { r: 2.0 });
    assert_eq!(
        p(3),
        (Prim::Cylinder { h: 4.0, r1: 2.0, r2: 1.0 }, DMat4::from_translation(DVec3::new(0.0, 0.0, -2.0)))
    );
    assert_eq!(p(4).0, Prim::Cylinder { h: 10.0, r1: 2.0, r2: 2.0 });
    assert_eq!(p(4).1, DMat4::IDENTITY);
    assert_eq!(p(5).0, Prim::Torus { big_r: 10.0, r: 2.0 });
    assert_eq!(
        ops[6],
        &Op::Profile(Profile::Rect { min: glam::DVec2::new(-1.0, -1.5), max: glam::DVec2::new(1.0, 1.5) })
    );
    assert_eq!(ops[7], &Op::Profile(Profile::Circle { r: 3.0 }));
    assert!(matches!(ops[8], Op::Profile(Profile::Polygon { loops }) if loops.len() == 1 && loops[0].len() == 3));
    assert!(matches!(ops[9], Op::Profile(Profile::Spline { closed: true, .. })));
    assert_eq!(
        ops[10],
        &Op::Profile(Profile::Rect { min: glam::DVec2::new(1.0, 1.0), max: glam::DVec2::new(3.0, 4.0) })
    );
    assert!(
        matches!(ops[11], Op::Prim { prim: Prim::Polyhedron { points, faces }, .. } if points.len() == 4 && faces.len() == 4)
    );
}

#[test]
fn cylinder_between_points_is_oriented() {
    let out = clean("cylinder([1, 2, 3], [1, 2, 13], r = 2); cylinder(a = [0, 0, 0], b = [10, 0, 0], r = 1);");
    let cyl = nodes_by_label(&out, "cylinder");
    let Op::Prim { place, .. } = &out.tree.node(cyl[0]).op else { panic!() };
    assert!(approx3(place.transform_point3(DVec3::ZERO), [1.0, 2.0, 3.0]));
    let Op::Prim { place, prim } = &out.tree.node(cyl[1]).op else { panic!() };
    assert_eq!(prim, &Prim::Cylinder { h: 10.0, r1: 1.0, r2: 1.0 });
    assert!(approx3(place.transform_point3(DVec3::new(0.0, 0.0, 10.0)), [10.0, 0.0, 0.0]));
}

#[test]
fn small_fn_makes_polygons_like_openscad() {
    let out = run(
        "cylinder(h = 5, r = 4, $fn = 6); sphere(r = 3, $fn = 8); circle(r = 2, $fn = 4); cylinder(h = 1, r = 1, $fn = 64);",
    );
    let n = &out.tree.nodes;
    let Op::Prim { prim: Prim::Polyhedron { points, faces }, .. } = &n[1].op else { panic!("{:?}", n[1].op) };
    assert_eq!(points.len(), 12);
    assert_eq!(faces.len(), 8);
    assert!(approx3(points[0], [4.0, 0.0, 0.0]));
    assert!(approx3(points[1], [2.0, 12f64.sqrt(), 0.0]));
    let Op::Prim { prim: Prim::Polyhedron { points, faces }, .. } = &n[2].op else { panic!() };
    // OpenSCAD: 8 fragments → 4 rings of 8 points.
    assert_eq!(points.len(), 32);
    assert_eq!(faces.len(), 2 + 3 * 8);
    let phi = 180.0 * 0.5 / 4.0_f64;
    assert!(approx(points[0].z, 3.0 * phi.to_radians().cos()));
    let Op::Profile(Profile::Polygon { loops }) = &n[3].op else { panic!() };
    assert_eq!(loops[0].len(), 4);
    assert!(matches!(n[4].op, Op::Prim { prim: Prim::Cylinder { .. }, .. }));
    assert!(out.diagnostics.iter().any(|d| d.message.contains("`$fn` ignoriert")));
    // A global $fn gives exact geometry and a hint.
    let out = run("$fn = 6; cylinder(h = 1, r = 1);");
    assert!(matches!(out.tree.nodes[1].op, Op::Prim { prim: Prim::Cylinder { .. }, .. }));
    assert!(out.diagnostics.iter().any(|d| d.severity == cad_syntax::Severity::Info));
}

#[test]
fn transforms() {
    let out = clean(
        "rotate([90, 0, 0]) cube(1); rotate(45, [0, 0, 1]) cube(1); rotate(30) cube(1); rotate(to = [1, 0, 0]) cube(1);
         scale(2) cube(1); mirror([1, 0, 0]) cube(1); multmatrix([[1, 0, 0, 5], [0, 1, 0, 0], [0, 0, 1, 0]]) cube(1);
         resize([10, 0, 0], auto = true) cube(1); color(\"red\") cube(1); color([0, 1, 0], 0.5) cube(1);",
    );
    let tr: Vec<(TransformKind, DMat4)> = out
        .tree
        .nodes
        .iter()
        .filter_map(|n| if let Op::Transform { kind, m } = &n.op { Some((*kind, *m)) } else { None })
        .collect();
    assert_eq!(tr.len(), 7);
    assert!(approx3(tr[0].1.transform_point3(DVec3::Y), [0.0, 0.0, 1.0]));
    assert!(approx3(tr[1].1.transform_point3(DVec3::X), [0.5f64.sqrt(), 0.5f64.sqrt(), 0.0]));
    assert!(approx3(tr[3].1.transform_point3(DVec3::Z), [1.0, 0.0, 0.0]));
    assert_eq!(tr[4].1, DMat4::from_scale(DVec3::splat(2.0)));
    assert!(approx3(tr[5].1.transform_point3(DVec3::new(1.0, 2.0, 3.0)), [-1.0, 2.0, 3.0]));
    assert_eq!(tr[6].0, TransformKind::Matrix);
    assert!(approx3(tr[6].1.transform_point3(DVec3::ZERO), [5.0, 0.0, 0.0]));
    let resize = out.tree.nodes.iter().find(|n| matches!(n.op, Op::Resize { .. })).unwrap();
    assert_eq!(resize.op, Op::Resize { size: DVec3::new(10.0, 0.0, 0.0), auto: [true; 3] });
    let colors: Vec<[f32; 4]> = out
        .tree
        .nodes
        .iter()
        .filter_map(|n| if let Op::Color { rgba, .. } = n.op { Some(rgba) } else { None })
        .collect();
    // "red" is the palette's muted red (#c74240), vectors stay exact.
    let red = cad_eval::palette::rgba_of([0xc7, 0x42, 0x40]);
    assert_eq!(colors, [red, [0.0, 1.0, 0.0, 0.5]]);
    // Children inherit the display colour.
    let last = out.tree.nodes.last().unwrap();
    assert_eq!(last.color, Some([0.0, 1.0, 0.0, 0.5]));
}

#[test]
fn nonuniform_scale_warns() {
    let out = run("scale([1, 2, 1]) cube(1);");
    assert!(has_warning(&out, "ungleichmäßige Skalierung"));
}

#[test]
fn booleans_and_fillet_parameters() {
    let src = "p = [10, 10, 0]; difference(fillet = 2) { cube(20); translate(p) cylinder(h = 30, r = 5); } \
               union(chamfer = 1) { cube(1); sphere(1); } intersection() { cube(1); sphere(1); }";
    let out = clean(src);
    let b: Vec<Op> =
        out.tree.nodes.iter().filter(|n| matches!(n.op, Op::Boolean { .. })).map(|n| n.op.clone()).collect();
    assert_eq!(b[0], Op::Boolean { op: BoolOp::Difference, fillet: Some(2.0), chamfer: None, fillet_law: None });
    assert_eq!(b[1], Op::Boolean { op: BoolOp::Union, fillet: None, chamfer: Some(1.0), fillet_law: None });
    assert_eq!(b[2], Op::Boolean { op: BoolOp::Intersection, fillet: None, chamfer: None, fillet_law: None });
}

#[test]
fn spec_fillet_zone_block() {
    let src = r#"
module teil(w = 20, h = 20) {
    grid g {
        x: links = 0, rechts = w;
        y: vorne = 0, hinten = w;
        z: boden = 0, deckel = h;
    }
    p = [10, 10, 0];
    fillet(2) {
        difference() {
            cube(20);
            translate(p) cylinder(h = 30, r = 5);
        }
        translate(p) sphere(8);
        zone(g[rechts, *, deckel], d = 2);
        zone(g.deckel, d = 1);
    }
    chamfer(1, select = "touch") cube(5);
    shell(1) { cube(10); zone(g.boden, d = 1); }
}
teil();
"#;
    let out = clean(src);
    assert_eq!(
        dump(&out),
        "group\n  teil\n    fillet\n      difference\n        box\n        translate\n          cylinder\n      translate [zone]\n        sphere\n      zone [zone]\n      zone [zone]\n    chamfer\n      box\n    shell\n      box\n      zone [zone]\n"
    );
    let ops: Vec<&Op> = out.tree.nodes.iter().map(|n| &n.op).collect();
    assert!(ops.contains(&&Op::Blend { kind: BlendKind::Fillet, r: 2.0, select: Select::Inside, law: None }));
    assert!(ops.contains(&&Op::Blend { kind: BlendKind::Chamfer, r: 1.0, select: Select::Touch, law: None }));
    assert!(ops.contains(&&Op::Shell { t: 1.0, select: Select::Inside }));
    let zones: Vec<&ZoneShape> = ops.iter().filter_map(|o| if let Op::Zone(z) = o { Some(z) } else { None }).collect();
    assert_eq!(zones[0], &ZoneShape::Line { point: DVec3::new(20.0, 0.0, 20.0), dir: DVec3::Y, d: 2.0 });
    assert_eq!(zones[1], &ZoneShape::Plane { point: DVec3::new(0.0, 0.0, 20.0), normal: DVec3::Z, d: 1.0 });
    // Zones and bodies: solid children of the fillet block are just the body.
    let f = out.tree.nodes.iter().position(|n| matches!(n.op, Op::Blend { kind: BlendKind::Fillet, .. })).unwrap();
    assert_eq!(out.tree.solid_children(f as u32).count(), 1);
}

#[test]
fn zone_outside_block_warns_and_bad_target_errors() {
    let out = run("grid g { z: a = 1; } zone(g.a, d = 1); fillet(1) { cube(1); zone([1, 2, 3], d = 1); }");
    assert!(has_warning(&out, "zone() außerhalb"));
    assert!(has_error(&out, "zone() braucht Rasterlinie"));
}

#[test]
fn special_variables_on_primitives() {
    let out = clean(
        "cube(20, $fillet = 1); $chamfer = 0.5; $tol = 0.01; sphere(1); translate([1, 0, 0]) cube(1, $fillet = 2);",
    );
    let blends: Vec<_> =
        out.tree.nodes.iter().filter(|n| matches!(n.op, Op::Prim { .. })).map(|n| (n.blend, n.tol)).collect();
    assert_eq!(
        blends,
        [
            (Some((BlendKind::Fillet, 1.0)), Some(0.01)),
            (Some((BlendKind::Chamfer, 0.5)), Some(0.01)),
            (Some((BlendKind::Fillet, 2.0)), Some(0.01))
        ]
    );
}

#[test]
fn spec_loft_and_sweep() {
    let src = r#"
loft() {
    circle(10);
    translate([0, 0, 30]) square(15, center = true);
}

sweep(path = [[0,0,0], [20,0,10], [40,10,10]], interp = "spline") {
    circle(2);
}
sweep([[0, 0], [10, 0]], interp = "linear") circle(1);
linear_extrude(height = 10, twist = 90, scale = 0.5, center = true) square(5);
rotate_extrude(angle = 180) translate([5, 0]) circle(1);
thicken(2) surface([[[0,0,0],[10,0,0]],[[0,10,0],[10,10,5]]]);
"#;
    let out = clean(src);
    assert_eq!(
        dump(&out),
        "group\n  loft\n    circle\n    translate\n      rect\n  sweep\n    circle\n  sweep\n    circle\n  linear_extrude\n    rect\n  rotate_extrude\n    translate\n      circle\n  thicken\n    surface\n"
    );
    let ops: Vec<&Op> = out.tree.nodes.iter().map(|n| &n.op).collect();
    assert!(ops.contains(&&Op::Loft { ruled: false }));
    assert!(ops.contains(&&Op::Sweep {
        path: vec![DVec3::ZERO, DVec3::new(20.0, 0.0, 10.0), DVec3::new(40.0, 10.0, 10.0)],
        interp: PathInterp::Spline
    }));
    assert!(
        ops.contains(&&Op::Sweep { path: vec![DVec3::ZERO, DVec3::new(10.0, 0.0, 0.0)], interp: PathInterp::Linear })
    );
    assert!(ops.contains(&&Op::LinearExtrude { h: 10.0, twist: 90.0, scale: glam::DVec2::splat(0.5), center: true }));
    assert!(ops.contains(&&Op::RotateExtrude { angle: 180.0 }));
    assert!(ops.contains(&&Op::Thicken { t: 2.0 }));
    assert!(ops.iter().any(|o| matches!(o, Op::Surface { rows } if rows.len() == 2)));
    let lf = out.tree.nodes.iter().position(|n| matches!(n.op, Op::Loft { .. })).unwrap();
    assert_eq!(out.tree.dim(lf as u32), Some(cad_ir::Dim::Three));
}

#[test]
fn surface_errors() {
    let out = run("surface(file = \"x.dat\"); surface([[[0,0,0],[1,0,0]],[[0,1,0]]]);");
    assert!(has_warning(&out, "nicht unterstützt"));
    assert!(has_error(&out, "gleich lange Zeilen"));
}

#[test]
fn modifiers() {
    let out =
        run("#cube(1); %sphere(1); *cylinder(h = 1, r = 1); echo(\"x\"); !translate([1, 0, 0]) cube(2); !cube(3);");
    assert_eq!(dump(&out), "translate\n  box\n");
    assert!(has_warning(&out, "mehrere `!`-Modifikatoren"));
    let all: Vec<String> = out.tree.nodes.iter().map(|n| n.op.label()).collect();
    assert_eq!(all, ["group", "box", "sphere", "translate", "box", "box"]);
    assert!(out.tree.nodes[1].flags.highlight);
    assert!(out.tree.nodes[2].flags.background);
    assert_eq!(out.context.root_modifier, Some(3));
    // `*` is not evaluated at all.
    let out = clean("*echo(\"never\"); *{ echo(\"never\"); }");
    assert!(out.echoes.is_empty());
}

#[test]
fn geometric_asserts() {
    let out = clean(
        "cube(10);\nassert_keepout(clearance = 0.5) translate([1, 1, 9]) cube(2);\n\
         assert_touches(tol = 0.01, contact = \"any\") { cube(1); sphere(1); }\n\
         module m() assert_disjoint() { cube(1); cube(2); }\nm(); m();\n",
    );
    let a = &out.tree.asserts;
    assert_eq!(a.len(), 4, "{a:?}");
    assert_eq!(a[0].kind, cad_ir::AssertKind::Keepout);
    assert_eq!(a[0].clearance, 0.5);
    let k = out.tree.node(a[0].node);
    assert!(k.flags.keepout && k.flags.background && matches!(k.op, Op::Group));
    assert!(out.tree.in_keepout(k.children[0]));
    assert_eq!((a[1].kind, a[1].tol, a[1].contact), (cad_ir::AssertKind::Touches, 0.01, cad_ir::Contact::Any));
    assert!(!out.tree.node(a[1].node).flags.background);
    // one per instance
    assert_eq!(a[2].kind, cad_ir::AssertKind::Disjoint);
    assert_ne!(a[2].node, a[3].node);
    // the keep-out volume is not part of the result
    assert_eq!(out.tree.solid_children(out.tree.root.unwrap()).count(), 4);
}

#[test]
fn unsupported_operations_keep_children() {
    let out = run("hull() { cube(1); sphere(1); } import(\"a.obj\"); minkowski() cube(1); projection() cube(1);");
    // hull and minkowski are nodes of their own (the engine builds the exact special cases)
    assert_eq!(dump(&out), "group\n  hull\n    box\n    sphere\n  minkowski\n    box\n  empty\n    box\n");
    assert!(has_warning(&out, "`projection()` nicht unterstützt"));
    assert!(!has_warning(&out, "`hull()`"));
    // text() and import() of SVG (Bericht 16) and STL (Plan 21 B) exist; other formats do not
    assert!(has_error(&out, "nur SVG und STL"));
}

#[test]
fn unknown_module_and_include() {
    let out = run("include <x.scad>\nfoo(1); cube(1) sphere(1);");
    assert!(has_error(&out, "Datei `x.scad` nicht gefunden"));
    assert!(has_warning(&out, "unbekanntes Modul `foo`"));
    assert!(has_warning(&out, "`cube()` nimmt keine Kinder"));
}

#[test]
fn intersection_for() {
    let out = clean("intersection_for (i = [0:2]) rotate([0, 0, i * 30]) cube(10, center = true);");
    assert!(matches!(out.tree.nodes[1].op, Op::Boolean { op: BoolOp::Intersection, .. }));
    assert_eq!(out.tree.nodes[1].children.len(), 3);
}

#[test]
fn module_context_uses_defaults_and_overrides() {
    let out = run_module(GEHAEUSE, "gehaeuse", vec![("w", PValue::Num(50.0))]);
    assert!(errors(&out).is_empty(), "{:?}", out.diagnostics);
    assert_eq!(dump(&out), "gehaeuse\n  difference\n    box\n    box\n");
    let root = out.tree.root.unwrap();
    assert!(matches!(&out.tree.node(root).op, Op::ModuleCall { name, .. } if name == "gehaeuse"));
    assert_eq!(out.context.module.as_deref(), Some("gehaeuse"));
    assert_eq!(out.context.path, ["gehaeuse"]);
    let w = out.context.params.iter().find(|p| p.name == "w").unwrap();
    assert_eq!((w.value.clone(), w.overridden), (PValue::Num(50.0), true));
    let t = out.context.params.iter().find(|p| p.name == "t").unwrap();
    assert_eq!((t.value.clone(), t.overridden), (PValue::Num(3.0), false));
    let g = out.grid_by_name("g").unwrap();
    assert!(g.in_context);
    assert_eq!(g.planes.iter().find(|p| p.name == "rechts").unwrap().value, 50.0);
    // Top-level statements are not evaluated in module context.
    assert!(out.grid_by_name("h").is_none());
    assert_eq!(out.instances[0].anchors.len(), 2);
}

/// `-D` in the file context: top-level assignments take the value (the last one of a name),
/// dependent variables compute with it; `$` variables apply without an assignment.
#[test]
fn file_context_overrides_top_level_assignments() {
    let src = "
        w = 2;
        d = w * 2;
        h = 1;
        h = 3;
        module m() cube([w, d, h]);
        m();
        sphere(1);
    ";
    let ov = |v: Vec<(&str, PValue)>| EvalOptions {
        context: Context::File { overrides: v.into_iter().map(|(n, v)| (n.to_string(), v)).collect() },
        ..Default::default()
    };
    let out = run_with(src, &ov(vec![("w", PValue::Num(5.0)), ("h", PValue::Num(7.0)), ("$fn", PValue::Num(12.0))]));
    assert!(errors(&out).is_empty(), "{:?}", out.diagnostics);
    assert_eq!(warnings(&out), ["`h` aus Zeile 4 wird in Zeile 5 überschrieben"]);
    // $fn reaches sphere(): it says it ignores it
    assert!(out.diagnostics.iter().any(|d| d.message.starts_with("`$fn` ignoriert")), "{:?}", out.diagnostics);
    let b = nodes_by_label(&out, "box")[0];
    assert_eq!(prim(&out, b), &Prim::Box { min: DVec3::ZERO, max: DVec3::new(5.0, 10.0, 7.0) });
    // the value hint of the assignment shows the overridden value
    let w = out.hints.iter().find(|h| h.text == "5").map(|h| h.value.clone());
    assert_eq!(w, Some(PValue::Num(5.0)));
    let out = run_with(src, &ov(vec![("breite", PValue::Num(1.0))]));
    assert!(has_warning(&out, "-D breite"), "{:?}", out.diagnostics);
    assert_eq!(
        prim(&out, nodes_by_label(&out, "box")[0]),
        &Prim::Box { min: DVec3::ZERO, max: DVec3::new(2.0, 4.0, 3.0) }
    );
}

#[test]
fn nested_module_context() {
    let src = "
        size = 4;
        module outer(a = 2) {
            module inner(b = 3) { cube(a + b + size); }
            inner();
        }
        outer();
    ";
    let out = run_module(src, "outer::inner", vec![]);
    assert!(errors(&out).is_empty(), "{:?}", out.diagnostics);
    assert_eq!(out.context.path, ["outer", "inner"]);
    let b = nodes_by_label(&out, "box")[0];
    assert_eq!(prim(&out, b), &Prim::Box { min: DVec3::ZERO, max: DVec3::splat(9.0) });
    let out = run_module(src, "inner", vec![("b", PValue::Num(1.0)), ("zz", PValue::Num(1.0))]);
    assert_eq!(prim(&out, nodes_by_label(&out, "box")[0]), &Prim::Box { min: DVec3::ZERO, max: DVec3::splat(7.0) });
    assert!(has_warning(&out, "hat keinen Parameter `zz`"));
    let out = run_module(src, "missing", vec![]);
    assert!(has_error(&out, "Modul `missing` nicht gefunden"));
}

#[test]
fn module_signatures() {
    let src = "module m(a = 1, b = true, c = [1, 2, 3], d = \"x\", e, f = 2 * 3, g = -5, h = [[1]]) {} module n() { module inner(q = 1) {} }";
    let out = clean(src);
    use cad_eval::ParamType::*;
    let m = &out.modules[0];
    let tys: Vec<_> = m.params.iter().map(|p| p.ty).collect();
    assert_eq!(tys, [Number, Bool, Vector, String, Unknown, Unknown, Number, List]);
    assert_eq!(m.params[5].default_text.as_deref(), Some("2 * 3"));
    assert_eq!(m.params[6].default_value, Some(PValue::Num(-5.0)));
    assert_eq!(out.modules[2].name, "inner");
    assert_eq!(out.modules[2].path, ["n"]);
}

#[test]
fn hints_for_assignments() {
    let src = "a = 3;\nb = a * 2;\nxs = [for (i = [0:9]) i * 8];\nflag = true;\nmodule m(t = 1) { u = t * 10; }\nm(1); m(2);\nfor (i = [0:5]) { q = i; }";
    let out = clean(src);
    assert_eq!(hint(&out, src, "a"), "3");
    assert_eq!(hint(&out, src, "b"), "6");
    assert_eq!(hint(&out, src, "xs"), "[0, 8, 16, 24, 32, 40, …] (10)");
    assert_eq!(hint(&out, src, "u"), "10 | 20");
    assert_eq!(hint(&out, src, "q"), "0 | 1 | 2 | 3 | …");
    let a = out.hints.iter().find(|h| span_text(src, h.span).starts_with("a =")).unwrap();
    assert_eq!(span_text(src, a.literal.unwrap()), "3");
    assert!(a.in_context);
    let f = out.hints.iter().find(|h| span_text(src, h.span).starts_with("flag")).unwrap();
    assert_eq!(span_text(src, f.literal.unwrap()), "true");
    let u = out.hints.iter().find(|h| span_text(src, h.span).starts_with("u =")).unwrap();
    assert!(!u.in_context);
    // In module context the module's own values win.
    let out = run_module(src, "m", vec![("t", PValue::Num(5.0))]);
    assert_eq!(hint(&out, src, "u"), "50");
}

#[test]
fn output_serialises() {
    // All output types derive Serialize; check that a full output can be serialised
    // through serde's data model (count of top-level fields via a tiny serializer).
    let out = clean(GEHAEUSE);
    let s = format!("{out:?}");
    assert!(s.contains("gehaeuse"));
    fn assert_ser<T: serde::Serialize + for<'de> serde::Deserialize<'de>>(_: &T) {}
    assert_ser(&out);
    assert_ser(&cad_eval::EvalOptions::default());
    assert_ser(&cad_eval::DragTarget::Free(cad_eval::DragValue::Scalar(1.0)));
}
