//! Embeds the usage manual (`manuell/*.md` at the workspace root, English, for LLM agents) into
//! the binary: `$OUT_DIR/manual.rs` lists every page as `(name, include_str!(…))`, sorted by
//! file name (`00-overview.md` → topic `overview`). `cad usage` prints it (`src/usage.rs`).

use std::path::PathBuf;

fn main() {
    let dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../../manuell");
    let dir = dir.canonicalize().unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut pages: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "md"))
        .collect();
    pages.sort();
    let mut out = String::from("pub(crate) const PAGES: &[(&str, &str)] = &[\n");
    for p in &pages {
        println!("cargo:rerun-if-changed={}", p.display());
        let stem = p.file_stem().unwrap().to_str().expect("UTF-8 file name");
        // `00-overview` → `overview`: the number only orders the pages
        let name = stem.split_once('-').filter(|(n, _)| n.chars().all(|c| c.is_ascii_digit())).map_or(stem, |(_, t)| t);
        out.push_str(&format!("    ({name:?}, include_str!({:?})),\n", p.display().to_string()));
    }
    out.push_str("];\n");
    let target = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("manual.rs");
    std::fs::write(target, out).unwrap();
}
