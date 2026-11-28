//! Resolve unresolved refs via LSP definition and write Exact edges.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ax_types::{Edge, EdgeConfidence, EdgeKind, Provenance, ReferenceKind, UnresolvedReference};

use crate::client::{column_for_name, language_id_for_ext, LspClient};
use crate::servers::{spec_for_extension, ServerResolver, ServerSpec};

#[derive(Debug, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrichReport {
    pub examined: u32,
    pub resolved: u32,
    pub skipped_no_server: u32,
    pub skipped_no_definition: u32,
    /// Refs left for the next run because the batch hit [`ENRICH_DEADLINE`].
    pub skipped_deadline: u32,
    pub errors: Vec<String>,
}

/// Longest time one enrich batch may spend on language-server work.
pub(crate) const ENRICH_DEADLINE: Duration = Duration::from_secs(15 * 60);

pub(crate) struct FileEnrich {
    pub edges: Vec<Edge>,
    pub resolved_refs: Vec<UnresolvedReference>,
    pub resolved: u32,
    pub skipped_no_definition: u32,
    pub skipped_deadline: u32,
    pub errors: Vec<String>,
}

/// One file prepared for enrichment (relative path, absolute path, language id, refs).
pub(crate) type EnrichFileJob = (String, PathBuf, String, Vec<UnresolvedReference>);

/// Enrich many files while reusing one language-server process per server id.
/// Spawning rust-analyzer per file is unusable on large Cargo workspaces.
///
/// Each server is resolved and started at most once per batch; a server that
/// fails to start is not retried for later files. Work stops at `deadline`.
pub(crate) fn enrich_files_blocking(
    project_root: &Path,
    files: Vec<EnrichFileJob>,
    resolver: &mut ServerResolver,
    deadline: Instant,
) -> (EnrichReport, Vec<FileEnrich>) {
    let mut report = EnrichReport::default();
    let mut results = Vec::new();
    let mut sessions: HashMap<&'static str, LspClient> = HashMap::new();
    let mut failed: HashSet<&'static str> = HashSet::new();

    for (rel, full, language_id, refs) in files {
        report.examined += refs.len() as u32;
        if Instant::now() >= deadline {
            report.skipped_deadline += refs.len() as u32;
            continue;
        }
        let ext = Path::new(&rel)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        let Some(spec) = spec_for_extension(ext) else {
            report.skipped_no_server += refs.len() as u32;
            continue;
        };
        if failed.contains(spec.id) {
            report.skipped_no_server += refs.len() as u32;
            continue;
        }
        let Some(bin) = resolver.binary(spec) else {
            report.skipped_no_server += refs.len() as u32;
            continue;
        };

        if let Err(e) = ensure_session(&mut sessions, spec, &bin, project_root, deadline) {
            report.errors.push(format!("{rel}: {e}"));
            report.skipped_no_server += refs.len() as u32;
            failed.insert(spec.id);
            continue;
        }
        let client = sessions.get_mut(spec.id).unwrap();

        match enrich_file_with_client(client, project_root, &full, &language_id, &refs, deadline) {
            Ok(p) => {
                report.resolved += p.resolved;
                report.skipped_no_definition += p.skipped_no_definition;
                report.skipped_deadline += p.skipped_deadline;
                report.errors.extend(p.errors.iter().cloned());
                results.push(p);
            }
            Err(e) => report.errors.push(format!("{rel}: {e}")),
        }
    }

    if report.skipped_deadline > 0 {
        report.errors.push(format!(
            "enrich stopped at its time limit; {} refs are left for the next run",
            report.skipped_deadline
        ));
    }
    for (_, client) in sessions {
        client.shutdown();
    }
    (report, results)
}

fn ensure_session(
    sessions: &mut HashMap<&'static str, LspClient>,
    spec: &'static ServerSpec,
    bin: &Path,
    project_root: &Path,
    deadline: Instant,
) -> Result<(), String> {
    if sessions.contains_key(spec.id) {
        return Ok(());
    }
    let mut client = LspClient::start(spec, bin, project_root)?;
    // Large Cargo workspaces need a long first index before definition works.
    let warmup = if spec.id == "rust-analyzer" {
        Duration::from_secs(180)
    } else {
        Duration::from_secs(8)
    };
    client.wait_ready(warmup.min(deadline.saturating_duration_since(Instant::now())))?;
    sessions.insert(spec.id, client);
    Ok(())
}

pub(crate) fn enrich_file_with_client(
    client: &mut LspClient,
    project_root: &Path,
    full_path: &Path,
    language_id: &str,
    refs: &[UnresolvedReference],
    deadline: Instant,
) -> Result<FileEnrich, String> {
    let content = std::fs::read_to_string(full_path).map_err(|e| e.to_string())?;
    client.did_open(full_path, language_id, &content)?;
    // Give the server a moment to ingest the buffer; RA may briefly leave quiescent.
    client.wait_ready(Duration::from_secs(3))?;
    collect_definitions(client, project_root, full_path, &content, refs, deadline)
}

fn collect_definitions(
    client: &mut LspClient,
    project_root: &Path,
    full_path: &Path,
    content: &str,
    refs: &[UnresolvedReference],
    deadline: Instant,
) -> Result<FileEnrich, String> {
    let mut out = FileEnrich {
        edges: Vec::new(),
        resolved_refs: Vec::new(),
        resolved: 0,
        skipped_no_definition: 0,
        skipped_deadline: 0,
        errors: Vec::new(),
    };

    for r in refs {
        if Instant::now() >= deadline {
            out.skipped_deadline += 1;
            continue;
        }
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

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    fn write_exec(path: &Path, body: &str) {
        fs::write(path, body).unwrap();
        let mut p = fs::metadata(path).unwrap().permissions();
        p.set_mode(0o755);
        fs::set_permissions(path, p).unwrap();
    }

    fn unresolved(name: &str, file: &str) -> UnresolvedReference {
        UnresolvedReference {
            from_node_id: "n1".into(),
            reference_name: name.into(),
            reference_kind: ReferenceKind::Calls,
            line: 1,
            column: 0,
            file_path: Some(file.into()),
            language: None,
            candidates: None,
        }
    }

    /// A server that answers version probes and dies when started for LSP work.
    /// Every launch appends one line to `log`.
    fn logging_server(bin_dir: &Path, command: &str, log: &Path) {
        fs::create_dir_all(bin_dir).unwrap();
        write_exec(
            &bin_dir.join(command),
            &format!(
                "#!/bin/sh\necho \"$@\" >> '{}'\ncase \"$1\" in --version|version) echo 1.0; exit 0;; esac\nexit 1\n",
                log.display()
            ),
        );
    }

    fn jobs_for(root: &Path, files: &[&str]) -> Vec<EnrichFileJob> {
        files
            .iter()
            .map(|f| {
                let full = root.join(f);
                fs::write(&full, "foo();\n").unwrap();
                (
                    f.to_string(),
                    full,
                    "typescript".to_string(),
                    vec![unresolved("foo", f)],
                )
            })
            .collect()
    }

    #[test]
    fn server_is_resolved_and_started_once_per_batch() {
        let root = tempfile::tempdir().unwrap();
        let log = root.path().join("launches.log");
        let bin = root.path().join("bin");
        logging_server(&bin, "typescript-language-server", &log);
        let jobs = jobs_for(root.path(), &["a.ts", "b.ts", "c.ts", "d.ts"]);
        let mut resolver = ServerResolver::only_dirs(vec![bin]);
        let deadline = Instant::now() + ENRICH_DEADLINE;
        let (report, _) = enrich_files_blocking(root.path(), jobs, &mut resolver, deadline);
        assert_eq!(report.examined, 4);
        let launches = fs::read_to_string(&log).unwrap_or_default();
        let count = launches.lines().count();
        // One version probe and one failed start for the whole batch.
        assert!(
            count <= 2,
            "server launched {count} times for 4 files:\n{launches}"
        );
    }

    #[test]
    fn work_past_the_deadline_is_left_for_the_next_run() {
        let root = tempfile::tempdir().unwrap();
        let log = root.path().join("launches.log");
        let bin = root.path().join("bin");
        logging_server(&bin, "typescript-language-server", &log);
        let jobs = jobs_for(root.path(), &["a.ts", "b.ts"]);
        let mut resolver = ServerResolver::only_dirs(vec![bin]);
        let (report, results) =
            enrich_files_blocking(root.path(), jobs, &mut resolver, Instant::now());
        assert_eq!(report.examined, 2);
        assert_eq!(report.skipped_deadline, 2);
        assert!(results.is_empty());
        assert!(!log.exists(), "no server may start after the deadline");
        assert!(report.errors.iter().any(|e| e.contains("time limit")));
    }
}
