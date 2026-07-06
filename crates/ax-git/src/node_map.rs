//! Map diff hunks to graph nodes via SQLite line ranges.

use ax_db::queries::QueryBuilder;
use ax_types::Node;
use sqlx::SqlitePool;

use crate::ChangedHunk;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DirtyNode {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub file_path: String,
    pub start_line: i64,
    pub end_line: i64,
}

pub async fn map_hunks_to_nodes(
    pool: &SqlitePool,
    hunks: &[ChangedHunk],
) -> Result<Vec<DirtyNode>, ax_utils::errors::AxError> {
    let queries = QueryBuilder::new(pool.clone());
    let mut seen = std::collections::HashSet::new();
    let mut dirty = Vec::new();

    for hunk in hunks {
        let nodes = queries.get_nodes_by_file(&hunk.path).await?;
        for node in nodes {
            if overlaps_hunk(&node, hunk) && seen.insert(node.id.clone()) {
                dirty.push(node_to_dirty(&node));
            }
        }
    }

    Ok(dirty)
}

pub async fn map_files_to_nodes(
    pool: &SqlitePool,
    files: &[String],
) -> Result<Vec<DirtyNode>, ax_utils::errors::AxError> {
    let queries = QueryBuilder::new(pool.clone());
    let mut dirty = Vec::new();
    for path in files {
        let nodes = queries.get_nodes_by_file(path).await?;
        for node in nodes {
            if is_symbol_node(&node.kind) {
                dirty.push(node_to_dirty(&node));
            }
        }
    }
    Ok(dirty)
}

fn overlaps_hunk(node: &Node, hunk: &ChangedHunk) -> bool {
    let hunk_end = hunk.new_start.saturating_add(hunk.new_lines).saturating_sub(1);
    let node_start = node.start_line as u32;
    let node_end = node.end_line as u32;
    node_start <= hunk_end && hunk.new_start <= node_end
}

fn is_symbol_node(kind: &ax_types::NodeKind) -> bool {
    use ax_types::NodeKind;
    matches!(
        kind,
        NodeKind::Function
            | NodeKind::Method
            | NodeKind::Class
            | NodeKind::Struct
            | NodeKind::Test
            | NodeKind::Route
            | NodeKind::Trait
    )
}

fn node_to_dirty(node: &Node) -> DirtyNode {
    DirtyNode {
        id: node.id.clone(),
        name: node.name.clone(),
        kind: node.kind.as_str().to_string(),
        file_path: node.file_path.clone(),
        start_line: node.start_line as i64,
        end_line: node.end_line as i64,
    }
}
