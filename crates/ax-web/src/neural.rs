//! Command Center status for required Neural Intelligence.

use axum::{extract::State, response::Json, routing::get, Router};

use crate::workspace_state::WebHub;

pub fn router(hub: WebHub) -> Router {
    Router::new()
        .route("/status", get(handle_status))
        .with_state(hub)
}

async fn handle_status(State(hub): State<WebHub>) -> Json<serde_json::Value> {
    let ws = hub.read().await;
    let root = ws.project_root.clone();
    let pool = ws.graph_pool.clone();
    drop(ws);
    let value =
        match ax_neural::dispatch::handle(&root, "status", &serde_json::json!({}), Some(&pool))
            .await
        {
            Ok(value) => value,
            // Status still answers when the handler itself fails.
            Err(_) => ax_neural::status::status_report(&root),
        };
    Json(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    #[tokio::test]
    async fn status_measures_feedback_on_the_open_database() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join(".ax").join("ax.db");
        std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
        drop(ax_db::Database::open(&db_path).await.unwrap());
        let hub = WebHub::open(dir.path().to_path_buf(), true, 0)
            .await
            .unwrap();
        let response = router(hub)
            .oneshot(
                Request::builder()
                    .uri("/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["enabled"], true);
        assert_eq!(value["rankingActive"], true);
        assert_eq!(value["metrics"]["feedbackCount"]["kind"], "measured");
        assert_eq!(value["metrics"]["feedbackCount"]["value"], 0);
        assert_eq!(value["capabilities"]["hashVectorRetrieval"]["status"], "active");
        assert_eq!(value["capabilities"]["hashVectorRetrieval"]["kind"], "hash-v1");
    }
}
