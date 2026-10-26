//! Test-only crate. The tests live in `tests/`:
//!
//! - `openscad_compat.rs`: stage 1 of the OpenSCAD comparison. Every file in
//!   `tests/models/openscad-compat/` evaluates without errors, yields geometry, and prints
//!   the same `echo` lines as OpenSCAD (`tests/openscad-compat/reference.json`).
//! - `spec_models.rs`: the models in `tests/models/spec/` (grids, anchors, fillets, …)
//!   evaluate without errors and have the expected grids, anchors and handles.
//!
//! Stage 2 (volume, area, bounds against OpenSCAD) runs through the CLI:
//! `tests/openscad-compat/compare.py`.
