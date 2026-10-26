//! Colours, filament slots and colour regions in `outline`, `measure --colors` and `export`
//! (Plan 16): one table for text and JSON.

use cad_i18n::{tr, trs};
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

/// "Z.n" of a range, with the file name for ranges in other project files (`use`/`include`).
fn line_ref(sc: &p::Scene, r: &p::SrcRange) -> String {
    match r.file {
        None | Some(0) => tr!("Z.{}", "line {}", r.line),
        Some(id) => {
            let f = sc.files.iter().find(|f| f.id == id).map(|f| f.path.as_str()).unwrap_or("?");
            format!("{f}:{}", r.line)
        }
    }
}

/// Colour table of the scene, with volumes per colour if `kernel` (colour regions of the
/// parts and the material outside them, or of the root without parts). (JSON `colors`, JSON
/// `regions`|null, text)
pub(crate) fn table(m: &mut Model, sc: &p::Scene, kernel: bool) -> Result<(Json, Json, String), Fail> {
    table_of(m, sc, kernel, None)
}

/// [`table`] of some parts only (`mcad export --part`): their regions and the colours in them.
pub(crate) fn table_of(
    m: &mut Model,
    sc: &p::Scene,
    kernel: bool,
    only: Option<&[String]>,
) -> Result<(Json, Json, String), Fail> {
    let mut sets = if kernel && sc.complete && !sc.colors.is_empty() {
        Some(m.session.color_regions(false, false).map_err(Fail::Kernel)?)
    } else {
        None
    };
    let mut shown: Vec<&p::ColorInfo> = sc.colors.iter().collect();
    if let Some(names) = only
        && let Some(s) = sets.as_mut()
    {
        s.retain(|set| set.part.as_ref().is_some_and(|p| names.contains(p)));
        shown.retain(|c| s.iter().flat_map(|x| &x.regions).any(|r| r.color == c.key));
    }
    let volume_of = |key: &str| -> Option<f64> {
        sets.as_ref().map(|s| s.iter().flat_map(|x| &x.regions).filter(|r| r.color == key).map(|r| r.volume).sum())
    };
    let rows: Vec<Json> = shown
        .iter()
        .map(|c| {
            let v = volume_of(&c.key);
            json!({
                "key": c.key, "label": c.label, "source": source(c.source), "filament": c.filament,
                "explicit": c.explicit, "lines": c.ranges.iter().map(|r| r.line).collect::<Vec<_>>(),
                "places": c.ranges.iter().map(|r| line_ref(sc, r)).collect::<Vec<_>>(),
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
    if !shown.is_empty() {
        t.push_str(trs!(
            "Farben (Filament, Farbe, Volumen, Masse PLA 1,24 g/cm³):\n",
            "Colours (filament, colour, volume, mass PLA 1.24 g/cm³):\n"
        ));
        for c in &shown {
            let v = volume_of(&c.key);
            let lines: Vec<String> = c.ranges.iter().map(|r| line_ref(sc, r)).collect();
            let mut l = format!("  F{:<2} {:<14} {}", c.filament, c.label, c.key);
            if let Some(v) = v {
                l.push_str(&format!("  {} mm³  {} g", num(v), num((pla_grams(v) * 100.0).round() / 100.0)));
            }
            if !lines.is_empty() {
                l.push_str(&format!("  {}", lines.join(", ")));
            }
            match c.source {
                p::ColorSource::Default => l.push_str(trs!("  (ohne color())", "  (without color())")),
                p::ColorSource::Random => {
                    l.push_str(trs!("  (unbekannter Name: Zufallsfarbe)", "  (unknown name: random colour)"))
                }
                _ => {}
            }
            if c.explicit {
                l.push_str(&format!("  filament = {}", c.filament));
            }
            t.push_str(&l);
            t.push('\n');
        }
        for w in sets.iter().flatten().filter_map(|s| s.warning.as_ref()) {
            t.push_str(&tr!("  Warnung: {w}\n", "  warning: {w}\n"));
        }
        if let Some(s) = &sets
            && s.len() > 1
        {
            // one line per part (the material outside the parts may be several nodes)
            let mut names: Vec<Option<&String>> = Vec::new();
            for set in s {
                if !names.contains(&set.part.as_ref()) {
                    names.push(set.part.as_ref());
                }
            }
            for name in names {
                let mut acc: Vec<(u32, String, f64)> = Vec::new();
                for r in s.iter().filter(|x| x.part.as_ref() == name).flat_map(|x| &x.regions) {
                    let label =
                        sc.colors.iter().find(|c| c.key == r.color).map_or(r.color.clone(), |c| c.label.clone());
                    match acc.iter_mut().find(|a| a.1 == label && a.0 == r.filament) {
                        Some(a) => a.2 += r.volume,
                        None => acc.push((r.filament, label, r.volume)),
                    }
                }
                let parts: Vec<String> = acc.iter().map(|(f, l, v)| format!("F{f} {l} {} mm³", num(*v))).collect();
                let name = match name {
                    None => trs!("Modell", "model"),
                    // the material outside the parts (a fixed name, see cad_session::NO_PART)
                    Some(n) if n == cad_session::NO_PART && !cad_i18n::is_de() => "outside any part",
                    Some(n) => n.as_str(),
                };
                t.push_str(&format!("  {name}: {}\n", parts.join(", ")));
            }
        }
    }
    Ok((json!(rows), regions, t))
}
