//! Query layer for ax database.

use std::collections::HashMap;

use ax_types::{
    Edge, EdgeConfidence, EdgeKind, FileRecord, GraphStats, Language, Node, NodeKind, Provenance,
    ReferenceKind, SearchOptions, SearchResult, UnresolvedReference, Visibility,
};
use sqlx::SqlitePool;

use ax_utils::errors::{AxError, DatabaseError};

pub struct QueryBuilder {
    pool: SqlitePool,
}

impl QueryBuilder {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub async fn upsert_node(&self, node: &Node) -> Result<(), AxError> {
        sqlx::query(
            r#"
            INSERT INTO nodes (
                id, kind, name, qualified_name, file_path, language,
                start_line, end_line, start_column, end_column,
                docstring, signature, visibility, is_exported, is_async,
                is_static, is_abstract, decorators, type_parameters, return_type, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                kind=excluded.kind, name=excluded.name, qualified_name=excluded.qualified_name,
                file_path=excluded.file_path, language=excluded.language,
                start_line=excluded.start_line, end_line=excluded.end_line,
                start_column=excluded.start_column, end_column=excluded.end_column,
                docstring=excluded.docstring, signature=excluded.signature,
                visibility=excluded.visibility, is_exported=excluded.is_exported,
                is_async=excluded.is_async, is_static=excluded.is_static,
                is_abstract=excluded.is_abstract, decorators=excluded.decorators,
                type_parameters=excluded.type_parameters, return_type=excluded.return_type,
                updated_at=excluded.updated_at
            "#,
        )
        .bind(&node.id)
        .bind(node.kind.as_str())
        .bind(&node.name)
        .bind(&node.qualified_name)
        .bind(&node.file_path)
        .bind(node.language.as_str())
        .bind(node.start_line)
        .bind(node.end_line)
        .bind(node.start_column)
        .bind(node.end_column)
        .bind(&node.docstring)
        .bind(&node.signature)
        .bind(node.visibility.map(|v| match v {
            Visibility::Public => "public",
            Visibility::Private => "private",
            Visibility::Protected => "protected",
            Visibility::Internal => "internal",
        }))
        .bind(node.is_exported.unwrap_or(false))
        .bind(node.is_async.unwrap_or(false))
        .bind(node.is_static.unwrap_or(false))
        .bind(node.is_abstract.unwrap_or(false))
        .bind(node.decorators.as_ref().map(|d| serde_json::to_string(d).unwrap_or_default()))
        .bind(node.type_parameters.as_ref().map(|t| serde_json::to_string(t).unwrap_or_default()))
        .bind(&node.return_type)
        .bind(node.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
        Ok(())
    }

    pub async fn upsert_edge(&self, edge: &Edge) -> Result<(), AxError> {
        sqlx::query(
            r#"
            INSERT OR IGNORE INTO edges (source, target, kind, metadata, line, col, provenance, confidence)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&edge.source)
        .bind(&edge.target)
        .bind(edge.kind.as_str())
        .bind(edge.metadata.as_ref().map(|m| serde_json::to_string(m).unwrap_or_default()))
        .bind(edge.line)
        .bind(edge.column)
        .bind(edge.provenance.map(|p| match p {
            Provenance::TreeSitter => "tree-sitter",
            Provenance::Scip => "scip",
            Provenance::Heuristic => "heuristic",
        }))
        .bind(edge_confidence_str(edge))
        .execute(&self.pool)
        .await
        .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
        Ok(())
    }

    /// CG: `insertNodes` — multi-row upsert in a single transaction.
    /// Chunked to stay well under SQLite's bind-variable limit.
    pub async fn upsert_nodes(&self, nodes: &[Node]) -> Result<(), AxError> {
        if nodes.is_empty() {
            return Ok(());
        }
        // 21 columns per row; 400 rows = 8,400 binds per statement.
        const CHUNK: usize = 400;
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
        for chunk in nodes.chunks(CHUNK) {
            let mut qb = sqlx::QueryBuilder::new(
                "INSERT INTO nodes (
                    id, kind, name, qualified_name, file_path, language,
                    start_line, end_line, start_column, end_column,
                    docstring, signature, visibility, is_exported, is_async,
                    is_static, is_abstract, decorators, type_parameters, return_type, updated_at
                ) ",
            );
            qb.push_values(chunk, |mut b, node| {
                b.push_bind(&node.id)
                    .push_bind(node.kind.as_str())
                    .push_bind(&node.name)
                    .push_bind(&node.qualified_name)
                    .push_bind(&node.file_path)
                    .push_bind(node.language.as_str())
                    .push_bind(node.start_line)
                    .push_bind(node.end_line)
                    .push_bind(node.start_column)
                    .push_bind(node.end_column)
                    .push_bind(&node.docstring)
                    .push_bind(&node.signature)
                    .push_bind(node.visibility.map(|v| match v {
                        Visibility::Public => "public",
                        Visibility::Private => "private",
                        Visibility::Protected => "protected",
                        Visibility::Internal => "internal",
                    }))
                    .push_bind(node.is_exported.unwrap_or(false))
                    .push_bind(node.is_async.unwrap_or(false))
                    .push_bind(node.is_static.unwrap_or(false))
                    .push_bind(node.is_abstract.unwrap_or(false))
                    .push_bind(node.decorators.as_ref().map(|d| serde_json::to_string(d).unwrap_or_default()))
                    .push_bind(node.type_parameters.as_ref().map(|t| serde_json::to_string(t).unwrap_or_default()))
                    .push_bind(&node.return_type)
                    .push_bind(node.updated_at);
            });
            qb.push(
                " ON CONFLICT(id) DO UPDATE SET
                    kind=excluded.kind, name=excluded.name, qualified_name=excluded.qualified_name,
                    file_path=excluded.file_path, language=excluded.language,
                    start_line=excluded.start_line, end_line=excluded.end_line,
                    start_column=excluded.start_column, end_column=excluded.end_column,
                    docstring=excluded.docstring, signature=excluded.signature,
                    visibility=excluded.visibility, is_exported=excluded.is_exported,
                    is_async=excluded.is_async, is_static=excluded.is_static,
                    is_abstract=excluded.is_abstract, decorators=excluded.decorators,
                    type_parameters=excluded.type_parameters, return_type=excluded.return_type,
                    updated_at=excluded.updated_at",
            );
            qb.build()
                .execute(&mut *tx)
                .await
                .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
        }
        tx.commit()
            .await
            .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
        Ok(())
    }

    pub async fn upsert_edges(&self, edges: &[Edge]) -> Result<(), AxError> {
        if edges.is_empty() {
            return Ok(());
        }
        // 8 columns per row; 875 rows = 7,000 binds per statement.
        const CHUNK: usize = 875;
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
        for chunk in edges.chunks(CHUNK) {
            let mut qb = sqlx::QueryBuilder::new(
                "INSERT OR IGNORE INTO edges (source, target, kind, metadata, line, col, provenance, confidence) ",
            );
            qb.push_values(chunk, |mut b, edge| {
                b.push_bind(&edge.source)
                    .push_bind(&edge.target)
                    .push_bind(edge.kind.as_str())
                    .push_bind(edge.metadata.as_ref().map(|m| serde_json::to_string(m).unwrap_or_default()))
                    .push_bind(edge.line)
                    .push_bind(edge.column)
                    .push_bind(edge.provenance.map(|p| match p {
                        Provenance::TreeSitter => "tree-sitter",
                        Provenance::Scip => "scip",
                        Provenance::Heuristic => "heuristic",
                    }))
                    .push_bind(edge_confidence_str(edge));
            });
            qb.build()
                .execute(&mut *tx)
                .await
                .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
        }
        tx.commit()
            .await
            .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
        Ok(())
    }

    pub async fn delete_nodes_by_file(&self, file_path: &str) -> Result<(), AxError> {
        self.clear_file(file_path).await
    }

    /// CG: `deleteFile` — remove file record, nodes (cascade edges), and unresolved refs.
    pub async fn clear_file(&self, file_path: &str) -> Result<(), AxError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
        sqlx::query("DELETE FROM unresolved_refs WHERE file_path = ?")
            .bind(file_path)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        sqlx::query("DELETE FROM nodes WHERE file_path = ?")
            .bind(file_path)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        sqlx::query("DELETE FROM files WHERE path = ?")
            .bind(file_path)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        tx.commit().await.map_err(db_err)?;
        Ok(())
    }

    pub async fn get_nodes_by_lower_name(&self, lower_name: &str) -> Result<Vec<Node>, AxError> {
        let rows = sqlx::query_as::<_, NodeRow>("SELECT * FROM nodes WHERE lower(name) = lower(?)")
            .bind(lower_name)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
        Ok(rows.into_iter().map(|r| r.into_node()).collect())
    }

    pub async fn upsert_file(&self, file: &FileRecord) -> Result<(), AxError> {
        sqlx::query(
            r#"
            INSERT INTO files (path, content_hash, language, size, modified_at, indexed_at, node_count, errors)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(path) DO UPDATE SET
                content_hash=excluded.content_hash, language=excluded.language,
                size=excluded.size, modified_at=excluded.modified_at,
                indexed_at=excluded.indexed_at, node_count=excluded.node_count,
                errors=excluded.errors
            "#,
        )
        .bind(&file.path)
        .bind(&file.content_hash)
        .bind(file.language.as_str())
        .bind(file.size)
        .bind(file.modified_at)
        .bind(file.indexed_at)
        .bind(file.node_count)
        .bind(file.errors.as_ref().map(|e| serde_json::to_string(e).unwrap_or_default()))
        .execute(&self.pool)
        .await
        .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
        Ok(())
    }

    pub async fn get_node_by_id(&self, id: &str) -> Result<Option<Node>, AxError> {
        let row = sqlx::query_as::<_, NodeRow>("SELECT * FROM nodes WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
        Ok(row.map(|r| r.into_node()))
    }

    /// Batch fetch nodes by id — avoids N+1 round trips in graph traversal.
    pub async fn get_nodes_by_ids(&self, ids: &[String]) -> Result<Vec<Node>, AxError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        const CHUNK: usize = 900;
        let mut nodes = Vec::with_capacity(ids.len());
        for chunk in ids.chunks(CHUNK) {
            let mut qb = sqlx::QueryBuilder::new("SELECT * FROM nodes WHERE id IN (");
            let mut separated = qb.separated(", ");
            for id in chunk {
                separated.push_bind(id);
            }
            separated.push_unseparated(")");
            let rows: Vec<NodeRow> = qb
                .build_query_as()
                .fetch_all(&self.pool)
                .await
                .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
            nodes.extend(rows.into_iter().map(|r| r.into_node()));
        }
        Ok(nodes)
    }

    pub async fn get_nodes_by_file(&self, file_path: &str) -> Result<Vec<Node>, AxError> {
        let rows = sqlx::query_as::<_, NodeRow>("SELECT * FROM nodes WHERE file_path = ?")
            .bind(file_path)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
        Ok(rows.into_iter().map(|r| r.into_node()).collect())
    }

    pub async fn get_nodes_by_name(&self, name: &str) -> Result<Vec<Node>, AxError> {
        let rows = sqlx::query_as::<_, NodeRow>("SELECT * FROM nodes WHERE lower(name) = lower(?)")
            .bind(name)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
        Ok(rows.into_iter().map(|r| r.into_node()).collect())
    }

    pub async fn get_outgoing_edges(&self, node_id: &str, kinds: Option<&[EdgeKind]>) -> Result<Vec<Edge>, AxError> {
        let rows = if let Some(kinds) = kinds {
            let placeholders: Vec<String> = kinds.iter().map(|k| k.as_str().to_string()).collect();
            let sql = format!(
                "SELECT source, target, kind, metadata, line, col, provenance, confidence FROM edges WHERE source = ? AND kind IN ({})",
                placeholders.iter().map(|_| "?").collect::<Vec<_>>().join(",")
            );
            let mut query = sqlx::query_as::<_, EdgeRow>(&sql).bind(node_id);
            for k in kinds {
                query = query.bind(k.as_str());
            }
            query
                .fetch_all(&self.pool)
                .await
                .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?
        } else {
            sqlx::query_as::<_, EdgeRow>(
                "SELECT source, target, kind, metadata, line, col, provenance, confidence FROM edges WHERE source = ?",
            )
            .bind(node_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?
        };
        Ok(rows.into_iter().map(|r| r.into_edge()).collect())
    }

    pub async fn get_incoming_edges(&self, node_id: &str) -> Result<Vec<Edge>, AxError> {
        let rows = sqlx::query_as::<_, EdgeRow>(
            "SELECT source, target, kind, metadata, line, col, provenance, confidence FROM edges WHERE target = ?",
        )
        .bind(node_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
        Ok(rows.into_iter().map(|r| r.into_edge()).collect())
    }

    /// Fetch every edge in the graph. Used by whole-graph analysis
    /// (community detection, god-node ranking).
    pub async fn get_all_edges(&self) -> Result<Vec<Edge>, AxError> {
        let rows = sqlx::query_as::<_, EdgeRow>(
            "SELECT source, target, kind, metadata, line, col, provenance, confidence FROM edges",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
        Ok(rows.into_iter().map(|r| r.into_edge()).collect())
    }

    /// Fetch every node in the graph. Used by whole-graph analysis.
    pub async fn get_all_nodes(&self) -> Result<Vec<Node>, AxError> {
        let rows = sqlx::query_as::<_, NodeRow>("SELECT * FROM nodes")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
        Ok(rows.into_iter().map(|r| r.into_node()).collect())
    }

    /// Replace all stored community assignments in a single transaction.
    pub async fn replace_node_communities(
        &self,
        assignments: &[(String, i64, Option<String>)],
    ) -> Result<(), AxError> {
        let now = now_ms();
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
        sqlx::query("DELETE FROM node_communities")
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        if !assignments.is_empty() {
            const CHUNK: usize = 800;
            for chunk in assignments.chunks(CHUNK) {
                let mut qb = sqlx::QueryBuilder::new(
                    "INSERT OR REPLACE INTO node_communities (node_id, community_id, community_label, computed_at) ",
                );
                qb.push_values(chunk, |mut b, (node_id, community_id, label)| {
                    b.push_bind(node_id)
                        .push_bind(community_id)
                        .push_bind(label)
                        .push_bind(now);
                });
                qb.build().execute(&mut *tx).await.map_err(db_err)?;
            }
        }
        tx.commit().await.map_err(db_err)?;
        Ok(())
    }

    /// All stored community assignments as (node_id, community_id, label).
    pub async fn get_node_communities(
        &self,
    ) -> Result<Vec<(String, i64, Option<String>)>, AxError> {
        let rows = sqlx::query_as::<_, (String, i64, Option<String>)>(
            "SELECT node_id, community_id, community_label FROM node_communities",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(rows)
    }

    /// Remove all documentation nodes (edges cascade via FK). Used to refresh
    /// the markdown/doc layer on each index pass.
    pub async fn delete_doc_nodes(&self) -> Result<(), AxError> {
        sqlx::query("DELETE FROM nodes WHERE kind = 'doc'")
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(())
    }

    /// Node ids whose exact `qualified_name` or `name` equals `symbol`
    /// (excluding doc nodes). Used to link doc mentions to code symbols.
    pub async fn get_node_ids_by_symbol(&self, symbol: &str, limit: i64) -> Result<Vec<String>, AxError> {
        let rows = sqlx::query_scalar::<_, String>(
            "SELECT id FROM nodes WHERE kind != 'doc' AND (qualified_name = ? OR name = ?) LIMIT ?",
        )
        .bind(symbol)
        .bind(symbol)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(rows)
    }

    /// When communities were last computed (max computed_at), if ever.
    pub async fn communities_computed_at(&self) -> Result<Option<i64>, AxError> {
        let v: Option<i64> = sqlx::query_scalar("SELECT MAX(computed_at) FROM node_communities")
            .fetch_one(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(v)
    }

    pub async fn search_nodes(&self, query: &str, opts: &SearchOptions) -> Result<Vec<SearchResult>, AxError> {
        let limit = opts.limit.unwrap_or(50) as i64;
        let offset = opts.offset.unwrap_or(0) as i64;
        let kind_filter: Option<Vec<String>> = opts
            .kinds
            .as_ref()
            .map(|k| k.iter().map(|kind| kind.as_str().to_string()).collect());
        let lang_filter: Option<Vec<String>> = opts
            .languages
            .as_ref()
            .map(|l| l.iter().map(|lang| lang.as_str().to_string()).collect());

        let rows = if query.trim().is_empty() {
            let mut sql = String::from("SELECT * FROM nodes WHERE 1=1");
            let mut binds: Vec<String> = Vec::new();
            if let Some(kinds) = &kind_filter {
                let ph = kinds.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                sql.push_str(&format!(" AND kind IN ({})", ph));
                binds.extend(kinds.clone());
            }
            if let Some(langs) = &lang_filter {
                let ph = langs.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                sql.push_str(&format!(" AND language IN ({})", ph));
                binds.extend(langs.clone());
            }
            sql.push_str(" LIMIT ? OFFSET ?");
            let mut q = sqlx::query_as::<_, NodeRow>(&sql);
            for b in &binds {
                q = q.bind(b);
            }
            q.bind(limit)
                .bind(offset)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?
        } else {
            let fts_limit = (limit * 5).max(100);
            let fts_query = build_fts_prefix_query(query);
            let mut rows: Vec<NodeRow> = if let Some(fts_q) = fts_query {
                let mut sql = String::from(
                    "SELECT n.* FROM nodes n JOIN nodes_fts fts ON n.rowid = fts.rowid WHERE nodes_fts MATCH ?",
                );
                let mut binds: Vec<String> = vec![fts_q];
                if let Some(kinds) = &kind_filter {
                    let ph = kinds.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                    sql.push_str(&format!(" AND n.kind IN ({})", ph));
                    binds.extend(kinds.clone());
                }
                if let Some(langs) = &lang_filter {
                    let ph = langs.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                    sql.push_str(&format!(" AND n.language IN ({})", ph));
                    binds.extend(langs.clone());
                }
                sql.push_str(" ORDER BY rank LIMIT ? OFFSET ?");
                let mut q = sqlx::query_as::<_, NodeRow>(&sql);
                for b in &binds {
                    q = q.bind(b);
                }
                q.bind(fts_limit)
                    .bind(offset)
                    .fetch_all(&self.pool)
                    .await
                    .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?
            } else {
                Vec::new()
            };

            if rows.is_empty() && query.trim().len() >= 2 {
                let like = format!("%{}%", query.trim().to_lowercase());
                let mut sql = String::from(
                    "SELECT * FROM nodes WHERE lower(name) LIKE ? OR lower(qualified_name) LIKE ?",
                );
                let mut binds: Vec<String> = vec![like.clone(), like];
                if let Some(kinds) = &kind_filter {
                    let ph = kinds.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                    sql.push_str(&format!(" AND kind IN ({})", ph));
                    binds.extend(kinds.clone());
                }
                if let Some(langs) = &lang_filter {
                    let ph = langs.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                    sql.push_str(&format!(" AND language IN ({})", ph));
                    binds.extend(langs.clone());
                }
                sql.push_str(" LIMIT ? OFFSET ?");
                let mut q = sqlx::query_as::<_, NodeRow>(&sql);
                for b in &binds {
                    q = q.bind(b);
                }
                rows = q
                    .bind(limit)
                    .bind(offset)
                    .fetch_all(&self.pool)
                    .await
                    .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
            }

            rows
        };

        let mut results: Vec<SearchResult> = rows
            .into_iter()
            .map(|r| {
                let node = r.into_node();
                let score = score_node_for_query(query, &node);
                SearchResult {
                    node,
                    score,
                    highlights: None,
                }
            })
            .collect();

        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        if let Some(patterns) = &opts.include_patterns {
            results.retain(|r| patterns.iter().any(|p| r.node.file_path.contains(p)));
        }
        if let Some(patterns) = &opts.exclude_patterns {
            results.retain(|r| !patterns.iter().any(|p| r.node.file_path.contains(p)));
        }

        // CG: exact name supplement when FTS/LIKE returned candidates.
        if !query.trim().is_empty() && !results.is_empty() {
            let mut existing: std::collections::HashSet<String> =
                results.iter().map(|r| r.node.id.clone()).collect();
            for term in query.split_whitespace().filter(|t| t.len() >= 2) {
                let exact = self.get_nodes_by_lower_name(term).await?;
                for node in exact {
                    if existing.insert(node.id.clone()) {
                        results.push(SearchResult {
                            node,
                            score: 1.0,
                            highlights: None,
                        });
                    }
                }
            }
        }

        if results.len() > limit as usize {
            results.truncate(limit as usize);
        }

        Ok(results)
    }

    pub async fn insert_unresolved_ref(&self, ref_: &UnresolvedReference) -> Result<(), AxError> {
        sqlx::query(
            r#"
            INSERT INTO unresolved_refs (from_node_id, reference_name, reference_kind, line, col, candidates, file_path, language)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&ref_.from_node_id)
        .bind(&ref_.reference_name)
        .bind(ref_.reference_kind.as_str())
        .bind(ref_.line)
        .bind(ref_.column)
        .bind(ref_.candidates.as_ref().map(|c| serde_json::to_string(c).unwrap_or_default()))
        .bind(ref_.file_path.as_deref().unwrap_or(""))
        .bind(ref_.language.map(|l| l.as_str()).unwrap_or("unknown"))
        .execute(&self.pool)
        .await
        .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
        Ok(())
    }

    pub async fn insert_unresolved_refs(&self, refs: &[UnresolvedReference]) -> Result<(), AxError> {
        if refs.is_empty() {
            return Ok(());
        }
        // 8 columns per row; 1000 rows = 8,000 binds per statement.
        const CHUNK: usize = 1000;
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
        for chunk in refs.chunks(CHUNK) {
            let mut qb = sqlx::QueryBuilder::new(
                "INSERT INTO unresolved_refs (from_node_id, reference_name, reference_kind, line, col, candidates, file_path, language) ",
            );
            qb.push_values(chunk, |mut b, ref_| {
                b.push_bind(&ref_.from_node_id)
                    .push_bind(&ref_.reference_name)
                    .push_bind(ref_.reference_kind.as_str())
                    .push_bind(ref_.line)
                    .push_bind(ref_.column)
                    .push_bind(ref_.candidates.as_ref().map(|c| serde_json::to_string(c).unwrap_or_default()))
                    .push_bind(ref_.file_path.as_deref().unwrap_or(""))
                    .push_bind(ref_.language.map(|l| l.as_str()).unwrap_or("unknown"));
            });
            qb.build()
                .execute(&mut *tx)
                .await
                .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
        }
        tx.commit()
            .await
            .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
        Ok(())
    }

    pub async fn get_unresolved_refs(&self) -> Result<Vec<UnresolvedReference>, AxError> {
        let rows = sqlx::query_as::<_, UnresolvedRefRow>("SELECT * FROM unresolved_refs")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
        Ok(rows.into_iter().map(|r| r.into_ref()).collect())
    }

    pub async fn get_unresolved_refs_by_files(&self, files: &[String]) -> Result<Vec<UnresolvedReference>, AxError> {
        if files.is_empty() {
            return Ok(vec![]);
        }
        let placeholders = files.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!("SELECT * FROM unresolved_refs WHERE file_path IN ({})", placeholders);
        let mut query = sqlx::query_as::<_, UnresolvedRefRow>(&sql);
        for f in files {
            query = query.bind(f);
        }
        let rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
        Ok(rows.into_iter().map(|r| r.into_ref()).collect())
    }


    pub async fn delete_unresolved_ref(&self, ref_: &UnresolvedReference) -> Result<(), AxError> {
        sqlx::query(
            r#"
            DELETE FROM unresolved_refs
            WHERE from_node_id = ? AND reference_name = ? AND reference_kind = ?
              AND line = ? AND col = ? AND file_path = ?
            "#,
        )
        .bind(&ref_.from_node_id)
        .bind(&ref_.reference_name)
        .bind(ref_.reference_kind.as_str())
        .bind(ref_.line)
        .bind(ref_.column)
        .bind(ref_.file_path.as_deref().unwrap_or(""))
        .execute(&self.pool)
        .await
        .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
        Ok(())
    }

    pub async fn count_unresolved_refs(&self) -> Result<i64, AxError> {
        sqlx::query_scalar("SELECT COUNT(*) FROM unresolved_refs")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))
    }
    pub async fn clear_unresolved_refs(&self) -> Result<(), AxError> {
        sqlx::query("DELETE FROM unresolved_refs")
            .execute(&self.pool)
            .await
            .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
        Ok(())
    }

    /// Rows whose `from_node_id` no longer exists in `nodes`.
    pub async fn prune_orphan_unresolved_refs(&self) -> Result<u64, AxError> {
        let result = sqlx::query(
            "DELETE FROM unresolved_refs WHERE from_node_id NOT IN (SELECT id FROM nodes)",
        )
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(result.rows_affected())
    }

    /// Rows for files removed from the index.
    pub async fn prune_stale_file_unresolved_refs(&self) -> Result<u64, AxError> {
        let result = sqlx::query(
            "DELETE FROM unresolved_refs WHERE file_path != '' AND file_path NOT IN (SELECT path FROM files)",
        )
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(result.rows_affected())
    }

    /// Full generic call text captured before callee normalization (e.g. `AddSingleton<T>`).
    pub async fn prune_malformed_call_unresolved_refs(&self) -> Result<u64, AxError> {
        let result = sqlx::query(
            "DELETE FROM unresolved_refs WHERE reference_kind = 'calls' AND reference_name LIKE '%<%'",
        )
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(result.rows_affected())
    }

    /// Known library / framework calls that cannot resolve to indexed symbols.
    pub async fn prune_external_call_unresolved_refs(&self) -> Result<u64, AxError> {
        const EXTERNAL: &[&str] = &[
            "AddSingleton",
            "AddScoped",
            "AddTransient",
            "AddMediatR",
            "RegisterServicesFromAssembly",
            "GetExecutingAssembly",
            "AddControllers",
            "AddDbContext",
            "AddAutoMapper",
            "AddHttpClient",
            "AddMemoryCache",
            "AddStackExchangeRedisCache",
            "AddAuthentication",
            "AddAuthorization",
            "AddCors",
            "AddSwaggerGen",
            "UseAuthentication",
            "UseAuthorization",
            "UseRouting",
            "UseEndpoints",
            "UseSwagger",
            "UseSwaggerUI",
            "MapControllers",
            "MapGet",
            "MapPost",
            "Configure",
            "ConfigureServices",
            "ConfigureAppConfiguration",
            "Build",
            "Run",
            "RunAsync",
            "CreateBuilder",
            "CreateDefaultBuilder",
            "AddMvc",
            "AddRazorPages",
            "AddSignalR",
            "AddHealthChecks",
            "AddLogging",
            "AddOptions",
            "Bind",
            "GetService",
            "GetRequiredService",
            "GetServices",
            "Console",
            "WriteLine",
            "ToString",
            "Equals",
            "GetHashCode",
            "GetType",
        ];
        let placeholders = EXTERNAL.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "DELETE FROM unresolved_refs WHERE reference_kind = 'calls' AND reference_name IN ({placeholders})"
        );
        let mut query = sqlx::query(&sql);
        for name in EXTERNAL {
            query = query.bind(*name);
        }
        let result = query.execute(&self.pool).await.map_err(db_err)?;
        Ok(result.rows_affected())
    }

    pub async fn get_stats(&self) -> Result<GraphStats, AxError> {
        let node_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM nodes")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
        let edge_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM edges")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
        let file_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM files")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;

        let nodes_by_kind = self.count_grouped("nodes", "kind").await?;
        let edges_by_kind = self.count_grouped("edges", "kind").await?;
        let docs_by_extension = self.count_doc_extensions().await?;
        let files_by_language = self.count_grouped("files", "language").await?;

        let last_updated: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(updated_at), 0) FROM nodes")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;

        let unresolved_ref_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM unresolved_refs")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;

        let resolution_total = self.parse_metadata_u32("resolution_total").await?;
        let resolution_resolved = self.parse_metadata_u32("resolution_resolved").await?;
        let resolution_unresolved = self.parse_metadata_u32("resolution_unresolved").await?;

        Ok(GraphStats {
            node_count,
            edge_count,
            file_count,
            nodes_by_kind,
            edges_by_kind,
            docs_by_extension,
            files_by_language,
            db_size_bytes: 0,
            last_updated,
            unresolved_ref_count: Some(unresolved_ref_count),
            resolution_total,
            resolution_resolved,
            resolution_unresolved,
        })
    }

    async fn count_grouped(&self, table: &str, column: &str) -> Result<HashMap<String, i64>, AxError> {
        let sql = format!("SELECT {}, COUNT(*) as cnt FROM {} GROUP BY {}", column, table, column);
        let rows: Vec<(String, i64)> = sqlx::query_as(&sql)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
        Ok(rows.into_iter().collect())
    }

    /// Doc nodes grouped by lowercase file extension (`md`, `pdf`, `docx`, …).
    pub async fn count_doc_extensions(&self) -> Result<HashMap<String, i64>, AxError> {
        let rows: Vec<(String, i64)> = sqlx::query_as(
            "SELECT LOWER(CASE \
             WHEN instr(file_path, '.') > 0 THEN substr(file_path, instr(file_path, '.') + 1) \
             ELSE '(no ext)' END) AS ext, COUNT(*) AS cnt \
             FROM nodes WHERE kind = 'doc' GROUP BY ext",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
        Ok(rows.into_iter().collect())
    }

    pub async fn get_last_indexed_at(&self) -> Result<i64, AxError> {
        let result: Option<i64> = sqlx::query_scalar("SELECT MAX(indexed_at) FROM files")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
        Ok(result.unwrap_or(0))
    }

    async fn parse_metadata_u32(&self, key: &str) -> Result<Option<u32>, AxError> {
        match self.get_metadata(key).await? {
            Some(v) => Ok(v.parse().ok()),
            None => Ok(None),
        }
    }

    pub async fn set_metadata(&self, key: &str, value: &str) -> Result<(), AxError> {
        let now = now_ms();
        sqlx::query(
            "INSERT INTO project_metadata (key, value, updated_at) VALUES (?, ?, ?) ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at",
        )
        .bind(key)
        .bind(value)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
        Ok(())
    }

    pub async fn get_metadata(&self, key: &str) -> Result<Option<String>, AxError> {
        let result: Option<String> = sqlx::query_scalar("SELECT value FROM project_metadata WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
        Ok(result)
    }

    pub async fn set_project_name_tokens(&self, tokens: &[String]) -> Result<(), AxError> {
        self.set_metadata("project_name_tokens", &serde_json::to_string(tokens).unwrap_or_default())
            .await
    }

    pub async fn get_project_name_tokens(&self) -> Result<Vec<String>, AxError> {
        match self.get_metadata("project_name_tokens").await? {
            Some(v) => Ok(serde_json::from_str(&v).unwrap_or_else(|_| vec![])),
            None => Ok(vec![]),
        }
    }

    pub async fn get_all_files(&self) -> Result<Vec<FileRecord>, AxError> {
        let rows = sqlx::query_as::<_, FileRow>("SELECT * FROM files ORDER BY path")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
        Ok(rows.into_iter().map(|r| r.into_file()).collect())
    }

    pub async fn clear_all(&self) -> Result<(), AxError> {
        sqlx::query("DELETE FROM edges").execute(&self.pool).await.map_err(db_err)?;
        sqlx::query("DELETE FROM unresolved_refs").execute(&self.pool).await.map_err(db_err)?;
        sqlx::query("DELETE FROM nodes").execute(&self.pool).await.map_err(db_err)?;
        sqlx::query("DELETE FROM files").execute(&self.pool).await.map_err(db_err)?;
        Ok(())
    }

    pub async fn get_top_route_file(&self) -> Result<Option<String>, AxError> {
        let result: Option<String> = sqlx::query_scalar(
            "SELECT file_path FROM nodes WHERE kind = 'route' GROUP BY file_path ORDER BY COUNT(*) DESC LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
        Ok(result)
    }

    pub async fn get_routing_manifest(&self) -> Result<Vec<(String, String)>, AxError> {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT name, file_path FROM nodes WHERE kind = 'route' ORDER BY file_path, name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
        Ok(rows)
    }
}

fn db_err(e: sqlx::Error) -> AxError {
    AxError::Database(DatabaseError::new(e.to_string()))
}

fn score_node_for_query(query: &str, node: &Node) -> f64 {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return 1.0;
    }
    let name = node.name.to_lowercase();
    let qual = node.qualified_name.to_lowercase();
    if name == q {
        return 10.0;
    }
    if qual == q {
        return 9.0;
    }
    if name.starts_with(&q) {
        return 5.0;
    }
    if name.contains(&q) {
        return 2.5;
    }
    if qual.contains(&q) {
        return 1.5;
    }
    1.0
}

fn build_fts_prefix_query(text: &str) -> Option<String> {
    let fts_query = text
        .replace("::", " ")
        .chars()
        .filter(|c| !matches!(c, '\'' | '"' | '*' | '(' | ')' | ':' | '^'))
        .collect::<String>()
        .split_whitespace()
        .filter(|term| {
            term.len() > 0 && !matches!(
                term.to_uppercase().as_str(),
                "AND" | "OR" | "NOT" | "NEAR"
            )
        })
        .map(|term| format!("\"{}\"*", term))
        .collect::<Vec<_>>()
        .join(" OR ");
    if fts_query.is_empty() {
        None
    } else {
        Some(fts_query)
    }
}

#[cfg(test)]
mod tests {
    use super::build_fts_prefix_query;

    #[test]
    fn fts_prefix_splits_rust_qualifier() {
        let q = build_fts_prefix_query("stage_apply::run").unwrap();
        assert!(q.contains("\"stage_apply\"*"));
        assert!(q.contains("\"run\"*"));
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[derive(sqlx::FromRow)]
struct NodeRow {
    id: String,
    kind: String,
    name: String,
    qualified_name: String,
    file_path: String,
    language: String,
    start_line: i32,
    end_line: i32,
    start_column: i32,
    end_column: i32,
    docstring: Option<String>,
    signature: Option<String>,
    visibility: Option<String>,
    is_exported: Option<bool>,
    is_async: Option<bool>,
    is_static: Option<bool>,
    is_abstract: Option<bool>,
    decorators: Option<String>,
    type_parameters: Option<String>,
    return_type: Option<String>,
    updated_at: i64,
}

impl NodeRow {
    fn into_node(self) -> Node {
        Node {
            id: self.id,
            kind: NodeKind::from_str(&self.kind).unwrap_or(NodeKind::Variable),
            name: self.name,
            qualified_name: self.qualified_name,
            file_path: self.file_path,
            language: Language::from_str(&self.language).unwrap_or(Language::Unknown),
            start_line: self.start_line,
            end_line: self.end_line,
            start_column: self.start_column,
            end_column: self.end_column,
            docstring: self.docstring,
            signature: self.signature,
            visibility: self.visibility.as_ref().and_then(|v| match v.as_str() {
                "public" => Some(Visibility::Public),
                "private" => Some(Visibility::Private),
                "protected" => Some(Visibility::Protected),
                "internal" => Some(Visibility::Internal),
                _ => None,
            }),
            is_exported: self.is_exported,
            is_async: self.is_async,
            is_static: self.is_static,
            is_abstract: self.is_abstract,
            decorators: self.decorators.and_then(|d| serde_json::from_str(&d).ok()),
            type_parameters: self.type_parameters.and_then(|t| serde_json::from_str(&t).ok()),
            return_type: self.return_type,
            updated_at: self.updated_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct EdgeRow {
    source: String,
    target: String,
    kind: String,
    metadata: Option<String>,
    line: Option<i32>,
    col: Option<i32>,
    provenance: Option<String>,
    confidence: Option<String>,
}

impl EdgeRow {
    fn into_edge(self) -> Edge {
        let provenance = self.provenance.as_ref().and_then(|p| match p.as_str() {
            "tree-sitter" => Some(Provenance::TreeSitter),
            "scip" => Some(Provenance::Scip),
            "heuristic" => Some(Provenance::Heuristic),
            _ => None,
        });
        // Prefer the stored value; older rows written before v11 fall back to a
        // provenance-derived confidence so downstream consumers always see one.
        let confidence = self
            .confidence
            .as_ref()
            .and_then(|c| EdgeConfidence::from_str(c))
            .or_else(|| EdgeConfidence::from_provenance(provenance));
        Edge {
            source: self.source,
            target: self.target,
            kind: EdgeKind::from_str(&self.kind).unwrap_or(EdgeKind::References),
            metadata: self.metadata.and_then(|m| serde_json::from_str(&m).ok()),
            line: self.line,
            column: self.col,
            provenance,
            confidence,
        }
    }
}

/// Serialize an edge's confidence for storage, deriving from provenance when
/// the edge itself did not carry an explicit value.
fn edge_confidence_str(edge: &Edge) -> Option<&'static str> {
    edge.confidence
        .or_else(|| EdgeConfidence::from_provenance(edge.provenance))
        .map(|c| c.as_str())
}

#[derive(sqlx::FromRow)]
struct UnresolvedRefRow {
    from_node_id: String,
    reference_name: String,
    reference_kind: String,
    line: i32,
    col: i32,
    candidates: Option<String>,
    file_path: String,
    language: String,
}

impl UnresolvedRefRow {
    fn into_ref(self) -> UnresolvedReference {
        UnresolvedReference {
            from_node_id: self.from_node_id,
            reference_name: self.reference_name,
            reference_kind: ReferenceKind::from_str(&self.reference_kind).unwrap_or(ReferenceKind::References),
            line: self.line,
            column: self.col,
            file_path: Some(self.file_path),
            language: Language::from_str(&self.language),
            candidates: self.candidates.and_then(|c| serde_json::from_str(&c).ok()),
        }
    }
}

#[derive(sqlx::FromRow)]
struct FileRow {
    path: String,
    content_hash: String,
    language: String,
    size: i64,
    modified_at: i64,
    indexed_at: i64,
    node_count: Option<i64>,
    errors: Option<String>,
}

impl FileRow {
    fn into_file(self) -> FileRecord {
        FileRecord {
            path: self.path,
            content_hash: self.content_hash,
            language: Language::from_str(&self.language).unwrap_or(Language::Unknown),
            size: self.size,
            modified_at: self.modified_at,
            indexed_at: self.indexed_at,
            node_count: self.node_count.unwrap_or(0),
            errors: self.errors.and_then(|e| serde_json::from_str(&e).ok()),
        }
    }
}
