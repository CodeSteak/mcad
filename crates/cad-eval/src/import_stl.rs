//! `import("teil.stl", center, scale, convexity, as)` (Plan 21, phase B): an STL file as a
//! [`Prim::Mesh`] node. `cad_stl` parses and prepares the mesh (weld, orient, merge coplanar
//! triangles into faces); then the mode is chosen: a closed mesh of at most
//! [`cad_stl::AUTO_SOLID_FACES`] faces becomes a B-rep (polyhedron of its faces), anything else
//! stays a mesh; `as = "solid"` / `as = "mesh"` overrides.
//!
//! Prepared meshes are cached by content hash and options (thread-local, bounded in bytes), so
//! a re-evaluation of an unchanged project does not parse the file again.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use cad_ir::{MeshMode, MeshRef, Prim};
use cad_syntax::Span;
use cad_syntax::ast::Stmt;
use glam::{DMat4, DVec3};

use crate::env::Dyn;
use crate::eval::{ArgV, Ctx, Ev};
use crate::value::Value;

/// Prepared mesh with what the preparation reported.
struct Imported {
    mesh: MeshRef,
    stats: cad_stl::Stats,
    warnings: Vec<String>,
}

/// Cache of prepared meshes: at most this many bytes of meshes are kept.
const CACHE_BYTES: usize = 256 << 20;

/// A prepared mesh or why the file could not be imported.
type Loaded = Rc<Result<Imported, String>>;

thread_local! {
    static MESHES: RefCell<HashMap<[u8; 32], Loaded>> = RefCell::new(HashMap::new());
}

fn load(data: &[u8], scale: DVec3, center: bool) -> Loaded {
    let mut h = blake3::Hasher::new();
    h.update(cad_stl::VERSION.as_bytes());
    for v in scale.to_array() {
        h.update(&v.to_le_bytes());
    }
    h.update(&[center as u8]);
    h.update(&(data.len() as u64).to_le_bytes());
    h.update(data);
    let hash = *h.finalize().as_bytes();
    if let Some(r) = MESHES.with(|c| c.borrow().get(&hash).cloned()) {
        return r;
    }
    let r = Rc::new(prepare(data, scale, center, hash));
    MESHES.with(|c| {
        let mut c = c.borrow_mut();
        let size = |r: &Result<Imported, String>| r.as_ref().map_or(64, |i| i.mesh.mesh.bytes());
        let mut total: usize = c.values().map(|r| size(r)).sum::<usize>() + size(&r);
        // drop other entries until the new one fits (no order: entries are equally cheap to redo)
        let keys: Vec<[u8; 32]> = c.keys().copied().collect();
        for k in keys {
            if total <= CACHE_BYTES && c.len() < 32 {
                break;
            }
            if let Some(old) = c.remove(&k) {
                total -= size(&old);
            }
        }
        c.insert(hash, r.clone());
    });
    r
}

fn prepare(data: &[u8], scale: DVec3, center: bool, hash: [u8; 32]) -> Result<Imported, String> {
    let stl = cad_stl::parse(data)?;
    let mut warnings = stl.warnings.clone();
    let mut tris: Vec<[DVec3; 3]> =
        stl.triangles.iter().map(|t| t.map(|p| DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64) * scale)).collect();
    if center {
        let mut lo = DVec3::splat(f64::INFINITY);
        let mut hi = DVec3::splat(f64::NEG_INFINITY);
        for p in tris.iter().flatten().filter(|p| p.is_finite()) {
            lo = lo.min(*p);
            hi = hi.max(*p);
        }
        if lo.is_finite() {
            let c = 0.5 * (lo + hi);
            tris.iter_mut().flatten().for_each(|p| *p -= c);
        }
    }
    let p = cad_stl::prepare(&tris)?;
    let st = &p.stats;
    if st.non_finite > 0 {
        warnings
            .push(format!("{} Dreiecke mit ungültigen Koordinaten (NaN oder unendlich) weggelassen", st.non_finite));
    }
    if st.degenerate > 0 {
        warnings.push(format!(
            "{} entartete Dreiecke (Ecken zusammengefallen, verschweißt mit Toleranz {} mm) entfernt",
            st.degenerate,
            crate::value::fmt_num(st.weld_tol)
        ));
    }
    if st.duplicate > 0 {
        warnings.push(format!("{} doppelte Dreiecke entfernt", st.duplicate));
    }
    if st.inverted_parts > 0 && st.flipped == st.triangles {
        warnings.push(format!(
            "Orientierung korrigiert: Netz war innen nach außen (alle {} Dreiecke gewendet)",
            st.triangles
        ));
    } else if st.flipped > 0 {
        warnings.push(format!(
            "Orientierung korrigiert: {} von {} Dreiecken gewendet (uneinheitlich orientiert)",
            st.flipped, st.triangles
        ));
    }
    if st.nonorientable {
        warnings.push("Netz nicht orientierbar (Nachbardreiecke widersprechen sich, z. B. Möbiusband)".into());
    }
    Ok(Imported { mesh: MeshRef { hash, mesh: Arc::new(p.mesh) }, stats: p.stats, warnings })
}

impl<'a> Ev<'a> {
    /// Uniform or per-axis scale (`scale = 2`, `scale = [1, 1, 2]`), positive.
    fn scale3(&mut self, a: Option<&ArgV<'a>>) -> DVec3 {
        let Some(x) = a else { return DVec3::ONE };
        if matches!(x.val.v, Value::Undef) {
            return DVec3::ONE;
        }
        if let Some(v) = x.val.as_num()
            && v.is_finite()
            && v > 0.0
        {
            return DVec3::splat(v);
        }
        if let Some(l) = x.val.as_list()
            && l.len() == 3
        {
            let v: Vec<f64> = l.iter().filter_map(|e| e.as_num()).filter(|v| v.is_finite() && *v > 0.0).collect();
            if v.len() == 3 {
                return DVec3::new(v[0], v[1], v[2]);
            }
        }
        self.warn(x.expr.span, "import: `scale` muss eine positive Zahl oder [x, y, z] sein (Spiegeln mit mirror())");
        DVec3::ONE
    }

    /// The STL branch of `import()`; `p` binds `file, id, scale, dpi, center, convexity, layer,
    /// origin, width, height, as`.
    pub(crate) fn import_stl(
        &mut self,
        s: &'a Stmt,
        p: &[Option<&ArgV<'a>>],
        fspan: Span,
        file: &str,
        ctx: &Ctx<'a>,
        dy: &Dyn<'a>,
    ) {
        for (i, n) in [(1, "id"), (3, "dpi")] {
            if let Some(x) = p[i] {
                self.warn(x.expr.span, format!("import: `{n}` gilt nur für SVG und wird ignoriert"));
            }
        }
        let forced = match p[10].map(|x| (&x.val.v, x.expr.span)) {
            None | Some((Value::Undef, _)) => None,
            Some((Value::Str(m), _)) if &**m == "solid" => Some(MeshMode::Solid),
            Some((Value::Str(m), _)) if &**m == "mesh" => Some(MeshMode::Mesh),
            Some((_, sp)) => {
                self.warn(sp, "import: `as` ist \"solid\" (B-Rep) oder \"mesh\" (Netz); automatisch gewählt");
                None
            }
        };
        let scale = self.scale3(p[2]);
        let center = p[4].is_some_and(|x| matches!(x.val.v, Value::Bool(true)));
        let Some((path, data)) = self.asset_data(fspan, file) else { return };
        let loaded = load(&data, scale, center);
        let imp = match loaded.as_ref() {
            Ok(i) => i,
            Err(e) => {
                self.error(fspan, format!("{path}: {e}"));
                return;
            }
        };
        for w in &imp.warnings {
            self.warn(fspan, format!("{path}: {w}"));
        }
        let st = &imp.stats;
        let why_not = if st.nonmanifold_edges > 0 {
            Some(format!("nicht mannigfaltig ({} Kanten mit mehr als zwei Dreiecken)", st.nonmanifold_edges))
        } else if st.open_edges > 0 {
            Some(format!("nicht geschlossen ({} offene Kanten)", st.open_edges))
        } else if st.nonorientable {
            Some("nicht orientierbar".to_string())
        } else {
            None
        };
        let mode = match (forced, &why_not) {
            (Some(MeshMode::Solid), Some(why)) => {
                self.error(
                    fspan,
                    format!("{path}: as = \"solid\" geht nicht, das Netz ist {why}; als Netz importiert"),
                );
                MeshMode::Mesh
            }
            (Some(m), _) => m,
            (None, Some(why)) => {
                self.warn(fspan, format!("{path}: Netz {why}: als Netz ohne B-Rep importiert (keine Booleans)"));
                MeshMode::Mesh
            }
            (None, None) if st.faces <= cad_stl::AUTO_SOLID_FACES => MeshMode::AutoSolid,
            (None, None) => MeshMode::Mesh,
        };
        self.prim(s, Prim::Mesh { mesh: imp.mesh.clone(), mode }, DMat4::IDENTITY, ctx, dy);
    }
}
