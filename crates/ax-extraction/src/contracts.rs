//! API contract federation — OpenAPI, Protobuf, GraphQL → graph nodes/edges.
//!
//! Creates one `contract:…` Route node per operation and file, such as
//! `contract:openapi:<file>:GET:/users`. Operations in different files that
//! share the same path+method / RPC name / GraphQL field are linked with
//! inferred `References` edges. Contract files themselves are `Doc` nodes.

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};

use ax_db::queries::QueryBuilder;
use ax_types::{Edge, EdgeConfidence, EdgeKind, Language, Node, NodeKind, Provenance};
use ignore::WalkBuilder;
use serde_json::Value;

fn meta(pairs: &[(&str, Value)]) -> Option<HashMap<String, Value>> {
    let mut m = HashMap::new();
    for (k, v) in pairs {
        m.insert((*k).into(), v.clone());
    }
    Some(m)
}

const SKIP_DIRS: &[&str] = &[
    "node_modules",
    "target",
    "target-dev",
    "dist",
    "build",
    ".git",
    ".ax",
    "vendor",
];

#[derive(Debug, Clone)]
struct ContractOp {
    /// Node id, unique per file: `contract:openapi:<file>:GET:/users`.
    id: String,
    /// File-independent key shared by producers and consumers of the same
    /// operation: `contract:openapi:GET:/users` or `contract:proto:UserService.Get`.
    key: String,
    name: String,
    kind: &'static str,
    file: String,
    line: i32,
}

/// Index OpenAPI / Protobuf / GraphQL contracts under `project_root`.
/// Returns the number of contract operation nodes written.
pub async fn index_contracts(
    project_root: &Path,
    queries: &QueryBuilder,
    exclude: &[String],
) -> Result<usize, ax_utils::errors::AxError> {
    let files = scan_contract_files(project_root, exclude);
    let now = now_ms();
    let mut ops: Vec<ContractOp> = Vec::new();
    let mut doc_nodes: Vec<Node> = Vec::new();
    let mut file_ops: HashMap<String, Vec<String>> = HashMap::new();

    for file in &files {
        let content = match ax_utils::read_text_file(&file.full) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let parsed = match file.kind {
            ContractKind::OpenApi => parse_openapi(&file.rel, &content),
            ContractKind::Protobuf => parse_protobuf(&file.rel, &content),
            ContractKind::GraphQl => parse_graphql(&file.rel, &content),
        };
        if parsed.is_empty() && file.kind != ContractKind::OpenApi {
            // Still record the file as a Doc when it looks like a contract by extension.
        }
        doc_nodes.push(contract_doc_node(&file.rel, file.kind, now));
        let mut ids = Vec::new();
        for op in parsed {
            ids.push(op.id.clone());
            ops.push(op);
        }
        file_ops.insert(file.rel.clone(), ids);
    }

    // This pass scans every contract file, so the ops above are the whole
    // `contract:` namespace. Drop ids it no longer writes (deleted files, or an
    // id format an older extractor used) with their edges, then upsert the rest.
    let keep: HashSet<String> = ops.iter().map(|op| op.id.clone()).collect();
    queries
        .delete_nodes_with_prefix_except("contract:", &keep)
        .await?;
    let mut nodes: Vec<Node> = doc_nodes;
    for op in &ops {
        nodes.push(op_node(op, now));
    }
    if !nodes.is_empty() {
        queries.upsert_nodes(&nodes).await?;
    }

    let mut edges: Vec<Edge> = Vec::new();
    // Doc → operation
    for (file, ids) in &file_ops {
        let doc_id = format!("doc:{file}");
        for op_id in ids {
            edges.push(Edge {
                source: doc_id.clone(),
                target: op_id.clone(),
                kind: EdgeKind::Contains,
                metadata: meta(&[("contract", Value::Bool(true))]),
                line: None,
                column: None,
                provenance: Some(Provenance::Heuristic),
                confidence: Some(EdgeConfidence::Inferred),
            });
        }
    }

    // Same operation key appearing in multiple files → References (producer ↔ consumer).
    let mut by_key: HashMap<&str, Vec<&ContractOp>> = HashMap::new();
    for op in &ops {
        by_key.entry(op.key.as_str()).or_default().push(op);
    }
    for group in by_key.values() {
        if group.len() < 2 {
            continue;
        }
        for i in 0..group.len() {
            for j in (i + 1)..group.len() {
                if group[i].file == group[j].file || group[i].id == group[j].id {
                    continue;
                }
                edges.push(Edge {
                    source: group[i].id.clone(),
                    target: group[j].id.clone(),
                    kind: EdgeKind::References,
                    metadata: meta(&[
                        ("federation", Value::Bool(true)),
                        ("reason", Value::String("shared_contract_operation".into())),
                    ]),
                    line: None,
                    column: None,
                    provenance: Some(Provenance::Heuristic),
                    confidence: Some(EdgeConfidence::Inferred),
                });
            }
        }
    }

    // Link contract ops to existing Route nodes with matching names/paths.
    // Contract nodes are skipped: they are linked through their shared key above.
    let mut seen_route_links = HashSet::new();
    let mut hits_by_name: HashMap<&str, Vec<String>> = HashMap::new();
    for op in &ops {
        if !hits_by_name.contains_key(op.name.as_str()) {
            let hits = queries
                .get_node_ids_by_symbol(&op.name, 4)
                .await
                .unwrap_or_default();
            hits_by_name.insert(op.name.as_str(), hits);
        }
        for target in &hits_by_name[op.name.as_str()] {
            if target.starts_with("contract:")
                || !seen_route_links.insert((op.id.clone(), target.clone()))
            {
                continue;
            }
            let target = target.clone();
            edges.push(Edge {
                source: op.id.clone(),
                target,
                kind: EdgeKind::References,
                metadata: meta(&[
                    ("federation", Value::Bool(true)),
                    ("reason", Value::String("route_match".into())),
                ]),
                line: None,
                column: None,
                provenance: Some(Provenance::Heuristic),
                confidence: Some(EdgeConfidence::Ambiguous),
            });
        }
    }

    if !edges.is_empty() {
        queries.upsert_edges(&edges).await?;
    }
    Ok(ops.len())
}

/// True when `rel_path` has an extension the contract pass may index
/// (`.proto`, `.graphql`, `.gql`, or a YAML/JSON file that may be OpenAPI).
pub fn may_be_contract_path(rel_path: &str) -> bool {
    let ext = Path::new(rel_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(
        ext.as_str(),
        "proto" | "graphql" | "gql" | "yaml" | "yml" | "json"
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ContractKind {
    OpenApi,
    Protobuf,
    GraphQl,
}

struct ScannedContract {
    rel: String,
    full: PathBuf,
    kind: ContractKind,
}

/// Bytes read from a YAML/JSON file to look for an `openapi` / `swagger` key.
const SNIFF_BYTES: u64 = 400;

/// The first [`SNIFF_BYTES`] of `path` as text. Only the head is read, so large
/// data files cost one small read and bytes after the head may be any encoding.
fn contract_head(path: &Path) -> Option<String> {
    let mut head = Vec::with_capacity(SNIFF_BYTES as usize);
    std::fs::File::open(path)
        .ok()?
        .take(SNIFF_BYTES)
        .read_to_end(&mut head)
        .ok()?;
    Some(String::from_utf8_lossy(&head).into_owned())
}

fn scan_contract_files(project_root: &Path, exclude: &[String]) -> Vec<ScannedContract> {
    let exclude_matcher = crate::orchestrator::build_exclude_matcher(project_root, exclude);
    let walker = WalkBuilder::new(project_root)
        .hidden(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !SKIP_DIRS.contains(&name.as_ref())
        })
        .build();
    let mut out = Vec::new();
    for entry in walker.flatten() {
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let path = entry.path();
        let rel = path
            .strip_prefix(project_root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        if exclude_matcher
            .as_ref()
            .is_some_and(|m| m.matched_path_or_any_parents(&rel, false).is_ignore())
        {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        let kind = if ext == "proto" {
            Some(ContractKind::Protobuf)
        } else if ext == "graphql" || ext == "gql" {
            Some(ContractKind::GraphQl)
        } else if matches!(ext.as_str(), "yaml" | "yml" | "json")
            && (name.contains("openapi")
                || name.contains("swagger")
                || name == "api.yaml"
                || name == "api.yml"
                || name == "api.json")
        {
            Some(ContractKind::OpenApi)
        } else if matches!(ext.as_str(), "yaml" | "yml" | "json") {
            // Content sniff for openapi key — cheap peek.
            if let Some(head) = contract_head(path) {
                if head.contains("openapi:")
                    || head.contains("\"openapi\"")
                    || head.contains("swagger:")
                    || head.contains("\"swagger\"")
                {
                    Some(ContractKind::OpenApi)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        if let Some(kind) = kind {
            out.push(ScannedContract {
                rel,
                full: path.to_path_buf(),
                kind,
            });
        }
    }
    out
}

fn parse_openapi(rel: &str, content: &str) -> Vec<ContractOp> {
    let value = if rel.ends_with(".json") {
        serde_json::from_str::<Value>(content).ok()
    } else {
        serde_yaml::from_str::<Value>(content).ok()
    };
    let Some(root) = value else {
        return Vec::new();
    };
    let Some(paths) = root.get("paths").and_then(|p| p.as_object()) else {
        return Vec::new();
    };
    let mut ops = Vec::new();
    for (path, methods) in paths {
        let Some(methods) = methods.as_object() else {
            continue;
        };
        for (method, body) in methods {
            let m = method.to_ascii_uppercase();
            if !matches!(
                m.as_str(),
                "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "OPTIONS" | "HEAD"
            ) {
                continue;
            }
            let op_id = body
                .get("operationId")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("{m}:{path}"));
            ops.push(ContractOp {
                id: format!("contract:openapi:{rel}:{m}:{path}"),
                key: format!("contract:openapi:{m}:{path}"),
                name: op_id,
                kind: "openapi",
                file: rel.to_string(),
                line: 1,
            });
        }
    }
    ops
}

fn parse_protobuf(rel: &str, content: &str) -> Vec<ContractOp> {
    let mut ops = Vec::new();
    let mut current_service: Option<String> = None;
    for (idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("service ") {
            let name = rest
                .split(|c: char| c == '{' || c.is_whitespace())
                .next()
                .unwrap_or("")
                .to_string();
            if !name.is_empty() {
                current_service = Some(name);
            }
            continue;
        }
        if trimmed.starts_with('}') {
            // naive: closing brace may end service
            if current_service.is_some() && !trimmed.contains("rpc") {
                // keep service until next service keyword; ignore
            }
        }
        if let Some(rest) = trimmed.strip_prefix("rpc ") {
            let rpc = rest
                .split(|c: char| c == '(' || c.is_whitespace())
                .next()
                .unwrap_or("")
                .to_string();
            if rpc.is_empty() {
                continue;
            }
            let svc = current_service.clone().unwrap_or_else(|| "Service".into());
            let qn = format!("{svc}.{rpc}");
            ops.push(ContractOp {
                id: format!("contract:proto:{rel}:{qn}"),
                key: format!("contract:proto:{qn}"),
                name: qn,
                kind: "protobuf",
                file: rel.to_string(),
                line: (idx + 1) as i32,
            });
        }
    }
    ops
}

fn parse_graphql(rel: &str, content: &str) -> Vec<ContractOp> {
    let mut ops = Vec::new();
    let mut in_type: Option<String> = None;
    for (idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed
            .strip_prefix("type ")
            .or_else(|| trimmed.strip_prefix("extend type "))
        {
            let name = rest
                .split(|c: char| c == '{' || c.is_whitespace())
                .next()
                .unwrap_or("")
                .to_string();
            if matches!(name.as_str(), "Query" | "Mutation" | "Subscription") {
                in_type = Some(name);
            } else {
                in_type = None;
            }
            continue;
        }
        if trimmed.starts_with('}') {
            in_type = None;
            continue;
        }
        let Some(ty) = in_type.clone() else {
            continue;
        };
        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }
        let field = trimmed
            .split(|c: char| c == '(' || c == ':' || c.is_whitespace())
            .next()
            .unwrap_or("")
            .to_string();
        if field.is_empty() || field.starts_with('"') {
            continue;
        }
        let qn = format!("{ty}.{field}");
        ops.push(ContractOp {
            id: format!("contract:graphql:{rel}:{qn}"),
            key: format!("contract:graphql:{qn}"),
            name: qn,
            kind: "graphql",
            file: rel.to_string(),
            line: (idx + 1) as i32,
        });
    }
    ops
}

fn contract_doc_node(rel: &str, kind: ContractKind, now: i64) -> Node {
    let label = match kind {
        ContractKind::OpenApi => "OpenAPI",
        ContractKind::Protobuf => "Protobuf",
        ContractKind::GraphQl => "GraphQL",
    };
    let name = rel.rsplit('/').next().unwrap_or(rel).to_string();
    Node {
        id: format!("doc:{rel}"),
        kind: NodeKind::Doc,
        name,
        qualified_name: format!("{label}:{rel}"),
        file_path: rel.to_string(),
        language: Language::Unknown,
        start_line: 1,
        end_line: 1,
        start_column: 0,
        end_column: 0,
        docstring: Some(format!("{label} contract")),
        signature: None,
        visibility: None,
        is_exported: None,
        is_async: None,
        is_static: None,
        is_abstract: None,
        decorators: None,
        type_parameters: None,
        return_type: None,
        updated_at: now,
    }
}

fn op_node(op: &ContractOp, now: i64) -> Node {
    Node {
        id: op.id.clone(),
        kind: NodeKind::Route,
        name: op.name.clone(),
        qualified_name: op.id.clone(),
        file_path: op.file.clone(),
        language: Language::Unknown,
        start_line: op.line,
        end_line: op.line,
        start_column: 0,
        end_column: 0,
        docstring: Some(format!("{} contract operation", op.kind)),
        signature: Some(op.key.clone()),
        visibility: None,
        is_exported: Some(true),
        is_async: None,
        is_static: None,
        is_abstract: None,
        decorators: None,
        type_parameters: None,
        return_type: None,
        updated_at: now,
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_openapi_json() {
        let json = r#"{
          "openapi": "3.0.0",
          "paths": {
            "/users": {
              "get": { "operationId": "listUsers" },
              "post": { "operationId": "createUser" }
            }
          }
        }"#;
        let ops = parse_openapi("openapi.json", json);
        assert_eq!(ops.len(), 2);
        assert!(ops.iter().any(|o| o.key == "contract:openapi:GET:/users"));
        assert!(ops
            .iter()
            .any(|o| o.id == "contract:openapi:openapi.json:GET:/users"));
        assert!(ops.iter().any(|o| o.name == "listUsers"));
    }

    #[test]
    fn parses_proto_rpc() {
        let proto = r#"
syntax = "proto3";
service UserService {
  rpc GetUser (GetUserRequest) returns (User);
  rpc ListUsers (ListRequest) returns (ListReply);
}
"#;
        let ops = parse_protobuf("user.proto", proto);
        assert_eq!(ops.len(), 2);
        assert!(ops
            .iter()
            .any(|o| o.key == "contract:proto:UserService.GetUser"
                && o.id == "contract:proto:user.proto:UserService.GetUser"));
    }

    #[test]
    fn sniff_does_not_panic_when_byte_400_splits_a_multibyte_char() {
        let dir = std::env::temp_dir().join(format!(
            "ax-contract-head-{}-{}",
            std::process::id(),
            "split"
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut body = "a".repeat(399);
        body.push('ï');
        body.push_str("\nopenapi: 3.0.0\n");
        std::fs::write(dir.join("service.yaml"), &body).unwrap();
        let found = scan_contract_files(&dir, &[]);
        assert!(
            found.is_empty(),
            "the openapi marker sits past the 400-byte peek"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn sniff_still_detects_openapi_before_a_multibyte_char() {
        let dir = std::env::temp_dir().join(format!(
            "ax-contract-head-{}-{}",
            std::process::id(),
            "detect"
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut body = "openapi: 3.0.0\n".to_string();
        body.push_str(&"a".repeat(399 - body.len()));
        body.push('ï');
        std::fs::write(dir.join("service.yaml"), &body).unwrap();
        let found = scan_contract_files(&dir, &[]);
        assert_eq!(found.len(), 1);
        assert!(matches!(found[0].kind, ContractKind::OpenApi));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn scan_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ax-contract-scan-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn sniff_reads_only_the_head_so_later_non_utf8_bytes_do_not_hide_openapi() {
        let dir = scan_dir("non-utf8");
        let mut body = b"openapi: 3.0.0\npaths: {}\n".to_vec();
        body.resize(600, b' ');
        body.extend_from_slice(&[0xff, 0xfe, 0x00, 0x80]);
        std::fs::write(dir.join("service.yaml"), &body).unwrap();
        let found = scan_contract_files(&dir, &[]);
        assert_eq!(found.len(), 1, "openapi head must be detected");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn exclude_patterns_use_gitignore_semantics() {
        let dir = scan_dir("exclude");
        for sub in ["general", "generated", "a/generated", "keep"] {
            std::fs::create_dir_all(dir.join(sub)).unwrap();
            std::fs::write(dir.join(sub).join("openapi.yaml"), "openapi: 3.0.0\n").unwrap();
        }
        let rels = |exclude: &[&str]| -> Vec<String> {
            let exclude: Vec<String> = exclude.iter().map(|s| s.to_string()).collect();
            let mut v: Vec<String> = scan_contract_files(&dir, &exclude)
                .into_iter()
                .map(|f| f.rel)
                .collect();
            v.sort();
            v
        };
        // A plain name matches a path component, not any substring.
        assert_eq!(
            rels(&["gen"]),
            vec![
                "a/generated/openapi.yaml",
                "general/openapi.yaml",
                "generated/openapi.yaml",
                "keep/openapi.yaml"
            ]
        );
        // A directory pattern excludes files below it at any depth.
        assert_eq!(
            rels(&["generated/"]),
            vec!["general/openapi.yaml", "keep/openapi.yaml"]
        );
        assert_eq!(
            rels(&["**/generated/**"]),
            vec!["general/openapi.yaml", "keep/openapi.yaml"]
        );
        // Anchored patterns only match from the project root.
        assert_eq!(
            rels(&["/generated"]),
            vec![
                "a/generated/openapi.yaml",
                "general/openapi.yaml",
                "keep/openapi.yaml"
            ]
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn parses_graphql_query_fields() {
        let gql = r#"
type Query {
  user(id: ID!): User
  users: [User!]!
}
type Mutation {
  createUser(input: CreateUserInput!): User
}
"#;
        let ops = parse_graphql("schema.graphql", gql);
        assert!(ops.iter().any(|o| o.name == "Query.user"));
        assert!(ops.iter().any(|o| o.name == "Mutation.createUser"));
    }
}
