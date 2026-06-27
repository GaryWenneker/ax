//! Main reference resolver.

use std::collections::HashMap;

use ax_db::queries::QueryBuilder;
use ax_types::{Edge, EdgeKind, Provenance, ReferenceKind, UnresolvedReference};

use crate::callback_synthesizer::CallbackSynthesizer;
use crate::c_fnptr_synthesizer::CFnptrSynthesizer;
use crate::framework_resolve;
use crate::frameworks::FrameworkRegistry;
use crate::import_resolver::ImportResolver;
use crate::name_matcher::NameMatcher;
use crate::types::{ResolvedBy, ResolvedRef, ResolutionResult, ResolutionStats, UnresolvedRef};

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

    pub async fn resolve_all(&mut self, queries: &QueryBuilder) -> Result<ResolutionResult, ax_utils::errors::AxError> {
        self.frameworks
            .run_post_extract(self.import_resolver.project_root(), queries)
            .await?;

        let refs = queries.get_unresolved_refs().await?;
        let import_map = build_import_map(&refs, &self.import_resolver);

        let mut deferred_refs: Vec<UnresolvedReference> = Vec::new();

        let mut result = ResolutionResult {
            resolved: vec![],
            unresolved: vec![],
            stats: ResolutionStats::default(),
        };

        for db_ref in refs {
            let ref_ = unresolved_from_db(&db_ref);
            result.stats.total += 1;

            if ref_.reference_kind == ReferenceKind::Calls && needs_deferred_resolution(&ref_) {
                deferred_refs.push(db_ref);
                continue;
            }

            let resolved = match ref_.reference_kind {
                ReferenceKind::FunctionRef => self.name_matcher.match_function_ref(queries, &ref_).await,
                ReferenceKind::Imports => self.resolve_import_ref(queries, &ref_).await,
                _ => {
                    if let Some(r) = framework_resolve::try_resolve(self.import_resolver.project_root(), queries, &ref_).await {
                        Some(r)
                    } else if ref_.reference_kind == ReferenceKind::Calls {
                        if let Some(target_file) =
                            import_map.get(&(ref_.file_path.clone(), ref_.reference_name.clone()))
                        {
                            self.name_matcher
                                .resolve_in_file(queries, &ref_, target_file)
                                .await
                        } else {
                            self.name_matcher.resolve_ref(queries, &ref_).await
                        }
                    } else {
                        self.name_matcher.resolve_ref(queries, &ref_).await
                    }
                }
            };

            if let Some(r) = resolved {
                result.stats.resolved += 1;
                let method = format!("{:?}", r.resolved_by);
                *result.stats.by_method.entry(method).or_insert(0) += 1;
                result.resolved.push(r);
                if let Err(e) = queries.delete_unresolved_ref(&db_ref).await {
                    tracing::warn!("failed to delete resolved ref: {}", e);
                }
            } else {
                result.stats.unresolved += 1;
                result.unresolved.push(ref_);
            }
        }

        for db_ref in deferred_refs {
            let ref_ = unresolved_from_db(&db_ref);
            if let Some(r) = self.name_matcher.resolve_deferred_call(queries, &ref_).await {
                result.stats.resolved += 1;
                let method = format!("{:?}", r.resolved_by);
                *result.stats.by_method.entry(method).or_insert(0) += 1;
                result.resolved.push(r);
                if let Err(e) = queries.delete_unresolved_ref(&db_ref).await {
                    tracing::warn!("failed to delete resolved ref: {}", e);
                }
            } else {
                result.stats.unresolved += 1;
                result.unresolved.push(ref_);
            }
        }

        self.callback_synth
            .synthesize(self.import_resolver.project_root(), queries)
            .await?;

        self.c_fnptr_synth
            .synthesize(self.import_resolver.project_root(), queries)
            .await?;

        Ok(result)
    }

    pub async fn resolve_for_files(
        &mut self,
        queries: &QueryBuilder,
        files: &[String],
    ) -> Result<ResolutionResult, ax_utils::errors::AxError> {
        let refs = queries.get_unresolved_refs_by_files(files).await?;
        let import_map = build_import_map(&refs, &self.import_resolver);
        let mut result = ResolutionResult {
            resolved: vec![],
            unresolved: vec![],
            stats: ResolutionStats::default(),
        };
        for db_ref in refs {
            let ref_ = unresolved_from_db(&db_ref);
            result.stats.total += 1;
            let resolved = if let Some(target_file) = import_map.get(&(ref_.file_path.clone(), ref_.reference_name.clone())) {
                self.name_matcher.resolve_in_file(queries, &ref_, target_file).await
            } else {
                self.name_matcher.resolve_ref(queries, &ref_).await
            };
            if let Some(r) = resolved {
                result.stats.resolved += 1;
                result.resolved.push(r);
                if let Err(e) = queries.delete_unresolved_ref(&db_ref).await {
                    tracing::warn!("failed to delete resolved ref: {}", e);
                }
            } else {
                result.stats.unresolved += 1;
                result.unresolved.push(ref_);
            }
        }
        Ok(result)
    }

    async fn resolve_import_ref(&self, queries: &QueryBuilder, ref_: &UnresolvedRef) -> Option<ResolvedRef> {
        let module = ref_
            .candidates
            .as_ref()
            .and_then(|c| c.first())
            .map(|s| s.as_str())
            .unwrap_or(&ref_.reference_name);

        let target_file = self
            .import_resolver
            .resolve_to_indexed_file(&ref_.file_path, module)?;

        let nodes = queries.get_nodes_by_file(&target_file).await.unwrap_or_default();
        let binding = &ref_.reference_name;

        let target = nodes
            .iter()
            .find(|n| n.name == *binding && (matches!(n.kind, ax_types::NodeKind::Function | ax_types::NodeKind::Method | ax_types::NodeKind::Class | ax_types::NodeKind::Interface)))
            .or_else(|| {
                nodes.iter().find(|n| n.kind == ax_types::NodeKind::File)
            })?;

        let edge = Edge {
            source: ref_.from_node_id.clone(),
            target: target.id.clone(),
            kind: EdgeKind::Imports,
            metadata: None,
            line: Some(ref_.line),
            column: Some(ref_.column),
            provenance: Some(Provenance::Heuristic),
        };
        if queries.upsert_edge(&edge).await.is_err() {
            return None;
        }

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