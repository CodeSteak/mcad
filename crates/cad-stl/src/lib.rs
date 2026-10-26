//! STL import (Plan 21, phase B): [`parse`] reads ASCII and binary STL (units: mm),
//! [`prepare`] turns the triangle soup into a [`TriMesh`]: vertices welded, degenerate and
//! duplicate triangles removed, orientation made consistent and outward, closedness and
//! manifoldness checked, coplanar neighbouring triangles merged into planar faces.
//!
//! Binary or ASCII: a file whose size is exactly `84 + 50·n` (n from the header) is binary,
//! even if it starts with `solid` (many exporters write that into the binary header);
//! otherwise a file starting with `solid` is ASCII. Messages are in the language of
//! [`cad_i18n`] (they reach the user as diagnostics).

mod prepare;

use cad_i18n::{tr, trs};

pub use cad_kernel::trimesh::TriMesh;
pub use prepare::{Prepared, Stats, prepare};

/// Version of parser and preparation, part of the cache keys of imported meshes.
pub const VERSION: &str = "cad-stl/1";

/// Up to this many faces (after merging) an imported closed mesh becomes a B-rep (`polyhedron`)
/// automatically; above it stays a mesh (`as = "solid"` forces the B-rep). Measured in
/// Bericht 21 (phase B): the kernel builds 500 faces in ~0.3 s and a Boolean with a cylinder in
/// ~0.8 s more; 1000 faces take ~0.8 s + 3 s, 2000 faces 1.6 s + 17 s.
pub const AUTO_SOLID_FACES: usize = 500;

/// Triangles of an STL file.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Stl {
    pub binary: bool,
    /// Name after `solid` (ASCII) or the header text (binary), if any.
    pub name: Option<String>,
    pub triangles: Vec<[[f32; 3]; 3]>,
    /// Problems that did not stop the import.
    pub warnings: Vec<String>,
}

/// Parses an STL file (see the crate docs for the format detection).
pub fn parse(data: &[u8]) -> Result<Stl, String> {
    if data.is_empty() {
        return Err(trs!("leere Datei", "empty file").into());
    }
    let binary_size = (data.len() >= 84).then(|| {
        let n = u32::from_le_bytes([data[80], data[81], data[82], data[83]]) as u64;
        (n, 84 + 50 * n)
    });
    if let Some((n, size)) = binary_size
        && size == data.len() as u64
    {
        return Ok(parse_binary(data, n as usize));
    }
    let text_start = data.iter().position(|b| !b.is_ascii_whitespace()).unwrap_or(0);
    let solid = data[text_start..].len() >= 5 && data[text_start..text_start + 5].eq_ignore_ascii_case(b"solid");
    if solid && std::str::from_utf8(data).is_ok_and(|t| t.contains("facet") || !t.contains('\0')) {
        return parse_ascii(std::str::from_utf8(data).unwrap_or(""));
    }
    match binary_size {
        Some((n, size)) if (data.len() as u64) < size => Err(tr!(
            "Binär-STL abgeschnitten: Kopf nennt {n} Dreiecke ({size} Bytes), Datei hat {} Bytes",
            "binary STL truncated: header names {n} triangles ({size} bytes), file has {} bytes",
            data.len()
        )),
        Some((n, size)) => {
            let mut s = parse_binary(data, n as usize);
            s.warnings.push(tr!(
                "{} Bytes nach den {n} Dreiecken ignoriert",
                "{} bytes after the {n} triangles ignored",
                data.len() as u64 - size
            ));
            Ok(s)
        }
        None if solid => {
            Err(trs!("ASCII-STL ist kein gültiger Text (UTF-8)", "ASCII STL is not valid text (UTF-8)").into())
        }
        None => Err(tr!(
            "keine STL-Datei ({} Bytes: zu kurz für Binär-STL, beginnt nicht mit „solid“)",
            "not an STL file ({} bytes: too short for binary STL, does not start with \"solid\")",
            data.len()
        )),
    }
}

fn parse_binary(data: &[u8], n: usize) -> Stl {
    let header = String::from_utf8_lossy(&data[..80]);
    let name = header.trim_matches(|c: char| c == '\0' || c.is_whitespace()).to_string();
    let f = |o: usize| f32::from_le_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]]);
    let mut triangles = Vec::with_capacity(n);
    for i in 0..n {
        let o = 84 + 50 * i + 12; // skip the normal
        let v = |k: usize| [f(o + 12 * k), f(o + 12 * k + 4), f(o + 12 * k + 8)];
        triangles.push([v(0), v(1), v(2)]);
    }
    Stl { binary: true, name: (!name.is_empty()).then_some(name), triangles, warnings: Vec::new() }
}

fn parse_ascii(text: &str) -> Result<Stl, String> {
    let mut s = Stl::default();
    let mut verts: Vec<[f32; 3]> = Vec::new();
    let mut facet_line = 0;
    let mut in_facet = false;
    for (i, line) in text.lines().enumerate() {
        let ln = i + 1;
        let mut w = line.split_whitespace();
        let Some(kw) = w.next() else { continue };
        match kw.to_ascii_lowercase().as_str() {
            "solid" => {
                if s.name.is_none() {
                    let rest = line.trim_start()[5..].trim();
                    if !rest.is_empty() {
                        s.name = Some(rest.to_string());
                    }
                }
            }
            "facet" => {
                if in_facet {
                    return Err(tr!(
                        "Zeile {ln}: `facet` ohne `endfacet` davor (Zeile {facet_line})",
                        "line {ln}: `facet` without `endfacet` before it (line {facet_line})"
                    ));
                }
                in_facet = true;
                facet_line = ln;
                verts.clear();
            }
            "vertex" => {
                if !in_facet {
                    return Err(tr!(
                        "Zeile {ln}: `vertex` außerhalb von `facet`",
                        "line {ln}: `vertex` outside `facet`"
                    ));
                }
                let mut v = [0f32; 3];
                for c in &mut v {
                    let t = w.next().ok_or_else(|| {
                        tr!("Zeile {ln}: `vertex` braucht drei Zahlen", "line {ln}: `vertex` needs three numbers")
                    })?;
                    *c = t
                        .parse::<f32>()
                        .map_err(|_| tr!("Zeile {ln}: `{t}` ist keine Zahl", "line {ln}: `{t}` is not a number"))?;
                }
                verts.push(v);
            }
            "endfacet" => {
                if !in_facet {
                    return Err(tr!("Zeile {ln}: `endfacet` ohne `facet`", "line {ln}: `endfacet` without `facet`"));
                }
                in_facet = false;
                match verts.len() {
                    3 => s.triangles.push([verts[0], verts[1], verts[2]]),
                    n => {
                        return Err(tr!(
                            "Zeile {facet_line}: Dreieck mit {n} statt 3 Ecken",
                            "line {facet_line}: triangle with {n} instead of 3 vertices"
                        ));
                    }
                }
            }
            "outer" | "endloop" | "endsolid" => {}
            other => {
                let t: String = other.chars().take(20).collect();
                return Err(tr!(
                    "Zeile {ln}: unbekanntes Wort `{t}` (kein ASCII-STL?)",
                    "line {ln}: unknown word `{t}` (not an ASCII STL?)"
                ));
            }
        }
    }
    if in_facet {
        return Err(tr!(
            "Zeile {facet_line}: `facet` ohne `endfacet` (Datei abgeschnitten?)",
            "line {facet_line}: `facet` without `endfacet` (file truncated?)"
        ));
    }
    Ok(s)
}

/// ASCII STL of triangles (tests, examples; normals from the winding).
pub fn write_ascii(name: &str, tris: &[[[f32; 3]; 3]]) -> String {
    let mut out = format!("solid {name}\n");
    for t in tris {
        let v = t.map(glam::Vec3::from);
        let n = (v[1] - v[0]).cross(v[2] - v[0]).normalize_or_zero();
        out.push_str(&format!("  facet normal {} {} {}\n    outer loop\n", n.x, n.y, n.z));
        for p in v {
            out.push_str(&format!("      vertex {} {} {}\n", p.x, p.y, p.z));
        }
        out.push_str("    endloop\n  endfacet\n");
    }
    out.push_str(&format!("endsolid {name}\n"));
    out
}

/// Binary STL of triangles (tests; `header` is padded or cut to 80 bytes).
pub fn write_binary(header: &str, tris: &[[[f32; 3]; 3]]) -> Vec<u8> {
    let mut out = Vec::with_capacity(84 + 50 * tris.len());
    let mut h = header.as_bytes().to_vec();
    h.resize(80, b' ');
    out.extend_from_slice(&h);
    out.extend_from_slice(&(tris.len() as u32).to_le_bytes());
    for t in tris {
        let v = t.map(glam::Vec3::from);
        let n = (v[1] - v[0]).cross(v[2] - v[0]).normalize_or_zero();
        for x in [n, v[0], v[1], v[2]] {
            for c in x.to_array() {
                out.extend_from_slice(&c.to_le_bytes());
            }
        }
        out.extend_from_slice(&[0, 0]);
    }
    out
}

#[cfg(test)]
mod tests;
