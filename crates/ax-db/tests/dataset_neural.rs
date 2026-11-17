//! Neural optional field on replication Dataset (Master Plan §9 / SPEC-FULL F4).

use ax_db::replication::{Dataset, FileManifest};
use std::collections::BTreeMap;

fn minimal_without_neural() -> &'static str {
    r#"{
      "origin": "primary",
      "graph": { "digest": "abc", "bytes": 1, "chunks": ["c0"] },
      "logs": {},
      "nodes": 0,
      "edges": 0,
      "files": 0,
      "source_bytes": 0,
      "agent_sessions": 0
    }"#
}

#[test]
fn dataset_neural_field_is_optional() {
    let old: Dataset = serde_json::from_str(minimal_without_neural()).expect("old peer JSON");
    assert!(old.neural.is_none());

    let with = Dataset {
        origin: "primary".into(),
        graph: FileManifest {
            digest: "abc".into(),
            bytes: 1,
            chunks: vec!["c0".into()],
        },
        history: None,
        history_rows: 0,
        logs: BTreeMap::new(),
        nodes: 0,
        edges: 0,
        files: 0,
        source_bytes: 0,
        agent_sessions: 0,
        neural: Some(FileManifest {
            digest: "neural1".into(),
            bytes: 2,
            chunks: vec!["n0".into()],
        }),
    };
    let text = serde_json::to_string(&with).unwrap();
    assert!(text.contains("neural"));
    let round: Dataset = serde_json::from_str(&text).unwrap();
    assert_eq!(round.neural.as_ref().unwrap().digest, "neural1");
}
