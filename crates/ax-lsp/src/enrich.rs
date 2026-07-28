//! Resolve unresolved refs via LSP definition and write Exact edges.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use ax_types::{
    Edge, EdgeConfidence, EdgeKind, Provenance, ReferenceKind, UnresolvedReference,
};

use crate::client::{column_for_name, language_id_for_ext, LspClient};
use crate::servers::{server_available, spec_for_extension, ServerSpec};

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

/// One file prepared for enrichment (relative path, absolute path, language id, refs).
pub(crate) type EnrichFileJob = (String, PathBuf, String, Vec<UnresolvedReference>);

/// Enrich many files while reusing one language-server process per server id.
/// Spawning rust-analyzer per file is unusable on large Cargo workspaces.
pub(crate) fn enrich_files_blocking(
    project_root: &Path,
    files: Vec<EnrichFileJob>,
) -> (EnrichReport, Vec<FileEnrich>) {
    let mut report = EnrichReport::default();
    let mut results = Vec::new();
    let mut sessions: HashMap<&'static str, LspClient> = HashMap::new();

    for (rel, full, language_id, refs) in files {
        report.examined += refs.len() as u32;
        let ext = Path::new(&rel)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        let Some(spec) = spec_for_extension(ext) else {
            report.skipped_no_server += refs.len() as u32;
            continue;
        };
        if !server_available(spec) {
            report.skipped_no_server += refs.len() as u32;
            continue;
        }

        if let Err(e) = ensure_session(&mut sessions, spec, project_root) {
            report.errors.push(format!("{rel}: {e}"));
            continue;
        }
        let client = sessions.get_mut(spec.id).unwrap();

        match enrich_file_with_client(client, project_root, &full, &language_id, &refs) {
            Ok(p) => {
                report.resolved += p.resolved;
                report.skipped_no_definition += p.skipped_no_definition;
                report.errors.extend(p.errors.iter().cloned());
                results.push(p);
            }
            Err(e) => report.errors.push(format!("{rel}: {e}")),
        }
    }

    for (_, client) in sessions {
        client.shutdown();
    }
    (report, results)
}

fn ensure_session(
    sessions: &mut HashMap<&'static str, LspClient>,
    spec: &'static ServerSpec,
    project_root: &Path,
) -> Result<(), String> {
    if sessions.contains_key(spec.id) {
        return Ok(());
    }
    let mut client = LspClient::start(spec, project_root)?;
    // Large Cargo workspaces need a long first index before definition works.
    let warmup = if spec.id == "rust-analyzer" {
        Duration::from_secs(180)
    } else {
        Duration::from_secs(8)
    };
    client.wait_ready(warmup)?;
    sessions.insert(spec.id, client);
    Ok(())
}

pub(crate) fn enrich_file_with_client(
    client: &mut LspClient,
    project_root: &Path,
    full_path: &Path,
    language_id: &str,
    refs: &[UnresolvedReference],
) -> Result<FileEnrich, String> {
    let content = std::fs::read_to_string(full_path).map_err(|e| e.to_string())?;
    client.did_open(full_path, language_id, &content)?;
    // Give the server a moment to ingest the buffer; RA may briefly leave quiescent.
    client.wait_ready(Duration::from_secs(3))?;
    collect_definitions(client, project_root, full_path, &content, refs)
}

fn collect_definitions(
    client: &mut LspClient,
    project_root: &Path,
    full_path: &Path,
    content: &str,
    refs: &[UnresolvedReference],
) -> Result<FileEnrich, String> {
    let mut out = FileEnrich {
        edges: Vec::new(),
        resolved_refs: Vec::new(),
        resolved: 0,
        skipped_no_definition: 0,
        errors: Vec::new(),
    };

    for r in refs {
        let line = r.line.max(1) as u32 - 1;
        let col = column_for_name(content, r.line, r.column, &r.reference_name);
        match client.definition_ready(full_path, line, col) {
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
    Ok(out)
}

pub fn language_id(ext: &str) -> &'static str {
    language_id_for_ext(ext)
}

fn relativize(root: &Path, path: &Path) -> String {
    let root = crate::client::strip_verbatim_prefix(
        &root.canonicalize().unwrap_or_else(|_| root.to_path_buf()),
    );
    let path = crate::client::strip_verbatim_prefix(
        &path.canonicalize().unwrap_or_else(|_| path.to_path_buf()),
    );
    path.strip_prefix(&root)
        .unwrap_or(&path)
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
