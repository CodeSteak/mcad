//! `nurbs_surface`, `thicken`, `shell`, `offset` (plan 1.6), as the SMLib facade's
//! `sc_nurbs_surface`, `sc_thicken`, `sc_shell`, `sc_offset`: solidcore's
//! `SmBSplineSurface::ApproximatePoints`, `SmApiShellBrepFull` and `SmApiOffsetBrepFull` with the
//! facade's flags and its checks of the result. Not ported: the facade's fallback for a failed
//! shell (`shell_by_booleans`, a shell from an inward offset and Booleans) and its pre-check of
//! opposite walls; the check of the offset's trim curves (`trim_gap`, SMLib's tessellation
//! follows them, the neutral B-rep does not).

use cad_kernel::{FaceId, KResult, KernelError, Tag};
use glam::DVec3;
use solidcore::offset::offset_executive::p2::{OffsetOptions, ShellOptions, offset_brep_simple, shell_brep_simple};
use solidcore::query::api_queries as q;

use crate::Shape;
use crate::imp::{Inner, check_err, dv, err, pt};

fn failed(m: impl std::fmt::Display) -> KernelError {
    KernelError::failed(format!("solidcore: {m}"))
}

fn invalid(m: impl std::fmt::Display) -> KernelError {
    KernelError::InvalidInput(format!("solidcore: {m}"))
}

impl Inner {
    /// An interpolating B-spline surface through the grid `rows` as one open face, as
    /// `sc_nurbs_surface`: SMLib's least-squares fit with one control point per point,
    /// degree 3 per direction where it has at least 4 points, else one less than its count.
    pub fn nurbs_surface(&mut self, rows: &[Vec<DVec3>], tag: Tag) -> KResult<Shape> {
        const OP: &str = "nurbs_surface";
        let cols = rows.first().map_or(0, Vec::len);
        if rows.len() < 2 || cols < 2 || rows.iter().any(|r| r.len() != cols) {
            return Err(invalid("nurbs_surface needs at least 2 rows of equal length >= 2"));
        }
        if rows.iter().flatten().any(|p| !p.is_finite()) {
            return Err(invalid("nurbs_surface: non-finite point"));
        }
        let pts: Vec<solidcore::Point3> = rows.iter().flatten().map(|p| pt(*p)).collect();
        let (du, dv) = (3.min(rows.len() - 1) as u64, 3.min(cols - 1) as u64);
        let bs = solidcore::nurbs::b_spline_surface::BSplineSurface::approximate_points(
            &pts,
            rows.len() as u64,
            cols as u64,
            du,
            dv,
        )
        .map_err(|e| err(OP, e))?;
        let srf = solidcore::surface::surface::Surface::from(bs);
        let mut b = solidcore::topology::brep::p3::new_brep(
            &mut self.k,
            solidcore::geom::tol::USE_DEFAULT,
            solidcore::geom::tol::USE_DEFAULT,
        )
        .map_err(|e| err(OP, e))?;
        solidcore::blend::fillet_executive::api::create_face_from_surface(&mut self.k, &mut b, &srf)
            .map_err(|e| err(OP, e))?;
        b.refresh_cached_topology();
        crate::imp::made(b, tag)
    }

    /// A sheet thickened by `t` into a solid (side walls at its open edges), as `sc_thicken`
    /// (`SmApiShellBrepFull(t, extend, merge, solid)`).
    pub fn thicken(&mut self, s: &Shape, t: f64, tag: Tag) -> KResult<Shape> {
        const OP: &str = "thicken";
        if !(t.abs() > 0.0 && t.is_finite()) {
            return Err(invalid("thickness must be non-zero"));
        }
        if self.is_empty(s) || self.is_solid(s) {
            return Err(invalid("thicken expects an open surface"));
        }
        let opts = ShellOptions {
            offset: OffsetOptions { distance: t, extend_convex_edges: true, merge_results: true },
            create_offset_solid: true,
            merge_sheet_side_walls: false,
        };
        let r = shell_brep_simple(&mut self.k, &s.brep.borrow(), opts, &[]).map_err(|e| err(OP, e))?;
        let r = crate::ShapeData::new(r, tag);
        self.check_valid(&r, OP)?;
        self.retag(&r, s, tag)?;
        Ok(r)
    }

    /// A hollow solid with walls of thickness `t` inside the body, the faces `open` removed, as
    /// `sc_shell` (`SmApiShellBrepFull(-t, extend, merge)`): a closed solid without
    /// self-overlap, smaller than the body.
    pub fn shell(&mut self, s: &Shape, t: f64, open: &[FaceId], tag: Tag) -> KResult<Shape> {
        const OP: &str = "shell";
        if !(t > 0.0 && t.is_finite()) {
            return Err(invalid("shell thickness must be > 0"));
        }
        let faces = {
            let b = s.brep.borrow();
            let all = if b.face_list().is_empty() { Vec::new() } else { q::get_faces(&b).map_err(|e| err(OP, e))? };
            let mut v = Vec::with_capacity(open.len());
            for &i in open {
                v.push(*all.get(i as usize).ok_or_else(|| invalid(format!("face id {i} out of range")))?);
            }
            v
        };
        let opts = ShellOptions {
            offset: OffsetOptions { distance: -t, extend_convex_edges: true, merge_results: true },
            create_offset_solid: false,
            merge_sheet_side_walls: false,
        };
        let r = shell_brep_simple(&mut self.k, &s.brep.borrow(), opts, &faces).map_err(|e| err(OP, e))?;
        let r = crate::ShapeData::new(r, tag);
        if self.is_empty(&r) || !self.is_solid(&r) {
            return Err(failed(format!(
                "shell: wall thickness {t} leaves no cavity (solidcore returned no closed solid)"
            )));
        }
        self.check_valid(&r, OP)?;
        let (v0, v1) = (
            self.mass_props(s).map_err(|e| check_err(OP, e))?.volume,
            self.mass_props(&r).map_err(|e| check_err(OP, e))?.volume,
        );
        if v1.is_nan() || v1 <= 0.0 || v1 >= v0 {
            return Err(failed(format!("shell returned a wrong body (volume {v1}, body {v0})")));
        }
        self.retag(&r, s, tag)?;
        Ok(r)
    }

    /// The outward offset by `d` (Minkowski sum with a ball), as `sc_offset` with mode 2
    /// (self-intersection handling, convex edges rounded): a closed solid larger than the body,
    /// without self-overlap, whose boundary lies at distance `d` from the body (else `Failed`:
    /// exact or nothing).
    pub fn offset(&mut self, s: &Shape, d: f64, tag: Tag) -> KResult<Shape> {
        const OP: &str = "offset";
        if !(d > 0.0 && d.is_finite()) {
            return Err(invalid("offset distance must be > 0"));
        }
        if self.is_empty(s) || !self.is_solid(s) {
            return Err(invalid("offset expects a closed solid"));
        }
        let mut c = self.copy(&s.brep.borrow())?;
        let opts = OffsetOptions { distance: d, extend_convex_edges: false, merge_results: true };
        let o = offset_brep_simple(&mut self.k, &mut c, opts).map_err(|e| err(OP, e))?;
        let o = crate::ShapeData::new(o, tag);
        if self.is_empty(&o) || !self.is_solid(&o) {
            return Err(failed("offset did not produce a closed solid"));
        }
        let (vin, vo) = (
            self.mass_props(s).map_err(|e| check_err(OP, e))?.volume,
            self.mass_props(&o).map_err(|e| check_err(OP, e))?.volume,
        );
        if vo.is_nan() || vo <= vin {
            return Err(failed("offset produced a smaller solid"));
        }
        self.check_valid(&o, OP)?;
        let tol = 1e-6 * crate::imp::model_diag(&self.k, &o.brep.borrow()).max(1.0);
        let dev = self.offset_deviation(&o, s, d)?;
        if dev.is_nan() || dev > tol {
            return Err(failed(format!(
                "offset is not exact: the result lies up to {dev} mm off the distance {d} from the body"
            )));
        }
        self.retag(&o, s, tag)?;
        Ok(o)
    }

    /// As the facade's `check_valid`: no self-overlapping faces.
    fn check_valid(&mut self, r: &Shape, what: &str) -> KResult<()> {
        let bad = self.invalid_faces(r, None)?;
        if !bad.is_empty() {
            return Err(failed(format!(
                "{what} produced an invalid (self-overlapping) solid at {} face(s); parameter too large?",
                bad.len()
            )));
        }
        Ok(())
    }

    /// Tags of the faces of `r` from the input `s` (verified), new faces `tag`.
    fn retag(&mut self, r: &Shape, s: &Shape, tag: Tag) -> KResult<()> {
        let mut b = r.brep.borrow_mut();
        self.resolve_tags(&mut b, std::slice::from_ref(s), tag, true)?;
        Ok(())
    }

    /// Largest deviation of the boundary of `o` from the distance `d` to the body `s`, as the
    /// facade's `offset_deviation`: 9 points per edge and a sample of every face. The distance
    /// to the body is the smallest to its faces (the foot on a face's surface where it lies in
    /// the face) and its edges.
    fn offset_deviation(&mut self, o: &Shape, s: &Shape, d: f64) -> KResult<f64> {
        const OP: &str = "offset (check)";
        let mut pts = Vec::new();
        {
            let mut b = o.brep.borrow_mut();
            for e in b.edge_list().to_vec() {
                let ed = b.edge(e).map_err(|e| err(OP, e))?;
                let Some(c) = ed.curve.as_ref() else { continue };
                let iv = ed.interval;
                for j in 0..=8 {
                    if let Ok(p) = c.evaluate_point(&self.k, iv.min() + (iv.max() - iv.min()) * f64::from(j) / 8.0) {
                        pts.push(dv(p));
                    }
                }
            }
            for f in b.face_list().to_vec() {
                if let Some((p, _)) = crate::brep::face_sample(&self.k, &mut b, f) {
                    pts.push(p);
                }
            }
        }
        let mut b = s.brep.borrow_mut();
        let (faces, edges) = (b.face_list().to_vec(), b.edge_list().to_vec());
        let mut dev: f64 = 0.0;
        for p in pts {
            let mut best = f64::INFINITY;
            for &e in &edges {
                if let Ok(c) = q::edge_closest_point(&self.k, &b, e, pt(p)) {
                    best = best.min(c.closest.distance);
                }
            }
            for &f in &faces {
                let Ok(Some(srf)) = solidcore::topology::face::p2::surface(&b, f) else { continue };
                let Ok(c) = q::surface_closest_point(&self.k, srf, pt(p)) else { continue };
                if c.closest.distance >= best {
                    continue;
                }
                let inside = c.uvs.iter().any(|&uv| {
                    q::face_classify_uv(&self.k, &mut b, f, uv)
                        .is_ok_and(|x| x == solidcore::intersect::point_class::PointClassType::Face)
                });
                if inside {
                    best = c.closest.distance;
                }
            }
            dev = dev.max((best - d).abs());
        }
        Ok(dev)
    }
}
