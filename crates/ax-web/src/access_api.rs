use ax_global_db::{access, global_db_path, open_and_init};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::workspace_state::WebHub;

#[derive(Deserialize)]
struct CreateMember {
    display_name: String,
    email: Option<String>,
    role: String,
}

#[derive(Deserialize)]
struct CreateClient {
    member_id: String,
    display_name: String,
    client_type: String,
    scopes: Vec<String>,
}

async fn pool() -> Result<sqlx::SqlitePool, String> {
    let path = global_db_path().map_err(|e| e.to_string())?;
    let pool = open_and_init(&path).await.map_err(|e| e.to_string())?;
    let _ = access::bootstrap_owner(&pool, "local-owner", "Local owner", None)
        .await
        .map_err(|e| e.to_string())?;
    Ok(pool)
}

fn role(value: &str) -> Result<access::MemberRole, String> {
    match value {
        "owner" => Ok(access::MemberRole::Owner),
        "admin" => Ok(access::MemberRole::Admin),
        "member" => Ok(access::MemberRole::Member),
        "readonly" => Ok(access::MemberRole::Readonly),
        _ => Err("invalid role".into()),
    }
}

fn writable(hub: &WebHub) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    if hub.readonly {
        Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error":"Command Center is read-only"})),
        ))
    } else {
        Ok(())
    }
}

async fn list_team() -> impl IntoResponse {
    match pool().await {
        Ok(pool) => match access::list_members(&pool).await {
            Ok(members) => (StatusCode::OK, Json(serde_json::json!({"members": members}))),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error":e.to_string()}))),
        },
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error":e}))),
    }
}

async fn create_member(State(hub): State<WebHub>, Json(body): Json<CreateMember>) -> impl IntoResponse {
    if let Err(resp) = writable(&hub) { return resp; }
    let pool = match pool().await {
        Ok(pool) => pool,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error":e}))),
    };
    let role = match role(&body.role) {
        Ok(role) => role,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":e}))),
    };
    let id = Uuid::new_v4().to_string();
    match access::create_member(&pool, &id, &body.display_name, body.email.as_deref(), role).await {
        Ok(()) => {
            let _ = access::audit(&pool, Some("local-owner"), None, "member.created", "member", &id, &serde_json::json!({"role":body.role})).await;
            (StatusCode::CREATED, Json(serde_json::json!({"id":id})))
        }
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":e.to_string()}))),
    }
}

async fn list_clients() -> impl IntoResponse {
    match pool().await {
        Ok(pool) => match access::list_clients(&pool).await {
            Ok(clients) => (StatusCode::OK, Json(serde_json::json!({"clients": clients}))),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error":e.to_string()}))),
        },
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error":e}))),
    }
}

async fn create_client(State(hub): State<WebHub>, Json(body): Json<CreateClient>) -> impl IntoResponse {
    if let Err(resp) = writable(&hub) { return resp; }
    let pool = match pool().await {
        Ok(pool) => pool,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error":e}))),
    };
    let id = Uuid::new_v4().to_string();
    match access::create_client(&pool, &id, &body.member_id, &body.display_name, &body.client_type, &body.scopes).await {
        Ok(()) => {
            let _ = access::audit(&pool, Some("local-owner"), None, "client.created", "client", &id, &serde_json::json!({"member_id":body.member_id,"scopes":body.scopes})).await;
            (StatusCode::CREATED, Json(serde_json::json!({"id":id})))
        }
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":e.to_string()}))),
    }
}

async fn issue_client_token(State(hub): State<WebHub>, Path(id): Path<String>) -> impl IntoResponse {
    if let Err(resp) = writable(&hub) { return resp; }
    let pool = match pool().await {
        Ok(pool) => pool,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error":e}))),
    };
    match access::issue_client_credential(&pool, &id, None).await {
        Ok(credential) => {
            let _ = access::audit(&pool, Some("local-owner"), None, "client.token_issued", "client", &id, &serde_json::json!({"credential_id":credential.credential_id})).await;
            (StatusCode::CREATED, Json(serde_json::json!({"credential":credential})))
        }
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":e.to_string()}))),
    }
}

async fn revoke_client(State(hub): State<WebHub>, Path(id): Path<String>) -> impl IntoResponse {
    if let Err(resp) = writable(&hub) { return resp; }
    let pool = match pool().await {
        Ok(pool) => pool,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error":e}))),
    };
    match access::revoke_client(&pool, &id).await {
        Ok(true) => {
            let _ = access::audit(&pool, Some("local-owner"), None, "client.revoked", "client", &id, &serde_json::json!({})).await;
            (StatusCode::OK, Json(serde_json::json!({"revoked":true})))
        }
        Ok(false) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error":"client not found or already revoked"}))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error":e.to_string()}))),
    }
}

async fn list_nodes() -> impl IntoResponse {
    match pool().await {
        Ok(pool) => match access::list_nodes(&pool).await {
            Ok(nodes) => (StatusCode::OK, Json(serde_json::json!({"nodes": nodes}))),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error":e.to_string()}))),
        },
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error":e}))),
    }
}

async fn revoke_node(State(hub): State<WebHub>, Path(id): Path<String>) -> impl IntoResponse {
    if let Err(resp) = writable(&hub) { return resp; }
    let pool = match pool().await {
        Ok(pool) => pool,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error":e}))),
    };
    match access::revoke_node(&pool, &id).await {
        Ok(true) => {
            let _ = access::audit(&pool, Some("local-owner"), None, "node.revoked", "node", &id, &serde_json::json!({})).await;
            (StatusCode::OK, Json(serde_json::json!({"revoked":true})))
        }
        Ok(false) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error":"node not found or already revoked"}))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error":e.to_string()}))),
    }
}

pub fn router(hub: WebHub) -> Router {
    Router::new()
        .route("/team", get(list_team).post(create_member))
        .route("/clients", get(list_clients).post(create_client))
        .route("/clients/{id}/token", post(issue_client_token))
        .route("/clients/{id}/revoke", post(revoke_client))
        .route("/nodes", get(list_nodes))
        .route("/nodes/{id}/revoke", post(revoke_node))
        .with_state(hub)
}
