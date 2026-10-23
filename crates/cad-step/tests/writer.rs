//! STEP writer: syntax, references and numbers read back with the crate's own Part 21 reader
//! (no Python, no kernel).

use cad_kernel::brep::*;
use cad_step::{Schema, StepOptions, p21, write};
use glam::DVec3;

/// Axis-parallel box `[0,a]x[0,b]x[0,c]` as a neutral B-rep (planes, lines).
fn box_brep(a: f64, b: f64, c: f64) -> Brep {
    let v = |i: u32| DVec3::new(if i & 1 != 0 { a } else { 0.0 }, if i & 2 != 0 { b } else { 0.0 }, if i & 4 != 0 { c } else { 0.0 });
    let vertices: Vec<DVec3> = (0..8).map(v).collect();
    let mut edges: Vec<Edge> = Vec::new();
    let mut edge = |s: u32, e: u32| -> OrientedEdge {
        if let Some(i) = edges.iter().position(|x| x.start == s && x.end == e) {
            return OrientedEdge { edge: i as u32, forward: true };
        }
        if let Some(i) = edges.iter().position(|x| x.start == e && x.end == s) {
            return OrientedEdge { edge: i as u32, forward: false };
        }
        let (p, q) = (vertices[s as usize], vertices[e as usize]);
        edges.push(Edge { start: s, end: e, curve: Curve::Line { origin: p, dir: (q - p).normalize() }, approximated: false });
        OrientedEdge { edge: edges.len() as u32 - 1, forward: true }
    };
    // faces: vertex cycles counter-clockwise seen from outside, outward normal
    let quads: [([u32; 4], DVec3); 6] = [
        ([0, 2, 3, 1], -DVec3::Z),
        ([4, 5, 7, 6], DVec3::Z),
        ([0, 1, 5, 4], -DVec3::Y),
        ([2, 6, 7, 3], DVec3::Y),
        ([0, 4, 6, 2], -DVec3::X),
        ([1, 3, 7, 5], DVec3::X),
    ];
    let mut faces = Vec::new();
    for (k, (q, n)) in quads.iter().enumerate() {
        let es: Vec<OrientedEdge> = (0..4).map(|i| edge(q[i], q[(i + 1) % 4])).collect();
        let p0 = vertices[q[0] as usize];
        let x = (vertices[q[1] as usize] - p0).normalize();
        faces.push(Face {
            id: k as u32,
            tag: 1,
            surface: Surface::Plane { frame: Frame::new(p0, *n, x).unwrap() },
            same_sense: true,
            loops: vec![Loop::Edges(es)],
            approximated: false,
        });
    }
    let b = Brep { vertices, edges, bodies: vec![Body::Solid { outer: Shell { faces }, voids: vec![] }] };
    b.validate().unwrap();
    b
}

fn opts() -> StepOptions {
    StepOptions { name: "Würfel 'A'".into(), ..Default::default() }
}

#[test]
fn box_syntax_and_counts() {
    let b = box_brep(10.0, 20.0, 30.0);
    let (text, st) = write(&[&b], &opts()).unwrap();
    let m = p21::parse(&text).unwrap();
    assert!(m.schema.starts_with("AP242_MANAGED_MODEL_BASED_3D_ENGINEERING_MIM_LF"), "{}", m.schema);
    for (ty, n) in [
        ("MANIFOLD_SOLID_BREP", 1),
        ("CLOSED_SHELL", 1),
        ("ADVANCED_FACE", 6),
        ("FACE_BOUND", 6),
        ("EDGE_LOOP", 6),
        ("ORIENTED_EDGE", 24),
        ("EDGE_CURVE", 12),
        ("VERTEX_POINT", 8),
        ("LINE", 12),
        ("PLANE", 6),
        ("PRODUCT", 1),
        ("SHAPE_DEFINITION_REPRESENTATION", 1),
        ("ADVANCED_BREP_SHAPE_REPRESENTATION", 1),
        ("CONVERSION_BASED_UNIT", 1),
    ] {
        assert_eq!(m.count(ty), n, "{ty}");
    }
    assert_eq!((st.solids, st.faces, st.edges, st.vertices), (1, 6, 12, 8));
    assert_eq!(st.entities, m.entities.len());
    // product name with umlaut and quote survives the encoding
    assert!(text.contains("PRODUCT('W\\X2\\00FC\\X0\\rfel ''A''','W\\X2\\00FC\\X0\\rfel ''A'''"));
    // every edge's vertices are the box corners
    for e in m.of_type("EDGE_CURVE") {
        for v in &e.args()[1..3] {
            let vp = m.get(v.as_ref_id().unwrap()).unwrap();
            let c = m.coords(vp.args()[1].as_ref_id().unwrap()).unwrap();
            assert!(c.iter().zip([10.0, 20.0, 30.0]).all(|(x, l)| *x == 0.0 || *x == l), "{c:?}");
        }
    }
    // the solid's shell lists all six faces, each with a plane
    let solid = m.of_type("MANIFOLD_SOLID_BREP").next().unwrap();
    let shell = m.get(solid.args()[1].as_ref_id().unwrap()).unwrap();
    let faces = shell.args()[1].as_list().unwrap();
    assert_eq!(faces.len(), 6);
    for f in faces {
        let f = m.get(f.as_ref_id().unwrap()).unwrap();
        assert_eq!(f.ty(), "ADVANCED_FACE");
        assert_eq!(m.get(f.args()[2].as_ref_id().unwrap()).unwrap().ty(), "PLANE");
        assert_eq!(f.args()[3].as_enum(), Some("T"));
    }
}

#[test]
fn ap214_header() {
    let b = box_brep(1.0, 1.0, 1.0);
    let (text, _) = write(&[&b], &StepOptions { schema: Schema::Ap214, ..opts() }).unwrap();
    let m = p21::parse(&text).unwrap();
    assert_eq!(m.schema, "AUTOMOTIVE_DESIGN { 1 0 10303 214 1 1 1 1 }");
    assert_eq!(m.count("APPLICATION_PROTOCOL_DEFINITION"), 1);
}

fn expand(knots: &[p21::Value], mults: &[p21::Value]) -> Vec<f64> {
    let mut out = Vec::new();
    for (k, m) in knots.iter().zip(mults) {
        for _ in 0..m.as_num().unwrap() as usize {
            out.push(k.as_num().unwrap());
        }
    }
    out
}

#[test]
fn analytic_and_rational_geometry_round_trip() {
    // a box whose top face carries a rational B-spline surface and one edge a rational curve,
    // a void (a smaller box, written reversed) and a second solid with a cylinder face
    let mut b = box_brep(10.0, 10.0, 10.0);
    let s = std::f64::consts::FRAC_1_SQRT_2;
    let nurbs = BSplineSurface {
        degree_u: 2,
        degree_v: 1,
        knots_u: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        knots_v: vec![-1.0, -1.0, 2.0, 2.0],
        nu: 3,
        nv: 2,
        points: (0..6).map(|i| DVec3::new(i as f64, 0.5 * i as f64, 10.0)).collect(),
        weights: Some(vec![1.0, 1.0, s, s, 1.0, 1.0]),
    };
    let curve = BSplineCurve {
        degree: 2,
        knots: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        points: vec![DVec3::new(0.0, 0.0, 0.0), DVec3::new(5.0, -1.0, 0.0), DVec3::new(10.0, 0.0, 0.0)],
        weights: Some(vec![1.0, s, 1.0]),
    };
    let Body::Solid { outer, .. } = &mut b.bodies[0] else { unreachable!() };
    outer.faces[1].surface = Surface::BSpline(nurbs.clone());
    let target = b
        .edges
        .iter()
        .position(|e| {
            let (p, q) = (b.vertices[e.start as usize], b.vertices[e.end as usize]);
            p.y == 0.0 && q.y == 0.0 && p.z == 0.0 && q.z == 0.0
        })
        .unwrap();
    let mut curve = curve;
    if b.vertices[b.edges[target].start as usize].x > 0.0 {
        curve.points.reverse();
    }
    b.edges[target].curve = Curve::BSpline(curve.clone());
    let mut void = box_brep(2.0, 2.0, 2.0);
    void.transform(&glam::DMat4::from_translation(DVec3::splat(4.0)));
    // void faces must point into the void: reverse the small box
    let Body::Solid { outer: vo, .. } = &mut void.bodies[0] else { unreachable!() };
    for f in &mut vo.faces {
        f.same_sense = !f.same_sense;
        for l in &mut f.loops {
            if let Loop::Edges(es) = l {
                es.reverse();
                es.iter_mut().for_each(|oe| oe.forward = !oe.forward);
            }
        }
    }
    let Body::Solid { outer: vshell, .. } = void.bodies.remove(0) else { unreachable!() };
    let base = b.vertices.len() as u32;
    let ebase = b.edges.len() as u32;
    b.vertices.extend(void.vertices);
    b.edges.extend(void.edges.into_iter().map(|mut e| {
        e.start += base;
        e.end += base;
        e
    }));
    let mut vshell = vshell;
    for f in &mut vshell.faces {
        for l in &mut f.loops {
            if let Loop::Edges(es) = l {
                es.iter_mut().for_each(|oe| oe.edge += ebase);
            }
        }
    }
    let Body::Solid { voids, .. } = &mut b.bodies[0] else { unreachable!() };
    voids.push(vshell);
    // second solid: replace one face of another box by a cylinder (geometry only)
    let mut c = box_brep(1.0, 1.0, 1.0);
    let Body::Solid { outer: co, .. } = &mut c.bodies[0] else { unreachable!() };
    let frame = Frame::new(DVec3::new(1.0, 2.0, 3.0), DVec3::new(0.0, 0.0, 2.0), DVec3::new(1.0, 1.0, 0.0)).unwrap();
    co.faces[2].surface = Surface::Cylinder { frame, r: 2.5 };
    co.faces[3].surface = Surface::Cone { frame, r: 1.5, semi_angle: 30f64.to_radians() };
    co.faces[4].surface = Surface::Torus { frame, major: 7.0, minor: 0.25 };

    let (text, st) = write(&[&b, &c], &StepOptions { colors: vec![Some([1.0, 0.5, 0.0]), None], ..opts() }).unwrap();
    assert_eq!((st.solids, st.voids), (2, 1));
    let m = p21::parse(&text).unwrap();
    assert_eq!(m.count("BREP_WITH_VOIDS"), 1);
    assert_eq!(m.count("MANIFOLD_SOLID_BREP"), 1);
    let ocs = m.of_type("ORIENTED_CLOSED_SHELL").next().unwrap();
    assert_eq!(ocs.args()[3].as_enum(), Some("F"));
    assert_eq!(m.count("STYLED_ITEM"), 1);
    assert_eq!(m.count("COLOUR_RGB"), 1);

    // rational surface: degrees, knots (expanded), weights and control points read back
    let rs = m.of_type("RATIONAL_B_SPLINE_SURFACE").next().unwrap();
    let bs = rs.part("B_SPLINE_SURFACE").unwrap();
    assert_eq!((bs[0].as_num(), bs[1].as_num()), (Some(2.0), Some(1.0)));
    let net = bs[2].as_list().unwrap();
    assert_eq!(net.len(), 3);
    let mut k = 0;
    for row in net {
        for p in row.as_list().unwrap() {
            let xyz = m.coords(p.as_ref_id().unwrap()).unwrap();
            assert_eq!(DVec3::from(xyz), nurbs.points[k]);
            k += 1;
        }
    }
    let kn = rs.part("B_SPLINE_SURFACE_WITH_KNOTS").unwrap();
    assert_eq!(expand(kn[2].as_list().unwrap(), kn[0].as_list().unwrap()), nurbs.knots_u);
    assert_eq!(expand(kn[3].as_list().unwrap(), kn[1].as_list().unwrap()), nurbs.knots_v);
    let w: Vec<f64> = rs.part("RATIONAL_B_SPLINE_SURFACE").unwrap()[0]
        .as_list()
        .unwrap()
        .iter()
        .flat_map(|r| r.as_list().unwrap().iter().map(|x| x.as_num().unwrap()).collect::<Vec<_>>())
        .collect();
    assert_eq!(w, nurbs.weights.clone().unwrap());

    // rational curve
    let rc = m.of_type("RATIONAL_B_SPLINE_CURVE").next().unwrap();
    let w: Vec<f64> = rc.part("RATIONAL_B_SPLINE_CURVE").unwrap()[0].as_list().unwrap().iter().map(|x| x.as_num().unwrap()).collect();
    assert_eq!(w, vec![1.0, s, 1.0]);
    let kc = rc.part("B_SPLINE_CURVE_WITH_KNOTS").unwrap();
    assert_eq!(expand(kc[1].as_list().unwrap(), kc[0].as_list().unwrap()), curve.knots);

    // analytic parameters: radius, semi-angle in degrees, torus radii, placement
    let cyl = m.of_type("CYLINDRICAL_SURFACE").next().unwrap();
    assert_eq!(cyl.args()[2].as_num(), Some(2.5));
    let ax = m.get(cyl.args()[1].as_ref_id().unwrap()).unwrap();
    assert_eq!(m.coords(ax.args()[1].as_ref_id().unwrap()), Some([1.0, 2.0, 3.0]));
    assert_eq!(m.coords(ax.args()[2].as_ref_id().unwrap()), Some([0.0, 0.0, 1.0]));
    let xd = m.coords(ax.args()[3].as_ref_id().unwrap()).unwrap();
    assert!((xd[0] - s).abs() < 1e-15 && (xd[1] - s).abs() < 1e-15);
    let cone = m.of_type("CONICAL_SURFACE").next().unwrap();
    assert_eq!(cone.args()[2].as_num(), Some(1.5));
    assert!((cone.args()[3].as_num().unwrap() - 30.0).abs() < 1e-12);
    let tor = m.of_type("TOROIDAL_SURFACE").next().unwrap();
    assert_eq!((tor.args()[2].as_num(), tor.args()[3].as_num()), (Some(7.0), Some(0.25)));
}

#[test]
fn sheet_bodies_use_surface_models() {
    let b = box_brep(1.0, 1.0, 1.0);
    let Body::Solid { outer, .. } = b.bodies[0].clone() else { unreachable!() };
    let sheet = Brep { bodies: vec![Body::Sheet { shells: vec![outer] }], ..b };
    let (text, st) = write(&[&sheet], &opts()).unwrap();
    assert_eq!((st.solids, st.sheets), (0, 1));
    let m = p21::parse(&text).unwrap();
    assert_eq!(m.count("MANIFOLD_SURFACE_SHAPE_REPRESENTATION"), 1);
    assert_eq!(m.count("SHELL_BASED_SURFACE_MODEL"), 1);
    assert_eq!(m.count("OPEN_SHELL"), 1);
}

#[test]
fn rejects_empty_and_broken_input() {
    assert!(write(&[&Brep::default()], &opts()).is_err());
    let mut b = box_brep(1.0, 1.0, 1.0);
    b.edges[0].end = 99;
    assert!(write(&[&b], &opts()).is_err());
    let mut b = box_brep(1.0, 1.0, 1.0);
    b.vertices[0].x = f64::NAN;
    assert!(write(&[&b], &opts()).is_err());
}

#[test]
fn parser_rejects_dangling_references() {
    let b = box_brep(1.0, 1.0, 1.0);
    let (text, _) = write(&[&b], &opts()).unwrap();
    let broken = text.replacen("EDGE_CURVE('',#", "EDGE_CURVE('',#9999", 1);
    assert!(p21::parse(&broken).unwrap_err().contains("undefined"));
}
