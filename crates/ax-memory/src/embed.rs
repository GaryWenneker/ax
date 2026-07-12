//! Local, dependency-free text embeddings for hybrid recall.
//!
//! Uses feature hashing over word tokens and character trigrams into a fixed
//! 256-dim vector (L2-normalized). This is not a neural embedding — it gives
//! deterministic, typo- and morphology-tolerant similarity that fuses with
//! FTS5 rankings via RRF. A real model can replace `embed_text` later without
//! changing the storage format.

pub const EMBED_DIM: usize = 256;

/// FNV-1a — stable across platforms and runs (unlike `DefaultHasher`).
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn add_feature(vec: &mut [f32; EMBED_DIM], feature: &[u8], weight: f32) {
    let h = fnv1a(feature);
    let idx = (h % EMBED_DIM as u64) as usize;
    // Second hash bit decides sign, which reduces collision bias.
    let sign = if (h >> 32) & 1 == 0 { 1.0 } else { -1.0 };
    vec[idx] += sign * weight;
}

pub fn embed_text(text: &str) -> Vec<f32> {
    let mut vec = [0.0f32; EMBED_DIM];
    let lower = text.to_lowercase();

    for token in lower.split(|c: char| !c.is_alphanumeric() && c != '_') {
        if token.len() < 2 {
            continue;
        }
        add_feature(&mut vec, token.as_bytes(), 2.0);

        let chars: Vec<char> = token.chars().collect();
        if chars.len() >= 3 {
            for window in chars.windows(3) {
                let tri: String = window.iter().collect();
                add_feature(&mut vec, tri.as_bytes(), 1.0);
            }
        }
    }

    let norm = vec.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm > 0.0 {
        for v in vec.iter_mut() {
            *v /= norm;
        }
    }
    vec.to_vec()
}

pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    // Vectors are already L2-normalized, so the dot product is the cosine.
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

pub fn embedding_to_blob(embedding: &[f32]) -> Vec<u8> {
    embedding.iter().flat_map(|v| v.to_le_bytes()).collect()
}

pub fn blob_to_embedding(blob: &[u8]) -> Option<Vec<f32>> {
    if blob.is_empty() || blob.len() % 4 != 0 {
        return None;
    }
    Some(
        blob.chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect(),
    )
}

/// Reciprocal Rank Fusion constant (standard k=60).
const RRF_K: f64 = 60.0;

pub fn rrf_score(rank: usize) -> f64 {
    1.0 / (RRF_K + rank as f64 + 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_texts_have_cosine_one() {
        let a = embed_text("reinstall the ax cli after building");
        let b = embed_text("reinstall the ax cli after building");
        assert!((cosine(&a, &b) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn related_texts_beat_unrelated() {
        let target = embed_text("tokenizer counts BPE tokens for cost estimation");
        let related = embed_text("token counting with a BPE tokeniser for costs");
        let unrelated = embed_text("git branch cleanup after merge");
        assert!(cosine(&target, &related) > cosine(&target, &unrelated));
    }

    #[test]
    fn typos_still_score_high() {
        let a = embed_text("reinstall binary");
        let b = embed_text("reinstal binry");
        assert!(cosine(&a, &b) > 0.4, "got {}", cosine(&a, &b));
    }

    #[test]
    fn blob_roundtrip() {
        let e = embed_text("some text");
        let blob = embedding_to_blob(&e);
        let back = blob_to_embedding(&blob).unwrap();
        assert_eq!(e, back);
    }
}
