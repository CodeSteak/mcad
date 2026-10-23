//! Binary wire format of engine answers (engine process → app → webview).
//!
//! One batch of `FromEngine` messages (the answers of one post/step round, like the worker's
//! `out` envelope) is one [`cad_ipc::Frame`]: event `out`, body `{ msgs, busy }`. The messages
//! are their serde JSON form, except that the typed arrays of `Meshes` items (`positions`,
//! `normals`, `indices`, `tri_tags`, `tri_faces`, `edge_segments`, `edge_ids`) and the bytes of
//! `ExportResult` are replaced by `{ "$b": [offset, length, "f32"|"u32"|"u8"] }` (offset in
//! bytes into the frame's data part, length in elements). Every blob starts 4-byte aligned, the
//! data part itself too (header padding), so the webview creates `Float32Array`/`Uint32Array`
//! views on the received `ArrayBuffer` without copying. No JSON number arrays for meshes.

use cad_ipc::Frame;
use cad_session::protocol::{FromEngine, MeshData};
use serde_json::{Value as Json, json};

/// Event name of an engine batch.
pub const OUT: &str = "out";

struct Blobs(Vec<u8>);

impl Blobs {
    fn align(&mut self) {
        while self.0.len() % 4 != 0 {
            self.0.push(0);
        }
    }
    fn f32(&mut self, v: &[f32]) -> Json {
        self.align();
        let off = self.0.len();
        self.0.reserve(v.len() * 4);
        for x in v {
            self.0.extend_from_slice(&x.to_le_bytes());
        }
        json!({ "$b": [off, v.len(), "f32"] })
    }
    fn u32(&mut self, v: &[u32]) -> Json {
        self.align();
        let off = self.0.len();
        self.0.reserve(v.len() * 4);
        for x in v {
            self.0.extend_from_slice(&x.to_le_bytes());
        }
        json!({ "$b": [off, v.len(), "u32"] })
    }
    fn u8(&mut self, v: &[u8]) -> Json {
        self.align();
        let off = self.0.len();
        self.0.extend_from_slice(v);
        json!({ "$b": [off, v.len(), "u8"] })
    }
}

fn mesh_json(m: &MeshData, b: &mut Blobs) -> Json {
    json!({
        "node": m.node,
        "nodes": m.nodes,
        "key": m.key,
        "coarse": m.coarse,
        "tol": m.tol,
        "positions": b.f32(&m.positions),
        "normals": b.f32(&m.normals),
        "indices": b.u32(&m.indices),
        "tri_tags": b.u32(&m.tri_tags),
        "tri_faces": b.u32(&m.tri_faces),
        "edge_segments": b.f32(&m.edge_segments),
        "edge_ids": b.u32(&m.edge_ids),
    })
}

/// Encodes one batch.
pub fn encode_batch(msgs: &[FromEngine], busy: bool) -> Frame {
    let mut b = Blobs(Vec::new());
    let out: Vec<Json> = msgs
        .iter()
        .map(|m| match m {
            FromEngine::Meshes { version, items } => {
                let items: Vec<Json> = items.iter().map(|it| mesh_json(it, &mut b)).collect();
                json!({ "type": "Meshes", "version": version, "items": items })
            }
            FromEngine::ExportResult { format, bytes, triangles, parts, zip } => {
                let mut o = json!({ "type": "ExportResult", "format": format, "bytes": b.u8(bytes), "triangles": triangles });
                if let Some(p) = parts {
                    o["parts"] = json!(p);
                }
                if let Some(z) = zip {
                    o["zip"] = json!(z);
                }
                o
            }
            other => serde_json::to_value(other).unwrap_or_else(|e| json!({ "type": "Error", "message": e.to_string(), "version": null, "node": null })),
        })
        .collect();
    Frame::event(OUT, json!({ "msgs": out, "busy": busy })).with_data(b.0)
}

/// Decodes a batch with the blobs as JSON number arrays (tests, debugging).
pub fn decode_batch(f: &Frame) -> (Vec<Json>, bool) {
    fn fix(v: &mut Json, data: &[u8]) {
        match v {
            Json::Object(o) => {
                if let Some(Json::Array(d)) = o.get("$b")
                    && d.len() == 3
                {
                    let off = d[0].as_u64().unwrap_or(0) as usize;
                    let len = d[1].as_u64().unwrap_or(0) as usize;
                    let kind = d[2].as_str().unwrap_or("");
                    let arr: Vec<Json> = match kind {
                        "f32" => (0..len)
                            .map(|i| json!(f32::from_le_bytes(data[off + 4 * i..off + 4 * i + 4].try_into().unwrap())))
                            .collect(),
                        "u32" => (0..len)
                            .map(|i| json!(u32::from_le_bytes(data[off + 4 * i..off + 4 * i + 4].try_into().unwrap())))
                            .collect(),
                        _ => data[off..off + len].iter().map(|x| json!(x)).collect(),
                    };
                    *v = Json::Array(arr);
                    return;
                }
                for x in o.values_mut() {
                    fix(x, data);
                }
            }
            Json::Array(a) => {
                for x in a {
                    fix(x, data);
                }
            }
            _ => {}
        }
    }
    let mut msgs = f.header.body["msgs"].clone();
    fix(&mut msgs, &f.data);
    let busy = f.header.body["busy"].as_bool().unwrap_or(false);
    (msgs.as_array().cloned().unwrap_or_default(), busy)
}

/// Encodes a frame to bytes (what the webview receives).
pub fn frame_bytes(f: &Frame) -> Vec<u8> {
    let mut v = Vec::with_capacity(f.data.len() + 256);
    // Writing into a Vec cannot fail except for oversized frames.
    let _ = cad_ipc::write_frame(&mut v, f);
    v
}
