//! Scope-aware name matching with LRU cache.

use std::num::NonZeroUsize;

use lru::LruCache;

use ax_db::queries::QueryBuilder;
use ax_types::{Edge, EdgeKind, Node, Provenance, ReferenceKind};

use crate::types::{ResolvedBy, ResolvedRef, UnresolvedRef};

pub struct NameMatcher {
    cache: LruCache<String, Vec<Node>>,
}

impl NameMatcher {
    pub fn new() -> Self {
        Self {
            cache: LruCache::new(NonZeroUsize::new(1024).unwrap()),
        }
    }

    pub async fn resolve_ref(&mut self, queries: &QueryBuilder, ref_: &UnresolvedRef) -> Option<ResolvedRef> {
        let nodes = self.cached_nodes_by_name(queries, &ref_.reference_name).await;
        if nodes.is_empty() {
            return None;
        }

        let candidates = if !ref_.file_path.is_empty() {
            let in_file: Vec<Node> = nodes
                .into_iter()
                .filter(|n| n.file_path == ref_.file_path)
                .collect();
            if !in_file.is_empty() {
                in_file
            } else {
                self.cached_nodes_by_name(queries, &ref_.reference_name).await
            }
        } else {
            nodes
        };

        self.commit_resolution(queries, ref_, &candidates[0], ResolvedBy::ExactMatch).await
    }

    async fn cached_nodes_by_name(&mut self, queries: &QueryBuilder, name: &str) -> Vec<Node> {
        if let Some(hit) = self.cache.get(name) {
            return hit.clone();
        }
        let nodes = queries.get_nodes_by_name(name).await.unwrap_or_default();
        self.cache.put(name.to_string(), nodes.clone());
        nodes
    }

    pub async fn resolve_in_file(
        &mut self,
        queries: &QueryBuilder,
        ref_: &UnresolvedRef,
        file_path: &str,
    ) -> Option<ResolvedRef> {
        let nodes = queries.get_nodes_by_file(file_path).await.unwrap_or_default();
        let callable: Vec<Node> = nodes
            .iter()
            .filter(|n| {
                n.name == ref_.reference_name
                    && matches!(n.kind, ax_types::NodeKind::Function | ax_types::NodeKind::Method)
            })
            .cloned()
            .collect();
        if callable.is_empty() {
            return None;
        }
        self.commit_resolution(queries, ref_, &callable[0], ResolvedBy::Import).await
    }

    pub async fn match_function_ref(&mut self, queries: &QueryBuilder, ref_: &UnresolvedRef) -> Option<ResolvedRef> {
        if ref_.reference_kind != ReferenceKind::FunctionRef {
            return None;
        }
        let nodes = self.cached_nodes_by_name(queries, &ref_.reference_name).await;
        let func_nodes: Vec<Node> = nodes
            .into_iter()
            .filter(|n| matches!(n.kind, ax_types::NodeKind::Function | ax_types::NodeKind::Method))
            .collect();
        if func_nodes.is_empty() {
            return None;
        }
        self.commit_resolution(queries, ref_, &func_nodes[0], ResolvedBy::FunctionRef).await
    }

    pub async fn resolve_deferred_call(
        &mut self,
        queries: &QueryBuilder,
        ref_: &UnresolvedRef,
    ) -> Option<ResolvedRef> {
        if ref_.reference_kind != ReferenceKind::Calls {
            return None;
        }
        if ref_.reference_name.starts_with("this.") {
            let method = ref_.reference_name.strip_prefix("this.").unwrap_or("");
            return self
                .resolve_method_in_file_scope(queries, ref_, method)
                .await;
        }
        if let Some(pos) = ref_.reference_name.find("().") {
            let inner = ref_.reference_name[..pos].trim();
            let method = ref_.reference_name[pos + 3..].trim();
            if !inner.is_empty() && !method.is_empty() {
                return self
                    .resolve_method_after_inner(queries, ref_, inner, method)
                    .await;
            }
        }
        None
    }

    async fn resolve_method_in_file_scope(
        &self,
        queries: &QueryBuilder,
        ref_: &UnresolvedRef,
        method: &str,
    ) -> Option<ResolvedRef> {
        let file_nodes = queries
            .get_nodes_by_file(&ref_.file_path)
            .await
            .unwrap_or_default();
        let class = file_nodes
            .iter()
            .filter(|n| n.kind == ax_types::NodeKind::Class)
            .filter(|n| ref_.line >= n.start_line && ref_.line <= n.end_line)
            .max_by_key(|n| n.start_line);
        let method_node = file_nodes.iter().find(|n| {
            n.kind == ax_types::NodeKind::Method
                && n.name == method
                && class.is_none_or(|c| n.start_line >= c.start_line && n.end_line <= c.end_line)
        });
        if let Some(target) = method_node {
            return self
                .commit_resolution(queries, ref_, target, ResolvedBy::ExactMatch)
                .await;
        }
        None
    }

    async fn resolve_method_after_inner(
        &self,
        queries: &QueryBuilder,
        ref_: &UnresolvedRef,
        inner: &str,
        method: &str,
    ) -> Option<ResolvedRef> {
        let inner_name = inner.split('.').next().unwrap_or(inner).trim();
        if inner_name.is_empty() {
            return None;
        }
        let file_nodes = queries
            .get_nodes_by_file(&ref_.file_path)
            .await
            .unwrap_or_default();
        let inner_node = file_nodes.iter().find(|n| {
            matches!(
                n.kind,
                ax_types::NodeKind::Function | ax_types::NodeKind::Method | ax_types::NodeKind::Class
            ) && n.name == inner_name
        });
        if let Some(inner_node) = inner_node {
            let class = if inner_node.kind == ax_types::NodeKind::Class {
                Some(inner_node)
            } else {
                file_nodes
                    .iter()
                    .filter(|n| n.kind == ax_types::NodeKind::Class)
                    .filter(|n| inner_node.start_line >= n.start_line && inner_node.end_line <= n.end_line)
                    .max_by_key(|n| n.start_line)
            };
            if let Some(class) = class {
                let method_node = file_nodes.iter().find(|n| {
                    n.kind == ax_types::NodeKind::Method
                        && n.name == method
                        && n.start_line >= class.start_line
                        && n.end_line <= class.end_line
                });
                if let Some(target) = method_node {
                    return self
                        .commit_resolution(queries, ref_, target, ResolvedBy::ExactMatch)
                        .await;
                }
            }
        }
        None
    }

    async fn commit_resolution(
        &self,
        queries: &QueryBuilder,
        ref_: &UnresolvedRef,
        target: &Node,
        resolved_by: ResolvedBy,
    ) -> Option<ResolvedRef> {
        let edge = Edge {
            source: ref_.from_node_id.clone(),
            target: target.id.clone(),
            kind: reference_kind_to_edge(ref_.reference_kind),
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
            resolved_by,
        })
    }
}

fn reference_kind_to_edge(kind: ReferenceKind) -> EdgeKind {
    match kind {
        ReferenceKind::Calls => EdgeKind::Calls,
        ReferenceKind::Imports => EdgeKind::Imports,
        ReferenceKind::References | ReferenceKind::FunctionRef => EdgeKind::References,
        ReferenceKind::TypeOf => EdgeKind::TypeOf,
        ReferenceKind::Returns => EdgeKind::Returns,
        ReferenceKind::Extends => EdgeKind::Extends,
        ReferenceKind::Implements => EdgeKind::Implements,
        ReferenceKind::Contains => EdgeKind::Contains,
        ReferenceKind::Exports => EdgeKind::Exports,
        ReferenceKind::Instantiates => EdgeKind::Instantiates,
        ReferenceKind::Overrides => EdgeKind::Overrides,
        ReferenceKind::Decorates => EdgeKind::Decorates,
    }
}