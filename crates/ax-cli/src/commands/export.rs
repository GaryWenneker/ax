use std::collections::HashMap;
use std::path::PathBuf;

use ax_graph::analysis::is_semantic_edge;
use serde_json::json;

use crate::commands::resolve_path;
use crate::ui::SpinnerGuard;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphFormat {
    Html,
    Json,
    Dot,
    GraphMl,
    Gexf,
    Cypher,
    Mermaid,
    PlantUml,
}

impl GraphFormat {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim().to_ascii_lowercase().as_str() {
            "html" | "graph-html" => Ok(Self::Html),
            "json" => Ok(Self::Json),
            "dot" => Ok(Self::Dot),
            "graphml" => Ok(Self::GraphMl),
            "gexf" => Ok(Self::Gexf),
            "cypher" | "neo4j" => Ok(Self::Cypher),
            "mermaid" => Ok(Self::Mermaid),
            "plantuml" | "puml" => Ok(Self::PlantUml),
            other => Err(format!(
                "unknown graph format '{other}' — use html|json|dot|graphml|gexf|cypher|mermaid|plantuml"
            )),
        }
    }

    fn default_filename(self) -> &'static str {
        match self {
            Self::Html => "graph.html",
            Self::Json => "graph.json",
            Self::Dot => "graph.dot",
            Self::GraphMl => "graph.graphml",
            Self::Gexf => "graph.gexf",
            Self::Cypher => "graph.cypher",
            Self::Mermaid => "graph.mmd",
            Self::PlantUml => "graph.puml",
        }
    }
}

struct GraphExportData {
    project_name: String,
    total_nodes: usize,
    nodes: Vec<ExportNode>,
    edges: Vec<ExportEdge>,
}

struct ExportNode {
    id: String,
    name: String,
    kind: String,
    file: String,
    community: i64,
    label: Option<String>,
    degree: usize,
    god_node: bool,
}

struct ExportEdge {
    source: String,
    target: String,
    kind: String,
    confidence: String,
}

pub async fn run_graph_html(
    path: Option<String>,
    out: Option<String>,
    resolution: f64,
    limit: usize,
) -> Result<(), String> {
    run_graph(path, out, "html", resolution, limit).await
}

pub async fn run_graph(
    path: Option<String>,
    out: Option<String>,
    format: &str,
    resolution: f64,
    limit: usize,
) -> Result<(), String> {
    let fmt = GraphFormat::parse(format)?;
    let root = resolve_path(path);
    let resolution = if resolution > 0.0 { resolution } else { 1.0 };
    let limit = limit.max(1);

    let data = {
        let _spinner = SpinnerGuard::new(
            format!("Exporting graph ({})...", format),
            false,
        );
        load_export_data(&root, resolution, limit).await?
    };

    let body = match fmt {
        GraphFormat::Html => {
            let payload = json!({
                "nodes": data.nodes.iter().map(|n| json!({
                    "id": n.id,
                    "name": n.name,
                    "kind": n.kind,
                    "file": n.file,
                    "community": n.community,
                    "label": n.label,
                    "degree": n.degree,
                    "godNode": n.god_node,
                })).collect::<Vec<_>>(),
                "edges": data.edges.iter().map(|e| json!({
                    "s": e.source,
                    "t": e.target,
                    "kind": e.kind,
                    "confidence": e.confidence,
                })).collect::<Vec<_>>(),
                "totalNodes": data.total_nodes,
            });
            let payload_json = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".into());
            let safe_json = payload_json.replace("</", "<\\/");
            render_html(&data.project_name, &safe_json)
        }
        GraphFormat::Json => render_json(&data),
        GraphFormat::Dot => render_dot(&data),
        GraphFormat::GraphMl => render_graphml(&data),
        GraphFormat::Gexf => render_gexf(&data),
        GraphFormat::Cypher => render_cypher(&data),
        GraphFormat::Mermaid => render_mermaid(&data),
        GraphFormat::PlantUml => render_plantuml(&data),
    };

    let out_path = out
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join(fmt.default_filename()));
    std::fs::write(&out_path, body)
        .map_err(|e| format!("failed to write {}: {e}", out_path.display()))?;
    println!("Wrote graph ({format}) to {}", out_path.display());
    Ok(())
}

async fn load_export_data(
    root: &std::path::Path,
    resolution: f64,
    limit: usize,
) -> Result<GraphExportData, String> {
    let ax = ax_core::Ax::open(root).await.map_err(|e| e.to_string())?;
    ax.insights(resolution, 25, 25)
        .await
        .map_err(|e| e.to_string())?;

    let nodes = ax.queries().get_all_nodes().await.map_err(|e| e.to_string())?;
    let edges = ax.queries().get_all_edges().await.map_err(|e| e.to_string())?;
    let communities = ax
        .queries()
        .get_node_communities()
        .await
        .map_err(|e| e.to_string())?;

    let community_map: HashMap<String, (i64, Option<String>)> = communities
        .into_iter()
        .map(|(id, cid, label)| (id, (cid, label)))
        .collect();

    let mut degree: HashMap<&str, usize> = HashMap::new();
    for e in &edges {
        if !is_semantic_edge(e.kind) {
            continue;
        }
        *degree.entry(e.source.as_str()).or_insert(0) += 1;
        *degree.entry(e.target.as_str()).or_insert(0) += 1;
    }

    let mut ranked: Vec<&ax_types::Node> = nodes.iter().collect();
    ranked.sort_by(|a, b| {
        degree
            .get(b.id.as_str())
            .unwrap_or(&0)
            .cmp(degree.get(a.id.as_str()).unwrap_or(&0))
            .then_with(|| a.name.cmp(&b.name))
    });
    ranked.truncate(limit);

    let god_cutoff = ranked
        .get(0)
        .and_then(|n| degree.get(n.id.as_str()).copied())
        .unwrap_or(0);
    // Top 5% by degree (min 1) count as god-nodes in the export payload.
    let god_threshold = ((ranked.len() as f64) * 0.05).ceil().max(1.0) as usize;
    let god_ids: std::collections::HashSet<&str> = ranked
        .iter()
        .take(god_threshold)
        .filter(|n| degree.get(n.id.as_str()).copied().unwrap_or(0) > 0)
        .map(|n| n.id.as_str())
        .collect();
    let _ = god_cutoff;

    let selected: std::collections::HashSet<&str> =
        ranked.iter().map(|n| n.id.as_str()).collect();

    let export_nodes: Vec<ExportNode> = ranked
        .iter()
        .map(|n| {
            let (cid, label) = community_map
                .get(&n.id)
                .cloned()
                .unwrap_or((-1, None));
            let deg = degree.get(n.id.as_str()).copied().unwrap_or(0);
            ExportNode {
                id: n.id.clone(),
                name: n.name.clone(),
                kind: n.kind.as_str().to_string(),
                file: n.file_path.clone(),
                community: cid,
                label,
                degree: deg,
                god_node: god_ids.contains(n.id.as_str()),
            }
        })
        .collect();

    let export_edges: Vec<ExportEdge> = edges
        .iter()
        .filter(|e| {
            is_semantic_edge(e.kind)
                && selected.contains(e.source.as_str())
                && selected.contains(e.target.as_str())
        })
        .map(|e| ExportEdge {
            source: e.source.clone(),
            target: e.target.clone(),
            kind: e.kind.as_str().to_string(),
            confidence: e.effective_confidence().as_str().to_string(),
        })
        .collect();

    let project_name = root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("project")
        .to_string();

    Ok(GraphExportData {
        project_name,
        total_nodes: nodes.len(),
        nodes: export_nodes,
        edges: export_edges,
    })
}

fn render_html(project_name: &str, data_json: &str) -> String {
    let template = include_str!("export_graph.html");
    template
        .replace("__PROJECT_NAME__", &html_escape(project_name))
        .replace("\"__GRAPH_DATA__\"", data_json)
}

fn render_json(data: &GraphExportData) -> String {
    let payload = json!({
        "project": data.project_name,
        "totalNodes": data.total_nodes,
        "nodes": data.nodes.iter().map(|n| json!({
            "id": n.id,
            "name": n.name,
            "kind": n.kind,
            "file": n.file,
            "community": n.community,
            "label": n.label,
            "degree": n.degree,
            "godNode": n.god_node,
        })).collect::<Vec<_>>(),
        "edges": data.edges.iter().map(|e| json!({
            "source": e.source,
            "target": e.target,
            "kind": e.kind,
            "confidence": e.confidence,
        })).collect::<Vec<_>>(),
    });
    serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".into())
}

fn render_dot(data: &GraphExportData) -> String {
    let mut out = String::from("digraph ax {\n  rankdir=LR;\n  node [shape=box];\n");
    for n in &data.nodes {
        let label = format!("{}\\n{}", escape_dot(&n.name), n.kind);
        out.push_str(&format!(
            "  \"{}\" [label=\"{}\", community={}];\n",
            escape_dot(&n.id),
            label,
            n.community
        ));
    }
    for e in &data.edges {
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

fn render_graphml(data: &GraphExportData) -> String {
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
    for n in &data.nodes {
        out.push_str(&format!(
            "    <node id=\"{}\">\n      <data key=\"name\">{}</data>\n      <data key=\"kind\">{}</data>\n      <data key=\"community\">{}</data>\n      <data key=\"degree\">{}</data>\n      <data key=\"godNode\">{}</data>\n    </node>\n",
            xml_escape(&n.id),
            xml_escape(&n.name),
            xml_escape(&n.kind),
            n.community,
            n.degree,
            if n.god_node { "true" } else { "false" },
        ));
    }
    for (i, e) in data.edges.iter().enumerate() {
        out.push_str(&format!(
            "    <edge id=\"e{i}\" source=\"{}\" target=\"{}\">\n      <data key=\"ekind\">{}</data>\n      <data key=\"confidence\">{}</data>\n    </edge>\n",
            xml_escape(&e.source),
            xml_escape(&e.target),
            xml_escape(&e.kind),
            xml_escape(&e.confidence),
        ));
    }
    out.push_str("  </graph>\n</graphml>\n");
    out
}

fn render_gexf(data: &GraphExportData) -> String {
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
    for n in &data.nodes {
        out.push_str(&format!(
            "      <node id=\"{}\" label=\"{}\">\n        <attvalues>\n          <attvalue for=\"0\" value=\"{}\"/>\n          <attvalue for=\"1\" value=\"{}\"/>\n          <attvalue for=\"2\" value=\"{}\"/>\n          <attvalue for=\"3\" value=\"{}\"/>\n        </attvalues>\n      </node>\n",
            xml_escape(&n.id),
            xml_escape(&n.name),
            xml_escape(&n.kind),
            n.community,
            n.degree,
            if n.god_node { "true" } else { "false" },
        ));
    }
    out.push_str("    </nodes>\n    <edges>\n");
    for (i, e) in data.edges.iter().enumerate() {
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

fn render_cypher(data: &GraphExportData) -> String {
    let mut out = String::from("// ax graph export — Neo4j Cypher\n");
    for n in &data.nodes {
        out.push_str(&format!(
            "MERGE (n:AxNode {{id: {}}}) SET n.name = {}, n.kind = {}, n.file = {}, n.community = {}, n.degree = {}, n.godNode = {};\n",
            cypher_str(&n.id),
            cypher_str(&n.name),
            cypher_str(&n.kind),
            cypher_str(&n.file),
            n.community,
            n.degree,
            n.god_node,
        ));
    }
    for e in &data.edges {
        out.push_str(&format!(
            "MATCH (a:AxNode {{id: {}}}), (b:AxNode {{id: {}}}) MERGE (a)-[r:AX_EDGE {{kind: {}, confidence: {}}}]-(b);\n",
            cypher_str(&e.source),
            cypher_str(&e.target),
            cypher_str(&e.kind),
            cypher_str(&e.confidence),
        ));
    }
    out
}

fn render_mermaid(data: &GraphExportData) -> String {
    let mut out = String::from("flowchart LR\n");
    let mut id_map = HashMap::new();
    for (i, n) in data.nodes.iter().enumerate() {
        let sid = format!("N{i}");
        id_map.insert(n.id.as_str(), sid.clone());
        out.push_str(&format!(
            "  {sid}[\"{}\"]\n",
            mermaid_label(&format!("{} ({})", n.name, n.kind))
        ));
    }
    for e in &data.edges {
        let Some(s) = id_map.get(e.source.as_str()) else {
            continue;
        };
        let Some(t) = id_map.get(e.target.as_str()) else {
            continue;
        };
        out.push_str(&format!(
            "  {s} -->|{}| {t}\n",
            mermaid_label(&e.kind)
        ));
    }
    out
}

fn render_plantuml(data: &GraphExportData) -> String {
    let mut out = String::from("@startuml\nleft to right direction\n");
    for n in &data.nodes {
        out.push_str(&format!(
            "rectangle \"{}\" as {}\n",
            plantuml_escape(&format!("{}\\n{}", n.name, n.kind)),
            plantuml_id(&n.id),
        ));
    }
    for e in &data.edges {
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

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
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
