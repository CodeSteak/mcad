//! Kernel wrapper that counts calls (cache tests, CLI statistics) and, with a clock, sums the
//! time spent per method (profiling).

use std::collections::BTreeMap;

use cad_kernel::{
    BoolOp, Class, EdgeId, EdgeInfo, ExportFormat, FaceId, FaceInfo, KResult, Kernel, Loop, MassProps, Mesh,
    PathInterp, PlacedLoop, Tag,
};
use glam::{DMat4, DVec2, DVec3};

use crate::Clock;

/// Delegates to `inner` and counts every call by method name.
pub struct CountingKernel<K> {
    pub inner: K,
    pub counts: BTreeMap<&'static str, u64>,
    /// Milliseconds per method name (only with [`CountingKernel::with_clock`]).
    pub times: BTreeMap<&'static str, f64>,
    clock: Option<Clock>,
}

impl<K> CountingKernel<K> {
    pub fn new(inner: K) -> Self {
        CountingKernel { inner, counts: BTreeMap::new(), times: BTreeMap::new(), clock: None }
    }

    /// Also measures the time per method with `clock` (milliseconds).
    pub fn with_clock(inner: K, clock: Clock) -> Self {
        CountingKernel { clock: Some(clock), ..Self::new(inner) }
    }

    pub fn total(&self) -> u64 {
        self.counts.values().sum()
    }

    pub fn count(&self, name: &str) -> u64 {
        self.counts.get(name).copied().unwrap_or(0)
    }

    pub fn reset(&mut self) {
        self.counts.clear();
        self.times.clear();
    }

    fn hit(&mut self, name: &'static str) -> f64 {
        *self.counts.entry(name).or_default() += 1;
        self.clock.as_ref().map_or(0.0, |c| c())
    }

    fn done<T>(&mut self, name: &'static str, t0: f64, r: T) -> T {
        if let Some(c) = &self.clock {
            *self.times.entry(name).or_default() += c() - t0;
        }
        r
    }
}

impl<K: Kernel> Kernel for CountingKernel<K> {
    type Shape = K::Shape;

    fn id(&self) -> &'static str {
        self.inner.id()
    }
    fn make_box(&mut self, min: DVec3, max: DVec3, tag: Tag) -> KResult<Self::Shape> {
        let t0 = self.hit("make_box");
        let r = self.inner.make_box(min, max, tag);
        self.done("make_box", t0, r)
    }
    fn make_sphere(&mut self, r: f64, tag: Tag) -> KResult<Self::Shape> {
        let t0 = self.hit("make_sphere");
        let r = self.inner.make_sphere(r, tag);
        self.done("make_sphere", t0, r)
    }
    fn make_cylinder(&mut self, h: f64, r1: f64, r2: f64, tag: Tag) -> KResult<Self::Shape> {
        let t0 = self.hit("make_cylinder");
        let r = self.inner.make_cylinder(h, r1, r2, tag);
        self.done("make_cylinder", t0, r)
    }
    fn make_torus(&mut self, big_r: f64, r: f64, tag: Tag) -> KResult<Self::Shape> {
        let t0 = self.hit("make_torus");
        let r = self.inner.make_torus(big_r, r, tag);
        self.done("make_torus", t0, r)
    }
    fn make_polyhedron(&mut self, points: &[DVec3], faces: &[Vec<u32>], tag: Tag) -> KResult<Self::Shape> {
        let t0 = self.hit("make_polyhedron");
        let r = self.inner.make_polyhedron(points, faces, tag);
        self.done("make_polyhedron", t0, r)
    }
    fn transform(&mut self, s: &Self::Shape, m: &DMat4) -> KResult<Self::Shape> {
        let t0 = self.hit("transform");
        let r = self.inner.transform(s, m);
        self.done("transform", t0, r)
    }
    fn boolean(&mut self, op: BoolOp, shapes: &[Self::Shape], tag: Tag) -> KResult<Self::Shape> {
        let t0 = self.hit("boolean");
        let r = self.inner.boolean(op, shapes, tag);
        self.done("boolean", t0, r)
    }
    fn fillet(&mut self, s: &Self::Shape, edges: &[EdgeId], r: f64, tag: Tag) -> KResult<Self::Shape> {
        let t0 = self.hit("fillet");
        let r = self.inner.fillet(s, edges, r, tag);
        self.done("fillet", t0, r)
    }
    fn chamfer(&mut self, s: &Self::Shape, edges: &[EdgeId], d: f64, tag: Tag) -> KResult<Self::Shape> {
        let t0 = self.hit("chamfer");
        let r = self.inner.chamfer(s, edges, d, tag);
        self.done("chamfer", t0, r)
    }
    fn law_stations(&mut self, s: &Self::Shape, edges: &[EdgeId], counts: &[u32]) -> KResult<Vec<Vec<DVec3>>> {
        let t0 = self.hit("law_stations");
        let r = self.inner.law_stations(s, edges, counts);
        self.done("law_stations", t0, r)
    }
    fn fillet_variable(
        &mut self,
        s: &Self::Shape,
        edges: &[EdgeId],
        radii: &[Vec<f64>],
        tag: Tag,
    ) -> KResult<Self::Shape> {
        let t0 = self.hit("fillet_variable");
        let r = self.inner.fillet_variable(s, edges, radii, tag);
        self.done("fillet_variable", t0, r)
    }
    fn extrude(&mut self, p: &PlacedLoop, h: f64, twist: f64, scale: DVec2, tag: Tag) -> KResult<Self::Shape> {
        let t0 = self.hit("extrude");
        let r = self.inner.extrude(p, h, twist, scale, tag);
        self.done("extrude", t0, r)
    }
    fn revolve(&mut self, p: &PlacedLoop, angle: f64, tag: Tag) -> KResult<Self::Shape> {
        let t0 = self.hit("revolve");
        let r = self.inner.revolve(p, angle, tag);
        self.done("revolve", t0, r)
    }
    fn loft(&mut self, sections: &[PlacedLoop], ruled: bool, tag: Tag) -> KResult<Self::Shape> {
        let t0 = self.hit("loft");
        let r = self.inner.loft(sections, ruled, tag);
        self.done("loft", t0, r)
    }
    fn sweep(&mut self, p: &Loop, path: &[DVec3], interp: PathInterp, tag: Tag) -> KResult<Self::Shape> {
        let t0 = self.hit("sweep");
        let r = self.inner.sweep(p, path, interp, tag);
        self.done("sweep", t0, r)
    }
    fn nurbs_surface(&mut self, rows: &[Vec<DVec3>], tag: Tag) -> KResult<Self::Shape> {
        let t0 = self.hit("nurbs_surface");
        let r = self.inner.nurbs_surface(rows, tag);
        self.done("nurbs_surface", t0, r)
    }
    fn thicken(&mut self, s: &Self::Shape, t: f64, tag: Tag) -> KResult<Self::Shape> {
        let t0 = self.hit("thicken");
        let r = self.inner.thicken(s, t, tag);
        self.done("thicken", t0, r)
    }
    fn shell(&mut self, s: &Self::Shape, t: f64, open: &[FaceId], tag: Tag) -> KResult<Self::Shape> {
        let t0 = self.hit("shell");
        let r = self.inner.shell(s, t, open, tag);
        self.done("shell", t0, r)
    }
    fn edges(&mut self, s: &Self::Shape, samples: u32) -> KResult<Vec<EdgeInfo>> {
        let t0 = self.hit("edges");
        let r = self.inner.edges(s, samples);
        self.done("edges", t0, r)
    }
    fn faces(&mut self, s: &Self::Shape, samples: u32) -> KResult<Vec<FaceInfo>> {
        let t0 = self.hit("faces");
        let r = self.inner.faces(s, samples);
        self.done("faces", t0, r)
    }
    fn classify(&mut self, s: &Self::Shape, pts: &[DVec3]) -> KResult<Vec<Class>> {
        let t0 = self.hit("classify");
        let r = self.inner.classify(s, pts);
        self.done("classify", t0, r)
    }
    fn tessellate(&mut self, s: &Self::Shape, tol: f64) -> KResult<Mesh> {
        let t0 = self.hit("tessellate");
        let r = self.inner.tessellate(s, tol);
        self.done("tessellate", t0, r)
    }
    fn mass_props(&mut self, s: &Self::Shape) -> KResult<MassProps> {
        let t0 = self.hit("mass_props");
        let r = self.inner.mass_props(s);
        self.done("mass_props", t0, r)
    }
    fn bounds(&mut self, s: &Self::Shape) -> KResult<Option<(DVec3, DVec3)>> {
        let t0 = self.hit("bounds");
        let r = self.inner.bounds(s);
        self.done("bounds", t0, r)
    }
    fn is_empty(&mut self, s: &Self::Shape) -> bool {
        let t0 = self.hit("is_empty");
        let r = self.inner.is_empty(s);
        self.done("is_empty", t0, r)
    }
    fn is_solid(&mut self, s: &Self::Shape) -> bool {
        let t0 = self.hit("is_solid");
        let r = self.inner.is_solid(s);
        self.done("is_solid", t0, r)
    }
    fn brep(&mut self, s: &Self::Shape) -> KResult<cad_kernel::brep::Brep> {
        let t0 = self.hit("brep");
        let r = self.inner.brep(s);
        self.done("brep", t0, r)
    }
    fn export(&mut self, shapes: &[Self::Shape], fmt: ExportFormat) -> KResult<Vec<u8>> {
        let t0 = self.hit("export");
        let r = self.inner.export(shapes, fmt);
        self.done("export", t0, r)
    }
    fn set_draft(&mut self, draft: bool) {
        self.inner.set_draft(draft)
    }
    fn shape_bytes(&mut self, s: &Self::Shape) -> usize {
        self.inner.shape_bytes(s)
    }
}
