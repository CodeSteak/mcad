//! Measurements on SMLib shapes against analytic values: radii and areas of cylinders,
//! cones, spheres and tori, fillet and chamfer geometry, distances, angles, sections
//! through bores and wall thickness.

use std::f64::consts::PI;
use std::rc::Rc;

use cad_kernel::{BoolOp, Kernel};
use cad_measure::section::{SectionPlane, cut_contours};
use cad_measure::{Body, Geo, Radius, Section, angle, distance, face_area, radius};
use glam::{DMat4, DVec3};
use kernel_smlib::{Shape, SmlibKernel};

const TOL: f64 = 0.01;

fn k() -> SmlibKernel {
    SmlibKernel::new()
}

fn body(k: &mut SmlibKernel, s: &Shape) -> Body {
    let brep = k.brep(s).unwrap();
    let mesh = k.tessellate(s, TOL).unwrap();
    Body::new(Rc::new(brep), &mesh, &DMat4::IDENTITY, TOL, true)
}

/// Face closest to `p` (by its triangles).
fn face_at(b: &Body, p: DVec3) -> usize {
    (0..b.faces.len()).min_by(|&x, &y| b.mesh_closest(x, p).0.partial_cmp(&b.mesh_closest(y, p).0).unwrap()).unwrap()
}

/// Edge closest to `p`.
fn edge_at(b: &Body, p: DVec3) -> usize {
    (0..b.edges.len())
        .min_by(|&x, &y| {
            let dx = b.edges[x].closest(p).1.distance(p);
            let dy = b.edges[y].closest(p).1.distance(p);
            dx.partial_cmp(&dy).unwrap()
        })
        .unwrap()
}

fn face(b: &Body, p: DVec3) -> Geo<'_> {
    Geo::Face { body: b, face: face_at(b, p) }
}

fn close(a: f64, b: f64, eps: f64) -> bool {
    (a - b).abs() <= eps
}

fn translate(x: f64, y: f64, z: f64) -> DMat4 {
    DMat4::from_translation(DVec3::new(x, y, z))
}

#[test]
fn cylinder_radius_area_length() {
    let mut k = k();
    let s = k.make_cylinder(10.0, 3.0, 3.0, 1).unwrap();
    let b = body(&mut k, &s);
    let side = face(&b, DVec3::new(3.0, 0.0, 5.0));
    match radius(&side, DVec3::new(3.1, 0.1, 5.0)).unwrap().radius {
        Radius::Cylinder { r, axis_a, axis_b, dir, at, .. } => {
            assert!(close(r, 3.0, 1e-12), "{r}");
            assert!(close(dir.z.abs(), 1.0, 1e-12));
            let (lo, hi) = (axis_a.z.min(axis_b.z), axis_a.z.max(axis_b.z));
            assert!(close(lo, 0.0, 1e-9) && close(hi, 10.0, 1e-9), "{axis_a} {axis_b}");
            assert!(close(at.truncate().length(), 3.0, 1e-12) && close(at.z, 5.0, 1e-12), "{at}");
        }
        other => panic!("{other:?}"),
    }
    let Geo::Face { face: f, .. } = side else { unreachable!() };
    let (a, exact) = face_area(&b, f);
    assert!(exact && close(a, 2.0 * PI * 3.0 * 10.0, 1e-6 * a), "{a} vs {}", 60.0 * PI);
    let top = face(&b, DVec3::new(0.5, 0.5, 10.0));
    let Geo::Face { face: tf, .. } = top else { unreachable!() };
    let (a, exact) = face_area(&b, tf);
    assert!(exact && close(a, 9.0 * PI, 1e-10), "{a}");
    let e = edge_at(&b, DVec3::new(0.0, 3.0, 10.0));
    let c = &b.edges[e];
    assert_eq!(c.kind(), "circle");
    assert!(close(c.length(), 6.0 * PI, 1e-12));
    match radius(&Geo::Curve(c), DVec3::new(0.0, 4.0, 10.0)).unwrap().radius {
        Radius::Circle { centre, r, sweep_deg, at, .. } => {
            assert!(close(r, 3.0, 1e-12) && centre.distance(DVec3::new(0.0, 0.0, 10.0)) < 1e-12);
            assert!(close(sweep_deg, 360.0, 1e-9));
            assert!(at.distance(DVec3::new(0.0, 3.0, 10.0)) < 1e-12, "{at}");
        }
        other => panic!("{other:?}"),
    }
    // top and bottom: parallel planes
    let bottom = face(&b, DVec3::new(0.5, 0.5, 0.0));
    let d = distance(&top, &bottom, TOL).unwrap();
    assert!(d.parallel && d.exact && close(d.distance, 10.0, 1e-12), "{d:?}");
    assert!(close((d.a - d.b).length(), 10.0, 1e-12));
}

#[test]
fn cone_sphere_torus() {
    let mut k = k();
    let cone = k.make_cylinder(10.0, 4.0, 2.0, 1).unwrap();
    let b = body(&mut k, &cone);
    let side = face(&b, DVec3::new(3.0, 0.0, 5.0));
    match radius(&side, DVec3::new(3.0, 0.0, 5.0)).unwrap().radius {
        Radius::Cone { r, semi_deg, apex, .. } => {
            assert!(close(r, 3.0, 1e-9), "{r}");
            assert!(close(semi_deg, (0.2f64).atan().to_degrees(), 1e-9), "{semi_deg}");
            assert!(apex.distance(DVec3::new(0.0, 0.0, 20.0)) < 1e-9, "{apex}");
        }
        other => panic!("{other:?}"),
    }
    let Geo::Face { face: f, .. } = side else { unreachable!() };
    let slant = (10.0f64 * 10.0 + 2.0 * 2.0).sqrt();
    let (a, _) = face_area(&b, f);
    assert!(close(a, PI * (4.0 + 2.0) * slant, 1e-6 * a), "{a}");

    let sph = k.make_sphere(5.0, 2).unwrap();
    let b = body(&mut k, &sph);
    let f = face_at(&b, DVec3::new(0.0, 0.0, 5.0));
    match radius(&Geo::Face { body: &b, face: f }, DVec3::new(1.0, 1.0, 6.0)).unwrap().radius {
        Radius::Sphere { r, centre, at } => {
            assert!(close(r, 5.0, 1e-12) && centre.length() < 1e-12);
            assert!(close(at.length(), 5.0, 1e-12));
        }
        other => panic!("{other:?}"),
    }
    let total: f64 = (0..b.faces.len()).map(|i| face_area(&b, i).0).sum();
    assert!(close(total, 4.0 * PI * 25.0, 1e-6 * total), "{total}");
    let (lo, hi) = b.bounds().unwrap();
    assert!(lo.distance(DVec3::splat(-5.0)) < 1e-9 && hi.distance(DVec3::splat(5.0)) < 1e-9, "{lo} {hi}");

    let tor = k.make_torus(10.0, 2.0, 3).unwrap();
    let b = body(&mut k, &tor);
    let f = face_at(&b, DVec3::new(12.0, 0.0, 0.0));
    match radius(&Geo::Face { body: &b, face: f }, DVec3::new(0.0, 12.5, 0.5)).unwrap().radius {
        Radius::Torus { major, minor, tube, .. } => {
            assert!(close(major, 10.0, 1e-12) && close(minor, 2.0, 1e-12));
            assert!(tube.distance(DVec3::new(0.0, 10.0, 0.0)) < 1e-9, "{tube}");
        }
        other => panic!("{other:?}"),
    }
    let total: f64 = (0..b.faces.len()).map(|i| face_area(&b, i).0).sum();
    assert!(close(total, 4.0 * PI * PI * 10.0 * 2.0, 1e-6 * total), "{total}");
}

#[test]
fn fillet_and_chamfer() {
    let mut k = k();
    let bx = k.make_box(DVec3::ZERO, DVec3::new(20.0, 10.0, 8.0), 1).unwrap();
    let all: Vec<u32> = (0..12).collect();
    let f = k.fillet(&bx, &all, 1.5, 2).unwrap();
    let b = body(&mut k, &f);
    // fillet along the edge x = 20, z = 8: cylinder r = 1.5
    let p = DVec3::new(20.0 - 1.5 + 1.5 * (PI / 4.0).cos(), 5.0, 8.0 - 1.5 + 1.5 * (PI / 4.0).sin());
    match radius(&face(&b, p), p).unwrap().radius {
        Radius::Cylinder { r, .. } => assert!(close(r, 1.5, 1e-9), "{r}"),
        other => panic!("{other:?}"),
    }
    // corner: sphere r = 1.5
    let c = DVec3::new(20.0 - 1.5, 10.0 - 1.5, 8.0 - 1.5) + DVec3::ONE.normalize() * 1.5;
    match radius(&face(&b, c), c).unwrap().radius {
        Radius::Sphere { r, centre, .. } => {
            assert!(close(r, 1.5, 1e-9) && centre.distance(DVec3::new(18.5, 8.5, 6.5)) < 1e-9);
        }
        other => panic!("{other:?}"),
    }
    // fillet along the top circle of a cylinder: torus with minor radius 1
    let cyl = k.make_cylinder(10.0, 5.0, 5.0, 3).unwrap();
    let top = k.edges_where(&cyl, |e| e.points.iter().all(|q| (q.z - 10.0).abs() < 1e-6)).unwrap();
    let fc = k.fillet(&cyl, &top, 1.0, 4).unwrap();
    let b = body(&mut k, &fc);
    let q = DVec3::new(4.0 + (PI / 4.0).cos(), 0.0, 9.0 + (PI / 4.0).sin());
    match radius(&face(&b, q), q).unwrap().radius {
        Radius::Torus { major, minor, .. } => assert!(close(minor, 1.0, 1e-9) && close(major, 4.0, 1e-9)),
        other => panic!("{other:?}"),
    }

    // chamfer 2 mm on the top edges: chamfer face 45° to the top and to the sides
    let topb = k.edges_where(&bx, |e| e.points.iter().all(|q| (q.z - 8.0).abs() < 1e-6)).unwrap();
    let ch = k.chamfer(&bx, &topb, 2.0, 5).unwrap();
    let b = body(&mut k, &ch);
    let cf = DVec3::new(19.0, 5.0, 7.0);
    let topf = DVec3::new(10.0, 5.0, 8.0);
    let side = DVec3::new(20.0, 5.0, 3.0);
    let a = angle(&face(&b, cf), cf, &face(&b, topf), topf).unwrap();
    assert!(close(a.deg, 45.0, 1e-9), "{a:?}");
    assert!(close(a.opening.unwrap(), 135.0, 1e-9), "{a:?}");
    let a = angle(&face(&b, cf), cf, &face(&b, side), side).unwrap();
    assert!(close(a.deg, 45.0, 1e-9), "{a:?}");
    // the top and a side: 90°
    let a = angle(&face(&b, topf), topf, &face(&b, side), side).unwrap();
    assert!(close(a.deg, 90.0, 1e-9));
    // chamfer edges meet the top at the corner: edges at a common vertex
    let e1 = edge_at(&b, DVec3::new(10.0, 8.0, 8.0));
    let e2 = edge_at(&b, DVec3::new(18.0, 5.0, 8.0));
    let a = angle(
        &Geo::Curve(&b.edges[e1]),
        DVec3::new(10.0, 8.0, 8.0),
        &Geo::Curve(&b.edges[e2]),
        DVec3::new(18.0, 5.0, 8.0),
    )
    .unwrap();
    assert_eq!(a.kind, "vertex");
    assert!(close(a.deg, 90.0, 1e-9), "{a:?}");
}

#[test]
fn distances() {
    let mut k = k();
    let a = k.make_box(DVec3::ZERO, DVec3::splat(10.0), 1).unwrap();
    let s = k.make_sphere(2.0, 2).unwrap();
    let s = k.transform(&s, &translate(20.0, 5.0, 5.0)).unwrap();
    let u = k.boolean(BoolOp::Union, &[a, s], 3).unwrap();
    let b = body(&mut k, &u);
    // box face x = 10 to the sphere: 20 − 2 − 10 = 8, foot points exact
    let fb = face(&b, DVec3::new(10.0, 5.0, 5.0));
    let fs = face(&b, DVec3::new(18.0, 5.0, 5.0));
    let d = distance(&fb, &fs, TOL).unwrap();
    assert!(d.exact && close(d.distance, 8.0, 1e-9), "{d:?}");
    assert!(
        d.a.distance(DVec3::new(10.0, 5.0, 5.0)) < 1e-6 && d.b.distance(DVec3::new(18.0, 5.0, 5.0)) < 1e-6,
        "{d:?}"
    );
    // point to sphere
    let d = distance(&Geo::Point(DVec3::new(20.0, 5.0, 15.0)), &fs, TOL).unwrap();
    assert!(close(d.distance, 8.0, 1e-9), "{d:?}");
    // skew edges of the box: (0,0,z) and (x,10,10)
    let e1 = edge_at(&b, DVec3::new(0.0, 0.0, 5.0));
    let e2 = edge_at(&b, DVec3::new(5.0, 10.0, 10.0));
    let d = distance(&Geo::Curve(&b.edges[e1]), &Geo::Curve(&b.edges[e2]), TOL).unwrap();
    assert!(d.exact && close(d.distance, 10.0, 1e-9), "{d:?}");
    // face to a non-parallel face of the sphere: point on the box corner edge
    let top = face(&b, DVec3::new(5.0, 5.0, 10.0));
    let d = distance(&top, &fs, TOL).unwrap();
    let want = DVec3::new(10.0, 5.0, 10.0).distance(DVec3::new(20.0, 5.0, 5.0)) - 2.0;
    assert!(close(d.distance, want, 1e-7), "{} {want}", d.distance);
}

fn section_of(k: &mut SmlibKernel, s: &Shape, p: DVec3, n: DVec3) -> Section {
    let plane = SectionPlane::new(p, n);
    let x = n.any_orthonormal_vector();
    let y = n.cross(x);
    let frame = DMat4::from_cols(x.extend(0.0), y.extend(0.0), n.extend(0.0), p.extend(1.0));
    let l = 500.0;
    let bx = k.make_box(DVec3::splat(-l), DVec3::new(l, l, 0.0), 0).unwrap();
    let bx = k.transform(&bx, &frame).unwrap();
    let cut = k.boolean(BoolOp::Intersection, &[s.clone(), bx], 0).unwrap();
    let brep = k.brep(&cut).unwrap();
    Section::new(plane, vec![cut_contours(&brep, &plane, 0)])
}

#[test]
fn section_through_bores_and_wall() {
    let mut k = k();
    let plate = k.make_box(DVec3::ZERO, DVec3::new(40.0, 20.0, 6.0), 1).unwrap();
    let h1 = k.make_cylinder(10.0, 2.0, 2.0, 2).unwrap();
    let h1 = k.transform(&h1, &translate(10.0, 10.0, -2.0)).unwrap();
    let h2 = k.make_cylinder(10.0, 3.0, 3.0, 3).unwrap();
    let h2 = k.transform(&h2, &translate(28.0, 10.0, -2.0)).unwrap();
    let d = k.boolean(BoolOp::Difference, &[plate, h1, h2], 4).unwrap();
    let sec = section_of(&mut k, &d, DVec3::new(0.0, 0.0, 3.0), DVec3::Z);
    assert_eq!(sec.regions.len(), 1);
    let r = &sec.regions[0];
    assert_eq!(r.holes.len(), 2);
    let want = 40.0 * 20.0 - PI * 4.0 - PI * 9.0;
    assert!(close(r.area, want, 1e-9), "{} {want}", r.area);
    let mut radii: Vec<f64> = r
        .holes
        .iter()
        .flat_map(|&h| sec.contours[h].segs.iter().filter_map(|s| s.curve.circle().map(|c| c.2)))
        .collect();
    radii.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert!(close(radii[0], 2.0, 1e-12) && close(radii[radii.len() - 1], 3.0, 1e-12), "{radii:?}");
    assert!(sec.exact, "all segments exact");
    // wall between the small bore and the left side: 10 − 2 = 8
    let w = sec.wall(DVec3::new(8.0, 10.0, 3.0)).unwrap();
    assert!(close(w.thickness, 8.0, 1e-9), "{w:?}");
    assert!(w.to.distance(DVec3::new(0.0, 10.0, 3.0)) < 1e-9, "{w:?}");
    // between the bores: 28 − 3 − (10 + 2) = 13
    let w = sec.wall(DVec3::new(12.0, 10.0, 3.0)).unwrap();
    assert!(close(w.thickness, 13.0, 1e-9), "{w:?}");
    // from the outline inwards (front side y = 0 at x = 10): 10 − 2 = 8 up to the bore
    let w = sec.wall(DVec3::new(10.0, -1.0, 3.0)).unwrap();
    assert!(close(w.thickness, 8.0, 1e-9), "{w:?}");
    assert!(sec.region_at(DVec3::new(1.0, 1.0, 3.0)) == Some(0));
    assert!(sec.region_at(DVec3::new(10.0, 10.0, 3.0)).is_none());
    // section along the bore axes (y = 10): rectangles, holes as straight walls
    let sec = section_of(&mut k, &d, DVec3::new(0.0, 10.0, 0.0), DVec3::Y);
    let total = sec.area();
    let want = 40.0 * 6.0 - 4.0 * 6.0 - 6.0 * 6.0;
    assert!(close(total, want, 1e-9), "{total} {want}");
    // oblique cut of the big bore: ellipse
    let n = DVec3::new(1.0, 0.0, 1.0).normalize();
    let sec = section_of(&mut k, &d, DVec3::new(28.0, 10.0, 3.0), n);
    let ell = sec.contours.iter().flat_map(|c| c.segs.iter()).find(|s| s.curve.kind() == "ellipse");
    let e = ell.expect("ellipse in the oblique section");
    let cad_measure::EdgeCurve::Ellipse { r1, r2, .. } = e.curve else { unreachable!() };
    assert!(close(r1.max(r2), 3.0 * 2f64.sqrt(), 1e-9) && close(r1.min(r2), 3.0, 1e-9), "{r1} {r2}");
    assert!(e.exact, "{e:?}");
}

#[test]
fn names_and_errors_in_both_languages() {
    let mut k = k();
    let s = k.make_cylinder(10.0, 3.0, 3.0, 1).unwrap();
    let b = body(&mut k, &s);
    let side = face(&b, DVec3::new(3.0, 0.0, 5.0));
    let top = face(&b, DVec3::new(0.5, 0.5, 10.0));
    let at = DVec3::new(3.1, 0.1, 5.0);
    {
        let _en = cad_i18n::scoped(cad_i18n::Lang::En);
        assert_eq!(radius(&side, at).unwrap().source, "cylinder");
        assert_eq!(radius(&top, at).unwrap_err(), "radius: the face is a plane – no radius");
    }
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    assert_eq!(radius(&side, at).unwrap().source, "Zylinder");
    assert_eq!(radius(&top, at).unwrap_err(), "Radius: die Fläche ist eine Ebene – kein Radius");
}
