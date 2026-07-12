//! Prune stale and unresolvable entries from `unresolved_refs`.

use ax_db::queries::QueryBuilder;
use ax_utils::errors::AxError;

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct UnresolvedCleanupStats {
    pub orphan_from_node: u64,
    pub stale_file: u64,
    pub malformed_generic: u64,
    pub external_calls: u64,
}

/// Remove orphan, stale-file, malformed generic, and known external-library call refs.
pub async fn prune_stale_unresolved_refs(
    queries: &QueryBuilder,
) -> Result<UnresolvedCleanupStats, AxError> {
    let orphan_from_node = queries.prune_orphan_unresolved_refs().await?;
    let stale_file = queries.prune_stale_file_unresolved_refs().await?;
    let malformed_generic = queries.prune_malformed_call_unresolved_refs().await?;
    let external_calls = queries.prune_external_call_unresolved_refs().await?;
    Ok(UnresolvedCleanupStats {
        orphan_from_node,
        stale_file,
        malformed_generic,
        external_calls,
    })
}
