//! Resolve unresolved refs via LSP definition and write Exact edges.

use std::path::Path;
use std::time::Duration;

use ax_types::{
    Edge, EdgeConfidence, EdgeKind, Provenance, ReferenceKind, UnresolvedReference,
};

use crate::client::{language_id_for_ext, LspClient};
use crate::servers::ServerSpec;

#[derive(Debug, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrichReport {
    pub examined: u32,
    pub resolved: u32,
    pub skipped_no_server: u32,
    pub skipped_no_definition: u32,
    pub errors: Vec<String>,
}

pub(crate) struct FileEnrich {
    pub edges: Vec<Edge>,
    pub resolved_refs: Vec<UnresolvedReference>,
    pub resolved: u32,
    pub skipped_no_definition: u32,
    pub errors: Vec<String>,
}

pub(crate) fn enrich_file_blocking(
    project_root: &Path,
    full_path: &Path,
    language_id: &str,
    spec: &'static ServerSpec,
    content: &str,
    refs: &[UnresolvedReference],
) -> Result<FileEnrich, String> {
    let mut client = LspClient::start(spec, project_root)?;
    client.did_open(full_path, language_id, content)?;
    std::thread::sleep(Duration::from_millis(400));

    let mut out = FileEnrich {
        edges: Vec::new(),
        resolved_refs: Vec::new(),
        resolved: 0,
        skipped_no_definition: 0,
        errors: Vec::new(),
    };

    for r in refs {
        let line = r.line.max(1) as u32 - 1;
        let col = r.column.max(0) as u32;
        match client.definition(full_path, line, col, Duration::from_secs(8)) {
            Ok(Some(loc)) => {
                let target_rel = relativize(project_root, &loc.path);
                out.edges.push(Edge {
                    source: r.from_node_id.clone(),
                    target: format!("lsp:{}:{}", target_rel, loc.line + 1),
                    kind: reference_kind_to_edge(r.reference_kind),
                    metadata: Some(std::collections::HashMap::from([(
                        "lspTarget".into(),
                        serde_json::json!({
                            "path": target_rel,
                            "line": loc.line + 1,
                            "character": loc.character,
                        }),
                    )])),
                    line: Some(r.line),
                    column: Some(r.column),
                    provenance: Some(Provenance::Lsp),
                    confidence: Some(EdgeConfidence::Exact),
                });
                out.resolved_refs.push(r.clone());
                out.resolved += 1;
            }
            Ok(None) => out.skipped_no_definition += 1,
            Err(e) => out.errors.push(e),
        }
    }
    client.shutdown();
    Ok(out)
}

pub fn language_id(ext: &str) -> &'static str {
    language_id_for_ext(ext)
}

fn relativize(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn reference_kind_to_edge(kind: ReferenceKind) -> EdgeKind {
    match kind {
        ReferenceKind::Calls => EdgeKind::Calls,
        ReferenceKind::Imports => EdgeKind::Imports,
        ReferenceKind::References | ReferenceKind::FunctionRef => EdgeKind::References,
        ReferenceKind::TypeOf => EdgeKind::TypeOf,
        ReferenceKind::Extends => EdgeKind::Extends,
        ReferenceKind::Implements => EdgeKind::Implements,
        ReferenceKind::Exports => EdgeKind::Exports,
        ReferenceKind::Returns => EdgeKind::Returns,
        ReferenceKind::Instantiates => EdgeKind::Instantiates,
        ReferenceKind::Overrides => EdgeKind::Overrides,
        ReferenceKind::Decorates => EdgeKind::Decorates,
        ReferenceKind::Contains => EdgeKind::Contains,
    }
}
