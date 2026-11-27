//! Framework-specific post-extract passes.

pub mod angular;
pub mod cargo_workspace;
pub mod django;
mod express;
pub mod extract;
pub mod flask;
pub mod go;
pub mod laravel;
pub mod nestjs;
pub mod react;
pub mod rust;
pub mod spring;
pub mod svelte;
pub mod vue;

use std::path::Path;
use std::sync::Mutex;

use ax_db::queries::QueryBuilder;
use ax_types::{EdgeKind, Provenance};

use crate::resolver::ResolutionScope;
use extract::{is_js_family, FrameworkExtractResult};

/// Which framework resolvers apply to this project, decided once per run.
#[derive(Debug, Clone, Copy, Default)]
pub struct FrameworkGates {
    /// Flask or FastAPI was detected.
    pub flask: bool,
    /// Django was detected.
    pub django: bool,
    /// Spring was detected.
    pub spring: bool,
    /// Vue or Nuxt was detected.
    pub vue: bool,
    /// Svelte or SvelteKit was detected.
    pub svelte: bool,
}

pub struct FrameworkRegistry {
    frameworks: Vec<&'static str>,
    /// What the last detection found. A full pass detects over every file
    /// and replaces it; a scoped pass (one watcher sync) only adds what its
    /// own files show, so it never rescans the whole repository.
    detected: Mutex<Option<Detected>>,
}

impl Default for FrameworkRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Which framework extractors apply to this project.
#[derive(Debug, Clone, Copy, Default)]
struct Detected {
    express: bool,
    react: bool,
    nestjs: bool,
    go: bool,
    rust: bool,
    laravel: bool,
    django: bool,
    flask: bool,
    fastapi: bool,
    vue: bool,
    svelte: bool,
    spring: bool,
    angular: bool,
}

impl Detected {
    /// Read the project manifests and look at `file_paths` (some detectors
    /// open those files, so pass only the files that need checking).
    fn detect(project_root: &Path, file_paths: &[String]) -> Self {
        Self {
            express: express::detect(project_root),
            react: react::detect(project_root),
            nestjs: nestjs::detect(project_root),
            go: go::detect(project_root, file_paths),
            rust: rust::detect(project_root),
            laravel: laravel::detect(project_root),
            django: django::detect(project_root),
            flask: flask::flask_detect(project_root, file_paths),
            fastapi: flask::fastapi_detect(project_root),
            vue: vue::detect(project_root, file_paths),
            svelte: svelte::detect(project_root, file_paths),
            spring: spring::detect(project_root, file_paths),
            angular: angular::detect(project_root),
        }
    }

    fn union(self, o: Self) -> Self {
        Self {
            express: self.express || o.express,
            react: self.react || o.react,
            nestjs: self.nestjs || o.nestjs,
            go: self.go || o.go,
            rust: self.rust || o.rust,
            laravel: self.laravel || o.laravel,
            django: self.django || o.django,
            flask: self.flask || o.flask,
            fastapi: self.fastapi || o.fastapi,
            vue: self.vue || o.vue,
            svelte: self.svelte || o.svelte,
            spring: self.spring || o.spring,
            angular: self.angular || o.angular,
        }
    }

    fn any(&self) -> bool {
        self.express
            || self.react
            || self.nestjs
            || self.go
            || self.rust
            || self.laravel
            || self.django
            || self.flask
            || self.fastapi
            || self.vue
            || self.svelte
            || self.spring
            || self.angular
    }

    fn gates(&self) -> FrameworkGates {
        FrameworkGates {
            flask: self.flask || self.fastapi,
            django: self.django,
            spring: self.spring,
            vue: self.vue,
            svelte: self.svelte,
        }
    }

    /// Every framework node, reference and edge one file yields.
    fn extract(&self, path: &str, content: &str) -> FrameworkExtractResult {
        let mut out = FrameworkExtractResult::default();
        if self.rust && path.ends_with(".rs") {
            merge_extract(&mut out, rust::extract_file(path, content));
        }
        if self.go && path.ends_with(".go") {
            merge_extract(&mut out, go::extract_file(path, content));
        }
        if path.ends_with(".py") {
            if self.django {
                merge_extract(&mut out, django::extract_file(path, content));
            }
            if self.flask {
                merge_extract(&mut out, flask::flask_extract_file(path, content));
            }
            if self.fastapi {
                merge_extract(&mut out, flask::fastapi_extract_file(path, content));
            }
        }
        if self.laravel && path.ends_with(".php") {
            merge_extract(&mut out, laravel::extract_file(path, content));
        }
        if self.vue {
            merge_extract(&mut out, vue::extract_file(path, content));
        }
        if self.svelte {
            merge_extract(&mut out, svelte::extract_file(path, content));
        }
        if self.spring
            && (path.ends_with(".java")
                || path.ends_with(".kt")
                || spring::is_spring_config_path(path))
        {
            merge_extract(&mut out, spring::extract_file(path, content));
        }
        if self.angular && path.ends_with(".ts") {
            merge_extract(&mut out, angular::extract_file(path, content));
        }
        if is_js_family(path) {
            if self.express {
                merge_extract(&mut out, express::extract_file(path, content));
            }
            if self.react {
                merge_extract(&mut out, react::extract_file(path, content));
            }
            if self.nestjs {
                merge_extract(&mut out, nestjs::extract_file(path, content));
            }
        }
        out
    }
}

impl FrameworkRegistry {
    pub fn new() -> Self {
        Self {
            frameworks: vec![
                "astro",
                "cargo-workspace",
                "csharp",
                "drupal",
                "expo-modules",
                "express",
                "fabric",
                "go",
                "goframe",
                "java",
                "laravel",
                "nestjs",
                "play",
                "python",
                "react",
                "react-native",
                "ruby",
                "rust",
                "svelte",
                "swift",
                "swift-objc",
                "vue",
            ],
            detected: Mutex::new(None),
        }
    }

    pub fn detected_frameworks(&self) -> &[&'static str] {
        &self.frameworks
    }

    pub async fn run_post_extract(
        &self,
        project_root: &Path,
        queries: &QueryBuilder,
        scope: ResolutionScope<'_>,
    ) -> Result<FrameworkGates, ax_utils::errors::AxError> {
        // Indexed files only: the pass hangs its nodes under their file nodes.
        let all: Vec<String> = queries
            .get_all_files()
            .await?
            .into_iter()
            .map(|f| f.path)
            .collect();
        let scoped: Option<Vec<String>> = scope.filter(|s| !s.is_empty()).map(|s| {
            let wanted: std::collections::HashSet<&str> = s.iter().map(String::as_str).collect();
            all.iter()
                .filter(|p| wanted.contains(p.as_str()))
                .cloned()
                .collect()
        });
        let cached = *self.detected.lock().unwrap_or_else(|e| e.into_inner());

        // A full pass, or the first pass of this registry, detects over every
        // file; a later scoped pass only looks at the files it was given.
        let (paths, detected) = match (scoped, cached) {
            (Some(paths), Some(prev)) => {
                let detected = prev.union(detect_blocking(project_root, paths.clone()).await?);
                (paths, detected)
            }
            (scoped, _) => {
                let detected = detect_blocking(project_root, all.clone()).await?;
                (scoped.unwrap_or(all), detected)
            }
        };
        *self.detected.lock().unwrap_or_else(|e| e.into_inner()) = Some(detected);
        if !detected.any() {
            return Ok(detected.gates());
        }

        for path in paths {
            let content = tokio::fs::read_to_string(project_root.join(&path))
                .await
                .unwrap_or_default();
            if content.is_empty() {
                continue;
            }
            let extracted = detected.extract(&path, &content);
            if extracted.nodes.is_empty()
                && extracted.references.is_empty()
                && extracted.edges.is_empty()
            {
                continue;
            }
            apply_extract(queries, &path, extracted).await?;
        }

        Ok(detected.gates())
    }
}

/// Run detection off the async worker: it reads manifests and, for some
/// frameworks, the given source files.
async fn detect_blocking(
    project_root: &Path,
    paths: Vec<String>,
) -> Result<Detected, ax_utils::errors::AxError> {
    let root = project_root.to_path_buf();
    tokio::task::spawn_blocking(move || Detected::detect(&root, &paths))
        .await
        .map_err(|e| ax_utils::errors::AxError::Other(format!("framework detection: {e}")))
}

fn merge_extract(dst: &mut FrameworkExtractResult, src: FrameworkExtractResult) {
    dst.nodes.extend(src.nodes);
    dst.references.extend(src.references);
    dst.edges.extend(src.edges);
}

async fn apply_extract(
    queries: &QueryBuilder,
    file_path: &str,
    mut extracted: FrameworkExtractResult,
) -> Result<(), ax_utils::errors::AxError> {
    extracted.dedup_nodes();
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
            confidence: Some(ax_types::EdgeConfidence::Inferred),
        };
        queries.upsert_edge(&edge).await?;
    }
    queries.upsert_edges(&extracted.edges).await?;
    queries
        .insert_unresolved_refs(&extracted.references)
        .await?;
    Ok(())
}
