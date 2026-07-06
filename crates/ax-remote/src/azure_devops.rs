//! Azure DevOps REST API PR integration.

use async_trait::async_trait;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};

use crate::provider::{DraftPrRequest, PrProvider, PrRef, ReviewComment};

pub struct AzureDevOpsProvider {
    client: reqwest::Client,
    org: String,
    project: String,
    repo_id: String,
    auth_header: String,
}

impl AzureDevOpsProvider {
    pub fn new(pat: String, org: String, project: String, repo_id: String) -> Self {
        use base64::Engine;
        let token = base64::engine::general_purpose::STANDARD.encode(format!(":{pat}"));
        Self {
            client: reqwest::Client::new(),
            org,
            project,
            repo_id,
            auth_header: format!("Basic {token}"),
        }
    }

    fn base_url(&self) -> String {
        format!(
            "https://dev.azure.com/{}/{}/_apis/git/repositories/{}",
            self.org, self.project, self.repo_id
        )
    }
}

#[derive(Serialize)]
struct AdoPrCreate {
    sourceRefName: String,
    targetRefName: String,
    title: String,
    description: String,
    isDraft: bool,
}

#[derive(Deserialize)]
struct AdoPrResponse {
    pullRequestId: u64,
    url: String,
}

#[async_trait]
impl PrProvider for AzureDevOpsProvider {
    async fn create_draft_pr(&self, req: DraftPrRequest) -> Result<PrRef, String> {
        let url = format!("{}/pullrequests?api-version=7.1", self.base_url());
        let body = AdoPrCreate {
            sourceRefName: format!("refs/heads/{}", req.head_branch),
            targetRefName: format!("refs/heads/{}", req.base_branch),
            title: req.title,
            description: req.body,
            isDraft: req.draft,
        };
        let resp = self
            .client
            .post(&url)
            .header(AUTHORIZATION, &self.auth_header)
            .header(CONTENT_TYPE, "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("AzDO create PR: {}", resp.status()));
        }
        let pr: AdoPrResponse = resp.json().await.map_err(|e| e.to_string())?;
        Ok(PrRef {
            number: pr.pullRequestId,
            url: pr.url,
            provider: "azure_devops".into(),
        })
    }

    async fn request_reviewers(&self, pr: &PrRef, reviewers: &[String]) -> Result<(), String> {
        let url = format!(
            "{}/pullrequests/{}/reviewers?api-version=7.1",
            self.base_url(),
            pr.number
        );
        for reviewer in reviewers {
            let body = serde_json::json!({ "id": reviewer });
            self.client
                .put(&url)
                .header(AUTHORIZATION, &self.auth_header)
                .header(CONTENT_TYPE, "application/json")
                .json(&body)
                .send()
                .await
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    async fn list_comments(&self, pr: &PrRef) -> Result<Vec<ReviewComment>, String> {
        let url = format!(
            "{}/pullrequests/{}/threads?api-version=7.1",
            self.base_url(),
            pr.number
        );
        let resp = self
            .client
            .get(&url)
            .header(AUTHORIZATION, &self.auth_header)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        if let Some(threads) = body.get("value").and_then(|v| v.as_array()) {
            for thread in threads {
                if let Some(comments) = thread.get("comments").and_then(|c| c.as_array()) {
                    for c in comments {
                        out.push(ReviewComment {
                            id: c
                                .get("id")
                                .and_then(|v| v.as_i64())
                                .map(|n| n.to_string())
                                .unwrap_or_default(),
                            author: c
                                .get("author")
                                .and_then(|a| a.get("displayName"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .into(),
                            body: c
                                .get("content")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .into(),
                            path: thread
                                .get("threadContext")
                                .and_then(|tc| tc.get("filePath"))
                                .and_then(|v| v.as_str())
                                .map(String::from),
                            line: thread
                                .get("threadContext")
                                .and_then(|tc| tc.get("rightFileStart"))
                                .and_then(|r| r.get("line"))
                                .and_then(|v| v.as_u64())
                                .map(|n| n as u32),
                        });
                    }
                }
            }
        }
        Ok(out)
    }
}
