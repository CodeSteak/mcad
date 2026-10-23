//! Neutral 2D contours for imported outlines (SVG, font glyphs): exact segments – lines,
//! circular arcs, elliptic arcs, quadratic and cubic Bézier curves – with affine maps (circles
//! stay circles under similarities), flattening for measurements and the fill-rule
//! normalisation that turns `evenodd`/`nonzero` regions into consistently oriented contours.
//!
//! The evaluator turns them into `cad_ir::Profile::Sketch` contours; nothing here knows the
//! language or the kernel.

use glam::{DAffine2, DVec2};
use std::f64::consts::{PI, TAU};

/// One exact segment. Angles in radians.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Seg {
    Line {
        a: DVec2,
        b: DVec2,
    },
    /// Circular arc from angle `a0` to `a1`: `a1 > a0` runs counter-clockwise; `|a1 - a0| <= 2π`.
    Arc {
        center: DVec2,
        r: f64,
        a0: f64,
        a1: f64,
    },
    /// Quadratic Bézier curve (TrueType glyphs, SVG `Q`/`T`).
    Quad([DVec2; 3]),
    /// Cubic Bézier curve (CFF glyphs, SVG `C`/`S`).
    Cubic([DVec2; 4]),
    /// Elliptic arc `center + u cos t + v sin t` for `t` from `t0` to `t1` (`u`, `v` conjugate
    /// semi-diameters; perpendicular for SVG ellipses, any after a shear).
    Ellipse {
        center: DVec2,
        u: DVec2,
        v: DVec2,
        t0: f64,
        t1: f64,
    },
}

fn polar(c: DVec2, r: f64, a: f64) -> DVec2 {
    c + DVec2::new(a.cos(), a.sin()) * r
}

impl Seg {
    pub fn start(&self) -> DVec2 {
        self.point(0.0)
    }

    pub fn end(&self) -> DVec2 {
        self.point(1.0)
    }

    /// Point at the normalised parameter `s` in `[0, 1]`.
    pub fn point(&self, s: f64) -> DVec2 {
        match *self {
            Seg::Line { a, b } => a.lerp(b, s),
            Seg::Arc { center, r, a0, a1 } => polar(center, r, a0 + (a1 - a0) * s),
            Seg::Quad([p0, p1, p2]) => {
                let t = 1.0 - s;
                p0 * (t * t) + p1 * (2.0 * t * s) + p2 * (s * s)
            }
            Seg::Cubic([p0, p1, p2, p3]) => {
                let t = 1.0 - s;
                p0 * (t * t * t) + p1 * (3.0 * t * t * s) + p2 * (3.0 * t * s * s) + p3 * (s * s * s)
            }
            Seg::Ellipse { center, u, v, t0, t1 } => {
                let t = t0 + (t1 - t0) * s;
                center + u * t.cos() + v * t.sin()
            }
        }
    }

    pub fn reversed(&self) -> Seg {
        match *self {
            Seg::Line { a, b } => Seg::Line { a: b, b: a },
            Seg::Arc { center, r, a0, a1 } => Seg::Arc { center, r, a0: a1, a1: a0 },
            Seg::Quad([a, b, c]) => Seg::Quad([c, b, a]),
            Seg::Cubic([a, b, c, d]) => Seg::Cubic([d, c, b, a]),
            Seg::Ellipse { center, u, v, t0, t1 } => Seg::Ellipse { center, u, v, t0: t1, t1: t0 },
        }
    }

    /// Image under an affine map. Circular arcs stay circular under similarities (mirrored
    /// ones run the other way), otherwise they become elliptic arcs; elliptic arcs that map
    /// onto a circle become circular arcs.
    pub fn transform(&self, m: &DAffine2) -> Seg {
        match *self {
            Seg::Line { a, b } => Seg::Line { a: m.transform_point2(a), b: m.transform_point2(b) },
            Seg::Quad(p) => Seg::Quad(p.map(|q| m.transform_point2(q))),
            Seg::Cubic(p) => Seg::Cubic(p.map(|q| m.transform_point2(q))),
            Seg::Arc { center, r, a0, a1 } => {
                Seg::Ellipse { center, u: DVec2::new(r, 0.0), v: DVec2::new(0.0, r), t0: a0, t1: a1 }.transform(m)
            }
            Seg::Ellipse { center, u, v, t0, t1 } => {
                ellipse_or_circle(m.transform_point2(center), m.transform_vector2(u), m.transform_vector2(v), t0, t1)
            }
        }
    }

    /// Points from start to end (both included) with a chordal deviation of about `tol`.
    pub fn flatten(&self, tol: f64) -> Vec<DVec2> {
        let tol = tol.max(1e-9);
        let n = match *self {
            Seg::Line { .. } => 1,
            Seg::Arc { r, a0, a1, .. } => arc_steps(r, (a1 - a0).abs(), tol),
            Seg::Ellipse { u, v, t0, t1, .. } => arc_steps(u.length().max(v.length()), (t1 - t0).abs(), tol),
            Seg::Quad([a, b, c]) => {
                let d = (a - b * 2.0 + c).length();
                ((d / (4.0 * tol)).sqrt().ceil() as usize).clamp(1, 256)
            }
            Seg::Cubic([a, b, c, d]) => {
                let dd = (a - b * 2.0 + c).length().max((b - c * 2.0 + d).length());
                ((3.0 * dd / (4.0 * tol)).sqrt().ceil() as usize).clamp(1, 256)
            }
        };
        (0..=n).map(|i| self.point(i as f64 / n as f64)).collect()
    }

    /// Bounding box of the flattened segment.
    pub fn bounds(&self, tol: f64) -> (DVec2, DVec2) {
        let pts = self.flatten(tol);
        pts.iter().fold((pts[0], pts[0]), |(lo, hi), p| (lo.min(*p), hi.max(*p)))
    }

    /// Short name for messages.
    pub fn kind(&self) -> &'static str {
        match self {
            Seg::Line { .. } => "line",
            Seg::Arc { .. } => "arc",
            Seg::Quad(_) => "quad",
            Seg::Cubic(_) => "cubic",
            Seg::Ellipse { .. } => "ellipse",
        }
    }
}

fn arc_steps(r: f64, sweep: f64, tol: f64) -> usize {
    let c = (1.0 - tol / r.max(1e-12)).clamp(-1.0, 1.0);
    let step = (2.0 * c.acos()).max(1e-3);
    ((sweep / step).ceil() as usize).clamp(1, 1024)
}

/// The elliptic arc `c + u cos t + v sin t`, as a circular arc when it is one.
fn ellipse_or_circle(c: DVec2, u: DVec2, v: DVec2, t0: f64, t1: f64) -> Seg {
    let (lu, lv) = (u.length(), v.length());
    let s = lu.max(lv).max(1e-300);
    if (lu - lv).abs() <= 1e-9 * s && u.dot(v).abs() <= 1e-9 * s * s && lu > 0.0 {
        let phi = u.y.atan2(u.x);
        if u.perp_dot(v) > 0.0 {
            // v = rot90(u): c + r (cos(t + φ), sin(t + φ))
            return Seg::Arc { center: c, r: lu, a0: t0 + phi, a1: t1 + phi };
        }
        // v = -rot90(u): c + r (cos(φ - t), sin(φ - t))
        return Seg::Arc { center: c, r: lu, a0: phi - t0, a1: phi - t1 };
    }
    Seg::Ellipse { center: c, u, v, t0, t1 }
}

/// Connected segments; `closed` contours end where they start.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Contour {
    pub segs: Vec<Seg>,
    pub closed: bool,
}

impl Contour {
    pub fn transform(&self, m: &DAffine2) -> Contour {
        Contour { segs: self.segs.iter().map(|s| s.transform(m)).collect(), closed: self.closed }
    }

    pub fn reversed(&self) -> Contour {
        Contour { segs: self.segs.iter().rev().map(|s| s.reversed()).collect(), closed: self.closed }
    }

    /// Flattened contour (closed ones without repeating the start point).
    pub fn polyline(&self, tol: f64) -> Vec<DVec2> {
        let mut out: Vec<DVec2> = Vec::new();
        for s in &self.segs {
            let pts = s.flatten(tol);
            let skip = usize::from(!out.is_empty());
            out.extend_from_slice(&pts[skip..]);
        }
        if self.closed && out.len() > 1 && out[0].distance(out[out.len() - 1]) <= 1e-12 * (1.0 + out[0].length()) {
            out.pop();
        }
        out
    }

    /// Signed area (counter-clockwise positive) of the flattened contour.
    pub fn signed_area(&self, tol: f64) -> f64 {
        poly_area(&self.polyline(tol))
    }

    pub fn bounds(&self, tol: f64) -> Option<(DVec2, DVec2)> {
        bounds_of(self.polyline(tol).iter().copied())
    }
}

/// Bounds of points (`None` if there are none).
pub fn bounds_of(pts: impl IntoIterator<Item = DVec2>) -> Option<(DVec2, DVec2)> {
    pts.into_iter().fold(None, |b, p| {
        Some(match b {
            None => (p, p),
            Some((lo, hi)) => (lo.min(p), hi.max(p)),
        })
    })
}

/// Fill rule of a region bounded by several contours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FillRule {
    #[default]
    NonZero,
    EvenOdd,
}

impl FillRule {
    pub fn filled(self, winding: i32) -> bool {
        match self {
            FillRule::NonZero => winding != 0,
            FillRule::EvenOdd => winding % 2 != 0,
        }
    }
}

/// Signed area of a closed polygon (counter-clockwise positive).
pub fn poly_area(p: &[DVec2]) -> f64 {
    let n = p.len();
    if n < 3 {
        return 0.0;
    }
    (0..n).map(|i| p[i].perp_dot(p[(i + 1) % n])).sum::<f64>() / 2.0
}

/// Winding number of the closed polygon `poly` around `p`.
pub fn winding(poly: &[DVec2], p: DVec2) -> i32 {
    let n = poly.len();
    let mut w = 0;
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        let side = (b - a).perp_dot(p - a);
        if a.y <= p.y {
            if b.y > p.y && side > 0.0 {
                w += 1;
            }
        } else if b.y <= p.y && side < 0.0 {
            w -= 1;
        }
    }
    w
}

/// Size measure of contours (largest coordinate magnitude), for relative tolerances.
pub fn extent(contours: &[Contour]) -> f64 {
    let mut s: f64 = 0.0;
    for c in contours {
        for g in &c.segs {
            s = s.max(g.start().abs().max_element()).max(g.end().abs().max_element());
        }
    }
    s.max(1e-9)
}

/// Orients the closed contours of one filled region (filled by `rule`) so that the region lies
/// on the left of every contour: outer boundaries counter-clockwise, holes clockwise, islands
/// in holes counter-clockwise again. Contours with the region on both sides or on neither side
/// (e.g. a second, same-direction copy under `nonzero`) and contours without area are dropped.
/// The result fills the same region under the nonzero rule as long as no two contours cross
/// (glyph outlines, SVG shapes in practice). Open contours are dropped.
pub fn normalize(contours: &[Contour], rule: FillRule) -> Vec<Contour> {
    let size = extent(contours);
    let tol = size * 1e-5;
    let closed: Vec<&Contour> = contours.iter().filter(|c| c.closed && !c.segs.is_empty()).collect();
    let polys: Vec<Vec<DVec2>> = closed.iter().map(|c| c.polyline(tol)).collect();
    let filled = |p: DVec2| rule.filled(polys.iter().map(|q| winding(q, p)).sum());
    let mut out = Vec::new();
    for (c, poly) in closed.iter().zip(&polys) {
        let n = poly.len();
        if n < 3 || poly_area(poly).abs() <= size * size * 1e-14 {
            continue;
        }
        // test points left and right of the longest edge
        let k = (0..n)
            .max_by(|&a, &b| {
                let la = poly[a].distance_squared(poly[(a + 1) % n]);
                let lb = poly[b].distance_squared(poly[(b + 1) % n]);
                la.total_cmp(&lb)
            })
            .unwrap_or(0);
        let (a, b) = (poly[k], poly[(k + 1) % n]);
        let d = (b - a).normalize_or_zero();
        let left = DVec2::new(-d.y, d.x);
        let delta = (a.distance(b) * 1e-3).min(size * 1e-6).max(size * 1e-12);
        let mid = (a + b) / 2.0;
        match (filled(mid + left * delta), filled(mid - left * delta)) {
            (true, false) => out.push((*c).clone()),
            (false, true) => out.push(c.reversed()),
            _ => {}
        }
    }
    out
}

/// Area of a region given by normalised contours (sum of signed areas).
pub fn region_area(contours: &[Contour], tol: f64) -> f64 {
    contours.iter().filter(|c| c.closed).map(|c| c.signed_area(tol)).sum()
}

/// Full circle as one closed contour of two half arcs (counter-clockwise).
pub fn circle(center: DVec2, r: f64) -> Contour {
    Contour {
        segs: vec![Seg::Arc { center, r, a0: 0.0, a1: PI }, Seg::Arc { center, r, a0: PI, a1: TAU }],
        closed: true,
    }
}

/// Polygon through `pts` as line segments (`closed`: back to the first point).
pub fn polygon(pts: &[DVec2], closed: bool) -> Contour {
    let mut segs: Vec<Seg> = pts.windows(2).filter(|w| w[0] != w[1]).map(|w| Seg::Line { a: w[0], b: w[1] }).collect();
    if closed && pts.len() > 2 && pts[0] != pts[pts.len() - 1] {
        segs.push(Seg::Line { a: pts[pts.len() - 1], b: pts[0] });
    }
    Contour { segs, closed }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x0: f64, y0: f64, s: f64, ccw: bool) -> Contour {
        let mut p =
            vec![DVec2::new(x0, y0), DVec2::new(x0 + s, y0), DVec2::new(x0 + s, y0 + s), DVec2::new(x0, y0 + s)];
        if !ccw {
            p.reverse();
        }
        polygon(&p, true)
    }

    #[test]
    fn circle_area_and_mirror() {
        let c = circle(DVec2::new(1.0, 2.0), 3.0);
        assert!((c.signed_area(1e-6) - PI * 9.0).abs() < 1e-4);
        let m = c.transform(&DAffine2::from_scale(DVec2::new(1.0, -1.0)));
        assert!(m.segs.iter().all(|s| matches!(s, Seg::Arc { .. })), "mirror keeps arcs");
        assert!((m.signed_area(1e-6) + PI * 9.0).abs() < 1e-4);
        let e = c.transform(&DAffine2::from_scale(DVec2::new(2.0, 1.0)));
        assert!(e.segs.iter().all(|s| matches!(s, Seg::Ellipse { .. })));
        assert!((e.signed_area(1e-6) - PI * 18.0).abs() < 1e-3);
        let r = c.transform(&DAffine2::from_angle(0.7));
        assert!(r.segs.iter().all(|s| matches!(s, Seg::Arc { .. })));
        assert!(r.segs[0].start().distance(DAffine2::from_angle(0.7).transform_point2(DVec2::new(4.0, 2.0))) < 1e-12);
    }

    #[test]
    fn normalize_rules() {
        // two nested squares in the same direction
        let cs = vec![square(0.0, 0.0, 10.0, true), square(2.0, 2.0, 2.0, true)];
        let nz = normalize(&cs, FillRule::NonZero);
        assert_eq!(nz.len(), 1, "inner same-direction square drops out under nonzero");
        assert!((region_area(&nz, 1e-6) - 100.0).abs() < 1e-9);
        let eo = normalize(&cs, FillRule::EvenOdd);
        assert_eq!(eo.len(), 2);
        assert!((region_area(&eo, 1e-6) - 96.0).abs() < 1e-9);
        // clockwise outer square: reoriented
        let cw = normalize(&[square(0.0, 0.0, 1.0, false)], FillRule::NonZero);
        assert!(cw[0].signed_area(1e-6) > 0.0);
        // island in a hole (evenodd)
        let three = vec![square(0.0, 0.0, 10.0, true), square(1.0, 1.0, 8.0, true), square(3.0, 3.0, 2.0, true)];
        let eo = normalize(&three, FillRule::EvenOdd);
        assert!((region_area(&eo, 1e-6) - (100.0 - 64.0 + 4.0)).abs() < 1e-9);
    }
}
