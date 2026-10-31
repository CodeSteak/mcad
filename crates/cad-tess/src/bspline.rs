//! B-spline evaluation with derivatives (Piegl & Tiller, "The NURBS Book", A2.1, A2.3, A3.2,
//! A4.2, A4.4) for the neutral curves and surfaces, rational or not.

use cad_kernel::brep::{BSplineCurve, BSplineSurface};
use glam::DVec3;

/// Knot span `k` with `knots[k] <= t < knots[k + 1]` among the spans of the domain
/// (`degree <= k < n`, `n` control points), `t` clamped into the domain.
pub(crate) fn find_span(knots: &[f64], degree: usize, n: usize, t: f64) -> usize {
    if t >= knots[n] {
        let mut k = n - 1;
        while k > degree && knots[k] >= knots[n] {
            k -= 1;
        }
        return k;
    }
    if t <= knots[degree] {
        let mut k = degree;
        while k + 1 < n && knots[k + 1] <= knots[degree] {
            k += 1;
        }
        return k;
    }
    let (mut lo, mut hi) = (degree, n);
    while hi - lo > 1 {
        let mid = (lo + hi) / 2;
        if t < knots[mid] { hi = mid } else { lo = mid }
    }
    lo
}

/// Largest degree + 1 handled (the kernels write degree 7 at most).
pub(crate) const MAXP: usize = 10;

/// Non-zero basis functions of span `k` at `t` and their derivatives up to order `nd`
/// (`out[d][j]` is the `d`-th derivative of `N_{k-p+j}`), A2.3.
fn basis_ders(knots: &[f64], k: usize, t: f64, p: usize, nd: usize) -> [[f64; MAXP]; 3] {
    let mut ndu = [[0.0f64; MAXP]; MAXP];
    let mut left = [0.0f64; MAXP];
    let mut right = [0.0f64; MAXP];
    ndu[0][0] = 1.0;
    for j in 1..=p {
        left[j] = t - knots[k + 1 - j];
        right[j] = knots[k + j] - t;
        let mut saved = 0.0;
        for r in 0..j {
            ndu[j][r] = right[r + 1] + left[j - r];
            let tmp = if ndu[j][r] != 0.0 { ndu[r][j - 1] / ndu[j][r] } else { 0.0 };
            ndu[r][j] = saved + right[r + 1] * tmp;
            saved = left[j - r] * tmp;
        }
        ndu[j][j] = saved;
    }
    let mut ders = [[0.0f64; MAXP]; 3];
    for j in 0..=p {
        ders[0][j] = ndu[j][p];
    }
    let nd = nd.min(p).min(2);
    let mut a = [[0.0f64; MAXP]; 2];
    for r in 0..=p {
        let (mut s1, mut s2) = (0usize, 1usize);
        a[0] = [0.0; MAXP];
        a[0][0] = 1.0;
        for kk in 1..=nd {
            let mut d = 0.0;
            let rk = r as isize - kk as isize;
            let pk = p - kk;
            if r >= kk {
                let den = ndu[pk + 1][rk as usize];
                a[s2][0] = if den != 0.0 { a[s1][0] / den } else { 0.0 };
                d = a[s2][0] * ndu[rk as usize][pk];
            }
            let j1 = if rk >= -1 { 1 } else { (-rk) as usize };
            let j2 = if r as isize - 1 <= pk as isize { kk - 1 } else { p - r };
            for j in j1..=j2 {
                let idx = (rk + j as isize) as usize;
                let den = ndu[pk + 1][idx];
                a[s2][j] = if den != 0.0 { (a[s1][j] - a[s1][j - 1]) / den } else { 0.0 };
                d += a[s2][j] * ndu[idx][pk];
            }
            if r <= pk {
                let den = ndu[pk + 1][r];
                a[s2][kk] = if den != 0.0 { -a[s1][kk - 1] / den } else { 0.0 };
                d += a[s2][kk] * ndu[r][pk];
            }
            ders[kk][r] = d;
            std::mem::swap(&mut s1, &mut s2);
        }
    }
    let mut f = p as f64;
    for (kk, row) in ders.iter_mut().enumerate().take(nd + 1).skip(1) {
        for x in row.iter_mut().take(p + 1) {
            *x *= f;
        }
        f *= (p - kk) as f64;
    }
    ders
}

/// Homogeneous point `(w·x, w·y, w·z, w)`.
type H = [f64; 4];

fn h_add(a: &mut H, b: &H, s: f64) {
    for c in 0..4 {
        a[c] += b[c] * s;
    }
}

/// Curve with its homogeneous control points, for repeated evaluation.
#[derive(Debug, Clone)]
pub(crate) struct Curve {
    pub degree: usize,
    pub knots: Vec<f64>,
    pw: Vec<H>,
    rational: bool,
}

impl Curve {
    pub fn new(c: &BSplineCurve) -> Curve {
        let pw = c.points.iter().enumerate().map(|(i, p)| homog(*p, c.weight(i))).collect();
        Curve { degree: c.degree as usize, knots: c.knots.clone(), pw, rational: c.is_rational() }
    }

    pub fn domain(&self) -> (f64, f64) {
        (self.knots[self.degree], self.knots[self.pw.len()])
    }

    /// Distinct knots inside the domain, both ends included.
    pub fn breaks(&self) -> Vec<f64> {
        let (a, b) = self.domain();
        let mut out = vec![a];
        for &k in &self.knots[self.degree + 1..self.pw.len()] {
            if k > *out.last().unwrap() && k < b {
                out.push(k);
            }
        }
        out.push(b);
        out
    }

    /// Point and derivatives up to order 2 at `t` (clamped into the domain).
    pub fn eval_d(&self, t: f64) -> [DVec3; 3] {
        let p = self.degree;
        let n = self.pw.len();
        let (a, b) = self.domain();
        let t = t.clamp(a, b);
        let k = find_span(&self.knots, p, n, t);
        let nd = basis_ders(&self.knots, k, t, p, 2);
        let mut ck = [[0.0; 4]; 3];
        for (d, row) in nd.iter().enumerate() {
            for (j, w) in row.iter().take(p + 1).enumerate() {
                h_add(&mut ck[d], &self.pw[k - p + j], *w);
            }
        }
        rational_ders(&ck, self.rational)
    }

    pub fn eval(&self, t: f64) -> DVec3 {
        self.eval_d(t)[0]
    }

    /// The curve's end points coincide (within `eps`).
    pub fn is_closed(&self, eps: f64) -> bool {
        let (a, b) = self.domain();
        (self.eval(a) - self.eval(b)).length() <= eps
    }
}

/// Derivatives of the rational curve from those of its homogeneous form, A4.2.
fn rational_ders(ck: &[H; 3], rational: bool) -> [DVec3; 3] {
    let a = |h: &H| DVec3::new(h[0], h[1], h[2]);
    if !rational {
        let w = ck[0][3];
        return [a(&ck[0]) / w, a(&ck[1]) / w, a(&ck[2]) / w];
    }
    let w0 = ck[0][3];
    let c0 = a(&ck[0]) / w0;
    let c1 = (a(&ck[1]) - c0 * ck[1][3]) / w0;
    let c2 = (a(&ck[2]) - c1 * (2.0 * ck[1][3]) - c0 * ck[2][3]) / w0;
    [c0, c1, c2]
}

fn homog(p: DVec3, w: f64) -> H {
    [p.x * w, p.y * w, p.z * w, w]
}

/// Point and derivatives of a surface: `[S, Su, Sv, Suu, Suv, Svv]`.
pub(crate) type SurfDers = [DVec3; 6];

/// Surface with its homogeneous control points (u-major).
#[derive(Debug, Clone)]
pub(crate) struct Surface {
    pub pu: usize,
    pub pv: usize,
    pub ku: Vec<f64>,
    pub kv: Vec<f64>,
    pub nu: usize,
    pub nv: usize,
    pw: Vec<H>,
    rational: bool,
}

impl Surface {
    pub fn new(s: &BSplineSurface) -> Surface {
        let mut pw = Vec::with_capacity(s.points.len());
        for i in 0..s.nu {
            for j in 0..s.nv {
                pw.push(homog(s.points[i * s.nv + j], s.weight(i, j)));
            }
        }
        Surface {
            pu: s.degree_u as usize,
            pv: s.degree_v as usize,
            ku: s.knots_u.clone(),
            kv: s.knots_v.clone(),
            nu: s.nu,
            nv: s.nv,
            pw,
            rational: s.is_rational(),
        }
    }

    pub fn domain(&self) -> ((f64, f64), (f64, f64)) {
        ((self.ku[self.pu], self.ku[self.nu]), (self.kv[self.pv], self.kv[self.nv]))
    }

    /// Control point `(i, j)`, dehomogenised.
    pub fn point(&self, i: usize, j: usize) -> DVec3 {
        let h = self.pw[i * self.nv + j];
        DVec3::new(h[0] / h[3], h[1] / h[3], h[2] / h[3])
    }

    /// Distinct knots of u (`dir == 0`) or v inside the domain, both ends included.
    pub fn breaks(&self, dir: usize) -> Vec<f64> {
        let (k, p, n) = if dir == 0 { (&self.ku, self.pu, self.nu) } else { (&self.kv, self.pv, self.nv) };
        let (a, b) = (k[p], k[n]);
        let mut out = vec![a];
        for &x in &k[p + 1..n] {
            if x > *out.last().unwrap() && x < b {
                out.push(x);
            }
        }
        out.push(b);
        out
    }

    /// Point and derivatives up to order 2 (clamped into the domain), A3.6 / A4.4.
    #[allow(clippy::needless_range_loop)]
    pub fn eval_d(&self, u: f64, v: f64) -> SurfDers {
        let ((u0, u1), (v0, v1)) = self.domain();
        let (u, v) = (u.clamp(u0, u1), v.clamp(v0, v1));
        let (p, q) = (self.pu, self.pv);
        let ku = find_span(&self.ku, p, self.nu, u);
        let kv = find_span(&self.kv, q, self.nv, v);
        let nu = basis_ders(&self.ku, ku, u, p, 2);
        let nv = basis_ders(&self.kv, kv, v, q, 2);
        // skl[k][l]: k-th u, l-th v derivative (k + l <= 2) of the homogeneous surface
        let mut skl = [[[0.0f64; 4]; 3]; 3];
        for l in 0..=q {
            let row = (kv - q + l, nv[0][l], nv[1][l], nv[2][l]);
            let mut tmp = [[0.0f64; 4]; 3];
            for i in 0..=p {
                let h = &self.pw[(ku - p + i) * self.nv + row.0];
                for (kk, t) in tmp.iter_mut().enumerate() {
                    h_add(t, h, nu[kk][i]);
                }
            }
            for (kk, t) in tmp.iter().enumerate() {
                for (ll, w) in [row.1, row.2, row.3].iter().enumerate() {
                    if kk + ll <= 2 {
                        h_add(&mut skl[kk][ll], t, *w);
                    }
                }
            }
        }
        let a = |h: &H| DVec3::new(h[0], h[1], h[2]);
        let w = |k: usize, l: usize| skl[k][l][3];
        let s00 = a(&skl[0][0]) / w(0, 0);
        if !self.rational {
            let w0 = w(0, 0);
            return [
                s00,
                a(&skl[1][0]) / w0,
                a(&skl[0][1]) / w0,
                a(&skl[2][0]) / w0,
                a(&skl[1][1]) / w0,
                a(&skl[0][2]) / w0,
            ];
        }
        let w0 = w(0, 0);
        let su = (a(&skl[1][0]) - s00 * w(1, 0)) / w0;
        let sv = (a(&skl[0][1]) - s00 * w(0, 1)) / w0;
        let suu = (a(&skl[2][0]) - su * (2.0 * w(1, 0)) - s00 * w(2, 0)) / w0;
        let svv = (a(&skl[0][2]) - sv * (2.0 * w(0, 1)) - s00 * w(0, 2)) / w0;
        let suv = (a(&skl[1][1]) - su * w(0, 1) - sv * w(1, 0) - s00 * w(1, 1)) / w0;
        [s00, su, sv, suu, suv, svv]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn circle() -> BSplineCurve {
        let s = std::f64::consts::FRAC_1_SQRT_2;
        let pts = [
            (1.0, 0.0),
            (1.0, 1.0),
            (0.0, 1.0),
            (-1.0, 1.0),
            (-1.0, 0.0),
            (-1.0, -1.0),
            (0.0, -1.0),
            (1.0, -1.0),
            (1.0, 0.0),
        ];
        BSplineCurve {
            degree: 2,
            knots: vec![0.0, 0.0, 0.0, 1.0, 1.0, 2.0, 2.0, 3.0, 3.0, 4.0, 4.0, 4.0],
            points: pts.iter().map(|(x, y)| DVec3::new(*x, *y, 0.0)).collect(),
            weights: Some((0..9).map(|i| if i % 2 == 1 { s } else { 1.0 }).collect()),
        }
    }

    #[test]
    fn curve_derivatives_match_differences() {
        let bc = circle();
        let c = Curve::new(&bc);
        for i in 1..40 {
            let t = 4.0 * i as f64 / 40.0 + 0.013;
            let [p, d1, d2] = c.eval_d(t);
            assert!((p - bc.eval(t)).length() < 1e-12);
            assert!((p.length() - 1.0).abs() < 1e-12);
            let h = 1e-5;
            let fd1 = (c.eval(t + h) - c.eval(t - h)) / (2.0 * h);
            let fd2 = (c.eval(t + h) - 2.0 * p + c.eval(t - h)) / (h * h);
            assert!((d1 - fd1).length() < 1e-6, "{d1} {fd1}");
            assert!((d2 - fd2).length() < 1e-3, "{d2} {fd2}");
        }
    }

    #[test]
    fn surface_derivatives_match_differences() {
        // rational: the circle swept along z with a bend in the control points
        let bc = circle();
        let mut points = Vec::new();
        let mut weights = Vec::new();
        for (i, p) in bc.points.iter().enumerate() {
            for j in 0..4 {
                points.push(*p * (1.0 + 0.2 * j as f64) + DVec3::new(0.0, 0.1 * (j * j) as f64, j as f64));
                weights.push(bc.weight(i) * (1.0 + 0.1 * j as f64));
            }
        }
        let bs = BSplineSurface {
            degree_u: 2,
            degree_v: 3,
            knots_u: bc.knots.clone(),
            knots_v: vec![0.0, 0.0, 0.0, 0.0, 2.0, 2.0, 2.0, 2.0],
            nu: 9,
            nv: 4,
            points,
            weights: Some(weights),
        };
        let s = Surface::new(&bs);
        let h = 1e-5;
        for (u, v) in [(0.3, 0.2), (1.7, 1.1), (3.2, 1.9), (2.5, 0.7)] {
            let d = s.eval_d(u, v);
            assert!((d[0] - bs.eval(u, v)).length() < 1e-12);
            let e = |u: f64, v: f64| s.eval_d(u, v)[0];
            let fu = (e(u + h, v) - e(u - h, v)) / (2.0 * h);
            let fv = (e(u, v + h) - e(u, v - h)) / (2.0 * h);
            let fuu = (e(u + h, v) - 2.0 * d[0] + e(u - h, v)) / (h * h);
            let fvv = (e(u, v + h) - 2.0 * d[0] + e(u, v - h)) / (h * h);
            let fuv = (e(u + h, v + h) - e(u + h, v - h) - e(u - h, v + h) + e(u - h, v - h)) / (4.0 * h * h);
            assert!((d[1] - fu).length() < 1e-6 && (d[2] - fv).length() < 1e-6);
            assert!((d[3] - fuu).length() < 1e-3 && (d[5] - fvv).length() < 1e-3 && (d[4] - fuv).length() < 1e-3);
        }
    }
}
