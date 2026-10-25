//! Colour regions and filament slots (Plan 16.2, 16.3).
//!
//! **Colour table** ([`ColorTable::of`], no kernel): the colours of the *material tree* (the
//! result without `%`, `*`, zones and the subtracted operands of `difference`) in order of
//! first appearance (pre-order), one entry per RGBA value. Material from a primitive,
//! extrusion or surface without any `color()` above it is the *default* entry ("Standard").
//!
//! **Filament slots:** a `filament(c, n);` statement at the top level of the main file sets
//! the slot of colour `c` for the model (Bericht 12, F2); otherwise an entry with
//! `filament = n` at one of its `color()` calls gets slot `n` (the first explicit value
//! counts; the evaluator warns about conflicting ones). The
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
//! Then `piece(C) = R ∩ M(C) − ⋃ M(later, other colour)`, `rest = R − ⋃ M(all)`; pieces of
//! one colour are united (a same-coloured neighbour is not subtracted: no slivers). The
//! volumes add up to the volume of the result (up to the kernel tolerance; empty pieces are
//! dropped).
//!
//! `color()` inside 2D subtrees (below an extrusion without a colour of its own, Bericht 12,
//! F3): one colour for all 2D leaves → the extrusion is a body of that colour (as if `color()`
//! were around it); several colours → one body of the default colour and a warning.

use std::rc::Rc;

use cad_ir::{Dim, NodeIdx, Op, Tree};
use cad_kernel::{BoolOp, Kernel, Mesh};
use glam::{DMat4, DVec3};
use serde::Serialize;

use crate::key::{CacheKey, KeyHasher};
use crate::{Cached, Engine, Evaluation, MeshPart, Slot, append_offset, mesh_bytes};

/// Colour identity: RGB bytes, alpha always 255 (alpha only changes the view: a print colour
/// is its RGB value, `color("red", alpha = 0.5)` is the entry `red`).
pub type ColorKey = [u8; 4];

pub fn color_key(c: [f32; 4]) -> ColorKey {
    let mut k = c.map(|x| (x.clamp(0.0, 1.0) * 255.0).round() as u8);
    k[3] = 255;
    k
}

/// Highest filament slot (Bambu Studio and OrcaSlicer: 16; `cad_eval::palette::MAX_FILAMENT`).
pub const MAX_SLOT: u32 = 16;

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
    /// The `filament(c, n)` statement (index into `Tree::filaments`) that set the slot.
    pub decl: Option<usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ColorTable {
    pub entries: Vec<ColorEntry>,
    /// Profile operations with 2D content of several colours (one body, warning).
    pub profile_remarks: Vec<(NodeIdx, bool, String)>,
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
            | Op::Hull
            | Op::Minkowski
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

/// Colours of the 2D content of a profile operation (`linear_extrude`, …) without a colour in
/// effect (Bericht 12, F3): all 2D leaves in one colour → `Uniform(colour, first 2D color()
/// node)` (the body prints in it, as if `color()` were around the extrusion); leaves of
/// several colours, or partly uncoloured → `Mixed` (one body, default colour, warning);
/// no colour → `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProfileColor {
    None,
    Uniform(ColorKey, NodeIdx),
    Mixed,
}

pub(crate) fn profile_color(tree: &Tree, n: NodeIdx) -> ProfileColor {
    let node = tree.node(n);
    if !crate::is_profile_op(&node.op) || node.color.is_some() {
        return ProfileColor::None;
    }
    let mut leaves: Vec<Option<ColorKey>> = Vec::new();
    let mut first: Option<NodeIdx> = None;
    let mut stack: Vec<NodeIdx> = tree.solid_children(n).collect();
    while let Some(i) = stack.pop() {
        let c = tree.node(i);
        if first.is_none() && matches!(c.op, Op::Color { .. }) {
            first = Some(i);
        }
        if matches!(c.op, Op::Profile(_)) {
            leaves.push(c.color.map(color_key));
        }
        // the first operand of a 2D difference gives the material
        let kids: Vec<NodeIdx> = match &c.op {
            Op::Boolean { op: BoolOp::Difference, .. } => tree.solid_children(i).take(1).collect(),
            _ => tree.solid_children(i).collect(),
        };
        stack.extend(kids);
    }
    let known: Vec<ColorKey> = leaves.iter().flatten().copied().collect();
    if known.is_empty() {
        return ProfileColor::None;
    }
    if known.len() == leaves.len() && known.iter().all(|k| *k == known[0]) {
        // the color() node of that colour that comes first in pre-order
        let mut best: Option<NodeIdx> = None;
        let mut st = vec![n];
        while let Some(i) = st.pop() {
            if let Op::Color { rgba, .. } = &tree.node(i).op
                && color_key(*rgba) == known[0]
            {
                best = Some(best.map_or(i, |b: NodeIdx| b.min(i)));
            }
            st.extend(tree.solid_children(i));
        }
        return ProfileColor::Uniform(known[0], best.or(first).unwrap_or(n));
    }
    ProfileColor::Mixed
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
                match profile_color(tree, i) {
                    // colour of the 2D profile: the 2D color() node gives the entry
                    ProfileColor::Uniform(k, c) => order.push((k, c)),
                    _ => uncolored = true,
                }
            }
        });
        walk(tree, root, &mut |i, eff| {
            if eff.is_none() && profile_color(tree, i) == ProfileColor::Mixed {
                t.profile_remarks.push((
                    i,
                    true,
                    format!(
                        "Farben im 2D-Profil von {} werden nicht getrennt: der Körper druckt als Standard – color() um die Extrusion setzen oder je Farbe extrudieren",
                        tree.node(i).op.label()
                    ),
                ));
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
                decl: None,
            });
        }
        for (key, i) in order {
            let Op::Color { rgba, name, filament } = &tree.node(i).op else { continue };
            if t.entries.iter().any(|e| e.nodes.contains(&i)) {
                continue;
            }
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
                        decl: None,
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
        // `filament(c, n)` statements: the slot table of the model (the evaluator drops
        // conflicting `filament = m` at `color()`)
        for (di, d) in tree.filaments.iter().enumerate() {
            let key = color_key(d.rgba);
            if let Some(e) = t.entries.iter_mut().find(|e| e.key == key && e.source != ColorSource::Default)
                && e.decl.is_none()
            {
                e.filament = d.slot;
                e.explicit = true;
                e.explicit_node = None;
                e.decl = Some(di);
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

    /// Remarks about the slots (Bericht 12, F8): `(node, warning?, text)` with the `color()`
    /// node to show them at; `warning = false` is a hint.
    /// * more than [`MAX_SLOT`] slots (automatic ones beyond 16): warning;
    /// * two colours on the same explicit slot: hint (they print with one filament);
    /// * an explicit slot 1 next to material without `color()` (slot 1): hint.
    pub fn remarks(&self) -> Vec<(NodeIdx, bool, String)> {
        let mut out = self.profile_remarks.clone();
        let default = self.default_entry().is_some();
        for (i, e) in self.entries.iter().enumerate() {
            let Some(&node) = e.explicit_node.as_ref().or(e.nodes.first()) else { continue };
            if e.filament > MAX_SLOT {
                out.push((
                    node,
                    true,
                    format!(
                        "Farbe `{}` bekommt Slot {} – mehr als {MAX_SLOT} Filamente (Bambu Studio/OrcaSlicer: höchstens {MAX_SLOT}); Farben zusammenlegen oder filament = n setzen",
                        e.label, e.filament
                    ),
                ));
                continue;
            }
            if !e.explicit {
                continue;
            }
            if let Some(o) =
                self.entries[..i].iter().find(|o| o.filament == e.filament && o.source != ColorSource::Default)
            {
                out.push((
                    node,
                    false,
                    format!(
                        "Slot {} hat schon die Farbe `{}`: beide drucken mit demselben Filament",
                        e.filament, o.label
                    ),
                ));
            } else if e.filament == 1 && default {
                out.push((
                    node,
                    false,
                    format!("Slot 1 druckt auch das Material ohne color() („Standard“) – `{}` und Standard teilen sich das Filament", e.label),
                ));
            }
        }
        out
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
    /// Meshes without B-rep of this colour (node system).
    pub meshes: Vec<MeshPart>,
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
        } else if eff.is_none()
            && let ProfileColor::Uniform(k, _) = profile_color(tree, i)
        {
            // an extrusion of a coloured profile: its body is the colour's material
            out.push((i, k));
        }
    });
    out
}

/// Has the material tree below `p` a primitive without colour?
fn has_uncolored(tree: &Tree, p: NodeIdx) -> bool {
    let mut found = false;
    walk(tree, p, &mut |i, eff| {
        if eff.is_none()
            && creates_material(&tree.node(i).op)
            && tree.dim(i) == Some(Dim::Three)
            && !matches!(profile_color(tree, i), ProfileColor::Uniform(..))
        {
            found = true;
        }
    });
    found
}

/// Is the result of `n` the plain union of the placed colour nodes' material (only groups,
/// transforms, colours and unions without blends between `n` and them, every material leaf
/// below a placed colour node)? Then `R ∩ M(C) = M(C)` and nothing is left for the rest
/// colour: the regions need no Boolean with the result (Bericht 16: multi-coloured text).
fn covered_by_union(tree: &Tree, n: NodeIdx, placed: &[(NodeIdx, ColorKey, DMat4)]) -> bool {
    if placed.iter().any(|p| p.0 == n) {
        return true;
    }
    let node = tree.node(n);
    let union_like = match &node.op {
        Op::Group | Op::ModuleCall { .. } | Op::Transform { .. } | Op::Color { .. } | Op::Empty => true,
        Op::Boolean { op: BoolOp::Union, fillet, chamfer, fillet_law } => {
            fillet.is_none() && chamfer.is_none() && fillet_law.is_none()
        }
        _ => false,
    };
    let kids: Vec<NodeIdx> = tree.solid_children(n).collect();
    union_like && !kids.is_empty() && kids.iter().all(|&c| covered_by_union(tree, c, placed))
}

/// Boxes overlap with positive volume (touching boxes do not).
fn boxes_overlap(a: (DVec3, DVec3), b: (DVec3, DVec3)) -> bool {
    let size = (a.1 - a.0).max(b.1 - b.0).max_element().max(1.0);
    let eps = 1e-6 * size;
    (0..3).all(|i| a.1[i].min(b.1[i]) - a.0[i].max(b.0[i]) > eps)
}

/// The region colour of every mesh of `node`: the latest colour node in `placed` (pre-order,
/// with placements into `node`) whose result holds the same mesh at the same place, else
/// `rest`.
fn mesh_colors<S>(
    ev: &Evaluation<S>,
    node: NodeIdx,
    rest: RegionColor,
    placed: &[(NodeIdx, ColorKey, DMat4)],
) -> Vec<RegionColor> {
    ev.meshes(node)
        .iter()
        .map(|mp| {
            placed
                .iter()
                .rev()
                .find(|(c, _, m)| {
                    ev.meshes(*c).iter().any(|q| {
                        std::sync::Arc::ptr_eq(&q.mesh, &mp.mesh)
                            && (*m * q.place).abs_diff_eq(mp.place, 1e-9 * (1.0 + mp.place.w_axis.length()))
                    })
                })
                .map_or(rest, |&(_, k, _)| RegionColor::Color(k))
        })
        .collect()
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
        // the Booleans only see colour nodes with B-rep material (meshes are assigned below)
        let solid_placed: Vec<(NodeIdx, ColorKey, DMat4)> =
            placed.iter().filter(|(c, _, _)| ev.parts(*c).iter().any(|p| p.solid)).copied().collect();
        let whole = if solid_placed.is_empty() { Some(self.brep_volume(ev, node)?) } else { None };
        let mut out = self.compute_regions(ev, node, rest, &solid_placed, key, whole)?;
        out.colored_nodes = placed.len();
        // meshes: each in the colour of the latest colour node it comes from (no Booleans)
        let colors = mesh_colors(ev, node, rest, &placed);
        for (i, (mp, c)) in ev.meshes(node).iter().zip(colors).enumerate() {
            let v = mp.mesh.mass_props(&mp.place).volume;
            out.total += v;
            match out.regions.iter_mut().find(|r| r.color == c) {
                Some(r) => {
                    r.meshes.push(mp.clone());
                    r.volume += v;
                }
                None => {
                    let rkey = KeyHasher::new().key(&key).str("mesh").u64(i as u64).finish();
                    out.regions.push(Region {
                        color: c,
                        shapes: Vec::new(),
                        meshes: vec![mp.clone()],
                        volume: v,
                        key: rkey,
                    });
                }
            }
        }
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
                out.regions.push(Region { color: rest, shapes: solids, meshes: Vec::new(), volume, key: rkey });
            }
            return Ok(out);
        }
        if covered_by_union(&ev.tree, node, placed) {
            return self.union_regions(ev, node, placed, key, out);
        }
        let k = &mut self.kernel;
        let r = if solids.len() == 1 {
            solids[0].clone()
        } else {
            k.boolean(BoolOp::Union, &solids, tag).map_err(|e| fail("Vereinigung", e))?
        };
        let total = k.mass_props(&r).map_err(|e| fail("Volumen", e))?.volume;
        out.total = total;
        // Later statements win, but only over other colours: a piece is reduced by the
        // material of later colour nodes of *other* colours (Bericht 12, F6: subtracting a
        // touching neighbour of the same colour left slivers; the same colour's pieces are
        // united below anyway, so the ownership is the same).
        let mut pieces: Vec<(RegionColor, K::Shape)> = Vec::new();
        // material of the colour nodes processed so far (later in pre-order), per colour
        let mut claimed: Vec<(ColorKey, K::Shape)> = Vec::new();
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
            let others: Vec<K::Shape> = claimed.iter().filter(|(x, _)| *x != ck).map(|(_, s)| s.clone()).collect();
            if !others.is_empty() && !k.is_empty(&piece) {
                let mut ops = vec![piece];
                ops.extend(others);
                piece = k.boolean(BoolOp::Difference, &ops, ctag).map_err(|e| fail("Differenz", e))?;
            }
            match claimed.iter_mut().find(|(x, _)| *x == ck) {
                Some((_, cl)) => {
                    *cl = k.boolean(BoolOp::Union, &[cl.clone(), mc], ctag).map_err(|e| fail("Vereinigung", e))?
                }
                None => claimed.push((ck, mc)),
            }
            pieces.push((RegionColor::Color(ck), piece));
        }
        pieces.reverse();
        if !claimed.is_empty() {
            let mut ops = vec![r];
            ops.extend(claimed.into_iter().map(|(_, s)| s));
            let rem = k.boolean(BoolOp::Difference, &ops, tag).map_err(|e| fail("Differenz", e))?;
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
            out.regions.push(Region { color: c, shapes: vec![shape], meshes: Vec::new(), volume, key: rkey });
        }
        let sum: f64 = out.regions.iter().map(|r| r.volume).sum();
        if (sum - total).abs() > 1e-4 * total.abs() + 1e-3 {
            out.warning = Some(format!(
                "Farbregionen ungenau: Summe {sum:.3} mm³ statt {total:.3} mm³ (Boolean des Kernels unvollständig)"
            ));
        }
        Ok(out)
    }

    /// Regions of a result that is the plain union of its colour nodes ([`covered_by_union`]):
    /// a colour node's material minus the later material of other colours whose bounds overlap
    /// it; no Boolean with the whole result, none between bodies that are apart or only touch
    /// (a sign with text on it: seconds instead of minutes). Same result as the general rule.
    fn union_regions(
        &mut self,
        ev: &Evaluation<K::Shape>,
        node: NodeIdx,
        placed: &[(NodeIdx, ColorKey, DMat4)],
        key: CacheKey,
        mut out: ColorRegions<K::Shape>,
    ) -> Result<ColorRegions<K::Shape>, String> {
        type Bounds = (DVec3, DVec3);
        let total = self.brep_volume(ev, node)?;
        out.total = total;
        let k = &mut self.kernel;
        let fail = |what: &str, e: cad_kernel::KernelError| format!("Farbregionen: {what} gescheitert: {e}");
        let mut later: Vec<(ColorKey, K::Shape, Bounds)> = Vec::new();
        let mut pieces: Vec<(ColorKey, K::Shape, Bounds)> = Vec::new();
        for &(c, ck, m) in placed.iter().rev() {
            let ctag = ev.reports[c as usize].tag;
            for p in ev.parts(c).iter().filter(|p| p.solid) {
                let s = if m.abs_diff_eq(DMat4::IDENTITY, 1e-12) {
                    p.shape.clone()
                } else {
                    k.transform(&p.shape, &m).map_err(|e| fail("Transformation", e))?
                };
                let Some(b) = k.bounds(&s).map_err(|e| fail("Hülle", e))? else { continue };
                let cut: Vec<K::Shape> = later
                    .iter()
                    .filter(|(x, _, bb)| *x != ck && boxes_overlap(b, *bb))
                    .map(|(_, s, _)| s.clone())
                    .collect();
                let piece = if cut.is_empty() {
                    s.clone()
                } else {
                    let mut ops = vec![s.clone()];
                    ops.extend(cut);
                    k.boolean(BoolOp::Difference, &ops, ctag).map_err(|e| fail("Differenz", e))?
                };
                later.push((ck, s, b));
                pieces.push((ck, piece, b));
            }
        }
        pieces.reverse();
        let tag = ev.reports[node as usize].tag;
        let eps = (total.abs() * EMPTY_REL).max(EMPTY_ABS);
        // colours in order of appearance; a colour's pieces stay apart unless they overlap
        let mut order: Vec<ColorKey> = Vec::new();
        for &(_, ck, _) in placed {
            if !order.contains(&ck) {
                order.push(ck);
            }
        }
        for ck in order {
            let mine: Vec<&(ColorKey, K::Shape, Bounds)> = pieces.iter().filter(|p| p.0 == ck).collect();
            let apart = mine.iter().enumerate().all(|(i, a)| mine[i + 1..].iter().all(|b| !boxes_overlap(a.2, b.2)));
            let shapes: Vec<K::Shape> = if apart || mine.len() < 2 {
                mine.iter().map(|p| p.1.clone()).collect()
            } else {
                let all: Vec<K::Shape> = mine.iter().map(|p| p.1.clone()).collect();
                vec![k.boolean(BoolOp::Union, &all, tag).map_err(|e| fail("Vereinigung", e))?]
            };
            let mut kept = Vec::new();
            let mut volume = 0.0;
            for s in shapes {
                if k.is_empty(&s) {
                    continue;
                }
                let v = k.mass_props(&s).map_err(|e| fail("Volumen", e))?.volume;
                if v.abs() > eps {
                    volume += v;
                    kept.push(s);
                }
            }
            if kept.is_empty() {
                continue;
            }
            let rkey = KeyHasher::new().key(&key).u64(out.regions.len() as u64).finish();
            out.regions.push(Region {
                color: RegionColor::Color(ck),
                shapes: kept,
                meshes: Vec::new(),
                volume,
                key: rkey,
            });
        }
        let sum: f64 = out.regions.iter().map(|r| r.volume).sum();
        if (sum - total).abs() > 1e-4 * total.abs() + 1e-3 {
            out.warning = Some(format!(
                "Farbregionen ungenau: Summe {sum:.3} mm³ statt {total:.3} mm³ (Boolean des Kernels unvollständig)"
            ));
        }
        Ok(out)
    }

    /// Region colour of every face of part `part` of a node's result, by [`cad_kernel::FaceId`]
    /// (Bericht 12, F1/F3; STEP colours). The rule of the regions without booleans: a face
    /// gets the colour of the latest colour node (pre-order) whose placed material contains
    /// its sample points (inside or on the boundary), otherwise the rest colour; majority of
    /// the face's sample points. Faces are not split (a face merged across a colour boundary
    /// gets one colour; the 3MF regions split it).
    pub fn face_region_colors(
        &mut self,
        ev: &Evaluation<K::Shape>,
        node: NodeIdx,
        part: usize,
    ) -> Result<Vec<RegionColor>, String> {
        let tree = ev.tree.clone();
        let inner = inner_color_nodes(&tree, node);
        let base = effective(&tree, node);
        let rest = match (base, inner.first()) {
            (Some(b), _) => RegionColor::Color(b),
            (None, _) if has_uncolored(&tree, node) => RegionColor::Default,
            (None, Some(&(_, k))) => RegionColor::Color(k),
            (None, None) => RegionColor::Default,
        };
        let n = ev.parts(node).len();
        if part >= n {
            // a mesh: one colour (the colour of the latest colour node it comes from)
            let mp = ev.meshes(node).get(part - n).ok_or("kein solcher Teil")?;
            let placed: Vec<(NodeIdx, ColorKey, DMat4)> =
                inner.iter().filter_map(|&(c, k)| placement(ev, node, c).map(|m| (c, k, m))).collect();
            let c = mesh_colors(ev, node, rest, &placed)[part - n];
            return Ok(vec![c; mp.mesh.face_count()]);
        }
        let shape = ev.parts(node).get(part).ok_or("kein solcher Teil")?.shape.clone();
        let fail = |what: &str, e: cad_kernel::KernelError| format!("Flächenfarben: {what} gescheitert: {e}");
        let faces = self.kernel.faces(&shape, 3).map_err(|e| fail("Flächen", e))?;
        if inner.is_empty() {
            return Ok(vec![rest; faces.len()]);
        }
        let pts: Vec<DVec3> = faces.iter().flat_map(|f| f.points.iter().copied()).collect();
        // owner per sample point: index into `inner` of the latest colour node containing it
        let mut owner: Vec<Option<usize>> = vec![None; pts.len()];
        for (ci, &(c, _)) in inner.iter().enumerate() {
            let m = placement(ev, node, c).ok_or("Farbknoten liegt nicht unter dem Knoten")?;
            for p in ev.parts(c).iter().filter(|p| p.solid) {
                let s = if m.abs_diff_eq(DMat4::IDENTITY, 1e-12) {
                    p.shape.clone()
                } else {
                    self.kernel.transform(&p.shape, &m).map_err(|e| fail("Transformation", e))?
                };
                let cls = self.kernel.classify(&s, &pts).map_err(|e| fail("Klassifikation", e))?;
                for (o, c) in owner.iter_mut().zip(cls) {
                    if c != cad_kernel::Class::Outside {
                        *o = Some(ci);
                    }
                }
            }
        }
        let mut out = Vec::with_capacity(faces.len());
        let mut at = 0;
        for f in &faces {
            let mut votes: Vec<(RegionColor, usize)> = Vec::new();
            for o in &owner[at..at + f.points.len()] {
                let c = o.map_or(rest, |ci| RegionColor::Color(inner[ci].1));
                match votes.iter_mut().find(|(x, _)| *x == c) {
                    Some(v) => v.1 += 1,
                    None => votes.push((c, 1)),
                }
            }
            at += f.points.len();
            // most votes; ties: the later colour (owners are in pre-order)
            let best = votes.iter().rev().max_by_key(|(_, n)| *n).map_or(rest, |(c, _)| *c);
            out.push(best);
        }
        Ok(out)
    }

    /// Volume of the B-rep parts of a node (its volume without the meshes).
    fn brep_volume(&mut self, ev: &Evaluation<K::Shape>, node: NodeIdx) -> Result<f64, String> {
        let meshes: f64 = ev.meshes(node).iter().map(|m| m.mesh.mass_props(&m.place).volume).sum();
        Ok(self.mass_props(ev, node)?.volume - meshes)
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
        for m in &r.meshes {
            append_offset(&mut mesh, &m.display());
        }
        let cost = mesh_bytes(&mesh);
        let mesh = Rc::new(mesh);
        self.cache.insert(slot, Cached::Mesh(mesh.clone()), cost);
        Ok(mesh)
    }
}
