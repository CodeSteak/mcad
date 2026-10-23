//! Built-in modules: primitives, transformations, booleans, blends, profile
//! operations, `children()`, `echo()`, `assert()`.

use cad_ir::{BlendKind, NodeIdx, Op, Prim, Profile, Select, TransformKind, ZoneShape};
use cad_kernel::{BoolOp, PathInterp};
use cad_syntax::Span;
use cad_syntax::ast::{Ident, Stmt};
use glam::{DMat4, DVec2, DVec3};

use crate::env::{Dyn, dyn_frame, dyn_lookup, dyn_push};
use crate::eval::{ArgV, Args, Ctx, Ev, R};
use crate::math::{self, cos_deg, sin_deg};
use crate::output::HandleKind;
use crate::palette::{self, ColorKind, parse_color};
use crate::value::{Sel, Val, Value, fmt_num};

pub(crate) const BUILTIN_MODULES: &[&str] = &[
    "cube",
    "box",
    "sphere",
    "cylinder",
    "torus",
    "polyhedron",
    "square",
    "rect",
    "circle",
    "polygon",
    "spline",
    "translate",
    "rotate",
    "scale",
    "mirror",
    "multmatrix",
    "resize",
    "color",
    "filament",
    "union",
    "difference",
    "intersection",
    "fillet",
    "chamfer",
    "zone",
    "linear_extrude",
    "rotate_extrude",
    "loft",
    "sweep",
    "surface",
    "thicken",
    "shell",
    "children",
    "echo",
    "assert",
    "render",
    "group",
    "hull",
    "minkowski",
    "offset",
    "projection",
    "text",
    "import",
];

const LEAVES: &[&str] = &[
    "cube",
    "box",
    "sphere",
    "cylinder",
    "torus",
    "polyhedron",
    "square",
    "rect",
    "circle",
    "polygon",
    "spline",
    "surface",
    "zone",
    "text",
    "import",
];

type Slots<'x, 'a> = Vec<Option<&'x ArgV<'a>>>;

impl<'a> Ev<'a> {
    /// Assigns arguments of a built-in module to parameter slots. The first `npos`
    /// names also take positional arguments; named arguments win.
    fn bind<'x>(&mut self, what: &str, a: &'x Args<'a>, names: &[&str], npos: usize) -> Slots<'x, 'a> {
        let mut slots: Slots<'x, 'a> = vec![None; names.len()];
        let mut pi = 0;
        for arg in a.list.iter().filter(|x| x.name.is_none()) {
            if pi < npos {
                slots[pi] = Some(arg);
                pi += 1;
            } else {
                self.warn(arg.expr.span, format!("zu viele Argumente für `{what}()`"));
            }
        }
        for arg in &a.list {
            if let Some(n) = arg.name {
                match names.iter().position(|x| *x == n.name) {
                    Some(i) => slots[i] = Some(arg),
                    None => self.warn(n.span, format!("`{what}()` hat keinen Parameter `{}`", n.name)),
                }
            }
        }
        // Grids, lines, planes and instances only where explicitly accepted.
        for (i, slot) in slots.iter_mut().enumerate() {
            if let Some(arg) = *slot
                && arg.val.is_special()
                && !matches!((what, names[i]), ("mirror", "v") | ("zone", "target"))
            {
                self.special_misuse(arg, names[i]);
                *slot = None;
            }
        }
        slots
    }

    fn num_of(&mut self, a: Option<&ArgV<'a>>, what: &str) -> Option<f64> {
        let a = a?;
        match a.val.as_num() {
            Some(n) => Some(n),
            None => {
                if a.val.is_special() {
                    self.special_misuse(a, what);
                } else if !a.val.is_undef() {
                    let t = a.val.type_de();
                    self.warn(a.expr.span, format!("`{what}` muss eine Zahl sein, ist {t}"));
                }
                None
            }
        }
    }

    fn vec3_of(&mut self, a: Option<&ArgV<'a>>, what: &str) -> Option<DVec3> {
        let a = a?;
        match a.val.as_vec3() {
            Some(v) => Some(v),
            None => {
                if a.val.is_special() {
                    self.special_misuse(a, what);
                } else {
                    let t = a.val.type_de();
                    self.warn(a.expr.span, format!("`{what}` muss ein Vektor [x, y, z] sein, ist {t}"));
                }
                None
            }
        }
    }

    /// A grid, grid line/plane or instance where a number or point is expected.
    fn special_misuse(&mut self, a: &ArgV<'a>, what: &str) {
        let t = a.val.type_de();
        let hint = match a.val.v {
            Value::GridSel { .. } => "; Linien und Ebenen nur in zone() und mirror()",
            Value::Instance(_) => "; Anker `k.name` verwenden",
            Value::Family(_) => "; Anker `k[i].name` verwenden",
            _ => "; Rasterpunkt `g[x, y, z]` oder Ebene `g.name` verwenden",
        };
        self.error(a.expr.span, format!("Typfehler: `{what}` erwartet Zahl oder Punkt, ist {t}{hint}"));
    }

    /// 2D point; a z component other than 0 is projected away with a warning.
    fn pt2(&mut self, v: &Val<'a>, span: Span) -> Option<DVec2> {
        let l = v.as_list()?;
        let p = v.as_vec2()?;
        if l.len() >= 3
            && let Some(z) = l[2].as_num()
            && z != 0.0
        {
            self.warn(span, "z-Koordinate ignoriert: 2D-Formen liegen in der xy-Ebene");
        }
        Some(p)
    }

    fn set_blend(&mut self, n: NodeIdx, dy: &Dyn<'a>) {
        let get = |name: &str| dyn_lookup(dy, name).and_then(|v| v.as_num()).filter(|x| *x > 0.0);
        let blend =
            get("$fillet").map(|r| (BlendKind::Fillet, r)).or_else(|| get("$chamfer").map(|r| (BlendKind::Chamfer, r)));
        let tol = get("$tol");
        let node = &mut self.nodes[n as usize];
        node.blend = blend;
        node.tol = tol;
    }

    fn prim(&mut self, s: &Stmt, prim: Prim, place: DMat4, ctx: &Ctx<'a>, dy: &Dyn<'a>) -> NodeIdx {
        let n = self.node(s, Op::Prim { prim, place }, ctx);
        self.set_blend(n, dy);
        if dyn_lookup(dy, "$fillet").is_some_and(|v| matches!(v.v, Value::Func(_))) {
            self.warn(
                s.span,
                "variabler Radius über `$fillet` nicht unterstützt: `fillet(r = function (p) …) { … }` verwenden",
            );
        }
        n
    }

    /// A primitive with invalid dimensions: OpenSCAD makes nothing. Empty node (it
    /// keeps the handles) and a warning.
    fn invalid_prim(&mut self, s: &Stmt, msg: &str, ctx: &Ctx<'a>) -> NodeIdx {
        self.warn(s.span, format!("{msg}, kein Körper"));
        self.node(s, Op::Empty, ctx)
    }

    fn profile(&mut self, s: &Stmt, p: Profile, ctx: &Ctx<'a>, dy: &Dyn<'a>) -> NodeIdx {
        let n = self.node(s, Op::Profile(p), ctx);
        self.nodes[n as usize].tol = dyn_lookup(dy, "$tol").and_then(|v| v.as_num()).filter(|x| *x > 0.0);
        n
    }

    /// `$fn` up to 12 directly at the call, or directly at an enclosing
    /// `linear_extrude` / `rotate_extrude` call → number of facets (OpenSCAD polygon;
    /// below 3 counts as 3 as in OpenSCAD). Otherwise exact geometry, with a hint if
    /// `$fn` is set.
    fn facets(&mut self, s: &Stmt, a: &Args<'a>, dy: &Dyn<'a>) -> Option<u32> {
        let direct = a.dyn_num("$fn").or_else(|| extrude_fn(dy));
        if let Some(n) = direct.and_then(facet_count) {
            return Some(n);
        }
        let eff = direct.or_else(|| dyn_lookup(dy, "$fn").and_then(|v| v.as_num())).unwrap_or(0.0);
        if eff > 0.0 {
            self.info(s.span, "`$fn` ignoriert, Geometrie exakt (Polygon nur bei `$fn` ≤ 12 direkt am Aufruf)");
        }
        None
    }

    #[inline(never)]
    pub(crate) fn builtin_module(
        &mut self,
        s: &'a Stmt,
        name: &'a Ident,
        a: Args<'a>,
        children: &'a [Stmt],
        ctx: &Ctx<'a>,
    ) -> R<()> {
        let n = name.name.as_str();
        if LEAVES.contains(&n) && children.iter().any(crate::eval::is_geometric) {
            self.warn(name.span, format!("`{n}()` nimmt keine Kinder"));
        }
        if a.dyn_vars.is_empty() {
            let dy = ctx.dyn_.clone();
            self.dispatch(s, name, &a, children, ctx, &dy)
        } else {
            let mut vars = a.dyn_vars.clone();
            if matches!(n, "linear_extrude" | "rotate_extrude") && a.dyn_num("$fn").is_some() {
                vars.push((EXTRUDE_FN, Val::bool(true)));
            }
            let dy = dyn_push(&ctx.dyn_, vars);
            let c = Ctx { dyn_: dy.clone(), ..ctx.clone() };
            self.dispatch(s, name, &a, children, &c, &dy)
        }
    }

    fn dispatch(
        &mut self,
        s: &'a Stmt,
        name: &'a Ident,
        a: &Args<'a>,
        children: &'a [Stmt],
        ctx: &Ctx<'a>,
        dy: &Dyn<'a>,
    ) -> R<()> {
        let n = name.name.as_str();
        match n {
            "cube" => self.m_cube(s, a, ctx, dy),
            "box" => self.m_box(s, a, ctx, dy),
            "sphere" => self.m_sphere(s, a, ctx, dy),
            "cylinder" => self.m_cylinder(s, a, ctx, dy),
            "torus" => self.m_torus(s, a, ctx, dy),
            "polyhedron" => self.m_polyhedron(s, a, ctx, dy),
            "square" => self.m_square(s, a, ctx, dy),
            "rect" => self.m_rect(s, a, ctx, dy),
            "circle" => self.m_circle(s, a, ctx, dy),
            "polygon" | "spline" => self.m_polygon(s, n, a, ctx, dy),
            "translate" | "rotate" | "scale" | "mirror" | "multmatrix" => {
                return self.m_transform(s, n, a, children, ctx, dy);
            }
            "color" => return self.m_color(s, a, children, ctx, dy),
            "filament" => self.m_filament(s, a),
            "fillet" | "chamfer" | "shell" => return self.m_blend(s, n, a, children, ctx, dy),
            "zone" => self.m_zone(s, a, ctx),
            "linear_extrude" | "rotate_extrude" | "loft" | "sweep" | "thicken" | "resize" => {
                return self.m_profile_op(s, n, a, children, ctx, dy);
            }
            "surface" => self.m_surface(s, a, ctx, dy),
            "children" => return self.children_stmt(s, a, ctx),
            _ => return self.m_group_like(s, name, a, children, ctx, dy),
        }
        Ok(())
    }

    /// Booleans, groups, echo/assert with children, unsupported operations.
    #[inline(never)]
    fn m_group_like(
        &mut self,
        s: &'a Stmt,
        name: &'a Ident,
        a: &Args<'a>,
        children: &'a [Stmt],
        ctx: &Ctx<'a>,
        dy: &Dyn<'a>,
    ) -> R<()> {
        let n = name.name.as_str();
        let op = match n {
            "union" | "difference" | "intersection" => {
                let p = self.bind(n, a, &["fillet", "chamfer"], 0);
                let (fillet, fillet_law) = self.fillet_param(p[0], "fillet", dy)?;
                let chamfer = self.blend_param(p[1], "chamfer");
                let op = match n {
                    "union" => BoolOp::Union,
                    "difference" => BoolOp::Difference,
                    _ => BoolOp::Intersection,
                };
                Op::Boolean { op, fillet, chamfer, fillet_law }
            }
            "group" | "render" => Op::Group,
            "echo" | "assert" => {
                if n == "echo" {
                    self.echo(s.span, a, &ctx.env);
                } else {
                    self.assert_args(s.span, a)?;
                }
                if children.is_empty() {
                    return Ok(());
                }
                Op::Group
            }
            "hull" | "minkowski" | "offset" | "projection" => {
                self.warn(name.span, format!("`{n}()` nicht unterstützt; Kinder werden unverändert gezeigt"));
                Op::Empty
            }
            "text" | "import" => {
                self.warn(name.span, format!("`{n}()` nicht unterstützt"));
                return Ok(());
            }
            _ => {
                self.warn(name.span, format!("unbekanntes Modul `{n}`"));
                return Ok(());
            }
        };
        let node = self.node(s, op, ctx);
        self.eval_children(node, &DMat4::IDENTITY, children, ctx, dy)
    }

    fn blend_param(&mut self, a: Option<&ArgV<'a>>, what: &str) -> Option<f64> {
        let a = a?;
        if matches!(a.val.v, Value::Func(_)) {
            self.warn(a.expr.span, format!("variabler Wert `{what}` (Funktion) wird hier nicht unterstützt"));
            return None;
        }
        self.num_of(Some(a), what)
    }

    /// Fillet radius: a number, or a function of the position (variable radius, compiled
    /// into a law). Compilation errors are reported; the result is then `(None, None)`.
    fn fillet_param(
        &mut self,
        a: Option<&ArgV<'a>>,
        what: &str,
        dy: &Dyn<'a>,
    ) -> R<(Option<f64>, Option<Box<cad_ir::Law>>)> {
        let Some(arg) = a else { return Ok((None, None)) };
        if let Value::Func(c) = &arg.val.v {
            let c = c.clone();
            return Ok(match self.compile_law(&c, arg.expr.span, dy)? {
                Ok(law) => (None, Some(Box::new(law))),
                Err(e) => {
                    self.error(e.span, format!("Radiusfunktion `{what}`: {}", e.msg));
                    (None, None)
                }
            });
        }
        Ok((self.num_of(Some(arg), what), None))
    }

    // ------------------------------------------------------------ 3D primitives

    #[inline(never)]
    fn m_cube(&mut self, s: &'a Stmt, a: &Args<'a>, ctx: &Ctx<'a>, dy: &Dyn<'a>) {
        let p = self.bind("cube", a, &["size", "center"], 2);
        let center = is_true(p[1]);
        let size = match p[0] {
            None => DVec3::ONE,
            Some(x) => match (x.val.as_num(), x.val.as_list()) {
                (Some(n), _) => DVec3::splat(n),
                (_, Some(l)) if l.len() == 3 && x.val.as_vec3().is_some() => x.val.as_vec3().unwrap(),
                _ => {
                    self.warn(x.expr.span, "cube: Größe muss Zahl oder [x, y, z] sein");
                    DVec3::ONE
                }
            },
        };
        let (lo, hi) = if center { (-size / 2.0, size / 2.0) } else { (DVec3::ZERO, size) };
        let node = if positive(&size.to_array()) {
            self.prim(s, Prim::Box { min: lo, max: hi }, DMat4::IDENTITY, ctx, dy)
        } else {
            self.invalid_prim(s, "cube: Größe muss positiv sein", ctx)
        };
        if let Some(x) = p[0] {
            self.point_handle(s, ctx, node, 0, x, hi, 3, if center { 2.0 } else { 1.0 });
        }
    }

    #[inline(never)]
    fn m_box(&mut self, s: &'a Stmt, a: &Args<'a>, ctx: &Ctx<'a>, dy: &Dyn<'a>) {
        let p = self.bind("box", a, &["a", "b"], 2);
        let pa = self.vec3_of(p[0], "a");
        let pb = self.vec3_of(p[1], "b");
        let (Some(pa), Some(pb)) = (pa, pb) else {
            self.error(s.span, "box() braucht zwei Eckpunkte `a` und `b`");
            return;
        };
        let node = self.prim(s, Prim::Box { min: pa.min(pb), max: pa.max(pb) }, DMat4::IDENTITY, ctx, dy);
        for (i, (x, v)) in [(p[0], pa), (p[1], pb)].into_iter().enumerate() {
            if let Some(x) = x {
                let dims = x.val.as_list().map_or(3, |l| l.len().min(3));
                self.point_handle(s, ctx, node, i as u32, x, v, dims, 1.0);
            }
        }
    }

    /// OpenSCAD `lookup_radius`: the diameter wins over the radius (with a warning if
    /// both are given). Returns the radius, its argument and `literal / radius`.
    fn radius<'x>(
        &mut self,
        r: Option<&'x ArgV<'a>>,
        d: Option<&'x ArgV<'a>>,
        rname: &str,
        dname: &str,
    ) -> Option<(f64, &'x ArgV<'a>, f64)> {
        let rv = self.num_of(r, rname);
        match (self.num_of(d, dname), d) {
            (Some(dv), Some(darg)) => {
                if let (Some(_), Some(rarg)) = (rv, r) {
                    self.warn(rarg.expr.span, format!("`{rname}` ignoriert, da auch `{dname}` angegeben ist"));
                }
                Some((dv / 2.0, darg, 2.0))
            }
            _ => Some((rv?, r?, 1.0)),
        }
    }

    #[inline(never)]
    fn m_sphere(&mut self, s: &'a Stmt, a: &Args<'a>, ctx: &Ctx<'a>, dy: &Dyn<'a>) {
        let p = self.bind("sphere", a, &["r", "d"], 1);
        let (r, src, scale) = match self.radius(p[0], p[1], "r", "d") {
            Some((r, x, sc)) => (r, Some(x), sc),
            None => (1.0, None, 1.0),
        };
        let node = match self.facets(s, a, dy) {
            _ if !positive(&[r]) => self.invalid_prim(s, "sphere: Radius muss positiv sein", ctx),
            Some(n) => {
                let (points, faces) = sphere_poly(r, n);
                self.prim(s, Prim::Polyhedron { points, faces }, DMat4::IDENTITY, ctx, dy)
            }
            None => self.prim(s, Prim::Sphere { r }, DMat4::IDENTITY, ctx, dy),
        };
        if let Some(x) = src {
            let w = self.nodes[node as usize].world;
            self.scalar_handle(
                s,
                ctx,
                node,
                HandleKind::Radius,
                0,
                &x.val,
                x.expr.span,
                r,
                scale,
                w,
                DVec3::ZERO,
                DVec3::X,
            );
        }
    }

    /// Bottom and top radius with their source arguments, as in OpenSCAD: `d1`
    /// before `r1`, then `d` before `r` (default 1).
    #[allow(clippy::type_complexity, clippy::too_many_arguments)]
    fn radii<'x>(
        &mut self,
        s: &Stmt,
        r1: Option<&'x ArgV<'a>>,
        r2: Option<&'x ArgV<'a>>,
        r: Option<&'x ArgV<'a>>,
        d: Option<&'x ArgV<'a>>,
        d1: Option<&'x ArgV<'a>>,
        d2: Option<&'x ArgV<'a>>,
    ) -> ((f64, Option<(&'x ArgV<'a>, f64)>), (f64, Option<(&'x ArgV<'a>, f64)>)) {
        let both = self.radius(r, d, "r", "d");
        let bottom = self.radius(r1, d1, "r1", "d1");
        let top = self.radius(r2, d2, "r2", "d2");
        if both.is_some() && (bottom.is_some() || top.is_some()) {
            self.warn(s.span, "mehrdeutige Zylinderparameter: `r`/`d` zusammen mit `r1`/`r2`/`d1`/`d2`");
        }
        let pick = |x: Option<(f64, &'x ArgV<'a>, f64)>| match x.or(both) {
            Some((v, arg, sc)) => (v, Some((arg, sc))),
            None => (1.0, None),
        };
        (pick(bottom), pick(top))
    }

    #[inline(never)]
    fn m_cylinder(&mut self, s: &'a Stmt, a: &Args<'a>, ctx: &Ctx<'a>, dy: &Dyn<'a>) {
        let pos: Vec<&ArgV<'a>> = a.positional().collect();
        let named = |n: &str| a.list.iter().any(|x| x.name.is_some_and(|i| i.name == n));
        let point_form = (pos.len() >= 2 && pos[0].val.as_list().is_some() && pos[1].val.as_list().is_some())
            || named("a")
            || named("b");
        if point_form {
            return self.m_cylinder_points(s, a, ctx, dy);
        }
        let p = self.bind("cylinder", a, &["h", "r1", "r2", "center", "r", "d", "d1", "d2"], 4);
        let h = self.num_of(p[0], "h").unwrap_or(1.0);
        let center = is_true(p[3]);
        let ((r1, s1), (r2, s2)) = self.radii(s, p[1], p[2], p[4], p[5], p[6], p[7]);
        let valid = positive(&[h]) && r1.is_finite() && r2.is_finite() && r1 >= 0.0 && r2 >= 0.0 && r1 + r2 > 0.0;
        let z0 = if center { -h / 2.0 } else { 0.0 };
        let node = match self.facets(s, a, dy) {
            _ if !valid => self.invalid_prim(s, "cylinder: braucht Höhe > 0 und Radien ≥ 0, nicht beide 0", ctx),
            Some(n) => {
                let (points, faces) = prism(n, h, r1, r2, z0);
                self.prim(s, Prim::Polyhedron { points, faces }, DMat4::IDENTITY, ctx, dy)
            }
            None => {
                self.prim(s, Prim::Cylinder { h, r1, r2 }, DMat4::from_translation(DVec3::new(0.0, 0.0, z0)), ctx, dy)
            }
        };
        let w = self.nodes[node as usize].world;
        if let Some(x) = p[0] {
            let (v, sc) = if center { (h / 2.0, 2.0) } else { (h, 1.0) };
            self.scalar_handle(
                s,
                ctx,
                node,
                HandleKind::Height,
                0,
                &x.val,
                x.expr.span,
                v,
                sc,
                w,
                DVec3::ZERO,
                DVec3::Z,
            );
        }
        if let Some((x, sc)) = s1 {
            self.scalar_handle(
                s,
                ctx,
                node,
                HandleKind::Radius,
                0,
                &x.val,
                x.expr.span,
                r1,
                sc,
                w,
                DVec3::new(0.0, 0.0, z0),
                DVec3::X,
            );
        }
        if let Some((x, sc)) = s2
            && !s1.is_some_and(|(y, _)| std::ptr::eq(x, y))
        {
            self.scalar_handle(
                s,
                ctx,
                node,
                HandleKind::Radius,
                1,
                &x.val,
                x.expr.span,
                r2,
                sc,
                w,
                DVec3::new(0.0, 0.0, z0 + h),
                DVec3::X,
            );
        }
    }

    #[inline(never)]
    fn m_cylinder_points(&mut self, s: &'a Stmt, a: &Args<'a>, ctx: &Ctx<'a>, dy: &Dyn<'a>) {
        let p = self.bind("cylinder", a, &["a", "b", "r", "r1", "r2", "d", "d1", "d2"], 3);
        let pa = self.vec3_of(p[0], "a");
        let pb = self.vec3_of(p[1], "b");
        let (Some(pa), Some(pb)) = (pa, pb) else {
            self.error(s.span, "cylinder(a, b, r) braucht zwei Punkte");
            return;
        };
        let ((r1, s1), (r2, s2)) = self.radii(s, p[3], p[4], p[2], p[5], p[6], p[7]);
        let d = pb - pa;
        let h = d.length();
        if h == 0.0 {
            self.warn(s.span, "cylinder(a, b, r): beide Punkte gleich");
        }
        let place = DMat4::from_translation(pa) * math::rot_to(d);
        let node = self.prim(s, Prim::Cylinder { h, r1, r2 }, place, ctx, dy);
        for (i, (x, v)) in [(p[0], pa), (p[1], pb)].into_iter().enumerate() {
            if let Some(x) = x {
                let dims = x.val.as_list().map_or(3, |l| l.len().min(3));
                self.point_handle(s, ctx, node, i as u32, x, v, dims, 1.0);
            }
        }
        let w = self.nodes[node as usize].world;
        let perp = math::perpendicular(d);
        if let Some((x, sc)) = s1 {
            self.scalar_handle(s, ctx, node, HandleKind::Radius, 0, &x.val, x.expr.span, r1, sc, w, pa, perp);
        }
        if let Some((x, sc)) = s2
            && !s1.is_some_and(|(y, _)| std::ptr::eq(x, y))
        {
            self.scalar_handle(s, ctx, node, HandleKind::Radius, 1, &x.val, x.expr.span, r2, sc, w, pb, perp);
        }
    }

    #[inline(never)]
    fn m_torus(&mut self, s: &'a Stmt, a: &Args<'a>, ctx: &Ctx<'a>, dy: &Dyn<'a>) {
        let p = self.bind("torus", a, &["R", "r"], 2);
        let big = self.num_of(p[0], "R");
        let small = self.num_of(p[1], "r");
        let (Some(big_r), Some(r)) = (big, small) else {
            self.error(s.span, "torus() braucht die Radien `R` und `r`");
            return;
        };
        if r <= 0.0 || big_r <= 0.0 {
            self.warn(s.span, "torus: Radien müssen positiv sein");
        }
        let node = self.prim(s, Prim::Torus { big_r, r }, DMat4::IDENTITY, ctx, dy);
        let w = self.nodes[node as usize].world;
        if let Some(x) = p[0] {
            self.scalar_handle(
                s,
                ctx,
                node,
                HandleKind::Radius,
                0,
                &x.val,
                x.expr.span,
                big_r,
                1.0,
                w,
                DVec3::ZERO,
                DVec3::X,
            );
        }
        if let Some(x) = p[1] {
            self.scalar_handle(
                s,
                ctx,
                node,
                HandleKind::Radius,
                1,
                &x.val,
                x.expr.span,
                r,
                1.0,
                w,
                DVec3::new(big_r, 0.0, 0.0),
                DVec3::Z,
            );
        }
    }

    #[inline(never)]
    fn m_polyhedron(&mut self, s: &'a Stmt, a: &Args<'a>, ctx: &Ctx<'a>, dy: &Dyn<'a>) {
        let p = self.bind("polyhedron", a, &["points", "faces", "convexity", "triangles"], 3);
        let Some(pts_arg) = p[0] else {
            self.error(s.span, "polyhedron() braucht `points`");
            return;
        };
        let Some(points) =
            pts_arg.val.as_list().and_then(|l| l.iter().map(|v| v.as_vec3()).collect::<Option<Vec<_>>>())
        else {
            self.error(pts_arg.expr.span, "polyhedron: `points` muss eine Liste von [x, y, z] sein");
            return;
        };
        let Some(faces_arg) = p[1].or(p[3]) else {
            self.error(s.span, "polyhedron() braucht `faces`");
            return;
        };
        let n = points.len() as f64;
        let faces: Option<Vec<Vec<u32>>> = faces_arg.val.as_list().and_then(|l| {
            l.iter()
                .map(|f| {
                    f.as_list()?
                        .iter()
                        .map(|i| i.as_num().filter(|x| *x >= 0.0 && *x < n && x.fract() == 0.0).map(|x| x as u32))
                        .collect()
                })
                .collect()
        });
        let Some(faces) = faces else {
            self.error(faces_arg.expr.span, "polyhedron: `faces` müssen Listen von Punktindizes sein");
            return;
        };
        let node = self.prim(s, Prim::Polyhedron { points: points.clone(), faces }, DMat4::IDENTITY, ctx, dy);
        let w = self.nodes[node as usize].world;
        if let Some(l) = pts_arg.val.as_list() {
            for (i, (e, pt)) in l.iter().zip(points).enumerate() {
                self.elem_handle(s, ctx, node, i as u32, pts_arg, e, pt, 3, w);
            }
        }
    }

    // ------------------------------------------------------------ 2D

    #[inline(never)]
    fn m_square(&mut self, s: &'a Stmt, a: &Args<'a>, ctx: &Ctx<'a>, dy: &Dyn<'a>) {
        let p = self.bind("square", a, &["size", "center"], 2);
        let center = is_true(p[1]);
        let size = match p[0] {
            None => DVec2::ONE,
            Some(x) => match (x.val.as_num(), x.val.as_vec2()) {
                (Some(n), _) => DVec2::splat(n),
                (_, Some(v)) => v,
                _ => {
                    self.warn(x.expr.span, "square: Größe muss Zahl oder [x, y] sein");
                    DVec2::ONE
                }
            },
        };
        let (lo, hi) = if center { (-size / 2.0, size / 2.0) } else { (DVec2::ZERO, size) };
        let node = if positive(&size.to_array()) {
            self.profile(s, Profile::Rect { min: lo, max: hi }, ctx, dy)
        } else {
            self.invalid_prim(s, "square: Größe muss positiv sein", ctx)
        };
        if let Some(x) = p[0] {
            self.point_handle(s, ctx, node, 0, x, hi.extend(0.0), 2, if center { 2.0 } else { 1.0 });
        }
    }

    #[inline(never)]
    fn m_rect(&mut self, s: &'a Stmt, a: &Args<'a>, ctx: &Ctx<'a>, dy: &Dyn<'a>) {
        let p = self.bind("rect", a, &["a", "b"], 2);
        let pa = p[0].and_then(|x| self.pt2(&x.val, x.expr.span));
        let pb = p[1].and_then(|x| self.pt2(&x.val, x.expr.span));
        let (Some(pa), Some(pb)) = (pa, pb) else {
            self.error(s.span, "rect() braucht zwei Eckpunkte `a` und `b`");
            return;
        };
        let node = self.profile(s, Profile::Rect { min: pa.min(pb), max: pa.max(pb) }, ctx, dy);
        for (i, (x, v)) in [(p[0], pa), (p[1], pb)].into_iter().enumerate() {
            if let Some(x) = x {
                self.point_handle(s, ctx, node, i as u32, x, v.extend(0.0), 2, 1.0);
            }
        }
    }

    #[inline(never)]
    fn m_circle(&mut self, s: &'a Stmt, a: &Args<'a>, ctx: &Ctx<'a>, dy: &Dyn<'a>) {
        let p = self.bind("circle", a, &["r", "d"], 1);
        let (r, src, scale) = match self.radius(p[0], p[1], "r", "d") {
            Some((r, x, sc)) => (r, Some(x), sc),
            None => (1.0, None, 1.0),
        };
        let prof = match self.facets(s, a, dy) {
            _ if !positive(&[r]) => None,
            Some(n) => Some(Profile::Polygon {
                loops: vec![
                    (0..n)
                        .map(|i| {
                            let ang = 360.0 * i as f64 / n as f64;
                            DVec2::new(r * cos_deg(ang), r * sin_deg(ang))
                        })
                        .collect(),
                ],
            }),
            None => Some(Profile::Circle { r }),
        };
        let node = match prof {
            Some(prof) => self.profile(s, prof, ctx, dy),
            None => self.invalid_prim(s, "circle: Radius muss positiv sein", ctx),
        };
        if let Some(x) = src {
            let w = self.nodes[node as usize].world;
            self.scalar_handle(
                s,
                ctx,
                node,
                HandleKind::Radius,
                0,
                &x.val,
                x.expr.span,
                r,
                scale,
                w,
                DVec3::ZERO,
                DVec3::X,
            );
        }
    }

    #[inline(never)]
    fn m_polygon(&mut self, s: &'a Stmt, what: &str, a: &Args<'a>, ctx: &Ctx<'a>, dy: &Dyn<'a>) {
        let names: &[&str] = if what == "spline" { &["points", "closed"] } else { &["points", "paths", "convexity"] };
        let p = self.bind(what, a, names, names.len());
        let Some(pa) = p[0] else {
            self.error(s.span, format!("{what}() braucht `points`"));
            return;
        };
        let Some(list) = pa.val.as_list() else {
            self.error(pa.expr.span, format!("{what}: `points` muss eine Liste von [x, y] sein"));
            return;
        };
        let mut pts = Vec::with_capacity(list.len());
        for e in list {
            match self.pt2(e, e.def.unwrap_or(pa.expr.span)) {
                Some(v) => pts.push(v),
                None => {
                    self.error(pa.expr.span, format!("{what}: `points` muss eine Liste von [x, y] sein"));
                    return;
                }
            }
        }
        let prof = if what == "spline" {
            let closed = p[1].is_none_or(|x| x.val.truthy());
            Profile::Spline { points: pts.clone(), closed }
        } else {
            let loops = match p[1] {
                None => vec![pts.clone()],
                Some(x) => {
                    let paths: Option<Vec<Vec<DVec2>>> = x.val.as_list().and_then(|l| {
                        l.iter()
                            .map(|path| {
                                path.as_list()?
                                    .iter()
                                    .map(|i| {
                                        let i = i.as_num()?;
                                        (i >= 0.0 && (i as usize) < pts.len()).then(|| pts[i as usize])
                                    })
                                    .collect()
                            })
                            .collect()
                    });
                    match paths {
                        Some(p) => p,
                        None => {
                            self.error(x.expr.span, "polygon: `paths` müssen Listen von Punktindizes sein");
                            return;
                        }
                    }
                }
            };
            Profile::Polygon { loops }
        };
        let node = self.profile(s, prof, ctx, dy);
        let w = self.nodes[node as usize].world;
        for (i, (e, pt)) in list.iter().zip(pts).enumerate() {
            let dims = e.as_list().map_or(2, |l| l.len().min(3));
            self.elem_handle(s, ctx, node, i as u32, pa, e, pt.extend(0.0), dims, w);
        }
    }

    // ------------------------------------------------------------ transforms

    #[inline(never)]
    fn m_transform(
        &mut self,
        s: &'a Stmt,
        n: &str,
        a: &Args<'a>,
        children: &'a [Stmt],
        ctx: &Ctx<'a>,
        dy: &Dyn<'a>,
    ) -> R<()> {
        let (kind, m) = self.transform_matrix(s, n, a);
        let node = self.node(s, Op::Transform { kind, m }, ctx);
        self.transform_handles(s, n, a, ctx, node);
        self.eval_children(node, &m, children, ctx, dy)
    }

    #[inline(never)]
    fn transform_matrix(&mut self, s: &'a Stmt, n: &str, a: &Args<'a>) -> (TransformKind, DMat4) {
        match n {
            "translate" => {
                let p = self.bind(n, a, &["v"], 1);
                let v = self.vec3_of(p[0], "v").unwrap_or(DVec3::ZERO);
                (TransformKind::Translate, DMat4::from_translation(v))
            }
            "rotate" => {
                let p = self.bind(n, a, &["a", "v", "to"], 2);
                if let Some(t) = p[2] {
                    let v = self.vec3_of(Some(t), "to").unwrap_or(DVec3::Z);
                    return (TransformKind::Rotate, math::rot_to(v));
                }
                let m = match p[0] {
                    None => DMat4::IDENTITY,
                    Some(x) => match (&x.val.v, x.val.as_num()) {
                        (Value::List(l), _) => math::rot_euler(euler_of(l)),
                        (_, Some(ang)) => match p[1].and_then(|v| v.val.as_vec3()) {
                            Some(axis) => math::rot_axis(ang, axis),
                            None => math::rot_z(ang),
                        },
                        _ => {
                            self.warn(x.expr.span, "rotate: Winkel muss Zahl oder [x, y, z] sein");
                            DMat4::IDENTITY
                        }
                    },
                };
                (TransformKind::Rotate, m)
            }
            "scale" => {
                let p = self.bind(n, a, &["v"], 1);
                let m = match p[0] {
                    None => DMat4::IDENTITY,
                    Some(x) => match (x.val.as_num(), scale_vec(&x.val)) {
                        (Some(k), _) => DMat4::from_scale(DVec3::splat(k)),
                        (_, Some(v)) => {
                            if !(v.x == v.y && v.y == v.z) {
                                self.warn(
                                    s.span,
                                    "ungleichmäßige Skalierung verzerrt Verrundungen, Zylinder und Wandstärken; besser Modulparameter",
                                );
                            }
                            DMat4::from_scale(v)
                        }
                        _ => {
                            self.warn(x.expr.span, "scale: muss Zahl oder [x, y, z] sein");
                            DMat4::IDENTITY
                        }
                    },
                };
                (TransformKind::Scale, m)
            }
            "mirror" => {
                let p = self.bind(n, a, &["v"], 1);
                let m = match p[0] {
                    None => math::mirror(DVec3::X),
                    Some(x) => match &x.val.v {
                        Value::PlaneRef { grid, plane, .. } => {
                            self.grid_plane(*grid, *plane).map_or(DMat4::IDENTITY, |(pt, nrm)| math::mirror_at(pt, nrm))
                        }
                        Value::GridSel { grid, sel } if sel.iter().filter(|s| **s == Sel::Wild).count() == 2 => {
                            let (pt, wild) = self.sel_point(*grid, sel);
                            let fixed = (0..3).find(|i| !wild.contains(i)).unwrap_or(0);
                            let mut ax = DVec3::ZERO;
                            ax[fixed] = 1.0;
                            let g = &self.grids[*grid as usize];
                            math::mirror_at(pt, math::transform_dir(&g.frame_module, ax))
                        }
                        Value::GridSel { .. } => {
                            self.error(x.expr.span, "mirror() braucht eine Ebene, keine Rasterlinie");
                            DMat4::IDENTITY
                        }
                        _ => match x.val.as_vec3() {
                            Some(v) => math::mirror(v),
                            None => {
                                self.warn(x.expr.span, "mirror() braucht einen Normalenvektor oder eine Rasterebene");
                                DMat4::IDENTITY
                            }
                        },
                    },
                };
                (TransformKind::Mirror, m)
            }
            _ => {
                let p = self.bind(n, a, &["m"], 1);
                let rows: Option<Vec<Vec<f64>>> =
                    p[0].and_then(|x| x.val.as_list()).and_then(|l| l.iter().map(|r| r.as_nums()).collect());
                match rows {
                    Some(r) if !r.is_empty() => (TransformKind::Matrix, math::from_rows(&r)),
                    _ => {
                        if let Some(x) = p[0] {
                            self.warn(x.expr.span, "multmatrix() braucht eine 4×4- oder 3×4-Matrix");
                        }
                        (TransformKind::Matrix, DMat4::IDENTITY)
                    }
                }
            }
        }
    }

    /// Handles of `translate` (target point), `rotate` (angle per axis) and `scale`.
    #[inline(never)]
    fn transform_handles(&mut self, s: &'a Stmt, n: &str, a: &Args<'a>, ctx: &Ctx<'a>, node: NodeIdx) {
        if !ctx.in_context() {
            return;
        }
        let w = ctx.xf.world;
        let first = |name: &str| a.get(0, name);
        match n {
            "translate" => {
                if let Some(x) = first("v")
                    && let Some(v) = x.val.as_vec3()
                {
                    let dims = x.val.as_list().map_or(3, |l| l.len().min(3));
                    self.point_handle(s, ctx, node, 0, x, v, dims, 1.0);
                }
            }
            "rotate" => {
                if a.get(2, "to").is_some() {
                    return;
                }
                let Some(x) = first("a") else { return };
                match (&x.val.v, x.val.as_num()) {
                    (Value::List(l), _) => {
                        let e = euler_of(l);
                        let frames = [w * math::rot_z(e.z) * math::rot_y(e.y), w * math::rot_z(e.z), w];
                        for (i, c) in l.iter().take(3).enumerate() {
                            let mut ax = DVec3::ZERO;
                            ax[i] = 1.0;
                            let span = c.def.unwrap_or(x.expr.span);
                            let kind = HandleKind::Angle { axis: i as u8 };
                            self.scalar_handle(
                                s,
                                ctx,
                                node,
                                kind,
                                i as u32,
                                c,
                                span,
                                e[i],
                                1.0,
                                frames[i],
                                DVec3::ZERO,
                                ax,
                            );
                        }
                    }
                    (_, Some(ang)) => {
                        let (kind, ax) = match a.get(1, "v").and_then(|v| v.val.as_vec3()) {
                            Some(axis) => (HandleKind::Angle { axis: 3 }, axis.normalize_or_zero()),
                            None => (HandleKind::Angle { axis: 2 }, DVec3::Z),
                        };
                        self.scalar_handle(s, ctx, node, kind, 0, &x.val, x.expr.span, ang, 1.0, w, DVec3::ZERO, ax);
                    }
                    _ => {}
                }
            }
            "scale" => {
                let Some(x) = first("v") else { return };
                if let Some(k) = x.val.as_num() {
                    let kind = HandleKind::Scale { axis: None };
                    self.scalar_handle(
                        s,
                        ctx,
                        node,
                        kind,
                        0,
                        &x.val,
                        x.expr.span,
                        k,
                        1.0,
                        w,
                        DVec3::ZERO,
                        DVec3::ONE.normalize(),
                    );
                } else if let (Some(v), Some(l)) = (scale_vec(&x.val), x.val.as_list()) {
                    for (i, c) in l.iter().take(3).enumerate() {
                        let mut ax = DVec3::ZERO;
                        ax[i] = 1.0;
                        let span = c.def.unwrap_or(x.expr.span);
                        let kind = HandleKind::Scale { axis: Some(i as u8) };
                        self.scalar_handle(s, ctx, node, kind, i as u32, c, span, v[i], 1.0, w, DVec3::ZERO, ax);
                    }
                }
            }
            _ => {}
        }
    }

    /// A grid plane as (point, normal) in its module's coordinates.
    fn grid_plane(&self, grid: u32, plane: u32) -> Option<(DVec3, DVec3)> {
        let g = self.grids.get(grid as usize)?;
        let p = g.planes.get(plane as usize)?;
        let mut axis = DVec3::ZERO;
        axis[p.axis as usize] = 1.0;
        Some((g.frame_module.transform_point3(axis * p.value), math::transform_dir(&g.frame_module, axis)))
    }

    /// The colour of a `color()`/`filament()` argument (with the warnings of Plan 16.1 and
    /// Bericht 12, F7): RGBA (vectors clamped to 0..1) and the palette or unknown name.
    /// `None`: no colour (`""`, `transparent`, malformed).
    fn color_arg(&mut self, x: &ArgV<'a>) -> (Option<[f32; 4]>, Option<String>) {
        let mut name = None;
        let rgba = match (&x.val.v, x.val.as_nums()) {
            (Value::Str(text), _) if text.trim().is_empty() => {
                self.warn(x.expr.span, "leerer Farbname: color() hat keine Wirkung");
                None
            }
            (Value::Str(text), _) if text.trim().eq_ignore_ascii_case("transparent") => {
                // OpenSCAD hides it in the preview; a print has no transparent filament
                self.warn(
                    x.expr.span,
                    "`transparent` ist keine Druckfarbe: das Material bleibt ungefärbt (Standard, Filament 1)",
                );
                None
            }
            (Value::Str(text), _) => match parse_color(text) {
                Some(c) => {
                    if c.kind == ColorKind::Random {
                        let known: Vec<&str> = palette::PALETTE.iter().map(|p| p.name).collect();
                        self.warn(
                            x.expr.span,
                            format!("Farbe `{text}` unbekannt – Zufallsfarbe (Palette: {})", known.join(", ")),
                        );
                    }
                    name = c.name;
                    if c.kind == ColorKind::Random { Some(self.random_color(text)) } else { Some(c.rgba) }
                }
                None => {
                    self.warn(x.expr.span, format!("`{text}` ist keine Farbe (#rgb, #rrggbb, #rrggbbaa oder Name)"));
                    None
                }
            },
            (_, Some(v)) if v.len() == 3 || v.len() == 4 => {
                if v.iter().any(|c| !(0.0..=1.0).contains(c)) {
                    let hint = if v.iter().take(3).any(|&c| c > 1.0) && v.iter().take(3).all(|&c| c <= 255.0) {
                        let b: Vec<u8> = v.iter().take(3).map(|&c| c.clamp(0.0, 255.0).round() as u8).collect();
                        format!(" – für Werte 0–255 \"#{:02x}{:02x}{:02x}\" schreiben", b[0], b[1], b[2])
                    } else {
                        String::new()
                    };
                    self.warn(
                        x.expr.span,
                        format!("Farbwerte liegen zwischen 0 und 1 (wie in OpenSCAD), andere werden geklemmt{hint}"),
                    );
                }
                let c = |i: usize| v.get(i).copied().unwrap_or(1.0).clamp(0.0, 1.0) as f32;
                Some([c(0), c(1), c(2), c(3)])
            }
            _ => {
                self.warn(x.expr.span, "Farbe muss Name oder [r, g, b, a] sein");
                None
            }
        };
        (rgba, name)
    }

    /// `filament(c, n);` (Bericht 12, F2): colour `c` prints with slot `n`, for the whole model.
    /// Only at the top level of the main file (evaluated first, see `filament_pass`); the
    /// first declaration of a colour counts, `filament = m` at its `color()` calls must agree.
    #[inline(never)]
    fn m_filament(&mut self, s: &'a Stmt, a: &Args<'a>) {
        if !self.decl_pass {
            if !self.filament_decls.iter().any(|d| d.span == s.span) {
                self.warn(s.span, "filament(…) gilt nur auf oberster Ebene der Hauptdatei – hier ohne Wirkung");
            }
            return;
        }
        let p = self.bind("filament", a, &["c", "n"], 2);
        let Some(c) = p[0] else {
            self.warn(s.span, "filament(Farbe, Slot): Farbe fehlt");
            return;
        };
        let (rgba, name) = self.color_arg(c);
        let Some(rgba) = rgba else { return };
        let Some(n) = self.num_of(p[1], "n") else {
            self.warn(s.span, "filament(Farbe, Slot): Slot fehlt");
            return;
        };
        if n.fract() != 0.0 || !(1.0..=palette::MAX_FILAMENT as f64).contains(&n) {
            let sp = p[1].map_or(s.span, |x| x.expr.span);
            self.warn(sp, format!("Slot muss eine ganze Zahl von 1 bis {} sein", palette::MAX_FILAMENT));
            return;
        }
        let key = palette::rgb_key(rgba);
        let n = n as u32;
        if let Some(&(m, _)) = self.filaments.get(&key) {
            self.warn(s.span, format!("Farbe hat schon Filament {m} (die erste Angabe gilt), Slot {n} wird ignoriert"));
            return;
        }
        let sp = p[1].map_or(s.span, |x| x.expr.span);
        self.filaments.insert(key, (n, sp));
        self.filament_decls.push(cad_ir::FilamentDecl { rgba, name, slot: n, stmt: s.id, span: s.span });
    }

    /// Colour of an unknown name in this model: the same for every spelling of the name, apart
    /// from the random colours of the names before it (`palette::random_color_avoiding`).
    fn random_color(&mut self, text: &str) -> [f32; 4] {
        let key = text.trim().to_ascii_lowercase();
        if let Some((_, c)) = self.random_colors.iter().find(|(n, _)| *n == key) {
            return palette::rgba_of(*c);
        }
        let others: Vec<[u8; 3]> = self.random_colors.iter().map(|(_, c)| *c).collect();
        let c = palette::random_color_avoiding(&key, &others);
        let b = palette::rgba_bytes(c);
        self.random_colors.push((key, [b[0], b[1], b[2]]));
        c
    }

    #[inline(never)]
    fn m_color(&mut self, s: &'a Stmt, a: &Args<'a>, children: &'a [Stmt], ctx: &Ctx<'a>, dy: &Dyn<'a>) -> R<()> {
        let p = self.bind("color", a, &["c", "alpha", "filament"], 2);
        let (mut rgba, name) = match p[0] {
            None => {
                self.warn(s.span, "color() ohne Farbe hat keine Wirkung");
                (None, None)
            }
            Some(x) => self.color_arg(x),
        };
        if let Some(al) = self.num_of(p[1], "alpha") {
            if !(0.0..=1.0).contains(&al) {
                self.warn(p[1].map_or(s.span, |x| x.expr.span), "alpha liegt zwischen 0 und 1, der Wert wird geklemmt");
            }
            if let Some(c) = rgba.as_mut() {
                c[3] = al.clamp(0.0, 1.0) as f32;
            }
        }
        let mut filament = None;
        if let Some(arg) = p[2]
            && let Some(f) = self.num_of(Some(arg), "filament")
        {
            if f.fract() != 0.0 || !(1.0..=palette::MAX_FILAMENT as f64).contains(&f) {
                self.warn(
                    arg.expr.span,
                    format!("filament muss eine ganze Zahl von 1 bis {} sein", palette::MAX_FILAMENT),
                );
            } else if let Some(c) = rgba {
                // the identity of a print colour is its RGB value (alpha only affects the view)
                let key = palette::rgb_key(c);
                let n = f as u32;
                match self.filaments.get(&key) {
                    Some(&(m, _)) if m != n => {
                        self.warn(
                            arg.expr.span,
                            format!(
                                "Farbe hat schon Filament {m} (die erste Angabe gilt), filament = {n} wird ignoriert"
                            ),
                        );
                    }
                    Some(_) => filament = Some(n),
                    None => {
                        self.filaments.insert(key, (n, arg.expr.span));
                        filament = Some(n);
                    }
                }
            }
        }
        let node = match rgba {
            Some(c) => self.node(s, Op::Color { rgba: c, name, filament }, ctx),
            None => self.node(s, Op::Group, ctx),
        };
        let mut c = self.sub(ctx, node, &DMat4::IDENTITY);
        c.dyn_ = dy.clone();
        if rgba.is_some() {
            c.color = rgba;
        }
        self.scope_list(children, &c)
    }

    #[inline(never)]
    fn m_blend(
        &mut self,
        s: &'a Stmt,
        n: &str,
        a: &Args<'a>,
        children: &'a [Stmt],
        ctx: &Ctx<'a>,
        dy: &Dyn<'a>,
    ) -> R<()> {
        let names: &[&str] = match n {
            "fillet" => &["r", "select"],
            "chamfer" => &["d", "select", "r"],
            _ => &["t", "select"],
        };
        let p = self.bind(n, a, names, 2);
        let main = if n == "chamfer" { p[0].or(p[2]) } else { p[0] };
        let what = if n == "shell" { "t" } else { "r" };
        let mut law = None;
        let r = match main {
            Some(x) if n == "fillet" && matches!(x.val.v, Value::Func(_)) => {
                let (r, l) = self.fillet_param(Some(x), what, dy)?;
                law = l;
                if law.is_some() { Some(0.0) } else { r }
            }
            Some(x) => self.blend_param(Some(x), what),
            None => {
                self.error(
                    s.span,
                    format!("{n}() braucht {}", if n == "shell" { "eine Wandstärke `t`" } else { "einen Radius" }),
                );
                None
            }
        };
        let select = match p[1] {
            None => Select::Inside,
            Some(x) => match x.val.as_str() {
                Some("inside") => Select::Inside,
                Some("touch") => Select::Touch,
                _ => {
                    self.warn(x.expr.span, "`select` muss \"inside\" oder \"touch\" sein");
                    Select::Inside
                }
            },
        };
        let r = r.unwrap_or(0.0);
        let op = match n {
            "fillet" => Op::Blend { kind: BlendKind::Fillet, r, select, law },
            "chamfer" => Op::Blend { kind: BlendKind::Chamfer, r, select, law: None },
            _ => Op::Shell { t: r, select },
        };
        let node = self.node(s, op, ctx);
        self.eval_children(node, &DMat4::IDENTITY, children, ctx, dy)?;
        let kids = self.nodes[node as usize].children.clone();
        if kids.is_empty() {
            self.warn(s.span, format!("{n}() ohne Körper"));
        }
        if let Some(&first) = kids.first()
            && matches!(self.nodes[first as usize].op, Op::Zone(_))
        {
            let sp = self.nodes[first as usize].span;
            self.warn(sp, "das erste Kind ist der Körper, Zonen folgen danach");
        }
        for &k in kids.iter().skip(1) {
            self.nodes[k as usize].flags.zone = true;
        }
        Ok(())
    }

    #[inline(never)]
    fn m_zone(&mut self, s: &'a Stmt, a: &Args<'a>, ctx: &Ctx<'a>) {
        let p = self.bind("zone", a, &["target", "d"], 2);
        let Some(t) = p[0] else {
            self.error(s.span, "zone() braucht eine Rasterlinie oder -ebene");
            return;
        };
        let d = match self.num_of(p[1], "d") {
            Some(d) => d,
            None => {
                self.error(s.span, "zone() braucht Durchmesser/Dicke `d`");
                return;
            }
        };
        let shape = match &t.val.v {
            Value::PlaneRef { grid, plane, .. } => {
                self.grid_plane(*grid, *plane).map(|(point, normal)| ZoneShape::Plane { point, normal, d })
            }
            Value::GridSel { grid, sel } => {
                let (point, wild) = self.sel_point(*grid, sel);
                let g = &self.grids[*grid as usize];
                if wild.len() == 1 {
                    let mut ax = DVec3::ZERO;
                    ax[wild[0]] = 1.0;
                    Some(ZoneShape::Line { point, dir: math::transform_dir(&g.frame_module, ax), d })
                } else {
                    let fixed = (0..3).find(|i| !wild.contains(i)).unwrap_or(0);
                    let mut ax = DVec3::ZERO;
                    ax[fixed] = 1.0;
                    Some(ZoneShape::Plane { point, normal: math::transform_dir(&g.frame_module, ax), d })
                }
            }
            _ => None,
        };
        let Some(shape) = shape else {
            let ty = t.val.type_de();
            self.error(t.expr.span, format!("zone() braucht Rasterlinie `g[a, *, c]` oder Ebene `g.name`, ist {ty}"));
            return;
        };
        let mut ok = false;
        let mut cur = ctx.parent;
        while let Some(pi) = cur {
            match self.nodes[pi as usize].op {
                Op::Blend { .. } | Op::Shell { .. } => {
                    ok = true;
                    break;
                }
                Op::ModuleCall { .. } => break,
                _ => cur = self.nodes[pi as usize].parent,
            }
        }
        if !ok {
            self.warn(s.span, "zone() außerhalb von fillet/chamfer/shell ohne Wirkung");
        }
        self.node(s, Op::Zone(shape), ctx);
    }

    fn path_points(&mut self, x: &ArgV<'a>, what: &str) -> Option<Vec<DVec3>> {
        let l = x.val.as_list();
        match l.and_then(|l| l.iter().map(|p| p.as_vec3()).collect::<Option<Vec<_>>>()) {
            Some(p) if p.len() >= 2 => Some(p),
            _ => {
                self.error(x.expr.span, format!("{what}: braucht eine Liste von mindestens zwei Punkten"));
                None
            }
        }
    }

    #[inline(never)]
    fn m_profile_op(
        &mut self,
        s: &'a Stmt,
        n: &str,
        a: &Args<'a>,
        children: &'a [Stmt],
        ctx: &Ctx<'a>,
        dy: &Dyn<'a>,
    ) -> R<()> {
        let mut handle: Option<(HandleKind, &ArgV<'a>, f64, f64)> = None;
        let mut path_arg = None;
        // OpenSCAD produces nothing (zero height or angle): empty node, children unused.
        let mut empty = false;
        let op = match n {
            "linear_extrude" => {
                let p = self.bind(n, a, &["height", "center", "convexity", "twist", "slices", "scale", "h"], 6);
                let src = p[0].or(p[6]);
                let h = self.num_of(src, "height").unwrap_or(100.0);
                let center = is_true(p[1]);
                let twist = self.num_of(p[3], "twist").unwrap_or(0.0);
                let mut scale = match p[5] {
                    None => DVec2::ONE,
                    Some(x) => match (x.val.as_num(), x.val.as_vec2()) {
                        (Some(k), _) => DVec2::splat(k),
                        (_, Some(v)) => v,
                        _ => {
                            self.warn(x.expr.span, "scale: muss Zahl oder [x, y] sein");
                            DVec2::ONE
                        }
                    },
                };
                // OpenSCAD clamps negative scale factors to 0 (the top shrinks to a point).
                if scale.min_element() < 0.0 {
                    self.warn(s.span, "linear_extrude: negativer `scale` wird zu 0");
                    scale = scale.max(DVec2::ZERO);
                }
                if let Some(x) = src {
                    handle = Some(if center {
                        (HandleKind::Height, x, h / 2.0, 2.0)
                    } else {
                        (HandleKind::Height, x, h, 1.0)
                    });
                }
                if h.is_nan() || h <= 0.0 {
                    self.warn(s.span, format!("linear_extrude: Höhe {} ≤ 0, kein Körper", fmt_num(h)));
                    empty = true;
                }
                Op::LinearExtrude { h, twist, scale, center }
            }
            "rotate_extrude" => {
                let p = self.bind(n, a, &["angle", "convexity"], 1);
                let given = self.num_of(p[0], "angle");
                let mut angle = given.unwrap_or(360.0);
                // OpenSCAD: angles outside (-360, 360] become a full turn; negative
                // angles turn clockwise.
                if !(angle > -360.0 && angle <= 360.0) {
                    self.warn(s.span, format!("rotate_extrude: Winkel {}° wird zu 360°", fmt_num(angle)));
                    angle = 360.0;
                }
                if angle == 0.0 {
                    self.warn(s.span, "rotate_extrude: Winkel 0°, kein Körper");
                    empty = true;
                }
                if let Some(x) = p[0] {
                    handle = Some((HandleKind::Angle { axis: 2 }, x, angle, 1.0));
                }
                if let Some(f) = a.dyn_num("$fn").and_then(facet_count) {
                    self.info(
                        s.span,
                        format!("rotate_extrude: Umlauf bleibt exakt rund; `$fn` = {f} gilt nur für Kreise im Profil"),
                    );
                }
                Op::RotateExtrude { angle }
            }
            "loft" => {
                let p = self.bind(n, a, &["ruled"], 1);
                Op::Loft { ruled: p[0].is_some_and(|x| x.val.truthy()) }
            }
            "sweep" => {
                let p = self.bind(n, a, &["path", "interp"], 2);
                let Some(pa) = p[0] else {
                    self.error(s.span, "sweep() braucht `path`");
                    return Ok(());
                };
                let Some(path) = self.path_points(pa, "sweep-Pfad") else { return Ok(()) };
                let interp = match p[1].map(|x| (x, x.val.as_str())) {
                    None | Some((_, Some("spline"))) => PathInterp::Spline,
                    Some((_, Some("linear"))) => PathInterp::Linear,
                    Some((x, _)) => {
                        self.warn(x.expr.span, "`interp` muss \"linear\" oder \"spline\" sein");
                        PathInterp::Spline
                    }
                };
                path_arg = Some((pa, path.clone()));
                Op::Sweep { path, interp }
            }
            "thicken" => {
                let p = self.bind(n, a, &["t"], 1);
                let Some(t) = self.num_of(p[0], "t") else {
                    self.error(s.span, "thicken() braucht eine Dicke `t`");
                    return Ok(());
                };
                Op::Thicken { t }
            }
            _ => {
                let p = self.bind(n, a, &["newsize", "auto", "convexity"], 2);
                let size = self.vec3_of(p[0], "newsize").unwrap_or(DVec3::ZERO);
                let auto = match p[1].map(|x| &x.val) {
                    None => [false; 3],
                    Some(v) => match v.as_list() {
                        Some(l) => [0, 1, 2].map(|i| l.get(i).is_some_and(|b| b.truthy())),
                        None => [v.truthy(); 3],
                    },
                };
                Op::Resize { size, auto }
            }
        };
        let node = self.node(s, if empty { Op::Empty } else { op }, ctx);
        let w = self.nodes[node as usize].world;
        if let Some((kind, x, v, sc)) = handle {
            self.scalar_handle(s, ctx, node, kind, 0, &x.val, x.expr.span, v, sc, w, DVec3::ZERO, DVec3::Z);
        }
        if empty {
            return Ok(());
        }
        if let Some((pa, path)) = path_arg
            && let Some(l) = pa.val.as_list()
        {
            for (i, (e, pt)) in l.iter().zip(path).enumerate() {
                let dims = e.as_list().map_or(3, |l| l.len().min(3));
                self.elem_handle(s, ctx, node, i as u32, pa, e, pt, dims, w);
            }
        }
        self.eval_children(node, &DMat4::IDENTITY, children, ctx, dy)
    }

    #[inline(never)]
    fn m_surface(&mut self, s: &'a Stmt, a: &Args<'a>, ctx: &Ctx<'a>, dy: &Dyn<'a>) {
        let p = self.bind("surface", a, &["points", "file", "center", "invert", "convexity"], 1);
        if p[1].is_some() {
            self.warn(s.span, "surface(file = …) (Höhenkarten) nicht unterstützt; surface(points) verwenden");
            return;
        }
        let Some(pa) = p[0] else {
            self.error(s.span, "surface() braucht `points` (Zeilen von Kontrollpunkten)");
            return;
        };
        let rows: Option<Vec<Vec<DVec3>>> = pa
            .val
            .as_list()
            .and_then(|l| l.iter().map(|r| r.as_list()?.iter().map(|p| p.as_vec3()).collect()).collect());
        let Some(rows) = rows else {
            self.error(pa.expr.span, "surface: `points` muss eine Liste von Zeilen aus [x, y, z] sein");
            return;
        };
        if rows.len() < 2 || rows.iter().any(|r| r.len() != rows[0].len() || r.len() < 2) {
            self.error(
                pa.expr.span,
                "surface: `points` braucht mindestens 2 gleich lange Zeilen mit mindestens 2 Punkten",
            );
            return;
        }
        let cols = rows[0].len();
        let node = self.node(s, Op::Surface { rows: rows.clone() }, ctx);
        self.set_blend(node, dy);
        let w = self.nodes[node as usize].world;
        if let Some(l) = pa.val.as_list() {
            for (ri, r) in l.iter().enumerate() {
                let Some(rl) = r.as_list() else { continue };
                for (ci, e) in rl.iter().enumerate() {
                    let dims = e.as_list().map_or(3, |l| l.len().min(3));
                    self.elem_handle(s, ctx, node, (ri * cols + ci) as u32, pa, e, rows[ri][ci], dims, w);
                }
            }
        }
    }

    pub(crate) fn assert_args(&mut self, span: Span, a: &Args<'a>) -> R<()> {
        let Some(cond) = a.get(0, "condition") else {
            return self.fatal(span, "assert() braucht eine Bedingung");
        };
        if cond.val.truthy() {
            return Ok(());
        }
        let text = self.text(cond.expr.span).to_string();
        let msg = match a.get(1, "message") {
            Some(m) => format!("assert '{text}' fehlgeschlagen: {}", crate::value::to_str(&m.val)),
            None => format!("assert '{text}' fehlgeschlagen"),
        };
        self.fatal(span, msg)
    }
}

/// Marks the `$` frame of a `linear_extrude` / `rotate_extrude` call with `$fn`: its
/// `$fn` counts as "directly at the call" for the circles in the profile. Not a valid
/// identifier, so source code cannot set it.
const EXTRUDE_FN: &str = "$ extrude $fn";

/// `$fn` of the innermost frame that sets it, if that frame is an extrusion call.
fn extrude_fn(dy: &Dyn) -> Option<f64> {
    let f = dyn_frame(dy, "$fn")?;
    if !f.vars.iter().any(|(n, _)| *n == EXTRUDE_FN) {
        return None;
    }
    f.vars.iter().rev().find(|(n, _)| *n == "$fn").and_then(|(_, v)| v.as_num())
}

/// `center = true`: OpenSCAD only accepts the bool (`center = 1` does not center).
fn is_true(a: Option<&ArgV>) -> bool {
    a.is_some_and(|x| matches!(x.val.v, Value::Bool(true)))
}

/// All dimensions finite and > 0 (otherwise OpenSCAD creates nothing).
fn positive(dims: &[f64]) -> bool {
    dims.iter().all(|d| d.is_finite() && *d > 0.0)
}

/// Facets for a `$fn` that makes a polygon (0 < `$fn` < 13; OpenSCAD uses at least 3).
fn facet_count(n: f64) -> Option<u32> {
    (n > 0.0 && n < 13.0).then(|| (n.floor() as u32).max(3))
}

/// OpenSCAD sphere with `n` fragments: `(n + 1) / 2` rings.
fn sphere_poly(r: f64, n: u32) -> (Vec<DVec3>, Vec<Vec<u32>>) {
    let f = n.max(3) as usize;
    let rings = f.div_ceil(2).max(1);
    let mut pts = Vec::with_capacity(f * rings);
    for i in 0..rings {
        let phi = 180.0 * (i as f64 + 0.5) / rings as f64;
        let (rr, z) = (r * sin_deg(phi), r * cos_deg(phi));
        for j in 0..f {
            let a = 360.0 * j as f64 / f as f64;
            pts.push(DVec3::new(rr * cos_deg(a), rr * sin_deg(a), z));
        }
    }
    let mut faces = vec![(0..f as u32).rev().collect::<Vec<_>>()];
    for i in 0..rings - 1 {
        for j in 0..f {
            let (a, b) = ((i * f + j) as u32, (i * f + (j + 1) % f) as u32);
            let (c, d) = (((i + 1) * f + (j + 1) % f) as u32, ((i + 1) * f + j) as u32);
            faces.push(vec![a, b, c, d]);
        }
    }
    faces.push(((rings - 1) * f..rings * f).map(|i| i as u32).collect());
    (pts, faces)
}

/// Prism / pyramid with `n` sides (OpenSCAD cylinder with small `$fn`).
fn prism(n: u32, h: f64, r1: f64, r2: f64, z0: f64) -> (Vec<DVec3>, Vec<Vec<u32>>) {
    let n = n as usize;
    let ring = |r: f64, z: f64| -> Vec<DVec3> {
        if r <= 0.0 {
            vec![DVec3::new(0.0, 0.0, z)]
        } else {
            (0..n)
                .map(|i| {
                    let a = 360.0 * i as f64 / n as f64;
                    DVec3::new(r * cos_deg(a), r * sin_deg(a), z)
                })
                .collect()
        }
    };
    let bottom = ring(r1, z0);
    let top = ring(r2, z0 + h);
    let (nb, nt) = (bottom.len(), top.len());
    let mut pts = bottom;
    pts.extend(top);
    let mut faces = Vec::new();
    if nb > 1 {
        faces.push((0..nb as u32).rev().collect());
    }
    if nt > 1 {
        faces.push((nb as u32..(nb + nt) as u32).collect());
    }
    for i in 0..n {
        let j = (i + 1) % n;
        let b = |k: usize| if nb == 1 { 0 } else { k as u32 };
        let t = |k: usize| nb as u32 + if nt == 1 { 0 } else { k as u32 };
        let mut f = vec![b(i)];
        if nb > 1 {
            f.push(b(j));
        }
        f.push(t(j));
        if nt > 1 {
            f.push(t(i));
        }
        faces.push(f);
    }
    (pts, faces)
}

/// Euler angles of a `rotate([x, y, z])` list (missing components 0).
fn euler_of(l: &[Val]) -> DVec3 {
    let mut e = DVec3::ZERO;
    for (i, c) in l.iter().take(3).enumerate() {
        e[i] = c.as_num().unwrap_or(0.0);
    }
    e
}

/// Scale vector of a `scale([x, y(, z)])` list (z = 1 if missing).
fn scale_vec(v: &Val) -> Option<DVec3> {
    let l = v.as_list()?;
    if l.len() < 2 {
        return None;
    }
    let mut r = DVec3::ONE;
    for (i, c) in l.iter().take(3).enumerate() {
        r[i] = c.as_num()?;
    }
    Some(r)
}
