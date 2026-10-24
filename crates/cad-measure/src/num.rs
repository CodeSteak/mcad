//! Numeric helpers: 1D minimisation, Gauss–Legendre quadrature, small vector utilities.

use glam::DVec3;

/// Minimises `f` over `[a, b]`: `n` samples, then golden-section search around the best one.
/// Returns `(t, f(t))`.
pub fn minimise(a: f64, b: f64, n: usize, f: impl Fn(f64) -> f64) -> (f64, f64) {
    let n = n.max(4);
    let mut best = (a, f(a));
    for i in 1..=n {
        let t = a + (b - a) * i as f64 / n as f64;
        let v = f(t);
        if v < best.1 {
            best = (t, v);
        }
    }
    let h = (b - a) / n as f64;
    let (mut lo, mut hi) = ((best.0 - h).max(a.min(b)), (best.0 + h).min(a.max(b)));
    let g = 0.5 * (5f64.sqrt() - 1.0);
    let (mut x1, mut x2) = (hi - g * (hi - lo), lo + g * (hi - lo));
    let (mut f1, mut f2) = (f(x1), f(x2));
    for _ in 0..80 {
        if f1 < f2 {
            hi = x2;
            x2 = x1;
            f2 = f1;
            x1 = hi - g * (hi - lo);
            f1 = f(x1);
        } else {
            lo = x1;
            x1 = x2;
            f1 = f2;
            x2 = lo + g * (hi - lo);
            f2 = f(x2);
        }
        if (hi - lo).abs() <= 1e-15 * (1.0 + lo.abs()) {
            break;
        }
    }
    let t = 0.5 * (lo + hi);
    let v = f(t);
    if v < best.1 { (t, v) } else { best }
}

/// 8-point Gauss–Legendre nodes on [-1, 1] and weights.
const GL8: [(f64, f64); 8] = [
    (-0.960_289_856_497_536_2, 0.101_228_536_290_376_26),
    (-0.796_666_477_413_626_7, 0.222_381_034_453_374_47),
    (-0.525_532_409_916_329, 0.313_706_645_877_887_3),
    (-0.183_434_642_495_649_8, 0.362_683_783_378_362),
    (0.183_434_642_495_649_8, 0.362_683_783_378_362),
    (0.525_532_409_916_329, 0.313_706_645_877_887_3),
    (0.796_666_477_413_626_7, 0.222_381_034_453_374_47),
    (0.960_289_856_497_536_2, 0.101_228_536_290_376_26),
];

/// ∫ f over [a, b] with `pieces` Gauss–Legendre panels of 8 points.
pub fn integrate(a: f64, b: f64, pieces: usize, f: impl Fn(f64) -> f64) -> f64 {
    let pieces = pieces.max(1);
    let h = (b - a) / pieces as f64;
    let mut sum = 0.0;
    for k in 0..pieces {
        let (lo, hi) = (a + h * k as f64, a + h * (k + 1) as f64);
        let (m, r) = (0.5 * (lo + hi), 0.5 * (hi - lo));
        for (x, w) in GL8 {
            sum += w * r * f(m + r * x);
        }
    }
    sum
}

/// Unit vector perpendicular to `n` (stable choice).
pub fn perp(n: DVec3) -> DVec3 {
    n.any_orthonormal_vector()
}

/// Angle between two vectors in radians (0..π), robust near 0 and π.
pub fn angle_between(a: DVec3, b: DVec3) -> f64 {
    a.cross(b).length().atan2(a.dot(b))
}

/// Rounds tiny values to 0 and the rest to 12 significant digits (stable output).
pub fn clean(v: f64) -> f64 {
    if !v.is_finite() {
        return v;
    }
    if v.abs() < 1e-12 {
        return 0.0;
    }
    let e = v.abs().log10().floor() as i32;
    let s = 10f64.powi(11 - e);
    (v * s).round() / s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quadrature_and_minimum() {
        let v = integrate(0.0, std::f64::consts::PI, 4, |x| x.sin());
        assert!((v - 2.0).abs() < 1e-13, "{v}");
        let (t, f) = minimise(-3.0, 5.0, 32, |x| (x - 1.25) * (x - 1.25) + 0.5);
        assert!((t - 1.25).abs() < 1e-7 && (f - 0.5).abs() < 1e-13, "{t} {f}");
        assert_eq!(clean(1e-14), 0.0);
        assert_eq!(clean(2.999_999_999_999_9), 3.0);
        assert!((angle_between(DVec3::X, DVec3::Y) - std::f64::consts::FRAC_PI_2).abs() < 1e-15);
    }
}
