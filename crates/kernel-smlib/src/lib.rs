//! [`cad_kernel::Kernel`] over NVIDIA SMLib (usd-brep @ c9c5979) through the C++ facade in
//! `smlib/facade`.
//!
//! * feature `native` (default): facade + SMLib/SM_API statically linked (see `build.rs`).
//! * feature `wasm` (target `wasm32-unknown-unknown`): SMLib runs as a separate Emscripten
//!   module (`smlib.js` / `smlib.wasm`, built by `smlib/build-wasm.sh`); calls go through
//!   `js/smlib-bridge.js`, which must have been initialised with `initSmlib(createSmlib)`
//!   before [`SmlibKernel::new`] is called.
//!
//! Shapes are reference-counted handles into the facade's handle table; dropping the last
//! clone releases the SMLib Brep. Face tags (provenance) are SMLib attributes carried through
//! operations; see `smlib/facade/sc_facade.cpp` ("Provenance").

use std::rc::Rc;

use cad_kernel::{
    BoolOp, Class, EdgeInfo, FaceId, FaceInfo, KResult, Kernel, KernelError, Loop, MassProps, Mesh,
    PathInterp, PlacedLoop, Tag,
};
use glam::{DMat4, DVec2, DVec3};

#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
#[path = "raw_native.rs"]
mod raw;

#[cfg(all(feature = "wasm", target_arch = "wasm32"))]
#[path = "raw_wasm.rs"]
mod raw;

#[cfg(not(any(
    all(feature = "native", not(target_arch = "wasm32")),
    all(feature = "wasm", target_arch = "wasm32")
)))]
compile_error!(
    "kernel-smlib: enable feature `native` (non-wasm targets) or `wasm` (wasm32-unknown-unknown)"
);

/// Facade calls are not reentrant and SMLib's context is process-global: serialise all calls
/// (a call and the reads of its output buffers form one critical section).
#[cfg(not(target_arch = "wasm32"))]
fn locked<T>(f: impl FnOnce() -> T) -> T {
    use std::sync::Mutex;
    static LOCK: Mutex<()> = Mutex::new(());
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    f()
}

#[cfg(target_arch = "wasm32")]
fn locked<T>(f: impl FnOnce() -> T) -> T {
    f()
}

const OP_UNION: i32 = 0;
const OP_DIFFERENCE: i32 = 1;
const OP_INTERSECTION: i32 = 2;

const LOOP_POLYGON: f64 = 0.0;
const LOOP_CIRCLE: f64 = 1.0;
const LOOP_SPLINE: f64 = 2.0;

/// One SMLib Brep in the facade's handle table. Released on drop.
#[derive(Debug)]
pub struct ShapeHandle {
    h: u32,
}

impl ShapeHandle {
    /// Raw facade handle (for diagnostics).
    pub fn raw(&self) -> u32 {
        self.h
    }
}

impl Drop for ShapeHandle {
    fn drop(&mut self) {
        let h = self.h;
        locked(|| raw::release(h));
    }
}

pub type Shape = Rc<ShapeHandle>;

/// Kernel over SMLib. Cheap to construct; all instances share the facade's global state.
#[derive(Debug)]
pub struct SmlibKernel {
    _priv: (),
}

impl Default for SmlibKernel {
    fn default() -> Self {
        Self::new()
    }
}

/// Error of the last facade call.
fn last_error() -> KernelError {
    let code = raw::last_status();
    let mut msg = raw::last_message();
    if msg.is_empty() {
        msg = "SMLib operation failed".to_string();
    }
    match code {
        2 => KernelError::Unsupported(msg),
        3 => KernelError::InvalidInput(msg),
        _ => KernelError::Failed { msg, edges: raw::last_edges() },
    }
}

fn shape_result(h: u32) -> KResult<Shape> {
    if h == 0 { Err(last_error()) } else { Ok(Rc::new(ShapeHandle { h })) }
}

fn status(code: i32) -> KResult<()> {
    if code == 0 { Ok(()) } else { Err(last_error()) }
}

fn v3(p: &[f64]) -> DVec3 {
    DVec3::new(p[0], p[1], p[2])
}

fn encode_loop(out: &mut Vec<f64>, lp: &Loop, frame: &DMat4) {
    let push_pts = |out: &mut Vec<f64>, kind: f64, pts: &[DVec2]| {
        out.push(kind);
        out.push(pts.len() as f64);
        for p in pts {
            out.push(p.x);
            out.push(p.y);
        }
    };
    match lp {
        Loop::Polygon(pts) => push_pts(out, LOOP_POLYGON, pts),
        Loop::Spline(pts) => push_pts(out, LOOP_SPLINE, pts),
        Loop::Circle { center, r } => {
            out.extend_from_slice(&[LOOP_CIRCLE, 3.0, center.x, center.y, *r]);
        }
    }
    out.extend_from_slice(&frame.to_cols_array());
}

impl SmlibKernel {
    /// Creates the kernel (initialises SMLib's context on first use).
    pub fn new() -> Self {
        locked(raw::init);
        SmlibKernel { _priv: () }
    }

    /// Facade ABI version.
    pub fn facade_version(&self) -> u32 {
        locked(raw::version)
    }

    /// Number of live Breps in the facade's handle table (leak checks).
    pub fn live_handles(&self) -> u32 {
        locked(raw::live_handles)
    }

    pub fn face_count(&self, s: &Shape) -> usize {
        locked(|| raw::face_count(s.h)).max(0) as usize
    }

    pub fn edge_count(&self, s: &Shape) -> usize {
        locked(|| raw::edge_count(s.h)).max(0) as usize
    }

    fn call(&self, f: impl FnOnce() -> u32) -> KResult<Shape> {
        locked(|| shape_result(f()))
    }

    /// Edge ids of `s` whose sample points all satisfy `pred` (test/diagnostic helper).
    pub fn edges_where(
        &mut self,
        s: &Shape,
        pred: impl Fn(&EdgeInfo) -> bool,
    ) -> KResult<Vec<u32>> {
        Ok(self.edges(s, 3)?.into_iter().filter(|e| pred(e)).map(|e| e.id).collect())
    }
}

impl Kernel for SmlibKernel {
    type Shape = Shape;

    fn id(&self) -> &'static str {
        "smlib-c9c5979/facade-1"
    }

    fn make_box(&mut self, min: DVec3, max: DVec3, tag: Tag) -> KResult<Shape> {
        self.call(|| raw::make_box(min.to_array(), max.to_array(), tag))
    }

    fn make_sphere(&mut self, r: f64, tag: Tag) -> KResult<Shape> {
        self.call(|| raw::make_sphere(r, tag))
    }

    fn make_cylinder(&mut self, h: f64, r1: f64, r2: f64, tag: Tag) -> KResult<Shape> {
        self.call(|| raw::make_cylinder(h, r1, r2, tag))
    }

    fn make_torus(&mut self, big_r: f64, r: f64, tag: Tag) -> KResult<Shape> {
        self.call(|| raw::make_torus(big_r, r, tag))
    }

    fn make_polyhedron(&mut self, points: &[DVec3], faces: &[Vec<u32>], tag: Tag) -> KResult<Shape> {
        let pts: Vec<f64> = points.iter().flat_map(|p| p.to_array()).collect();
        let sizes: Vec<u32> = faces.iter().map(|f| f.len() as u32).collect();
        let idx: Vec<u32> = faces.iter().flatten().copied().collect();
        self.call(|| raw::make_polyhedron(&pts, &sizes, &idx, tag))
    }

    fn transform(&mut self, s: &Shape, m: &DMat4) -> KResult<Shape> {
        let a = m.to_cols_array();
        self.call(|| raw::transform(s.h, &a))
    }

    fn boolean(&mut self, op: BoolOp, shapes: &[Shape], tag: Tag) -> KResult<Shape> {
        let op = match op {
            BoolOp::Union => OP_UNION,
            BoolOp::Difference => OP_DIFFERENCE,
            BoolOp::Intersection => OP_INTERSECTION,
        };
        let hs: Vec<u32> = shapes.iter().map(|s| s.h).collect();
        self.call(|| raw::boolean(op, &hs, tag))
    }

    fn fillet(&mut self, s: &Shape, edges: &[u32], r: f64, tag: Tag) -> KResult<Shape> {
        self.call(|| raw::fillet(s.h, edges, r, tag))
    }

    fn chamfer(&mut self, s: &Shape, edges: &[u32], d: f64, tag: Tag) -> KResult<Shape> {
        self.call(|| raw::chamfer(s.h, edges, d, tag))
    }

    fn extrude(&mut self, p: &PlacedLoop, h: f64, twist: f64, scale: DVec2, tag: Tag)
    -> KResult<Shape> {
        let mut enc = Vec::new();
        encode_loop(&mut enc, &p.lp, &p.frame);
        self.call(|| raw::extrude(&enc, h, twist, scale.x, scale.y, tag))
    }

    fn revolve(&mut self, p: &PlacedLoop, angle: f64, tag: Tag) -> KResult<Shape> {
        let mut enc = Vec::new();
        encode_loop(&mut enc, &p.lp, &p.frame);
        self.call(|| raw::revolve(&enc, angle, tag))
    }

    fn loft(&mut self, sections: &[PlacedLoop], ruled: bool, tag: Tag) -> KResult<Shape> {
        let mut enc = Vec::new();
        for s in sections {
            encode_loop(&mut enc, &s.lp, &s.frame);
        }
        let n = sections.len() as u32;
        self.call(|| raw::loft(&enc, n, ruled, tag))
    }

    fn sweep(&mut self, p: &Loop, path: &[DVec3], interp: PathInterp, tag: Tag) -> KResult<Shape> {
        let mut enc = Vec::new();
        encode_loop(&mut enc, p, &DMat4::IDENTITY);
        let pts: Vec<f64> = path.iter().flat_map(|p| p.to_array()).collect();
        let interp = match interp {
            PathInterp::Linear => 0,
            PathInterp::Spline => 1,
        };
        self.call(|| raw::sweep(&enc, &pts, interp, tag))
    }

    fn nurbs_surface(&mut self, rows: &[Vec<DVec3>], tag: Tag) -> KResult<Shape> {
        let cols = rows.first().map(|r| r.len()).unwrap_or(0);
        if rows.len() < 2 || cols < 2 || rows.iter().any(|r| r.len() != cols) {
            return Err(KernelError::InvalidInput(
                "nurbs_surface needs at least 2 rows of equal length >= 2".into(),
            ));
        }
        let pts: Vec<f64> = rows.iter().flatten().flat_map(|p| p.to_array()).collect();
        let nr = rows.len() as u32;
        self.call(|| raw::nurbs_surface(&pts, nr, cols as u32, tag))
    }

    fn thicken(&mut self, s: &Shape, t: f64, tag: Tag) -> KResult<Shape> {
        self.call(|| raw::thicken(s.h, t, tag))
    }

    fn shell(&mut self, s: &Shape, t: f64, open: &[FaceId], tag: Tag) -> KResult<Shape> {
        self.call(|| raw::shell(s.h, t, open, tag))
    }

    fn edges(&mut self, s: &Shape, samples: u32) -> KResult<Vec<EdgeInfo>> {
        let (pts, meta) = locked(|| {
            status(raw::edges(s.h, samples))?;
            Ok::<_, KernelError>((raw::out_f64(0), raw::out_u32(0)))
        })?;
        let mut out = Vec::new();
        let (mut pi, mut mi) = (0usize, 0usize);
        while mi < meta.len() {
            let np = meta[mi] as usize;
            let nt = meta[mi + 1] as usize;
            let face_tags = meta[mi + 2..mi + 2 + nt].to_vec();
            mi += 2 + nt;
            let points = (0..np).map(|k| v3(&pts[(pi + k) * 3..])).collect();
            pi += np;
            out.push(EdgeInfo { id: out.len() as u32, points, face_tags });
        }
        Ok(out)
    }

    fn faces(&mut self, s: &Shape, samples: u32) -> KResult<Vec<FaceInfo>> {
        let (pts, meta) = locked(|| {
            status(raw::faces(s.h, samples))?;
            Ok::<_, KernelError>((raw::out_f64(0), raw::out_u32(0)))
        })?;
        let mut out = Vec::new();
        let mut pi = 0usize;
        for m in meta.chunks_exact(2) {
            let (tag, np) = (m[0], m[1] as usize);
            let points = (0..np).map(|k| v3(&pts[(pi + k) * 3..])).collect();
            pi += np;
            out.push(FaceInfo { id: out.len() as u32, tag, points });
        }
        Ok(out)
    }

    fn classify(&mut self, s: &Shape, pts: &[DVec3]) -> KResult<Vec<Class>> {
        let flat: Vec<f64> = pts.iter().flat_map(|p| p.to_array()).collect();
        let res = locked(|| {
            status(raw::classify(s.h, &flat))?;
            Ok::<_, KernelError>(raw::out_u32(0))
        })?;
        Ok(res
            .into_iter()
            .map(|c| match c {
                0 => Class::Inside,
                2 => Class::On,
                _ => Class::Outside,
            })
            .collect())
    }

    fn tessellate(&mut self, s: &Shape, tol: f64) -> KResult<Mesh> {
        locked(|| {
            status(raw::tessellate(s.h, tol))?;
            Ok(Mesh {
                positions: raw::out_f32(0),
                normals: raw::out_f32(1),
                edge_segments: raw::out_f32(2),
                indices: raw::out_u32(0),
                tri_tags: raw::out_u32(1),
                tri_faces: raw::out_u32(2),
                edge_ids: raw::out_u32(3),
            })
        })
    }

    fn mass_props(&mut self, s: &Shape) -> KResult<MassProps> {
        let v = locked(|| {
            status(raw::mass_props(s.h))?;
            Ok::<_, KernelError>(raw::out_f64(0))
        })?;
        if v.len() < 5 {
            return Err(KernelError::failed("mass_props: no result"));
        }
        Ok(MassProps { volume: v[0], area: v[1], centroid: DVec3::new(v[2], v[3], v[4]) })
    }

    fn bounds(&mut self, s: &Shape) -> KResult<Option<(DVec3, DVec3)>> {
        let v = locked(|| {
            status(raw::bounds(s.h))?;
            Ok::<_, KernelError>(raw::out_f64(0))
        })?;
        if v.len() < 6 {
            return Ok(None);
        }
        Ok(Some((v3(&v[0..3]), v3(&v[3..6]))))
    }

    fn is_empty(&mut self, s: &Shape) -> bool {
        locked(|| raw::is_empty(s.h)) == 1
    }

    fn is_solid(&mut self, s: &Shape) -> bool {
        locked(|| raw::is_solid(s.h)) == 1
    }
}
