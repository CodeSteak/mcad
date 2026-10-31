//! `Kernel::brep` over solidcore (plan 1.11): the neutral B-rep with the conventions of
//! kernel-smlib (surface and curve kinds, orientation, loops, seams, tags).

#![cfg(feature = "solidcore")]

use cad_kernel::brep::{Body, Brep, Curve, Loop, Surface};
use cad_kernel::{BoolOp, Kernel, Loop as Profile, PlacedLoop};
use glam::{DMat4, DVec2, DVec3};
use kernel_solidcore::{Shape, SolidcoreKernel};

/// Surface kinds, curve kinds (sorted) and the problems of the conventions.
fn kinds(k: &mut SolidcoreKernel, s: &Shape) -> (Brep, Vec<&'static str>, Vec<&'static str>) {
    let (b, st) = k.brep_with_stats(s).unwrap();
    assert_eq!((st.analytic_rejected, st.orientation_fallback, st.edges_reversed), (0, 0, 0), "{st:?}");
    let problems = b.convention_problems(1e-6);
    assert!(problems.is_empty(), "{problems:?}");
    let mut sk: Vec<_> = b.faces().map(|f| f.surface.kind()).collect();
    let mut ck: Vec<_> = b.edges.iter().map(|e| e.curve.kind()).collect();
    sk.sort_unstable();
    ck.sort_unstable();
    (b, sk, ck)
}

/// Every face normal points out of the material: near the middle of the first edge of the
/// first loop, a little into the face (to the left of the edge, the loops run counter-clockwise
/// about the normal), a point a little along the normal is outside the solid, one against it
/// inside.
fn normals_point_out(k: &mut SolidcoreKernel, s: &Shape, b: &Brep) {
    for f in b.faces() {
        let Some(Loop::Edges(es)) = f.loops.first() else { continue };
        let pts = b.edge_points(es[0], 4);
        let p = pts[2];
        let Some(n) = f.surface.normal_at(p, None) else { continue };
        let n = if f.same_sense { n } else { -n };
        let q = p + n.cross((pts[3] - pts[1]).normalize()) * 1e-2;
        let c = k.classify(s, &[q + n * 1e-3, q - n * 1e-3]).unwrap();
        let what = format!("face {} ({}) at {q:.3}", f.id, f.surface.kind());
        assert_eq!(c, [cad_kernel::Class::Outside, cad_kernel::Class::Inside], "{what}");
    }
}

#[test]
fn primitives() {
    let mut k = SolidcoreKernel::new();
    let bx = k.make_box(DVec3::ZERO, DVec3::new(10.0, 20.0, 30.0), 7).unwrap();
    let (b, sk, ck) = kinds(&mut k, &bx);
    assert_eq!((b.vertices.len(), sk, ck), (8, vec!["plane"; 6], vec!["line"; 12]));
    assert!(b.faces().all(|f| f.tag == 7));
    assert!(matches!(&b.bodies[..], [Body::Solid { voids, .. }] if voids.is_empty()));
    normals_point_out(&mut k, &bx, &b);

    let c = k.make_cylinder(10.0, 5.0, 5.0, 1).unwrap();
    let (b, sk, ck) = kinds(&mut k, &c);
    assert_eq!(sk, vec!["cylinder", "plane", "plane"]);
    assert_eq!(ck, vec!["circle", "circle", "line"]);
    normals_point_out(&mut k, &c, &b);
    // the seam is used twice by the side face, once in each direction
    let side = b.faces().find(|f| f.surface.kind() == "cylinder").unwrap();
    let Loop::Edges(es) = &side.loops[0] else { panic!() };
    let seam = es.iter().filter(|e| matches!(b.edges[e.edge as usize].curve, Curve::Line { .. })).count();
    assert_eq!(seam, 2);

    let cone = k.make_cylinder(10.0, 5.0, 2.0, 1).unwrap();
    let (b, sk, _) = kinds(&mut k, &cone);
    assert_eq!(sk, vec!["cone", "plane", "plane"]);
    normals_point_out(&mut k, &cone, &b);
    let apex = k.make_cylinder(10.0, 0.0, 4.0, 1).unwrap();
    let (b, sk, _) = kinds(&mut k, &apex);
    assert_eq!(sk, vec!["cone", "plane"]);
    normals_point_out(&mut k, &apex, &b);

    let sp = k.make_sphere(5.0, 1).unwrap();
    let (b, sk, _) = kinds(&mut k, &sp);
    assert_eq!(sk, vec!["sphere"]);
    normals_point_out(&mut k, &sp, &b);

    let t = k.make_torus(10.0, 2.0, 1).unwrap();
    let (b, sk, _) = kinds(&mut k, &t);
    assert_eq!(sk, vec!["torus"]);
    normals_point_out(&mut k, &t, &b);
}

#[test]
fn transformed_booleans_and_fillets() {
    let mut k = SolidcoreKernel::new();
    let a = k.make_box(DVec3::ZERO, DVec3::splat(10.0), 1).unwrap();
    let c = k.make_cylinder(20.0, 3.0, 3.0, 2).unwrap();
    let c = k.transform(&c, &DMat4::from_translation(DVec3::new(5.0, 5.0, -5.0))).unwrap();
    let d = k.boolean(BoolOp::Difference, &[a.clone(), c], 3).unwrap();
    let (b, sk, _) = kinds(&mut k, &d);
    assert_eq!(sk, vec!["cylinder", "plane", "plane", "plane", "plane", "plane", "plane"]);
    normals_point_out(&mut k, &d, &b);
    // the hole: a cylinder face whose normal points to the axis
    let hole = b.faces().find(|f| f.surface.kind() == "cylinder").unwrap();
    assert!(!hole.same_sense);
    assert_eq!(hole.tag, 2);
    // top and bottom have a hole: two loops each
    assert_eq!(b.faces().filter(|f| f.loops.len() == 2).count(), 2);

    let inner = k.make_box(DVec3::splat(3.0), DVec3::splat(7.0), 4).unwrap();
    let v = k.boolean(BoolOp::Difference, &[a.clone(), inner], 3).unwrap();
    let (b, _, _) = kinds(&mut k, &v);
    match &b.bodies[..] {
        [Body::Solid { outer, voids }] => assert_eq!((outer.faces.len(), voids.len(), voids[0].faces.len()), (6, 1, 6)),
        other => panic!("{other:?}"),
    }
    normals_point_out(&mut k, &v, &b);

    let edges: Vec<u32> = (0..12).collect();
    let f = k.fillet(&a, &edges, 1.0, 5).unwrap();
    let (b, sk, _) = kinds(&mut k, &f);
    assert_eq!(sk, [vec!["cylinder"; 12], vec!["plane"; 6], vec!["sphere"; 8]].concat());
    assert_eq!(b.faces().count(), 26);
    assert!(b.faces().filter(|f| f.surface.kind() == "plane").all(|f| f.tag == 1));
}

#[test]
fn extrusions_and_revolutions() {
    let mut k = SolidcoreKernel::new();
    let lp = PlacedLoop { lp: Profile::Circle { center: DVec2::new(10.0, 0.0), r: 2.0 }, frame: DMat4::IDENTITY };
    let e = k.extrude(&lp, 5.0, 0.0, DVec2::ONE, 1).unwrap();
    let (b, sk, _) = kinds(&mut k, &e);
    assert_eq!(sk, ["cylinder", "plane", "plane"]);
    normals_point_out(&mut k, &e, &b);
    let rf = DMat4::from_rotation_x(std::f64::consts::FRAC_PI_2);
    let r = k.revolve(&PlacedLoop { frame: rf, ..lp.clone() }, std::f64::consts::TAU, 1).unwrap();
    let (b, sk, _) = kinds(&mut k, &r);
    assert_eq!(sk, ["torus"]);
    normals_point_out(&mut k, &r, &b);
    let poly = Profile::Polygon(vec![DVec2::new(2.0, 0.0), DVec2::new(6.0, 0.0), DVec2::new(4.0, 5.0)]);
    let r = k.revolve(&PlacedLoop { lp: poly, frame: rf }, 1.5, 1).unwrap();
    let (b, sk, _) = kinds(&mut k, &r);
    assert_eq!(sk, ["cone", "cone", "plane", "plane", "plane"]);
    normals_point_out(&mut k, &r, &b);
    let sp =
        Profile::Spline(vec![DVec2::new(0.0, 0.0), DVec2::new(8.0, 1.0), DVec2::new(6.0, 6.0), DVec2::new(1.0, 5.0)]);
    let e = k.extrude(&PlacedLoop { lp: sp, frame: DMat4::IDENTITY }, 3.0, 0.0, DVec2::ONE, 1).unwrap();
    let (b, sk, _) = kinds(&mut k, &e);
    assert!(sk.iter().all(|s| matches!(*s, "plane" | "extrusion" | "bspline")), "{sk:?}");
    normals_point_out(&mut k, &e, &b);
    let _ = Surface::kind;
}
