//! Live action stream for Command Center (agent/MCP/graph events).

use std::convert::Infallible;
use std::sync::OnceLock;
use std::time::Duration;

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::workspace_state::WebHub;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionEvent {
    pub ts: i64,
    pub kind: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<serde_json::Value>,
}

fn bus() -> &'static broadcast::Sender<ActionEvent> {
    static BUS: OnceLock<broadcast::Sender<ActionEvent>> = OnceLock::new();
    BUS.get_or_init(|| {
        let (tx, _) = broadcast::channel(256);
        tx
    })
}

pub fn publish(kind: impl Into<String>, message: impl Into<String>, meta: Option<serde_json::Value>) {
    let event = ActionEvent {
        ts: chrono::Utc::now().timestamp_millis(),
        kind: kind.into(),
        message: message.into(),
        meta,
    };
    let _ = bus().send(event);
}

pub fn router_hub(hub: WebHub) -> Router {
    Router::new()
        .route("/events", get(handle_events))
        .route("/publish", post(handle_publish))
        .with_state(hub)
}

async fn handle_events(
    State(_hub): State<WebHub>,
) -> Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>> {
    let mut rx = bus().subscribe();
    publish("stream", "client connected", None);
    let s = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(ev) => {
                    let data = serde_json::to_string(&ev).unwrap_or_default();
                    yield Ok(Event::default().event("action").data(data));
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    };
    Sse::new(s).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PublishBody {
    kind: String,
    message: String,
    #[serde(default)]
    meta: Option<serde_json::Value>,
}

async fn handle_publish(
    State(hub): State<WebHub>,
    Json(body): Json<PublishBody>,
) -> Json<serde_json::Value> {
    if hub.readonly {
        return Json(serde_json::json!({ "ok": false, "error": "read-only" }));
    }
    publish(body.kind, body.message, body.meta);
    Json(serde_json::json!({ "ok": true }))
}
