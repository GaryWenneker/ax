//! GitHub API via octocrab.

use async_trait::async_trait;
use octocrab::Octocrab;

use crate::provider::{DraftPrRequest, PrProvider, PrRef, ReviewComment};

pub struct GithubProvider {
    client: Octocrab,
    owner: String,
    repo: String,
}

impl GithubProvider {
    pub fn new(token: String, owner: String, repo: String) -> Result<Self, octocrab::Error> {
        let client = Octocrab::builder().personal_token(token).build()?;
        Ok(Self { client, owner, repo })
    }
}

#[async_trait]
impl PrProvider for GithubProvider {
    async fn create_draft_pr(&self, req: DraftPrRequest) -> Result<PrRef, String> {
        let title = req.title;
        let head = req.head_branch;
        let base = req.base_branch;
        let body = req.body;
        let pulls = self.client.pulls(&self.owner, &self.repo);
        let mut op = pulls.create(title.as_str(), head.as_str(), base.as_str());
        op = op.body(body);
        if req.draft {
            op = op.draft(true);
        }
        let pr = op.send().await.map_err(|e| e.to_string())?;
        Ok(PrRef {
            number: pr.number,
            url: pr.html_url.map(|u| u.to_string()).unwrap_or_default(),
            provider: "github".into(),
        })
    }

    async fn request_reviewers(&self, pr: &PrRef, reviewers: &[String]) -> Result<(), String> {
        self.client
            ._post(
                format!(
                    "/repos/{}/{}/pulls/{}/requested_reviewers",
                    self.owner, self.repo, pr.number
                ),
                Some(&serde_json::json!({ "reviewers": reviewers })),
            )
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    async fn list_comments(&self, pr: &PrRef) -> Result<Vec<ReviewComment>, String> {
        let comments = self
            .client
            .issues(&self.owner, &self.repo)
            .list_comments(pr.number)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        Ok(comments
            .items
            .into_iter()
            .map(|c| ReviewComment {
                id: c.id.to_string(),
                author: c.user.login,
                body: c.body.unwrap_or_default(),
                path: None,
                line: None,
            })
            .collect())
    }
}
