//! Ship Command Center API (SSE + status).

use std::convert::Infallible;
use std::sync::Arc;

use ax_ship::{ShipDaemon, ShipEvent, ShipReport};
use axum::{
    extract::State,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Json,
    },
    routing::{get, post},
    Router,
};
use futures_util::stream::Stream;
use serde::Deserialize;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct ShipApiState {
    pub daemon: Arc<ShipDaemon>,
    pub report: Arc<Mutex<Option<ShipReport>>>,
}

pub fn router(state: ShipApiState) -> Router {
    Router::new()
        .route("/events", get(handle_ship_events))
        .route("/status", get(handle_ship_status))
        .route("/impact", get(handle_ship_impact))
        .route("/command", post(handle_ship_command))
        .with_state(state)
}

async fn handle_ship_status(State(s): State<ShipApiState>) -> impl IntoResponse {
    let report = s.report.lock().await.clone();
    Json(serde_json::json!({
        "branch": ax_git::current_branch(&s.daemon.project_root).ok().flatten(),
        "report": report,
    }))
}

async fn handle_ship_impact(State(s): State<ShipApiState>) -> impl IntoResponse {
    let report = s.report.lock().await.clone();
    Json(serde_json::json!({ "report": report }))
}

async fn handle_ship_events(
    State(s): State<ShipApiState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut rx = s.daemon.bus.subscribe();
    let report_store = s.report.clone();
    let stream = async_stream::stream! {
        if let Ok(report) = s.daemon.evaluate().await {
            *report_store.lock().await = Some(report.clone());
            yield Ok(Event::default().data(serde_json::to_string(&ShipEvent::ReportUpdated(report)).unwrap_or_default()));
        }
        loop {
            match rx.recv().await {
                Ok(ev) => {
                    if let ShipEvent::ReportUpdated(ref r) = ev {
                        *report_store.lock().await = Some(r.clone());
                    }
                    yield Ok(Event::default().data(serde_json::to_string(&ev).unwrap_or_default()));
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            }
        }
    };
    Sse::new(stream).keep_alive(KeepAlive::default())
}

#[derive(Deserialize)]
struct ShipCommandBody {
    cmd: String,
}

async fn handle_ship_command(
    State(s): State<ShipApiState>,
    Json(body): Json<ShipCommandBody>,
) -> impl IntoResponse {
    match body.cmd.as_str() {
        "evaluate" => match s.daemon.evaluate().await {
            Ok(r) => Json(serde_json::json!({ "ok": true, "report": r })).into_response(),
            Err(e) => Json(serde_json::json!({ "ok": false, "error": e })).into_response(),
        },
        "draft" => {
            let pipeline = ax_ship::ShipPipeline::new(
                s.daemon.project_root.clone(),
                s.daemon.config.clone(),
                s.daemon.bus.clone(),
            );
            match pipeline
                .create_draft_pr("ax ship draft", "Created via ax Command Center")
                .await
            {
                Ok(pr) => Json(serde_json::json!({ "ok": true, "pr": pr })).into_response(),
                Err(e) => Json(serde_json::json!({ "ok": false, "error": e })).into_response(),
            }
        }
        other => Json(serde_json::json!({ "ok": false, "error": format!("unknown cmd: {other}") })).into_response(),
    }
}

pub async fn start_watcher(state: ShipApiState) {
    let root = state.daemon.project_root.clone();
    let bus = state.daemon.bus.clone();
    let _ = ax_ship::ShipDaemon::new(root.clone()).run_watch().await;
    let _ = state.daemon.evaluate().await;
    let _ = bus;
}
