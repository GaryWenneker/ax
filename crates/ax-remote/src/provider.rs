//! PR provider trait.

use async_trait::async_trait;
use std::path::PathBuf;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DraftPrRequest {
    pub title: String,
    pub body: String,
    pub head_branch: String,
    pub base_branch: String,
    pub draft: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PrRef {
    pub number: u64,
    pub url: String,
    pub provider: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReviewComment {
    pub id: String,
    pub author: String,
    pub body: String,
    pub path: Option<String>,
    pub line: Option<u32>,
}

#[async_trait]
pub trait PrProvider: Send + Sync {
    async fn create_draft_pr(&self, req: DraftPrRequest) -> Result<PrRef, String>;
    async fn request_reviewers(&self, pr: &PrRef, reviewers: &[String]) -> Result<(), String>;
    async fn list_comments(&self, pr: &PrRef) -> Result<Vec<ReviewComment>, String>;
    async fn suggest_reviewers(&self, changed_files: &[PathBuf]) -> Result<Vec<String>, String> {
        let _ = changed_files;
        Ok(vec![])
    }
}
