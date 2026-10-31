//! Face tags (provenance) and edge information over solidcore (plan 1.9, 1.10): tags as the
//! SMLib facade carries them (`smlib/facade/sc_facade.cpp`, "Provenance"), `smooth` edges as
//! SMLib's `GetBlendEdgeuse`.

#![cfg(feature = "solidcore")]

use cad_kernel::{BoolOp, Kernel, Loop, PlacedLoop, Segment, Tag};
use glam::{DMat4, DVec2, DVec3};
use kernel_solidcore::{Shape, SolidcoreKernel};
use std::f64::consts::{FRAC_PI_2, PI};

/// The face tags of `s`, sorted.
fn tags(k: &mut SolidcoreKernel, s: &Shape) -> Vec<Tag> {
    let mut t: Vec<Tag> = k.faces(s, 0).unwrap().iter().map(|f| f.tag).collect();
    t.sort_unstable();
    t
}

#[test]
fn tags_survive_transform_and_booleans() {
    let mut k = SolidcoreKernel::new();
    let a = k.make_box(DVec3::ZERO, DVec3::splat(10.0), 1).unwrap();
    let b = k.make_box(DVec3::splat(5.0), DVec3::splat(15.0), 2).unwrap();
    assert_eq!(tags(&mut k, &a), vec![1; 6]);
    let moved = k.transform(&b, &DMat4::from_translation(DVec3::new(0.5, 0.0, 0.0))).unwrap();
    assert_eq!(tags(&mut k, &moved), vec![2; 6]);
    let u = k.boolean(BoolOp::Union, &[a.clone(), b.clone()], 3).unwrap();
    assert_eq!(tags(&mut k, &u), [vec![1; 6], vec![2; 6]].concat());
    let d = k.boolean(BoolOp::Difference, &[a.clone(), b.clone()], 3).unwrap();
    assert_eq!(tags(&mut k, &d), [vec![1; 6], vec![2; 3]].concat());
    let i = k.boolean(BoolOp::Intersection, &[a.clone(), b.clone()], 3).unwrap();
    assert_eq!(tags(&mut k, &i), [vec![1; 3], vec![2; 3]].concat());
    // the edges where the boxes cross separate a face of each
    let new: Vec<_> =
        k.edges(&u, 0).unwrap().into_iter().filter(|e| e.face_tags == [1, 2] || e.face_tags == [2, 1]).collect();
    assert_eq!(new.len(), 6, "{new:?}");
}

#[test]
fn smooth_edges_are_seams_poles_and_tangent_edges() {
    let mut k = SolidcoreKernel::new();
    let c = k.make_cylinder(10.0, 5.0, 5.0, 1).unwrap();
    let e = k.edges(&c, 1).unwrap();
    // two rims to blend, the seam not
    let sharp = e.iter().filter(|e| !e.smooth).count();
    println!("cylinder edges: {:?}", e.iter().map(|e| (e.smooth, e.face_tags.clone())).collect::<Vec<_>>());
    assert_eq!(sharp, 2, "{e:?}");
    assert!(e.iter().all(|e| e.face_tags.len() == 2), "{e:?}");
    let s = k.make_sphere(5.0, 1).unwrap();
    let e = k.edges(&s, 1).unwrap();
    println!("sphere edges: {:?}", e.iter().map(|e| (e.smooth, e.face_tags.clone())).collect::<Vec<_>>());
    assert!(e.iter().all(|e| e.smooth), "{e:?}");
    let b = k.make_box(DVec3::ZERO, DVec3::splat(10.0), 1).unwrap();
    assert!(k.edges(&b, 0).unwrap().iter().all(|e| !e.smooth));
}

#[test]
fn tangent_edges_of_a_rounded_profile_are_smooth() {
    let mut k = SolidcoreKernel::new();
    let line = |a: [f64; 2], b: [f64; 2]| Segment::Line { a: DVec2::from(a), b: DVec2::from(b) };
    let arc = |c: [f64; 2], a0: f64, a1: f64| Segment::Arc { center: DVec2::from(c), r: 2.0, a0, a1 };
    let segs = vec![
        line([2.0, 0.0], [18.0, 0.0]),
        arc([18.0, 2.0], -FRAC_PI_2, 0.0),
        line([20.0, 2.0], [20.0, 8.0]),
        arc([18.0, 8.0], 0.0, FRAC_PI_2),
        line([18.0, 10.0], [2.0, 10.0]),
        arc([2.0, 8.0], FRAC_PI_2, PI),
        line([0.0, 8.0], [0.0, 2.0]),
        arc([2.0, 2.0], PI, 1.5 * PI),
    ];
    let p = PlacedLoop { lp: Loop::Composite(segs), frame: DMat4::IDENTITY };
    let s = k.extrude(&p, 5.0, 0.0, DVec2::ONE, 7).unwrap();
    let e = k.edges(&s, 1).unwrap();
    assert_eq!(e.len(), 24);
    // the 8 vertical joints of lines and arcs are tangent; the 16 rims are sharp
    let vertical = |e: &cad_kernel::EdgeInfo| (e.points[0].z - e.points.last().unwrap().z).abs() > 1.0;
    assert!(e.iter().all(|e| e.smooth == vertical(e)), "{e:?}");
    assert!(e.iter().all(|e| e.face_tags == [7, 7]), "{e:?}");
    assert_eq!(tags(&mut k, &s), vec![7; 10]);
}
