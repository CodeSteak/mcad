//! Source → `cad_eval` → engine (native SMLib) on the spec examples.

mod common;

use cad_eval::{EvalOptions, eval_source};
use cad_ir::{BlendKind, Op};
use cad_kernel::Kernel;
use common::*;

fn eval(src: &str) -> cad_eval::EvalOutput {
    let out = eval_source(src, &EvalOptions::default());
    let errs: Vec<_> = out.errors().collect();
    assert!(errs.is_empty(), "{errs:#?}");
    out
}

fn find(tree: &cad_ir::Tree, pred: impl Fn(&Op) -> bool) -> u32 {
    tree.nodes.iter().position(|n| pred(&n.op)).expect("node") as u32
}

#[test]
fn spec_gehaeuse() {
    let src = r#"
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
}

module deckel() cube([10, 10, 2]);

k: gehaeuse(w = 60);
translate(k.ecke) deckel();
"#;
    let out = eval(src);
    let mut e = engine();
    let ev = e.evaluate(out.tree);
    assert_clean(&ev);
    // U-profile 60×30×20, wall 3, plus the lid on the corner (touching, disjoint volume)
    let want = 60.0 * 30.0 * 20.0 - 54.0 * 30.0 * 17.0 + 200.0;
    assert_approx("gehaeuse", root_volume(&mut e, &ev), want, 1e-6);
    // the lid sits at k.ecke = [60, 30, 20] in the root system
    let lid = find(&ev.tree, |o| matches!(o, Op::ModuleCall { name, .. } if name == "deckel"));
    let w = ev.report(lid).display_world;
    assert!(w.w_axis.truncate().abs_diff_eq(glam::DVec3::new(60.0, 30.0, 20.0), 1e-9), "{w}");
}

#[test]
fn spec_fillet_zone_block() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
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
    }
}
teil();
"#;
    let out = eval(src);
    let mut e = engine();
    let ev = e.evaluate(out.tree);
    assert_clean(&ev);
    let f = find(&ev.tree, |o| matches!(o, Op::Blend { kind: BlendKind::Fillet, .. }));
    let rep = ev.report(f);
    // bottom rim of the bore (inside the sphere at z = 0) and the edge x = 20, z = 20
    let line = rep
        .selected_edges
        .iter()
        .filter(|e| e.iter().all(|p| (p.x - 20.0).abs() < 1e-6 && (p.z - 20.0).abs() < 1e-6))
        .count();
    let rim = rep
        .selected_edges
        .iter()
        .filter(|e| e.iter().all(|p| ((p.x - 10.0).hypot(p.y - 10.0) - 5.0).abs() < 1e-3 && p.z.abs() < 1e-6))
        .count();
    assert_eq!(line, 1, "{:?}", rep.selected_edges);
    assert!((1..=2).contains(&rim));
    assert_eq!(line + rim, rep.selected_edges.len());
    let n = rep.selected_edges.len();
    assert!(rep.hints.contains(&format!("{n} Kanten")), "{:?}", rep.hints);
    // the file root has no statement
    let root = ev.root().unwrap();
    assert_eq!(ev.tree.node(root).stmt, cad_ir::NO_STMT);
    let vol = root_volume(&mut e, &ev);
    let unfilleted = 8000.0 - std::f64::consts::PI * 25.0 * 20.0;
    assert!(vol < unfilleted && vol > unfilleted - 50.0, "{vol}");
}

const PI: f64 = std::f64::consts::PI;

/// `tests/models/spec/knauf_mirror.mcad` (report 1: open body, torus and pin missing, fillet
/// SM_ERR). Cause: the closed spline profile crossed the revolve axis between its two axis
/// points, which gave self-overlapping revolved solids.
#[test]
fn spec_knauf_mirror_file() {
    let out = eval(include_str!("../../../tests/models/spec/knauf_mirror.mcad"));
    let mut e = engine();
    let ev = e.evaluate(out.tree);
    assert_clean(&ev);
    let root = ev.root().unwrap();
    assert_eq!(ev.parts(root).len(), 1);
    let shape = ev.parts(root)[0].shape.clone();
    assert!(e.kernel_mut().inner.is_solid(&shape));
    let knobs: Vec<u32> = (0..ev.tree.nodes.len() as u32)
        .filter(|&i| matches!(&ev.tree.node(i).op, Op::ModuleCall { name, .. } if name == "knauf"))
        .collect();
    assert_eq!(knobs.len(), 2);
    let (v15, v10) = (volume(&mut e, &ev, knobs[0]), volume(&mut e, &ev, knobs[1]));
    // revolved knob minus bore; the r = 10 copy was 172 mm³ before
    assert!(v15 > 11000.0 && v15 < 11600.0, "knauf r = 15: {v15}");
    assert!(v10 > 4200.0 && v10 < 4700.0, "knauf r = 10: {v10}");
    // the torus lies inside the knob (the spline bulges to z = 22.7 above it); the pin
    // (z 20..23) sticks out of its top
    let (lo, hi) = ev.report(root).bounds.unwrap();
    assert!((hi.z - 23.0).abs() < 1e-6, "{lo} {hi}");
    let total = root_volume(&mut e, &ev);
    let pin = PI * 4.0 * 3.0;
    assert!(total > v15 + v10 + 0.5 * pin && total < v15 + v10 + pin, "{total}");
}

/// `tests/models/spec/winkel_anker.mcad` (report 1: union of touching bodies open, volume 0).
#[test]
fn spec_winkel_anker_file() {
    let out = eval(include_str!("../../../tests/models/spec/winkel_anker.mcad"));
    let mut e = engine();
    let ev = e.evaluate(out.tree);
    assert_clean(&ev);
    let root = ev.root().unwrap();
    let shape = ev.parts(root)[0].shape.clone();
    assert!(e.kernel_mut().inner.is_solid(&shape));
    // brackets 3420 + 4620, screws (head d 6 h 2, shank d 3) minus their 3 mm in the walls,
    // ruler cube 8 (touches bracket a along an edge)
    let head = PI * 9.0 * 2.0;
    let shank = |l: f64| PI * 2.25 * l;
    let want = 3420.0 + 4620.0 + (head + shank(10.0)) + 2.0 * (head + shank(8.0)) - 3.0 * shank(3.0) + 8.0;
    assert_approx("winkel_anker", root_volume(&mut e, &ev), want, 1e-6);
}
