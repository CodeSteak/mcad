//! [`cad_kernel::Kernel`] over [solidcore](https://github.com/tangency-labs/solidcore), the Rust
//! port of SMLib (plan section 2.3, milestone M8).
//!
//! solidcore is not usable end to end yet (its README: "no primitives, booleans or fillets"):
//! its `SM_API`-level entry points exist, but most stop at a unit that is not ported and return
//! `SolidError::Unported`. This crate calls the public API where it exists and maps
//!
//! * `SolidError::Unported` → [`KernelError::Unsupported`]`("solidcore: <op>: …")`,
//! * every operation solidcore has no entry point for → `Unsupported("solidcore: <op>")`,
//! * other solidcore errors → [`KernelError::Failed`] / [`KernelError::InvalidInput`].
//!
//! So `cad diff-kernels` can report which operations already work ("coverage"). Faces carry
//! the tag of the operation that made the shape (solidcore's attribute plumbing for provenance
//! is not wired here yet).
//!
//! Feature `solidcore` (default): link solidcore (git dependency pinned to a commit). Without
//! it the crate compiles without solidcore and every operation is `Unsupported`.

// without solidcore the operations ignore their arguments
#![cfg_attr(not(feature = "solidcore"), allow(unused_variables))]

use cad_kernel::{
    BoolOp, Class, EdgeInfo, FaceId, FaceInfo, KResult, Kernel, KernelError, Loop, MassProps, Mesh, PathInterp,
    PlacedLoop, Tag,
};
use glam::{DMat4, DVec2, DVec3};
use std::rc::Rc;

/// One solidcore B-rep and the tag of the operation that made it.
#[derive(Debug)]
pub struct ShapeData {
    #[cfg(feature = "solidcore")]
    pub brep: solidcore::Brep,
    pub tag: Tag,
}

pub type Shape = Rc<ShapeData>;

/// The commit of solidcore this crate is built against.
pub const SOLIDCORE_REV: &str = "3a6a50b30a7829d00abcffbe9fa561d88ab37dc1";

fn unsupported(op: &str) -> KernelError {
    KernelError::Unsupported(format!("solidcore: {op}"))
}

#[cfg(feature = "solidcore")]
mod imp {
    use super::*;
    use solidcore::SolidError;
    use solidcore::geom::a_object::BooleanOperation;
    use solidcore::mass::integrator::properties as mp;
    use solidcore::primitive::api_primitives as ap;
    use solidcore::query::api_queries as q;

    pub fn err(op: &str, e: SolidError) -> KernelError {
        if e.is_unported() {
            KernelError::Unsupported(format!("solidcore: {op}: {e}"))
        } else if matches!(e, SolidError::InvalidInput { .. }) {
            KernelError::InvalidInput(format!("solidcore: {op}: {e}"))
        } else {
            KernelError::failed(format!("solidcore: {op}: {e}"))
        }
    }

    pub fn pt(p: DVec3) -> solidcore::Point3 {
        solidcore::Point3::new(p.x, p.y, p.z)
    }

    pub struct Inner {
        pub k: solidcore::Kernel,
    }

    impl Inner {
        pub fn new() -> Self {
            Inner { k: solidcore::Kernel::default() }
        }

        fn wrap(r: Result<solidcore::Brep, SolidError>, op: &str, tag: Tag) -> KResult<Shape> {
            r.map(|brep| Rc::new(ShapeData { brep, tag })).map_err(|e| err(op, e))
        }

        pub fn make_box(&mut self, min: DVec3, max: DVec3, tag: Tag) -> KResult<Shape> {
            let d = max - min;
            Self::wrap(ap::create_box(&mut self.k, pt(min), d.x, d.y, d.z), "make_box", tag)
        }

        pub fn make_sphere(&mut self, r: f64, tag: Tag) -> KResult<Shape> {
            Self::wrap(ap::create_sphere(&mut self.k, pt(DVec3::ZERO), r), "make_sphere", tag)
        }

        pub fn make_cylinder(&mut self, h: f64, r1: f64, r2: f64, tag: Tag) -> KResult<Shape> {
            if (r1 - r2).abs() <= 1e-12 * r1.max(r2) {
                Self::wrap(ap::create_cylinder(&mut self.k, pt(DVec3::ZERO), r1, h), "make_cylinder", tag)
            } else if r1 > 0.0 {
                Self::wrap(ap::create_cone(&mut self.k, pt(DVec3::ZERO), r1, r2, h), "make_cylinder (cone)", tag)
            } else {
                // the facade flips a cone built with its tip on top; needs a B-rep transform
                Err(unsupported("make_cylinder: cone with the tip at z = 0 (needs transform)"))
            }
        }

        pub fn make_torus(&mut self, big_r: f64, r: f64, tag: Tag) -> KResult<Shape> {
            Self::wrap(ap::create_torus(&mut self.k, pt(DVec3::ZERO), big_r, r), "make_torus", tag)
        }

        pub fn boolean(&mut self, op: BoolOp, shapes: &[Shape], tag: Tag) -> KResult<Shape> {
            let op_sc = match op {
                BoolOp::Union => BooleanOperation::Union,
                BoolOp::Difference => BooleanOperation::Difference,
                BoolOp::Intersection => BooleanOperation::Intersection,
            };
            let mut it = shapes.iter();
            let first = it.next().ok_or_else(|| KernelError::InvalidInput("boolean without operands".into()))?;
            let mut acc = first.brep.clone();
            for s in it {
                let out = solidcore::boolean::api_brep::boolean(&mut self.k, acc, s.brep.clone(), op_sc)
                    .map_err(|e| err("boolean", e))?;
                acc = out.result;
            }
            Ok(Rc::new(ShapeData { brep: acc, tag }))
        }

        pub fn faces(&mut self, s: &Shape) -> KResult<Vec<FaceInfo>> {
            let fs = q::get_faces(&s.brep).map_err(|e| err("faces", e))?;
            let mut out = Vec::with_capacity(fs.len());
            for (i, f) in fs.into_iter().enumerate() {
                let p = q::face_internal_point(&self.k, &s.brep, f).map_err(|e| err("faces", e))?;
                out.push(FaceInfo { id: i as FaceId, tag: s.tag, points: vec![DVec3::new(p.x, p.y, p.z)] });
            }
            Ok(out)
        }

        pub fn edges(&mut self, s: &Shape) -> KResult<Vec<EdgeInfo>> {
            let es = q::get_edges(&s.brep).map_err(|e| err("edges", e))?;
            let mut out = Vec::with_capacity(es.len());
            for (i, e) in es.into_iter().enumerate() {
                let b = q::edge_bounding_box(&self.k, &s.brep, e, true).map_err(|e| err("edges", e))?;
                let lo = DVec3::new(b.min_at(0), b.min_at(1), b.min_at(2));
                let hi = DVec3::new(b.max_at(0), b.max_at(1), b.max_at(2));
                out.push(EdgeInfo { id: i as u32, points: vec![lo, hi], face_tags: vec![s.tag, s.tag], smooth: false });
            }
            Ok(out)
        }

        pub fn mass_props(&mut self, s: &Shape) -> KResult<MassProps> {
            let store = solidcore::geom::attribute::AttributeStore::new();
            let key_of = |_e: solidcore::EntityRef| solidcore::geom::attribute::ObjectKey(0);
            let attrs = mp::PropertyAttributes { store: &store, key_of: &key_of };
            let m = mp::brep_mass_properties(&self.k, &s.brep, 1e-6, 1.0, pt(DVec3::ZERO), &attrs)
                .map_err(|e| err("mass_props", e))?;
            let area = mp::brep_area(&self.k, &s.brep, 1e-6).map_err(|e| err("mass_props (area)", e))?;
            let c = m.centroid;
            Ok(MassProps { volume: m.properties.volume, area, centroid: DVec3::new(c.x, c.y, c.z) })
        }

        pub fn bounds(&mut self, s: &Shape) -> KResult<Option<(DVec3, DVec3)>> {
            if s.brep.face_list().is_empty() {
                return Ok(None);
            }
            let b = q::brep_bounding_box(&self.k, &s.brep, true).map_err(|e| err("bounds", e))?;
            Ok(Some((
                DVec3::new(b.min_at(0), b.min_at(1), b.min_at(2)),
                DVec3::new(b.max_at(0), b.max_at(1), b.max_at(2)),
            )))
        }

        pub fn is_solid(&mut self, s: &Shape) -> bool {
            !s.brep.face_list().is_empty() && q::brep_is_manifold_solid(&s.brep).unwrap_or(false)
        }

        pub fn is_empty(&mut self, s: &Shape) -> bool {
            s.brep.face_list().is_empty()
        }
    }
}

/// Kernel over solidcore.
pub struct SolidcoreKernel {
    #[cfg(feature = "solidcore")]
    inner: imp::Inner,
}

impl std::fmt::Debug for SolidcoreKernel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SolidcoreKernel")
    }
}

impl Default for SolidcoreKernel {
    fn default() -> Self {
        Self::new()
    }
}

impl SolidcoreKernel {
    pub fn new() -> Self {
        SolidcoreKernel {
            #[cfg(feature = "solidcore")]
            inner: imp::Inner::new(),
        }
    }

    /// `true` if built with solidcore (feature `solidcore`).
    pub fn available() -> bool {
        cfg!(feature = "solidcore")
    }
}

/// Dispatches to `imp` with feature `solidcore`, else `Unsupported`.
macro_rules! with_sc {
    ($self:ident, $op:literal, |$i:ident| $body:expr) => {{
        #[cfg(feature = "solidcore")]
        {
            let $i = &mut $self.inner;
            $body
        }
        #[cfg(not(feature = "solidcore"))]
        {
            Err(unsupported(concat!($op, " (built without feature `solidcore`)")))
        }
    }};
}

impl Kernel for SolidcoreKernel {
    type Shape = Shape;

    fn id(&self) -> &'static str {
        "solidcore-3a6a50b"
    }

    fn make_box(&mut self, min: DVec3, max: DVec3, tag: Tag) -> KResult<Shape> {
        with_sc!(self, "make_box", |i| i.make_box(min, max, tag))
    }

    fn make_sphere(&mut self, r: f64, tag: Tag) -> KResult<Shape> {
        with_sc!(self, "make_sphere", |i| i.make_sphere(r, tag))
    }

    fn make_cylinder(&mut self, h: f64, r1: f64, r2: f64, tag: Tag) -> KResult<Shape> {
        with_sc!(self, "make_cylinder", |i| i.make_cylinder(h, r1, r2, tag))
    }

    fn make_torus(&mut self, big_r: f64, r: f64, tag: Tag) -> KResult<Shape> {
        with_sc!(self, "make_torus", |i| i.make_torus(big_r, r, tag))
    }

    fn make_polyhedron(&mut self, _points: &[DVec3], _faces: &[Vec<u32>], _tag: Tag) -> KResult<Shape> {
        Err(unsupported("make_polyhedron (SmBrep::CreateFromPolyBrep not exposed)"))
    }

    fn transform(&mut self, _s: &Shape, _m: &DMat4) -> KResult<Shape> {
        Err(unsupported("transform (no B-rep transform in the public API)"))
    }

    fn boolean(&mut self, op: BoolOp, shapes: &[Shape], tag: Tag) -> KResult<Shape> {
        with_sc!(self, "boolean", |i| i.boolean(op, shapes, tag))
    }

    fn fillet(&mut self, _s: &Shape, _edges: &[u32], _r: f64, _tag: Tag) -> KResult<Shape> {
        Err(unsupported("fillet"))
    }

    fn chamfer(&mut self, _s: &Shape, _edges: &[u32], _d: f64, _tag: Tag) -> KResult<Shape> {
        Err(unsupported("chamfer"))
    }

    fn extrude(&mut self, _p: &PlacedLoop, _h: f64, _twist: f64, _scale: DVec2, _tag: Tag) -> KResult<Shape> {
        Err(unsupported("extrude"))
    }

    fn revolve(&mut self, _p: &PlacedLoop, _angle: f64, _tag: Tag) -> KResult<Shape> {
        Err(unsupported("revolve"))
    }

    fn loft(&mut self, _sections: &[PlacedLoop], _ruled: bool, _tag: Tag) -> KResult<Shape> {
        Err(unsupported("loft"))
    }

    fn sweep(&mut self, _p: &Loop, _path: &[DVec3], _interp: PathInterp, _tag: Tag) -> KResult<Shape> {
        Err(unsupported("sweep"))
    }

    fn nurbs_surface(&mut self, _rows: &[Vec<DVec3>], _tag: Tag) -> KResult<Shape> {
        Err(unsupported("nurbs_surface"))
    }

    fn thicken(&mut self, _s: &Shape, _t: f64, _tag: Tag) -> KResult<Shape> {
        Err(unsupported("thicken"))
    }

    fn shell(&mut self, _s: &Shape, _t: f64, _open: &[FaceId], _tag: Tag) -> KResult<Shape> {
        Err(unsupported("shell"))
    }

    fn edges(&mut self, s: &Shape, _samples: u32) -> KResult<Vec<EdgeInfo>> {
        let _ = s;
        with_sc!(self, "edges", |i| i.edges(s))
    }

    fn faces(&mut self, s: &Shape, _samples: u32) -> KResult<Vec<FaceInfo>> {
        let _ = s;
        with_sc!(self, "faces", |i| i.faces(s))
    }

    fn classify(&mut self, _s: &Shape, _pts: &[DVec3]) -> KResult<Vec<Class>> {
        Err(unsupported("classify"))
    }

    fn tessellate(&mut self, _s: &Shape, _tol: f64) -> KResult<Mesh> {
        Err(unsupported("tessellate"))
    }

    fn mass_props(&mut self, s: &Shape) -> KResult<MassProps> {
        let _ = s;
        with_sc!(self, "mass_props", |i| i.mass_props(s))
    }

    fn bounds(&mut self, s: &Shape) -> KResult<Option<(DVec3, DVec3)>> {
        let _ = s;
        with_sc!(self, "bounds", |i| i.bounds(s))
    }

    fn is_empty(&mut self, s: &Shape) -> bool {
        #[cfg(feature = "solidcore")]
        {
            self.inner.is_empty(s)
        }
        #[cfg(not(feature = "solidcore"))]
        {
            let _ = s;
            true
        }
    }

    fn is_solid(&mut self, s: &Shape) -> bool {
        #[cfg(feature = "solidcore")]
        {
            self.inner.is_solid(s)
        }
        #[cfg(not(feature = "solidcore"))]
        {
            let _ = s;
            false
        }
    }
}
