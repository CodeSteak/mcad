//! Control protocol between the CLI (`cad ui …`) and running desktop windows, on top of
//! [`cad_ipc`]. Every app process serves the endpoint `ui-<pid>`.
//!
//! | method | body | answer |
//! | --- | --- | --- |
//! | `status` | – | `{ pid, endpoint, windows: [UiWindow] }` |
//! | `show` | `{ file, line?, module? }` (absolute path) | `{ shown: true, file, line, module }`; brings the window to the front, opens the file (or switches to it), sets the cursor / module context |
//! | `screenshot` | – | `{ width, height, mime: "image/png" }` + PNG bytes in the frame's data |
//!
//! `UiWindow` (built by the webview): `{ label, project: { name, root, main, active, files,
//! folder }, dirty: string[], cursor: { line, col, file }, selection: { line, nodes: [{idx,
//! label, line}] }, context: string|null, params: { [name]: value }, diagnostics: Diagnostic[],
//! scene: { version, complete, nodes, parts } | null, section: {...} | null }`.

use std::path::PathBuf;
use std::time::Duration;

use cad_ipc::{Error, Frame};
use serde_json::Value as Json;

/// Name prefix of the UI endpoints.
pub const PREFIX: &str = "ui-";

pub const STATUS: &str = "status";
pub const SHOW: &str = "show";
pub const SCREENSHOT: &str = "screenshot";

/// Endpoint name of the app process `pid`.
pub fn endpoint_name(pid: u32) -> String {
    format!("{PREFIX}{pid}")
}

/// Running UI processes (endpoint name, socket path); stale sockets are cleaned up.
pub fn running() -> Result<Vec<(String, PathBuf)>, Error> {
    cad_ipc::list(PREFIX)
}

/// Sends one request to the endpoint at `path` and waits up to `timeout`.
pub fn request(path: &std::path::Path, method: &str, body: Json, timeout: Duration) -> Result<Frame, Error> {
    let mut c = cad_ipc::connect_path(path)?;
    c.set_timeout(Some(timeout))?;
    c.request(method, body)
}
