//! Loopback-only browser login controls shared by web and embedded IDE Settings.
use crate::workspace_state::WebHub;
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::OnceLock;

fn csrf() -> &'static str {
    static TOKEN: OnceLock<String> = OnceLock::new();
    TOKEN.get_or_init(|| uuid::Uuid::new_v4().to_string())
}

pub fn router(hub: WebHub) -> Router {
    Router::new()
        .route("/", axum::routing::get(status))
        .route("/login", axum::routing::post(login))
        .route("/login/{id}", axum::routing::get(login_status))
        .route("/login/{id}/cancel", axum::routing::post(cancel))
        .route("/logout", axum::routing::post(logout))
        .with_state(hub)
}

fn check(headers: &HeaderMap, hub: &WebHub, mutation: bool) -> Result<(), &'static str> {
    check_local(headers, hub.port, hub.readonly, mutation)
}

fn check_local(
    headers: &HeaderMap,
    port: u16,
    readonly: bool,
    mutation: bool,
) -> Result<(), &'static str> {
    if readonly {
        return Err("Remote connections can only be managed on the local Command Center");
    }
    let expected = format!("127.0.0.1:{port}");
    let alternate = format!("localhost:{port}");
    let host = headers
        .get("host")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    if host != expected && host != alternate {
        return Err("Local Command Center host required");
    }
    if headers.get("sec-fetch-site").and_then(|h| h.to_str().ok()) == Some("cross-site") {
        return Err("Cross-site request rejected");
    }
    if let Some(origin) = headers.get("origin") {
        let origin = origin.to_str().unwrap_or("");
        if origin != format!("http://{expected}") && origin != format!("http://{alternate}") {
            return Err("Local Command Center origin required");
        }
    }
    if mutation && headers.get("x-ax-csrf").and_then(|h| h.to_str().ok()) != Some(csrf()) {
        return Err("Reload Settings before changing the connection");
    }
    Ok(())
}

fn failure(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({"error":message}))).into_response()
}
fn success(value: Value) -> Response {
    let mut response = Json(value).into_response();
    response.headers_mut().insert(
        "cache-control",
        axum::http::HeaderValue::from_static("no-store"),
    );
    response
}

async fn status(State(hub): State<WebHub>, headers: HeaderMap) -> Response {
    if let Err(error) = check(&headers, &hub, false) {
        return failure(StatusCode::FORBIDDEN, error);
    }
    match tokio::task::spawn_blocking(ax_mcp::remote::credentials::connections).await {
        Ok(Ok(connections)) => success(
            json!({"csrf":csrf(),"connections":connections.into_iter().map(|c| json!({"url":c.url,"client_id":c.client_id,"issuer":c.issuer,"expires_at":c.expires_at,"state":if c.expires_at > ax_mcp::remote::credentials::now() { "stored" } else { "expired" }})).collect::<Vec<_>>() }),
        ),
        _ => failure(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Cannot read remote connections",
        ),
    }
}
#[derive(Deserialize)]
struct LoginBody {
    url: String,
    client_id: String,
    #[serde(default)]
    write: bool,
}

async fn login(
    State(hub): State<WebHub>,
    headers: HeaderMap,
    Json(body): Json<LoginBody>,
) -> Response {
    if let Err(error) = check(&headers, &hub, true) {
        return failure(StatusCode::FORBIDDEN, error);
    }
    match ax_mcp::remote::client::start_login(body.url, body.client_id, body.write).await {
        Ok(attempt) => success(serde_json::to_value(attempt).unwrap_or(Value::Null)),
        Err(error) => failure(StatusCode::BAD_REQUEST, &error),
    }
}
async fn login_status(
    State(hub): State<WebHub>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    if let Err(error) = check(&headers, &hub, false) {
        return failure(StatusCode::FORBIDDEN, error);
    }
    match ax_mcp::remote::client::login_status(&id).await {
        Some(status) => success(serde_json::to_value(status).unwrap_or(Value::Null)),
        None => failure(StatusCode::NOT_FOUND, "Sign-in attempt is unavailable"),
    }
}
async fn cancel(State(hub): State<WebHub>, headers: HeaderMap, Path(id): Path<String>) -> Response {
    if let Err(error) = check(&headers, &hub, true) {
        return failure(StatusCode::FORBIDDEN, error);
    }
    match ax_mcp::remote::client::cancel_login(&id).await {
        Ok(()) => success(json!({"ok":true})),
        Err(error) => failure(StatusCode::NOT_FOUND, &error),
    }
}

#[derive(Deserialize)]
struct LogoutBody {
    url: String,
}
async fn logout(
    State(hub): State<WebHub>,
    headers: HeaderMap,
    Json(body): Json<LogoutBody>,
) -> Response {
    if let Err(error) = check(&headers, &hub, true) {
        return failure(StatusCode::FORBIDDEN, error);
    }
    match ax_mcp::remote::client::logout(&body.url).await {
        Ok(()) => success(json!({"ok":true})),
        Err(error) => failure(StatusCode::BAD_REQUEST, &error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_login_controls_reject_foreign_origin_and_csrf() {
        let mut headers = HeaderMap::new();
        headers.insert("host", "127.0.0.1:7070".parse().unwrap());
        assert!(check_local(&headers, 7070, false, false).is_ok());
        assert!(check_local(&headers, 7070, false, true).is_err());
        headers.insert("x-ax-csrf", csrf().parse().unwrap());
        assert!(check_local(&headers, 7070, false, true).is_ok());
        headers.insert("origin", "https://attacker.example".parse().unwrap());
        assert!(check_local(&headers, 7070, false, true).is_err());
        headers.remove("origin");
        headers.insert("sec-fetch-site", "cross-site".parse().unwrap());
        assert!(check_local(&headers, 7070, false, false).is_err());
        headers.remove("sec-fetch-site");
        assert!(check_local(&headers, 7070, true, false).is_err());
    }
}
