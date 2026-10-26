//! Mesh preparation: weld, clean, orient, check, merge coplanar triangles into faces.

use std::collections::{HashMap, HashSet, VecDeque};

use cad_i18n::{tr, trs};
use cad_kernel::trimesh::{TriMesh, ray_parity};
use glam::DVec3;

/// What the preparation found and changed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Stats {
    pub input_triangles: usize,
    /// Triangles of the mesh.
    pub triangles: usize,
    pub vertices: usize,
    /// Weld tolerance (mm, relative to the size).
    pub weld_tol: f64,
    /// Triangles with a NaN or infinite coordinate (dropped).
    pub non_finite: usize,
    /// Triangles with two equal corners after welding (dropped).
    pub degenerate: usize,
    /// Triangles that repeat another one (dropped).
    pub duplicate: usize,
    /// Triangles whose orientation was reversed.
    pub flipped: usize,
    /// All triangles of a closed part were reversed (inside out).
    pub inverted_parts: usize,
    /// Edges with one triangle.
    pub open_edges: usize,
    /// Edges with more than two triangles.
    pub nonmanifold_edges: usize,
    /// Neighbouring triangles could not be oriented consistently (Möbius strip, …).
    pub nonorientable: bool,
    /// Connected parts.
    pub components: usize,
    /// Planar faces after merging.
    pub faces: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Prepared {
    pub mesh: TriMesh,
    pub stats: Stats,
}

/// Edge of the welded mesh: the triangles using it and whether they run `lo → hi`.
#[derive(Clone, Copy, Default)]
struct Use {
    t: [u32; 2],
    fwd: [bool; 2],
    n: u32,
}

/// Prepares triangles (mm) for import, see the crate docs. `Err` if nothing usable is left.
pub fn prepare(input: &[[DVec3; 3]]) -> Result<Prepared, String> {
    let mut st = Stats { input_triangles: input.len(), ..Default::default() };
    if input.is_empty() {
        return Err(trs!("0 Dreiecke in der Datei", "0 triangles in the file").into());
    }
    let finite: Vec<&[DVec3; 3]> = input.iter().filter(|t| t.iter().all(|p| p.is_finite())).collect();
    st.non_finite = input.len() - finite.len();
    if finite.is_empty() {
        return Err(tr!(
            "alle {} Dreiecke haben ungültige Koordinaten (NaN oder unendlich)",
            "all {} triangles have invalid coordinates (NaN or infinite)",
            input.len()
        ));
    }
    let mut lo = DVec3::splat(f64::INFINITY);
    let mut hi = DVec3::splat(f64::NEG_INFINITY);
    for p in finite.iter().flat_map(|t| t.iter()) {
        lo = lo.min(*p);
        hi = hi.max(*p);
    }
    let diag = (hi - lo).length();
    if diag <= 0.0 {
        return Err(trs!("alle Ecken liegen in einem Punkt", "all vertices lie in one point").into());
    }
    let scale = diag.max(lo.abs().max(hi.abs()).max_element());
    st.weld_tol = 1e-6 * diag;

    // ---- weld: exact duplicates first, then a grid of cells of the tolerance
    let mut points: Vec<DVec3> = Vec::new();
    let mut exact: HashMap<[u64; 3], u32> = HashMap::new();
    let mut grid: HashMap<[i64; 3], Vec<u32>> = HashMap::new();
    let cell = st.weld_tol;
    let cell_of = |p: DVec3| [(p.x / cell).floor() as i64, (p.y / cell).floor() as i64, (p.z / cell).floor() as i64];
    let mut weld = |p: DVec3| -> u32 {
        let bits = [p.x.to_bits(), p.y.to_bits(), p.z.to_bits()];
        if let Some(&i) = exact.get(&bits) {
            return i;
        }
        let c = cell_of(p);
        let mut found = None;
        'cells: for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    if let Some(v) = grid.get(&[c[0] + dx, c[1] + dy, c[2] + dz]) {
                        for &i in v {
                            if (points[i as usize] - p).length() <= cell {
                                found = Some(i);
                                break 'cells;
                            }
                        }
                    }
                }
            }
        }
        let i = found.unwrap_or_else(|| {
            points.push(p);
            let i = (points.len() - 1) as u32;
            grid.entry(c).or_default().push(i);
            i
        });
        exact.insert(bits, i);
        i
    };
    let mut tris: Vec<[u32; 3]> = Vec::with_capacity(finite.len());
    let mut seen: HashSet<[u32; 3]> = HashSet::with_capacity(finite.len());
    for t in &finite {
        let v = [weld(t[0]), weld(t[1]), weld(t[2])];
        if v[0] == v[1] || v[1] == v[2] || v[0] == v[2] {
            st.degenerate += 1;
            continue;
        }
        let mut k = v;
        k.sort_unstable();
        if !seen.insert(k) {
            st.duplicate += 1;
            continue;
        }
        tris.push(v);
    }
    if tris.is_empty() {
        return Err(tr!(
            "nur entartete Dreiecke ({} Dreiecke ohne Fläche)",
            "only degenerate triangles ({} triangles without area)",
            st.degenerate
        ));
    }

    // ---- edges
    let key = |a: u32, b: u32| if a < b { (a, b) } else { (b, a) };
    let mut edges: HashMap<(u32, u32), Use> = HashMap::with_capacity(tris.len() * 3 / 2);
    for (i, t) in tris.iter().enumerate() {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            let u = edges.entry(key(a, b)).or_default();
            if u.n < 2 {
                u.t[u.n as usize] = i as u32;
                u.fwd[u.n as usize] = a < b;
            }
            u.n += 1;
        }
    }
    st.open_edges = edges.values().filter(|u| u.n == 1).count();
    st.nonmanifold_edges = edges.values().filter(|u| u.n > 2).count();
    // neighbour across edge k (t[k] → t[k+1]) through a manifold edge
    let neighbours = |tris: &[[u32; 3]]| -> Vec<[Option<u32>; 3]> {
        tris.iter()
            .enumerate()
            .map(|(i, t)| {
                std::array::from_fn(|k| {
                    let u = edges[&key(t[k], t[(k + 1) % 3])];
                    (u.n == 2).then(|| if u.t[0] == i as u32 { u.t[1] } else { u.t[0] })
                })
            })
            .collect()
    };
    let nb = neighbours(&tris);

    // ---- orientation: flip flags relative to the input, per connected part
    let n = tris.len();
    let mut flip: Vec<Option<bool>> = vec![None; n];
    let mut comp: Vec<u32> = vec![0; n];
    let mut comps: Vec<Vec<u32>> = Vec::new();
    for seed in 0..n {
        if flip[seed].is_some() {
            continue;
        }
        let c = comps.len() as u32;
        let mut members = vec![seed as u32];
        flip[seed] = Some(false);
        comp[seed] = c;
        let mut q = VecDeque::from([seed as u32]);
        while let Some(t) = q.pop_front() {
            let tt = tris[t as usize];
            let ft = flip[t as usize].unwrap();
            for k in 0..3 {
                let Some(u) = nb[t as usize][k] else { continue };
                let (a, b) = (tt[k], tt[(k + 1) % 3]);
                // u must run b → a; does it (unflipped) contain a → b?
                let tu = tris[u as usize];
                let same = (0..3).any(|j| tu[j] == a && tu[(j + 1) % 3] == b);
                let want = ft ^ same;
                match flip[u as usize] {
                    None => {
                        flip[u as usize] = Some(want);
                        comp[u as usize] = c;
                        members.push(u);
                        q.push_back(u);
                    }
                    Some(f) if f != want => st.nonorientable = true,
                    _ => {}
                }
            }
        }
        comps.push(members);
    }
    st.components = comps.len();
    let mut flip: Vec<bool> = flip.into_iter().map(|f| f.unwrap_or(false)).collect();
    let oriented = |t: [u32; 3], f: bool| if f { [t[0], t[2], t[1]] } else { t };
    let comp_closed: Vec<bool> =
        comps.iter().map(|m| m.iter().all(|&t| nb[t as usize].iter().all(|x| x.is_some()))).collect();
    let closed_ids: Vec<usize> = (0..comps.len()).filter(|&c| comp_closed[c]).collect();
    let volume = |c: usize, flip: &[bool]| -> f64 {
        comps[c]
            .iter()
            .map(|&t| {
                let [a, b, d] = oriented(tris[t as usize], flip[t as usize]).map(|i| points[i as usize]);
                a.dot(b.cross(d))
            })
            .sum::<f64>()
    };
    // nesting depth of the closed parts (a void inside a solid points inward)
    let depth: Vec<usize> = (0..comps.len())
        .map(|c| {
            if !comp_closed[c] || closed_ids.len() > 64 {
                return 0;
            }
            let p = points[tris[comps[c][0] as usize][0] as usize];
            closed_ids
                .iter()
                .filter(|&&o| o != c)
                .filter(|&&o| ray_parity(p, comps[o].iter().map(|&t| tris[t as usize].map(|i| points[i as usize]))))
                .count()
        })
        .collect();
    for c in 0..comps.len() {
        let reverse = if comp_closed[c] {
            let v = volume(c, &flip);
            let want_positive = depth[c].is_multiple_of(2);
            let r = (v < 0.0) == want_positive && v != 0.0;
            if r {
                st.inverted_parts += 1;
            }
            r
        } else {
            // open part: keep the orientation most of its triangles had in the file
            2 * comps[c].iter().filter(|&&t| flip[t as usize]).count() > comps[c].len()
        };
        if reverse {
            for &t in &comps[c] {
                flip[t as usize] = !flip[t as usize];
            }
        }
    }
    st.flipped = flip.iter().filter(|&&f| f).count();
    let tris: Vec<[u32; 3]> = tris.iter().zip(&flip).map(|(t, &f)| oriented(*t, f)).collect();
    // edge k of a reversed triangle is another edge
    let nb = neighbours(&tris);
    let closed = st.open_edges == 0 && st.nonmanifold_edges == 0 && !st.nonorientable;

    // ---- planar faces
    let plane_tol = 1e-6 * scale;
    let normal = |t: &[u32; 3]| {
        let [a, b, c] = t.map(|i| points[i as usize]);
        (b - a).cross(c - a)
    };
    let raw: Vec<DVec3> = tris.iter().map(normal).collect();
    let mut order: Vec<u32> = (0..n as u32).collect();
    order.sort_by(|&a, &b| raw[b as usize].length_squared().total_cmp(&raw[a as usize].length_squared()));
    let mut face: Vec<u32> = vec![u32::MAX; n];
    // per vertex: last face id whose region uses it
    let mut vstamp: Vec<u32> = vec![u32::MAX; points.len()];
    let mut loops: Vec<Vec<u32>> = Vec::new();
    let mut face_normal: Vec<DVec3> = Vec::new();
    for &seed in &order {
        if face[seed as usize] != u32::MAX {
            continue;
        }
        let ns = raw[seed as usize].normalize_or_zero();
        let d = ns.dot(points[tris[seed as usize][0] as usize]);
        let fid = loops.len() as u32;
        let mut region = vec![seed];
        face[seed as usize] = fid;
        tris[seed as usize].iter().for_each(|&v| vstamp[v as usize] = fid);
        if ns != DVec3::ZERO {
            // The region grows as a disk (one simple boundary loop): a triangle joins along two
            // edges, or along one edge if its third corner is new. A plane with holes thus
            // becomes a few faces without holes. A triangle refused now is tried again when
            // another neighbour joins.
            let mut q = VecDeque::from([seed]);
            while let Some(t) = q.pop_front() {
                for u in nb[t as usize].iter().flatten() {
                    let u = *u;
                    if face[u as usize] != u32::MAX {
                        continue;
                    }
                    let nu = raw[u as usize].normalize_or_zero();
                    let flat = tris[u as usize].iter().all(|&i| (ns.dot(points[i as usize]) - d).abs() <= plane_tol);
                    if !flat || (nu != DVec3::ZERO && nu.dot(ns) <= 0.5) {
                        continue;
                    }
                    let tu = tris[u as usize];
                    let shared: Vec<usize> =
                        (0..3).filter(|&k| nb[u as usize][k].is_some_and(|x| face[x as usize] == fid)).collect();
                    let disk = match shared[..] {
                        [k] => vstamp[tu[(k + 2) % 3] as usize] != fid,
                        [_, _] => true,
                        _ => false,
                    };
                    if disk {
                        face[u as usize] = fid;
                        tu.iter().for_each(|&v| vstamp[v as usize] = fid);
                        region.push(u);
                        q.push_back(u);
                    }
                }
            }
        }
        match (region.len() > 1).then(|| boundary_loop(&tris, &nb, &face, &region, fid)).flatten() {
            Some(l) => {
                loops.push(l);
                face_normal.push(ns);
            }
            None => {
                // a region with holes (or pinched): its triangles stay single faces
                for (k, &t) in region.iter().enumerate() {
                    let id = if k == 0 { fid } else { loops.len() as u32 };
                    face[t as usize] = id;
                    loops.push(tris[t as usize].to_vec());
                    face_normal.push(raw[t as usize].normalize_or_zero());
                }
            }
        }
    }
    // the kernel needs exactly planar faces: vertices onto the planes of their faces, faces
    // whose vertices cannot lie on their plane fall back to triangles
    let original = points.clone();
    for _ in 0..2 {
        let bad = snap_to_planes(&mut points, &original, &tris, &face, &loops, plane_tol, 1e-10 * scale);
        if bad.is_empty() {
            break;
        }
        let mut members: HashMap<u32, Vec<usize>> = bad.iter().map(|&f| (f, Vec::new())).collect();
        for (t, f) in face.iter().enumerate() {
            if let Some(m) = members.get_mut(f) {
                m.push(t);
            }
        }
        for f in bad {
            for (k, &t) in members[&f].iter().enumerate() {
                let id = if k == 0 { f } else { loops.len() as u32 };
                face[t] = id;
                let l = tris[t].to_vec();
                if k == 0 {
                    loops[f as usize] = l;
                    face_normal[f as usize] = raw[t].normalize_or_zero();
                } else {
                    loops.push(l);
                    face_normal.push(raw[t].normalize_or_zero());
                }
            }
        }
    }
    reduce_loops(&mut loops, &face_normal, &tris, &nb, points.len());
    // drop corners only dropped triangles used
    let mut remap = vec![u32::MAX; points.len()];
    let mut used = Vec::with_capacity(points.len());
    for t in &tris {
        for &v in t {
            if remap[v as usize] == u32::MAX {
                remap[v as usize] = used.len() as u32;
                used.push(points[v as usize]);
            }
        }
    }
    let tris: Vec<[u32; 3]> = tris.iter().map(|t| t.map(|v| remap[v as usize])).collect();
    for l in &mut loops {
        l.iter_mut().for_each(|v| *v = remap[*v as usize]);
    }
    st.faces = loops.len();
    st.triangles = tris.len();
    st.vertices = used.len();
    Ok(Prepared { mesh: TriMesh { points: used, tris, tri_face: face, loops, closed }, stats: st })
}

/// The single boundary loop of a region of triangles (face id `fid` in `face`), `None` if it
/// has several loops or touches itself.
fn boundary_loop(
    tris: &[[u32; 3]],
    nb: &[[Option<u32>; 3]],
    face: &[u32],
    region: &[u32],
    fid: u32,
) -> Option<Vec<u32>> {
    let mut next: HashMap<u32, u32> = HashMap::new();
    for &t in region {
        let tt = tris[t as usize];
        for k in 0..3 {
            let inside = nb[t as usize][k].is_some_and(|u| face[u as usize] == fid);
            if !inside && next.insert(tt[k], tt[(k + 1) % 3]).is_some() {
                return None;
            }
        }
    }
    let start = *next.keys().next()?;
    let mut l = vec![start];
    let mut v = next[&start];
    while v != start {
        if l.len() > next.len() {
            return None;
        }
        l.push(v);
        v = *next.get(&v)?;
    }
    (l.len() == next.len()).then_some(l)
}

/// Drops loop vertices that only lie on a straight run between two faces of different planes
/// (in exactly two loops, on no open edge), as long as every loop keeps three vertices.
fn reduce_loops(loops: &mut [Vec<u32>], normals: &[DVec3], tris: &[[u32; 3]], nb: &[[Option<u32>; 3]], nv: usize) {
    let mut count = vec![0u8; nv];
    let mut first = vec![u32::MAX; nv];
    let mut second = vec![u32::MAX; nv];
    for (f, l) in loops.iter().enumerate() {
        for &v in l {
            let c = &mut count[v as usize];
            match *c {
                0 => first[v as usize] = f as u32,
                1 => second[v as usize] = f as u32,
                _ => {}
            }
            *c = c.saturating_add(1);
        }
    }
    let mut open = vec![false; nv];
    for (t, n) in tris.iter().zip(nb) {
        for k in 0..3 {
            if n[k].is_none() {
                open[t[k] as usize] = true;
                open[t[(k + 1) % 3] as usize] = true;
            }
        }
    }
    let mut drop: Vec<bool> = (0..nv)
        .map(|v| {
            count[v] == 2 && !open[v] && normals[first[v] as usize].dot(normals[second[v] as usize]).abs() < 1.0 - 1e-9
        })
        .collect();
    loop {
        let mut changed = false;
        for l in loops.iter() {
            if l.iter().filter(|&&v| !drop[v as usize]).count() < 3 {
                for &v in l {
                    changed |= drop[v as usize];
                    drop[v as usize] = false;
                }
            }
        }
        if !changed {
            break;
        }
    }
    for l in loops.iter_mut() {
        l.retain(|&v| !drop[v as usize]);
    }
}

/// Moves every vertex onto the planes of its merged faces (faces of more than one triangle):
/// exactly onto up to three independent planes (the closest such point to its original
/// position `orig`), a vertex of more faces must already lie on the others within `tight`.
/// Moves larger than `10 · plane_tol` are refused. Returns the faces whose vertices do not all
/// lie within `tight` of the face's plane afterwards.
fn snap_to_planes(
    points: &mut [DVec3],
    orig: &[DVec3],
    tris: &[[u32; 3]],
    face: &[u32],
    loops: &[Vec<u32>],
    plane_tol: f64,
    tight: f64,
) -> Vec<u32> {
    let nf = loops.len();
    let mut count = vec![0u32; nf];
    let mut nsum = vec![DVec3::ZERO; nf];
    let mut csum = vec![DVec3::ZERO; nf];
    for (t, &f) in tris.iter().zip(face) {
        let [a, b, c] = t.map(|i| orig[i as usize]);
        let cr = (b - a).cross(c - a);
        count[f as usize] += 1;
        nsum[f as usize] += cr;
        csum[f as usize] += cr.length() * (a + b + c) / 3.0;
    }
    let planes: Vec<Option<(DVec3, f64)>> = (0..nf)
        .map(|f| {
            let n = nsum[f].normalize_or_zero();
            let area = nsum[f].length();
            (count[f] > 1 && n != DVec3::ZERO && area > 0.0).then(|| (n, n.dot(csum[f] / area)))
        })
        .collect();
    // merged faces per vertex
    let mut vf: Vec<Vec<u32>> = vec![Vec::new(); points.len()];
    for (t, &f) in tris.iter().zip(face) {
        if planes[f as usize].is_some() {
            for &v in t {
                if !vf[v as usize].contains(&f) {
                    vf[v as usize].push(f);
                }
            }
        }
    }
    for (v, fs) in vf.iter().enumerate() {
        let p = orig[v];
        if fs.is_empty() {
            points[v] = p;
            continue;
        }
        // up to three independent planes, greedily
        let pl: Vec<(DVec3, f64)> = fs.iter().filter_map(|&f| planes[f as usize]).collect();
        let mut pick: Vec<(DVec3, f64)> = vec![pl[0]];
        if let Some(&q) = pl.iter().min_by(|a, b| a.0.dot(pick[0].0).abs().total_cmp(&b.0.dot(pick[0].0).abs()))
            && q.0.cross(pick[0].0).length() > 1e-6
        {
            pick.push(q);
            let axis = pick[0].0.cross(pick[1].0).normalize();
            if let Some(&r) = pl.iter().max_by(|a, b| a.0.dot(axis).abs().total_cmp(&b.0.dot(axis).abs()))
                && r.0.dot(axis).abs() > 1e-6
            {
                pick.push(r);
            }
        }
        // x = p + Σ a_i n_i with n_i · x = d_i (Gram system)
        let k = pick.len();
        let mut g = [[0.0f64; 3]; 3];
        let mut r = [0.0f64; 3];
        for i in 0..k {
            for j in 0..k {
                g[i][j] = pick[i].0.dot(pick[j].0);
            }
            r[i] = pick[i].1 - pick[i].0.dot(p);
        }
        let a = solve(&g, &r, k);
        let x = p + (0..k).map(|i| pick[i].0 * a[i]).sum::<DVec3>();
        points[v] = if x.is_finite() && (x - p).length() <= 10.0 * plane_tol { x } else { p };
    }
    let mut bad: Vec<u32> = Vec::new();
    for (t, &f) in tris.iter().zip(face) {
        if let Some((n, d)) = planes[f as usize]
            && t.iter().any(|&v| (n.dot(points[v as usize]) - d).abs() > tight)
            && !bad.contains(&f)
        {
            bad.push(f);
        }
    }
    bad
}

/// Solves the leading `k × k` system (Cramer's rule, k ≤ 3).
fn solve(g: &[[f64; 3]; 3], r: &[f64; 3], k: usize) -> [f64; 3] {
    match k {
        1 => [r[0] / g[0][0], 0.0, 0.0],
        2 => {
            let det = g[0][0] * g[1][1] - g[0][1] * g[1][0];
            [(r[0] * g[1][1] - g[0][1] * r[1]) / det, (g[0][0] * r[1] - r[0] * g[1][0]) / det, 0.0]
        }
        _ => {
            let m = glam::DMat3::from_cols_array_2d(g).transpose();
            let det = m.determinant();
            let col = |i: usize| {
                let mut c = m;
                *c.col_mut(i) = DVec3::from_array(*r);
                c.determinant() / det
            };
            [col(0), col(1), col(2)]
        }
    }
}
