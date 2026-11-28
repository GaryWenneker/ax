//! A policy index that runs while a rule is saved (the background watcher
//! in the Command Center does this) must not prune the new rule or put an
//! older body back. Both read and write the same files and rows, so they
//! take turns.

use std::path::Path;

use ax_policy::store::PolicyStore;
use ax_policy::{get_rule, index_policy, parse_rule_file};

fn rule(id: &str, body: &str) -> (ax_policy::types::RuleFrontmatter, String) {
    let raw = format!("---\nid: {id}\nlevel: WARNING\nalwaysApply: true\n---\n\n{body}");
    let doc = parse_rule_file(Path::new("x.mdc"), &raw).unwrap();
    (doc.frontmatter, doc.body)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_index_running_during_a_save_keeps_the_saved_rule() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    let db = ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .unwrap();
    let pool = db.pool().clone();
    let store = std::sync::Arc::new(PolicyStore::new(pool.clone(), root.clone()));
    // Enough rules that one index pass takes a while, as in a real project.
    for n in 0..300 {
        let (fm, b) = rule(&format!("filler-{n:03}"), "Filler.");
        store.save_rule(fm, b).await.unwrap();
    }
    for i in 0..40 {
        // Every other round saves a new rule, the others change one body.
        let id = if i % 2 == 0 {
            format!("race-{i}")
        } else {
            "race-edit".to_string()
        };
        let body = format!("Body {i}.");
        let (fm, b) = rule(&id, &body);
        let saver = {
            let store = store.clone();
            // Land the save somewhere inside the index pass.
            let delay = std::time::Duration::from_millis((i % 25) as u64 * 2);
            tokio::spawn(async move {
                tokio::time::sleep(delay).await;
                store.save_rule(fm, b).await.map(|_| ())
            })
        };
        let indexer = {
            let (pool, root) = (pool.clone(), root.clone());
            tokio::spawn(async move { index_policy(&pool, &root, false).await.map(|_| ()) })
        };
        saver.await.unwrap().unwrap();
        indexer.await.unwrap().unwrap();
        let row = get_rule(&pool, &id).await.unwrap();
        let row = row.unwrap_or_else(|| panic!("{id} was pruned by the concurrent index"));
        assert!(row.body.contains(&body), "{id} lost its body: {}", row.body);
    }
}
