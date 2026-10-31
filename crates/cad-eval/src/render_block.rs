//! `render { … }`: the studio look of the file (lights, materials, environment, shadows …).
//!
//! Only the first block at the top level of the main file applies; it is evaluated before the
//! other statements (like `filament()`), so its values may use the file's variables. Unknown
//! names and bad values are warnings (the value is ignored, the default applies): the block
//! never changes the geometry.

use cad_i18n::{tr, trs};
use cad_ir::{RenderDecl, RenderLight, RenderMaterial, RenderTarget};
use cad_syntax::Span;
use cad_syntax::ast::{RenderItem, RenderSetting, RenderTarget as AstTarget, Stmt, StmtKind};

use crate::eval::{Ctx, Ev, R};
use crate::palette::{self, ColorKind};
use crate::value::{Val, Value};

/// Settings at the top level of a render block.
pub const RENDER_SETTINGS: [&str; 16] = [
    "style",
    "edges",
    "environment",
    "ambient",
    "light_intensity",
    "shadows",
    "shadow_softness",
    "ao",
    "ao_radius",
    "ao_strength",
    "ground",
    "ground_color",
    "ground_reflection",
    "ground_roughness",
    "reflections",
    "background",
];
/// Settings of `light NAME { … }`.
pub const LIGHT_SETTINGS: [&str; 4] = ["azimuth", "elevation", "intensity", "color"];
/// Settings of `material SEL { … }`.
pub const MATERIAL_SETTINGS: [&str; 5] = ["preset", "metalness", "roughness", "clearcoat", "anisotropy"];
/// Material presets (the same as `cad_render::Material::PRESETS`).
pub const MATERIAL_PRESETS: [&str; 14] = [
    "plastic",
    "plastic_satin",
    "plastic_glossy",
    "rubber",
    "ceramic",
    "aluminium",
    "metal_brushed",
    "anodized",
    "chrome",
    "stainless",
    "titanium",
    "gold",
    "copper",
    "brass",
];
/// Values of `environment`.
pub const ENVIRONMENTS: [&str; 4] = ["studio", "soft", "overcast", "none"];
/// Values of the key light (defaults of a `light` entry).
pub const KEY_LIGHT: (f64, f64, f64) = (-40.0, 50.0, 1.0);

impl<'a> Ev<'a> {
    /// Evaluates the first `render { … }` at the top level of the main file (before the other
    /// statements: its values may use the file's variables, the CLI reads it in module context
    /// too).
    pub(crate) fn render_pass(&mut self, ctx: &Ctx<'a>) -> R<()> {
        let file = self.file;
        let Some(s) = file.stmts.iter().find(|s| matches!(s.kind, StmtKind::Render(_)) && s.span.file() == 0) else {
            return Ok(());
        };
        let StmtKind::Render(r) = &s.kind else { unreachable!() };
        self.render_span = Some(s.span);
        let decl = self.render_decl(s, &r.items, ctx)?;
        self.render = Some(decl);
        Ok(())
    }

    /// `render { … }` met by the normal pass: already evaluated, or without effect here.
    pub(crate) fn render_stmt(&mut self, s: &'a Stmt) {
        if self.render_span == Some(s.span) {
            return;
        }
        let msg =
            if self.render_span.is_some() && s.span.file() == 0 && self.file.stmts.iter().any(|t| t.span == s.span) {
                trs!(
                    "nur der erste render-Block gilt – dieser hat keine Wirkung",
                    "only the first render block applies – this one has no effect"
                )
            } else {
                trs!(
                    "render { … } gilt nur auf oberster Ebene der Hauptdatei – hier ohne Wirkung",
                    "render { … } applies only at the top level of the main file – no effect here"
                )
            };
        self.warn(s.span, msg);
    }

    fn render_decl(&mut self, s: &'a Stmt, items: &'a [RenderItem], ctx: &Ctx<'a>) -> R<RenderDecl> {
        let mut d = RenderDecl { stmt: s.id, span: s.span, ..RenderDecl::default() };
        let settings: Vec<&'a RenderSetting> = items
            .iter()
            .filter_map(|i| match i {
                RenderItem::Setting(x) => Some(x),
                _ => None,
            })
            .collect();
        self.render_duplicates(&settings);
        for st in settings {
            let name = st.name.name.as_str();
            if !RENDER_SETTINGS.contains(&name) {
                self.unknown_setting(st, "render", &RENDER_SETTINGS);
                continue;
            }
            let v = self.expr(&st.value, &ctx.env, &ctx.dyn_)?;
            let sp = st.value.span;
            match name {
                "style" => d.style = self.render_word(&v, sp, name, &["studio", "standard"]),
                "environment" => d.environment = self.render_word(&v, sp, name, &ENVIRONMENTS),
                "edges" => d.edges = self.render_bool(&v, sp, name),
                "shadows" => d.shadows = self.render_bool(&v, sp, name),
                "ao" => d.ao = self.render_bool(&v, sp, name),
                "ground" => d.ground = self.render_bool(&v, sp, name),
                "reflections" => d.reflections = self.render_bool(&v, sp, name),
                "ground_reflection" => d.ground_reflection = self.render_num(&v, sp, name, 0.0, 1.0),
                "ground_roughness" => d.ground_roughness = self.render_num(&v, sp, name, 0.0, 1.0),
                "ambient" => d.ambient = self.render_num(&v, sp, name, 0.0, 1.0),
                "light_intensity" => d.light_intensity = self.render_num(&v, sp, name, 0.0, 4.0),
                "shadow_softness" => d.shadow_softness = self.render_num(&v, sp, name, 0.0, 1.0),
                "ao_strength" => d.ao_strength = self.render_num(&v, sp, name, 0.0, 2.0),
                "ao_radius" => {
                    d.ao_radius = self.render_num(&v, sp, name, 0.0, f64::INFINITY).filter(|r| {
                        let ok = *r > 0.0;
                        if !ok {
                            self.warn(sp, tr!("render: {name} muss positiv sein", "render: {name} must be positive"));
                        }
                        ok
                    })
                }
                "ground_color" => d.ground_color = self.render_color(&v, sp, name),
                "background" => d.background = self.render_background(&v, sp),
                _ => unreachable!(),
            }
        }
        for item in items {
            match item {
                RenderItem::Setting(_) => {}
                RenderItem::Light { name, settings, span, .. } => {
                    if let Some(i) = d.lights.iter().position(|l| l.name == name.name) {
                        self.warn(
                            name.span,
                            tr!(
                                "render: Licht `{}` ist doppelt (das letzte gilt)",
                                "render: light `{}` is defined twice (the last one applies)",
                                name.name
                            ),
                        );
                        d.lights.remove(i);
                    }
                    let l = self.render_light(&name.name, settings, *span, ctx)?;
                    d.lights.push(l);
                }
                RenderItem::Material { target, settings, span, .. } => {
                    if let Some(m) = self.render_material(target, settings, *span, ctx)? {
                        d.materials.push(m);
                    }
                }
            }
        }
        Ok(d)
    }

    fn render_light(&mut self, name: &str, settings: &'a [RenderSetting], span: Span, ctx: &Ctx<'a>) -> R<RenderLight> {
        let mut l = RenderLight {
            name: name.to_string(),
            azimuth: KEY_LIGHT.0,
            elevation: KEY_LIGHT.1,
            intensity: KEY_LIGHT.2,
            color: [1.0; 4],
            span,
        };
        let list: Vec<&RenderSetting> = settings.iter().collect();
        self.render_duplicates(&list);
        let what = format!("light {name}");
        for st in settings {
            let key = st.name.name.as_str();
            if !LIGHT_SETTINGS.contains(&key) {
                self.unknown_setting(st, &what, &LIGHT_SETTINGS);
                continue;
            }
            let v = self.expr(&st.value, &ctx.env, &ctx.dyn_)?;
            let sp = st.value.span;
            match key {
                "azimuth" => {
                    if let Some(x) = self.render_num(&v, sp, key, -360.0, 360.0) {
                        l.azimuth = x;
                    }
                }
                "elevation" => {
                    if let Some(x) = self.render_num(&v, sp, key, 1.0, 90.0) {
                        l.elevation = x;
                    }
                }
                "intensity" => {
                    if let Some(x) = self.render_num(&v, sp, key, 0.0, 4.0) {
                        l.intensity = x;
                    }
                }
                "color" => {
                    if let Some(c) = self.render_color(&v, sp, key) {
                        l.color = c;
                    }
                }
                _ => unreachable!(),
            }
        }
        Ok(l)
    }

    fn render_material(
        &mut self,
        target: &'a AstTarget,
        settings: &'a [RenderSetting],
        span: Span,
        ctx: &Ctx<'a>,
    ) -> R<Option<RenderMaterial>> {
        let (target, what) = match target {
            AstTarget::Part(id) => (RenderTarget::Part(id.name.clone()), format!("material {}", id.name)),
            AstTarget::Color(text, sp) => {
                let what = format!("material \"{text}\"");
                if text.trim().eq_ignore_ascii_case("default") {
                    (RenderTarget::Color { label: "default".into(), rgba: None }, what)
                } else {
                    match palette::parse_color(text) {
                        Some(c) if c.kind != ColorKind::Random && c.rgba[3] > 0.0 => {
                            (RenderTarget::Color { label: text.trim().to_string(), rgba: Some(c.rgba) }, what)
                        }
                        _ => {
                            self.warn(
                                *sp,
                                tr!(
                                    "render: {what}: Farbe unbekannt (Palettenname, #rrggbb oder \"default\") – ignoriert",
                                    "render: {what}: unknown colour (palette name, #rrggbb or \"default\") – ignored"
                                ),
                            );
                            return Ok(None);
                        }
                    }
                }
            }
        };
        let mut m = RenderMaterial {
            target,
            preset: None,
            metalness: None,
            roughness: None,
            clearcoat: None,
            anisotropy: None,
            span,
        };
        let list: Vec<&RenderSetting> = settings.iter().collect();
        self.render_duplicates(&list);
        for st in settings {
            let key = st.name.name.as_str();
            if !MATERIAL_SETTINGS.contains(&key) {
                self.unknown_setting(st, &what, &MATERIAL_SETTINGS);
                continue;
            }
            let v = self.expr(&st.value, &ctx.env, &ctx.dyn_)?;
            let sp = st.value.span;
            match key {
                "preset" => {
                    m.preset = self.render_word(&v, sp, key, &MATERIAL_PRESETS).or_else(|| {
                        // `aluminum` as well
                        (v.as_str().map(|s| s.trim().to_ascii_lowercase()).as_deref() == Some("aluminum"))
                            .then(|| "aluminium".to_string())
                    })
                }
                "metalness" => m.metalness = self.render_num(&v, sp, key, 0.0, 1.0),
                "roughness" => m.roughness = self.render_num(&v, sp, key, 0.0, 1.0),
                "clearcoat" => m.clearcoat = self.render_num(&v, sp, key, 0.0, 1.0),
                "anisotropy" => m.anisotropy = self.render_num(&v, sp, key, 0.0, 1.0),
                _ => unreachable!(),
            }
        }
        Ok(Some(m))
    }

    /// Checks the material parts against the parts of the model (file context, at the end).
    pub(crate) fn check_render_parts(&mut self) {
        if !self.file_context {
            return;
        }
        let Some(d) = &self.render else { return };
        let names: Vec<String> = self.parts.iter().map(|p| p.name.clone()).collect();
        let mut bad = Vec::new();
        for m in &d.materials {
            if let RenderTarget::Part(p) = &m.target
                && !names.iter().any(|n| n == p || n.split('[').next() == Some(p.as_str()))
            {
                bad.push((m.span, p.clone()));
            }
        }
        for (sp, p) in bad {
            let known = if names.is_empty() { "–".to_string() } else { names.join(", ") };
            self.warn(
                sp,
                tr!(
                    "render: material {p}: kein Bauteil `{p}` (Bauteile: {known})",
                    "render: material {p}: no part `{p}` (parts: {known})"
                ),
            );
        }
    }

    fn render_duplicates(&mut self, settings: &[&RenderSetting]) {
        for (i, s) in settings.iter().enumerate() {
            if settings[i + 1..].iter().any(|t| t.name.name == s.name.name) {
                self.warn(
                    s.name.span,
                    tr!(
                        "render: `{}` ist doppelt gesetzt (der letzte Wert gilt)",
                        "render: `{}` is set twice (the last value applies)",
                        s.name.name
                    ),
                );
            }
        }
    }

    fn unknown_setting(&mut self, st: &RenderSetting, what: &str, known: &[&str]) {
        self.warn(
            st.name.span,
            tr!(
                "{what}: unbekannte Einstellung `{}` (bekannt: {})",
                "{what}: unknown setting `{}` (known: {})",
                st.name.name,
                known.join(", ")
            ),
        );
    }

    fn render_bool(&mut self, v: &Val<'a>, sp: Span, name: &str) -> Option<bool> {
        match v.v {
            Value::Bool(b) => Some(b),
            _ => {
                self.warn(
                    sp,
                    tr!(
                        "render: {name} erwartet true oder false, nicht {} – ignoriert",
                        "render: {name} expects true or false, not {} – ignored",
                        v.type_name()
                    ),
                );
                None
            }
        }
    }

    fn render_num(&mut self, v: &Val<'a>, sp: Span, name: &str, lo: f64, hi: f64) -> Option<f64> {
        match v.as_num() {
            Some(x) if x.is_finite() && x >= lo && x <= hi => Some(x),
            Some(x) if x.is_finite() => {
                let range = if hi.is_finite() { format!("{lo}…{hi}") } else { format!("≥ {lo}") };
                self.warn(
                    sp,
                    tr!(
                        "render: {name} = {x} außerhalb von {range} – ignoriert",
                        "render: {name} = {x} is outside {range} – ignored"
                    ),
                );
                None
            }
            _ => {
                self.warn(
                    sp,
                    tr!(
                        "render: {name} erwartet eine Zahl, nicht {} – ignoriert",
                        "render: {name} expects a number, not {} – ignored",
                        v.type_name()
                    ),
                );
                None
            }
        }
    }

    fn render_word(&mut self, v: &Val<'a>, sp: Span, name: &str, allowed: &[&str]) -> Option<String> {
        let w = v.as_str().map(|s| s.trim().to_ascii_lowercase());
        match w {
            Some(w) if allowed.contains(&w.as_str()) => Some(w),
            Some(w) if name == "preset" && w == "aluminum" => None,
            _ => {
                self.warn(
                    sp,
                    tr!(
                        "render: {name} erwartet {} – ignoriert",
                        "render: {name} expects {} – ignored",
                        allowed.iter().map(|a| format!("\"{a}\"")).collect::<Vec<_>>().join(", ")
                    ),
                );
                None
            }
        }
    }

    /// A colour: palette name, `#rrggbb`, or `[r, g, b]` (0…1).
    fn render_color(&mut self, v: &Val<'a>, sp: Span, name: &str) -> Option<[f32; 4]> {
        let c = match (&v.v, v.as_nums()) {
            (Value::Str(t), _) => palette::parse_color(t).filter(|c| c.kind != ColorKind::Random).map(|c| c.rgba),
            (_, Some(n)) if (n.len() == 3 || n.len() == 4) && n.iter().all(|x| (0.0..=1.0).contains(x)) => {
                Some([n[0] as f32, n[1] as f32, n[2] as f32, 1.0])
            }
            _ => None,
        };
        if c.is_none() {
            self.warn(
                sp,
                tr!(
                    "render: {name} erwartet eine Farbe (Palettenname, \"#rrggbb\" oder [r, g, b]) – ignoriert",
                    "render: {name} expects a colour (palette name, \"#rrggbb\" or [r, g, b]) – ignored"
                ),
            );
        }
        c.map(|c| [c[0], c[1], c[2], 1.0])
    }

    /// `background`: one colour or a list of two (top, bottom).
    fn render_background(&mut self, v: &Val<'a>, sp: Span) -> Option<Vec<[f32; 4]>> {
        if let Some(l) = v.as_list()
            && l.len() == 2
            && l.iter().all(|x| x.as_str().is_some() || x.as_list().is_some())
        {
            let a = self.render_color(&l[0], sp, "background")?;
            let b = self.render_color(&l[1], sp, "background")?;
            return Some(vec![a, b]);
        }
        self.render_color(v, sp, "background").map(|c| vec![c])
    }
}
