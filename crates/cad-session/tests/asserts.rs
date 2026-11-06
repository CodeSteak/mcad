//! Geometric assertions (Plan 21 C) over the kernel solidcore: values against formulas.

use std::f64::consts::PI;

use cad_session::protocol::*;
use cad_session::{Session, SessionOptions, std_clock};
use kernel_solidcore::SolidcoreKernel;

fn session() -> Session<SolidcoreKernel> {
    Session::new(SolidcoreKernel::new(), SessionOptions::default(), std_clock())
}

/// Evaluates `src` and returns the assertion results and all messages.
fn run(s: &mut Session<SolidcoreKernel>, src: &str, version: u64) -> (Vec<AssertResult>, Vec<FromEngine>) {
    let mut msgs = s.post(ToEngine::SetSource { text: src.into(), version });
    msgs.extend(s.run_to_idle());
    let res = msgs
        .iter()
        .rev()
        .find_map(|m| match m {
            FromEngine::Asserts { items, .. } => Some(items.clone()),
            _ => None,
        })
        .unwrap_or_default();
    (res, msgs)
}

fn check(src: &str) -> Vec<AssertResult> {
    run(&mut session(), src, 1).0
}

fn warnings(msgs: &[FromEngine]) -> Vec<String> {
    msgs.iter()
        .rev()
        .find_map(|m| match m {
            FromEngine::Diagnostics { items, .. } => Some(items.clone()),
            _ => None,
        })
        .unwrap_or_default()
        .into_iter()
        .filter(|d| d.severity == Severity::Warning)
        .map(|d| d.message)
        .collect()
}

fn close(what: &str, got: f64, want: f64, tol: f64) {
    assert!((got - want).abs() <= tol, "{what}: {got} statt {want} (± {tol})");
}

const PLATE: &str = "cube([40, 30, 3]);\n";

#[test]
fn keepout_overlap_is_exact_and_not_in_the_result() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let mut s = session();
    let src = format!("{PLATE}assert_keepout() translate([10, 10, 2]) cube([10, 10, 5]);\n");
    let (r, msgs) = run(&mut s, &src, 1);
    assert_eq!(r.len(), 1);
    let a = &r[0];
    assert!(a.checked && !a.ok, "{a:?}");
    assert_eq!(a.kind, AssertKind::Keepout);
    assert_eq!(a.pairs.len(), 1);
    let p = &a.pairs[0];
    assert_eq!(p.problem.as_deref(), Some("overlap"));
    close("Schnittvolumen", p.volume.unwrap(), 10.0 * 10.0 * 1.0, 1e-6);
    assert!(p.overlap.is_some() && p.focus.is_some());
    assert!(p.b.origin.iter().any(|o| o.starts_with("Zeile 1: box")), "{:?}", p.b);
    let w = warnings(&msgs);
    assert!(w.iter().any(|m| m.starts_with("assert_keepout: ") && m.contains("ragt ins Sperrvolumen")), "{w:?}");
    // the keep-out volume is no material: root volume = plate
    let d = s.done().unwrap();
    let root = d.scene.root.unwrap();
    let kn = d.scene.nodes.iter().find(|n| n.flags.keepout).unwrap();
    assert!(kn.flags.background && kn.label == "assert_keepout");
    assert!(!d.scene.nodes[root as usize].children.is_empty());
    close("Volumen", s.mass_props(root).unwrap().volume, 40.0 * 30.0 * 3.0, 1e-6);
}

#[test]
fn keepout_clearance() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    // 0.3 mm above the plate, 0.5 mm required
    let r = check(&format!("{PLATE}assert_keepout(clearance = 0.5) translate([10, 10, 3.3]) cube(5);\n"));
    let p = &r[0].pairs[0];
    assert_eq!(p.problem.as_deref(), Some("too_close"), "{p:?}");
    close("distance", p.distance.unwrap(), 0.3, 1e-5);
    assert_eq!(p.points.len(), 2);
    // a: the keep-out volume, b: the plate
    close("foot point keep-out volume", p.points[0].0[2], 3.3, 1e-5);
    close("foot point plate", p.points[1].0[2], 3.0, 1e-5);
    // 1 mm: fine (no pair listed)
    let r = check(&format!("{PLATE}assert_keepout(clearance = 0.5) translate([10, 10, 4]) cube(5);\n"));
    assert!(r[0].ok && r[0].pairs.is_empty(), "{:?}", r[0]);
    // clearance 0: touching is fine
    let r = check(&format!("{PLATE}assert_keepout() translate([10, 10, 3]) cube(5);\n"));
    assert!(r[0].ok, "{:?}", r[0]);
}

#[test]
fn keepout_checks_every_part() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let src = "module teil(x) translate([x, 0, 0]) cube(10);\n\
               a: teil(0);\nb: teil(20);\n\
               assert_keepout() translate([25, 5, 5]) cube(10);\n";
    let r = check(src);
    let a = &r[0];
    assert_eq!(a.bodies, 2);
    assert_eq!(a.pairs.len(), 1, "{a:?}");
    assert_eq!(a.pairs[0].b.label, "b");
    close("Schnittvolumen", a.pairs[0].volume.unwrap(), 5.0 * 5.0 * 5.0, 1e-6);
}

#[test]
fn disjoint() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let r = check("assert_disjoint() { cube(10); translate([8, 0, 0]) cube(10); }\n");
    let p = &r[0].pairs[0];
    assert!(!p.ok);
    close("Schnittvolumen", p.volume.unwrap(), 2.0 * 10.0 * 10.0, 1e-6);
    // apart and touching: fine (a failing boolean at touching faces would be a note)
    let r = check("assert_disjoint() { cube(10); translate([12, 0, 0]) cube(10); translate([0, 10, 0]) cube(10); }\n");
    assert!(r[0].ok, "{:?}", r[0]);
    assert_eq!(r[0].pairs.len(), 3);
}

#[test]
fn intersect_needs_a_connected_group() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    // chain a–b–c: a and c do not meet, but the group is connected
    let chain = "assert_intersect() { cube(10); translate([8, 0, 0]) cube(10); translate([16, 0, 0]) cube(10); }\n";
    let r = check(chain);
    assert!(r[0].ok, "{:?}", r[0]);
    // a fourth child far away: one violation (closest pair to the group)
    let r = check(
        "assert_intersect() { cube(10); translate([8, 0, 0]) cube(10); translate([16, 0, 0]) cube(10); translate([40, 0, 0]) cube(10); }\n",
    );
    let bad: Vec<&AssertPair> = r[0].pairs.iter().filter(|p| !p.ok).collect();
    assert_eq!(bad.len(), 1, "{:?}", r[0].pairs);
    assert_eq!(bad[0].problem.as_deref(), Some("apart"));
    close("distance", bad[0].distance.unwrap(), 40.0 - 26.0, 1e-5);
    // touching is no intersection
    let r = check("assert_intersect() { cube(10); translate([10, 0, 0]) cube(10); }\n");
    assert!(!r[0].ok);
}

#[test]
fn touches_face_contact_area() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    // box face 5 × 5 on the cube's face x = 10
    let r = check("assert_touches() { cube(10); translate([10, 2, 0]) cube([10, 5, 5]); }\n");
    let p = &r[0].pairs[0];
    assert!(p.ok, "{p:?}");
    assert_eq!(p.contact.as_deref(), Some("face"));
    assert!(p.contact_exact);
    close("contact area", p.contact_area.unwrap(), 25.0, 1e-9);
    assert!(p.contact_mesh.is_some() && !p.contact_lines.is_empty());
    // circle on a face: π r² exactly (arcs in region2)
    let r = check("assert_touches() { cube(10); translate([5, 5, 10]) cylinder(h = 5, r = 3); }\n");
    let p = &r[0].pairs[0];
    assert!(p.ok, "{p:?}");
    close("contact area circle", p.contact_area.unwrap(), PI * 9.0, 1e-6);
    assert!(p.contact_exact, "{p:?}");
}

#[test]
fn touches_line_and_point_contact() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let line =
        "assert_touches() { cube([30, 30, 3]); translate([5, 15, 8]) rotate([0, 90, 0]) cylinder(h = 20, r = 5); }\n";
    let r = check(line);
    let p = &r[0].pairs[0];
    assert_eq!(p.problem.as_deref(), Some("line_contact"), "{p:?}");
    assert!(!p.contact_lines.is_empty());
    let r = check(&line.replace("assert_touches()", "assert_touches(contact = \"any\")"));
    assert!(r[0].ok && r[0].pairs[0].contact.as_deref() == Some("line"), "{:?}", r[0]);
    let r = check("assert_touches() { cube([30, 30, 3]); translate([15, 15, 8]) sphere(5); }\n");
    assert_eq!(r[0].pairs[0].problem.as_deref(), Some("point_contact"), "{:?}", r[0].pairs[0]);
}

/// Surfaces that cannot lie on each other over an area touch along a line, however wide the
/// band of triangles within the tolerance: a torus on a plate (true contact a circle) and two
/// cylinders side by side 0.005 apart (SMLib's mesh gave a "face contact" of 0.002 mm² between
/// a cap's rim and the other side).
#[test]
fn touches_curved_surfaces_by_their_kind() {
    let r = check("assert_touches() { torus(20, 2); translate([-30, -30, 2]) cube([60, 60, 1]); }\n");
    assert_eq!(r[0].pairs[0].problem.as_deref(), Some("line_contact"), "{:?}", r[0].pairs[0]);
    let r = check(
        "assert_touches(tol = 0.01) { cylinder(r = 5, h = 10); translate([10.005, 0, 0]) cylinder(r = 5, h = 10); }\n",
    );
    assert_eq!(r[0].pairs[0].problem.as_deref(), Some("line_contact"), "{:?}", r[0].pairs[0]);
}

/// Surfaces of different kinds (or planes at an angle) whose gap stays within the tolerance over
/// their whole common extent touch over an area: a plate tilted by 0.005° (gap ≤ 0.0017 over
/// 20 mm), a cone of radius 5 → 5.002 in a hole of radius 5.002; a ball in its seat (concentric
/// spheres of equal radius, also 0.005 larger: within the tolerance).
#[test]
fn touches_within_the_tolerance_over_the_extent() {
    for src in [
        "assert_touches(tol = 0.01) { translate([-20, -20, -2]) cube([40, 40, 2]); rotate([0.005, 0, 0]) translate([-10, -10, 0.001]) cube(20); }\n",
        "assert_touches(tol = 0.01) { difference() { cylinder(r = 10, h = 20); translate([0, 0, -1]) cylinder(r = 5.002, h = 22); } cylinder(r1 = 5, r2 = 5.002, h = 20); }\n",
        "assert_touches(tol = 0.01) { difference() { cube(20, center = true); sphere(5); } sphere(5); }\n",
        "assert_touches(tol = 0.01) { difference() { translate([0, 0, -5]) cube([20, 20, 10], center = true); sphere(5.005); } sphere(5); }\n",
    ] {
        let r = check(src);
        let p = &r[0].pairs[0];
        assert!(p.ok && p.contact.as_deref() == Some("face"), "{src}: {p:?}");
    }
    // tilted by 1°: a line (an edge on the face)
    let r = check(
        "assert_touches(tol = 0.01) { translate([-20, -20, -2]) cube([40, 40, 2]); rotate([1, 0, 0]) translate([-10, 0, 0]) cube(20); }\n",
    );
    assert_eq!(r[0].pairs[0].contact.as_deref(), Some("line"), "{:?}", r[0].pairs[0]);
}

/// Planes at an angle that meet at a rim touch along a line: the cube's front face and the
/// plate's side face are near each other only at the shared edge (only there were mesh
/// vertices of either face near the other, so their projected overlap of 20 mm² counted as
/// face contact at 30°, 45°, 60° and 80°).
#[test]
fn touches_planes_meeting_at_a_rim() {
    for angle in [10, 30, 45, 60, 80] {
        let src = format!(
            "assert_touches(tol = 0.01) {{ translate([-20, -20, -2]) cube([40, 40, 2]); translate([-5, 20, 0]) rotate([-{angle}, 0, 0]) cube(10); }}\n"
        );
        let r = check(&src);
        let p = &r[0].pairs[0];
        assert!(!r[0].ok && p.contact.as_deref() == Some("line"), "{angle}°: {p:?}");
        assert_eq!(p.problem.as_deref(), Some("line_contact"), "{angle}°: {p:?}");
    }
}

/// A line contact is drawn along the contact curve: the circle of radius 20 of a torus on a
/// plate as one closed polyline, not a chord between two extreme vertices.
#[test]
fn line_contact_follows_the_curve() {
    let r = check("assert_touches(contact = \"any\") { torus(20, 2); translate([-30, -30, 2]) cube([60, 60, 1]); }\n");
    let p = &r[0].pairs[0];
    assert_eq!(p.contact.as_deref(), Some("line"), "{p:?}");
    assert_eq!(p.contact_lines.len(), 1, "{:?}", p.contact_lines.iter().map(|l| l.len()).collect::<Vec<_>>());
    let l = &p.contact_lines[0];
    assert!(l.len() > 50 && l[0] == l[l.len() - 1], "{}", l.len());
    for q in l {
        let r = q.0[0].hypot(q.0[1]);
        assert!((r - 20.0).abs() < 0.5 && (q.0[2] - 2.0).abs() < 0.05, "{q:?}");
    }
}

#[test]
fn touches_gap_and_overlap() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let r = check("assert_touches() { cube(10); translate([10.2, 0, 0]) cube(10); }\n");
    let p = &r[0].pairs[0];
    assert_eq!(p.problem.as_deref(), Some("gap"));
    close("gap", p.distance.unwrap(), 0.2, 1e-5);
    // within tol: contact
    let r = check("assert_touches(tol = 0.3) { cube(10); translate([10.2, 0, 0]) cube(10); }\n");
    assert!(r[0].ok, "{:?}", r[0]);
    let r = check("assert_touches() { cube(10); translate([9, 0, 0]) cube(10); }\n");
    let p = &r[0].pairs[0];
    assert_eq!(p.problem.as_deref(), Some("overlap"));
    close("Schnittvolumen", p.volume.unwrap(), 100.0, 1e-6);
}

#[test]
fn imported_mesh_as_keepout() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let stl = include_str!("../../../tests/models/stl/bracket.stl");
    let mut s = session();
    s.post(ToEngine::SetFiles { files: [("bracket.stl".to_string(), stl.to_string())].into(), main: None });
    let src = format!("{PLATE}assert_keepout() translate([5, 5, 2]) import(\"bracket.stl\", as = \"mesh\");\n");
    let (r, _) = run(&mut s, &src, 1);
    let a = &r[0];
    assert!(!a.ok, "{a:?}");
    let p = &a.pairs[0];
    assert!(p.mesh_based && p.problem.as_deref() == Some("overlap"), "{p:?}");
    assert!(a.note.as_deref().unwrap_or("").contains("Netz"));
    // above the plate: free
    let (r, _) = run(&mut s, &src.replace("[5, 5, 2]", "[5, 5, 3.5]"), 2);
    assert!(r[0].ok, "{:?}", r[0]);
}

#[test]
fn results_are_cached_and_skipped_while_dragging() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let mut s = session();
    let src = "assert_disjoint() { cube(10); translate([8, 0, 0]) cube(10); }\n";
    let (r, _) = run(&mut s, src, 1);
    assert!(!r[0].cached);
    // same content again (other version): every pair from the engine cache
    let (r, _) = run(&mut s, &format!("{src}\n"), 2);
    assert!(r[0].cached && !r[0].ok, "{:?}", r[0]);
    assert_eq!(s.done().unwrap().scene.asserts.len(), 1);
    // drag: unchecked
    s.post(ToEngine::DragStart { handle: 0 });
    let (r, _) = run(&mut s, &src.replace("[8, 0, 0]", "[7, 0, 0]"), 3);
    assert!(!r[0].checked && r[0].ok, "{:?}", r[0]);
}

#[test]
fn warnings_of_the_statement() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let mut s = session();
    let (_, msgs) =
        run(&mut s, "assert_disjoint() cube(1);\nassert_touches(contact = \"edge\", tol = -1) { cube(1); }\n", 1);
    let all: Vec<String> = msgs
        .iter()
        .filter_map(|m| match m {
            FromEngine::Diagnostics { items, .. } => Some(items.iter().map(|d| d.message.clone()).collect::<Vec<_>>()),
            _ => None,
        })
        .flatten()
        .collect();
    assert!(all.iter().any(|m| m.contains("mindestens zwei Kinder")), "{all:?}");
    assert!(all.iter().any(|m| m.contains("`contact` muss")), "{all:?}");
    assert!(all.iter().any(|m| m.contains("`tol` muss > 0")), "{all:?}");
}

/// The kernel with a failing tessellation (as solidcore before its tessellator).
struct NoMesh(SolidcoreKernel);

mod no_mesh {
    use cad_kernel::*;
    use glam::{DMat4, DVec2, DVec3};
    use kernel_solidcore::Shape;

    impl Kernel for super::NoMesh {
        type Shape = Shape;
        fn id(&self) -> &'static str {
            "no-mesh"
        }
        fn make_box(&mut self, min: DVec3, max: DVec3, tag: Tag) -> KResult<Shape> {
            self.0.make_box(min, max, tag)
        }
        fn make_sphere(&mut self, r: f64, tag: Tag) -> KResult<Shape> {
            self.0.make_sphere(r, tag)
        }
        fn make_cylinder(&mut self, h: f64, r1: f64, r2: f64, tag: Tag) -> KResult<Shape> {
            self.0.make_cylinder(h, r1, r2, tag)
        }
        fn make_torus(&mut self, big_r: f64, r: f64, tag: Tag) -> KResult<Shape> {
            self.0.make_torus(big_r, r, tag)
        }
        fn make_polyhedron(&mut self, points: &[DVec3], faces: &[Vec<u32>], tag: Tag) -> KResult<Shape> {
            self.0.make_polyhedron(points, faces, tag)
        }
        fn transform(&mut self, s: &Shape, m: &DMat4) -> KResult<Shape> {
            self.0.transform(s, m)
        }
        fn boolean(&mut self, op: BoolOp, shapes: &[Shape], tag: Tag) -> KResult<Shape> {
            self.0.boolean(op, shapes, tag)
        }
        fn fillet(&mut self, s: &Shape, edges: &[EdgeId], r: f64, tag: Tag) -> KResult<Shape> {
            self.0.fillet(s, edges, r, tag)
        }
        fn chamfer(&mut self, s: &Shape, edges: &[EdgeId], d: f64, tag: Tag) -> KResult<Shape> {
            self.0.chamfer(s, edges, d, tag)
        }
        fn extrude(&mut self, p: &PlacedLoop, h: f64, twist: f64, scale: DVec2, tag: Tag) -> KResult<Shape> {
            self.0.extrude(p, h, twist, scale, tag)
        }
        fn revolve(&mut self, p: &PlacedLoop, angle: f64, tag: Tag) -> KResult<Shape> {
            self.0.revolve(p, angle, tag)
        }
        fn loft(&mut self, sections: &[PlacedLoop], ruled: bool, tag: Tag) -> KResult<Shape> {
            self.0.loft(sections, ruled, tag)
        }
        fn sweep(&mut self, p: &Loop, path: &[DVec3], interp: PathInterp, tag: Tag) -> KResult<Shape> {
            self.0.sweep(p, path, interp, tag)
        }
        fn nurbs_surface(&mut self, rows: &[Vec<DVec3>], tag: Tag) -> KResult<Shape> {
            self.0.nurbs_surface(rows, tag)
        }
        fn thicken(&mut self, s: &Shape, t: f64, tag: Tag) -> KResult<Shape> {
            self.0.thicken(s, t, tag)
        }
        fn shell(&mut self, s: &Shape, t: f64, open: &[FaceId], tag: Tag) -> KResult<Shape> {
            self.0.shell(s, t, open, tag)
        }
        fn edges(&mut self, s: &Shape, samples: u32) -> KResult<Vec<EdgeInfo>> {
            self.0.edges(s, samples)
        }
        fn faces(&mut self, s: &Shape, samples: u32) -> KResult<Vec<FaceInfo>> {
            self.0.faces(s, samples)
        }
        fn classify(&mut self, s: &Shape, pts: &[DVec3]) -> KResult<Vec<Class>> {
            self.0.classify(s, pts)
        }
        fn tessellate(&mut self, _: &Shape, _: f64) -> KResult<Mesh> {
            Err(KernelError::Unsupported("tessellate".into()))
        }
        fn mass_props(&mut self, s: &Shape) -> KResult<MassProps> {
            self.0.mass_props(s)
        }
        fn bounds(&mut self, s: &Shape) -> KResult<Option<(DVec3, DVec3)>> {
            self.0.bounds(s)
        }
        fn is_empty(&mut self, s: &Shape) -> bool {
            self.0.is_empty(s)
        }
        fn is_solid(&mut self, s: &Shape) -> bool {
            self.0.is_solid(s)
        }
    }
}

#[test]
fn failing_tessellation_leaves_contact_and_intersection_unchecked() {
    let _en = cad_i18n::scoped(cad_i18n::Lang::En);
    let mut s = Session::new(NoMesh(SolidcoreKernel::new()), SessionOptions::default(), std_clock());
    let mut msgs = s.post(ToEngine::SetSource {
        text: "assert_touches() { cube(10); translate([10, 0, 0]) cube(10); }\n\
               assert_intersect() { cube(10); translate([10, 0, 0]) cube(10); }\n"
            .into(),
        version: 1,
    });
    msgs.extend(s.run_to_idle());
    let r = msgs
        .iter()
        .rev()
        .find_map(|m| match m {
            FromEngine::Asserts { items, .. } => Some(items.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(r.len(), 2);
    for a in &r {
        let p = &a.pairs[0];
        assert!(a.ok && p.ok, "{a:?}");
        assert_eq!(p.problem.as_deref(), Some("unchecked"), "{p:?}");
        assert!(p.message.contains("could not be checked") && p.message.contains("tessellate"), "{}", p.message);
    }
    assert!(r[0].pairs[0].message.contains("contact could not be checked"));
    assert!(warnings(&msgs).is_empty(), "{:?}", warnings(&msgs));
}
