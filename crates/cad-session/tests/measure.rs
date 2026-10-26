//! Measure mode over the protocol and through `mcad measure` references (Plan 18): snapping
//! (vertex, edge midpoint, circle centre, edge, face, contour), measurements against
//! analytic values, sections (wall thickness of the housing from tests/models/spec), provenance
//! (grid planes and points, anchors, named instances).

use cad_measure::section::SectionPlane;
use cad_session::protocol::*;
use cad_session::{Session, SessionOptions, std_clock};
use glam::DVec3;
use kernel_smlib::SmlibKernel;

fn session(text: &str) -> Session<SmlibKernel> {
    let mut s = Session::new(SmlibKernel::new(), SessionOptions::default(), std_clock());
    s.post(ToEngine::SetSource { text: text.into(), version: 1 });
    s.run_to_idle();
    s
}

fn example(name: &str) -> String {
    std::fs::read_to_string(format!("{}/../../tests/models/spec/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

fn root(s: &Session<SmlibKernel>) -> u32 {
    s.done().unwrap().scene.root.unwrap()
}

/// Ray cast on the root's display mesh: the hit as the UI sends it.
fn hit(s: &mut Session<SmlibKernel>, o: DVec3, d: DVec3) -> Option<MeasureHit> {
    let r = root(s);
    let mesh = s.mesh(r, false).unwrap();
    let p = |i: u32| {
        let k = i as usize * 3;
        DVec3::new(mesh.positions[k] as f64, mesh.positions[k + 1] as f64, mesh.positions[k + 2] as f64)
    };
    let mut best: Option<(f64, u32)> = None;
    for (t, f) in mesh.indices.chunks_exact(3).zip(mesh.tri_faces.iter()) {
        let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
        let (e1, e2) = (b - a, c - a);
        let h = d.cross(e2);
        let det = e1.dot(h);
        if det.abs() < 1e-12 {
            continue;
        }
        let inv = 1.0 / det;
        let sv = o - a;
        let u = inv * sv.dot(h);
        let q = sv.cross(e1);
        let w = inv * d.dot(q);
        if !(0.0..=1.0).contains(&u) || w < 0.0 || u + w > 1.0 {
            continue;
        }
        let t = inv * e2.dot(q);
        if t > 0.0 && best.is_none_or(|x| t < x.0) {
            best = Some((t, *f));
        }
    }
    best.map(|(t, f)| MeasureHit { node: r, face: f, point: Vec3((o + d * t).to_array()) })
}

fn answer(msgs: Vec<FromEngine>) -> MeasureAnswer {
    msgs.into_iter()
        .find_map(|m| match m {
            FromEngine::Measure(a) => Some(*a),
            _ => None,
        })
        .expect("Measure-Antwort")
}

/// Mouse over `(x, y)` from above (100 mm away: with these settings the snap radius is 1 mm).
fn over(
    s: &mut Session<SmlibKernel>,
    kind: MeasureKind,
    picks: Vec<MeasurePick>,
    o: DVec3,
    d: DVec3,
    click: bool,
    section: Option<SectionPlaneRef>,
) -> MeasureAnswer {
    let h = hit(s, o, d);
    answer(s.post(ToEngine::Measure(Box::new(MeasureRequest {
        id: 1,
        kind,
        picks,
        ray: Some(Ray { origin: Vec3(o.to_array()), dir: Vec3(d.to_array()) }),
        hit: h,
        pixel: 0.001,
        snap_px: 10.0,
        free: false,
        click,
        section,
        clip: None,
        node: None,
    }))))
}

fn down(x: f64, y: f64) -> (DVec3, DVec3) {
    (DVec3::new(x, y, 100.0), DVec3::NEG_Z)
}

fn parse_point(t: &str) -> Result<DVec3, String> {
    let v: Vec<f64> = t
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(|x| x.trim().parse().map_err(|_| t.to_string()))
        .collect::<Result<_, _>>()?;
    (v.len() == 3).then(|| DVec3::new(v[0], v[1], v[2])).ok_or_else(|| t.to_string())
}

/// A measurement from `mcad measure` references (as the CLI does it).
fn by_refs(s: &mut Session<SmlibKernel>, kind: MeasureKind, refs: &[&str], plane: Option<SectionPlane>) -> Measurement {
    let r = root(s);
    let m = s.measure_model(r).unwrap();
    let sec = plane.map(|p| s.measure_section(r, &p));
    let sec = sec.as_ref().map(|x| x.as_ref().as_ref().unwrap());
    let picks: Vec<MeasurePick> = refs.iter().map(|t| s.measure_ref(&m, sec, t, &parse_point).unwrap()).collect();
    s.measurement(&m, sec, kind, &picks).unwrap()
}

fn value(m: &Measurement, key: &str) -> f64 {
    m.values.iter().find(|v| v.key == key).unwrap_or_else(|| panic!("{key} in {:?}", m.values)).value
}

const BOX: &str =
    "grid g {\n  x: a = 0, b = 20;\n  y: v = 0, h = 10;\n  z: u = 0, o = 8;\n}\nbox(g[a, v, u], g[b, h, o]);\n";

#[test]
fn snaps_vertices_midpoints_faces_with_provenance() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let mut s = session(BOX);
    let (o, d) = down(10.0, 5.0);
    let a = over(&mut s, MeasureKind::Distance, vec![], o, d, false, None);
    let f = a.hover.expect("face");
    assert_eq!(f.kind, "face");
    assert!(f.origin.iter().any(|x| x == "Rasterebene g.o"), "{:?}", f.origin);
    assert!(f.origin.iter().any(|x| x.starts_with("Zeile 6: box")), "{:?}", f.origin);
    assert!(f.reference.starts_with("face@[10,5,8]"), "{}", f.reference);
    // near the corner: the vertex (snap radius 1 mm at 100 mm)
    let (o, d) = down(19.6, 9.7);
    let v = over(&mut s, MeasureKind::Distance, vec![], o, d, false, None).hover.unwrap();
    assert_eq!(v.kind, "vertex");
    assert_eq!(v.point, Vec3([20.0, 10.0, 8.0]));
    assert!(v.origin.iter().any(|x| x == "Rasterpunkt g[b, h, o]"), "{:?}", v.origin);
    // near the middle of the back top edge: its midpoint
    let (o, d) = down(10.3, 9.8);
    let m = over(&mut s, MeasureKind::Distance, vec![], o, d, false, None).hover.unwrap();
    assert_eq!(m.kind, "mid");
    assert_eq!(m.point, Vec3([10.0, 10.0, 8.0]));
    // on the edge away from the midpoint: the edge
    let (o, d) = down(4.0, 9.7);
    let e = over(&mut s, MeasureKind::Distance, vec![], o, d, false, None).hover.unwrap();
    assert_eq!(e.kind, "edge");
    assert!((e.point.0[0] - 4.0).abs() < 1e-9 && (e.point.0[1] - 10.0).abs() < 1e-12, "{:?}", e.point);
    // Radius on a plane: nothing to pick
    let (o, d) = down(10.0, 5.0);
    assert!(over(&mut s, MeasureKind::Radius, vec![], o, d, false, None).hover.is_none());
}

/// The same measurement in English (the default language).
#[test]
fn distance_texts_in_english() {
    let mut s = session(BOX);
    let (o, d) = down(10.0, 5.0);
    let a = over(&mut s, MeasureKind::Distance, vec![], o, d, true, None);
    assert!(a.prompt.contains("second element"), "{}", a.prompt);
    let a = over(&mut s, MeasureKind::Distance, a.picks, DVec3::new(10.0, 5.0, -100.0), DVec3::Z, true, None);
    let r = a.result.expect("result");
    assert_eq!(r.title, "Distance");
    assert!(r.details.iter().any(|x| x.contains("parallel planes: normal distance")), "{:?}", r.details);
    assert!(r.text.starts_with("Distance face · "), "{}", r.text);
    assert!(!r.text.contains("Fläche"), "{}", r.text);
}

#[test]
fn distance_by_clicks_parallel_planes_and_cli() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let mut s = session(BOX);
    let (o, d) = down(10.0, 5.0);
    let a = over(&mut s, MeasureKind::Distance, vec![], o, d, true, None);
    assert_eq!(a.picks.len(), 1);
    assert!(a.result.is_none());
    assert!(a.prompt.contains("zweites"), "{}", a.prompt);
    let a = over(&mut s, MeasureKind::Distance, a.picks, DVec3::new(10.0, 5.0, -100.0), DVec3::Z, true, None);
    let r = a.result.expect("result");
    assert!((r.value - 8.0).abs() < 1e-12, "{r:?}");
    assert!(r.exact && r.details.iter().any(|x| x.contains("parallele Ebenen")));
    assert_eq!(r.cli, vec!["--distance", "face@[10,5,8]", "face@[10,5,0]"]);
    assert!(r.text.starts_with("Abstand Fläche · Ebene – Fläche · Ebene: 8 mm"), "{}", r.text);
    assert!(a.picks.is_empty());
    // the same through the references
    let r2 = by_refs(&mut s, MeasureKind::Distance, &["face@[10,5,8]", "face@[10,5,0]"], None);
    assert_eq!(r2.value, r.value);
    // vertex to vertex: diagonal
    let r = by_refs(&mut s, MeasureKind::Distance, &["vertex@[0,0,0]", "vertex@[20,10,8]"], None);
    assert!((r.value - (400.0f64 + 100.0 + 64.0).sqrt()).abs() < 1e-12);
    assert_eq!(value(&r, "dx"), 20.0);
    // length and area
    let r = by_refs(&mut s, MeasureKind::Length, &["edge@[5,0,8]"], None);
    assert!((r.value - 20.0).abs() < 1e-12);
    let r = by_refs(&mut s, MeasureKind::Area, &["face@[10,5,8]"], None);
    assert!((r.value - 200.0).abs() < 1e-9 && r.exact);
    let r = by_refs(&mut s, MeasureKind::Body, &["body@[10,5,8]"], None);
    assert!((value(&r, "volume") - 1600.0).abs() < 1e-3, "{r:?}");
    assert_eq!((value(&r, "size_x"), value(&r, "size_y"), value(&r, "size_z")), (20.0, 10.0, 8.0));
    assert_eq!(r.guides.len(), 12);
}

#[test]
fn radius_of_bores_and_fillets_in_the_hole_plate() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let mut s = session(&example("hole_plate.mcad"));
    // bore d = 4 at x = 6, y = 6 (first hole)
    let r = by_refs(&mut s, MeasureKind::Radius, &["face@[8,6,2]"], None);
    assert!((r.value - 2.0).abs() < 1e-9, "{r:?}");
    assert!((value(&r, "diameter") - 4.0).abs() < 1e-9);
    assert!(r.label.starts_with("R 2 · ⌀ 4"), "{}", r.label);
    assert!(r.picks[0].origin.iter().any(|x| x.contains("Rasterlinie g[hole[0], mid, *]")), "{:?}", r.picks[0].origin);
    // fillet 1 mm at the upper bore edge: tube radius 1 around a circle of radius 3
    let c = std::f64::consts::FRAC_1_SQRT_2;
    let q = DVec3::new(6.0 + 3.0 - c, 6.0, 3.0 + c);
    let r = by_refs(&mut s, MeasureKind::Radius, &[&format!("face@[{},{},{}]", q.x, q.y, q.z)], None);
    assert!((r.value - 1.0).abs() < 1e-6, "{r:?}");
    if let Some(major) = r.values.iter().find(|v| v.key == "major") {
        assert!((major.value - 3.0).abs() < 1e-6, "{r:?}");
    }
    // the fillet meets the bottom face in a circle of radius 2 + 1 around the bore axis
    let r = by_refs(&mut s, MeasureKind::Radius, &["edge@[9,6,0]"], None);
    assert!((r.value - 3.0).abs() < 1e-9, "{r:?}");
    // its quarter-circle boundary on the section y = 6: radius 1
    let r = by_refs(&mut s, MeasureKind::Radius, &["edge@[8.292893,6,0.292893]"], None);
    assert!((r.value - 1.0).abs() < 1e-9, "{r:?}");
    let ctr = by_refs(&mut s, MeasureKind::Distance, &["center@[6,6,0]", "center@[18,6,0]"], None);
    assert!((ctr.value - 12.0).abs() < 1e-9, "pitch {ctr:?}");
}

#[test]
fn housing_wall_thickness_in_the_section_and_inner_distance() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let mut s = session(&example("housing.mcad"));
    let plane = SectionPlane::new(DVec3::new(0.0, 0.0, 10.0), DVec3::Z);
    // the side walls of k: housing(w = 60) are t = 3 thick
    let w = by_refs(&mut s, MeasureKind::Wall, &["[0,15,10]"], Some(plane));
    assert!((w.value - 3.0).abs() < 1e-12, "{w:?}");
    assert!(w.exact);
    assert_eq!(w.cli, vec!["--section", "z=10", "--wall", "[0,15,10]"]);
    let w = by_refs(&mut s, MeasureKind::Wall, &["[60,20,10]"], Some(plane));
    assert!((w.value - 3.0).abs() < 1e-12, "{w:?}");
    // area of a wall region: 3 × 30
    let a = by_refs(&mut s, MeasureKind::Area, &["region@[1.5,15,10]"], Some(plane));
    assert!((a.value - 90.0).abs() < 1e-9, "{a:?}");
    // distance of the inner faces: 60 − 2·3
    let d = by_refs(&mut s, MeasureKind::Distance, &["face@[3,15,10]", "face@[57,15,10]"], None);
    assert!((d.value - 54.0).abs() < 1e-12 && d.exact, "{d:?}");
    assert!(d.picks[0].origin.iter().any(|x| x.contains("Instanz k: housing")), "{:?}", d.picks[0].origin);
    // in the section: contour segments
    let d = by_refs(&mut s, MeasureKind::Distance, &["edge@[3,15,10]", "edge@[57,15,10]"], Some(plane));
    assert!((d.value - 54.0).abs() < 1e-12, "{d:?}");
    // body of the part: 60·30·20 − 54·30·17
    let b = by_refs(&mut s, MeasureKind::Body, &["body@[0,15,10]"], None);
    assert!((value(&b, "volume") - 8460.0).abs() < 0.5, "{b:?}");
    assert!(b.details.iter().any(|x| x.starts_with("Bauteil k")), "{:?}", b.details);
    // anchor as a point
    let r = s.measure_model(root(&s)).unwrap();
    let pk = s.measure_ref(&r, None, "vertex@[60,30,20]", &parse_point).unwrap();
    assert!(pk.origin.iter().any(|x| x == "Anker k.corner"), "{:?}", pk.origin);
}

#[test]
fn angle_of_a_chamfer() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let mut s = session(&example("chamfers.mcad"));
    // chamfer(1) on the top edges of box(g[a, v, u], g[b, h, o]) (40 × 25 × 10)
    let a = by_refs(&mut s, MeasureKind::Angle, &["face@[39.5,12,9.5]", "face@[20,12,10]"], None);
    assert!((a.value - 45.0).abs() < 1e-9, "{a:?}");
    assert!((value(&a, "opening") - 135.0).abs() < 1e-9);
    assert!(!a.lines.is_empty());
    let a = by_refs(&mut s, MeasureKind::Angle, &["face@[20,12,10]", "face@[40,12,5]"], None);
    assert!((a.value - 90.0).abs() < 1e-9, "{a:?}");
}

#[test]
fn section_mode_over_the_protocol() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let src = "difference() {\n  cube([30, 20, 6]);\n  translate([10, 10, -1]) cylinder(h = 8, r = 3);\n}\n";
    let mut s = session(src);
    let sec = SectionPlaneRef { point: Vec3([0.0, 0.0, 3.0]), normal: Vec3([0.0, 0.0, 1.0]) };
    // hover on the bore contour (radius 3 around (10, 10)): the arc
    let (o, d) = down(13.0, 10.2);
    let a = over(&mut s, MeasureKind::Radius, vec![], o, d, false, Some(sec));
    let info = a.section.clone().expect("section info");
    assert_eq!(info.regions, 1);
    assert!(info.exact && (info.area - (600.0 - 9.0 * std::f64::consts::PI)).abs() < 1e-9, "{info:?}");
    let h = a.hover.expect("contour");
    assert_eq!(h.kind, "section_edge");
    let a = over(&mut s, MeasureKind::Radius, vec![], o, d, true, Some(sec));
    let r = a.result.expect("radius");
    assert!((r.value - 3.0).abs() < 1e-12, "{r:?}");
    assert_eq!(r.section, Some(sec));
    assert!(r.cli.starts_with(&["--section".to_string(), "z=3".to_string()]), "{:?}", r.cli);
    // centre snap of the bore
    let (o, d) = down(10.2, 10.1);
    let c = over(&mut s, MeasureKind::Distance, vec![], o, d, false, Some(sec)).hover.unwrap();
    assert_eq!(c.kind, "section_center");
    assert!(DVec3::from_array(c.point.0).distance(DVec3::new(10.0, 10.0, 3.0)) < 1e-9, "{:?}", c.point);
    // wall from the bore to the left side: 10 − 3
    let (o, d) = down(7.0, 10.0);
    let w = over(&mut s, MeasureKind::Wall, vec![], o, d, true, Some(sec)).result.expect("wall");
    assert!((w.value - 7.0).abs() < 1e-9, "{w:?}");
    // area: click into the region
    let (o, d) = down(25.0, 5.0);
    let a = over(&mut s, MeasureKind::Area, vec![], o, d, true, Some(sec)).result.expect("area");
    assert!((a.value - (600.0 - 9.0 * std::f64::consts::PI)).abs() < 1e-9);
    // wall without a section: message
    let (o, d) = down(25.0, 5.0);
    let a = over(&mut s, MeasureKind::Wall, vec![], o, d, true, None);
    assert!(a.message.is_some() && a.result.is_none());
}
