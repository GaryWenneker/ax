//! Optional ONNX Runtime dense embeddings.
//!
//! Enable with `--features onnx` on `ax-memory` (or the CLI feature that
//! forwards it). When the feature is off, or no model file is found,
//! [`try_onnx_embed`] returns `None` and callers use feature-hash embeddings.
//!
//! Model discovery order:
//! 1. `AX_ONNX_MODEL` env var (path to `.onnx`)
//! 2. `~/.ax/models/all-MiniLM-L6-v2.onnx`
//!
//! Output vectors are projected to [`crate::embed::EMBED_DIM`] (256) so they
//! remain compatible with existing memory blobs.

/// Try ONNX dense embed when the `onnx` feature is enabled and a model exists.
pub fn try_onnx_embed(text: &str) -> Option<Vec<f32>> {
    #[cfg(feature = "onnx")]
    {
        runtime::try_embed(text)
    }
    #[cfg(not(feature = "onnx"))]
    {
        let _ = text;
        None
    }
}

/// True when a model path is configured (does not load the session).
pub fn onnx_model_configured() -> bool {
    model_path().is_some()
}

/// True when ONNX runtime successfully embeds a probe string.
pub fn onnx_available() -> bool {
    #[cfg(feature = "onnx")]
    {
        runtime::try_embed("ping").is_some()
    }
    #[cfg(not(feature = "onnx"))]
    {
        false
    }
}

fn model_path() -> Option<std::path::PathBuf> {
    if let Ok(p) = std::env::var("AX_ONNX_MODEL") {
        let path = std::path::PathBuf::from(p);
        if path.is_file() {
            return Some(path);
        }
    }
    #[cfg(feature = "onnx")]
    {
        return dirs::home_dir().and_then(|h| {
            let p = h.join(".ax").join("models").join("all-MiniLM-L6-v2.onnx");
            p.is_file().then_some(p)
        });
    }
    #[cfg(not(feature = "onnx"))]
    {
        None
    }
}

#[cfg(feature = "onnx")]
mod runtime {
    use std::sync::OnceLock;

    use ort::session::builder::GraphOptimizationLevel;
    use ort::session::Session;
    use ort::value::TensorRef;

    use super::model_path;
    use crate::embed::EMBED_DIM;

    fn project_to_embed_dim(v: &[f32]) -> Vec<f32> {
        let mut out = vec![0.0f32; EMBED_DIM];
        for (i, x) in v.iter().enumerate() {
            let idx = i % EMBED_DIM;
            let sign = if (i / EMBED_DIM) % 2 == 0 { 1.0 } else { -1.0 };
            out[idx] += sign * *x;
        }
        let norm = out.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in &mut out {
                *x /= norm;
            }
        }
        out
    }

    static SESSION: OnceLock<Option<Session>> = OnceLock::new();

    fn session() -> Option<&'static Session> {
        SESSION
            .get_or_init(|| {
                let path = model_path()?;
                tracing::info!(path = %path.display(), "loading ONNX embedding model");
                Session::builder()
                    .ok()?
                    .with_optimization_level(GraphOptimizationLevel::Level3)
                    .ok()?
                    .commit_from_file(path)
                    .ok()
            })
            .as_ref()
    }

    /// Run the ONNX session. Uses a lightweight hashed token-id stand-in when
    /// no WordPiece tokenizer is bundled; quality models still need a real
    /// tokenizer beside the `.onnx` file for production recall.
    pub fn try_embed(text: &str) -> Option<Vec<f32>> {
        let session = session()?;
        let tokens: Vec<i64> = text
            .split_whitespace()
            .take(128)
            .map(|t| {
                let mut h = 0u64;
                for b in t.bytes() {
                    h = h.wrapping_mul(31).wrapping_add(b as u64);
                }
                (h % 30_000) as i64
            })
            .collect();
        if tokens.is_empty() {
            return None;
        }
        let len = tokens.len();
        let attention = vec![1i64; len];
        let type_ids = vec![0i64; len];

        let ids = TensorRef::from_array_view(([1usize, len], tokens.as_slice())).ok()?;
        let mask = TensorRef::from_array_view(([1usize, len], attention.as_slice())).ok()?;
        let types = TensorRef::from_array_view(([1usize, len], type_ids.as_slice())).ok()?;

        let outputs = session
            .run(ort::inputs![
                "input_ids" => ids,
                "attention_mask" => mask,
                "token_type_ids" => types,
            ])
            .ok()?;

        let (_shape, data) = outputs[0].try_extract_tensor::<f32>().ok()?;
        let dim = data.len() / len.max(1);
        if dim == 0 {
            return None;
        }
        let mut pooled = vec![0.0f32; dim];
        for i in 0..len {
            for d in 0..dim {
                pooled[d] += data[i * dim + d];
            }
        }
        for v in &mut pooled {
            *v /= len as f32;
        }
        Some(project_to_embed_dim(&pooled))
    }
}
