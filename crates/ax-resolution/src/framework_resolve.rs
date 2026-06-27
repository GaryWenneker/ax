//! Framework-specific reference resolution — CG: resolution/index.ts strategy 1.

use ax_db::queries::QueryBuilder;
use ax_types::{Edge, EdgeKind, Provenance, ReferenceKind};

use crate::frameworks::{django, flask, go, laravel, react, rust, spring, svelte, vue};
use crate::types::{ResolvedBy, ResolvedRef, UnresolvedRef};

pub async fn try_resolve(
    project_root: &std::path::Path,
    queries: &QueryBuilder,
    ref_: &UnresolvedRef,
) -> Option<ResolvedRef> {
    let mut best: Option<(ax_types::Node, f64)> = None;

    // Vue handles Nuxt aliases (@/, ~/) and compiler macros — including Imports.
    if let Some((node, conf)) = vue::try_resolve_target(queries, project_root, ref_).await {
        if conf >= 0.9 {
            return commit_resolution(queries, ref_, &node, conf).await;
        }
        best = Some((node, conf));
    }

    if let Some((node, conf)) = svelte::try_resolve_target(queries, project_root, ref_).await {
        if conf >= 0.9 {
            return commit_resolution(queries, ref_, &node, conf).await;
        }
        match &best {
            Some((_, b)) if conf > *b => best = Some((node, conf)),
            None => best = Some((node, conf)),
            _ => {}
        }
    }

    if ref_.reference_kind == ReferenceKind::Imports {
        if let Some((node, conf)) = best {
            if conf >= 0.8 {
                return commit_resolution(queries, ref_, &node, conf).await;
            }
        }
        return None;
    }

        if spring::claims_reference(&ref_.reference_name) || ref_.reference_kind != ReferenceKind::Imports {
        if let Some((node, conf)) = spring::try_resolve_target(queries, ref_).await {
            if conf >= 0.85 {
                return commit_resolution(queries, ref_, &node, conf).await;
            }
            match &best {
                Some((_, b)) if conf > *b => best = Some((node, conf)),
                None => best = Some((node, conf)),
                _ => {}
            }
        }
    }
if let Some((node, conf)) = react::try_resolve_target(queries, ref_).await {
        if conf >= 0.9 {
            return commit_resolution(queries, ref_, &node, conf).await;
        }
        if best.as_ref().map(|b| conf > b.1).unwrap_or(true) {
            best = Some((node, conf));
        }
    }

    if let Some((node, conf)) = rust::try_resolve_target(queries, project_root, ref_).await {
        if conf >= 0.9 {
            return commit_resolution(queries, ref_, &node, conf).await;
        }
        if best.as_ref().map(|b| conf > b.1).unwrap_or(true) {
            best = Some((node, conf));
        }
    }

    if let Some((node, conf)) = flask::try_resolve_target(queries, ref_).await {
        if conf >= 0.75 {
            return commit_resolution(queries, ref_, &node, conf).await;
        }
        if best.as_ref().map(|b| conf > b.1).unwrap_or(true) {
            best = Some((node, conf));
        }
    }

    if let Some((node, conf)) = django::try_resolve_target(queries, ref_).await {
        if conf >= 0.8 {
            return commit_resolution(queries, ref_, &node, conf).await;
        }
        if best.as_ref().map(|b| conf > b.1).unwrap_or(true) {
            best = Some((node, conf));
        }
    }

    if let Some((node, conf)) = laravel::try_resolve_target(queries, project_root, ref_).await {
        if conf >= 0.9 {
            return commit_resolution(queries, ref_, &node, conf).await;
        }
        if best.as_ref().map(|b| conf > b.1).unwrap_or(true) {
            best = Some((node, conf));
        }
    }

    if let Some((node, conf)) = go::try_resolve_target(queries, ref_).await {
        if conf >= 0.9 {
            return commit_resolution(queries, ref_, &node, conf).await;
        }
        if best.as_ref().map(|b| conf > b.1).unwrap_or(true) {
            best = Some((node, conf));
        }
    }

    if let Some((node, conf)) = best { commit_resolution(queries, ref_, &node, conf).await } else { None }
}

async fn commit_resolution(
    queries: &QueryBuilder,
    ref_: &UnresolvedRef,
    target: &ax_types::Node,
    confidence: f64,
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
        confidence,
        resolved_by: ResolvedBy::Framework,
    })
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