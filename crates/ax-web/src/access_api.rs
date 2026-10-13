use ax_global_db::{access, global_db_path, open_and_init};
use axum::{
    extract::{Path, Request, State},
    http::{header, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Extension, Json, Router,
};
use serde::Deserialize;
use std::sync::OnceLock;
use uuid::Uuid;

#[derive(Clone)]
pub(crate) struct Actor {
    pub(crate) member_id: String,
    pub(crate) client_id: Option<String>,
    pub(crate) role: access::MemberRole,
}

fn csrf() -> &'static str {
    static TOKEN: OnceLock<String> = OnceLock::new();
    TOKEN.get_or_init(|| Uuid::new_v4().to_string())
}

async fn authorize(State(hub): State<WebHub>, mut req: Request, next: Next) -> Response {
    let mutation = req.method() != axum::http::Method::GET;
    if mutation && hub.readonly {
        return failure(StatusCode::FORBIDDEN, "Command Center is read-only");
    }
    let db = match pool().await {
        Ok(db) => db,
        Err(_) => {
            return failure(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Access database unavailable",
            )
        }
    };
    let token = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    let actor = if let Some(token) = token {
        let principal = match access::authenticate_client_token(&db, token).await {
            Ok(Some(p)) => p,
            _ => return failure(StatusCode::UNAUTHORIZED, "Invalid client credential"),
        };
        let path = req.uri().path();
        let scope = if path.contains("/team") {
            "admin:team"
        } else if path.contains("/nodes") {
            "admin:nodes"
        } else {
            "admin:clients"
        };
        if !principal.allows(scope) {
            let _ = access::audit(
                &db,
                Some(&principal.member_id),
                Some(&principal.client_id),
                "authorization.denied",
                "scope",
                scope,
                &serde_json::json!({}),
            )
            .await;
            return failure(StatusCode::FORBIDDEN, "Insufficient client scope");
        }
        Actor {
            member_id: principal.member_id,
            client_id: Some(principal.client_id),
            role: principal.role,
        }
    } else if crate::web_auth::web_login_enabled(&db).await {
        let Some((member_id, role)) = crate::web_auth::session_member(&db, req.headers()).await
        else {
            return failure(StatusCode::UNAUTHORIZED, "Sign in required");
        };
        if mutation
            && req
                .headers()
                .get("x-ax-access-csrf")
                .and_then(|v| v.to_str().ok())
                != Some(csrf())
        {
            return failure(StatusCode::FORBIDDEN, "Missing CSRF token");
        }
        let required_scope = if req.uri().path().contains("/team") {
            "admin:team"
        } else if req.uri().path().contains("/nodes") {
            "admin:nodes"
        } else {
            "admin:clients"
        };
        if !role.permits(required_scope) {
            return failure(StatusCode::FORBIDDEN, "Insufficient member permissions");
        }
        Actor {
            member_id,
            client_id: None,
            role,
        }
    } else {
        if !crate::local_owner::browser_serves(req.headers(), hub.port)
            || (mutation
                && req
                    .headers()
                    .get("x-ax-access-csrf")
                    .and_then(|v| v.to_str().ok())
                    != Some(csrf()))
        {
            return failure(
                StatusCode::FORBIDDEN,
                "Local owner session or client credential required",
            );
        }
        let owner = access::list_members(&db).await.ok().and_then(|members| {
            members
                .into_iter()
                .find(|m| m.role == access::MemberRole::Owner && m.status == "active")
        });
        let Some(owner) = owner else {
            return failure(StatusCode::FORBIDDEN, "No active owner");
        };
        Actor {
            member_id: owner.id,
            client_id: None,
            role: access::MemberRole::Owner,
        }
    };
    req.extensions_mut().insert(actor);
    let mut response = next.run(req).await;
    response
        .headers_mut()
        .insert("cache-control", HeaderValue::from_static("no-store"));
    response
}

fn failure(status: StatusCode, message: &str) -> Response {
    (status, Json(serde_json::json!({"error":message}))).into_response()
}

use crate::workspace_state::WebHub;

#[derive(Deserialize)]
struct CreateMember {
    display_name: String,
    email: Option<String>,
    role: Option<String>,
    roles: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct CreateClient {
    member_id: String,
    display_name: String,
    client_type: String,
    scopes: Vec<String>,
}

pub(crate) async fn pool() -> Result<sqlx::SqlitePool, String> {
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
            Ok(members) => (
                StatusCode::OK,
                Json(serde_json::json!({"members": members})),
            ),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error":e.to_string()})),
            ),
        },
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error":e})),
        ),
    }
}

async fn create_member(
    Extension(actor): Extension<Actor>,
    State(hub): State<WebHub>,
    Json(body): Json<CreateMember>,
) -> impl IntoResponse {
    if let Err(resp) = writable(&hub) {
        return resp;
    }
    let pool = match pool().await {
        Ok(pool) => pool,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error":e})),
            )
        }
    };
    let parsed_roles: Result<Vec<_>, _> = body
        .roles
        .as_ref()
        .map(|roles| roles.iter().map(|r| role(r)).collect())
        .unwrap_or_else(|| {
            body.role
                .as_deref()
                .ok_or_else(|| "At least one role required".to_string())
                .and_then(role)
                .map(|r| vec![r])
        });
    let roles = match parsed_roles {
        Ok(roles) if !roles.is_empty() => roles,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error":"Invalid role set"})),
            )
        }
    };
    if roles.contains(&access::MemberRole::Owner) && actor.role != access::MemberRole::Owner {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error":"Only an owner can assign ownership"})),
        );
    }
    let id = Uuid::new_v4().to_string();
    let created = access::create_member(
        &pool,
        &id,
        &body.display_name,
        body.email.as_deref(),
        roles[0],
    )
    .await;
    let created = match created {
        Ok(()) if roles.len() > 1 => {
            access::update_member_roles(&pool, &id, &roles, "active").await
        }
        other => other,
    };
    match created {
        Ok(()) => {
            let _ = access::audit(
                &pool,
                Some(&actor.member_id),
                actor.client_id.as_deref(),
                "member.created",
                "member",
                &id,
                &serde_json::json!({"role":body.role}),
            )
            .await;
            (StatusCode::CREATED, Json(serde_json::json!({"id":id})))
        }
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":e.to_string()})),
        ),
    }
}

async fn list_clients() -> impl IntoResponse {
    match pool().await {
        Ok(pool) => match access::list_clients(&pool).await {
            Ok(clients) => match access::list_client_credentials(&pool).await {
                Ok(credentials) => (
                    StatusCode::OK,
                    Json(serde_json::json!({"clients": clients, "credentials":credentials})),
                ),
                Err(_) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error":"Credentials unavailable"})),
                ),
            },
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error":e.to_string()})),
            ),
        },
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error":e})),
        ),
    }
}

async fn create_client(
    Extension(actor): Extension<Actor>,
    State(hub): State<WebHub>,
    Json(body): Json<CreateClient>,
) -> impl IntoResponse {
    if let Err(resp) = writable(&hub) {
        return resp;
    }
    let pool = match pool().await {
        Ok(pool) => pool,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error":e})),
            )
        }
    };
    let id = Uuid::new_v4().to_string();
    match access::create_client(
        &pool,
        &id,
        &body.member_id,
        &body.display_name,
        &body.client_type,
        &body.scopes,
    )
    .await
    {
        Ok(()) => {
            let _ = access::audit(
                &pool,
                Some(&actor.member_id),
                actor.client_id.as_deref(),
                "client.created",
                "client",
                &id,
                &serde_json::json!({"member_id":body.member_id,"scopes":body.scopes}),
            )
            .await;
            (StatusCode::CREATED, Json(serde_json::json!({"id":id})))
        }
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":e.to_string()})),
        ),
    }
}

async fn issue_client_token(
    Extension(actor): Extension<Actor>,
    State(hub): State<WebHub>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Err(resp) = writable(&hub) {
        return resp;
    }
    let pool = match pool().await {
        Ok(pool) => pool,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error":e})),
            )
        }
    };
    match access::issue_client_credential(&pool, &id, None).await {
        Ok(credential) => {
            let _ = access::audit(
                &pool,
                Some(&actor.member_id),
                actor.client_id.as_deref(),
                "client.token_issued",
                "client",
                &id,
                &serde_json::json!({"credential_id":credential.credential_id}),
            )
            .await;
            (
                StatusCode::CREATED,
                Json(serde_json::json!({"credential":credential})),
            )
        }
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":e.to_string()})),
        ),
    }
}

async fn revoke_client(
    Extension(actor): Extension<Actor>,
    State(hub): State<WebHub>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Err(resp) = writable(&hub) {
        return resp;
    }
    let pool = match pool().await {
        Ok(pool) => pool,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error":e})),
            )
        }
    };
    match access::revoke_client(&pool, &id).await {
        Ok(true) => {
            let _ = access::audit(
                &pool,
                Some(&actor.member_id),
                actor.client_id.as_deref(),
                "client.revoked",
                "client",
                &id,
                &serde_json::json!({}),
            )
            .await;
            (StatusCode::OK, Json(serde_json::json!({"revoked":true})))
        }
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error":"client not found or already revoked"})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error":e.to_string()})),
        ),
    }
}

async fn list_nodes() -> impl IntoResponse {
    match pool().await {
        Ok(pool) => match access::list_nodes(&pool).await {
            Ok(nodes) => (
                StatusCode::OK,
                Json(
                    serde_json::json!({"nodes": nodes, "local": access::local_replication_status(&pool).await.ok().flatten()}),
                ),
            ),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error":e.to_string()})),
            ),
        },
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error":e})),
        ),
    }
}

async fn revoke_node(
    Extension(actor): Extension<Actor>,
    State(hub): State<WebHub>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Err(resp) = writable(&hub) {
        return resp;
    }
    let pool = match pool().await {
        Ok(pool) => pool,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error":e})),
            )
        }
    };
    match access::revoke_node(&pool, &id).await {
        Ok(true) => {
            let _ = access::audit(
                &pool,
                Some(&actor.member_id),
                actor.client_id.as_deref(),
                "node.revoked",
                "node",
                &id,
                &serde_json::json!({}),
            )
            .await;
            (StatusCode::OK, Json(serde_json::json!({"revoked":true})))
        }
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error":"node not found or already revoked"})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error":e.to_string()})),
        ),
    }
}

async fn remove_node(
    Extension(actor): Extension<Actor>,
    State(hub): State<WebHub>,
    Path(id): Path<String>,
) -> Response {
    if let Err(response) = writable(&hub) {
        return response.into_response();
    }
    let db = match pool().await {
        Ok(db) => db,
        Err(error) => return failure(StatusCode::INTERNAL_SERVER_ERROR, &error),
    };
    match access::remove_revoked_node(&db, &id).await {
        Ok(true) => {
            // Removal has already succeeded; audit failure must not restore the record.
            let _ = access::audit(
                &db,
                Some(&actor.member_id),
                actor.client_id.as_deref(),
                "node.removed",
                "node",
                &id,
                &serde_json::json!({"revocation_preserved":true}),
            )
            .await;
            (StatusCode::OK, Json(serde_json::json!({"removed":true}))).into_response()
        }
        Ok(false) => failure(StatusCode::CONFLICT, "Only a revoked node can be removed"),
        Err(error) => failure(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    }
}

#[derive(Deserialize)]
struct UpdateMember {
    role: Option<String>,
    roles: Option<Vec<String>>,
    status: String,
}
#[derive(Deserialize)]
struct UpdateScopes {
    scopes: Vec<String>,
}

async fn session() -> impl IntoResponse {
    Json(serde_json::json!({"csrf":csrf()}))
}

async fn update_member(
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
    Json(body): Json<UpdateMember>,
) -> Response {
    let db = match pool().await {
        Ok(db) => db,
        Err(_) => {
            return failure(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Access database unavailable",
            )
        }
    };
    let roles: Vec<access::MemberRole> = match body.roles.as_ref() {
        Some(roles) if !roles.is_empty() => {
            match roles.iter().map(|r| role(r)).collect::<Result<Vec<_>, _>>() {
                Ok(parsed) => parsed,
                Err(_) => return failure(StatusCode::BAD_REQUEST, "Invalid roles"),
            }
        }
        Some(_) => return failure(StatusCode::BAD_REQUEST, "At least one role required"),
        None => match body.role.as_deref().map(role) {
            Some(Ok(role)) => vec![role],
            _ => return failure(StatusCode::BAD_REQUEST, "Invalid role"),
        },
    };
    let members = match access::list_members(&db).await {
        Ok(m) => m,
        Err(_) => return failure(StatusCode::INTERNAL_SERVER_ERROR, "Members unavailable"),
    };
    if actor.role != access::MemberRole::Owner
        && (roles.contains(&access::MemberRole::Owner)
            || members
                .iter()
                .any(|m| m.id == id && m.roles.contains(&access::MemberRole::Owner)))
    {
        return failure(StatusCode::FORBIDDEN, "Only an owner can change ownership");
    }
    match access::update_member_roles(&db, &id, &roles, &body.status).await {
        Ok(()) => {
            let _ = access::audit(
                &db,
                Some(&actor.member_id),
                actor.client_id.as_deref(),
                "member.updated",
                "member",
                &id,
                &serde_json::json!({"roles":body.roles,"role":body.role,"status":body.status}),
            )
            .await;
            Json(serde_json::json!({"updated":true})).into_response()
        }
        Err(_) => failure(
            StatusCode::BAD_REQUEST,
            "Member not found or last active owner cannot be changed",
        ),
    }
}

async fn remove_member(Extension(actor): Extension<Actor>, Path(id): Path<String>) -> Response {
    let db = match pool().await {
        Ok(db) => db,
        Err(_) => {
            return failure(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Access database unavailable",
            )
        }
    };
    let members = match access::list_members(&db).await {
        Ok(members) => members,
        Err(_) => return failure(StatusCode::INTERNAL_SERVER_ERROR, "Members unavailable"),
    };
    if actor.role != access::MemberRole::Owner
        && members
            .iter()
            .any(|member| member.id == id && member.roles.contains(&access::MemberRole::Owner))
    {
        return failure(StatusCode::FORBIDDEN, "Only an owner can remove an owner");
    }
    match access::delete_member(&db, &id).await {
        Ok(()) => {
            let actor_id = (actor.member_id != id).then_some(actor.member_id.as_str());
            let _ = access::audit(
                &db,
                actor_id,
                actor.client_id.as_deref(),
                "member.deleted",
                "member",
                &id,
                &serde_json::json!({}),
            )
            .await;
            Json(serde_json::json!({"deleted": true})).into_response()
        }
        Err(_) => failure(
            StatusCode::BAD_REQUEST,
            "Member not found or the last active owner cannot be removed",
        ),
    }
}

async fn update_scopes(
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
    Json(body): Json<UpdateScopes>,
) -> Response {
    let db = match pool().await {
        Ok(db) => db,
        Err(_) => {
            return failure(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Access database unavailable",
            )
        }
    };
    match access::update_client_scopes(&db, &id, &body.scopes).await {
        Ok(()) => {
            let _ = access::audit(
                &db,
                Some(&actor.member_id),
                actor.client_id.as_deref(),
                "client.scopes_updated",
                "client",
                &id,
                &serde_json::json!({"scopes":body.scopes}),
            )
            .await;
            Json(serde_json::json!({"updated":true})).into_response()
        }
        Err(_) => failure(
            StatusCode::BAD_REQUEST,
            "Invalid scopes or inactive client/member",
        ),
    }
}

async fn rotate_token(Extension(actor): Extension<Actor>, Path(id): Path<String>) -> Response {
    let db = match pool().await {
        Ok(db) => db,
        Err(_) => {
            return failure(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Access database unavailable",
            )
        }
    };
    match access::rotate_client_credential(&db, &id).await {
        Ok(credential) => {
            let _ = access::audit(
                &db,
                Some(&actor.member_id),
                actor.client_id.as_deref(),
                "client.token_rotated",
                "client",
                &id,
                &serde_json::json!({"credential_id":credential.credential_id}),
            )
            .await;
            (
                StatusCode::CREATED,
                Json(serde_json::json!({"credential":credential})),
            )
                .into_response()
        }
        Err(_) => failure(StatusCode::BAD_REQUEST, "Client or member is not active"),
    }
}

async fn revoke_token(Extension(actor): Extension<Actor>, Path(id): Path<String>) -> Response {
    let db = match pool().await {
        Ok(db) => db,
        Err(_) => {
            return failure(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Access database unavailable",
            )
        }
    };
    match access::revoke_client_credential(&db, &id).await {
        Ok(true) => {
            let _ = access::audit(
                &db,
                Some(&actor.member_id),
                actor.client_id.as_deref(),
                "client.token_revoked",
                "credential",
                &id,
                &serde_json::json!({}),
            )
            .await;
            Json(serde_json::json!({"revoked":true})).into_response()
        }
        Ok(false) => failure(
            StatusCode::NOT_FOUND,
            "Credential not found or already revoked",
        ),
        Err(_) => failure(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Credential revocation failed",
        ),
    }
}

async fn allow_reenrollment(
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
) -> Response {
    let db = match pool().await {
        Ok(db) => db,
        Err(_) => {
            return failure(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Access database unavailable",
            )
        }
    };
    match access::allow_node_reenrollment(&db, &id).await {
        Ok(true) => {
            let _ = access::audit(
                &db,
                Some(&actor.member_id),
                actor.client_id.as_deref(),
                "node.reenrollment_allowed",
                "node",
                &id,
                &serde_json::json!({}),
            )
            .await;
            Json(serde_json::json!({"allowed":true})).into_response()
        }
        Ok(false) => failure(StatusCode::NOT_FOUND, "Blocked node not found"),
        Err(_) => failure(StatusCode::INTERNAL_SERVER_ERROR, "Node update failed"),
    }
}

pub fn router(hub: WebHub) -> Router {
    Router::new()
        .route("/session", get(session))
        .route("/team/{id}", axum::routing::patch(update_member).delete(remove_member))
        .route("/clients/{id}/scopes", axum::routing::patch(update_scopes))
        .route("/clients/{id}/rotate", post(rotate_token))
        .route("/credentials/{id}/revoke", post(revoke_token))
        .route("/team", get(list_team).post(create_member))
        .route("/clients", get(list_clients).post(create_client))
        .route("/clients/{id}/token", post(issue_client_token))
        .route("/clients/{id}/revoke", post(revoke_client))
        .route(
            "/nodes/local",
            get(crate::replication_setup::status).post(crate::replication_setup::start),
        )
        .route(
            "/nodes/local/interfaces",
            get(crate::replication_setup::interfaces),
        )
        .route("/nodes/local/stop", post(crate::replication_setup::stop))
        .route(
            "/nodes/local/credential",
            post(crate::replication_setup::credential),
        )
        .route(
            "/nodes/local/remove",
            post(crate::replication_setup::remove),
        )
        .route(
            "/nodes/local/discover",
            post(crate::replication_setup::discover),
        )
        .route("/nodes", get(list_nodes))
        .route("/nodes/{id}/revoke", post(revoke_node))
        .route("/nodes/{id}/remove", post(remove_node))
        .route("/nodes/{id}/reenroll", post(allow_reenrollment))
        .layer(middleware::from_fn_with_state(hub.clone(), authorize))
        .with_state(hub)
}
