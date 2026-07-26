//! Multi-format graph download for Command Center (`GET /api/graph/export`).

use std::collections::HashMap;

use axum::extract::{Query, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::queries::{self, GraphPayload};
use crate::workspace_state::WebHub;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphExportFormat {
    Json,
    Dot,
    GraphMl,
    Gexf,
    Cypher,
    Mermaid,
    PlantUml,
}

impl GraphExportFormat {
    fn parse(s: &str) -> Result<Self, String> {
        match s.trim().to_ascii_lowercase().as_str() {
            "json" => Ok(Self::Json),
            "dot" => Ok(Self::Dot),
            "graphml" => Ok(Self::GraphMl),
            "gexf" => Ok(Self::Gexf),
            "cypher" | "neo4j" => Ok(Self::Cypher),
            "mermaid" => Ok(Self::Mermaid),
            "plantuml" | "puml" => Ok(Self::PlantUml),
            "html" | "graph-html" => Err(
                "interactive HTML export is CLI-only — run `ax export graph --format html`".into(),
            ),
            other => Err(format!(
                "unknown graph format '{other}' — use json|dot|graphml|gexf|cypher|mermaid|plantuml"
            )),
        }
    }

    fn filename(self) -> &'static str {
        match self {
            Self::Json => "graph.json",
            Self::Dot => "graph.dot",
            Self::GraphMl => "graph.graphml",
            Self::Gexf => "graph.gexf",
            Self::Cypher => "graph.cypher",
            Self::Mermaid => "graph.mmd",
            Self::PlantUml => "graph.puml",
        }
    }

    fn content_type(self) -> &'static str {
        match self {
            Self::Json => "application/json; charset=utf-8",
            Self::Dot => "text/vnd.graphviz; charset=utf-8",
            Self::GraphMl | Self::Gexf => "application/xml; charset=utf-8",
            Self::Cypher | Self::Mermaid | Self::PlantUml => "text/plain; charset=utf-8",
        }
    }
}

#[derive(Deserialize)]
pub struct ExportQuery {
    #[serde(default = "default_format")]
    pub format: String,
    #[serde(default = "default_export_limit")]
    pub limit: i64,
}

fn default_format() -> String {
    "json".into()
}

fn default_export_limit() -> i64 {
    600
}

fn graph_max_limit() -> i64 {
    3_000
}

pub async fn handle_export(
    State(hub): State<WebHub>,
    Query(p): Query<ExportQuery>,
) -> impl IntoResponse {
    let fmt = match GraphExportFormat::parse(&p.format) {
        Ok(f) => f,
        Err(msg) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": msg })),
            )
                .into_response();
        }
    };

    let ws = hub.read().await;
    let project = ws
        .project_root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("project")
        .to_string();
    let limit = p.limit.clamp(1, graph_max_limit());
    let payload = match queries::get_graph(&ws.graph_pool, limit).await {
        Ok(p) => p,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };
    drop(ws);

    let body = render(fmt, &project, &payload);
    let disposition = format!("attachment; filename=\"{}\"", fmt.filename());
    let Ok(disp) = HeaderValue::from_str(&disposition) else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "invalid Content-Disposition" })),
        )
            .into_response();
    };
    let Ok(ctype) = HeaderValue::from_str(fmt.content_type()) else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "invalid Content-Type" })),
        )
            .into_response();
    };

    let mut response = (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, ctype),
            (header::CONTENT_DISPOSITION, disp),
            (
                header::CACHE_CONTROL,
                HeaderValue::from_static("no-store"),
            ),
        ],
        body,
    )
        .into_response();

    // Expose slice stats for Command Center export UX (CORS-safe custom headers).
    let headers = response.headers_mut();
    if let Ok(v) = HeaderValue::from_str(&payload.nodes.len().to_string()) {
        headers.insert("x-ax-export-nodes", v);
    }
    if let Ok(v) = HeaderValue::from_str(&payload.edges.len().to_string()) {
        headers.insert("x-ax-export-edges", v);
    }
    headers.insert(
        "x-ax-export-truncated",
        HeaderValue::from_static(if payload.truncated { "1" } else { "0" }),
    );
    headers.insert(
        "access-control-expose-headers",
        HeaderValue::from_static(
            "x-ax-export-nodes, x-ax-export-edges, x-ax-export-truncated, content-disposition",
        ),
    );

    response
}

fn god_ids(payload: &GraphPayload) -> std::collections::HashSet<&str> {
    if payload.nodes.is_empty() {
        return std::collections::HashSet::new();
    }
    let god_threshold = ((payload.nodes.len() as f64) * 0.05).ceil().max(1.0) as usize;
    payload
        .nodes
        .iter()
        .take(god_threshold)
        .filter(|n| n.degree > 0)
        .map(|n| n.id.as_str())
        .collect()
}

fn render(fmt: GraphExportFormat, project: &str, payload: &GraphPayload) -> String {
    match fmt {
        GraphExportFormat::Json => render_json(project, payload),
        GraphExportFormat::Dot => render_dot(payload),
        GraphExportFormat::GraphMl => render_graphml(payload),
        GraphExportFormat::Gexf => render_gexf(payload),
        GraphExportFormat::Cypher => render_cypher(payload),
        GraphExportFormat::Mermaid => render_mermaid(payload),
        GraphExportFormat::PlantUml => render_plantuml(payload),
    }
}

fn render_json(project: &str, payload: &GraphPayload) -> String {
    let gods = god_ids(payload);
    let body = json!({
        "project": project,
        "totalNodes": payload.total_nodes,
        "truncated": payload.truncated,
        "nodes": payload.nodes.iter().map(|n| json!({
            "id": n.id,
            "name": n.name,
            "kind": n.kind,
            "file": n.file_path,
            "community": n.community_id,
            "label": n.community_label,
            "degree": n.degree,
            "godNode": gods.contains(n.id.as_str()),
        })).collect::<Vec<_>>(),
        "edges": payload.edges.iter().map(|e| json!({
            "source": e.source,
            "target": e.target,
            "kind": e.kind,
            "confidence": e.confidence,
        })).collect::<Vec<_>>(),
    });
    serde_json::to_string_pretty(&body).unwrap_or_else(|_| "{}".into())
}

fn render_dot(payload: &GraphPayload) -> String {
    let mut out = String::from("digraph ax {\n  rankdir=LR;\n  node [shape=box];\n");
    for n in &payload.nodes {
        let label = format!("{}\\n{}", escape_dot(&n.name), escape_dot(&n.kind));
        out.push_str(&format!(
            "  \"{}\" [label=\"{}\", community={}];\n",
            escape_dot(&n.id),
            label,
            n.community_id
        ));
    }
    for e in &payload.edges {
        out.push_str(&format!(
            "  \"{}\" -> \"{}\" [label=\"{}\"];\n",
            escape_dot(&e.source),
            escape_dot(&e.target),
            escape_dot(&e.kind)
        ));
    }
    out.push_str("}\n");
    out
}

fn render_graphml(payload: &GraphPayload) -> String {
    let gods = god_ids(payload);
    let mut out = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<graphml xmlns="http://graphml.graphdrawing.org/xmlns">
  <key id="name" for="node" attr.name="name" attr.type="string"/>
  <key id="kind" for="node" attr.name="kind" attr.type="string"/>
  <key id="community" for="node" attr.name="community" attr.type="long"/>
  <key id="degree" for="node" attr.name="degree" attr.type="long"/>
  <key id="godNode" for="node" attr.name="godNode" attr.type="boolean"/>
  <key id="ekind" for="edge" attr.name="kind" attr.type="string"/>
  <key id="confidence" for="edge" attr.name="confidence" attr.type="string"/>
  <graph id="ax" edgedefault="directed">
"#,
    );
    for n in &payload.nodes {
        out.push_str(&format!(
            "    <node id=\"{}\">\n      <data key=\"name\">{}</data>\n      <data key=\"kind\">{}</data>\n      <data key=\"community\">{}</data>\n      <data key=\"degree\">{}</data>\n      <data key=\"godNode\">{}</data>\n    </node>\n",
            xml_escape(&n.id),
            xml_escape(&n.name),
            xml_escape(&n.kind),
            n.community_id,
            n.degree,
            if gods.contains(n.id.as_str()) {
                "true"
            } else {
                "false"
            },
        ));
    }
    for (i, e) in payload.edges.iter().enumerate() {
        out.push_str(&format!(
            "    <edge id=\"e{i}\" source=\"{}\" target=\"{}\">\n      <data key=\"ekind\">{}</data>\n      <data key=\"confidence\">{}</data>\n    </edge>\n",
            xml_escape(&e.source),
            xml_escape(&e.target),
            xml_escape(&e.kind),
            xml_escape(e.confidence.as_deref().unwrap_or("")),
        ));
    }
    out.push_str("  </graph>\n</graphml>\n");
    out
}

fn render_gexf(payload: &GraphPayload) -> String {
    let gods = god_ids(payload);
    let mut out = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<gexf xmlns="http://gexf.net/1.3" version="1.3">
  <graph defaultedgetype="directed" mode="static">
    <attributes class="node">
      <attribute id="0" title="kind" type="string"/>
      <attribute id="1" title="community" type="integer"/>
      <attribute id="2" title="degree" type="integer"/>
      <attribute id="3" title="godNode" type="boolean"/>
    </attributes>
    <nodes>
"#,
    );
    for n in &payload.nodes {
        out.push_str(&format!(
            "      <node id=\"{}\" label=\"{}\">\n        <attvalues>\n          <attvalue for=\"0\" value=\"{}\"/>\n          <attvalue for=\"1\" value=\"{}\"/>\n          <attvalue for=\"2\" value=\"{}\"/>\n          <attvalue for=\"3\" value=\"{}\"/>\n        </attvalues>\n      </node>\n",
            xml_escape(&n.id),
            xml_escape(&n.name),
            xml_escape(&n.kind),
            n.community_id,
            n.degree,
            if gods.contains(n.id.as_str()) {
                "true"
            } else {
                "false"
            },
        ));
    }
    out.push_str("    </nodes>\n    <edges>\n");
    for (i, e) in payload.edges.iter().enumerate() {
        out.push_str(&format!(
            "      <edge id=\"{i}\" source=\"{}\" target=\"{}\" label=\"{}\"/>\n",
            xml_escape(&e.source),
            xml_escape(&e.target),
            xml_escape(&e.kind),
        ));
    }
    out.push_str("    </edges>\n  </graph>\n</gexf>\n");
    out
}

fn render_cypher(payload: &GraphPayload) -> String {
    let gods = god_ids(payload);
    let mut out = String::from("// ax graph export — Neo4j Cypher\n");
    for n in &payload.nodes {
        out.push_str(&format!(
            "MERGE (n:AxNode {{id: {}}}) SET n.name = {}, n.kind = {}, n.file = {}, n.community = {}, n.degree = {}, n.godNode = {};\n",
            cypher_str(&n.id),
            cypher_str(&n.name),
            cypher_str(&n.kind),
            cypher_str(&n.file_path),
            n.community_id,
            n.degree,
            gods.contains(n.id.as_str()),
        ));
    }
    for e in &payload.edges {
        out.push_str(&format!(
            "MATCH (a:AxNode {{id: {}}}), (b:AxNode {{id: {}}}) MERGE (a)-[r:AX_EDGE {{kind: {}, confidence: {}}}]-(b);\n",
            cypher_str(&e.source),
            cypher_str(&e.target),
            cypher_str(&e.kind),
            cypher_str(e.confidence.as_deref().unwrap_or("")),
        ));
    }
    out
}

fn render_mermaid(payload: &GraphPayload) -> String {
    let mut out = String::from("flowchart LR\n");
    let mut id_map = HashMap::new();
    for (i, n) in payload.nodes.iter().enumerate() {
        let sid = format!("N{i}");
        id_map.insert(n.id.as_str(), sid.clone());
        out.push_str(&format!(
            "  {sid}[\"{}\"]\n",
            mermaid_label(&format!("{} ({})", n.name, n.kind))
        ));
    }
    for e in &payload.edges {
        let Some(s) = id_map.get(e.source.as_str()) else {
            continue;
        };
        let Some(t) = id_map.get(e.target.as_str()) else {
            continue;
        };
        out.push_str(&format!("  {s} -->|{}| {t}\n", mermaid_label(&e.kind)));
    }
    out
}

fn render_plantuml(payload: &GraphPayload) -> String {
    let mut out = String::from("@startuml\nleft to right direction\n");
    for n in &payload.nodes {
        out.push_str(&format!(
            "rectangle \"{}\" as {}\n",
            plantuml_escape(&format!("{}\\n{}", n.name, n.kind)),
            plantuml_id(&n.id),
        ));
    }
    for e in &payload.edges {
        out.push_str(&format!(
            "{} --> {} : {}\n",
            plantuml_id(&e.source),
            plantuml_id(&e.target),
            plantuml_escape(&e.kind),
        ));
    }
    out.push_str("@enduml\n");
    out
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn escape_dot(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn cypher_str(s: &str) -> String {
    format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'"))
}

fn mermaid_label(s: &str) -> String {
    s.replace('"', "'").replace('[', "(").replace(']', ")")
}

fn plantuml_escape(s: &str) -> String {
    s.replace('"', "'")
}

fn plantuml_id(s: &str) -> String {
    let mut out = String::from("N_");
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    out
}
