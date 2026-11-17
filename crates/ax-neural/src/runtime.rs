//! Optional CPU inference behind the `neural-runtime` feature (ADR 003).
//! Default builds leave this module compiled but empty of backends.

/// Fixed width for the CPU blake3 vector. Not the memory hash-v1 layout.
pub const CPU_DIM: usize = 64;

pub trait Backend: Send + Sync {
    fn name(&self) -> &'static str;
    fn healthy(&self) -> bool;
    fn embed(&self, text: &str) -> Result<Vec<f32>, String>;
}

/// Pure-Rust CPU backend. Deterministic, offline, not a semantic model.
#[derive(Debug, Default, Clone, Copy)]
pub struct CpuBackend;

impl Backend for CpuBackend {
    fn name(&self) -> &'static str {
        "cpu-blake3"
    }

    fn healthy(&self) -> bool {
        true
    }

    fn embed(&self, text: &str) -> Result<Vec<f32>, String> {
        let digest = blake3::hash(text.as_bytes());
        let bytes = digest.as_bytes();
        let mut out = Vec::with_capacity(CPU_DIM);
        for i in 0..CPU_DIM {
            let b = bytes[i % bytes.len()];
            out.push((b as f32 / 255.0) * 2.0 - 1.0);
        }
        Ok(out)
    }
}

/// Model id for semantic search responses: runtime backend when the query was
/// embedded there, otherwise the hash-v1 fallback label operators expect.
pub fn semantic_search_model(query_embedded_with_runtime: bool) -> &'static str {
    if query_embedded_with_runtime {
        active()
            .map(|backend| backend.name())
            .unwrap_or(ax_memory::embed::ACTIVE_MODEL)
    } else {
        ax_memory::embed::ACTIVE_MODEL
    }
}

/// Active backend when the feature is on; `None` in the default binary.
pub fn active() -> Option<CpuBackend> {
    if cfg!(feature = "neural-runtime") {
        Some(CpuBackend)
    } else {
        None
    }
}

/// Hardware backends from the Master Plan §4.4. Capability-driven: report only.
pub fn hardware_capabilities() -> serde_json::Value {
    serde_json::json!({
        "cpu": {
            "required": true,
            "available": true,
            "backend": if cfg!(feature = "neural-runtime") { "cpu-blake3" } else { "hash-v1-memory" }
        },
        "cuda": { "required": false, "available": false, "reason": "not_linked" },
        "directml": { "required": false, "available": false, "reason": "not_linked" },
        "coreml": { "required": false, "available": false, "reason": "not_linked" },
        "vulkan": { "required": false, "available": false, "reason": "not_evaluated" },
        "webgpu": { "required": false, "available": false, "reason": "not_evaluated" }
    })
}

pub fn status_json() -> serde_json::Value {
    match active() {
        Some(backend) => serde_json::json!({
            "built": true,
            "backend": backend.name(),
            "healthy": backend.healthy(),
            "dim": CPU_DIM,
            "network": false,
            "replacesHashV1": false,
            "hardware": hardware_capabilities()
        }),
        None => serde_json::json!({
            "built": false,
            "backend": null,
            "healthy": false,
            "dim": null,
            "network": false,
            "replacesHashV1": false,
            "hardware": hardware_capabilities()
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_search_model_reflects_query_embedding_not_lexical_challenger() {
        assert_eq!(
            semantic_search_model(false),
            ax_memory::embed::ACTIVE_MODEL,
        );
        if crate::runtime_built() {
            assert_eq!(semantic_search_model(true), "cpu-blake3");
            assert_ne!(semantic_search_model(true), crate::lexical::CHALLENGER_ID);
        }
    }

    #[test]
    fn runtime_cpu_embeds_without_network() {
        let Some(backend) = active() else {
            // Default build: feature off — still compile the test, skip body.
            assert!(!crate::runtime_built());
            return;
        };
        assert!(crate::runtime_built());
        assert!(backend.healthy());
        let a = backend.embed("parser").unwrap();
        let b = backend.embed("parser").unwrap();
        let c = backend.embed("other").unwrap();
        assert_eq!(a.len(), CPU_DIM);
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(status_json()["network"], false);
        assert_eq!(status_json()["replacesHashV1"], false);
    }
}
