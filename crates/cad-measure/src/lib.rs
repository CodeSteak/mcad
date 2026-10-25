//! Measuring on the exact geometry (Plan 18): the shared measurement logic of `mcad measure`
//! and the measure mode of the UI (via `cad-session`).
//!
//! * [`curve::EdgeCurve`]: edges as bounded exact curves (closest point, length, midpoint).
//! * [`surface`]: closest point and normal on the exact surfaces.
//! * [`body::Body`]: one shape's B-rep in context coordinates with the triangles of each face
//!   (where a point lies on a trimmed face; coarse searches).
//! * [`measure`]: distance (minimum with foot points, parallel planes: normal distance),
//!   angle, radius/diameter, face area.
//! * [`section`]: contours of a section plane (plane ∩ B-rep from the kernel, segments in
//!   closed form where the cut surface is analytic), region areas, wall thickness.
//! * [`dist`]: minimum distance of triangle sets (coarse search, `--clearance`).
//!
//! Kernel-independent: the caller supplies B-reps (`Kernel::brep`), tessellations and, for
//! sections, the B-rep of the kept half (a kernel boolean).

pub mod body;
pub mod curve;
pub mod dist;
pub mod measure;
pub mod num;
pub mod section;
pub mod surface;

pub use body::{Body, FaceGeom};
pub use curve::EdgeCurve;
pub use measure::{AngleResult, DistanceResult, Geo, Radius, RadiusResult, angle, distance, face_area, radius};
pub use section::{Section, SectionPlane, Wall};
