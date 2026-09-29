//! `GET /api/links`: outgoing `[[links]]` of one rule, skill or memory, and its backlinks.
//! `GET /api/links/graph`: every item and every resolved link, for the policy graph.

use ax_policy::global_level::Kind;
use ax_policy::links::{parse_links, unique_stems, LinkIndex, LinkItem, LinkKind, LinkOrigin};
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::global_policy;
use crate::workspace_state::WebHub;

pub fn router_hub(hub: WebHub) -> Router {
    Router::new()
        .route("/", get(links))
        .route("/graph", get(graph))
        .with_state(hub)
}

#[derive(Deserialize)]
struct LinksQuery {
    kind: String,
    id: String,
    #[serde(default)]
    origin: Option<String>,
}

/// Something a link can point to, with the body its own links are read from.
struct Entry {
    item: LinkItem,
    title: String,
    body: String,
    /// Set for global items: the Command Center opens them with `projectId`.
    project_id: Option<i64>,
    tags: Vec<String>,
    /// Turn memories are raw prompts; the graph leaves them out.
    turn: bool,
}

impl Entry {
    fn reference(&self) -> Value {
        let mut v =
            json!({ "kind": self.item.kind, "id": self.item.id, "origin": self.item.origin });
        if let Some(pid) = self.project_id {
            v["projectId"] = json!(pid);
        }
        v
    }

    fn project(kind: LinkKind, id: &str, page: &str, title: &str, body: String) -> Self {
        Self {
            item: LinkItem::new(kind, LinkOrigin::Project, id, page),
            title: title.to_string(),
            body,
            project_id: None,
            tags: Vec::new(),
            turn: false,
        }
    }

    fn global(
        kind: LinkKind,
        item: &global_policy::GlobalItem,
        body: String,
        tags: Vec<String>,
    ) -> Self {
        Self {
            item: LinkItem::new(kind, LinkOrigin::Global, &item.name, &item.name),
            title: item.name.clone(),
            body,
            project_id: Some(item.project_id),
            tags,
            turn: false,
        }
    }
}

fn error(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

fn parse_kind(kind: &str) -> Option<LinkKind> {
    match kind {
        "rule" => Some(LinkKind::Rule),
        "skill" => Some(LinkKind::Skill),
        "memory" => Some(LinkKind::Memory),
        _ => None,
    }
}

/// Project rules, skills and memories, then the leading global rules and skills.
async fn entries(hub: &WebHub) -> Result<Vec<Entry>, String> {
    let mut out = Vec::new();
    {
        let ws = hub.read().await;
        let store = &ws.policy.store;
        for r in store.list_rules().await.map_err(|e| e.to_string())? {
            let mut entry = Entry::project(LinkKind::Rule, &r.id, &r.id, &r.id, r.body);
            entry.tags = r.tags;
            out.push(entry);
        }
        for s in store.list_skills().await.map_err(|e| e.to_string())? {
            let mut entry = Entry::project(LinkKind::Skill, &s.name, &s.name, &s.name, s.body);
            entry.tags = s.tags;
            out.push(entry);
        }
        let (memories, _) = ax_memory::list(&ws.graph_pool, 100_000, 0)
            .await
            .map_err(|e| e.to_string())?;
        let keys: Vec<(&str, &str, i64)> = memories
            .iter()
            .map(|m| (m.id.as_str(), m.title.as_str(), m.created_at))
            .collect();
        for (id, stem) in unique_stems(&keys) {
            if let Some(m) = memories.iter().find(|m| m.id == id) {
                let mut entry =
                    Entry::project(LinkKind::Memory, &id, &stem, &m.title, m.body.clone());
                entry.turn = m.kind == ax_memory::TURN_KIND;
                entry.tags = m.tags.clone();
                out.push(entry);
            }
        }
    }
    for item in global_policy::leaders(Kind::Rule).await? {
        if let Some(doc) = item.rule_doc() {
            out.push(Entry::global(
                LinkKind::Rule,
                &item,
                doc.body,
                doc.frontmatter.tags,
            ));
        }
    }
    for item in global_policy::leaders(Kind::Skill).await? {
        if let Some(doc) = item.skill_doc() {
            out.push(Entry::global(
                LinkKind::Skill,
                &item,
                doc.body,
                doc.frontmatter.tags,
            ));
        }
    }
    Ok(out)
}

async fn links(State(hub): State<WebHub>, Query(q): Query<LinksQuery>) -> Response {
    let Some(kind) = parse_kind(&q.kind) else {
        return error(
            StatusCode::BAD_REQUEST,
            "kind must be rule, skill or memory",
        );
    };
    let origin = match q.origin.as_deref() {
        Some("global") => LinkOrigin::Global,
        _ => LinkOrigin::Project,
    };
    let entries = match entries(&hub).await {
        Ok(e) => e,
        Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, &e),
    };
    let Some(me) = entries
        .iter()
        .position(|e| e.item.kind == kind && e.item.origin == origin && e.item.id == q.id)
    else {
        return error(StatusCode::NOT_FOUND, "not found");
    };
    let index = LinkIndex::new(entries.iter().map(|e| e.item.clone()).collect());
    let entry_for = |item: &LinkItem| entries.iter().find(|e| &e.item == item);

    let outgoing: Vec<Value> = parse_links(&entries[me].body)
        .into_iter()
        .map(|l| {
            let resolved = index.resolve(&l.target).and_then(entry_for).map(Entry::reference);
            json!({ "text": l.text, "target": l.target, "heading": l.heading, "label": l.label, "resolved": resolved })
        })
        .collect();

    let target = &entries[me].item;
    let mut sources: Vec<&Entry> = entries
        .iter()
        .enumerate()
        .filter(|(i, e)| {
            *i != me
                && parse_links(&e.body)
                    .iter()
                    .any(|l| index.resolve(&l.target) == Some(target))
        })
        .map(|(_, e)| e)
        .collect();
    sources.sort_by(|a, b| {
        (a.item.kind, &a.item.id, a.item.origin).cmp(&(b.item.kind, &b.item.id, b.item.origin))
    });
    let backlinks: Vec<Value> = sources
        .into_iter()
        .map(|e| {
            let mut v = e.reference();
            v["title"] = json!(e.title);
            v
        })
        .collect();

    Json(json!({ "outgoing": outgoing, "backlinks": backlinks })).into_response()
}

fn node_key(item: &LinkItem) -> String {
    let v = json!({ "kind": item.kind, "origin": item.origin });
    format!(
        "{}:{}:{}",
        v["kind"].as_str().unwrap_or_default(),
        v["origin"].as_str().unwrap_or_default(),
        item.id
    )
}

/// Resolved links as `(source, target)` indices into `entries`: no self-links, no duplicates,
/// and nothing to or from a turn memory.
fn graph_edges(entries: &[Entry]) -> Vec<(usize, usize)> {
    let index = LinkIndex::new(entries.iter().map(|e| e.item.clone()).collect());
    let mut seen = std::collections::HashSet::new();
    let mut edges = Vec::new();
    for (si, e) in entries.iter().enumerate() {
        if e.turn {
            continue;
        }
        for link in parse_links(&e.body) {
            let Some(target) = index.resolve(&link.target) else {
                continue;
            };
            let Some(ti) = entries.iter().position(|x| &x.item == target) else {
                continue;
            };
            if ti == si || entries[ti].turn || !seen.insert((si, ti)) {
                continue;
            }
            edges.push((si, ti));
        }
    }
    edges
}

async fn graph(State(hub): State<WebHub>) -> Response {
    let entries = match entries(&hub).await {
        Ok(e) => e,
        Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, &e),
    };
    let index = LinkIndex::new(entries.iter().map(|e| e.item.clone()).collect());
    let nodes: Vec<Value> = entries
        .iter()
        .filter(|e| !e.turn)
        .map(|e| {
            let mut v = e.reference();
            v["key"] = json!(node_key(&e.item));
            v["label"] = json!(e.title);
            v["target"] = json!(index.link_target(&e.item));
            v["tags"] = json!(e.tags);
            v
        })
        .collect();
    let edges: Vec<Value> = graph_edges(&entries)
        .into_iter()
        .map(|(s, t)| json!({ "source": node_key(&entries[s].item), "target": node_key(&entries[t].item) }))
        .collect();
    Json(json!({ "nodes": nodes, "edges": edges })).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(kind: LinkKind, id: &str, body: &str) -> Entry {
        Entry::project(kind, id, id, id, body.to_string())
    }

    #[test]
    fn duplicate_and_self_links_make_no_extra_edges() {
        let entries = vec![
            entry(
                LinkKind::Rule,
                "a",
                "[[b]] and [[b]] and [[a]] and [[missing]]",
            ),
            entry(LinkKind::Skill, "b", ""),
        ];
        assert_eq!(graph_edges(&entries), vec![(0, 1)]);
    }

    #[test]
    fn turn_memories_are_left_out() {
        let mut turn = entry(LinkKind::Memory, "t", "[[a]]");
        turn.turn = true;
        let entries = vec![entry(LinkKind::Rule, "a", "[[t]]"), turn];
        assert!(graph_edges(&entries).is_empty());
    }

    #[test]
    fn node_key_is_kind_origin_id() {
        assert_eq!(
            node_key(&entry(LinkKind::Skill, "pr", "").item),
            "skill:project:pr"
        );
    }
}
