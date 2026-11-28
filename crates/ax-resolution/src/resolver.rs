//! Main reference resolver.

use std::collections::HashMap;

use ax_db::queries::QueryBuilder;
use ax_types::{Edge, IndexPhase, IndexProgress, Provenance, ReferenceKind, UnresolvedReference};

use crate::c_fnptr_synthesizer::CFnptrSynthesizer;
use crate::callback_synthesizer::CallbackSynthesizer;
use crate::framework_resolve;
use crate::frameworks::FrameworkRegistry;
use crate::import_resolver::ImportResolver;
use crate::name_matcher::NameMatcher;
use crate::types::{ResolutionResult, ResolutionStats, ResolvedBy, ResolvedRef, UnresolvedRef};

/// Which unresolved references to process. `None` = full pass (e.g. `ax index`).
pub type ResolutionScope<'a> = Option<&'a [String]>;

pub struct ReferenceResolver {
    import_resolver: ImportResolver,
    name_matcher: NameMatcher,
    callback_synth: CallbackSynthesizer,
    c_fnptr_synth: CFnptrSynthesizer,
    frameworks: FrameworkRegistry,
}

impl ReferenceResolver {
    pub fn new(project_root: &std::path::Path) -> Self {
        Self {
            import_resolver: ImportResolver::new(project_root),
            name_matcher: NameMatcher::new(),
            callback_synth: CallbackSynthesizer::new(),
            c_fnptr_synth: CFnptrSynthesizer::new(),
            frameworks: FrameworkRegistry::new(),
        }
    }

    pub async fn resolve_all(
        &mut self,
        queries: &QueryBuilder,
        scope: ResolutionScope<'_>,
        mut on_progress: Option<&mut Box<dyn FnMut(IndexProgress) + Send>>,
    ) -> Result<ResolutionResult, ax_utils::errors::AxError> {
        let gates = self
            .frameworks
            .run_post_extract(self.import_resolver.project_root(), queries, scope)
            .await?;

        let mut refs = match scope {
            Some(files) if !files.is_empty() => queries.get_unresolved_refs_by_files(files).await?,
            _ => queries.get_unresolved_refs().await?,
        };
        refs.retain(|reference| !ax_db::replication::is_replica(&reference.from_node_id));
        let total = refs.len() as u32;
        if let Some(ref mut cb) = on_progress {
            let label = match scope {
                Some(files) if !files.is_empty() => {
                    format!("{total} references to resolve in {} file(s)", files.len())
                }
                _ => format!("{total} references to resolve"),
            };
            cb(IndexProgress {
                phase: IndexPhase::Resolving,
                current: 0,
                total,
                file_path: Some(label),
            });
        }
        let import_map = build_import_map(&refs, &self.import_resolver);

        let mut deferred_refs: Vec<UnresolvedReference> = Vec::new();
        let mut to_delete: Vec<UnresolvedReference> = Vec::new();

        let mut result = ResolutionResult {
            resolved: vec![],
            unresolved: vec![],
            stats: ResolutionStats::default(),
        };

        // Nothing below writes nodes until the edges are stored, so node
        // lookups can be memoized for the whole loop.
        let reads = queries.with_node_read_cache();
        for (i, db_ref) in refs.into_iter().enumerate() {
            if i > 0 && i % 500 == 0 {
                if let Some(ref mut cb) = on_progress {
                    cb(IndexProgress {
                        phase: IndexPhase::Resolving,
                        current: i as u32,
                        total,
                        file_path: None,
                    });
                }
            }
            let ref_ = unresolved_from_db(&db_ref);
            result.stats.total += 1;

            if ref_.reference_kind == ReferenceKind::Calls && needs_deferred_resolution(&ref_) {
                deferred_refs.push(db_ref);
                continue;
            }

            let resolved = match ref_.reference_kind {
                ReferenceKind::FunctionRef => {
                    self.name_matcher.match_function_ref(&reads, &ref_).await
                }
                ReferenceKind::Imports => self.resolve_import_ref(&reads, &ref_).await,
                _ => {
                    if let Some(r) = framework_resolve::try_resolve(
                        self.import_resolver.project_root(),
                        &reads,
                        &ref_,
                        gates,
                    )
                    .await
                    {
                        Some(r)
                    } else if ref_.reference_kind == ReferenceKind::Calls {
                        if let Some(target_file) =
                            import_map.get(&(ref_.file_path.clone(), ref_.reference_name.clone()))
                        {
                            self.name_matcher
                                .resolve_in_file(&reads, &ref_, target_file)
                                .await
                        } else {
                            self.name_matcher.resolve_ref(&reads, &ref_).await
                        }
                    } else {
                        self.name_matcher.resolve_ref(&reads, &ref_).await
                    }
                }
            };

            if let Some(r) = resolved {
                result.stats.resolved += 1;
                let method = format!("{:?}", r.resolved_by);
                *result.stats.by_method.entry(method).or_insert(0) += 1;
                result.resolved.push(r);
                to_delete.push(db_ref);
            } else {
                result.stats.unresolved += 1;
                result.unresolved.push(ref_);
            }
        }

        let deferred_count = deferred_refs.len() as u32;
        for (j, db_ref) in deferred_refs.into_iter().enumerate() {
            if j > 0 && j % 500 == 0 {
                if let Some(ref mut cb) = on_progress {
                    cb(IndexProgress {
                        phase: IndexPhase::Resolving,
                        current: total.saturating_sub(deferred_count) + j as u32,
                        total,
                        file_path: Some("deferred calls".into()),
                    });
                }
            }
            let ref_ = unresolved_from_db(&db_ref);
            if let Some(r) = self.name_matcher.resolve_deferred_call(&reads, &ref_).await {
                result.stats.resolved += 1;
                let method = format!("{:?}", r.resolved_by);
                *result.stats.by_method.entry(method).or_insert(0) += 1;
                result.resolved.push(r);
                to_delete.push(db_ref);
            } else {
                result.stats.unresolved += 1;
                result.unresolved.push(ref_);
            }
        }

        // A framework resolver that resolves a reference to its own source (a
        // compiler macro such as `defineProps`, a Svelte rune, a virtual
        // module) has handled it: its row goes, but it stands for no edge.
        // Any other self-resolution is real recursion and keeps its edge.
        result
            .resolved
            .retain(|r| !(r.resolved_by == ResolvedBy::Framework && is_self_resolution(r)));
        // One transaction for all edges instead of one commit per reference.
        // Only drop the unresolved rows once their edges are stored.
        let edges: Vec<Edge> = result.resolved.iter().map(resolved_edge).collect();
        queries.upsert_edges(&edges).await?;
        if let Err(e) = queries.delete_unresolved_refs_batch(&to_delete).await {
            tracing::warn!("failed to delete resolved refs: {}", e);
        }
        self.callback_synth
            .synthesize(self.import_resolver.project_root(), queries, scope)
            .await?;

        self.c_fnptr_synth
            .synthesize(self.import_resolver.project_root(), queries, scope)
            .await?;

        Ok(result)
    }

    pub async fn resolve_for_files(
        &mut self,
        queries: &QueryBuilder,
        files: &[String],
        on_progress: Option<&mut Box<dyn FnMut(IndexProgress) + Send>>,
    ) -> Result<ResolutionResult, ax_utils::errors::AxError> {
        self.resolve_all(queries, Some(files), on_progress).await
    }

    async fn resolve_import_ref(
        &self,
        queries: &QueryBuilder,
        ref_: &UnresolvedRef,
    ) -> Option<ResolvedRef> {
        let module = ref_
            .candidates
            .as_ref()
            .and_then(|c| c.first())
            .map(|s| s.as_str())
            .unwrap_or(&ref_.reference_name);

        let target_file = self
            .import_resolver
            .resolve_to_indexed_file(&ref_.file_path, module)?;

        let nodes = queries
            .get_nodes_by_file(&target_file)
            .await
            .unwrap_or_default();
        let binding = &ref_.reference_name;

        let target = nodes
            .iter()
            .find(|n| {
                n.name == *binding
                    && (matches!(
                        n.kind,
                        ax_types::NodeKind::Function
                            | ax_types::NodeKind::Method
                            | ax_types::NodeKind::Class
                            | ax_types::NodeKind::Interface
                    ))
            })
            .or_else(|| nodes.iter().find(|n| n.kind == ax_types::NodeKind::File))?;

        // The edge is written later in one batch (see `resolved_edge`).
        Some(ResolvedRef {
            original: ref_.clone(),
            target_node_id: target.id.clone(),
            confidence: 1.0,
            resolved_by: ResolvedBy::Import,
        })
    }
}

fn build_import_map(
    refs: &[UnresolvedReference],
    import_resolver: &ImportResolver,
) -> HashMap<(String, String), String> {
    let mut map = HashMap::new();
    for r in refs {
        if r.reference_kind != ReferenceKind::Imports {
            continue;
        }
        let file_path = r.file_path.clone().unwrap_or_default();
        if file_path.is_empty() {
            continue;
        }
        let module = r
            .candidates
            .as_ref()
            .and_then(|c| c.first())
            .map(|s| s.as_str())
            .unwrap_or(&r.reference_name);
        if let Some(resolved) = import_resolver.resolve_to_indexed_file(&file_path, module) {
            map.insert((file_path, r.reference_name.clone()), resolved);
        }
    }
    map
}

fn needs_deferred_resolution(ref_: &UnresolvedRef) -> bool {
    ref_.reference_name.starts_with("this.") || ref_.reference_name.contains("().")
}

fn unresolved_from_db(r: &UnresolvedReference) -> UnresolvedRef {
    UnresolvedRef {
        from_node_id: r.from_node_id.clone(),
        reference_name: r.reference_name.clone(),
        reference_kind: r.reference_kind,
        line: r.line,
        column: r.column,
        file_path: r.file_path.clone().unwrap_or_default(),
        language: r.language.unwrap_or(ax_types::Language::Unknown),
        candidates: r.candidates.clone(),
    }
}

/// True when a reference resolved to the node it comes from.
fn is_self_resolution(resolved: &ResolvedRef) -> bool {
    resolved.target_node_id == resolved.original.from_node_id
}

/// The edge a resolution stands for. Fuzzy matches picked one of several
/// plausible targets, so they are marked ambiguous.
pub(crate) fn resolved_edge(resolved: &ResolvedRef) -> Edge {
    let original = &resolved.original;
    let confidence = if resolved.resolved_by == ResolvedBy::Fuzzy {
        ax_types::EdgeConfidence::Ambiguous
    } else {
        ax_types::EdgeConfidence::Inferred
    };
    Edge {
        source: original.from_node_id.clone(),
        target: resolved.target_node_id.clone(),
        kind: crate::name_matcher::reference_kind_to_edge(original.reference_kind),
        metadata: None,
        line: Some(original.line),
        column: Some(original.column),
        provenance: Some(Provenance::Heuristic),
        confidence: Some(confidence),
    }
}
