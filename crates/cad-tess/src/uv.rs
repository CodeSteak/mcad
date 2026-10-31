//! A face's boundary loops in its parameter plane (plan 2.2). The neutral B-rep has no UV trim
//! curves, so every boundary point is inverted onto the surface:
//! * periodic parameters are unwrapped along the loop (each point within half a period of the
//!   previous one), so a seam edge used twice ends up on both sides of the face's domain;
//! * a pole (sphere pole, cone apex, a collapsed B-spline row) has no value of its free
//!   parameter: the loop runs along the pole from the value it arrives with to the value it
//!   leaves with, in the direction that keeps the face on its side (left of the loop if the face
//!   normal is `σu × σv`, else right);
//! * inner loops are shifted by whole periods into the range of the widest loop.

use glam::DVec2;

use cad_kernel::brep::{Face, Loop};

use crate::edges::EdgePolys;
use crate::surface::{Geom, Pole};

/// A boundary point in the parameter plane.
#[derive(Debug, Clone, Copy)]
pub(crate) struct UvPt {
    pub uv: DVec2,
    /// Point id (see [`EdgePolys`]).
    pub id: u32,
    pub pole: Option<Pole>,
}

fn get(uv: DVec2, i: usize) -> f64 {
    if i == 0 { uv.x } else { uv.y }
}

fn set(uv: &mut DVec2, i: usize, t: f64) {
    if i == 0 {
        uv.x = t;
    } else {
        uv.y = t;
    }
}

/// `uv` with its periodic parameters shifted by whole periods next to `near`.
fn unwrap(g: &Geom, mut uv: DVec2, near: DVec2) -> DVec2 {
    for i in 0..2 {
        if let Some(per) = g.period[i] {
            let t = get(uv, i);
            set(&mut uv, i, t + per * ((get(near, i) - t) / per).round());
        }
    }
    uv
}

/// The loops of `face` in its parameter plane, without repeated closing points. `orient`: `1`
/// if the face normal is `σu × σv` (loops counter-clockwise in (u, v)), else `-1`. Degenerate
/// loops (a vertex loop, fewer than three distinct points) are left out.
pub(crate) fn face_loops(face: &Face, g: &Geom, ep: &EdgePolys, orient: f64) -> Result<Vec<Vec<UvPt>>, String> {
    let mut loops = Vec::new();
    for l in &face.loops {
        let Loop::Edges(es) = l else { continue };
        let mut ids: Vec<u32> = Vec::new();
        for oe in es {
            let poly = &ep.edges[oe.edge as usize];
            let mut add = |id: u32| {
                if ids.last() != Some(&id)
                    && ids.last().is_none_or(|l| ep.points[*l as usize] != ep.points[id as usize])
                {
                    ids.push(id);
                }
            };
            if oe.forward {
                poly.iter().for_each(|i| add(*i));
            } else {
                poly.iter().rev().for_each(|i| add(*i));
            }
        }
        while ids.len() > 1 && ep.points[ids[0] as usize] == ep.points[*ids.last().unwrap() as usize] {
            ids.pop();
        }
        if ids.len() < 2 {
            continue;
        }
        if let Some(lp) = one_loop(g, ep, &ids, orient)? {
            loops.push(lp);
        }
    }
    // inner loops into the range of the widest one
    if loops.len() > 1 && (g.period[0].is_some() || g.period[1].is_some()) {
        let bbox = |l: &[UvPt]| {
            l.iter().fold((DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)), |(lo, hi), p| {
                (lo.min(p.uv), hi.max(p.uv))
            })
        };
        let widest = (0..loops.len())
            .max_by(|a, b| {
                let (la, ha) = bbox(&loops[*a]);
                let (lb, hb) = bbox(&loops[*b]);
                (ha - la).length().total_cmp(&(hb - lb).length())
            })
            .unwrap_or(0);
        let (rlo, rhi) = bbox(&loops[widest]);
        let mid = (rlo + rhi) * 0.5;
        for (k, l) in loops.iter_mut().enumerate() {
            if k == widest {
                continue;
            }
            let (lo, hi) = bbox(l);
            let c = (lo + hi) * 0.5;
            let mut shift = DVec2::ZERO;
            for i in 0..2 {
                if let Some(per) = g.period[i] {
                    set(&mut shift, i, per * ((get(mid, i) - get(c, i)) / per).round());
                }
            }
            for p in l.iter_mut() {
                p.uv += shift;
            }
        }
    }
    Ok(loops)
}

fn one_loop(g: &Geom, ep: &EdgePolys, ids: &[u32], orient: f64) -> Result<Option<Vec<UvPt>>, String> {
    let n = ids.len();
    let poles: Vec<Option<Pole>> = ids.iter().map(|i| g.pole(ep.points[*i as usize])).collect();
    let Some(s) = (0..n).find(|i| poles[*i].is_none()) else { return Ok(None) };
    let mut out: Vec<UvPt> = Vec::with_capacity(n + 4);
    let mut prev: Option<DVec2> = None;
    let mut pending: Option<(u32, Pole)> = None;
    let mut end = DVec2::ZERO;
    for k in 0..=n {
        let i = (s + k) % n;
        if let Some(pl) = poles[i] {
            pending = Some((ids[i], pl));
            continue;
        }
        let p = ep.points[ids[i] as usize];
        let raw = g.invert(p, prev);
        let mut uv = match prev {
            Some(q) => unwrap(g, raw, q),
            None => raw,
        };
        if let (Some((pid, pl)), Some(a)) = (pending.take(), prev) {
            let free = pl.free;
            let fixed = 1 - free;
            if let Some(per) = g.period[free] {
                // the face's side of the pole line, then the direction along it
                let side = if get(a, fixed) > pl.value { 1.0 } else { -1.0 };
                let dir = if free == 0 { orient * side } else { -orient * side };
                let mut travel = (get(uv, free) - get(a, free)) * dir;
                travel -= per * (travel / per).floor();
                if travel <= 1e-9 * per {
                    travel += per;
                }
                let t = get(a, free) + dir * travel;
                set(&mut uv, free, t);
            }
            let mut pa = DVec2::ZERO;
            set(&mut pa, free, get(a, free));
            set(&mut pa, fixed, pl.value);
            let mut pb = pa;
            set(&mut pb, free, get(uv, free));
            out.push(UvPt { uv: pa, id: pid, pole: Some(pl) });
            out.push(UvPt { uv: pb, id: pid, pole: Some(pl) });
        }
        if k == n {
            end = uv;
        } else {
            out.push(UvPt { uv, id: ids[i], pole: None });
        }
        prev = Some(uv);
    }
    let gap = end - out[0].uv;
    for i in 0..2 {
        if let Some(per) = g.period[i]
            && (get(gap, i) / per).round() != 0.0
        {
            return Err(format!("a loop wraps around the surface without a seam ({})", ["u", "v"][i]));
        }
    }
    if gap.length() > 1e-6 * (1.0 + out[0].uv.length()) {
        return Err(format!("a loop does not close in (u, v) (gap {:.2e})", gap.length()));
    }
    if out.len() < 3 {
        return Ok(None);
    }
    Ok(Some(out))
}
