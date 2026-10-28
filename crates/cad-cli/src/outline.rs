//! `mcad outline`: an overview of a project in one call — modules (parameters with defaults
//! and guessed types, grids and anchors evaluated with the defaults), functions, top-level
//! variables with values, grids and anchors of the file, named instances and parts (bounds,
//! volume), files and `include`/`use`, diagnostics.

use cad_i18n::{tr, trs};
use cad_session::protocol::{self as p, ToEngine};
use cad_syntax::ast::{Param, Stmt, StmtKind};
use serde_json::{Value as Json, json};

use crate::render::bounds_json;
use crate::{Common, Fail, Outcome, last_scene, load, num, vec3};

const AXES: [&str; 3] = ["x", "y", "z"];

fn ty(t: p::ParamType) -> &'static str {
    match t {
        p::ParamType::Number => "number",
        p::ParamType::Bool => "bool",
        p::ParamType::Vector => "vector",
        p::ParamType::String => "string",
        p::ParamType::List => "list",
        p::ParamType::Unknown => "unknown",
    }
}

/// Type name in the text output (German, or the JSON name in English).
fn ty_text(t: &'static str) -> &'static str {
    if !cad_i18n::is_de() {
        return if t == "unknown" { "?" } else { t };
    }
    match t {
        "number" => "Zahl",
        "bool" => "Bool",
        "vector" => "Vektor",
        "string" => "Text",
        "list" => "Liste",
        _ => "?",
    }
}

fn plane_kind(pl: &p::Plane) -> &'static str {
    if pl.implicit {
        "implicit"
    } else if pl.literal.is_some() {
        "literal"
    } else if pl.is_ref {
        "reference"
    } else {
        "expression"
    }
}

fn file_path(sc: &p::Scene, r: &p::SrcRange) -> String {
    let id = r.file.unwrap_or(0) as usize;
    sc.files.get(id).map(|f| f.path.clone()).unwrap_or_default()
}

/// Source text of a protocol range (UTF-16 offsets local to its file).
fn range_text(proj: &cad_eval::Project, r: &p::SrcRange) -> String {
    let Some(f) = proj.files.get(r.file.unwrap_or(0) as usize) else { return String::new() };
    let (a, b) = (f.lines.utf16_to_byte(r.from) as usize, f.lines.utf16_to_byte(r.to) as usize);
    f.text.get(a..b).unwrap_or("").to_string()
}

fn grid_json(sc: &p::Scene, proj: &cad_eval::Project, g: &p::Grid) -> Json {
    json!({
        "name": g.name,
        "file": file_path(sc, &g.range),
        "line": g.range.line,
        "planes": g.planes.iter().map(|pl| json!({
            "axis": pl.axis, "axis_name": AXES[pl.axis as usize], "name": pl.name, "value": pl.value, "kind": plane_kind(pl),
            "expr": pl.value_range.map(|r| range_text(proj, &r)),
        })).collect::<Vec<_>>(),
    })
}

fn grid_text(g: &p::Grid, proj: &cad_eval::Project) -> String {
    let mut axes = Vec::new();
    for a in 0..3u8 {
        let planes: Vec<String> = g
            .planes
            .iter()
            .filter(|pl| pl.axis == a && !pl.implicit)
            .map(|pl| {
                let expr = pl.value_range.map(|r| range_text(proj, &r)).unwrap_or_default();
                if pl.literal.is_some() || expr.is_empty() || expr == num(pl.value) {
                    format!("{} = {}", pl.name, num(pl.value))
                } else {
                    format!("{} = {expr} → {}", pl.name, num(pl.value))
                }
            })
            .collect();
        if !planes.is_empty() {
            axes.push(format!("{}: {}", ["x", "y", "z"][a as usize], planes.join(", ")));
        }
    }
    tr!("Raster {} (Z.{}): {}", "grid {} (line {}): {}", g.name, g.range.line, axes.join("; "))
}

fn params_text(params: &[Param], src: &dyn Fn(cad_syntax::Span) -> String) -> Vec<Json> {
    params
        .iter()
        .map(|pa| json!({ "name": pa.name.name, "default": pa.default.as_ref().map(|d| src(d.span)) }))
        .collect()
}

/// One line about the file's render block: what it sets.
fn render_block_text(r: &p::RenderBlock) -> String {
    let on = |b: bool| if b { "on" } else { "off" };
    let mut parts: Vec<String> = Vec::new();
    parts.push(format!("style {}", r.style.as_deref().unwrap_or("studio")));
    let sw = [
        ("edges", r.edges),
        ("shadows", r.shadows),
        ("ao", r.ao),
        ("ground", r.ground),
        ("reflections", r.reflections),
    ];
    parts.extend(sw.iter().filter_map(|(k, v)| v.map(|v| format!("{k} {}", on(v)))));
    let nums = [
        ("ambient", r.ambient),
        ("light_intensity", r.light_intensity),
        ("shadow_softness", r.shadow_softness),
        ("ao_radius", r.ao_radius),
        ("ao_strength", r.ao_strength),
        ("ground_reflection", r.ground_reflection),
        ("ground_roughness", r.ground_roughness),
    ];
    parts.extend(nums.iter().filter_map(|(k, v)| v.map(|v| format!("{k} {}", num(v)))));
    if let Some(e) = &r.environment {
        parts.push(format!("environment {e}"));
    }
    if let Some(c) = &r.ground_color {
        parts.push(format!("ground_color {c}"));
    }
    if let Some(b) = &r.background {
        parts.push(format!("background {}", b.join(",")));
    }
    let mut t = tr!("Render-Block (Zeile {}): {}\n", "Render block (line {}): {}\n", r.range.line, parts.join(", "));
    for l in &r.lights {
        t.push_str(&tr!(
            "  Licht {}: Azimut {}°, Höhe {}°, Stärke {}, {}\n",
            "  light {}: azimuth {}°, elevation {}°, intensity {}, {}\n",
            l.name,
            num(l.azimuth),
            num(l.elevation),
            num(l.intensity),
            l.color
        ));
    }
    for m in &r.materials {
        let target = match &m.target {
            p::RenderTarget::Color { color } => format!("\"{color}\""),
            p::RenderTarget::Part { part } => tr!("Bauteil {part}", "part {part}"),
        };
        let mut v: Vec<String> = m.preset.iter().cloned().collect();
        let vals = [
            ("metalness", m.metalness),
            ("roughness", m.roughness),
            ("clearcoat", m.clearcoat),
            ("anisotropy", m.anisotropy),
        ];
        v.extend(vals.iter().filter_map(|(k, x)| x.map(|x| format!("{k} {}", num(x)))));
        t.push_str(&tr!("  Material {target}: {}\n", "  material {target}: {}\n", v.join(", ")));
    }
    t
}

pub(crate) fn command(c: &Common, kernel: bool) -> Result<Outcome, Fail> {
    if c.module.is_some() {
        return Err(Fail::Usage(tr!(
            "outline zeigt immer das ganze Projekt (ohne --module; -D setzt Variablen der Datei)",
            "outline always shows the whole project (without --module; -D sets variables of the file)"
        )));
    }
    let defines = crate::defines(c)?;
    let mut m = load(c, None, true, false)?;
    let mut j = m.base_json("outline");
    let Some(file_scene) = m.scene.clone() else {
        let t = m.diagnostics_text();
        j["counts"] = json!({ "errors": m.diagnostics.iter().filter(|d| d.severity == p::Severity::Error).count() });
        return Ok(Outcome { json: j, text: t, errors: true, exit: None });
    };
    // Grids and anchors per module: each module evaluated as context with its defaults
    // (language only, milliseconds).
    let mut per_module: Vec<(Vec<p::Grid>, Vec<p::LocalAnchor>, usize)> = Vec::new();
    for md in &file_scene.modules {
        let mut path = md.path.clone();
        path.push(md.name.clone());
        let msgs = m.session.post(ToEngine::SetContext { module: Some(path.join(".")), overrides: Default::default() });
        m.session.post(ToEngine::Cancel);
        let errs = msgs
            .iter()
            .filter_map(|x| match x {
                p::FromEngine::Diagnostics { items, .. } => {
                    Some(items.iter().filter(|d| d.severity == p::Severity::Error).count())
                }
                _ => None,
            })
            .next_back()
            .unwrap_or(0);
        match last_scene(&msgs) {
            Some(sc) => {
                let grids = sc.grids.iter().filter(|g| g.in_context).cloned().collect();
                let anchors = sc.instances.first().map(|i| i.anchors.clone()).unwrap_or_default();
                per_module.push((grids, anchors, errs));
            }
            None => per_module.push((Vec::new(), Vec::new(), errs)),
        }
    }
    // back to the file; with the kernel for bounds and volumes of parts
    let mut msgs = m.session.post(ToEngine::SetContext { module: None, overrides: defines });
    if kernel {
        msgs.extend(m.session.run_to_idle());
    } else {
        m.session.post(ToEngine::Cancel);
    }
    if let Some(d) = crate::last_diagnostics(&msgs) {
        m.diagnostics = d;
    }
    let sc = if kernel { m.session.done().map(|d| d.scene.clone()).unwrap_or(file_scene) } else { file_scene };
    let (colors, color_regions, colors_text) = crate::colors::table(&mut m, &sc, kernel)?;
    let cur = m.session.current().ok_or_else(|| Fail::Kernel(tr!("keine Auswertung", "no evaluation")))?;
    let proj = &cur.proj;
    let src = |s: cad_syntax::Span| proj.text(s).to_string();

    let mut t = String::new();
    let errors = m.diagnostics.iter().filter(|d| d.severity == p::Severity::Error).count();
    let warnings = m.diagnostics.iter().filter(|d| d.severity == p::Severity::Warning).count();
    t.push_str(&tr!("{}: {} Fehler, {} Warnungen\n", "{}: {} errors, {} warnings\n", m.file, errors, warnings));

    // files and includes
    let files: Vec<Json> = sc.files.iter().map(|f| json!({ "path": f.path, "role": f.role })).collect();
    let includes: Vec<Json> = sc
        .includes
        .iter()
        .map(|i| json!({ "file": file_path(&sc, &i.range), "line": i.range.line, "path": i.target, "use": i.is_use, "found": i.found }))
        .collect();
    if !sc.includes.is_empty() {
        t.push_str(trs!("Dateien:\n", "Files:\n"));
        for i in &sc.includes {
            t.push_str(&format!(
                "  {} <{}>  {}:{}{}\n",
                if i.is_use { "use" } else { "include" },
                i.target,
                file_path(&sc, &i.range),
                i.range.line,
                if i.found { "" } else { trs!("  (nicht gefunden)", "  (not found)") }
            ));
        }
    }

    // modules
    let mut modules = Vec::new();
    if !sc.modules.is_empty() {
        t.push_str(trs!("Module:\n", "Modules:\n"));
    }
    for (k, md) in sc.modules.iter().enumerate() {
        let (grids, anchors, errs) = per_module.get(k).cloned().unwrap_or_default();
        let params: Vec<Json> = md
            .params
            .iter()
            .map(|pa| json!({ "name": pa.name, "default": pa.default_text, "default_value": pa.default_value, "type": ty(pa.ty) }))
            .collect();
        let ptxt: Vec<String> = md
            .params
            .iter()
            .map(|pa| match &pa.default_text {
                Some(d) => format!("{} = {d}: {}", pa.name, ty_text(ty(pa.ty))),
                None => format!("{}: {}", pa.name, ty_text(ty(pa.ty))),
            })
            .collect();
        let mut qual = md.path.clone();
        qual.push(md.name.clone());
        t.push_str(&format!(
            "  {}({})  {}:{}–{}\n",
            qual.join("."),
            ptxt.join(", "),
            file_path(&sc, &md.range),
            md.range.line,
            md.range.end_line
        ));
        for g in &grids {
            t.push_str(&format!("    {}\n", grid_text(g, proj)));
        }
        if !anchors.is_empty() {
            let a: Vec<String> = anchors
                .iter()
                .map(|a| {
                    let dir = a.dir.as_ref().map(|d| format!(" dir {}", vec3(d))).unwrap_or_default();
                    tr!("{} = {}{dir} (Z.{})", "{} = {}{dir} (line {})", a.name, vec3(&a.point), a.range.line)
                })
                .collect();
            t.push_str(&tr!("    Anker: {}\n", "    anchors: {}\n", a.join(", ")));
        }
        if errs > 0 {
            t.push_str(&tr!("    ({errs} Fehler mit Standardwerten)\n", "    ({errs} errors with the defaults)\n"));
        }
        modules.push(json!({
            "name": md.name, "path": md.path, "qualified": qual.join("."),
            "file": file_path(&sc, &md.range), "line": md.range.line, "end_line": md.range.end_line,
            "params": params,
            "grids": grids.iter().map(|g| grid_json(&sc, proj, g)).collect::<Vec<_>>(),
            "anchors": anchors.iter().map(|a| json!({ "name": a.name, "point": a.point, "dir": a.dir, "line": a.range.line })).collect::<Vec<_>>(),
            "errors_with_defaults": errs,
        }));
    }

    // functions and top-level variables (main file with includes, used libraries)
    let mut functions = Vec::new();
    let mut variables = Vec::new();
    // functions of module bodies are reachable as `modul.f(…)` (namespaces, Plan 17.2)
    fn module_functions(
        stmts: &[Stmt],
        prefix: &str,
        out: &mut Vec<(String, std::rc::Rc<cad_syntax::ast::FunctionDef>)>,
    ) {
        for s in stmts {
            match &s.kind {
                StmtKind::Function(f) if !prefix.is_empty() => {
                    out.push((format!("{prefix}{}", f.name.name), f.clone()))
                }
                StmtKind::Module(m) => module_functions(&m.body, &format!("{prefix}{}.", m.name.name), out),
                _ => {}
            }
        }
    }
    let visit = |stmts: &[Stmt], lib: bool, functions: &mut Vec<Json>, variables: &mut Vec<Json>| {
        let mut nested = Vec::new();
        module_functions(stmts, "", &mut nested);
        for (name, f) in &nested {
            functions.push(json!({
                "name": name, "params": params_text(&f.params, &src),
                "file": proj.path_of(f.span), "line": proj.line(f.span), "body": src(f.body.span),
            }));
        }
        for s in stmts {
            match &s.kind {
                StmtKind::Function(f) => functions.push(json!({
                    "name": f.name.name, "params": params_text(&f.params, &src),
                    "file": proj.path_of(f.span), "line": proj.line(f.span), "body": src(f.body.span),
                })),
                StmtKind::Assign { name, value } if !lib => {
                    let hint = sc.hints.iter().find(|h| h.stmt == s.id);
                    variables.push(json!({
                        "name": name.name, "expr": src(value.span),
                        "value": hint.map(|h| h.value.clone()), "text": hint.map(|h| h.text.clone()),
                        "literal": hint.and_then(|h| h.literal),
                        "file": proj.path_of(s.span), "line": proj.line(s.span),
                    }));
                }
                _ => {}
            }
        }
    };
    visit(&proj.main.stmts, false, &mut functions, &mut variables);
    for l in &proj.libs {
        visit(&l.ast.stmts, true, &mut functions, &mut variables);
    }
    if !functions.is_empty() {
        t.push_str(trs!("Funktionen:\n", "Functions:\n"));
        for f in &functions {
            let ps: Vec<String> = f["params"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| match p["default"].as_str() {
                    Some(d) => format!("{} = {d}", p["name"].as_str().unwrap_or("")),
                    None => p["name"].as_str().unwrap_or("").to_string(),
                })
                .collect();
            t.push_str(&format!(
                "  {}({}) = {}  {}:{}\n",
                f["name"].as_str().unwrap_or(""),
                ps.join(", "),
                f["body"].as_str().unwrap_or(""),
                f["file"].as_str().unwrap_or(""),
                f["line"]
            ));
        }
    }
    if !variables.is_empty() {
        t.push_str(trs!("Variablen:\n", "Variables:\n"));
        for v in &variables {
            let val = v["text"].as_str().map(|s| s.to_string()).unwrap_or_else(|| "?".into());
            let expr = v["expr"].as_str().unwrap_or("");
            let shown = if expr == val { val.clone() } else { format!("{expr}  → {val}") };
            let file = v["file"].as_str().unwrap_or("");
            let at = if file == proj.files[0].path {
                tr!("Z.{}", "line {}", v["line"])
            } else {
                format!("{file}:{}", v["line"])
            };
            t.push_str(&format!("  {} = {shown}  {at}\n", v["name"].as_str().unwrap_or("")));
        }
    }
    let file_grids: Vec<&p::Grid> = sc.grids.iter().filter(|g| g.in_context).collect();
    for g in &file_grids {
        t.push_str(&format!("{}\n", grid_text(g, proj)));
    }

    // named instances, parts, anchors in file coordinates. A name given inside a module body
    // (`boss[0]: standoff();` in `pi_plate`) belongs to that module, not to the file: such instances
    // are listed separately, with the module that names them ("scope": "module").
    let named_all: Vec<&p::Instance> = sc.instances.iter().filter(|i| i.label.is_some() && i.id != 0).collect();
    let in_module = |i: &p::Instance| i.depth > 1;
    let named: Vec<&p::Instance> = named_all.iter().copied().filter(|i| !in_module(i)).collect();
    let nested: Vec<&p::Instance> = named_all.iter().copied().filter(|i| in_module(i)).collect();
    let owner = |i: &p::Instance| -> String {
        i.parent.and_then(|q| sc.instances.iter().find(|x| x.id == q)).map(|x| x.module.clone()).unwrap_or_default()
    };
    let instances: Vec<Json> = named_all
        .iter()
        .map(|i| {
            let r = i.call_range.unwrap_or_default();
            json!({
                "name": i.label, "module": i.module, "file": file_path(&sc, &r), "line": r.line, "depth": i.depth,
                "part": sc.parts.iter().find(|pt| pt.inst == i.id).map(|pt| pt.name.clone()),
                "scope": if in_module(i) { "module" } else { "file" },
                "in_module": if in_module(i) { Some(owner(i)) } else { None },
            })
        })
        .collect();
    let parts: Vec<Json> = sc
        .parts
        .iter()
        .map(|pt| {
            let module = sc.instances.get(pt.inst as usize).map(|i| i.module.clone()).unwrap_or_default();
            json!({ "name": pt.name, "module": module, "line": pt.range.line, "node": pt.node,
                    "bounds": bounds_json(&pt.bounds), "volume": pt.volume })
        })
        .collect();
    if !named.is_empty() {
        t.push_str(trs!("Benannte Instanzen:\n", "Named instances:\n"));
        for i in &named {
            let r = i.call_range.unwrap_or_default();
            let part = sc.parts.iter().find(|pt| pt.inst == i.id);
            let mut l = tr!("  {}: {}  Z.{}", "  {}: {}  line {}", i.label.as_deref().unwrap_or(""), i.module, r.line);
            if let Some(pt) = part {
                l.push_str(trs!("  Bauteil", "  part"));
                if let Some(b) = &pt.bounds {
                    l.push_str(&tr!(", Hülle {}–{}", ", bounds {}–{}", vec3(&b.min), vec3(&b.max)));
                }
                if let Some(v) = pt.volume {
                    l.push_str(&tr!(", Volumen {} mm³", ", volume {} mm³", num(v)));
                }
            }
            t.push_str(&l);
            t.push('\n');
        }
    }
    if !nested.is_empty() {
        // grouped by the module whose body names them, with how often they occur
        let mut groups: Vec<(String, Vec<String>)> = Vec::new();
        for i in &nested {
            let m = owner(i);
            let item = format!("{}: {}", i.label.as_deref().unwrap_or(""), i.module);
            match groups.iter_mut().find(|g| g.0 == m) {
                Some(g) => {
                    if !g.1.contains(&item) {
                        g.1.push(item)
                    }
                }
                None => groups.push((m, vec![item])),
            }
        }
        t.push_str(trs!(
            "Benannte Instanzen in Modulen (nur dort sichtbar):\n",
            "Named instances in modules (visible only there):\n"
        ));
        for (m, items) in &groups {
            t.push_str(&format!("  in {m}: {}\n", items.join(", ")));
        }
    }
    let file_anchor = |a: &p::Anchor| sc.instances.iter().find(|i| i.id == a.inst).is_none_or(|i| !in_module(i));
    let anchors: Vec<Json> = sc
        .anchors
        .iter()
        .filter(|a| a.text.is_some() && file_anchor(a))
        .map(|a| json!({ "ref": a.text, "point": a.point, "dir": a.dir, "line": a.range.line }))
        .collect();
    if !anchors.is_empty() {
        let a: Vec<String> = sc
            .anchors
            .iter()
            .filter(|a| file_anchor(a))
            .filter_map(|a| {
                let dir = a.dir.as_ref().map(|d| format!(" dir {}", vec3(d))).unwrap_or_default();
                a.text.as_ref().map(|tx| format!("{tx} = {}{dir}", vec3(&a.point)))
            })
            .collect();
        t.push_str(&tr!("Anker: {}\n", "Anchors: {}\n", a.join(", ")));
    }
    t.push_str(&colors_text);
    if let Some(r) = &sc.render {
        t.push_str(&render_block_text(r));
    }
    if let Some(root) = sc.root.and_then(|r| sc.nodes.get(r as usize))
        && let Some(b) = &root.bounds
    {
        let wb = crate::world_bounds(b, &crate::mat(&root.display_world));
        let s: Vec<String> = (0..3).map(|i| num(wb.max.0[i] - wb.min.0[i])).collect();
        t.push_str(&tr!(
            "Gesamt: Hülle {}–{} ({} mm), {} Knoten\n",
            "Total: bounds {}–{} ({} mm), {} nodes\n",
            vec3(&wb.min),
            vec3(&wb.max),
            s.join(" × "),
            sc.nodes.len()
        ));
    }
    let (asserts_j, asserts_t) = crate::asserts_summary(&sc, None);
    if !asserts_t.is_empty() {
        t.push_str(trs!(
            "Asserts (Prüfungen nach der Auswertung, Verletzung = Warnung):\n",
            "Asserts (checks after the evaluation, violation = warning):\n"
        ));
        t.push_str(&asserts_t);
    }
    t.push_str(&m.diagnostics_text());

    j["asserts"] = asserts_j;
    j["files"] = json!(files);
    j["includes"] = json!(includes);
    j["modules"] = json!(modules);
    j["functions"] = json!(functions);
    j["variables"] = json!(variables);
    j["grids"] = json!(file_grids.iter().map(|g| grid_json(&sc, proj, g)).collect::<Vec<_>>());
    j["anchors"] = json!(anchors);
    j["instances"] = json!(instances);
    j["parts"] = json!(parts);
    j["colors"] = colors;
    j["color_regions"] = color_regions;
    j["render"] = json!(sc.render);
    j["counts"] = json!({ "errors": errors, "warnings": warnings, "nodes": sc.nodes.len(),
                          "modules": sc.modules.len(), "parts": sc.parts.len(), "colors": sc.colors.len() });
    j["kernel"] = json!(kernel);
    j["diagnostics"] = json!(m.diagnostics);
    j["ok"] = json!(errors == 0);
    let has_errors = m.has_errors();
    Ok(Outcome { json: j, text: t, errors: has_errors, exit: None })
}
