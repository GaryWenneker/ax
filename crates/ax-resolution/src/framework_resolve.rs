//! Framework-specific reference resolution — CG: resolution/index.ts strategy 1.

use ax_db::queries::QueryBuilder;
use ax_types::ReferenceKind;

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
            return commit_resolution(ref_, &node, conf).await;
        }
        best = Some((node, conf));
    }

    if let Some((node, conf)) = svelte::try_resolve_target(queries, project_root, ref_).await {
        if conf >= 0.9 {
            return commit_resolution(ref_, &node, conf).await;
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
                return commit_resolution(ref_, &node, conf).await;
            }
        }
        return None;
    }

    if spring::claims_reference(&ref_.reference_name)
        || ref_.reference_kind != ReferenceKind::Imports
    {
        if let Some((node, conf)) = spring::try_resolve_target(queries, ref_).await {
            if conf >= 0.85 {
                return commit_resolution(ref_, &node, conf).await;
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
            return commit_resolution(ref_, &node, conf).await;
        }
        if best.as_ref().map(|b| conf > b.1).unwrap_or(true) {
            best = Some((node, conf));
        }
    }

    if let Some((node, conf)) = rust::try_resolve_target(queries, project_root, ref_).await {
        if conf >= 0.9 {
            return commit_resolution(ref_, &node, conf).await;
        }
        if best.as_ref().map(|b| conf > b.1).unwrap_or(true) {
            best = Some((node, conf));
        }
    }

    if let Some((node, conf)) = flask::try_resolve_target(queries, ref_).await {
        if conf >= 0.75 {
            return commit_resolution(ref_, &node, conf).await;
        }
        if best.as_ref().map(|b| conf > b.1).unwrap_or(true) {
            best = Some((node, conf));
        }
    }

    if let Some((node, conf)) = django::try_resolve_target(queries, ref_).await {
        if conf >= 0.8 {
            return commit_resolution(ref_, &node, conf).await;
        }
        if best.as_ref().map(|b| conf > b.1).unwrap_or(true) {
            best = Some((node, conf));
        }
    }

    if let Some((node, conf)) = laravel::try_resolve_target(queries, project_root, ref_).await {
        if conf >= 0.9 {
            return commit_resolution(ref_, &node, conf).await;
        }
        if best.as_ref().map(|b| conf > b.1).unwrap_or(true) {
            best = Some((node, conf));
        }
    }

    if let Some((node, conf)) = go::try_resolve_target(queries, ref_).await {
        if conf >= 0.9 {
            return commit_resolution(ref_, &node, conf).await;
        }
        if best.as_ref().map(|b| conf > b.1).unwrap_or(true) {
            best = Some((node, conf));
        }
    }

    if let Some((node, conf)) = best {
        commit_resolution(ref_, &node, conf).await
    } else {
        None
    }
}

async fn commit_resolution(
    ref_: &UnresolvedRef,
    target: &ax_types::Node,
    confidence: f64,
) -> Option<ResolvedRef> {
    // The edge is written later in one batch by `ReferenceResolver::resolve_all`.
    Some(ResolvedRef {
        original: ref_.clone(),
        target_node_id: target.id.clone(),
        confidence,
        resolved_by: ResolvedBy::Framework,
    })
}
