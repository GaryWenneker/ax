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
//! Tokenizer discovery (WordPiece vocab from HuggingFace `tokenizer.json`):
//! 1. `AX_ONNX_TOKENIZER` env var
//! 2. `tokenizer.json` beside the model file
//!
//! Output vectors are projected to [`crate::embed::EMBED_DIM`] (256) so they
//! remain compatible with existing memory blobs.

use std::path::{Path, PathBuf};

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

/// True when a tokenizer.json (or `AX_ONNX_TOKENIZER`) is present beside the model.
pub fn onnx_tokenizer_configured() -> bool {
    model_path()
        .and_then(|m| tokenizer_path(&m))
        .is_some()
}

/// Resolved model path when present (for status UI / diagnostics).
pub fn onnx_model_path() -> Option<PathBuf> {
    model_path()
}

/// Resolved tokenizer path when present.
pub fn onnx_tokenizer_path() -> Option<PathBuf> {
    model_path().and_then(|m| tokenizer_path(&m))
}

/// Whether this binary was built with the `onnx` Cargo feature.
pub fn onnx_feature_enabled() -> bool {
    cfg!(feature = "onnx")
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

fn model_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("AX_ONNX_MODEL") {
        let path = PathBuf::from(p);
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

fn tokenizer_path(model: &Path) -> Option<PathBuf> {
    if let Ok(p) = std::env::var("AX_ONNX_TOKENIZER") {
        let path = PathBuf::from(p);
        if path.is_file() {
            return Some(path);
        }
    }
    let beside = model.with_file_name("tokenizer.json");
    beside.is_file().then_some(beside)
}

#[cfg(feature = "onnx")]
mod runtime {
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::OnceLock;

    use ort::session::builder::GraphOptimizationLevel;
    use ort::session::Session;
    use ort::value::TensorRef;

    use super::{model_path, tokenizer_path};
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
    static VOCAB: OnceLock<Option<WordPieceVocab>> = OnceLock::new();

    struct WordPieceVocab {
        token_to_id: HashMap<String, i64>,
        unk_id: i64,
        cls_id: i64,
        sep_id: i64,
    }

    impl WordPieceVocab {
        fn load(path: &PathBuf) -> Option<Self> {
            let raw = std::fs::read_to_string(path).ok()?;
            let json: serde_json::Value = serde_json::from_str(&raw).ok()?;
            let vocab_obj = json
                .pointer("/model/vocab")
                .or_else(|| json.get("vocab"))?
                .as_object()?;
            let mut token_to_id = HashMap::with_capacity(vocab_obj.len());
            for (tok, id) in vocab_obj {
                let id = id.as_i64().or_else(|| id.as_u64().map(|u| u as i64))?;
                token_to_id.insert(tok.clone(), id);
            }
            let unk_id = *token_to_id.get("[UNK]").or_else(|| token_to_id.get("<unk>"))?;
            let cls_id = *token_to_id
                .get("[CLS]")
                .or_else(|| token_to_id.get("<s>"))
                .unwrap_or(&0);
            let sep_id = *token_to_id
                .get("[SEP]")
                .or_else(|| token_to_id.get("</s>"))
                .unwrap_or(&0);
            Some(Self {
                token_to_id,
                unk_id,
                cls_id,
                sep_id,
            })
        }

        fn encode(&self, text: &str, max_len: usize) -> Vec<i64> {
            let mut ids = Vec::with_capacity(max_len.min(128));
            ids.push(self.cls_id);
            for word in text.split_whitespace() {
                if ids.len() + 1 >= max_len {
                    break;
                }
                let lower = word.to_lowercase();
                for piece in self.wordpiece(&lower) {
                    if ids.len() + 1 >= max_len {
                        break;
                    }
                    ids.push(piece);
                }
            }
            if ids.len() < max_len {
                ids.push(self.sep_id);
            }
            ids
        }

        fn wordpiece(&self, word: &str) -> Vec<i64> {
            if word.is_empty() {
                return vec![];
            }
            if let Some(&id) = self.token_to_id.get(word) {
                return vec![id];
            }
            let chars: Vec<char> = word.chars().collect();
            let mut start = 0usize;
            let mut out = Vec::new();
            while start < chars.len() {
                let mut end = chars.len();
                let mut matched: Option<i64> = None;
                while start < end {
                    let slice: String = chars[start..end].iter().collect();
                    let candidate = if start == 0 {
                        slice
                    } else {
                        format!("##{slice}")
                    };
                    if let Some(&id) = self.token_to_id.get(&candidate) {
                        matched = Some(id);
                        break;
                    }
                    end -= 1;
                }
                match matched {
                    Some(id) => {
                        out.push(id);
                        start = end;
                    }
                    None => {
                        out.push(self.unk_id);
                        start += 1;
                    }
                }
            }
            if out.is_empty() {
                vec![self.unk_id]
            } else {
                out
            }
        }
    }

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

    fn vocab() -> Option<&'static WordPieceVocab> {
        VOCAB
            .get_or_init(|| {
                let model = model_path()?;
                let path = tokenizer_path(&model)?;
                tracing::info!(path = %path.display(), "loading ONNX tokenizer.json");
                WordPieceVocab::load(&path)
            })
            .as_ref()
    }

    fn hashed_token_ids(text: &str) -> Vec<i64> {
        text.split_whitespace()
            .take(126)
            .map(|t| {
                let mut h = 0u64;
                for b in t.bytes() {
                    h = h.wrapping_mul(31).wrapping_add(b as u64);
                }
                (h % 30_000) as i64
            })
            .collect()
    }

    /// Run the ONNX session. Prefer WordPiece ids from `tokenizer.json`; fall
    /// back to hashed token-id stand-in when no vocab is available.
    pub fn try_embed(text: &str) -> Option<Vec<f32>> {
        let session = session()?;
        let tokens: Vec<i64> = if let Some(v) = vocab() {
            v.encode(text, 128)
        } else {
            let mut ids = hashed_token_ids(text);
            if ids.is_empty() {
                return None;
            }
            ids.insert(0, 101);
            ids.push(102);
            ids
        };
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
