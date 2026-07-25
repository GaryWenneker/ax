//! Minimal wasmtime host for extractor plugins.
//!
//! Guest must export:
//! - `memory`
//! - `alloc(size: i32) -> i32`
//! - `extract(ptr: i32, len: i32) -> i64`  (high 32 = out_ptr, low 32 = out_len)
//!
//! Input/output UTF-8 JSON (same shape as process plugins).

use std::path::Path;

use ax_types::ExtractionResult;
use wasmtime::{Engine, Linker, Module, Store};

use crate::host::PluginRunError;

pub fn run_wasm(
    wasm_path: &Path,
    path: &str,
    content: &str,
) -> Result<ExtractionResult, PluginRunError> {
    let engine = Engine::default();
    let module = Module::from_file(&engine, wasm_path)
        .map_err(|e| PluginRunError::Wasm(format!("load {}: {e}", wasm_path.display())))?;
    let mut linker = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker
        .instantiate(&mut store, &module)
        .map_err(|e| PluginRunError::Wasm(e.to_string()))?;

    let memory = instance
        .get_memory(&mut store, "memory")
        .ok_or_else(|| PluginRunError::Wasm("missing memory export".into()))?;
    let alloc = instance
        .get_typed_func::<i32, i32>(&mut store, "alloc")
        .map_err(|e| PluginRunError::Wasm(format!("missing alloc: {e}")))?;
    let extract = instance
        .get_typed_func::<(i32, i32), i64>(&mut store, "extract")
        .map_err(|e| PluginRunError::Wasm(format!("missing extract: {e}")))?;

    let input = serde_json::json!({ "path": path, "content": content }).to_string();
    let input_bytes = input.as_bytes();
    let in_ptr = alloc
        .call(&mut store, input_bytes.len() as i32)
        .map_err(|e| PluginRunError::Wasm(e.to_string()))?;
    memory
        .write(&mut store, in_ptr as usize, input_bytes)
        .map_err(|e| PluginRunError::Wasm(e.to_string()))?;

    let packed = extract
        .call(&mut store, (in_ptr, input_bytes.len() as i32))
        .map_err(|e| PluginRunError::Wasm(e.to_string()))?;
    let out_ptr = (packed >> 32) as i32 as usize;
    let out_len = (packed & 0xffff_ffff) as usize;
    let mut out = vec![0u8; out_len];
    memory
        .read(&store, out_ptr, &mut out)
        .map_err(|e| PluginRunError::Wasm(e.to_string()))?;

    serde_json::from_slice(&out).map_err(|e| PluginRunError::Parse(e.to_string()))
}
