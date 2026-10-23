//! Colour regions and filament slots (Plan 16.2, 16.3).
//!
//! **Colour table** ([`ColorTable::of`], no kernel): the colours of the *material tree* (the
//! result without `%`, `*`, zones and the subtracted operands of `difference`) in order of
//! first appearance (pre-order), one entry per RGBA value. Material from a primitive,
//! extrusion or surface without any `color()` above it is the *default* entry ("Standard").
//!
//! **Filament slots:** an entry with `filament = n` at one of its `color()` calls gets slot
//! `n` (the first explicit value counts; the evaluator warns about conflicting ones). The
//! default entry gets slot 1. All other entries get the lowest free slot in order of
//! appearance. Slots are thus fixed by the code alone, never by kernel results.
//!
//! **Colour regions** ([`Engine::color_regions`], kernel booleans, cached): the volume of a
//! node's result split by colour, in the node's own system:
//! * the material of a coloured subtree (`color()` node `C`) is the result of `C`, placed in
//!   the node's system (`local` matrices and `resize` factors on the way);
//! * overlaps: the later statement (pre-order: a nested `color()` after its parent, a later
//!   sibling after an earlier one) wins, the earlier one is reduced by it;
//! * subtracted material counts for all colours (everything is intersected with the result);
//! * material in no coloured subtree (e.g. a fillet between differently coloured bodies) gets
//!   the colour in effect at the node; without one, the default entry if the node has
//!   uncoloured primitives, otherwise the colour of its first coloured subtree.
//!
//! Then `piece(C) = R ∩ M(C) − ⋃ M(later)`, `rest = R − ⋃ M(all)`; pieces of one colour are
//! united. The volumes add up to the volume of the result (up to the kernel tolerance; empty
//! pieces are dropped). `color()` inside 2D subtrees (below an extrusion) is not split into
//! volumes: the extrusion is one region.

use std::rc::Rc;

use cad_ir::{Dim, NodeIdx, Op, Tree};
use cad_kernel::{BoolOp, Kernel, Mesh};
use glam::DMat4;
use serde::Serialize;

use crate::key::{CacheKey, KeyHasher};
use crate::{Cached, Engine, Evaluation, Slot, append_offset, mesh_bytes};

/// Colour identity: RGBA bytes.
pub type ColorKey = [u8; 4];

pub fn color_key(c: [f32; 4]) -> ColorKey {
    c.map(|x| (x.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// `#rrggbb` (or `#rrggbbaa` with alpha < 1).
pub fn key_hex(k: ColorKey) -> String {
    if k[3] == 255 {
        format!("#{:02x}{:02x}{:02x}", k[0], k[1], k[2])
    } else {
        format!("#{:02x}{:02x}{:02x}{:02x}", k[0], k[1], k[2], k[3])
    }
}

/// How an entry's colour was given.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ColorSource {
    /// Material without `color()`.
    Default,
    /// Palette name.
    Palette,
    /// Unknown name (stable random colour).
    Random,
    /// Hex or vector.
    Exact,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ColorEntry {
    pub key: ColorKey,
    /// Exact RGBA (the first `color()` that gave it).
    pub rgba: [f32; 4],
    /// Palette name, unknown name as written, hex text, or `Standard`.
    pub label: String,
    pub source: ColorSource,
    /// Filament slot, 1-based.
    pub filament: u32,
    /// The slot comes from `filament = n` in the code.
    pub explicit: bool,
    /// `color()` nodes with this colour, pre-order (empty for the default entry).
    pub nodes: Vec<NodeIdx>,
    /// The `color()` node whose `filament = n` set the slot.
    pub explicit_node: Option<NodeIdx>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ColorTable {
    pub entries: Vec<ColorEntry>,
}

/// Nodes that create material (leaves of the material tree).
fn creates_material(op: &Op) -> bool {
    matches!(
        op,
        Op::Prim { .. }
            | Op::LinearExtrude { .. }
            | Op::RotateExtrude { .. }
            | Op::Loft { .. }
            | Op::Sweep { .. }
            | Op::Surface { .. }
            | Op::Thicken { .. }
    )
}

/// Children of `n` that are material (not `%`, `*`, zones, subtracted operands; 2D subtrees
/// of profile operations are not descended into).
fn material_children(tree: &Tree, n: NodeIdx) -> Vec<NodeIdx> {
    let node = tree.node(n);
    match &node.op {
        Op::Boolean { op: BoolOp::Difference, .. } => tree.solid_children(n).take(1).collect(),
        Op::Blend { .. } | Op::Shell { .. } => node.children.first().copied().into_iter().collect(),
        op if crate::is_profile_op(op) => Vec::new(),
        _ => tree.solid_children(n).collect(),
    }
}

/// Colour in effect at a node, including its own `color()`.
fn effective(tree: &Tree, n: NodeIdx) -> Option<ColorKey> {
    let node = tree.node(n);
    match &node.op {
        Op::Color { rgba, .. } => Some(color_key(*rgba)),
        _ => node.color.map(color_key),
    }
}

/// Pre-order walk of the material tree below `n` (inclusive): `f(node, colour in effect)`.
fn walk(tree: &Tree, n: NodeIdx, f: &mut impl FnMut(NodeIdx, Option<ColorKey>)) {
    let mut stack = vec![n];
    while let Some(i) = stack.pop() {
        f(i, effective(tree, i));
        let kids = material_children(tree, i);
        stack.extend(kids.into_iter().rev());
    }
}

impl ColorTable {
    /// The table of a tree (see the module docs). `default_rgba`: colour of the default
    /// entry (`cad_eval::palette::DEFAULT_RGB`); `is_palette`: is a name a palette name
    /// (otherwise a named colour is a random one).
    pub fn of(tree: &Tree, default_rgba: [f32; 4], is_palette: &dyn Fn(&str) -> bool) -> ColorTable {
        let mut t = ColorTable::default();
        let Some(root) = tree.root else { return t };
        if tree.node(root).flags.disabled {
            return t;
        }
        let mut uncolored = false;
        let mut order: Vec<(ColorKey, NodeIdx)> = Vec::new();
        walk(tree, root, &mut |i, eff| {
            let node = tree.node(i);
            if matches!(node.op, Op::Color { .. }) {
                order.push((eff.expect("colour node"), i));
            } else if eff.is_none() && creates_material(&node.op) && tree.dim(i) == Some(Dim::Three) {
                uncolored = true;
            }
        });
        if uncolored {
            t.entries.push(ColorEntry {
                key: color_key(default_rgba),
                rgba: default_rgba,
                label: "Standard".into(),
                source: ColorSource::Default,
                filament: 1,
                explicit: false,
                nodes: Vec::new(),
                explicit_node: None,
            });
        }
        for (key, i) in order {
            let Op::Color { rgba, name, filament } = &tree.node(i).op else { continue };
            let pos = t.entries.iter().position(|e| e.key == key && e.source != ColorSource::Default);
            let e = match pos {
                Some(p) => &mut t.entries[p],
                None => {
                    let (label, source) = match name {
                        Some(n) if is_palette(n) => (n.clone(), ColorSource::Palette),
                        Some(n) => (n.clone(), ColorSource::Random),
                        None => (key_hex(key), ColorSource::Exact),
                    };
                    t.entries.push(ColorEntry {
                        key,
                        rgba: *rgba,
                        label,
                        source,
                        filament: 0,
                        explicit: false,
                        nodes: Vec::new(),
                        explicit_node: None,
                    });
                    t.entries.last_mut().unwrap()
                }
            };
            e.nodes.push(i);
            if let Some(f) = filament
                && !e.explicit
            {
                e.filament = *f;
                e.explicit = true;
                e.explicit_node = Some(i);
            }
        }
        // auto slots: lowest free slot in order of appearance
        let mut used: Vec<u32> =
            t.entries.iter().filter(|e| e.explicit || e.filament > 0).map(|e| e.filament).collect();
        for e in t.entries.iter_mut().filter(|e| e.filament == 0) {
            let mut s = 1;
            while used.contains(&s) {
                s += 1;
            }
            e.filament = s;
            used.push(s);
        }
        t
    }

    pub fn find(&self, key: ColorKey) -> Option<usize> {
        self.entries.iter().position(|e| e.key == key && e.source != ColorSource::Default)
    }

    pub fn default_entry(&self) -> Option<usize> {
        self.entries.iter().position(|e| e.source == ColorSource::Default)
    }

    /// Entry of a region colour (`None` key: default).
    pub fn entry_of(&self, c: RegionColor) -> Option<usize> {
        match c {
            RegionColor::Default => self.default_entry(),
            RegionColor::Color(k) => self.find(k),
        }
    }

    /// Number of distinct slots in use.
    pub fn slots(&self) -> Vec<u32> {
        let mut s: Vec<u32> = self.entries.iter().map(|e| e.filament).collect();
        s.sort_unstable();
        s.dedup();
        s
    }
}

/// Colour of a region.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum RegionColor {
    /// Material without `color()`.
    Default,
    Color(ColorKey),
}

/// One colour's share of a node result (node system).
#[derive(Debug, Clone)]
pub struct Region<S> {
    pub color: RegionColor,
    /// Solids of the region: one, or (no colour node below the part) the part's solids as
    /// they are, without a union (like the part's volume, see [`Engine::mass_props`]).
    pub shapes: Vec<S>,
    /// Volume in mm³.
    pub volume: f64,
    /// Cache key of this region (meshes).
    pub key: CacheKey,
}

#[derive(Debug, Clone)]
pub struct ColorRegions<S> {
    pub node: NodeIdx,
    /// The colour in effect at the node (or the rest colour) first, then in order of
    /// appearance.
    pub regions: Vec<Region<S>>,
    /// Volume of the node result (sum of its solid parts).
    pub total: f64,
    /// Number of `color()` subtrees that took part.
    pub colored_nodes: usize,
    pub key: CacheKey,
    /// The region volumes do not add up to the total (a kernel boolean lost material).
    pub warning: Option<String>,
}

/// Matrix from `c`'s system to `p`'s system (`local` of every node on the way, plus the scale
/// of `resize` nodes, which the engine applies to the result, not to `local`). `None` if `c`
/// is not below `p`.
fn placement<S>(ev: &Evaluation<S>, p: NodeIdx, c: NodeIdx) -> Option<DMat4> {
    let tree = &ev.tree;
    let mut m = DMat4::IDENTITY;
    let mut cur = c;
    while cur != p {
        let n = tree.node(cur);
        m = n.local * m;
        let parent = n.parent?;
        if let Op::Resize { size, auto } = &tree.node(parent).op {
            let mut b = None;
            for k in tree.solid_children(parent) {
                if let Some(kb) = ev.reports[k as usize].bounds {
                    b = crate::eval::merge_box(b, Some(crate::eval::transform_box(kb, &tree.node(k).local)));
                }
            }
            if let Some((lo, hi)) = b {
                let s = crate::profile::resize_scale(hi - lo, *size, *auto);
                m = DMat4::from_scale(s) * m;
            }
        }
        cur = parent;
    }
    Some(m)
}

/// Colour nodes below `p` (not `p` itself) whose colour differs from the one in effect at
/// their parent, pre-order, 3D only.
fn inner_color_nodes(tree: &Tree, p: NodeIdx) -> Vec<(NodeIdx, ColorKey)> {
    let mut out = Vec::new();
    walk(tree, p, &mut |i, eff| {
        if i == p {
            return;
        }
        let n = tree.node(i);
        if let Op::Color { rgba, .. } = &n.op
            && tree.dim(i) == Some(Dim::Three)
            && n.color.map(color_key) != Some(color_key(*rgba))
        {
            out.push((i, eff.expect("colour node")));
        }
    });
    out
}

/// Has the material tree below `p` a primitive without colour?
fn has_uncolored(tree: &Tree, p: NodeIdx) -> bool {
    let mut found = false;
    walk(tree, p, &mut |i, eff| {
        if eff.is_none() && creates_material(&tree.node(i).op) && tree.dim(i) == Some(Dim::Three) {
            found = true;
        }
    });
    found
}

/// A solid of one colour region with its volume.
type Piece<S> = (S, f64);

const EMPTY_REL: f64 = 1e-7;
const EMPTY_ABS: f64 = 1e-4;

impl<K: Kernel> Engine<K>
where
    K::Shape: 'static,
{
    /// The colour regions of a node's result (see the module docs), cached by the key of the
    /// node, of the colour nodes below it, their placements and colours.
    pub fn color_regions(
        &mut self,
        ev: &Evaluation<K::Shape>,
        node: NodeIdx,
    ) -> Result<Rc<ColorRegions<K::Shape>>, String> {
        let tree = ev.tree.clone();
        let rep = ev.reports.get(node as usize).ok_or_else(|| format!("Knoten {node} existiert nicht"))?;
        if rep.dim == Some(Dim::Two) {
            return Err("2D-Ergebnis hat keine Farbregionen".into());
        }
        let inner = inner_color_nodes(&tree, node);
        let base = effective(&tree, node);
        let rest = match (base, inner.first()) {
            (Some(b), _) => RegionColor::Color(b),
            (None, _) if has_uncolored(&tree, node) => RegionColor::Default,
            (None, Some(&(_, k))) => RegionColor::Color(k),
            (None, None) => RegionColor::Default,
        };
        let mut placed = Vec::new();
        let mut h = KeyHasher::new();
        h.str("color-regions/1").key(&rep.key);
        match rest {
            RegionColor::Default => h.tag(0),
            RegionColor::Color(k) => h.tag(1).bytes(&k),
        };
        for &(c, k) in &inner {
            let m = placement(ev, node, c).ok_or("Farbknoten liegt nicht unter dem Knoten")?;
            h.key(&ev.reports[c as usize].key).mat(&m).bytes(&k);
            placed.push((c, k, m));
        }
        let key = h.finish();
        if let Some(Cached::Regions(r)) = self.cache.get(&Slot::Regions(key)) {
            return Ok(r);
        }
        // without colour nodes the region is the whole result: its volume is the (cached)
        // part volume the parts list already asked for
        let whole = if placed.is_empty() { Some(self.mass_props(ev, node)?.volume) } else { None };
        let out = self.compute_regions(ev, node, rest, &placed, key, whole)?;
        let cost =
            256 + out.regions.iter().flat_map(|r| &r.shapes).map(|s| 64 + self.kernel.shape_bytes(s)).sum::<usize>();
        let rc = Rc::new(out);
        self.cache.insert(Slot::Regions(key), Cached::Regions(rc.clone()), cost);
        Ok(rc)
    }

    fn compute_regions(
        &mut self,
        ev: &Evaluation<K::Shape>,
        node: NodeIdx,
        rest: RegionColor,
        placed: &[(NodeIdx, ColorKey, DMat4)],
        key: CacheKey,
        whole: Option<f64>,
    ) -> Result<ColorRegions<K::Shape>, String> {
        let tag = ev.reports[node as usize].tag;
        let k = &mut self.kernel;
        let fail = |what: &str, e: cad_kernel::KernelError| format!("Farbregionen: {what} gescheitert: {e}");
        let solids: Vec<K::Shape> = ev.parts(node).iter().filter(|p| p.solid).map(|p| p.shape.clone()).collect();
        let mut out =
            ColorRegions { node, regions: Vec::new(), total: 0.0, colored_nodes: placed.len(), key, warning: None };
        if solids.is_empty() {
            return Ok(out);
        }
        if let Some(volume) = whole {
            // No colour below the part: one region with the part's solids, no Boolean (a union
            // of many solids can take seconds, and the list asks for every finished scene).
            out.total = volume;
            if volume.abs() > (volume.abs() * EMPTY_REL).max(EMPTY_ABS) {
                let rkey = KeyHasher::new().key(&key).u64(0).finish();
                out.regions.push(Region { color: rest, shapes: solids, volume, key: rkey });
            }
            return Ok(out);
        }
        let r = if solids.len() == 1 {
            solids[0].clone()
        } else {
            k.boolean(BoolOp::Union, &solids, tag).map_err(|e| fail("Vereinigung", e))?
        };
        let total = k.mass_props(&r).map_err(|e| fail("Volumen", e))?.volume;
        out.total = total;
        let mut pieces: Vec<(RegionColor, K::Shape)> = Vec::new();
        let mut claimed: Option<K::Shape> = None;
        for &(c, ck, m) in placed.iter().rev() {
            let ctag = ev.reports[c as usize].tag;
            let mut ms = Vec::new();
            for p in ev.parts(c).iter().filter(|p| p.solid) {
                let s = if m.abs_diff_eq(DMat4::IDENTITY, 1e-12) {
                    p.shape.clone()
                } else {
                    k.transform(&p.shape, &m).map_err(|e| fail("Transformation", e))?
                };
                ms.push(s);
            }
            if ms.is_empty() {
                continue;
            }
            let mc = if ms.len() == 1 {
                ms.pop().unwrap()
            } else {
                k.boolean(BoolOp::Union, &ms, ctag).map_err(|e| fail("Vereinigung", e))?
            };
            let mut piece =
                k.boolean(BoolOp::Intersection, &[r.clone(), mc.clone()], ctag).map_err(|e| fail("Schnittmenge", e))?;
            if let Some(cl) = &claimed
                && !k.is_empty(&piece)
            {
                piece = k.boolean(BoolOp::Difference, &[piece, cl.clone()], ctag).map_err(|e| fail("Differenz", e))?;
            }
            claimed = Some(match claimed.take() {
                None => mc,
                Some(cl) => k.boolean(BoolOp::Union, &[cl, mc], ctag).map_err(|e| fail("Vereinigung", e))?,
            });
            pieces.push((RegionColor::Color(ck), piece));
        }
        pieces.reverse();
        if let Some(cl) = claimed {
            let rem = k.boolean(BoolOp::Difference, &[r, cl], tag).map_err(|e| fail("Differenz", e))?;
            pieces.push((rest, rem));
        }
        // drop empty pieces, unite pieces of one colour (order of first piece)
        let eps = (total.abs() * EMPTY_REL).max(EMPTY_ABS);
        let mut merged: Vec<(RegionColor, Vec<Piece<K::Shape>>)> = Vec::new();
        for (c, s) in pieces {
            if k.is_empty(&s) {
                continue;
            }
            let v = k.mass_props(&s).map_err(|e| fail("Volumen", e))?.volume;
            if v.abs() <= eps {
                continue;
            }
            match merged.iter_mut().find(|(x, _)| *x == c) {
                Some((_, l)) => l.push((s, v)),
                None => merged.push((c, vec![(s, v)])),
            }
        }
        // order: the rest colour (outermost) first, then colours in order of appearance
        let mut order = vec![rest];
        order.extend(placed.iter().map(|&(_, k, _)| RegionColor::Color(k)));
        merged.sort_by_key(|(c, _)| order.iter().position(|o| o == c).unwrap_or(usize::MAX));
        for (i, (c, mut l)) in merged.into_iter().enumerate() {
            let (shape, volume) = if l.len() == 1 {
                l.pop().unwrap()
            } else {
                let shapes: Vec<K::Shape> = l.iter().map(|(s, _)| s.clone()).collect();
                let u = k.boolean(BoolOp::Union, &shapes, tag).map_err(|e| fail("Vereinigung", e))?;
                let v = k.mass_props(&u).map_err(|e| fail("Volumen", e))?.volume;
                (u, v)
            };
            let rkey = KeyHasher::new().key(&key).u64(i as u64).finish();
            out.regions.push(Region { color: c, shapes: vec![shape], volume, key: rkey });
        }
        let sum: f64 = out.regions.iter().map(|r| r.volume).sum();
        if (sum - total).abs() > 1e-4 * total.abs() + 1e-3 {
            out.warning = Some(format!(
                "Farbregionen ungenau: Summe {sum:.3} mm³ statt {total:.3} mm³ (Boolean des Kernels unvollständig)"
            ));
        }
        Ok(out)
    }

    /// Mesh of region `i` (node system), cached per region key and tolerance.
    pub fn region_mesh(&mut self, regions: &ColorRegions<K::Shape>, i: usize, tol: f64) -> Result<Rc<Mesh>, String> {
        let r = regions.regions.get(i).ok_or("keine solche Farbregion")?;
        let slot = Slot::Mesh(r.key, crate::key::canon_f64(tol));
        if let Some(Cached::Mesh(m)) = self.cache.get(&slot) {
            return Ok(m);
        }
        let mut mesh = Mesh::default();
        for s in &r.shapes {
            let m = self.kernel.tessellate(s, tol).map_err(|e| e.to_string())?;
            append_offset(&mut mesh, &m);
        }
        let cost = mesh_bytes(&mesh);
        let mesh = Rc::new(mesh);
        self.cache.insert(slot, Cached::Mesh(mesh.clone()), cost);
        Ok(mesh)
    }
}
