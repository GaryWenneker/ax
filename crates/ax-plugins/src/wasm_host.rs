//! Minimal wasmtime host for extractor plugins.
//!
//! Guest must export:
//! - `memory`
//! - `alloc(size: i32) -> i32`
//! - `extract(ptr: i32, len: i32) -> i64`  (high 32 = out_ptr, low 32 = out_len)
//!
//! Input/output UTF-8 JSON (same shape as process plugins).
//!
//! One [`WasmRuntime`] serves a whole index run: the engine is built once and
//! each module is compiled once. Every call runs in a fresh store with a
//! memory cap and an epoch deadline, so a guest that loops forever is
//! interrupted after the plugin's timeout.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ax_types::ExtractionResult;
use wasmtime::{Config, Engine, Linker, Module, Store, StoreLimits, StoreLimitsBuilder, Trap};

use crate::host::PluginRunError;

/// Interval at which the epoch advances; deadlines are counted in these.
const TICK: Duration = Duration::from_millis(50);
/// Largest linear memory one guest may grow to.
const MAX_GUEST_MEMORY: usize = 512 * 1024 * 1024;
/// Largest output a guest may return.
const MAX_GUEST_OUTPUT: usize = 64 * 1024 * 1024;

pub struct WasmRuntime {
    engine: Engine,
    modules: Mutex<HashMap<PathBuf, Module>>,
    stop: Arc<AtomicBool>,
}

impl Drop for WasmRuntime {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn wasm_err(e: impl std::fmt::Display) -> PluginRunError {
    PluginRunError::Wasm(e.to_string())
}

impl WasmRuntime {
    pub fn new() -> Result<Self, PluginRunError> {
        let mut config = Config::new();
        config.epoch_interruption(true);
        let engine = Engine::new(&config).map_err(wasm_err)?;
        let stop = Arc::new(AtomicBool::new(false));
        let (ticker, flag) = (engine.clone(), stop.clone());
        std::thread::Builder::new()
            .name("ax-wasm-epoch".into())
            .spawn(move || {
                while !flag.load(Ordering::Relaxed) {
                    std::thread::sleep(TICK);
                    ticker.increment_epoch();
                }
            })
            .map_err(wasm_err)?;
        Ok(Self {
            engine,
            modules: Mutex::new(HashMap::new()),
            stop,
        })
    }

    /// Number of compiled modules kept for reuse.
    #[cfg(test)]
    pub fn cached_modules(&self) -> usize {
        self.modules.lock().map(|m| m.len()).unwrap_or(0)
    }

    fn module(&self, wasm_path: &Path) -> Result<Module, PluginRunError> {
        let mut modules = self
            .modules
            .lock()
            .map_err(|_| wasm_err("module cache poisoned"))?;
        if let Some(m) = modules.get(wasm_path) {
            return Ok(m.clone());
        }
        let module = Module::from_file(&self.engine, wasm_path)
            .map_err(|e| PluginRunError::Wasm(format!("load {}: {e}", wasm_path.display())))?;
        modules.insert(wasm_path.to_path_buf(), module.clone());
        Ok(module)
    }

    pub fn run(
        &self,
        wasm_path: &Path,
        path: &str,
        content: &str,
        timeout: Duration,
    ) -> Result<ExtractionResult, PluginRunError> {
        let module = self.module(wasm_path)?;
        let limits = StoreLimitsBuilder::new()
            .memory_size(MAX_GUEST_MEMORY)
            .build();
        let mut store: Store<StoreLimits> = Store::new(&self.engine, limits);
        store.limiter(|limits| limits);
        let ticks = (timeout.as_millis() / TICK.as_millis()).max(1) as u64;
        store.set_epoch_deadline(ticks);
        store.epoch_deadline_trap();

        let call_err = |e: wasmtime::Error| match e.downcast_ref::<Trap>() {
            Some(Trap::Interrupt) => PluginRunError::Wasm(format!(
                "timed out after {}s on {path}; set timeout_secs in plugin.toml to allow longer",
                timeout.as_secs()
            )),
            _ => wasm_err(e),
        };

        let linker = Linker::new(&self.engine);
        let instance = linker.instantiate(&mut store, &module).map_err(call_err)?;
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
        let in_len = i32::try_from(input_bytes.len())
            .map_err(|_| PluginRunError::Wasm("input too large for a wasm guest".into()))?;
        let in_ptr = alloc.call(&mut store, in_len).map_err(call_err)?;
        memory
            .write(&mut store, in_ptr as u32 as usize, input_bytes)
            .map_err(wasm_err)?;

        let packed = extract
            .call(&mut store, (in_ptr, in_len))
            .map_err(call_err)?;
        let out_ptr = (packed as u64 >> 32) as usize;
        let out_len = (packed as u64 & 0xffff_ffff) as usize;
        if out_len > MAX_GUEST_OUTPUT {
            return Err(PluginRunError::Wasm(format!(
                "output too large: {out_len} bytes (limit {MAX_GUEST_OUTPUT})"
            )));
        }
        let data = memory.data(&store);
        let out = out_ptr
            .checked_add(out_len)
            .and_then(|end| data.get(out_ptr..end))
            .ok_or_else(|| PluginRunError::Wasm("output outside guest memory".into()))?;

        serde_json::from_slice(out).map_err(|e| PluginRunError::Parse(e.to_string()))
    }
}
