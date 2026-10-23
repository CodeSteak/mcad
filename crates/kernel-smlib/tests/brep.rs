//! `Kernel::brep` (facade `sc_brep`): topology, geometry and orientation of the neutral B-rep.

mod common;

use std::collections::BTreeMap;

use cad_kernel::brep::{Body, Brep, Curve, Loop, Surface, sample_edge};
use cad_kernel::*;
use common::*;
use glam::DVec3;
use kernel_smlib::{Shape, SmlibKernel};

fn summary(b: &Brep) -> (BTreeMap<&'static str, usize>, BTreeMap<&'static str, usize>) {
    let mut s = BTreeMap::new();
    for f in b.faces() {
        *s.entry(f.surface.kind()).or_insert(0) += 1;
    }
    let mut c = BTreeMap::new();
    for e in &b.edges {
        *c.entry(e.curve.kind()).or_insert(0) += 1;
    }
    (s, c)
}

/// Face normal (STEP convention) at a point of the surface near `p`.
fn face_normal(f: &cad_kernel::brep::Face, p: DVec3) -> Option<DVec3> {
    let n = f.surface.normal_at(p, None)?;
    Some(if f.same_sense { n } else { -n })
}

/// Checks the conventions of `cad_kernel::brep` on `b`:
/// * edges lie on the surfaces of their faces (analytic surfaces);
/// * planar faces bounded by straight edges: the outer loop turns counter-clockwise about the
///   face normal, which points out of the material (a point just outside is outside).
fn check_conventions(k: &mut SmlibKernel, s: &Shape, b: &Brep) {
    b.validate().unwrap();
    let diag = k.bounds(s).unwrap().map(|(a, c)| (c - a).length()).unwrap_or(1.0);
    for f in b.faces() {
        for l in &f.loops {
            let Loop::Edges(es) = l else { continue };
            for oe in es {
                let e = &b.edges[oe.edge as usize];
                let pts = sample_edge(&e.curve, b.vertices[e.start as usize], b.vertices[e.end as usize], 8);
                if let Some(d) = pts.iter().filter_map(|p| f.surface.distance(*p)).reduce(f64::max) {
                    assert!(d < 1e-6 * diag, "face {} ({}): edge {} off the surface by {d}", f.id, f.surface.kind(), oe.edge);
                }
            }
        }
        // outward normal: probe just outside / inside at a sample of the face
        let pts = k.faces(s, 1).unwrap();
        let fp = &pts[f.id as usize].points;
        let p = fp[0];
        if let Some(n) = face_normal(f, p) {
            let eps = 1e-3 * diag;
            let c = k.classify(s, &[p + n * eps, p - n * eps]).unwrap();
            assert_eq!(c, vec![Class::Outside, Class::Inside], "face {} ({}) normal points inwards", f.id, f.surface.kind());
        }
        if let (Surface::Plane { .. }, Some(Loop::Edges(es))) = (&f.surface, f.loops.first())
            && f.loops.len() == 1
            && es.iter().all(|oe| matches!(b.edges[oe.edge as usize].curve, Curve::Line { .. }))
        {
            let poly: Vec<DVec3> = es
                .iter()
                .map(|oe| {
                    let e = &b.edges[oe.edge as usize];
                    b.vertices[if oe.forward { e.start } else { e.end } as usize]
                })
                .collect();
            let mut newell = DVec3::ZERO;
            for i in 0..poly.len() {
                newell += poly[i].cross(poly[(i + 1) % poly.len()]);
            }
            let n = face_normal(f, poly[0]).unwrap();
            assert!(newell.dot(n) > 0.0, "face {}: loop turns clockwise about its normal", f.id);
        }
    }
}

#[test]
fn box_is_six_planes_and_twelve_lines() {
    let mut k = k();
    let s = k.make_box(DVec3::ZERO, DVec3::new(10.0, 20.0, 30.0), 7).unwrap();
    let (b, st) = k.brep_with_stats(&s).unwrap();
    let (sf, cv) = summary(&b);
    assert_eq!(sf.get("plane"), Some(&6), "{sf:?}");
    assert_eq!(cv.get("line"), Some(&12), "{cv:?}");
    assert_eq!(b.vertices.len(), 8);
    assert_eq!(b.bodies.len(), 1);
    assert!(matches!(&b.bodies[0], Body::Solid { voids, .. } if voids.is_empty()));
    assert!(b.faces().all(|f| f.tag == 7));
    assert_eq!(st.analytic_rejected, 0);
    assert!(st.max_vertex_gap < 1e-9, "{st:?}");
    check_conventions(&mut k, &s, &b);
}

#[test]
fn primitives_have_analytic_surfaces() {
    let mut k = k();
    let cases: Vec<(&str, Shape, &str)> = vec![
        ("cylinder", k.make_cylinder(10.0, 3.0, 3.0, 1).unwrap(), "cylinder"),
        ("cone", k.make_cylinder(10.0, 3.0, 1.0, 1).unwrap(), "cone"),
        ("cone tip", k.make_cylinder(10.0, 0.0, 2.0, 1).unwrap(), "cone"),
        ("sphere", k.make_sphere(4.0, 1).unwrap(), "sphere"),
        ("torus", k.make_torus(10.0, 2.0, 1).unwrap(), "torus"),
    ];
    for (name, s, want) in cases {
        let (b, st) = k.brep_with_stats(&s).unwrap();
        let (sf, cv) = summary(&b);
        println!("{name}: {sf:?} {cv:?} {st:?}");
        assert!(sf.contains_key(want), "{name}: {sf:?}");
        assert_eq!(st.analytic_rejected, 0, "{name}");
        check_conventions(&mut k, &s, &b);
    }
}

#[test]
fn boolean_fillet_and_transform() {
    let mut k = k();
    let bx = k.make_box(DVec3::ZERO, DVec3::splat(10.0), 1).unwrap();
    let c = k.make_cylinder(12.0, 2.0, 2.0, 2).unwrap();
    let c = k.transform(&c, &translate(5.0, 5.0, -1.0)).unwrap();
    let d = k.boolean(BoolOp::Difference, &[bx.clone(), c], 3).unwrap();
    let (b, st) = k.brep_with_stats(&d).unwrap();
    println!("box - cylinder: {:?} {st:?}", summary(&b));
    check_conventions(&mut k, &d, &b);
    assert!(b.faces().any(|f| f.tag == 2 && matches!(f.surface, Surface::Cylinder { .. })));

    let all: Vec<u32> = (0..12).collect();
    let f = k.fillet(&bx, &all, 1.0, 4).unwrap();
    let (b, st) = k.brep_with_stats(&f).unwrap();
    println!("filleted box: {:?} {st:?}", summary(&b));
    check_conventions(&mut k, &f, &b);

    let rot = glam::DMat4::from_rotation_y(0.3) * translate(3.0, -2.0, 1.0);
    let t = k.transform(&d, &rot).unwrap();
    let (b, _) = k.brep_with_stats(&t).unwrap();
    check_conventions(&mut k, &t, &b);
}

#[test]
fn hollow_sphere_has_a_void() {
    let mut k = k();
    let a = k.make_sphere(10.0, 1).unwrap();
    let c = k.make_sphere(5.0, 2).unwrap();
    let d = k.boolean(BoolOp::Difference, &[a, c], 3).unwrap();
    let (b, _) = k.brep_with_stats(&d).unwrap();
    match &b.bodies[..] {
        [Body::Solid { voids, .. }] => assert_eq!(voids.len(), 1, "{:?}", summary(&b)),
        other => panic!("{} bodies", other.len()),
    }
    check_conventions(&mut k, &d, &b);
}

#[test]
fn touching_union_gives_two_solids() {
    let mut k = k();
    let a = k.make_box(DVec3::ZERO, DVec3::splat(10.0), 1).unwrap();
    let c = k.make_box(DVec3::new(10.0, 10.0, 0.0), DVec3::new(20.0, 20.0, 10.0), 2).unwrap();
    let u = k.boolean(BoolOp::Union, &[a, c], 3).unwrap();
    let (b, _) = k.brep_with_stats(&u).unwrap();
    assert_eq!(b.bodies.len(), 2);
    check_conventions(&mut k, &u, &b);
}
