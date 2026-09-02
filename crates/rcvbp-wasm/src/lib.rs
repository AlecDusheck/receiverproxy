//! `rcvbp` and `wall` for the browser (docs/ui.md section 3).
//! Each export throws a JavaScript `Error` carrying the anyhow chain.

// wasm-bindgen's generated glue is unsafe and the lint cannot see past the macro.
#![allow(unsafe_code)]

pub mod api;

use serde::Serialize;
use wasm_bindgen::prelude::*;

fn js<T: Serialize>(r: anyhow::Result<T>) -> Result<JsValue, JsError> {
    let v = r.map_err(|e| JsError::new(&format!("{e:#}")))?;
    let s = serde_wasm_bindgen::Serializer::new().serialize_missing_as_null(true);
    v.serialize(&s).map_err(|e| JsError::new(&e.to_string()))
}

/// `Generated` from a panel spec's TOML text in `format` (one of `formats()`);
/// chip libraries come from the embedded set.
#[wasm_bindgen]
pub fn generate(spec_toml: &str, format: &str) -> Result<JsValue, JsError> {
    js(api::generate(spec_toml, format))
}

/// `Imported` from a vendor file's bytes.
///
/// `format` names one of `formats()` or is left out to detect it from the
/// bytes. The glue exports it as `_import` (a JavaScript function cannot be
/// named `import`); web/src/lib/wasm.ts maps it back.
#[wasm_bindgen]
pub fn import(bytes: &[u8], format: Option<String>) -> Result<JsValue, JsError> {
    js(api::import(bytes, format.as_deref()))
}

/// `Entry[]`: the embedded panel specs with their `[meta]`, module, chip and formats.
#[wasm_bindgen]
pub fn gallery() -> Result<JsValue, JsError> {
    js(api::gallery())
}

/// `Format[]`: the codec registry.
#[wasm_bindgen]
pub fn formats() -> Result<JsValue, JsError> {
    js(Ok(api::formats()))
}

/// `Inspection` of a `.rcvbp` file's bytes, compressed or legacy.
#[wasm_bindgen]
pub fn inspect(rcvbp: &[u8]) -> Result<JsValue, JsError> {
    js(api::inspect(rcvbp))
}

/// `Diff` of two `.rcvbp` files, record by record.
#[wasm_bindgen]
pub fn diff(a: &[u8], b: &[u8]) -> Result<JsValue, JsError> {
    js(api::diff(a, b))
}

/// `Libraries`: the embedded chip libraries and panel specs.
#[wasm_bindgen]
pub fn libraries() -> Result<JsValue, JsError> {
    js(Ok(api::libraries()))
}

/// `"ok"` or the layout error text.
#[wasm_bindgen]
pub fn validate_layout(json: &str) -> Result<String, JsError> {
    api::validate_layout(json).map_err(|e| JsError::new(&format!("{e:#}")))
}

/// One receiver per panel, as `rxp card layout-example` prints it.
#[wasm_bindgen]
pub fn layout_example(cols: u32, rows: u32, w: u32, h: u32) -> Result<String, JsError> {
    api::layout_example(cols, rows, w, h).map_err(|e| JsError::new(&format!("{e:#}")))
}
