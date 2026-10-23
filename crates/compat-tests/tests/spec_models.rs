//! Stage 1 for the spec models in `examples/spec/`: they use the new language features
//! (grids, anchors and named instances, `rotate(to = …)`, `fillet`/`chamfer` at Booleans
//! and as zone blocks with `select`, `$fillet`/`$chamfer`, `loft`, `sweep`, `shell`).
//! Every file evaluates without errors or warnings into 3D geometry, and has the expected
//! grids, instances with anchors, handles and operations.

mod common;

use cad_eval::EvalOutput;
use cad_ir::{BlendKind, Op, Select, TransformKind, ZoneShape};
use cad_kernel::PathInterp;
use cad_syntax::Severity;
use common::*;
use glam::DVec3;

struct Expect {
    file: &'static str,
    /// Grid name, number of planes (including implicit `_`), declared in the file context.
    grids: &'static [(&'static str, usize, bool)],
    /// Instance label, module, anchor names.
    instances: &'static [(&'static str, &'static str, &'static [&'static str])],
    /// Handles of the file context.
    handles: usize,
    /// Node label → count (only the listed labels are checked).
    ops: &'static [(&'static str, usize)],
    check: fn(&EvalOutput),
}

const EXPECT: &[Expect] = &[
    Expect {
        file: "dose_shell.scad",
        grids: &[("g", 6, false)],
        instances: &[("k", "dose", &["rand"])],
        handles: 0,
        ops: &[("shell", 1), ("zone", 1), ("box", 2)],
        check: |out| {
            assert!(has_op(out, |o| *o == Op::Shell { t: 1.5, select: Select::Inside }));
            assert!(has_op(out, |o| matches!(o, Op::Zone(ZoneShape::Plane { point, normal, d })
                if *point == DVec3::new(0.0, 0.0, 30.0) && *normal == DVec3::Z && *d == 1.0)));
            assert_eq!(blends(out), [(BlendKind::Fillet, 4.0), (BlendKind::Fillet, 0.8)]);
            let lid = nodes_with(out, "box")[1];
            assert_close(out.tree.node(lid).world.transform_point3(DVec3::ZERO), [0.0, 0.0, 35.0]);
        },
    },
    Expect {
        file: "fase_chamfer.scad",
        grids: &[("g", 6, true)],
        instances: &[],
        handles: 17,
        ops: &[("chamfer", 1), ("zone", 1), ("union", 1), ("box", 4), ("cylinder", 1)],
        check: |out| {
            assert!(has_op(out, |o| *o == Op::Blend { kind: BlendKind::Chamfer, r: 1.0, select: Select::Inside }));
            assert!(has_op(out, |o| matches!(o, Op::Boolean { chamfer: Some(c), fillet: None, .. } if *c == 0.5)));
            assert_eq!(blends(out), [(BlendKind::Chamfer, 1.0), (BlendKind::Fillet, 1.0)]);
            // All grid planes are literals: one draggable handle each.
            let planes = out.handles.iter().filter(|h| matches!(h.kind, cad_eval::HandleKind::Plane { .. }));
            assert_eq!(planes.filter(|h| h.comps.iter().all(|c| c.is_some())).count(), 6);
        },
    },
    Expect {
        file: "fillet_zonen.scad",
        grids: &[("g", 6, false)],
        instances: &[],
        handles: 0,
        ops: &[("fillet", 3), ("zone", 3), ("difference", 1), ("sphere", 1)],
        check: |out| {
            let b: Vec<(f64, Select)> = out
                .tree
                .nodes
                .iter()
                .filter_map(|n| match n.op {
                    Op::Blend { kind: BlendKind::Fillet, r, select } => Some((r, select)),
                    _ => None,
                })
                .collect();
            assert_eq!(b, [(1.0, Select::Inside), (2.0, Select::Inside), (1.5, Select::Touch)]);
            let zones: Vec<&ZoneShape> =
                out.tree.nodes.iter().filter_map(|n| if let Op::Zone(z) = &n.op { Some(z) } else { None }).collect();
            assert_eq!(zones[0], &ZoneShape::Line { point: DVec3::new(30.0, 0.0, 15.0), dir: DVec3::Y, d: 2.0 });
            assert_eq!(zones[1], &ZoneShape::Plane { point: DVec3::ZERO, normal: DVec3::Z, d: 1.0 });
            assert_eq!(zones[2], &ZoneShape::Line { point: DVec3::ZERO, dir: DVec3::Z, d: 1.0 });
            // The sphere is a zone (not part of the result), the outer block's body is the inner block.
            let s = nodes_with(out, "sphere")[0];
            let zone_parent = out.tree.node(s).parent.unwrap();
            assert!(out.tree.node(zone_parent).flags.zone);
            let outer = nodes_with(out, "fillet")[0];
            let body: Vec<u32> = out.tree.solid_children(outer).collect();
            assert_eq!(body.len(), 1);
            assert_eq!(out.tree.node(body[0]).op.label(), "fillet");
        },
    },
    Expect {
        file: "flasche.scad",
        grids: &[("g", 8, false), ("g", 8, false)],
        instances: &[],
        handles: 1,
        ops: &[("rotate_extrude", 2), ("fillet", 2), ("polygon", 2), ("cylinder", 4)],
        check: |out| {
            let angles: Vec<f64> = out
                .tree
                .nodes
                .iter()
                .filter_map(|n| if let Op::RotateExtrude { angle } = n.op { Some(angle) } else { None })
                .collect();
            assert_eq!(angles, [360.0, 270.0]);
            let f: Vec<f64> =
                out.tree.nodes.iter().filter_map(|n| if let Op::Blend { r, .. } = n.op { Some(r) } else { None }).collect();
            assert_eq!(f, [3.0, 3.0]);
        },
    },
    Expect {
        file: "freiformdeckel.scad",
        grids: &[("g", 9, false)],
        instances: &[],
        handles: 0,
        ops: &[("surface", 1), ("thicken", 1), ("box", 1)],
        check: |out| {
            let rows = out
                .tree
                .nodes
                .iter()
                .find_map(|n| if let Op::Surface { rows } = &n.op { Some(rows.clone()) } else { None })
                .unwrap();
            assert_eq!((rows.len(), rows[0].len()), (4, 4));
            assert_close(rows[1][1], [20.0, 13.0, 9.0]);
            assert!(has_op(out, |o| *o == Op::Thicken { t: 2.0 }));
        },
    },
    Expect {
        file: "gehaeuse.scad",
        grids: &[("g", 8, false), ("h", 3, true)],
        instances: &[("k", "gehaeuse", &["ecke", "oben"])],
        handles: 3,
        ops: &[("difference", 1), ("box", 4), ("deckel", 2)],
        check: |out| {
            let k = instance(out, "k");
            assert_close(k.anchors[0].point, [60.0, 30.0, 20.0]);
            assert_eq!(k.anchors[1].dir, Some(DVec3::Z));
            let h = out.grid_by_name("h").unwrap();
            assert_eq!(h.planes.iter().find(|p| p.name == "rand").unwrap().value, 60.0);
            let lid = nodes_with(out, "box")[2];
            assert_close(out.tree.node(lid).world.transform_point3(DVec3::ZERO), [60.0, 30.0, 20.0]);
        },
    },
    Expect {
        file: "griffschale.scad",
        grids: &[("g", 9, false)],
        instances: &[("s", "griffschale", &["deckel"])],
        handles: 0,
        ops: &[("shell", 1), ("zone", 1), ("difference", 3), ("cylinder", 4)],
        check: |out| {
            assert!(has_op(out, |o| *o == Op::Shell { t: 1.6, select: Select::Inside }));
            assert_eq!(blends(out), [(BlendKind::Chamfer, 4.0)]);
            let f = out.tree.nodes.iter().filter(|n| matches!(n.op, Op::Boolean { fillet: Some(f), .. } if f == 0.5));
            assert_eq!(f.count(), 2);
            let g = out.grid_by_name("g").unwrap();
            let dom: Vec<f64> = g.planes.iter().filter(|p| p.base == "dom").map(|p| p.value).collect();
            assert_eq!(dom, [18.0, 72.0]);
        },
    },
    Expect {
        file: "kabelfuehrung.scad",
        grids: &[("g", 7, true)],
        instances: &[],
        handles: 17,
        ops: &[("sweep", 2), ("polygon", 2), ("box", 2)],
        check: |out| {
            let sweeps: Vec<(&Vec<DVec3>, PathInterp)> = out
                .tree
                .nodes
                .iter()
                .filter_map(|n| match &n.op {
                    Op::Sweep { path, interp } => Some((path, *interp)),
                    _ => None,
                })
                .collect();
            assert_eq!(sweeps[0].1, PathInterp::Spline);
            assert_eq!(sweeps[0].0.len(), 4);
            assert_close(sweeps[0].0[2], [40.0, 25.0, 20.0]);
            assert_eq!(sweeps[1].1, PathInterp::Linear);
        },
    },
    Expect {
        file: "knauf_mirror.scad",
        grids: &[("s", 3, true), ("p", 7, false), ("p", 7, false)],
        instances: &[("k", "knauf", &["oben"]), ("", "knauf", &["oben"])],
        handles: 5,
        ops: &[("rotate_extrude", 2), ("spline", 2), ("torus", 2), ("mirror", 1)],
        check: |out| {
            assert!(
                out.tree
                    .nodes
                    .iter()
                    .filter(|n| matches!(n.op, Op::Boolean { fillet: Some(f), .. } if f == 0.5))
                    .count()
                    == 2
            );
            // mirror(s.spiegel): plane x = 20 of a grid scaled by 2 → x = 40.
            let m = out
                .tree
                .nodes
                .iter()
                .find_map(|n| match n.op {
                    Op::Transform { kind: TransformKind::Mirror, m } => Some(m),
                    _ => None,
                })
                .unwrap();
            assert_close(m.transform_point3(DVec3::new(0.0, 1.0, 2.0)), [80.0, 1.0, 2.0]);
            let p = out.grids.iter().find(|g| g.name == "p").unwrap();
            assert_eq!(p.planes.iter().find(|p| p.name == "taille").unwrap().value, 9.0);
        },
    },
    Expect {
        file: "lochplatte.scad",
        grids: &[("g", 11, false), ("l", 5, false)],
        instances: &[("p", "lochplatte", &["erstes_loch", "lasche"])],
        handles: 3,
        ops: &[("difference", 1), ("cylinder", 5), ("box", 2)],
        check: |out| {
            assert!(has_op(out, |o| matches!(o, Op::Boolean { fillet: Some(f), .. } if *f == 1.0)));
            let g = out.grid_by_name("g").unwrap();
            let loch: Vec<f64> = g.planes.iter().filter(|p| p.base == "loch").map(|p| p.value).collect();
            assert_eq!(loch, [6.0, 18.0, 30.0, 42.0]);
            let p = instance(out, "p");
            assert_close(p.anchors[1].point, [58.0, 12.0, 0.0]);
            // Holes at the grid points.
            let xs: Vec<f64> =
                nodes_with(out, "cylinder")[..4].iter().map(|&c| out.tree.node(c).world.w_axis.x).collect();
            assert_eq!(xs, [6.0, 18.0, 30.0, 42.0]);
        },
    },
    Expect {
        file: "loft_sweep.scad",
        grids: &[("g", 6, true)],
        instances: &[],
        handles: 21,
        ops: &[("loft", 1), ("sweep", 2), ("circle", 3), ("rect", 2)],
        check: |out| {
            let sweeps: Vec<(&Vec<DVec3>, PathInterp)> = out
                .tree
                .nodes
                .iter()
                .filter_map(|n| match &n.op {
                    Op::Sweep { path, interp } => Some((path, *interp)),
                    _ => None,
                })
                .collect();
            assert_eq!(sweeps[0].1, PathInterp::Spline);
            assert_eq!(sweeps[0].0, &vec![DVec3::ZERO, DVec3::new(20.0, 0.0, 15.0), DVec3::new(40.0, 0.0, 0.0)]);
            assert_eq!(sweeps[1].1, PathInterp::Linear);
            let loft = nodes_with(out, "loft")[0];
            assert_eq!(out.tree.solid_children(loft).count(), 3);
        },
    },
    Expect {
        file: "lueftungsgitter.scad",
        grids: &[("g", 15, false)],
        instances: &[("", "gitter", &["mitte"])],
        handles: 0,
        ops: &[("linear_extrude", 10), ("difference", 1), ("rect", 10)],
        check: |out| {
            assert!(has_op(out, |o| matches!(o, Op::Boolean { chamfer: Some(c), fillet: None, .. } if *c == 0.6)));
            let g = out.grid_by_name("g").unwrap();
            let s: Vec<f64> = g.planes.iter().filter(|p| p.base == "s").map(|p| p.value).collect();
            assert_eq!(s.len(), 7);
            assert_eq!((s[0], s[6]), (10.0, 70.0));
        },
    },
    Expect {
        file: "winkel_anker.scad",
        grids: &[("g", 10, false), ("g", 10, false), ("m", 4, true)],
        instances: &[
            ("a", "winkel", &["boden_loch", "wand_loch", "spitze"]),
            ("b", "winkel", &["boden_loch", "wand_loch", "spitze"]),
        ],
        handles: 12,
        ops: &[("schraube", 3), ("union", 2)],
        check: |out| {
            // Anchors of b in file coordinates: translate([0, 40, 0]) rotate([0, 0, 90]).
            let b = instance(out, "b");
            assert_close(b.matrix.transform_point3(b.anchors[1].point), [-10.0, 43.0, 32.0]);
            assert_close(b.matrix.transform_vector3(b.anchors[1].dir.unwrap()), [0.0, 1.0, 0.0]);
            let m = out.grid_by_name("m").unwrap();
            let v: Vec<(&str, f64)> = m.planes.iter().map(|p| (p.name.as_str(), p.value)).collect();
            assert_eq!(&v[..3], [("bis", -20.0), ("von", 30.0), ("y0", 20.0)]);
            // The screw at b.wand_loch points along the anchor direction (+y in file coordinates).
            let screws = nodes_with(out, "schraube");
            let w = out.tree.node(screws[2]).world;
            assert_close(w.transform_point3(DVec3::ZERO), [-10.0, 43.0, 32.0]);
            assert_close(w.transform_vector3(DVec3::Z), [0.0, 1.0, 0.0]);
        },
    },
];

fn has_op(out: &EvalOutput, f: impl Fn(&Op) -> bool) -> bool {
    out.tree.nodes.iter().any(|n| f(&n.op))
}

fn nodes_with(out: &EvalOutput, label: &str) -> Vec<u32> {
    (0..out.tree.nodes.len() as u32).filter(|&i| out.tree.node(i).op.label() == label).collect()
}

fn blends(out: &EvalOutput) -> Vec<(BlendKind, f64)> {
    out.tree.nodes.iter().filter_map(|n| n.blend).collect()
}

fn instance<'a>(out: &'a EvalOutput, label: &str) -> &'a cad_eval::InstanceInfo {
    out.instances.iter().find(|i| i.label.as_deref() == Some(label)).unwrap_or_else(|| panic!("no instance {label}"))
}

#[track_caller]
fn assert_close(v: DVec3, e: [f64; 3]) {
    assert!((v - DVec3::from(e)).abs().max_element() < 1e-9, "{v:?} != {e:?}");
}

#[test]
fn spec_models_stage1() {
    let files = scad_files("examples/spec");
    let names: Vec<String> = files.iter().map(|f| file_name(f)).collect();
    let expected: Vec<&str> = EXPECT.iter().map(|e| e.file).collect();
    assert_eq!(names, expected, "every model in examples/spec needs an entry in EXPECT");

    for (path, e) in files.iter().zip(EXPECT) {
        let src = std::fs::read_to_string(path).unwrap();
        assert!(src.starts_with("// Zweck:"), "{}: missing header comment", e.file);
        let out = eval(&src);
        let problems = stage1_problems(&src, &out);
        assert!(problems.is_empty(), "{}: {problems:#?}", e.file);
        let warnings = diags(&src, &out, Severity::Warning);
        assert!(warnings.is_empty(), "{}: unexpected warnings {warnings:#?}", e.file);

        let grids: Vec<(&str, usize, bool)> =
            out.grids.iter().map(|g| (g.name.as_str(), g.planes.len(), g.in_context)).collect();
        assert_eq!(grids, e.grids, "{}: grids", e.file);

        let named: Vec<(&str, &str, Vec<&str>)> = out.instances[1..]
            .iter()
            .filter(|i| !i.anchors.is_empty() || i.label.is_some())
            .map(|i| {
                (
                    i.label.as_deref().unwrap_or(""),
                    i.module.as_str(),
                    i.anchors.iter().map(|a| a.name.as_str()).collect(),
                )
            })
            .collect();
        let want: Vec<(&str, &str, Vec<&str>)> = e.instances.iter().map(|(l, m, a)| (*l, *m, a.to_vec())).collect();
        assert_eq!(named, want, "{}: instances with anchors", e.file);

        assert_eq!(
            out.handles.len(),
            e.handles,
            "{}: handles {:#?}",
            e.file,
            out.handles.iter().map(|h| h.kind).collect::<Vec<_>>()
        );
        for (label, n) in e.ops {
            assert_eq!(nodes_with(&out, label).len(), *n, "{}: number of `{label}` nodes", e.file);
        }
        (e.check)(&out);
        println!(
            "{:24} ok  grids={} anchors={} handles={}",
            e.file,
            out.grids.len(),
            out.instances.iter().map(|i| i.anchors.len()).sum::<usize>(),
            out.handles.len()
        );
    }
}
