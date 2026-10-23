//! Kernel interface. The engine talks to geometry only through [`Kernel`].
//!
//! Units: millimetres, angles in degrees at the language level; this interface takes
//! radians where it says so.

use glam::{DMat4, DVec2, DVec3};
use serde::{Deserialize, Serialize};

pub mod brep;
pub mod curve2;

pub use curve2::Segment;

/// Provenance tag on faces. The engine assigns tags; the kernel only carries them
/// through operations. Faces created by an operation (fillet faces, caps of an
/// extrusion, …) get the tag passed to that operation.
pub type Tag = u32;

/// Index of an edge in the list returned by [`Kernel::edges`] for the same shape.
pub type EdgeId = u32;
/// Index of a face in the list returned by [`Kernel::faces`] for the same shape.
pub type FaceId = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BoolOp {
    Union,
    Difference,
    Intersection,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum KernelError {
    /// The operation failed. `edges` lists edges of the input that caused it, if known.
    #[error("{msg}")]
    Failed { msg: String, edges: Vec<EdgeId> },
    #[error("not supported by this kernel: {0}")]
    Unsupported(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
}

impl KernelError {
    pub fn failed(msg: impl Into<String>) -> Self {
        KernelError::Failed { msg: msg.into(), edges: Vec::new() }
    }
}

pub type KResult<T> = Result<T, KernelError>;

/// A closed planar loop in the xy-plane of its profile system.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Loop {
    /// Straight segments, implicitly closed. Counter-clockwise or clockwise.
    Polygon(Vec<DVec2>),
    Circle {
        center: DVec2,
        r: f64,
    },
    /// Closed interpolating spline through the points.
    Spline(Vec<DVec2>),
    /// Connected segments (lines, arcs, B-splines) forming a closed loop: each segment starts
    /// where the previous one ends, the last ends at the start of the first (sketches).
    Composite(Vec<Segment>),
}

/// An open planar path of connected segments, placed in space by `frame` (the path of a
/// `sweep` drawn as an open sketch).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlacedPath {
    pub segs: Vec<Segment>,
    pub frame: DMat4,
}

/// A planar loop placed in space: `frame` maps the profile's xy-plane to model space.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlacedLoop {
    pub lp: Loop,
    pub frame: DMat4,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PathInterp {
    Linear,
    Spline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Class {
    Inside,
    Outside,
    On,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EdgeInfo {
    pub id: EdgeId,
    /// Start point, interior samples, end point (at least 2 points).
    pub points: Vec<DVec3>,
    /// Tags of the adjacent faces (one entry for boundary edges of open shells).
    pub face_tags: Vec<Tag>,
    /// Nothing to blend here: a seam or pole edge, an edge between tangent-continuous faces or
    /// an open boundary. Never selected for fillets, chamfers or zones.
    #[serde(default)]
    pub smooth: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FaceInfo {
    pub id: FaceId,
    pub tag: Tag,
    /// Sample points on the face (boundary and interior).
    pub points: Vec<DVec3>,
}

/// Triangle mesh with provenance. All arrays are flat.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Mesh {
    /// xyz per vertex.
    pub positions: Vec<f32>,
    /// xyz per vertex.
    pub normals: Vec<f32>,
    /// Three indices per triangle.
    pub indices: Vec<u32>,
    /// Face tag per triangle.
    pub tri_tags: Vec<Tag>,
    /// Face index ([`FaceId`]) per triangle.
    pub tri_faces: Vec<FaceId>,
    /// Edge polylines as segment pairs: xyz xyz per segment.
    pub edge_segments: Vec<f32>,
    /// [`EdgeId`] per segment.
    pub edge_ids: Vec<EdgeId>,
}

impl Mesh {
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// Appends `other` transformed by `m`.
    pub fn append_transformed(&mut self, other: &Mesh, m: &DMat4) {
        let base = (self.positions.len() / 3) as u32;
        let nm = m.inverse().transpose();
        let flip = m.determinant() < 0.0;
        for p in other.positions.chunks_exact(3) {
            let v = m.transform_point3(DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64));
            self.positions.extend_from_slice(&[v.x as f32, v.y as f32, v.z as f32]);
        }
        for n in other.normals.chunks_exact(3) {
            let v = nm.transform_vector3(DVec3::new(n[0] as f64, n[1] as f64, n[2] as f64)).normalize_or_zero();
            self.normals.extend_from_slice(&[v.x as f32, v.y as f32, v.z as f32]);
        }
        for t in other.indices.chunks_exact(3) {
            if flip {
                self.indices.extend_from_slice(&[t[0] + base, t[2] + base, t[1] + base]);
            } else {
                self.indices.extend_from_slice(&[t[0] + base, t[1] + base, t[2] + base]);
            }
        }
        self.tri_tags.extend_from_slice(&other.tri_tags);
        self.tri_faces.extend_from_slice(&other.tri_faces);
        for p in other.edge_segments.chunks_exact(3) {
            let v = m.transform_point3(DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64));
            self.edge_segments.extend_from_slice(&[v.x as f32, v.y as f32, v.z as f32]);
        }
        self.edge_ids.extend_from_slice(&other.edge_ids);
    }

    /// Axis-aligned bounds of the vertices, or `None` if empty.
    pub fn bounds(&self) -> Option<(DVec3, DVec3)> {
        let mut it = self.positions.chunks_exact(3);
        let first = it.next()?;
        let mut lo = DVec3::new(first[0] as f64, first[1] as f64, first[2] as f64);
        let mut hi = lo;
        for p in it {
            let v = DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64);
            lo = lo.min(v);
            hi = hi.max(v);
        }
        Some((lo, hi))
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct MassProps {
    pub volume: f64,
    pub area: f64,
    pub centroid: DVec3,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ExportFormat {
    Step,
}

/// A solid modelling kernel.
///
/// Shapes are immutable values: every operation returns a new shape and leaves its
/// inputs untouched. Shapes carry face tags; operations keep the tags of surviving
/// (possibly split or trimmed) faces and give new faces the operation's `tag`.
pub trait Kernel {
    type Shape: Clone;

    /// Name and version, part of every cache key.
    fn id(&self) -> &'static str;

    // ---- primitives (local system) ----
    fn make_box(&mut self, min: DVec3, max: DVec3, tag: Tag) -> KResult<Self::Shape>;
    fn make_sphere(&mut self, r: f64, tag: Tag) -> KResult<Self::Shape>;
    /// Cylinder or cone along +z from z = 0 to z = h. `r2` may be 0 (cone tip), not both.
    fn make_cylinder(&mut self, h: f64, r1: f64, r2: f64, tag: Tag) -> KResult<Self::Shape>;
    /// Torus in the xy-plane around the z-axis.
    fn make_torus(&mut self, big_r: f64, r: f64, tag: Tag) -> KResult<Self::Shape>;
    /// Solid bounded by planar polygonal faces (OpenSCAD `polyhedron`). Faces index into
    /// `points`, any orientation; the kernel orients them outward.
    fn make_polyhedron(&mut self, points: &[DVec3], faces: &[Vec<u32>], tag: Tag) -> KResult<Self::Shape>;

    // ---- operations ----
    fn transform(&mut self, s: &Self::Shape, m: &DMat4) -> KResult<Self::Shape>;
    /// `Difference`: first minus all others. `Union`/`Intersection`: all shapes.
    fn boolean(&mut self, op: BoolOp, shapes: &[Self::Shape], tag: Tag) -> KResult<Self::Shape>;
    fn fillet(&mut self, s: &Self::Shape, edges: &[EdgeId], r: f64, tag: Tag) -> KResult<Self::Shape>;
    fn chamfer(&mut self, s: &Self::Shape, edges: &[EdgeId], d: f64, tag: Tag) -> KResult<Self::Shape>;

    /// Variable-radius fillet, step 1: for each edge `edges[i]` the `counts[i]` points (in
    /// order along the edge, the first at its start, the last at its end) at which the radius
    /// law takes its values. The kernel decides where they lie; the caller evaluates its
    /// radius function there and passes the values to [`Kernel::fillet_variable`].
    fn law_stations(&mut self, s: &Self::Shape, edges: &[EdgeId], counts: &[u32]) -> KResult<Vec<Vec<DVec3>>> {
        let _ = (s, edges, counts);
        Err(KernelError::Unsupported("variable-radius fillet".into()))
    }

    /// Variable-radius fillet, step 2: `radii[i]` are the radii at the stations of `edges[i]`
    /// (as returned by [`Kernel::law_stations`] for `radii[i].len()` stations).
    fn fillet_variable(
        &mut self,
        s: &Self::Shape,
        edges: &[EdgeId],
        radii: &[Vec<f64>],
        tag: Tag,
    ) -> KResult<Self::Shape> {
        let _ = (s, edges, radii, tag);
        Err(KernelError::Unsupported("variable-radius fillet".into()))
    }

    /// Extrude along +z of the profile frame by `h`, rotating by `twist` (radians) and
    /// scaling the profile by `scale` at the top.
    fn extrude(&mut self, p: &PlacedLoop, h: f64, twist: f64, scale: DVec2, tag: Tag) -> KResult<Self::Shape>;
    /// Revolve around the z-axis of model space by `angle` (radians). The loop lies in
    /// the xz-half-plane x >= 0 after applying its frame.
    fn revolve(&mut self, p: &PlacedLoop, angle: f64, tag: Tag) -> KResult<Self::Shape>;
    fn loft(&mut self, sections: &[PlacedLoop], ruled: bool, tag: Tag) -> KResult<Self::Shape>;
    /// Sweep a profile along a path. The profile frame's origin sits at the path start
    /// with its z-axis along the start tangent.
    fn sweep(&mut self, p: &Loop, path: &[DVec3], interp: PathInterp, tag: Tag) -> KResult<Self::Shape>;
    /// Sweep a profile along an open path of exact segments. The profile is placed at the
    /// path start like in [`Kernel::sweep`].
    fn sweep_path(&mut self, p: &Loop, path: &PlacedPath, tag: Tag) -> KResult<Self::Shape> {
        let _ = (p, path, tag);
        Err(KernelError::Unsupported("sweep along a sketch path".into()))
    }
    /// Open NURBS surface interpolating a grid of points (rows of equal length).
    fn nurbs_surface(&mut self, rows: &[Vec<DVec3>], tag: Tag) -> KResult<Self::Shape>;
    /// Solid from an open surface, offset by `t` along its normal.
    fn thicken(&mut self, s: &Self::Shape, t: f64, tag: Tag) -> KResult<Self::Shape>;
    /// Hollow solid with wall thickness `t` (inwards); `open` faces are removed.
    fn shell(&mut self, s: &Self::Shape, t: f64, open: &[FaceId], tag: Tag) -> KResult<Self::Shape>;

    // ---- queries ----
    fn edges(&mut self, s: &Self::Shape, samples: u32) -> KResult<Vec<EdgeInfo>>;
    fn faces(&mut self, s: &Self::Shape, samples: u32) -> KResult<Vec<FaceInfo>>;
    fn classify(&mut self, s: &Self::Shape, pts: &[DVec3]) -> KResult<Vec<Class>>;
    /// `tol`: maximum chordal deviation in mm.
    fn tessellate(&mut self, s: &Self::Shape, tol: f64) -> KResult<Mesh>;
    fn mass_props(&mut self, s: &Self::Shape) -> KResult<MassProps>;
    /// Axis-aligned bounding box, `None` for an empty shape.
    fn bounds(&mut self, s: &Self::Shape) -> KResult<Option<(DVec3, DVec3)>>;
    fn is_empty(&mut self, s: &Self::Shape) -> bool;
    /// `true` for closed solids, `false` for open surfaces.
    fn is_solid(&mut self, s: &Self::Shape) -> bool;

    /// Topology and exact geometry of `s` ([`brep::Brep`], e.g. for STEP export).
    fn brep(&mut self, s: &Self::Shape) -> KResult<brep::Brep> {
        let _ = s;
        Err(KernelError::Unsupported("brep".into()))
    }

    fn export(&mut self, shapes: &[Self::Shape], fmt: ExportFormat) -> KResult<Vec<u8>> {
        let _ = (shapes, fmt);
        Err(KernelError::Unsupported("export".into()))
    }

    /// Estimated memory of `s` in bytes (cache limits of the engine). Should be cheap: called
    /// once per cached node result. Default: a flat 16 KiB.
    fn shape_bytes(&mut self, s: &Self::Shape) -> usize {
        let _ = s;
        16 << 10
    }

    /// Draft mode for the following calls: their results only serve a temporary display
    /// (while a handle is dragged) and are never reused for a final result, so the kernel may
    /// skip expensive validation of its results. Default: ignored.
    fn set_draft(&mut self, draft: bool) {
        let _ = draft;
    }
}
