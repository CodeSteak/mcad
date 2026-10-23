//! Engine integration tests with the native SMLib kernel and hand-built trees.

mod common;

use std::f64::consts::PI;

use cad_engine::builder::*;
use cad_engine::{Budget, Engine, Status, Step};
use cad_ir::{BlendKind, NodeIdx, Op, Select, Tree};
use cad_kernel::{Kernel, PathInterp};
use common::*;
use glam::{DVec2, DVec3};

/// difference() { cube(20); translate([10, 10, -5]) cylinder(h = 30, r = 5); }
/// Returns (tree, difference, cube, cylinder).
fn box_minus_cyl(fillet: Option<f64>) -> (Tree, NodeIdx, NodeIdx, NodeIdx) {
    let mut b = TreeBuilder::new();
    let d = b.root(match fillet {
        Some(r) => difference_fillet(r),
        None => difference(),
    });
    let c = b.child(d, cube(20.0));
    let t = b.child(d, translate(v(10.0, 10.0, -5.0)));
    let y = b.child(t, cylinder(30.0, 5.0));
    (b.finish(), d, c, y)
}

const BORE_V: f64 = 8000.0 - PI * 25.0 * 20.0;

fn on_bore(p: DVec3) -> bool {
    ((p.x - 10.0).hypot(p.y - 10.0) - 5.0).abs() < 1e-3
}

#[test]
fn boolean_volumes() {
    let mut e = engine();
    let (t, ..) = box_minus_cyl(None);
    let ev = e.evaluate(t);
    assert_clean(&ev);
    assert_approx("difference", root_volume(&mut e, &ev), BORE_V, 1e-3);

    // union of two overlapping cubes and their intersection
    for (op, want) in [(union(), 2000.0 - 125.0), (intersection(), 125.0)] {
        let mut b = TreeBuilder::new();
        let u = b.root(op);
        b.child(u, cube(10.0));
        let t = b.child(u, translate(v(5.0, 5.0, 5.0)));
        b.child(t, cube(10.0));
        let ev = e.evaluate(b.finish());
        assert_clean(&ev);
        assert_approx("bool", root_volume(&mut e, &ev), want, 1e-6);
    }

    // implicit union (group) of disjoint parts, under a rotation
    let mut b = TreeBuilder::new();
    let g = b.root(group());
    let r = b.child(g, rotate_z(30.0));
    b.child(r, cube(10.0));
    let t = b.child(g, translate(v(50.0, 0.0, 0.0)));
    b.child(t, sphere(5.0));
    let ev = e.evaluate(b.finish());
    assert_clean(&ev);
    assert_approx("group", root_volume(&mut e, &ev), 1000.0 + 4.0 / 3.0 * PI * 125.0, 1e-3);
    assert_eq!(ev.report(g).parts, 1);
}

#[test]
fn difference_fillet_selects_bore_rims_only() {
    let mut e = engine();
    let (t, d, ..) = box_minus_cyl(Some(1.0));
    let ev = e.evaluate(t);
    assert_clean(&ev);
    let rep = ev.report(d);
    let n = rep.selected_edges.len();
    assert!((2..=4).contains(&n), "rim edges: {n}");
    for e in &rep.selected_edges {
        assert!(e.iter().all(|p| on_bore(*p) && (p.z.abs() < 1e-6 || (p.z - 20.0).abs() < 1e-6)), "{e:?}");
    }
    // both rims
    assert!(rep.selected_edges.iter().any(|e| e[0].z < 1.0));
    assert!(rep.selected_edges.iter().any(|e| e[0].z > 19.0));
    assert_eq!(rep.hints, vec![format!("{n} Kanten")]);
    // Fillet adds material: (1 - π/4) r² per length, rim length 2π(5 + …) — just check the sign.
    let vol = root_volume(&mut e, &ev);
    assert!(vol < BORE_V && vol > BORE_V - 40.0, "{vol}");
}

#[test]
fn zone_block_sphere_and_line() {
    let mut e = engine();
    // fillet(1) { difference() { cube(20); bore }; translate([10,10,20]) sphere(8); }
    let mut b = TreeBuilder::new();
    let f = b.root(fillet(1.0, Select::Inside));
    let d = b.child(f, difference());
    b.child(d, cube(20.0));
    let t = b.child(d, translate(v(10.0, 10.0, -5.0)));
    b.child(t, cylinder(30.0, 5.0));
    let zt = b.child(f, translate(v(10.0, 10.0, 20.0)));
    b.flags(zt).zone = true;
    let zs = b.child(zt, sphere(8.0));
    // zone around the grid line x = 20, z = 20 (along y)
    let zl = b.child(f, zone_line(v(20.0, 0.0, 20.0), DVec3::Y, 2.0));
    b.flags(zl).zone = true;
    let ev = e.evaluate(b.finish());
    assert_clean(&ev);
    let rep = ev.report(f);
    let top_rim: Vec<_> = rep.selected_edges.iter().filter(|e| e.iter().all(|p| on_bore(*p))).collect();
    let line: Vec<_> = rep
        .selected_edges
        .iter()
        .filter(|e| e.iter().all(|p| (p.x - 20.0).abs() < 1e-6 && (p.z - 20.0).abs() < 1e-6))
        .collect();
    assert!(!top_rim.is_empty() && top_rim.len() <= 2, "{:?}", rep.selected_edges);
    assert!(top_rim.iter().all(|e| e.iter().all(|p| (p.z - 20.0).abs() < 1e-6)));
    assert_eq!(line.len(), 1);
    assert_eq!(top_rim.len() + line.len(), rep.selected_edges.len());
    assert!(rep.hints.contains(&"2 Zonen".to_string()));
    // zones are tessellatable on their own and don't change the result's bounds
    assert!(!e.tessellate(&ev, zs, Some(0.2)).unwrap().is_empty());
    let zone_mesh = e.tessellate(&ev, zl, Some(0.2)).unwrap();
    let (lo, hi) = zone_mesh.bounds().unwrap();
    // limited to the body's bounds + d along the line
    assert!(lo.y > -2.01 && hi.y < 22.01 && lo.y < -1.9 && hi.y > 21.9, "{lo} {hi}");
    let (blo, bhi) = rep.bounds.unwrap();
    assert!(blo.abs_diff_eq(DVec3::ZERO, 1e-6) && bhi.abs_diff_eq(DVec3::splat(20.0), 1e-6));
    assert!(root_volume(&mut e, &ev) < BORE_V);
}

#[test]
fn zone_select_touch_and_empty_selection() {
    let mut e = engine();
    for (sel, want) in [(Select::Touch, 3usize), (Select::Inside, 0)] {
        let mut b = TreeBuilder::new();
        let f = b.root(fillet(1.0, sel));
        b.child(f, cube(20.0));
        let t = b.child(f, translate(v(20.0, 20.0, 20.0)));
        b.flags(t).zone = true;
        b.child(t, sphere(3.0));
        let ev = e.evaluate(b.finish());
        let rep = ev.report(f);
        assert_eq!(rep.selected_edges.len(), want, "{sel:?}");
        if want == 0 {
            assert_eq!(rep.status, Status::Warning);
            assert_eq!(warnings(&ev), vec!["0 Kanten".to_string()]);
            assert!(rep.hints.contains(&"0 Kanten".to_string()));
            assert_approx("unchanged", root_volume(&mut e, &ev), 8000.0, 1e-9);
        } else {
            assert_clean(&ev);
            assert!(root_volume(&mut e, &ev) < 8000.0);
        }
    }
}

#[test]
fn nested_blocks_inner_first_outer_sees_new_edges() {
    let mut e = engine();
    // fillet(1) { fillet(3) { cube(20); zone(line x=20,z=20) }; zone(plane z=20, d=2) }
    let mut b = TreeBuilder::new();
    let outer = b.root(fillet(1.0, Select::Inside));
    let inner = b.child(outer, fillet(3.0, Select::Inside));
    b.child(inner, cube(20.0));
    let zl = b.child(inner, zone_line(v(20.0, 0.0, 20.0), DVec3::Y, 2.0));
    b.flags(zl).zone = true;
    let zp = b.child(outer, zone_plane(v(0.0, 0.0, 20.0), DVec3::Z, 2.0));
    b.flags(zp).zone = true;
    let ev = e.evaluate(b.finish());
    assert_eq!(errors(&ev), Vec::<String>::new());
    assert_eq!(ev.report(inner).selected_edges.len(), 1);
    let sel = &ev.report(outer).selected_edges;
    // the old edge is gone; the inner fillet's boundary at x = 17 on the top face is tangent
    // (smooth) and therefore not selected; the three other top edges are
    assert!(!sel.iter().any(|e| e.iter().all(|p| (p.x - 17.0).abs() < 1e-3 && (p.z - 20.0).abs() < 1e-6)), "{sel:?}");
    assert!(!sel.iter().any(|e| e.iter().all(|p| (p.x - 20.0).abs() < 1e-6 && (p.z - 20.0).abs() < 1e-6)));
    assert_eq!(sel.len(), 3, "{sel:?}");
    let v_inner = volume(&mut e, &ev, inner);
    assert_approx("inner fillet", v_inner, 8000.0 - (1.0 - PI / 4.0) * 9.0 * 20.0, 1e-4);
    assert!(volume(&mut e, &ev, outer) < v_inner);
}

#[test]
fn dollar_fillet_on_primitive() {
    let mut e = engine();
    let mut b = TreeBuilder::new();
    let c = b.root(cube(10.0));
    b.set_blend(c, BlendKind::Fillet, 1.0);
    let ev = e.evaluate(b.finish());
    assert_clean(&ev);
    assert_eq!(ev.report(c).hints, vec!["12 Kanten".to_string()]);
    assert_eq!(ev.report(c).selected_edges.len(), 12);
    // rounded box: 1000 − 12·8·(1 − π/4) − corners (8 · (1 − π/6 − …)) ≈ 977.8
    let vol = root_volume(&mut e, &ev);
    let want = 8.0f64.powi(3) + 6.0 * 64.0 + 12.0 * 8.0 * PI / 4.0 + 4.0 / 3.0 * PI;
    assert_approx("rounded cube", vol, want, 2e-3);
}

#[test]
fn failed_fillet_keeps_body_and_reports_edges() {
    let mut e = engine();
    let mut b = TreeBuilder::new();
    let f = b.root(fillet(20.0, Select::Inside));
    b.child(f, cube(10.0));
    let ev = e.evaluate(b.finish());
    let rep = ev.report(f);
    assert_eq!(rep.status, Status::Error, "{:?}", rep.diagnostics);
    let d = &rep.diagnostics[0];
    assert!(d.message.contains("Verrundung gescheitert"), "{}", d.message);
    assert!(!d.edges.is_empty());
    assert_eq!(d.stmt, ev.tree.node(f).stmt);
    assert_eq!(d.span, ev.tree.node(f).span);
    // shows the body without fillet; evaluation continued
    assert_approx("body", root_volume(&mut e, &ev), 1000.0, 1e-9);
    assert!(!e.tessellate_root(&ev, None).unwrap().is_empty());
}

#[test]
fn modifiers() {
    let mut e = engine();
    let mut b = TreeBuilder::new();
    let g = b.root(group());
    let a = b.child(g, cube(10.0));
    b.flags(a).highlight = true;
    let t1 = b.child(g, translate(v(20.0, 0.0, 0.0)));
    b.flags(t1).background = true;
    let bg = b.child(t1, cube(10.0));
    let t2 = b.child(g, translate(v(40.0, 0.0, 0.0)));
    b.flags(t2).disabled = true;
    let dis = b.child(t2, cube(10.0));
    let ev = e.evaluate(b.finish());
    assert_clean(&ev);
    assert_approx("only #-cube", root_volume(&mut e, &ev), 1000.0, 1e-9);
    // `%` is evaluated and shown on its own; `*` is not evaluated
    assert_eq!(ev.report(bg).status, Status::Ok);
    assert_approx("background", volume(&mut e, &ev, t1), 1000.0, 1e-9);
    assert!(!e.tessellate(&ev, t1, None).unwrap().is_empty());
    assert_eq!(ev.report(dis).status, Status::Skipped);
    assert_eq!(ev.report(t2).status, Status::Skipped);
    // background/disabled subtrees are not part of the root's key
    let mut b2 = TreeBuilder::new();
    let g2 = b2.root(group());
    b2.child(g2, cube(10.0));
    let ev2 = e.evaluate(b2.finish());
    assert_ne!(ev.report(g).key, ev2.report(g2).key, "flags of children are part of the key");
}

#[test]
fn cache_hits_on_second_evaluation() {
    let mut e = engine();
    let (t, ..) = box_minus_cyl(Some(1.0));
    let ev1 = e.evaluate(t.clone());
    let first = e.kernel().total();
    assert!(first > 0);
    e.kernel_mut().reset();
    let ev2 = e.evaluate(t.clone());
    assert_eq!(e.kernel().total(), 0, "{:?}", e.kernel().counts);
    assert_eq!(ev2.stats.cache_hits, ev2.stats.nodes);
    assert!(ev2.reports.iter().filter(|r| r.status != Status::Skipped).all(|r| r.cached));
    assert_eq!(ev1.report(0).key, ev2.report(0).key);

    // Changing the cylinder's position invalidates only the path to the root: the cube and
    // the cylinder itself (relative matrix sits in the parent's key) come from the cache.
    let mut t3 = t.clone();
    let Op::Transform { m, .. } = &mut t3.nodes[2].op else { panic!() };
    *m = glam::DMat4::from_translation(v(8.0, 10.0, -5.0));
    t3.nodes[3].local = *m;
    e.kernel_mut().reset();
    let ev3 = e.evaluate(t3);
    assert!(ev3.report(1).cached && ev3.report(3).cached);
    assert!(!ev3.report(2).cached && !ev3.report(0).cached);
    assert_eq!(e.kernel().count("make_box") + e.kernel().count("make_cylinder"), 0);
    assert!(e.kernel().count("boolean") >= 1);

    // the world matrix is not part of the key
    let mut t4 = t;
    for n in &mut t4.nodes {
        n.world = glam::DMat4::from_translation(v(100.0, 0.0, 0.0)) * n.world;
    }
    e.kernel_mut().reset();
    let _ = e.evaluate(t4);
    assert_eq!(e.kernel().total(), 0);
}

#[test]
fn cache_limit_evicts() {
    let mut e = Engine::with_config(
        cad_engine::CountingKernel::new(kernel_smlib::SmlibKernel::new()),
        cad_engine::EngineConfig { cache_entries: 2, ..Default::default() },
    );
    let (t, ..) = box_minus_cyl(None);
    let _ = e.evaluate(t.clone());
    assert_eq!(e.cache_len(), 2);
    e.kernel_mut().reset();
    let _ = e.evaluate(t);
    assert!(e.kernel().total() > 0);
}

#[test]
fn stepwise_abort_and_resume() {
    let (t, ..) = box_minus_cyl(Some(1.0));
    // reference
    let mut e0 = engine();
    let ev0 = e0.evaluate(t.clone());
    let total_calls = e0.kernel().total();
    let want = root_volume(&mut e0, &ev0);

    // one kernel call per step
    let mut e = engine();
    let mut job = e.start(t.clone());
    let mut steps = 0;
    let ev = loop {
        steps += 1;
        let before = e.kernel().total();
        match job.step(&mut e, Budget::ops(1)) {
            Step::Pending { done, total } => {
                assert!(done < total);
                assert!(e.kernel().total() - before <= 1, "at most one kernel call per step");
            }
            Step::Done(ev) => break ev,
        }
    };
    assert!(steps as u64 >= total_calls, "{steps} steps for {total_calls} calls");
    assert_eq!(e.kernel().total(), total_calls, "replay makes no extra kernel calls");
    assert_approx("stepwise", root_volume(&mut e, &ev), want, 1e-9);
    assert_eq!(ev.report(0).selected_edges, ev0.report(0).selected_edges);

    // abort: a few steps, drop the job; a new evaluation reuses the finished nodes
    let mut e = engine();
    let mut job = e.start(t.clone());
    for _ in 0..4 {
        assert!(!job.step(&mut e, Budget::ops(1)).is_done());
    }
    let done = job.done();
    assert!(done >= 1);
    drop(job);
    let spent = e.kernel().total();
    let ev = e.evaluate(t);
    assert!(ev.stats.cache_hits >= done);
    assert!(e.kernel().total() - spent < total_calls);

    // time budget with an injected clock
    let mut e = engine();
    let ticks = std::rc::Rc::new(std::cell::Cell::new(0.0));
    let t2 = ticks.clone();
    e.set_clock(Box::new(move || {
        t2.set(t2.get() + 1.0);
        t2.get()
    }));
    let (t, ..) = box_minus_cyl(None);
    let mut job = e.start(t);
    let mut n = 0;
    while !job.step(&mut e, Budget::ms(0.5)).is_done() {
        n += 1;
        assert!(n < 100);
    }
    assert!(n >= 1);
}

#[test]
fn lifting_square_minus_circle() {
    let mut e = engine();
    let mut b = TreeBuilder::new();
    let x = b.root(linear_extrude(10.0));
    let d = b.child(x, difference());
    let sq = b.child(d, square(20.0));
    let t = b.child(d, translate(v(10.0, 10.0, 0.0)));
    let c = b.child(t, circle(5.0));
    let ev = e.evaluate(b.finish());
    assert_clean(&ev);
    assert_approx("extruded", root_volume(&mut e, &ev), (400.0 - 25.0 * PI) * 10.0, 1e-3);
    // provenance: faces come from the profiles
    let m = e.tessellate_root(&ev, Some(0.05)).unwrap();
    let mut from_circle = 0;
    for (i, &tag) in m.tri_tags.iter().enumerate() {
        let nodes: Vec<_> = ev.nodes_for_tag(tag).iter().map(|r| r.node).collect();
        assert!(nodes.contains(&sq) || nodes.contains(&c), "{nodes:?}");
        if nodes.contains(&c) {
            from_circle += 1;
            let p = tri_centroid(&m, i);
            assert!(((p.x - 10.0).hypot(p.y - 10.0) - 5.0).abs() < 0.1);
        }
    }
    assert!(from_circle > 0);

    // twist + scale + center
    let mut b = TreeBuilder::new();
    let x = b.root(Op::LinearExtrude { h: 10.0, twist: 90.0, scale: DVec2::splat(0.5), center: true });
    b.child(x, rect(DVec2::splat(-5.0), DVec2::splat(5.0)));
    let ev = e.evaluate(b.finish());
    assert_clean(&ev);
    // frustum-like: A(t) = 100 (1 - t/2)² → V = 10·100·(7/12)
    assert_approx("twisted", root_volume(&mut e, &ev), 1000.0 * 7.0 / 12.0, 1e-4);
    let (lo, hi) = ev.report(x).bounds.unwrap();
    assert!((lo.z + 5.0).abs() < 1e-6 && (hi.z - 5.0).abs() < 1e-6);
}

/// translate([0, 25, 0]) linear_extrude(height = 12, twist = -60, scale = [1, 0.5],
/// center = true) square([10, 6], center = true) from 12_linear_extrude_twist.scad: OpenSCAD
/// rotates each section by -twist·t and then scales it along x/y (bounds y ±3.8309; scaling
/// first gave ±5.08). Mesh volume close to the exact 540.
#[test]
fn linear_extrude_twist_nonuniform_scale_like_openscad() {
    let mut e = engine();
    let mut b = TreeBuilder::new();
    let x = b.root(Op::LinearExtrude { h: 12.0, twist: -60.0, scale: DVec2::new(1.0, 0.5), center: true });
    b.child(x, rect(DVec2::new(-5.0, -3.0), DVec2::new(5.0, 3.0)));
    let ev = e.evaluate(b.finish());
    assert_clean(&ev);
    assert_approx("volume", root_volume(&mut e, &ev), 540.0, 1e-4);
    let (lo, hi) = ev.report(x).bounds.unwrap();
    // exact extents (OpenSCAD with $fn = 128: 5.83013, 3.8309)
    let want = v(34f64.sqrt(), 3.8309045, 6.0);
    assert!((hi - want).abs().max_element() < 1e-3 && (lo + want).abs().max_element() < 1e-3, "{lo} {hi}");
    let m = e.tessellate_root(&ev, Some(0.01)).unwrap();
    assert_approx("mesh volume", mesh_volume(&m), 540.0, 5e-3);
    // OpenSCAD's twist -60 turns counter-clockwise: at z = 0 (t = 1/2, 30°, sy = 0.75) the
    // corner (5, 3) lies at (2.83, 3.82)
    let c = e.kernel_mut().classify(&ev.parts(x)[0].shape, &[v(2.6, 3.5, 0.0), v(-2.6, 3.5, 0.0)]).unwrap();
    assert_eq!(c, vec![cad_kernel::Class::Inside, cad_kernel::Class::Outside]);
}

/// union() { cylinder(h = 30, r = 4, center = true); rotate([90, 0, 0]) …; rotate([0, 90, 0]) …; }
/// from 05_nested_booleans.scad: SMLib dropped the -x arm of the third cylinder (3237 mm³
/// instead of 3800, OpenSCAD 3798). Flat and nested unions, and the cross bore subtracted
/// from a cube.
#[test]
fn union_of_three_orthogonal_cylinders() {
    let r = 4.0f64;
    let cross = |l: f64| 3.0 * PI * r * r * l - 3.0 * 16.0 / 3.0 * r.powi(3) + 8.0 * (2.0 - 2f64.sqrt()) * r.powi(3);
    let cyl = |b: &mut TreeBuilder, parent: NodeIdx, m: glam::DMat4| {
        let t = b.child(parent, Op::Transform { kind: cad_ir::TransformKind::Rotate, m });
        let c = b.child(t, translate(v(0.0, 0.0, -15.0)));
        b.child(c, cylinder(30.0, r));
    };
    let rots = [glam::DMat4::IDENTITY, glam::DMat4::from_rotation_x(PI / 2.0), glam::DMat4::from_rotation_y(PI / 2.0)];
    let mut e = engine();
    for nested in [false, true] {
        let mut b = TreeBuilder::new();
        let u = b.root(union());
        let inner = if nested { b.child(u, union()) } else { u };
        cyl(&mut b, inner, rots[0]);
        cyl(&mut b, inner, rots[1]);
        cyl(&mut b, u, rots[2]);
        let ev = e.evaluate(b.finish());
        assert_clean(&ev);
        assert_approx("union volume", root_volume(&mut e, &ev), cross(30.0), 1e-5);
        let (lo, hi) = ev.report(u).bounds.unwrap();
        assert!((lo + DVec3::splat(15.0)).abs().max_element() < 1e-3, "{lo}");
        assert!((hi - DVec3::splat(15.0)).abs().max_element() < 1e-3, "{hi}");
        let m = e.tessellate_root(&ev, Some(0.01)).unwrap();
        assert_approx("mesh volume", mesh_volume(&m), cross(30.0), 3e-3);
    }
    // difference() { cube(20, center = true); union() { three cylinders } }
    let mut b = TreeBuilder::new();
    let d = b.root(difference());
    b.child(d, boxed(DVec3::splat(-10.0), DVec3::splat(10.0)));
    let u = b.child(d, union());
    for m in rots {
        cyl(&mut b, u, m);
    }
    let ev = e.evaluate(b.finish());
    assert_clean(&ev);
    assert_approx("cube minus cross", root_volume(&mut e, &ev), 8000.0 - cross(20.0), 1e-5);
}

fn tri_centroid(m: &cad_kernel::Mesh, i: usize) -> DVec3 {
    let p = |k: u32| {
        let k = k as usize * 3;
        DVec3::new(m.positions[k] as f64, m.positions[k + 1] as f64, m.positions[k + 2] as f64)
    };
    let t = &m.indices[i * 3..i * 3 + 3];
    (p(t[0]) + p(t[1]) + p(t[2])) / 3.0
}

#[test]
fn rotate_extrude_profile_in_xz() {
    let mut e = engine();
    let mut b = TreeBuilder::new();
    let r = b.root(rotate_extrude(360.0));
    b.child(r, rect(DVec2::new(10.0, 0.0), DVec2::new(12.0, 2.0)));
    let ev = e.evaluate(b.finish());
    assert_clean(&ev);
    assert_approx("ring", root_volume(&mut e, &ev), PI * (144.0 - 100.0) * 2.0, 1e-3);
    let (lo, hi) = ev.report(r).bounds.unwrap();
    assert!((hi.z - 2.0).abs() < 1e-6 && lo.z.abs() < 1e-6 && (hi.x - 12.0).abs() < 1e-3);

    // half revolution of a translated circle (torus half)
    let mut b = TreeBuilder::new();
    let r = b.root(rotate_extrude(180.0));
    let t = b.child(r, translate(v(10.0, 0.0, 0.0)));
    b.child(t, circle(2.0));
    let ev = e.evaluate(b.finish());
    assert_clean(&ev);
    assert_approx("half torus", root_volume(&mut e, &ev), PI * 4.0 * PI * 10.0, 1e-3);
}

#[test]
fn loft_and_sweep() {
    let mut e = engine();
    // loft of two equal squares = prism
    let mut b = TreeBuilder::new();
    let l = b.root(loft(true));
    b.child(l, rect(DVec2::splat(-5.0), DVec2::splat(5.0)));
    let t = b.child(l, translate(v(0.0, 0.0, 10.0)));
    b.child(t, rect(DVec2::splat(-5.0), DVec2::splat(5.0)));
    let ev = e.evaluate(b.finish());
    assert_clean(&ev);
    assert_approx("prism loft", root_volume(&mut e, &ev), 1000.0, 1e-3);

    // spec example: circle(10) → square(15, center) at z = 30
    let mut b = TreeBuilder::new();
    let l = b.root(loft(false));
    b.child(l, circle(10.0));
    let t = b.child(l, translate(v(0.0, 0.0, 30.0)));
    b.child(t, rect(DVec2::splat(-7.5), DVec2::splat(7.5)));
    let ev = e.evaluate(b.finish());
    assert_clean(&ev);
    let vol = root_volume(&mut e, &ev);
    assert!(vol > 225.0 * 30.0 && vol < 100.0 * PI * 30.0, "{vol}");

    // loft child with two loops → error at the node
    let mut b = TreeBuilder::new();
    let l = b.root(loft(false));
    let u = b.child(l, union());
    b.child(u, circle(3.0));
    let tt = b.child(u, translate(v(10.0, 0.0, 0.0)));
    b.child(tt, circle(3.0));
    let t = b.child(l, translate(v(0.0, 0.0, 10.0)));
    b.child(t, circle(3.0));
    let ev = e.evaluate(b.finish());
    assert_eq!(ev.report(l).status, Status::Error);
    assert!(errors(&ev)[0].contains("genau eine"));

    // sweep a circle along a straight path
    let mut b = TreeBuilder::new();
    let s = b.root(sweep(vec![v(0.0, 0.0, 0.0), v(0.0, 0.0, 20.0)], PathInterp::Linear));
    b.child(s, circle(2.0));
    let ev = e.evaluate(b.finish());
    assert_clean(&ev);
    assert_approx("sweep", root_volume(&mut e, &ev), PI * 4.0 * 20.0, 1e-3);

    // sweep along a spline path: just a valid solid
    let mut b = TreeBuilder::new();
    let s = b.root(sweep(vec![v(0.0, 0.0, 0.0), v(20.0, 0.0, 10.0), v(40.0, 10.0, 10.0)], PathInterp::Spline));
    b.child(s, circle(2.0));
    let ev = e.evaluate(b.finish());
    assert_clean(&ev);
    assert!(root_volume(&mut e, &ev) > PI * 4.0 * 40.0);
}

#[test]
fn provenance_bore_triangles_point_to_cylinder() {
    let mut e = engine();
    let (t, d, cube_n, cyl) = box_minus_cyl(None);
    let ev = e.evaluate(t);
    let m = e.tessellate(&ev, d, Some(0.05)).unwrap();
    let mut bore = 0;
    for i in 0..m.triangle_count() {
        let p = tri_centroid(&m, i);
        let nodes: Vec<NodeIdx> = ev.nodes_for_tag(m.tri_tags[i]).iter().map(|r| r.node).collect();
        if ((p.x - 10.0).hypot(p.y - 10.0) - 5.0).abs() < 0.05 && p.z > 0.01 && p.z < 19.99 {
            bore += 1;
            assert_eq!(nodes, vec![cyl]);
            let r = ev.nodes_for_tag(m.tri_tags[i])[0];
            assert_eq!(r.stmt, ev.tree.node(cyl).stmt);
            assert_eq!(r.inst_key, ev.tree.node(cyl).inst_key);
        } else {
            assert_eq!(nodes, vec![cube_n]);
        }
    }
    assert!(bore > 10);
    // subtracted body tessellated on its own, in its own system (the UI places it by world)
    let cm = e.tessellate(&ev, cyl, Some(0.1)).unwrap();
    let (lo, hi) = cm.bounds().unwrap();
    assert!(lo.z.abs() < 1e-4 && (hi.z - 30.0).abs() < 1e-4 && (hi.x - 5.0).abs() < 1e-3);
    assert_eq!(ev.report(cyl).display_world, ev.tree.node(cyl).world);
    // mesh cache: same (key, tol) → same Rc
    let m2 = e.tessellate(&ev, d, Some(0.05)).unwrap();
    assert!(std::rc::Rc::ptr_eq(&m, &m2));
}

#[test]
fn equal_content_nodes_share_a_tag() {
    let mut e = engine();
    // two equal cylinders at different places: one tag, two rows
    let mut b = TreeBuilder::new();
    let d = b.root(difference());
    b.child(d, cube(30.0));
    let t1 = b.child(d, translate(v(8.0, 8.0, -5.0)));
    let c1 = b.child(t1, cylinder(40.0, 3.0));
    let t2 = b.child(d, translate(v(22.0, 22.0, -5.0)));
    let c2 = b.child(t2, cylinder(40.0, 3.0));
    let ev = e.evaluate(b.finish());
    assert_eq!(ev.report(c1).tag, ev.report(c2).tag);
    let rows: Vec<_> = ev.nodes_for_tag(ev.report(c1).tag).iter().map(|r| r.node).collect();
    assert_eq!(rows, vec![c1, c2]);
    assert_approx("two bores", root_volume(&mut e, &ev), 27000.0 - 2.0 * PI * 9.0 * 30.0, 1e-3);
}

#[test]
fn shell_opens_faces_in_zone() {
    let mut e = engine();
    let mut b = TreeBuilder::new();
    let s = b.root(shell(1.0, Select::Inside));
    b.child(s, cube(20.0));
    let z = b.child(s, zone_plane(v(0.0, 0.0, 20.0), DVec3::Z, 1.0));
    b.flags(z).zone = true;
    let ev = e.evaluate(b.finish());
    assert_clean(&ev);
    assert_eq!(ev.report(s).opened_faces, 1);
    assert!(ev.report(s).hints.contains(&"1 Fläche geöffnet".to_string()));
    assert_approx("open box", root_volume(&mut e, &ev), 8000.0 - 18.0 * 18.0 * 19.0, 1e-3);
}

#[test]
fn surfaces_and_thicken() {
    let mut e = engine();
    let rows: Vec<Vec<DVec3>> =
        (0..4).map(|i| (0..4).map(|j| v(i as f64 * 10.0, j as f64 * 10.0, ((i + j) % 2) as f64)).collect()).collect();
    // open surface in a boolean → error at the boolean, inputs shown
    let mut b = TreeBuilder::new();
    let u = b.root(union());
    b.child(u, cube(5.0));
    let s = b.child(u, Op::Surface { rows: rows.clone() });
    let ev = e.evaluate(b.finish());
    assert_eq!(ev.report(s).status, Status::Ok);
    assert_eq!(ev.report(u).status, Status::Error);
    assert!(errors(&ev)[0].contains("Offene Fläche"));
    assert_eq!(ev.report(u).parts, 2);

    let mut b = TreeBuilder::new();
    let t = b.root(Op::Thicken { t: 1.0 });
    b.child(t, Op::Surface { rows });
    let ev = e.evaluate(b.finish());
    assert_clean(&ev);
    let vol = root_volume(&mut e, &ev);
    assert!(vol > 800.0 && vol < 1100.0, "{vol}");
}

#[test]
fn resize_scales_to_bounds() {
    let mut e = engine();
    let mut b = TreeBuilder::new();
    let r = b.root(Op::Resize { size: v(20.0, 0.0, 0.0), auto: [false, true, false] });
    b.child(r, cube(10.0));
    let ev = e.evaluate(b.finish());
    assert_clean(&ev);
    let (lo, hi) = ev.report(r).bounds.unwrap();
    assert!(lo.abs_diff_eq(DVec3::ZERO, 1e-6) && hi.abs_diff_eq(v(20.0, 20.0, 10.0), 1e-6), "{hi}");
}

#[test]
fn two_d_display_without_extrusion() {
    let mut e = engine();
    let mut b = TreeBuilder::new();
    let d = b.root(difference());
    let sq = b.child(d, square(20.0));
    let t = b.child(d, translate(v(10.0, 10.0, 0.0)));
    let c = b.child(t, circle(5.0));
    let ev = e.evaluate(b.finish());
    assert_clean(&ev);
    assert_eq!(e.kernel().total(), 0, "2D needs no kernel");
    assert_eq!(ev.report(d).dim, Some(cad_ir::Dim::Two));
    let m = e.tessellate(&ev, d, Some(0.01)).unwrap();
    let area: f64 = (0..m.triangle_count())
        .map(|i| {
            let t = &m.indices[i * 3..i * 3 + 3];
            let p = |k: u32| DVec2::new(m.positions[k as usize * 3] as f64, m.positions[k as usize * 3 + 1] as f64);
            let (a, b2, c2) = (p(t[0]), p(t[1]), p(t[2]));
            (b2 - a).perp_dot(c2 - a) / 2.0
        })
        .sum();
    assert_approx("2D area", area, 400.0 - 25.0 * PI, 2e-3);
    assert!(!m.edge_segments.is_empty());
    let sq_tag = ev.report(sq).tag;
    assert!(m.tri_tags.iter().all(|&t| t == sq_tag));
    // the circle alone, in its own system
    let cm = e.tessellate(&ev, c, Some(0.01)).unwrap();
    let (lo, hi) = cm.bounds().unwrap();
    assert!((lo.x + 5.0).abs() < 0.02 && (hi.y - 5.0).abs() < 0.02, "{lo} {hi}");
    assert!(ev.report(c).bounds.is_some());
}

#[test]
fn stl_and_3mf_export() {
    let mut e = engine();
    let (t, ..) = box_minus_cyl(None);
    let ev = e.evaluate(t);
    let stl = e.export_stl(&ev, None).unwrap();
    let n = u32::from_le_bytes(stl[80..84].try_into().unwrap()) as usize;
    assert_eq!(stl.len(), 84 + 50 * n);
    let mesh = e.tessellate_root(&ev, Some(0.01)).unwrap();
    assert_eq!(n, mesh.triangle_count());
    let tris: Vec<[[f32; 3]; 3]> = (0..n)
        .map(|i| {
            let o = 84 + 50 * i + 12;
            let f = |k: usize| f32::from_le_bytes(stl[o + 4 * k..o + 4 * k + 4].try_into().unwrap());
            [[f(0), f(1), f(2)], [f(3), f(4), f(5)], [f(6), f(7), f(8)]]
        })
        .collect();
    assert!(tris_closed(&tris), "STL is closed");
    assert_approx("mesh volume", mesh_volume(&mesh), BORE_V, 2e-3);

    let bytes = e.export_3mf(&ev, None).unwrap();
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("3MF is a zip");
    let mut names: Vec<String> = z.file_names().map(|s| s.to_string()).collect();
    names.sort();
    assert_eq!(names, ["3D/3dmodel.model", "[Content_Types].xml", "_rels/.rels"]);
    let mut model = String::new();
    std::io::Read::read_to_string(&mut z.by_name("3D/3dmodel.model").unwrap(), &mut model).unwrap();
    let ntri = model.matches("<triangle ").count();
    let nvert = model.matches("<vertex ").count();
    assert!(ntri > 0 && ntri <= n);
    assert!(nvert > 0 && nvert < 3 * ntri, "vertices are shared");
}

#[test]
fn step_export() {
    let mut e = engine();
    let (t, ..) = box_minus_cyl(None);
    let ev = e.evaluate(t);
    let opts = cad_engine::StepOptions { name: "bohrung".into(), ..Default::default() };
    let (bytes, st) = e.export_step(&ev, &opts).unwrap();
    assert_eq!((st.solids, st.faces), (1, 7));
    assert_eq!(st.surfaces.get("cylinder"), Some(&1));
    let m = cad_step::p21::parse(std::str::from_utf8(&bytes).unwrap()).unwrap();
    assert_eq!(m.count("CYLINDRICAL_SURFACE"), 1);
    assert_eq!(m.count("PLANE"), 6);
    assert_eq!(m.count("ADVANCED_FACE"), 7);
    // the bore's radius and axis survive exactly
    let cyl = m.of_type("CYLINDRICAL_SURFACE").next().unwrap();
    assert!((cyl.args()[2].as_num().unwrap() - 5.0).abs() < 1e-12);
    // 2D results cannot be exported
    let mut b = TreeBuilder::new();
    b.root(circle(3.0));
    let ev = e.evaluate(b.finish());
    assert!(e.export_step(&ev, &opts).is_err());
}

#[test]
fn root_modifier_ignores_transforms_above() {
    let mut e = engine();
    let mut b = TreeBuilder::new();
    let g = b.root(group());
    let t = b.child(g, translate(v(50.0, 0.0, 0.0)));
    let c = b.child(t, cube(10.0));
    let mut tree = b.finish();
    tree.root = Some(c); // `!cube(10)` below a translate
    let ev = e.evaluate(tree);
    assert!(ev.root_is_subtree());
    assert_eq!(ev.report(c).display_world, glam::DMat4::IDENTITY);
    assert_eq!(ev.report(g).status, Status::Skipped);
    let (lo, _) = ev.report(c).bounds.unwrap();
    assert!(lo.abs_diff_eq(DVec3::ZERO, 1e-9));
}
