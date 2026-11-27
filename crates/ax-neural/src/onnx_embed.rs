//! Optional ONNX Runtime MiniLM embeddings (`neural-ort` feature).
//! Loads `~/.ax/models/all-MiniLM-L6-v2.onnx` + `tokenizer.json`. Offline; no download on open.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

const MODEL_FILE: &str = "all-MiniLM-L6-v2.onnx";
const TOKENIZER_FILE: &str = "tokenizer.json";
const MODEL_ID: &str = "onnx-minilm-l6-v2";

static LOADED: OnceLock<Mutex<Result<OnnxMiniLm, String>>> = OnceLock::new();

pub fn model_dir() -> PathBuf {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".ax").join("models")
}

pub fn model_paths() -> (PathBuf, PathBuf) {
    let dir = model_dir();
    (dir.join(MODEL_FILE), dir.join(TOKENIZER_FILE))
}

pub fn model_checksum(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read model: {e}"))?;
    Ok(format!("blake3:{}", blake3::hash(&bytes).to_hex()))
}

pub fn capability_status() -> (&'static str, &'static str, String) {
    let (model, tok) = model_paths();
    if !model.is_file() || !tok.is_file() {
        return (
            "not_configured",
            "onnx-runtime-cpu",
            format!("expected {MODEL_FILE} and {TOKENIZER_FILE} under ~/.ax/models"),
        );
    }
    match try_backend() {
        Ok(guard) => match &*guard {
            Ok(backend) => (
                "implemented",
                "onnx-runtime-cpu",
                format!(
                    "local {MODEL_ID} loaded; not production-active until admitted; checksum {}",
                    backend.checksum
                ),
            ),
            Err(e) => ("not_configured", "onnx-runtime-cpu", e.clone()),
        },
        Err(e) => ("not_configured", "onnx-runtime-cpu", e),
    }
}

fn try_backend() -> Result<std::sync::MutexGuard<'static, Result<OnnxMiniLm, String>>, String> {
    LOADED
        .get_or_init(|| Mutex::new(OnnxMiniLm::load()))
        .lock()
        .map_err(|_| "onnx session lock poisoned".into())
}

pub struct OnnxMiniLm {
    pub checksum: String,
    pub model_path: PathBuf,
    _env: ort::environment::Environment,
    tokenizer: tokenizers::Tokenizer,
    session: ort::session::Session,
    input_ids_name: String,
    attention_name: String,
    token_type_name: Option<String>,
    output_name: String,
}

impl OnnxMiniLm {
    fn load() -> Result<Self, String> {
        let (model_path, tok_path) = model_paths();
        if !model_path.is_file() {
            return Err(format!("missing model file {}", model_path.display()));
        }
        if !tok_path.is_file() {
            return Err(format!("missing tokenizer {}", tok_path.display()));
        }
        let checksum = model_checksum(&model_path)?;
        let tokenizer = tokenizers::Tokenizer::from_file(&tok_path)
            .map_err(|e| format!("tokenizer load: {e}"))?;
        let env = ort::init()
            .with_name("ax-neural")
            .with_telemetry(false)
            .build()
            .map_err(|e| format!("ort env: {e}"))?;
        let session = ort::session::Session::builder(&env)
            .map_err(|e| format!("ort session builder: {e}"))?
            .commit_from_file(&model_path)
            .map_err(|e| format!("ort load model: {e}"))?;

        let mut input_ids_name = None;
        let mut attention_name = None;
        let mut token_type_name = None;
        for input in session.inputs() {
            let n = input.name().to_ascii_lowercase();
            if n.contains("input_ids") || n == "ids" {
                input_ids_name = Some(input.name().to_string());
            } else if n.contains("attention") {
                attention_name = Some(input.name().to_string());
            } else if n.contains("token_type") || n.contains("type_ids") {
                token_type_name = Some(input.name().to_string());
            }
        }
        let input_ids_name = input_ids_name
            .or_else(|| session.inputs().first().map(|i| i.name().to_string()))
            .ok_or_else(|| "onnx model has no inputs".to_string())?;
        let attention_name = attention_name.unwrap_or_else(|| input_ids_name.clone());
        let output_name = session
            .outputs()
            .first()
            .map(|o| o.name().to_string())
            .ok_or_else(|| "onnx model has no outputs".to_string())?;

        Ok(Self {
            checksum,
            model_path,
            _env: env,
            tokenizer,
            session,
            input_ids_name,
            attention_name,
            token_type_name,
            output_name,
        })
    }

    fn embed_inner(&mut self, text: &str) -> Result<Vec<f32>, String> {
        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| format!("tokenize: {e}"))?;
        let ids: Vec<i64> = encoding.get_ids().iter().map(|v| *v as i64).collect();
        let mask: Vec<i64> = encoding
            .get_attention_mask()
            .iter()
            .map(|v| *v as i64)
            .collect();
        let seq = ids.len();
        if seq == 0 {
            return Err("empty tokenization".into());
        }
        let type_ids: Vec<i64> = encoding.get_type_ids().iter().map(|v| *v as i64).collect();

        let ids_tensor = ort::value::Tensor::from_array(([1usize, seq], ids))
            .map_err(|e| format!("ids tensor: {e}"))?;
        let mask_tensor = ort::value::Tensor::from_array(([1usize, seq], mask.clone()))
            .map_err(|e| format!("mask tensor: {e}"))?;

        let mut inputs: Vec<(String, ort::session::SessionInputValue<'_>)> = vec![
            (self.input_ids_name.clone(), ids_tensor.into()),
            (self.attention_name.clone(), mask_tensor.into()),
        ];
        let type_tensor;
        if let Some(name) = &self.token_type_name {
            let tids = if type_ids.len() == seq {
                type_ids
            } else {
                vec![0i64; seq]
            };
            type_tensor = ort::value::Tensor::from_array(([1usize, seq], tids))
                .map_err(|e| format!("type tensor: {e}"))?;
            inputs.push((name.clone(), type_tensor.into()));
        }

        let outputs = self
            .session
            .run(inputs)
            .map_err(|e| format!("ort run: {e}"))?;
        let output = outputs
            .get(self.output_name.as_str())
            .ok_or_else(|| format!("missing output {}", self.output_name))?;
        let (shape, data) = output
            .try_extract_tensor::<f32>()
            .map_err(|e| format!("extract tensor: {e}"))?;
        mean_pool_normalize(&shape, data, &mask)
    }
}

fn mean_pool_normalize(shape: &[i64], data: &[f32], mask: &[i64]) -> Result<Vec<f32>, String> {
    match shape.len() {
        2 => {
            let hidden = shape[1] as usize;
            if data.len() < hidden {
                return Err("output shorter than hidden size".into());
            }
            let mut v = data[..hidden].to_vec();
            l2_normalize(&mut v);
            Ok(v)
        }
        3 => {
            let seq = shape[1] as usize;
            let hidden = shape[2] as usize;
            if data.len() < seq * hidden {
                return Err("output shorter than seq*hidden".into());
            }
            let mut acc = vec![0.0f32; hidden];
            let mut count = 0.0f32;
            for (i, m) in mask.iter().take(seq).enumerate() {
                if *m == 0 {
                    continue;
                }
                let row = &data[i * hidden..(i + 1) * hidden];
                for (a, b) in acc.iter_mut().zip(row.iter()) {
                    *a += *b;
                }
                count += 1.0;
            }
            if count <= 0.0 {
                return Err("attention mask empty".into());
            }
            for a in &mut acc {
                *a /= count;
            }
            l2_normalize(&mut acc);
            Ok(acc)
        }
        _ => Err(format!("unexpected output rank {}", shape.len())),
    }
}

fn l2_normalize(v: &mut [f32]) {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

pub fn try_embed_text(text: &str) -> Result<Vec<f32>, String> {
    let mut guard = try_backend()?;
    match guard.as_mut() {
        Ok(backend) => backend.embed_inner(text),
        Err(e) => Err(e.clone()),
    }
}

/// Validation-only document vectors for embed-bakeoff (not written to `memories.embedding`).
#[derive(Debug)]
pub struct BakeoffDocCache {
    pub entries: Vec<(String, Vec<f32>)>,
}

impl BakeoffDocCache {
    pub fn build<I>(doc_texts: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = (String, String)>,
    {
        let mut entries = Vec::new();
        for (key, text) in doc_texts {
            let vec = try_embed_text(&text)?;
            entries.push((key, vec));
        }
        Ok(Self { entries })
    }

    pub fn entries(&self) -> &[(String, Vec<f32>)] {
        &self.entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_without_panic() {
        let (status, kind, _note) = capability_status();
        assert_eq!(kind, "onnx-runtime-cpu");
        assert!(
            status == "implemented"
                || status == "not_configured"
                || status == "not_built"
                || status == "active"
        );
    }

    #[test]
    fn onnx_embeds_when_model_present() {
        let (model, tok) = model_paths();
        if !model.is_file() || !tok.is_file() {
            eprintln!("skip: model not present at {}", model.display());
            return;
        }
        let a = try_embed_text("hash vectors similarity").expect("embed a");
        let b = try_embed_text("hash vectors similarity").expect("embed b");
        let c = try_embed_text("totally unrelated garden vegetables").expect("embed c");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert!(a.len() >= 32);
        let cos_ab: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let cos_ac: f32 = a.iter().zip(c.iter()).map(|(x, y)| x * y).sum();
        assert!(
            cos_ab > cos_ac,
            "similar texts should outrank unrelated ({cos_ab} vs {cos_ac})"
        );
    }
}
