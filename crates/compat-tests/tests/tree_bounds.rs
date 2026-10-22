//! Stage 1b: bounding box of the evaluated geometry tree against OpenSCAD's STL bounds.
//!
//! Without a kernel the exact result of a Boolean is unknown, so the box is estimated
//! from the tree: union → union of boxes, intersection → intersection of boxes,
//! difference → box of the first child. Primitives, profiles and extrusions are bounded
//! exactly (spheres/discs as ellipsoids/ellipses under the affine frame; revolutions and
//! twists sampled finely). This checks transforms, `center`, `r`/`d`, `$fn` polygons,
//! modifiers and `children()` against OpenSCAD.
//!
//! The estimate must contain OpenSCAD's box. It must be equal (within the tolerance)
//! unless the file is listed in `LARGER`, where a difference cuts the outer extent.

mod common;

use common::bounds::bounds3;
use common::*;
use glam::{DMat4, DVec3};
use serde_json::Value;

/// OpenSCAD facets round shapes with `$fn` = 128 (inscribed polygons, sphere rings
/// that miss the poles and the equator): its box may be smaller by up to about
/// 3e-4 · r; 0.01 mm covers radii up to 30 mm.
const TOL: f64 = 0.01;

/// Files where the tree estimate is legitimately larger than the real result.
const LARGER: &[(&str, &str)] = &[("28_cable_clip.scad", "the opening (difference) cuts the top of the ring")];

fn vec3(v: &Value) -> DVec3 {
    let a: Vec<f64> = v.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect();
    DVec3::new(a[0], a[1], a[2])
}

fn fmt(v: DVec3) -> String {
    format!("[{:.4}, {:.4}, {:.4}]", v.x, v.y, v.z)
}

#[test]
fn tree_bounds_match_openscad() {
    let text = std::fs::read_to_string(workspace_root().join("tests/openscad-compat/reference.json")).unwrap();
    let reference: Value = serde_json::from_str(&text).unwrap();
    let mut fails = Vec::new();
    for path in scad_files("examples/openscad-compat") {
        let name = file_name(&path);
        let Some(r) = reference["files"].get(&name) else {
            continue;
        };
        let src = std::fs::read_to_string(&path).unwrap();
        let out = eval(&src);
        let root = out.tree.root.expect("tree root");
        // `!`: OpenSCAD makes the marked subtree the design root without the
        // transformations above it, so the estimate starts in the root's own system.
        let b = match bounds3(&out.tree, root, DMat4::IDENTITY) {
            Ok(b) => b,
            Err(e) => {
                fails.push(format!("{name}: {e}"));
                continue;
            }
        };
        let (rlo, rhi) = (vec3(&r["bounds"]["min"]), vec3(&r["bounds"]["max"]));
        let larger = LARGER.iter().any(|(f, _)| *f == name);
        let contains = (b.lo - rlo).max_element() <= TOL && (rhi - b.hi).max_element() <= TOL;
        let equal = (b.lo - rlo).abs().max_element() <= TOL && (b.hi - rhi).abs().max_element() <= TOL;
        let status = if !contains {
            "DOES NOT CONTAIN"
        } else if equal {
            if larger { "equal (listed in LARGER)" } else { "ok" }
        } else if larger {
            "ok (larger, expected)"
        } else {
            "LARGER"
        };
        println!("{name:32} {status:24} ours {} {}  openscad {} {}", fmt(b.lo), fmt(b.hi), fmt(rlo), fmt(rhi));
        if !status.starts_with("ok") {
            fails.push(format!(
                "{name}: {status}: ours {} {}, openscad {} {}",
                fmt(b.lo),
                fmt(b.hi),
                fmt(rlo),
                fmt(rhi)
            ));
        }
    }
    assert!(fails.is_empty(), "bounds differ:\n{}", fails.join("\n"));
}
