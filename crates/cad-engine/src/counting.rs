//! Kernel wrapper that counts calls (cache tests, CLI statistics).

use std::collections::BTreeMap;

use cad_kernel::{
    BoolOp, Class, EdgeId, EdgeInfo, ExportFormat, FaceId, FaceInfo, KResult, Kernel, Loop,
    MassProps, Mesh, PathInterp, PlacedLoop, Tag,
};
use glam::{DMat4, DVec2, DVec3};

/// Delegates to `inner` and counts every call by method name.
pub struct CountingKernel<K> {
    pub inner: K,
    pub counts: BTreeMap<&'static str, u64>,
}

impl<K> CountingKernel<K> {
    pub fn new(inner: K) -> Self {
        CountingKernel { inner, counts: BTreeMap::new() }
    }

    pub fn total(&self) -> u64 {
        self.counts.values().sum()
    }

    pub fn count(&self, name: &str) -> u64 {
        self.counts.get(name).copied().unwrap_or(0)
    }

    pub fn reset(&mut self) {
        self.counts.clear();
    }

    fn hit(&mut self, name: &'static str) {
        *self.counts.entry(name).or_default() += 1;
    }
}

impl<K: Kernel> Kernel for CountingKernel<K> {
    type Shape = K::Shape;

    fn id(&self) -> &'static str {
        self.inner.id()
    }
    fn make_box(&mut self, min: DVec3, max: DVec3, tag: Tag) -> KResult<Self::Shape> {
        self.hit("make_box");
        self.inner.make_box(min, max, tag)
    }
    fn make_sphere(&mut self, r: f64, tag: Tag) -> KResult<Self::Shape> {
        self.hit("make_sphere");
        self.inner.make_sphere(r, tag)
    }
    fn make_cylinder(&mut self, h: f64, r1: f64, r2: f64, tag: Tag) -> KResult<Self::Shape> {
        self.hit("make_cylinder");
        self.inner.make_cylinder(h, r1, r2, tag)
    }
    fn make_torus(&mut self, big_r: f64, r: f64, tag: Tag) -> KResult<Self::Shape> {
        self.hit("make_torus");
        self.inner.make_torus(big_r, r, tag)
    }
    fn make_polyhedron(&mut self, points: &[DVec3], faces: &[Vec<u32>], tag: Tag) -> KResult<Self::Shape> {
        self.hit("make_polyhedron");
        self.inner.make_polyhedron(points, faces, tag)
    }
    fn transform(&mut self, s: &Self::Shape, m: &DMat4) -> KResult<Self::Shape> {
        self.hit("transform");
        self.inner.transform(s, m)
    }
    fn boolean(&mut self, op: BoolOp, shapes: &[Self::Shape], tag: Tag) -> KResult<Self::Shape> {
        self.hit("boolean");
        self.inner.boolean(op, shapes, tag)
    }
    fn fillet(&mut self, s: &Self::Shape, edges: &[EdgeId], r: f64, tag: Tag) -> KResult<Self::Shape> {
        self.hit("fillet");
        self.inner.fillet(s, edges, r, tag)
    }
    fn chamfer(&mut self, s: &Self::Shape, edges: &[EdgeId], d: f64, tag: Tag) -> KResult<Self::Shape> {
        self.hit("chamfer");
        self.inner.chamfer(s, edges, d, tag)
    }
    fn extrude(&mut self, p: &PlacedLoop, h: f64, twist: f64, scale: DVec2, tag: Tag) -> KResult<Self::Shape> {
        self.hit("extrude");
        self.inner.extrude(p, h, twist, scale, tag)
    }
    fn revolve(&mut self, p: &PlacedLoop, angle: f64, tag: Tag) -> KResult<Self::Shape> {
        self.hit("revolve");
        self.inner.revolve(p, angle, tag)
    }
    fn loft(&mut self, sections: &[PlacedLoop], ruled: bool, tag: Tag) -> KResult<Self::Shape> {
        self.hit("loft");
        self.inner.loft(sections, ruled, tag)
    }
    fn sweep(&mut self, p: &Loop, path: &[DVec3], interp: PathInterp, tag: Tag) -> KResult<Self::Shape> {
        self.hit("sweep");
        self.inner.sweep(p, path, interp, tag)
    }
    fn nurbs_surface(&mut self, rows: &[Vec<DVec3>], tag: Tag) -> KResult<Self::Shape> {
        self.hit("nurbs_surface");
        self.inner.nurbs_surface(rows, tag)
    }
    fn thicken(&mut self, s: &Self::Shape, t: f64, tag: Tag) -> KResult<Self::Shape> {
        self.hit("thicken");
        self.inner.thicken(s, t, tag)
    }
    fn shell(&mut self, s: &Self::Shape, t: f64, open: &[FaceId], tag: Tag) -> KResult<Self::Shape> {
        self.hit("shell");
        self.inner.shell(s, t, open, tag)
    }
    fn edges(&mut self, s: &Self::Shape, samples: u32) -> KResult<Vec<EdgeInfo>> {
        self.hit("edges");
        self.inner.edges(s, samples)
    }
    fn faces(&mut self, s: &Self::Shape, samples: u32) -> KResult<Vec<FaceInfo>> {
        self.hit("faces");
        self.inner.faces(s, samples)
    }
    fn classify(&mut self, s: &Self::Shape, pts: &[DVec3]) -> KResult<Vec<Class>> {
        self.hit("classify");
        self.inner.classify(s, pts)
    }
    fn tessellate(&mut self, s: &Self::Shape, tol: f64) -> KResult<Mesh> {
        self.hit("tessellate");
        self.inner.tessellate(s, tol)
    }
    fn mass_props(&mut self, s: &Self::Shape) -> KResult<MassProps> {
        self.hit("mass_props");
        self.inner.mass_props(s)
    }
    fn bounds(&mut self, s: &Self::Shape) -> KResult<Option<(DVec3, DVec3)>> {
        self.hit("bounds");
        self.inner.bounds(s)
    }
    fn is_empty(&mut self, s: &Self::Shape) -> bool {
        self.hit("is_empty");
        self.inner.is_empty(s)
    }
    fn is_solid(&mut self, s: &Self::Shape) -> bool {
        self.hit("is_solid");
        self.inner.is_solid(s)
    }
    fn export(&mut self, shapes: &[Self::Shape], fmt: ExportFormat) -> KResult<Vec<u8>> {
        self.hit("export");
        self.inner.export(shapes, fmt)
    }
}
