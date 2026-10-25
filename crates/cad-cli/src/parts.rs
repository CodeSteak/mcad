//! `cad export --part name` / `--all-parts -o ordner/`: export of parts (named instances at
//! the top level of the main file, Plan 12.9).

use std::path::Path;

use cad_session::protocol as p;
use serde_json::json;

use crate::{Common, Fail, Outcome, load, num, utc_timestamp};

fn format_of(format: Option<&str>, output: &Path, dir: bool) -> Result<p::ExportFormat, Fail> {
    let f = format.map(|s| s.to_ascii_lowercase()).or_else(|| {
        if dir {
            Some("stl".into())
        } else {
            output.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase())
        }
    });
    match f.as_deref() {
        Some("stl") => Ok(p::ExportFormat::Stl),
        Some("3mf") => Ok(p::ExportFormat::ThreeMf),
        Some("step" | "stp") => Ok(p::ExportFormat::Step),
        Some(f) => Err(Fail::Usage(format!("unbekanntes Exportformat: {f} (stl, 3mf oder step)"))),
        None => Err(Fail::Usage("Exportformat fehlt (Endung .stl/.3mf/.step oder --format)".into())),
    }
}

fn ext(f: p::ExportFormat) -> &'static str {
    match f {
        p::ExportFormat::Stl => "stl",
        p::ExportFormat::ThreeMf => "3mf",
        p::ExportFormat::Step => "step",
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn export_parts(
    c: &Common,
    output: &Path,
    format: Option<&str>,
    schema: &str,
    names: &[String],
    all: bool,
    multicolor: Option<cad_engine::color_export::PackageOptions>,
    ignore_asserts: bool,
) -> Result<Outcome, Fail> {
    let pkg = multicolor.unwrap_or_default();
    let multicolor = multicolor.is_some();
    let fmt = format_of(format, output, all)?;
    let schema = cad_engine::StepSchema::parse(schema)
        .ok_or_else(|| Fail::Usage(format!("unbekanntes STEP-Schema: {schema} (ap242 oder ap214)")))?;
    // STEP header: product name = part name(s) (the engine joins them), time of the export.
    let step = cad_engine::StepOptions { schema, timestamp: utc_timestamp(), ..Default::default() };
    let mut m = load(c, None, true, true)?;
    let mut j = m.base_json("export");
    let Some(scene) = m.scene.clone() else {
        let t = m.diagnostics_text();
        return Ok(Outcome { json: j, text: t, errors: true, exit: None });
    };
    if scene.parts.is_empty() {
        return Err(Fail::Usage(
            "keine Bauteile: benannte Instanzen auf oberster Ebene, z. B. `deckel: deckel();`".into(),
        ));
    }
    let assert_note = match crate::assert_gate(&m, &mut j, ignore_asserts) {
        Ok(n) => n,
        Err(o) => return Ok(o),
    };
    let volume = |name: &str| scene.parts.iter().find(|pt| pt.name == name).and_then(|pt| pt.volume);
    let mut t = m.diagnostics_text();
    t.push_str(&assert_note);
    let mut rows = Vec::new();
    if all {
        crate::host::create_dir_all(output).map_err(|e| Fail::Usage(format!("{}: {e}", output.display())))?;
        for pt in &scene.parts {
            let one = std::slice::from_ref(&pt.name);
            let (bytes, tris) = if multicolor && fmt == p::ExportFormat::ThreeMf {
                m.session
                    .export_colors_with(fmt, Some(one), pkg)
                    .map(|(b, st)| (b, st.triangles))
                    .map_err(Fail::Kernel)?
            } else {
                m.session.export_parts_with(fmt, one, false, &step).map_err(Fail::Kernel)?
            };
            let file = output.join(cad_session::part_file_name(&pt.name, ext(fmt)));
            crate::host::write_file(&file, &bytes).map_err(|e| Fail::Usage(format!("{}: {e}", file.display())))?;
            t.push_str(&format!(
                "{}: {} ({} Dreiecke, {} Bytes{})\n",
                pt.name,
                file.display(),
                tris,
                bytes.len(),
                pt.volume.map(|v| format!(", Volumen {} mm³", num(v))).unwrap_or_default()
            ));
            rows.push(json!({
                "name": pt.name, "output": file.display().to_string(), "bytes": bytes.len(),
                "triangles": tris, "volume": pt.volume,
            }));
        }
        j["output"] = json!(output.display().to_string());
    } else {
        let (bytes, tris) = if multicolor && fmt == p::ExportFormat::ThreeMf {
            m.session
                .export_colors_with(fmt, Some(names), pkg)
                .map(|(b, st)| (b, st.triangles))
                .map_err(Fail::Kernel)?
        } else {
            m.session.export_parts_with(fmt, names, false, &step).map_err(Fail::Kernel)?
        };
        crate::host::write_file(output, &bytes).map_err(|e| Fail::Usage(format!("{}: {e}", output.display())))?;
        t.push_str(&format!("{}: {} ({} Dreiecke, {} Bytes)\n", names.join(", "), output.display(), tris, bytes.len()));
        for n in names {
            rows.push(json!({ "name": n, "output": output.display().to_string(), "volume": volume(n) }));
        }
        j["output"] = json!(output.display().to_string());
        j["bytes"] = json!(bytes.len());
        j["triangles"] = json!(tris);
        let vols: Option<Vec<f64>> = names.iter().map(|n| volume(n)).collect();
        j["volume"] = json!(vols.map(|v| v.iter().sum::<f64>()));
    }
    j["format"] = json!(fmt);
    j["multicolor"] = json!(multicolor && fmt == p::ExportFormat::ThreeMf);
    j["parts"] = json!(rows);
    let errors = m.has_errors();
    Ok(Outcome { json: j, text: t, errors, exit: None })
}
