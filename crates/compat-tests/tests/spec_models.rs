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
        file: "asserts.mcad",
        grids: &[],
        instances: &[("g", "gehaeuse", &[]), ("hl", "halter", &[]), ("kf", "kabelfuehrung", &[]), ("k", "deckel", &[])],
        handles: 1,
        ops: &[("mesh", 1), ("difference", 1)],
        check: |out| {
            // nested assertions, the keep-out volume from an STL mesh is not part of the result
            let kinds: Vec<cad_ir::AssertKind> = out.tree.asserts.iter().map(|a| a.kind).collect();
            use cad_ir::AssertKind::*;
            assert_eq!(kinds, [Touches, Intersect, Disjoint, Keepout]);
            let k = &out.tree.asserts[3];
            assert_eq!(k.clearance, 1.0);
            assert!(out.tree.node(k.node).flags.keepout);
            assert_eq!(out.parts.len(), 4);
        },
    },
    Expect {
        file: "dose_shell.mcad",
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
        file: "fase_chamfer.mcad",
        grids: &[("g", 6, true)],
        instances: &[],
        handles: 17,
        ops: &[("chamfer", 1), ("zone", 1), ("union", 1), ("box", 4), ("cylinder", 1)],
        check: |out| {
            assert!(has_op(out, |o| *o
                == Op::Blend { kind: BlendKind::Chamfer, r: 1.0, select: Select::Inside, law: None }));
            assert!(has_op(out, |o| matches!(o, Op::Boolean { chamfer: Some(c), fillet: None, .. } if *c == 0.5)));
            assert_eq!(blends(out), [(BlendKind::Chamfer, 1.0), (BlendKind::Fillet, 1.0)]);
            // All grid planes are literals: one draggable handle each.
            let planes = out.handles.iter().filter(|h| matches!(h.kind, cad_eval::HandleKind::Plane { .. }));
            assert_eq!(planes.filter(|h| h.comps.iter().all(|c| c.is_some())).count(), 6);
        },
    },
    Expect {
        file: "fillet_zonen.mcad",
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
                    Op::Blend { kind: BlendKind::Fillet, r, select, .. } => Some((r, select)),
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
        file: "flasche.mcad",
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
            let f: Vec<f64> = out
                .tree
                .nodes
                .iter()
                .filter_map(|n| if let Op::Blend { r, .. } = n.op { Some(r) } else { None })
                .collect();
            assert_eq!(f, [3.0, 3.0]);
        },
    },
    Expect {
        file: "freiformdeckel.mcad",
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
        file: "gehaeuse.mcad",
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
        file: "griffschale.mcad",
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
        file: "hull_minkowski.mcad",
        grids: &[],
        instances: &[],
        handles: 41,
        ops: &[("hull", 6), ("minkowski", 2), ("sphere", 11), ("cylinder", 2)],
        check: |out| {
            // the operations are nodes of their own, without warnings of the evaluation
            assert!(has_op(out, |o| *o == Op::Hull));
            assert!(has_op(out, |o| *o == Op::Minkowski));
        },
    },
    Expect {
        file: "kabelfuehrung.mcad",
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
        file: "knauf_mirror.mcad",
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
        file: "lochplatte.mcad",
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
        file: "loft_sweep.mcad",
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
        file: "lueftungsgitter.mcad",
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
        file: "mehrfarbig.mcad",
        grids: &[("g", 8, false), ("p", 9, false)],
        instances: &[("schild", "schild", &[]), ("figur", "figur", &[])],
        handles: 0,
        ops: &[("color", 6), ("difference", 2), ("rotate_extrude", 1)],
        check: |out| {
            // colours from the palette, one explicit filament slot (red on 3)
            let colors: Vec<(Option<&str>, Option<u32>)> = out
                .tree
                .nodes
                .iter()
                .filter_map(|n| match &n.op {
                    Op::Color { name, filament, .. } => Some((name.as_deref(), *filament)),
                    _ => None,
                })
                .collect();
            assert_eq!(
                colors,
                [
                    (Some("white"), None),
                    (Some("teal"), None),
                    (Some("red"), Some(3)),
                    (Some("blue"), None),
                    (Some("white"), None),
                    (Some("yellow"), None)
                ]
            );
        },
    },
    Expect {
        file: "schablone.mcad",
        grids: &[],
        instances: &[],
        handles: 4,
        ops: &[("linear_extrude", 3), ("difference", 1), ("offset", 2)],
        check: |out| {
            // the default font is the stencil font: glyphs of lines and arcs without holes,
            // "LACK 08" 6 glyphs and "RAL 9005 B" 8 glyphs (no glyph for spaces)
            let glyphs: Vec<&Vec<cad_ir::Contour>> = out
                .tree
                .nodes
                .iter()
                .filter_map(|n| match &n.op {
                    Op::Profile(cad_ir::Profile::Sketch { contours, .. }) => Some(contours),
                    _ => None,
                })
                .collect();
            assert_eq!(glyphs.len(), 6 + 8);
            for c in glyphs.iter().flat_map(|g| g.iter()) {
                assert!(c.segs.iter().all(|s| matches!(s.kind(), "line" | "arc")), "lines and arcs");
                let pts: Vec<_> = c.segs.iter().flat_map(|s| s.to_kernel().unwrap().sample(1e-3)).collect();
                let a: f64 = (0..pts.len()).map(|i| pts[i].perp_dot(pts[(i + 1) % pts.len()])).sum();
                assert!(a > 0.0, "outer contours only (no holes)");
            }
        },
    },
    Expect {
        file: "skizze_bibliothek.mcad",
        grids: &[],
        instances: &[],
        handles: 10,
        ops: &[("sketch", 2), ("linear_extrude", 2), ("lochplatte", 1), ("schraube", 4)],
        check: |out| {
            // qualified calls: the instances know their module (not the namespace)
            let mods: Vec<&str> = out.instances[1..].iter().map(|i| i.module.as_str()).collect();
            assert_eq!(mods, ["stern", "langloch", "lochplatte", "schraube", "schraube", "schraube", "schraube"]);
            // `raster` of the enclosing module with its default: a 40 × 10 plate
            let plate = out.tree.nodes.iter().find_map(|n| match &n.op {
                Op::Prim { prim: cad_ir::Prim::Box { max, .. }, .. } => Some(*max),
                _ => None,
            });
            assert_eq!(plate, Some(DVec3::new(40.0, 10.0, 3.0)));
            // teile.kopf(teile.M3)
            assert!(has_op(
                out,
                |o| matches!(o, Op::Prim { prim: cad_ir::Prim::Cylinder { r1, .. }, .. } if (*r1 - 1.8 * 3.2 / 2.0).abs() < 1e-12)
            ));
        },
    },
    Expect {
        file: "skizze_deckel.mcad",
        grids: &[],
        instances: &[],
        handles: 15,
        ops: &[("sketch", 3), ("offset", 2), ("difference", 2), ("cylinder", 4)],
        check: |out| {
            // the outline with four rounded corners: 4 lines + 4 arcs
            let n = out.tree.nodes.iter().find_map(|n| match &n.op {
                Op::Profile(cad_ir::Profile::Sketch { contours, .. }) => Some(contours.clone()),
                _ => None,
            });
            let kinds: Vec<&str> = n.as_ref().unwrap()[0].segs.iter().map(|g| g.kind()).collect();
            assert_eq!(kinds, ["line", "arc", "line", "arc", "line", "arc", "line", "arc"]);
            let offs: Vec<f64> = out
                .tree
                .nodes
                .iter()
                .filter_map(|n| match n.op {
                    Op::Offset { round: true, d, .. } => Some(d),
                    _ => None,
                })
                .collect();
            assert_eq!(offs.len(), 2);
            assert!((offs[0] + 2.2).abs() < 1e-12 && (offs[1] + 3.7).abs() < 1e-12, "{offs:?}");
        },
    },
    Expect {
        file: "skizze_halter.mcad",
        grids: &[],
        instances: &[],
        handles: 11,
        ops: &[("fillet2d", 2), ("stroke", 2), ("sketch", 2), ("polygon", 1)],
        check: |out| {
            let r: Vec<f64> = out
                .tree
                .nodes
                .iter()
                .filter_map(|n| match n.op {
                    Op::Blend2 { kind: BlendKind::Fillet, r, .. } => Some(r),
                    _ => None,
                })
                .collect();
            assert_eq!(r, [1.0, 5.0]);
            // the zone of the inner block
            let z = out.tree.nodes.iter().filter(|n| n.flags.zone).count();
            assert_eq!(z, 1);
        },
    },
    Expect {
        file: "skizze_kabelkanal.mcad",
        grids: &[],
        instances: &[],
        handles: 6,
        ops: &[("stroke", 4), ("sketch", 4), ("linear_extrude", 3)],
        check: |out| {
            let caps: Vec<cad_ir::StrokeCap> = out
                .tree
                .nodes
                .iter()
                .filter_map(|n| match n.op {
                    Op::Stroke { cap, .. } => Some(cap),
                    _ => None,
                })
                .collect();
            use cad_ir::StrokeCap::*;
            assert_eq!(caps, [Flat, Flat, Square, Round]);
            // the open path with a rounded corner: line, arc, line, arc (corner), line
            let n = out.tree.nodes.iter().find_map(|n| match &n.op {
                Op::Profile(cad_ir::Profile::Sketch { contours, closed: false }) => Some(contours.clone()),
                _ => None,
            });
            let kinds: Vec<&str> = n.as_ref().unwrap()[0].segs.iter().map(|g| g.kind()).collect();
            assert_eq!(kinds, ["line", "arc", "line", "arc", "line"]);
        },
    },
    Expect {
        file: "skizze_profil.mcad",
        grids: &[("s", 6, false)],
        instances: &[],
        handles: 9,
        ops: &[("sketch", 2), ("linear_extrude", 1), ("rotate_extrude", 1)],
        check: |out| {
            let sk: Vec<&Vec<cad_ir::Contour>> = out
                .tree
                .nodes
                .iter()
                .filter_map(|n| match &n.op {
                    Op::Profile(cad_ir::Profile::Sketch { contours, .. }) => Some(contours),
                    _ => None,
                })
                .collect();
            assert_eq!(
                sk[0].iter().map(|c| (c.hole, c.segs.len())).collect::<Vec<_>>(),
                [(false, 4), (false, 2), (true, 4)]
            );
            let kinds: Vec<&str> = sk[1][0].segs.iter().map(|g| g.kind()).collect();
            assert_eq!(kinds, ["line", "line", "arc", "bezier", "line"]);
            assert_eq!(out.sketches.len(), 2);
        },
    },
    Expect {
        file: "skizze_sweep.mcad",
        grids: &[],
        instances: &[],
        handles: 15,
        ops: &[("sweep", 2), ("sketch", 2), ("circle", 2), ("rect", 1)],
        check: |out| {
            // both sweeps take their path from an open sketch child
            assert_eq!(
                out.tree.nodes.iter().filter(|n| matches!(&n.op, Op::Sweep { path, .. } if path.is_empty())).count(),
                2
            );
            assert!(out.sketches.iter().all(|s| !s.closed));
        },
    },
    Expect {
        file: "skizze_zahnriemenscheibe.mcad",
        grids: &[],
        instances: &[],
        handles: 0,
        ops: &[("sketch", 1), ("scheibe", 1), ("cylinder", 4)],
        check: |out| {
            let n = out.tree.nodes.iter().find_map(|n| match &n.op {
                Op::Profile(cad_ir::Profile::Sketch { contours, .. }) => Some(contours.clone()),
                _ => None,
            });
            let c = n.unwrap();
            // 20 teeth: two arcs each; the bore: two arcs (a second contour)
            assert_eq!(c.iter().map(|c| c.segs.len()).collect::<Vec<_>>(), [40, 2]);
            assert!(c.iter().all(|c| c.closed && c.segs.iter().all(|g| g.kind() == "arc")));
        },
    },
    Expect {
        file: "stl_import.mcad",
        grids: &[],
        instances: &[],
        handles: 4,
        ops: &[("mesh", 2), ("difference", 1), ("cylinder", 1), ("mirror", 1)],
        check: |out| {
            // the same file twice: automatically a B-rep, then `as = "mesh"`
            let modes: Vec<cad_ir::MeshMode> = out
                .tree
                .nodes
                .iter()
                .filter_map(|n| match &n.op {
                    Op::Prim { prim: cad_ir::Prim::Mesh { mesh, mode }, .. } => {
                        assert_eq!((mesh.mesh.triangle_count(), mesh.mesh.face_count()), (20, 8));
                        Some(*mode)
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(modes, [cad_ir::MeshMode::AutoSolid, cad_ir::MeshMode::Mesh]);
        },
    },
    Expect {
        file: "svg_logo.mcad",
        grids: &[],
        instances: &[],
        handles: 11,
        ops: &[("linear_extrude", 5), ("color", 6), ("box", 1)],
        check: |out| {
            // one profile per filled SVG element: logo 10 (ring, 8 teeth, bolt), leaf 2,
            // house 4, oval (id) 1, arc (id) 1
            let sketches =
                out.tree.nodes.iter().filter(|n| matches!(n.op, Op::Profile(cad_ir::Profile::Sketch { .. })));
            assert_eq!(sketches.count(), 10 + 2 + 4 + 1 + 1);
            // the ring keeps its circle as arcs (quarters), with a hole
            let ring = out.tree.nodes.iter().find_map(|n| match &n.op {
                Op::Profile(cad_ir::Profile::Sketch { contours, .. })
                    if contours.len() == 2 && contours.iter().all(|c| c.segs.iter().all(|g| g.kind() == "arc")) =>
                {
                    Some(contours.clone())
                }
                _ => None,
            });
            assert!(ring.is_some(), "ring of arcs");
        },
    },
    Expect {
        file: "text_schild.mcad",
        grids: &[],
        instances: &[],
        handles: 8,
        ops: &[("linear_extrude", 4), ("color", 4), ("platte", 3)],
        check: |out| {
            // glyphs: "Werkstatt" 9, "ZUTRITT NUR MIT BRILLE" 19 (Allerta Stencil: letters in
            // several parts are one glyph each)
            let glyphs = out
                .tree
                .nodes
                .iter()
                .filter(|n| {
                    matches!(&n.op, Op::Profile(cad_ir::Profile::Sketch { .. })) && n.stmt != out.tree.nodes[0].stmt
                })
                .count();
            assert_eq!(glyphs, 3 + 9 + 19, "3 plate sketches + glyphs");
        },
    },
    Expect {
        file: "variabler_radius.mcad",
        grids: &[("g", 7, true)],
        instances: &[],
        handles: 16,
        ops: &[("fillet", 2), ("difference", 1), ("zone", 1), ("box", 4)],
        check: |out| {
            let laws: Vec<&cad_ir::Law> = out
                .tree
                .nodes
                .iter()
                .filter_map(|n| match &n.op {
                    Op::Blend { kind: BlendKind::Fillet, law: Some(l), .. } => Some(l.as_ref()),
                    Op::Boolean { fillet_law: Some(l), .. } => Some(l.as_ref()),
                    _ => None,
                })
                .collect();
            assert_eq!(laws.len(), 3);
            let r = |l: &cad_ir::Law, x: f64, z: f64| l.eval(DVec3::new(x, 0.0, z)).unwrap();
            // p.z < g.mitte ? 2 : 4 (the grid plane captured as its value 10)
            assert_eq!((r(laws[0], 0.0, 5.0), r(laws[0], 0.0, 15.0)), (2.0, 4.0));
            // 1 + p.x / 40 in the difference's own system
            assert_eq!(r(laws[1], 20.0, 0.0), 1.5);
            // lookup over x
            assert_eq!((r(laws[2], 0.0, 20.0), r(laws[2], 20.0, 20.0), r(laws[2], 30.0, 20.0)), (1.5, 4.0, 2.75));
        },
    },
    Expect {
        file: "winkel_anker.mcad",
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
    let files = model_files("examples/spec");
    let names: Vec<String> = files.iter().map(|f| file_name(f)).collect();
    let expected: Vec<&str> = EXPECT.iter().map(|e| e.file).collect();
    assert_eq!(names, expected, "every model in examples/spec needs an entry in EXPECT");

    for (path, e) in files.iter().zip(EXPECT) {
        let src = std::fs::read_to_string(path).unwrap();
        assert!(src.starts_with("// Zweck:"), "{}: missing header comment", e.file);
        let out = eval_file(path, &src);
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
