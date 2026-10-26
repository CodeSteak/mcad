//! SVG path data (`d`): M, L, H, V, C, S, Q, T, A, Z, absolute and relative, with implicit
//! repetitions. Coordinates stay in user units; arcs become exact circular or elliptic arcs.

use cad_i18n::{tr, trs};
use cad_outline::{Contour, Seg};
use glam::DVec2;
use std::f64::consts::TAU;

struct Lexer<'a> {
    s: &'a [u8],
    i: usize,
}

impl Lexer<'_> {
    fn skip_ws(&mut self) {
        while self.i < self.s.len() && (self.s[self.i].is_ascii_whitespace() || self.s[self.i] == b',') {
            self.i += 1;
        }
    }

    fn peek_cmd(&mut self) -> Option<u8> {
        self.skip_ws();
        let c = *self.s.get(self.i)?;
        c.is_ascii_alphabetic().then_some(c).filter(|c| !matches!(c, b'e' | b'E'))
    }

    fn at_number(&mut self) -> bool {
        self.skip_ws();
        matches!(self.s.get(self.i), Some(c) if c.is_ascii_digit() || matches!(c, b'-' | b'+' | b'.'))
    }

    fn number(&mut self) -> Result<f64, String> {
        self.skip_ws();
        let st = self.i;
        let s = self.s;
        let mut i = self.i;
        if i < s.len() && matches!(s[i], b'-' | b'+') {
            i += 1;
        }
        let mut digits = false;
        while i < s.len() && s[i].is_ascii_digit() {
            i += 1;
            digits = true;
        }
        if i < s.len() && s[i] == b'.' {
            i += 1;
            while i < s.len() && s[i].is_ascii_digit() {
                i += 1;
                digits = true;
            }
        }
        if digits && i < s.len() && matches!(s[i], b'e' | b'E') {
            let mut j = i + 1;
            if j < s.len() && matches!(s[j], b'-' | b'+') {
                j += 1;
            }
            if j < s.len() && s[j].is_ascii_digit() {
                while j < s.len() && s[j].is_ascii_digit() {
                    j += 1;
                }
                i = j;
            }
        }
        if !digits {
            return Err(tr!("Zahl erwartet an Stelle {st}", "number expected at position {st}"));
        }
        self.i = i;
        std::str::from_utf8(&s[st..i])
            .ok()
            .and_then(|t| t.parse().ok())
            .ok_or_else(|| tr!("ungültige Zahl an Stelle {st}", "invalid number at position {st}"))
    }

    /// Arc flags may be written without separators (`a10 10 0 0110 10`).
    fn flag(&mut self) -> Result<bool, String> {
        self.skip_ws();
        match self.s.get(self.i) {
            Some(b'0') => {
                self.i += 1;
                Ok(false)
            }
            Some(b'1') => {
                self.i += 1;
                Ok(true)
            }
            _ => Err(tr!(
                "Bogen-Flag (0 oder 1) erwartet an Stelle {}",
                "arc flag (0 or 1) expected at position {}",
                self.i
            )),
        }
    }

    fn point(&mut self) -> Result<DVec2, String> {
        Ok(DVec2::new(self.number()?, self.number()?))
    }
}

/// Subpaths of a path's data in user units. Subpaths closed by `Z`, or ending where they
/// start, are closed. Errors name the offending position; what parsed before stays (as SVG
/// renderers do), the error is returned alongside.
pub fn parse_path(d: &str) -> (Vec<Contour>, Option<String>) {
    let mut out = Vec::new();
    let mut cur = Contour::default();
    let mut start = DVec2::ZERO;
    let err = parse_into(d, &mut out, &mut cur, &mut start).err();
    finish(&mut cur, start, &mut out);
    (out, err)
}

fn finish(cur: &mut Contour, start: DVec2, out: &mut Vec<Contour>) {
    if cur.segs.is_empty() {
        cur.closed = false;
        return;
    }
    let end = cur.segs.last().map(|s| s.end()).unwrap_or(start);
    let scale = 1.0 + start.abs().max_element();
    if !cur.closed && end.distance(start) <= 1e-9 * scale && cur.segs.len() > 1 {
        cur.closed = true;
    }
    if cur.closed && end.distance(start) > 1e-12 * scale {
        cur.segs.push(Seg::Line { a: end, b: start });
    }
    out.push(std::mem::take(cur));
}

fn parse_into(d: &str, out: &mut Vec<Contour>, cur: &mut Contour, start: &mut DVec2) -> Result<(), String> {
    let mut lx = Lexer { s: d.as_bytes(), i: 0 };
    let mut p = DVec2::ZERO;
    // last control point for S / T reflection
    let mut last_c2: Option<DVec2> = None;
    let mut last_q: Option<DVec2> = None;
    let mut cmd: Option<u8> = None;
    loop {
        let c = match lx.peek_cmd() {
            Some(c) => {
                lx.i += 1;
                c
            }
            None => {
                lx.skip_ws();
                if lx.i >= lx.s.len() {
                    break;
                }
                match cmd {
                    // implicit repetition; after M/m the repetitions are L/l
                    Some(b'M') if lx.at_number() => b'L',
                    Some(b'm') if lx.at_number() => b'l',
                    Some(c) if lx.at_number() && !matches!(c, b'Z' | b'z') => c,
                    _ => {
                        return Err(tr!(
                            "unerwartetes Zeichen `{}` an Stelle {}",
                            "unexpected character `{}` at position {}",
                            lx.s[lx.i] as char,
                            lx.i
                        ));
                    }
                }
            }
        };
        if cmd.is_none() && !matches!(c, b'M' | b'm') {
            return Err(trs!("Pfad muss mit M beginnen", "path must start with M").into());
        }
        cmd = Some(c);
        let rel = c.is_ascii_lowercase();
        let base = if rel { p } else { DVec2::ZERO };
        let mut c2 = None;
        let mut q = None;
        match c.to_ascii_uppercase() {
            b'M' => {
                finish(cur, *start, out);
                p = base + lx.point()?;
                *start = p;
            }
            b'Z' => {
                cur.closed = true;
                finish(cur, *start, out);
                p = *start;
            }
            b'L' => {
                let b = base + lx.point()?;
                push_line(cur, p, b);
                p = b;
            }
            b'H' => {
                let x = lx.number()? + if rel { p.x } else { 0.0 };
                let b = DVec2::new(x, p.y);
                push_line(cur, p, b);
                p = b;
            }
            b'V' => {
                let y = lx.number()? + if rel { p.y } else { 0.0 };
                let b = DVec2::new(p.x, y);
                push_line(cur, p, b);
                p = b;
            }
            b'C' | b'S' => {
                let p1 =
                    if c.eq_ignore_ascii_case(&b'C') { base + lx.point()? } else { last_c2.map_or(p, |k| p * 2.0 - k) };
                let p2 = base + lx.point()?;
                let p3 = base + lx.point()?;
                cur.segs.push(Seg::Cubic([p, p1, p2, p3]));
                c2 = Some(p2);
                p = p3;
            }
            b'Q' | b'T' => {
                let p1 =
                    if c.eq_ignore_ascii_case(&b'Q') { base + lx.point()? } else { last_q.map_or(p, |k| p * 2.0 - k) };
                let p2 = base + lx.point()?;
                cur.segs.push(Seg::Quad([p, p1, p2]));
                q = Some(p1);
                p = p2;
            }
            b'A' => {
                let rx = lx.number()?;
                let ry = lx.number()?;
                let phi = lx.number()?;
                let large = lx.flag()?;
                let sweep = lx.flag()?;
                let b = base + lx.point()?;
                if let Some(s) = arc(p, b, rx, ry, phi, large, sweep) {
                    cur.segs.push(s);
                }
                p = b;
            }
            other => return Err(tr!("unbekannter Pfadbefehl `{}`", "unknown path command `{}`", other as char)),
        }
        last_c2 = c2;
        last_q = q;
    }
    Ok(())
}

fn push_line(cur: &mut Contour, a: DVec2, b: DVec2) {
    if a != b {
        cur.segs.push(Seg::Line { a, b });
    }
}

/// Endpoint arc of SVG (implementation notes F.6.5/F.6.6) as an exact arc; `None` if both
/// points coincide. Zero radii give a line.
pub fn arc(p1: DVec2, p2: DVec2, rx: f64, ry: f64, phi_deg: f64, large: bool, sweep: bool) -> Option<Seg> {
    if p1 == p2 {
        return None;
    }
    let (mut rx, mut ry) = (rx.abs(), ry.abs());
    if rx == 0.0 || ry == 0.0 {
        return Some(Seg::Line { a: p1, b: p2 });
    }
    let phi = phi_deg.to_radians();
    let (sn, cs) = phi.sin_cos();
    let h = (p1 - p2) / 2.0;
    let x1 = cs * h.x + sn * h.y;
    let y1 = -sn * h.x + cs * h.y;
    let lambda = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry);
    if lambda > 1.0 {
        let s = lambda.sqrt();
        rx *= s;
        ry *= s;
    }
    let num = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1;
    let den = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let mut co = if den > 0.0 { (num / den).max(0.0).sqrt() } else { 0.0 };
    if large == sweep {
        co = -co;
    }
    let cxp = co * rx * y1 / ry;
    let cyp = -co * ry * x1 / rx;
    let m = (p1 + p2) / 2.0;
    let center = DVec2::new(cs * cxp - sn * cyp + m.x, sn * cxp + cs * cyp + m.y);
    let ang = |u: DVec2, v: DVec2| {
        let a = u.perp_dot(v).atan2(u.dot(v));
        if a.is_finite() { a } else { 0.0 }
    };
    let u = DVec2::new((x1 - cxp) / rx, (y1 - cyp) / ry);
    let v = DVec2::new((-x1 - cxp) / rx, (-y1 - cyp) / ry);
    let t0 = ang(DVec2::X, u);
    let mut dt = ang(u, v) % TAU;
    if !sweep && dt > 0.0 {
        dt -= TAU;
    } else if sweep && dt < 0.0 {
        dt += TAU;
    }
    if dt.abs() < 1e-15 {
        return Some(Seg::Line { a: p1, b: p2 });
    }
    let ux = DVec2::new(cs, sn);
    let uy = DVec2::new(-sn, cs);
    if (rx - ry).abs() <= 1e-12 * rx.max(ry) {
        let r = (rx + ry) / 2.0;
        return Some(Seg::Arc { center, r, a0: t0 + phi, a1: t0 + phi + dt.clamp(-TAU, TAU) });
    }
    Some(Seg::Ellipse { center, u: ux * rx, v: uy * ry, t0, t1: t0 + dt })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    fn close(a: DVec2, b: DVec2) -> bool {
        a.distance(b) < 1e-9
    }

    #[test]
    fn commands_absolute_relative() {
        let (c, e) = parse_path("M10 10 h20 v20 H10 z m5 5 l2 0 0 2z");
        assert!(e.is_none());
        assert_eq!(c.len(), 2);
        assert!(c[0].closed && c[1].closed);
        assert_eq!(c[0].segs.len(), 4);
        assert!(close(c[1].segs[0].start(), DVec2::new(15.0, 15.0)), "m after z is relative to the start");
        assert!((c[0].signed_area(1e-6) - 400.0).abs() < 1e-9);
        assert!((c[1].signed_area(1e-6) - 2.0).abs() < 1e-9);
    }

    #[test]
    fn implicit_lineto_and_compact_numbers() {
        let (c, e) = parse_path("M0,0 10,0 10-10.5.5e1 10z");
        assert!(e.is_none(), "{e:?}");
        let pts: Vec<DVec2> = c[0].segs.iter().map(|s| s.end()).collect();
        assert!(close(pts[1], DVec2::new(10.0, -10.5)));
        assert!(close(pts[2], DVec2::new(5.0, 10.0)), "{pts:?}");
    }

    #[test]
    fn smooth_curves_reflect() {
        let (c, _) = parse_path("M0 0 C 0 10 10 10 10 0 S 20 -10 20 0 Q 25 5 30 0 T 40 0");
        let s = &c[0].segs;
        let Seg::Cubic(k) = s[1] else { panic!() };
        assert!(close(k[1], DVec2::new(10.0, -10.0)), "S reflects the second control point");
        let Seg::Quad(q) = s[3] else { panic!() };
        assert!(close(q[1], DVec2::new(35.0, -5.0)), "T reflects the control point");
        assert!(!c[0].closed);
    }

    #[test]
    fn arcs_exact() {
        // half circle r = 5 from (0,0) to (10,0)
        let (c, _) = parse_path("M0 0 A5 5 0 0 1 10 0");
        let Seg::Arc { center, r, a0, a1 } = c[0].segs[0] else { panic!("{:?}", c[0].segs[0]) };
        assert!(close(center, DVec2::new(5.0, 0.0)) && (r - 5.0).abs() < 1e-12);
        assert!(((a1 - a0).abs() - PI).abs() < 1e-12);
        // radius too small: scaled up to the half chord
        let (c, _) = parse_path("M0 0 A1 1 0 0 0 10 0");
        let Seg::Arc { r, .. } = c[0].segs[0] else { panic!() };
        assert!((r - 5.0).abs() < 1e-12);
        // ellipse, compact flags
        let (c, e) = parse_path("M0 0a10 5 0 1010 0");
        assert!(e.is_none(), "{e:?}");
        let Seg::Ellipse { .. } = c[0].segs[0] else { panic!("{:?}", c[0].segs[0]) };
        assert!(close(c[0].segs[0].end(), DVec2::new(10.0, 0.0)));
        // large and sweep flags pick the four arcs
        for (l, s) in [(false, false), (false, true), (true, false), (true, true)] {
            let a = arc(DVec2::ZERO, DVec2::new(10.0, 0.0), 10.0, 10.0, 0.0, l, s).unwrap();
            let Seg::Arc { a0, a1, .. } = a else { panic!() };
            assert_eq!((a1 - a0).abs() > PI, l);
            assert_eq!(a1 > a0, s);
            assert!(close(a.end(), DVec2::new(10.0, 0.0)));
        }
    }

    #[test]
    fn errors() {
        assert!(parse_path("L 1 2").1.is_some());
        let (c, e) = parse_path("M0 0 L10 0 L 10 x");
        assert!(e.is_some());
        assert_eq!(c.len(), 1, "what parsed stays");
        assert!(parse_path("M0 0 A 1 1 0 2 0 3 3").1.is_some(), "bad flag");
    }
}
