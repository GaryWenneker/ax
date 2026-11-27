//! Required Neural Intelligence. It stays on when ax.json omits it or sets enabled to false.
//! Hash-v1 remains the memory embedder. The default build does not link an ML runtime;
//! `--features neural-runtime` enables the pure-Rust CPU backend and lexical bake-off
//! challenger (ADR 002 / 003).

pub mod agent_eval;
pub mod bakeoff;
pub mod benchmark_abc;
pub mod code_aware;
pub mod code_discovery;
pub mod config;
pub mod dispatch;
pub mod embed_bakeoff;
pub mod graph;
pub mod learn_validate;
pub mod learning;
pub mod lexical;
mod metrics;
#[cfg(feature = "neural-ort")]
pub mod onnx_embed;
pub mod path_validate;
pub mod predict;
pub mod query_prior;
pub mod rank;
pub mod recovery;
pub mod rss;
pub mod runtime;
pub mod share;
pub mod speed;
pub mod status;
pub mod store;

pub use config::{load, NeuralConfig};
pub use rank::{plan as plan_blocks, RankClass, RankItem, RankNote};

pub const SCHEMA_SQL: &str = include_str!("../schema.sql");

/// The coding memory fixture, built in so the bake-off and evaluation run in any project.
pub const CODING_FIXTURE: &str = include_str!("../../../benchmarks/memory/coding.json");

/// Semantic blend weight beside the locked fusion mix. Stays zero until a
/// held-out run versions a new weight set.
pub const SEMANTIC_WEIGHT: f64 = 0.0;

/// Frozen production gate: operators read this on `ax neural status` without opening source.
pub const PRODUCTION_DEFAULTS_UNCHANGED: bool = true;

pub fn blend_semantic(fused: f64, semantic: f64) -> f64 {
    fused + SEMANTIC_WEIGHT * semantic
}

pub fn runtime_built() -> bool {
    cfg!(feature = "neural-runtime")
}
