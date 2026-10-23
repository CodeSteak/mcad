//! `cad export --part name` / `--all-parts -o ordner/`: export of parts (named instances at
//! the top level of the main file, Plan 12.9).

use std::path::Path;

use cad_session::protocol as p;
use serde_json::json;

use crate::{Common, Fail, Outcome, load, num};

fn format_of(format: Option<&str>, output: &Path, dir: bool) -> Result<p::ExportFormat, Fail> {
    let f = format.map(|s| s.to_ascii_lowercase()).or_else(|| {
        if dir { Some("stl".into()) } else { output.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()) }
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

pub(crate) fn export_parts(
    c: &Common,
    output: &Path,
    format: Option<&str>,
    names: &[String],
    all: bool,
) -> Result<Outcome, Fail> {
    let fmt = format_of(format, output, all)?;
    let mut m = load(c, None, true, true)?;
    let mut j = m.base_json("export");
    let Some(scene) = m.scene.clone() else {
        let t = m.diagnostics_text();
        return Ok(Outcome { json: j, text: t, errors: true });
    };
    if scene.parts.is_empty() {
        return Err(Fail::Usage("keine Bauteile: benannte Instanzen auf oberster Ebene, z. B. `deckel: deckel();`".into()));
    }
    let volume = |name: &str| scene.parts.iter().find(|pt| pt.name == name).and_then(|pt| pt.volume);
    let mut t = m.diagnostics_text();
    let mut rows = Vec::new();
    if all {
        std::fs::create_dir_all(output).map_err(|e| Fail::Usage(format!("{}: {e}", output.display())))?;
        for pt in &scene.parts {
            let (bytes, tris) = m.session.export_parts(fmt, std::slice::from_ref(&pt.name), false).map_err(Fail::Kernel)?;
            let file = output.join(cad_session::part_file_name(&pt.name, ext(fmt)));
            std::fs::write(&file, &bytes).map_err(|e| Fail::Usage(format!("{}: {e}", file.display())))?;
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
        let (bytes, tris) = m.session.export_parts(fmt, names, false).map_err(Fail::Kernel)?;
        std::fs::write(output, &bytes).map_err(|e| Fail::Usage(format!("{}: {e}", output.display())))?;
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
    j["parts"] = json!(rows);
    let errors = m.has_errors();
    Ok(Outcome { json: j, text: t, errors })
}
