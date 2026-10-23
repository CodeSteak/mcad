//! Renders an STL (ASCII or binary) with the CLI's software rasterizer, optionally with edge
//! polylines from a JSON file (`[[[x,y,z], …], …]`). Used by the STEP report to show geometry
//! read back by another kernel (tests/step/verify.py --stl-dir).
//!
//!   cargo run --release -p cad-cli --example render_stl -- in.stl out.png [edges.json] [view]

use cad_kernel::Mesh;
use cad_render::{Item, Polyline, RenderOptions, Scene, View, render};
use glam::DVec3;

fn read_stl(bytes: &[u8]) -> Mesh {
    let mut m = Mesh::default();
    let mut push = |tri: [[f32; 3]; 3]| {
        let base = (m.positions.len() / 3) as u32;
        let a = DVec3::from(tri[0].map(f64::from));
        let n = (DVec3::from(tri[1].map(f64::from)) - a).cross(DVec3::from(tri[2].map(f64::from)) - a).normalize_or_zero();
        for p in tri {
            m.positions.extend_from_slice(&p);
            m.normals.extend_from_slice(&[n.x as f32, n.y as f32, n.z as f32]);
        }
        m.indices.extend_from_slice(&[base, base + 1, base + 2]);
        m.tri_tags.push(0);
        m.tri_faces.push(0);
    };
    let ascii = bytes.starts_with(b"solid") && std::str::from_utf8(bytes).is_ok_and(|s| s.contains("facet"));
    if ascii {
        let s = std::str::from_utf8(bytes).unwrap();
        let mut v = Vec::new();
        for line in s.lines() {
            let mut it = line.split_whitespace();
            if it.next() == Some("vertex") {
                let c: Vec<f32> = it.filter_map(|x| x.parse().ok()).collect();
                v.push([c[0], c[1], c[2]]);
                if v.len() == 3 {
                    push([v[0], v[1], v[2]]);
                    v.clear();
                }
            }
        }
    } else {
        let n = u32::from_le_bytes(bytes[80..84].try_into().unwrap()) as usize;
        for i in 0..n {
            let o = 84 + 50 * i + 12;
            let f = |k: usize| f32::from_le_bytes(bytes[o + 4 * k..o + 4 * k + 4].try_into().unwrap());
            push([[f(0), f(1), f(2)], [f(3), f(4), f(5)], [f(6), f(7), f(8)]]);
        }
    }
    m
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: render_stl in.stl out.png [edges.json] [view]");
        std::process::exit(2);
    }
    let mesh = read_stl(&std::fs::read(&args[1]).expect("read STL"));
    let mut scene = Scene { items: vec![Item::new(&mesh, [0.55, 0.68, 0.82, 1.0]).with_edges(None)], lines: vec![] };
    if let Some(p) = args.get(3).filter(|p| !p.is_empty()) {
        let txt = std::fs::read_to_string(p).expect("read edges");
        let edges: Vec<Vec<[f64; 3]>> = parse_edges(&txt);
        for e in edges {
            scene.lines.push(Polyline {
                points: e.into_iter().map(DVec3::from).collect(),
                color: [0.12, 0.12, 0.16, 1.0],
                width: 1.2,
                on_top: false,
            });
        }
    }
    let view = args.get(4).and_then(|v| View::from_name(v)).unwrap_or(View::Iso);
    let img = render(&scene, &RenderOptions { width: 640, height: 480, view, ..Default::default() });
    img.write_png(&args[2]).expect("write PNG");
}

/// Minimal parser for `[[[x,y,z],...],...]` (numbers only).
fn parse_edges(s: &str) -> Vec<Vec<[f64; 3]>> {
    let mut out = Vec::new();
    let mut depth = 0;
    let mut num = String::new();
    let mut pt: Vec<f64> = Vec::new();
    let mut poly: Vec<[f64; 3]> = Vec::new();
    for ch in s.chars() {
        match ch {
            '[' => depth += 1,
            ']' | ',' => {
                if !num.is_empty() {
                    pt.push(num.parse().unwrap_or(0.0));
                    num.clear();
                }
                if ch == ']' {
                    if depth == 3 && pt.len() == 3 {
                        poly.push([pt[0], pt[1], pt[2]]);
                    }
                    if depth == 3 {
                        pt.clear();
                    }
                    if depth == 2 {
                        out.push(std::mem::take(&mut poly));
                    }
                    depth -= 1;
                }
            }
            c if c.is_ascii_digit() || matches!(c, '-' | '+' | '.' | 'e' | 'E') => num.push(c),
            _ => {}
        }
    }
    out
}
