//! Degree trigonometry (exact at special angles, like OpenSCAD) and transforms.

use glam::{DMat3, DMat4, DVec3, DVec4};

fn reduce(x: f64) -> f64 {
    if (0.0..360.0).contains(&x) {
        x
    } else {
        let r = x % 360.0;
        if r < 0.0 { r + 360.0 } else { r }
    }
}

pub fn sin_deg(x: f64) -> f64 {
    if !x.is_finite() {
        return f64::NAN;
    }
    let mut x = reduce(x);
    let oppose = x >= 180.0;
    if oppose {
        x -= 180.0;
    }
    if x > 90.0 {
        x = 180.0 - x;
    }
    let y = if x == 0.0 {
        0.0
    } else if x == 30.0 {
        0.5
    } else if x == 90.0 {
        1.0
    } else {
        x.to_radians().sin()
    };
    if oppose { -y } else { y }
}

pub fn cos_deg(x: f64) -> f64 {
    if !x.is_finite() {
        return f64::NAN;
    }
    let mut x = reduce(x);
    let mut oppose = x >= 180.0;
    if oppose {
        x -= 180.0;
    }
    if x > 90.0 {
        x = 180.0 - x;
        oppose = !oppose;
    }
    let y = if x == 0.0 {
        1.0
    } else if x == 60.0 {
        0.5
    } else if x == 90.0 {
        0.0
    } else {
        x.to_radians().cos()
    };
    if oppose { -y } else { y }
}

pub fn tan_deg(x: f64) -> f64 {
    if !x.is_finite() {
        return f64::NAN;
    }
    let r = reduce(x) % 180.0;
    if r == 0.0 {
        0.0
    } else if r == 45.0 {
        1.0
    } else if r == 135.0 {
        -1.0
    } else if r == 90.0 {
        f64::INFINITY
    } else {
        x.to_radians().tan()
    }
}

pub fn rot_x(a: f64) -> DMat4 {
    let (s, c) = (sin_deg(a), cos_deg(a));
    DMat4::from_cols(DVec4::new(1.0, 0.0, 0.0, 0.0), DVec4::new(0.0, c, s, 0.0), DVec4::new(0.0, -s, c, 0.0), DVec4::W)
}

pub fn rot_y(a: f64) -> DMat4 {
    let (s, c) = (sin_deg(a), cos_deg(a));
    DMat4::from_cols(DVec4::new(c, 0.0, -s, 0.0), DVec4::new(0.0, 1.0, 0.0, 0.0), DVec4::new(s, 0.0, c, 0.0), DVec4::W)
}

pub fn rot_z(a: f64) -> DMat4 {
    let (s, c) = (sin_deg(a), cos_deg(a));
    DMat4::from_cols(DVec4::new(c, s, 0.0, 0.0), DVec4::new(-s, c, 0.0, 0.0), DVec4::new(0.0, 0.0, 1.0, 0.0), DVec4::W)
}

/// OpenSCAD `rotate([ax, ay, az])` = Rz · Ry · Rx.
pub fn rot_euler(a: DVec3) -> DMat4 {
    rot_z(a.z) * rot_y(a.y) * rot_x(a.x)
}

/// Rotation by `a` degrees about axis `v` (OpenSCAD `rotate(a, v)`).
pub fn rot_axis(a: f64, v: DVec3) -> DMat4 {
    let len = v.length();
    if len == 0.0 || !len.is_finite() {
        return DMat4::IDENTITY;
    }
    let u = v / len;
    let (s, c) = (sin_deg(a), cos_deg(a));
    let t = 1.0 - c;
    let m = DMat3::from_cols(
        DVec3::new(t * u.x * u.x + c, t * u.x * u.y + s * u.z, t * u.x * u.z - s * u.y),
        DVec3::new(t * u.x * u.y - s * u.z, t * u.y * u.y + c, t * u.y * u.z + s * u.x),
        DVec3::new(t * u.x * u.z + s * u.y, t * u.y * u.z - s * u.x, t * u.z * u.z + c),
    );
    DMat4::from_mat3(m)
}

/// Rotation that turns +z onto `v` (shortest arc). `rotate(to = v)`.
pub fn rot_to(v: DVec3) -> DMat4 {
    let len = v.length();
    if len == 0.0 || !len.is_finite() {
        return DMat4::IDENTITY;
    }
    let d = v / len;
    let z = DVec3::Z;
    let c = z.dot(d);
    if c > 1.0 - 1e-15 {
        return DMat4::IDENTITY;
    }
    if c < -1.0 + 1e-15 {
        return rot_x(180.0);
    }
    let axis = z.cross(d);
    let angle = c.clamp(-1.0, 1.0).acos().to_degrees();
    rot_axis(angle, axis)
}

/// Reflection at the plane through the origin with normal `n`.
pub fn mirror(n: DVec3) -> DMat4 {
    let l2 = n.length_squared();
    if l2 == 0.0 || !l2.is_finite() {
        return DMat4::IDENTITY;
    }
    let m = DMat3::IDENTITY - DMat3::from_cols(n * n.x, n * n.y, n * n.z) * (2.0 / l2);
    DMat4::from_mat3(m)
}

/// Reflection at the plane through `p` with normal `n`.
pub fn mirror_at(p: DVec3, n: DVec3) -> DMat4 {
    DMat4::from_translation(p) * mirror(n) * DMat4::from_translation(-p)
}

/// Uniform scale factor of an orthogonal-times-uniform-scale matrix, `None` if the
/// linear part is sheared or scaled non-uniformly.
pub fn uniform_scale(m: &DMat4) -> Option<f64> {
    let c = [m.x_axis.truncate(), m.y_axis.truncate(), m.z_axis.truncate()];
    let l = [c[0].length(), c[1].length(), c[2].length()];
    let s = (l[0] + l[1] + l[2]) / 3.0;
    if s == 0.0 || !s.is_finite() {
        return None;
    }
    let tol = 1e-9 * s.max(1.0);
    let tol2 = 1e-9 * s * s;
    if (l[0] - s).abs() > tol || (l[1] - s).abs() > tol || (l[2] - s).abs() > tol {
        return None;
    }
    if c[0].dot(c[1]).abs() > tol2 || c[0].dot(c[2]).abs() > tol2 || c[1].dot(c[2]).abs() > tol2 {
        return None;
    }
    Some(s)
}

/// Transforms a direction (normal-safe for orthogonal-times-uniform matrices) and
/// normalises it.
pub fn transform_dir(m: &DMat4, d: DVec3) -> DVec3 {
    m.transform_vector3(d).normalize_or_zero()
}

/// Any unit vector perpendicular to `d`.
pub fn perpendicular(d: DVec3) -> DVec3 {
    let d = d.normalize_or_zero();
    let a = if d.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
    a.cross(d).cross(d).normalize_or_zero() * -1.0
}

/// 4x4 matrix from OpenSCAD row-major rows (3x4 or 4x4, missing values identity).
pub fn from_rows(rows: &[Vec<f64>]) -> DMat4 {
    let mut m = [[0.0; 4]; 4];
    for (i, r) in m.iter_mut().enumerate() {
        r[i] = 1.0;
    }
    for (i, row) in rows.iter().take(4).enumerate() {
        for (j, v) in row.iter().take(4).enumerate() {
            m[i][j] = *v;
        }
    }
    DMat4::from_cols_array_2d(&m).transpose()
}
