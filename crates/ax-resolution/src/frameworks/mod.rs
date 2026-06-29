//! Framework-specific post-extract passes.

pub mod angular;
pub mod extract;
mod cargo_workspace;
mod express;
pub mod go;
pub mod flask;
pub mod django;
pub mod laravel;
pub mod nestjs;
pub mod react;
pub mod rust;
pub mod vue;
pub mod svelte;
pub mod spring;

use std::path::Path;

use ax_db::queries::QueryBuilder;
use ax_types::{EdgeKind, Provenance};

use extract::{is_js_family, FrameworkExtractResult};

pub struct FrameworkRegistry {
    frameworks: Vec<&'static str>,
}

impl FrameworkRegistry {
    pub fn new() -> Self {
        Self {
            frameworks: vec![
                "astro", "cargo-workspace", "csharp", "drupal", "expo-modules",
                "express", "fabric", "go", "goframe", "java", "laravel", "nestjs",
                "play", "python", "react", "react-native", "ruby", "rust", "svelte",
                "swift", "swift-objc", "vue",
            ],
        }
    }

    pub fn detected_frameworks(&self) -> &[&'static str] {
        &self.frameworks
    }

    pub async fn run_post_extract(
        &self,
        project_root: &Path,
        queries: &QueryBuilder,
    ) -> Result<(), ax_utils::errors::AxError> {
        let files = queries.get_all_files().await?;
        let file_paths: Vec<String> = files.iter().map(|f| f.path.clone()).collect();
        let express_active = express::detect(project_root);
        let react_active = react::detect(project_root);
        let nestjs_active = nestjs::detect(project_root);
        let go_active = go::detect(project_root, &file_paths);
        let rust_active = rust::detect(project_root);
        let laravel_active = laravel::detect(project_root);
        let django_active = django::detect(project_root);
        let flask_active = flask::flask_detect(project_root, &file_paths);
        let fastapi_active = flask::fastapi_detect(project_root);
        let vue_active = vue::detect(project_root, &file_paths);
        let svelte_active = svelte::detect(project_root, &file_paths);
        let spring_active = spring::detect(project_root, &file_paths);
        let angular_active = angular::detect(project_root);
        if !express_active && !react_active && !nestjs_active && !go_active && !rust_active && !laravel_active && !django_active && !flask_active && !fastapi_active && !vue_active && !svelte_active && !spring_active && !angular_active {
            return Ok(());
        }

        for file in files {
            let full = project_root.join(&file.path);
            let content = std::fs::read_to_string(&full).unwrap_or_default();
            if content.is_empty() {
                continue;
            }

            let mut extracted = FrameworkExtractResult::default();
            if rust_active && file.path.ends_with(".rs") {
                merge_extract(&mut extracted, rust::extract_file(&file.path, &content));
            }
            if go_active && file.path.ends_with(".go") {
                merge_extract(&mut extracted, go::extract_file(&file.path, &content));
            }
            if django_active && file.path.ends_with(".py") {
                merge_extract(&mut extracted, django::extract_file(&file.path, &content));
            }
            if flask_active && file.path.ends_with(".py") {
                merge_extract(&mut extracted, flask::flask_extract_file(&file.path, &content));
            }
            if fastapi_active && file.path.ends_with(".py") {
                merge_extract(&mut extracted, flask::fastapi_extract_file(&file.path, &content));
            }
            if laravel_active && file.path.ends_with(".php") {
                merge_extract(&mut extracted, laravel::extract_file(&file.path, &content));
            }
            if vue_active {
                merge_extract(&mut extracted, vue::extract_file(&file.path, &content));
            }
            
            if spring_active {
                if file.path.ends_with(".java") || file.path.ends_with(".kt")
                    || spring::is_spring_config_path(&file.path)
                {
                    merge_extract(&mut extracted, spring::extract_file(&file.path, &content));
                }
            }
            if angular_active && file.path.ends_with(".ts") {
                merge_extract(&mut extracted, angular::extract_file(&file.path, &content));
            }
            if is_js_family(&file.path) {
                if express_active {
                    merge_extract(&mut extracted, express::extract_file(&file.path, &content));
                }
                if react_active {
                    merge_extract(&mut extracted, react::extract_file(&file.path, &content));
                }
                if nestjs_active {
                    merge_extract(&mut extracted, nestjs::extract_file(&file.path, &content));
                }
            }

            if extracted.nodes.is_empty() && extracted.references.is_empty() && extracted.edges.is_empty() {
                continue;
            }
            apply_extract(queries, &file.path, extracted).await?;
        }

        Ok(())
    }
}

fn merge_extract(dst: &mut FrameworkExtractResult, src: FrameworkExtractResult) {
    dst.nodes.extend(src.nodes);
    dst.references.extend(src.references);
    dst.edges.extend(src.edges);
}

async fn apply_extract(
    queries: &QueryBuilder,
    file_path: &str,
    extracted: FrameworkExtractResult,
) -> Result<(), ax_utils::errors::AxError> {
    if extracted.nodes.is_empty() && extracted.references.is_empty() && extracted.edges.is_empty() {
        return Ok(());
    }

    let file_id = extract::stable_node_id(file_path, file_path);
    queries.upsert_nodes(&extracted.nodes).await?;
    for node in &extracted.nodes {
        let edge = ax_types::Edge {
            source: file_id.clone(),
            target: node.id.clone(),
            kind: EdgeKind::Contains,
            metadata: None,
            line: Some(node.start_line),
            column: Some(node.start_column),
            provenance: Some(Provenance::Heuristic),
        };
        queries.upsert_edge(&edge).await?;
    }
    queries.upsert_edges(&extracted.edges).await?;
    queries.insert_unresolved_refs(&extracted.references).await?;
    Ok(())
}
