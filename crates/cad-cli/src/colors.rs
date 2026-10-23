//! Colours, filament slots and colour regions in `outline`, `measure --colors` and `export`
//! (Plan 16): one table for text and JSON.

use cad_session::protocol as p;
use serde_json::{Value as Json, json};

use crate::{Fail, Model, num};

/// Density of PLA for the mass estimate (g/cm³).
pub(crate) const PLA_DENSITY: f64 = 1.24;

/// Mass in grams of `volume` mm³ of PLA.
pub(crate) fn pla_grams(volume: f64) -> f64 {
    volume * PLA_DENSITY / 1000.0
}

fn source(s: p::ColorSource) -> &'static str {
    match s {
        p::ColorSource::Default => "default",
        p::ColorSource::Palette => "palette",
        p::ColorSource::Random => "random",
        p::ColorSource::Exact => "exact",
    }
}

/// Colour table of the scene, with volumes per colour if `kernel` (colour regions of the
/// parts, or of the root without parts). (JSON `colors`, JSON `regions`|null, text)
pub(crate) fn table(m: &mut Model, sc: &p::Scene, kernel: bool) -> Result<(Json, Json, String), Fail> {
    let sets = if kernel && sc.complete && !sc.colors.is_empty() {
        Some(m.session.color_regions(false, false).map_err(Fail::Kernel)?)
    } else {
        None
    };
    let volume_of = |key: &str| -> Option<f64> {
        sets.as_ref().map(|s| s.iter().flat_map(|x| &x.regions).filter(|r| r.color == key).map(|r| r.volume).sum())
    };
    let rows: Vec<Json> = sc
        .colors
        .iter()
        .map(|c| {
            let v = volume_of(&c.key);
            json!({
                "key": c.key, "label": c.label, "source": source(c.source), "filament": c.filament,
                "explicit": c.explicit, "lines": c.ranges.iter().map(|r| r.line).collect::<Vec<_>>(),
                "volume": v, "mass_g": v.map(pla_grams),
            })
        })
        .collect();
    let regions = match &sets {
        Some(s) => json!(
            s.iter()
                .map(|set| json!({
                    "part": set.part, "node": set.node, "total": set.total, "warning": set.warning,
                    "regions": set.regions.iter().map(|r| json!({
                        "color": r.color,
                        "label": sc.colors.iter().find(|c| c.key == r.color).map(|c| c.label.clone()),
                        "filament": r.filament, "volume": r.volume, "mass_g": pla_grams(r.volume),
                    })).collect::<Vec<_>>(),
                }))
                .collect::<Vec<_>>()
        ),
        None => Json::Null,
    };
    let mut t = String::new();
    if !sc.colors.is_empty() {
        t.push_str("Farben (Filament, Farbe, Volumen, Masse PLA 1,24 g/cm³):\n");
        for c in &sc.colors {
            let v = volume_of(&c.key);
            let lines: Vec<String> = c.ranges.iter().map(|r| format!("Z.{}", r.line)).collect();
            let mut l = format!("  F{:<2} {:<14} {}", c.filament, c.label, c.key);
            if let Some(v) = v {
                l.push_str(&format!("  {} mm³  {} g", num(v), num((pla_grams(v) * 100.0).round() / 100.0)));
            }
            if !lines.is_empty() {
                l.push_str(&format!("  {}", lines.join(", ")));
            }
            match c.source {
                p::ColorSource::Default => l.push_str("  (ohne color())"),
                p::ColorSource::Random => l.push_str("  (unbekannter Name: Zufallsfarbe)"),
                _ => {}
            }
            if c.explicit {
                l.push_str(&format!("  filament = {}", c.filament));
            }
            t.push_str(&l);
            t.push('\n');
        }
        for w in sets.iter().flatten().filter_map(|s| s.warning.as_ref()) {
            t.push_str(&format!("  Warnung: {w}\n"));
        }
        if let Some(s) = &sets
            && s.len() > 1
        {
            for set in s {
                let parts: Vec<String> = set
                    .regions
                    .iter()
                    .map(|r| {
                        let label =
                            sc.colors.iter().find(|c| c.key == r.color).map_or(r.color.as_str(), |c| c.label.as_str());
                        format!("F{} {label} {} mm³", r.filament, num(r.volume))
                    })
                    .collect();
                t.push_str(&format!("  {}: {}\n", set.part.as_deref().unwrap_or("Modell"), parts.join(", ")));
            }
        }
    }
    Ok((json!(rows), regions, t))
}
