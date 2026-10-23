//! wasm-bindgen entry for the browser worker (`web/src/engine/worker.ts`).
//!
//! [`Session`] wraps `cad_session::Session` over `kernel-smlib` (feature `wasm`): smlib.wasm
//! must have been loaded with `initSmlib(createSmlib)` from the kernel's JS bridge before the
//! first `new Session()`.
//!
//! * `post(msg)`: one `ToEngine` message (plain JS object) → array of `FromEngine` messages.
//! * `step(budget_ms)`: kernel work for about `budget_ms` milliseconds (clock:
//!   `performance.now()`) → array of messages. Call while `busy()`.
//!
//! Messages are converted with `serde-wasm-bindgen` (no JSON); `Meshes` arrays and export
//! bytes are `Float32Array`/`Uint32Array`/`Uint8Array` with their own buffers, ready to be
//! transferred to the main thread. Optional fields are `null`, maps are plain objects.
//!
//! Stack: `.cargo/config.toml` links with a 16 MiB stack (Wasm has no guard page); the
//! language evaluation stops with "recursion too deep" after 12 MiB.
#![cfg(target_arch = "wasm32")]

use std::rc::Rc;

use cad_session::protocol::{FromEngine, MeshData, ToEngine};
use cad_session::{PROTOCOL_VERSION, SessionOptions};
use js_sys::{Array, Float32Array, Object, Reflect, Uint8Array, Uint32Array};
use kernel_smlib::SmlibKernel;
use serde::Serialize;
use wasm_bindgen::prelude::*;

/// Native stack the language evaluation may use (below the 16 MiB linker stack).
const MAX_STACK_BYTES: usize = 12 << 20;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = performance, js_name = now)]
    fn performance_now() -> f64;
}

#[wasm_bindgen]
pub struct Session {
    inner: cad_session::Session<SmlibKernel>,
}

fn serializer() -> serde_wasm_bindgen::Serializer {
    serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true).serialize_missing_as_null(true)
}

fn set(o: &Object, k: &str, v: &JsValue) {
    let _ = Reflect::set(o, &JsValue::from_str(k), v);
}

fn mesh_js(m: &MeshData) -> Result<JsValue, JsValue> {
    let o = Object::new();
    set(&o, "node", &JsValue::from(m.node));
    set(&o, "nodes", &m.nodes.iter().map(|&n| JsValue::from(n)).collect::<Array>());
    set(&o, "key", &JsValue::from_str(&m.key));
    set(&o, "coarse", &JsValue::from_bool(m.coarse));
    set(&o, "tol", &JsValue::from_f64(m.tol));
    set(&o, "positions", &Float32Array::from(&m.positions[..]));
    set(&o, "normals", &Float32Array::from(&m.normals[..]));
    set(&o, "indices", &Uint32Array::from(&m.indices[..]));
    set(&o, "tri_tags", &Uint32Array::from(&m.tri_tags[..]));
    set(&o, "tri_faces", &Uint32Array::from(&m.tri_faces[..]));
    set(&o, "edge_segments", &Float32Array::from(&m.edge_segments[..]));
    set(&o, "edge_ids", &Uint32Array::from(&m.edge_ids[..]));
    Ok(o.into())
}

fn to_js(msgs: Vec<FromEngine>) -> Result<Array, JsValue> {
    let out = Array::new();
    for m in msgs {
        let v = match &m {
            FromEngine::Meshes { version, items } => {
                let o = Object::new();
                set(&o, "type", &JsValue::from_str("Meshes"));
                set(&o, "version", &JsValue::from_f64(*version as f64));
                let arr = Array::new();
                for it in items {
                    arr.push(&mesh_js(it)?);
                }
                set(&o, "items", &arr);
                o.into()
            }
            FromEngine::ExportResult { format, bytes, triangles, parts, zip } => {
                let o = Object::new();
                set(&o, "type", &JsValue::from_str("ExportResult"));
                set(&o, "format", &format.serialize(&serializer())?);
                set(&o, "bytes", &Uint8Array::from(&bytes[..]));
                set(&o, "triangles", &JsValue::from(*triangles));
                if let Some(p) = parts {
                    set(&o, "parts", &p.serialize(&serializer())?);
                }
                if let Some(z) = zip {
                    set(&o, "zip", &JsValue::from_bool(*z));
                }
                o.into()
            }
            other => other.serialize(&serializer())?,
        };
        out.push(&v);
    }
    Ok(out)
}

#[wasm_bindgen]
impl Session {
    /// Creates a session. smlib.wasm must be initialised (`initSmlib`).
    #[wasm_bindgen(constructor)]
    pub fn new() -> Session {
        let mut opts = SessionOptions::default();
        opts.eval.max_stack_bytes = MAX_STACK_BYTES;
        let clock: cad_session::SharedClock = Rc::new(performance_now);
        Session { inner: cad_session::Session::new(SmlibKernel::new(), opts, clock) }
    }

    /// Handles one `ToEngine` message; returns `FromEngine[]`.
    pub fn post(&mut self, msg: JsValue) -> Result<Array, JsValue> {
        let m: ToEngine =
            serde_wasm_bindgen::from_value(msg).map_err(|e| JsValue::from_str(&format!("ungültige Nachricht: {e}")))?;
        to_js(self.inner.post(m))
    }

    /// Kernel work for about `budget_ms` (at least one unit of progress); returns `FromEngine[]`.
    pub fn step(&mut self, budget_ms: f64) -> Result<Array, JsValue> {
        to_js(self.inner.step(Some(budget_ms)))
    }

    /// Work pending: call `step`.
    pub fn busy(&self) -> bool {
        self.inner.busy()
    }

    /// Protocol version (`PROTOCOL_VERSION` in protocol.ts).
    #[wasm_bindgen(js_name = protocolVersion)]
    pub fn protocol_version() -> u32 {
        PROTOCOL_VERSION
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}
