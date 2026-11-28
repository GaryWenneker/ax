//! Pull/push shared policy via a GitLab-compatible REST API (`/api/v4`)
//! using a personal/project access token.
//!
//! Some self-hosted GitLab instances sit behind a corporate SSO proxy that
//! unconditionally redirects the git smart-HTTP endpoint (and SSH) to an
//! interactive browser login, so a machine credential can never complete a
//! plain `git clone`/`git push`. Those same instances typically still expose
//! a scoped `/api/v4` surface for automation. This module implements the
//! same pull/push contract as [`crate::providers::github`] but over that API
//! instead of shelling out to `git`.

use std::collections::HashMap;
use std::path::Path;

use base64::Engine;
use reqwest::{Client, StatusCode};
use serde::Deserialize;

use crate::config::GithubShareConfig;
use crate::providers::PullResult;

/// Client that never follows redirects.
///
/// Some SSO-gated hosts respond to unauthenticated/ambient-credential
/// requests with a 3xx to an entirely different login host (e.g.
/// `auth.example.com`). A default reqwest client would silently follow
/// that redirect — leaking our `PRIVATE-TOKEN` header to a third-party host
/// and then failing with a confusing "error decoding response body" once it
/// tries to parse the resulting login HTML page as JSON. Disabling redirects
/// lets us detect this case explicitly and fail with a clear diagnostic
/// instead.
fn api_client() -> Result<Client, String> {
    Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| e.to_string())
}

/// Fail with a clear, actionable message if the response is a redirect
/// (rather than following it, which could leak the token cross-host).
fn reject_sso_redirect(resp: &reqwest::Response) -> Result<(), String> {
    if !resp.status().is_redirection() {
        return Ok(());
    }
    let location = resp
        .headers()
        .get(reqwest::header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("<no Location header>");
    let redirect_host = reqwest::Url::parse(location)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_string()))
        .unwrap_or_else(|| location.to_string());
    Err(format!(
        "GitLab API request was redirected (HTTP {}) to '{redirect_host}' instead of returning \
         data. This host now requires an authenticated browser SSO session even for /api/v4 \
         calls, so the configured token can't be used headlessly right now. Ask whoever \
         administers this GitLab instance to exempt /api/v4 from the SSO gate (or provision a \
         durable service/CI token or a non-purged SSH deploy key) — sync can't work around an \
         SSO login wall.",
        resp.status()
    ))
}

struct GitlabTarget {
    api_base: String,
    project_id: String,
}

fn parse_target(repo_url: &str) -> Result<GitlabTarget, String> {
    let trimmed = repo_url
        .trim()
        .trim_end_matches(".git")
        .trim_end_matches('/');
    let url = reqwest::Url::parse(trimmed).map_err(|e| format!("invalid repo URL: {e}"))?;
    let scheme = url.scheme();
    if scheme != "http" && scheme != "https" {
        return Err(format!(
            "GitLab API transport requires an http(s) repo URL, got scheme '{scheme}'"
        ));
    }
    let host = url.host_str().ok_or("repo URL is missing a host")?;
    let authority = match url.port() {
        Some(p) => format!("{host}:{p}"),
        None => host.to_string(),
    };
    let project_path = url.path().trim_matches('/');
    if project_path.is_empty() {
        return Err("repo URL is missing a namespace/project path".into());
    }
    Ok(GitlabTarget {
        api_base: format!("{scheme}://{authority}/api/v4"),
        project_id: percent_encode_path(project_path),
    })
}

/// Minimal percent-encoding sufficient for GitLab project paths and file
/// paths (avoids pulling in a dedicated URL-encoding crate).
fn percent_encode_path(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[derive(Debug, Deserialize)]
struct TreeEntry {
    #[serde(rename = "type")]
    kind: String,
    path: String,
}

async fn list_tree_recursive(
    client: &Client,
    target: &GitlabTarget,
    token: &str,
    subpath: &str,
    branch: &str,
) -> Result<Vec<String>, String> {
    let mut files = Vec::new();
    let mut page = 1u32;
    loop {
        let mut req = client
            .get(format!(
                "{}/projects/{}/repository/tree",
                target.api_base, target.project_id
            ))
            .query(&[
                ("ref", branch),
                ("recursive", "true"),
                ("per_page", "100"),
                ("page", &page.to_string()),
            ])
            .header("PRIVATE-TOKEN", token);
        if !subpath.is_empty() {
            req = req.query(&[("path", subpath)]);
        }
        let resp = req.send().await.map_err(|e| e.to_string())?;
        if resp.status() == StatusCode::NOT_FOUND {
            // Path doesn't exist yet in the repo — treat as empty, not fatal.
            return Ok(files);
        }
        reject_sso_redirect(&resp)?;
        let resp = resp.error_for_status().map_err(|e| e.to_string())?;
        let batch: Vec<TreeEntry> = resp.json().await.map_err(|e| e.to_string())?;
        let got = batch.len();
        for entry in batch {
            if entry.kind == "blob" {
                files.push(entry.path);
            }
        }
        if got < 100 {
            break;
        }
        page += 1;
    }
    Ok(files)
}

async fn get_raw_file(
    client: &Client,
    target: &GitlabTarget,
    token: &str,
    path: &str,
    branch: &str,
) -> Result<Vec<u8>, String> {
    let encoded = percent_encode_path(path);
    let resp = client
        .get(format!(
            "{}/projects/{}/repository/files/{}/raw",
            target.api_base, target.project_id, encoded
        ))
        .query(&[("ref", branch)])
        .header("PRIVATE-TOKEN", token)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    reject_sso_redirect(&resp)?;
    let resp = resp.error_for_status().map_err(|e| e.to_string())?;
    Ok(resp.bytes().await.map_err(|e| e.to_string())?.to_vec())
}

pub async fn pull_gitlab_api(
    config: &GithubShareConfig,
    dest_root: &Path,
) -> Result<PullResult, String> {
    let url = config.repo_url.trim();
    if url.is_empty() {
        return Err("GitHub repo URL is not configured".into());
    }
    let token = config.token.trim();
    if token.is_empty() {
        return Err("API token is not configured".into());
    }
    let branch = if config.branch.trim().is_empty() {
        "main"
    } else {
        config.branch.trim()
    };
    let sub = config.subpath.trim().trim_matches('/');

    let target = parse_target(url)?;
    let client = api_client()?;

    let remote_files = list_tree_recursive(&client, &target, token, sub, branch).await?;

    let pull_dir = dest_root.join("pull");
    if pull_dir.exists() {
        std::fs::remove_dir_all(&pull_dir).map_err(|e| e.to_string())?;
    }
    std::fs::create_dir_all(&pull_dir).map_err(|e| e.to_string())?;

    let prefix = if sub.is_empty() {
        String::new()
    } else {
        format!("{sub}/")
    };

    let mut files = 0usize;
    let mut pack_dir = None;
    let mut memory_file = None;

    for remote_path in &remote_files {
        let rel = remote_path
            .strip_prefix(&prefix)
            .unwrap_or(remote_path.as_str());
        let is_pack_file = rel.starts_with("policy/shared/");
        let is_memory_file = rel == "memory/shared.jsonl";
        if !is_pack_file && !is_memory_file {
            continue;
        }
        // The tree listing comes from the server; a path with `..` or a link
        // out of `pull_dir` must never decide where a file is written.
        let dest = ax_utils::contained_join(&pull_dir, rel).map_err(|e| {
            format!("refusing remote path '{remote_path}' outside the pull folder: {e}")
        })?;
        let content = get_raw_file(&client, &target, token, remote_path, branch).await?;
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&dest, &content).map_err(|e| e.to_string())?;
        files += 1;
        if is_pack_file {
            pack_dir = Some(pull_dir.join("policy").join("shared"));
        }
        if is_memory_file {
            memory_file = Some(pull_dir.join("memory").join("shared.jsonl"));
        }
    }

    if pack_dir.is_none() && memory_file.is_none() {
        return Err(format!(
            "No policy/shared or memory/shared.jsonl found under '{sub}' in repo (via API)"
        ));
    }

    Ok(PullResult {
        pack_dir,
        memory_file,
        files_copied: files,
    })
}

#[derive(serde::Serialize)]
struct CommitAction<'a> {
    action: &'a str,
    file_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    encoding: &'a str,
}

#[derive(serde::Serialize)]
struct CreateCommit<'a> {
    branch: &'a str,
    commit_message: &'a str,
    actions: Vec<CommitAction<'a>>,
}

pub async fn push_gitlab_api(
    config: &GithubShareConfig,
    pack_dir: &Path,
    memory_file: Option<&Path>,
) -> Result<usize, String> {
    let url = config.repo_url.trim();
    if url.is_empty() {
        return Err("GitHub repo URL is not configured".into());
    }
    let token = config.token.trim();
    if token.is_empty() {
        return Err("API token is not configured".into());
    }
    let branch = if config.branch.trim().is_empty() {
        "main"
    } else {
        config.branch.trim()
    };
    let sub = config.subpath.trim().trim_matches('/');
    let prefix = if sub.is_empty() {
        String::new()
    } else {
        format!("{sub}/")
    };

    let target = parse_target(url)?;
    let client = api_client()?;

    // This push owns only the content types it exports: the pack when one was
    // written, the memory file when one was given. Remote files of any other
    // type are left alone, so they are neither diffed nor deleted.
    let owns_pack = pack_dir.is_dir();
    let owns_memory = memory_file.is_some();

    // Snapshot what's already on the remote so we can diff (skip unchanged
    // files, pick create/update per-file, and delete anything now missing).
    let remote_files = list_tree_recursive(&client, &target, token, sub, branch).await?;
    let mut remote_contents: HashMap<String, Vec<u8>> = HashMap::new();
    for remote_path in &remote_files {
        let rel = remote_path
            .strip_prefix(&prefix)
            .unwrap_or(remote_path.as_str());
        let owned = (owns_pack && rel.starts_with("policy/shared/"))
            || (owns_memory && rel == "memory/shared.jsonl");
        if owned {
            let content = get_raw_file(&client, &target, token, remote_path, branch).await?;
            remote_contents.insert(rel.to_string(), content);
        }
    }

    let mut local_files: HashMap<String, Vec<u8>> = HashMap::new();
    if pack_dir.is_dir() {
        collect_files(pack_dir, "policy/shared", &mut local_files)?;
    }
    if let Some(mem) = memory_file {
        if mem.is_file() {
            let content = std::fs::read(mem).map_err(|e| e.to_string())?;
            local_files.insert("memory/shared.jsonl".to_string(), content);
        }
    }

    let mut actions = Vec::new();
    for (rel, content) in &local_files {
        let unchanged = remote_contents
            .get(rel)
            .map(|r| r == content)
            .unwrap_or(false);
        if unchanged {
            continue;
        }
        let action = if remote_contents.contains_key(rel) {
            "update"
        } else {
            "create"
        };
        actions.push(CommitAction {
            action,
            file_path: format!("{prefix}{rel}"),
            content: Some(base64::engine::general_purpose::STANDARD.encode(content)),
            encoding: "base64",
        });
    }
    for rel in remote_contents.keys() {
        if !local_files.contains_key(rel) {
            actions.push(CommitAction {
                action: "delete",
                file_path: format!("{prefix}{rel}"),
                content: None,
                encoding: "base64",
            });
        }
    }

    if actions.is_empty() {
        return Ok(0);
    }
    let written = actions.iter().filter(|a| a.action != "delete").count();

    let body = CreateCommit {
        branch,
        commit_message: "ax: policy share sync",
        actions,
    };

    let commit_resp = client
        .post(format!(
            "{}/projects/{}/repository/commits",
            target.api_base, target.project_id
        ))
        .header("PRIVATE-TOKEN", token)
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    reject_sso_redirect(&commit_resp)?;
    commit_resp.error_for_status().map_err(|e| e.to_string())?;

    Ok(written)
}

fn collect_files(
    dir: &Path,
    rel_prefix: &str,
    out: &mut HashMap<String, Vec<u8>>,
) -> Result<(), String> {
    for entry in walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let rel = entry
            .path()
            .strip_prefix(dir)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        let content = std::fs::read(entry.path()).map_err(|e| e.to_string())?;
        out.insert(format!("{rel_prefix}/{rel}"), content);
    }
    Ok(())
}

/// Whether this config should use the GitLab REST API transport instead of
/// raw git — true when an API token is configured and the repo URL is
/// http(s) (SSH URLs like `git@host:ns/proj.git` always use raw git).
pub fn should_use_api(config: &GithubShareConfig) -> bool {
    if config.token.trim().is_empty() {
        return false;
    }
    let url = config.repo_url.trim();
    url.starts_with("http://") || url.starts_with("https://")
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn cfg(repo_url: String, token: &str) -> GithubShareConfig {
        GithubShareConfig {
            repo_url,
            branch: "main".to_string(),
            subpath: ".ax".to_string(),
            token: token.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn parse_target_extracts_api_base_and_project_id() {
        let target = parse_target("https://gitlab.example.com/acme/contoso-web.git").unwrap();
        assert_eq!(target.api_base, "https://gitlab.example.com/api/v4");
        assert_eq!(target.project_id, "acme%2Fcontoso-web");
    }

    #[test]
    fn should_use_api_requires_token_and_http() {
        assert!(should_use_api(&cfg(
            "https://gitlab.example.com/a/b".to_string(),
            "tok"
        )));
        assert!(!should_use_api(&cfg(
            "https://gitlab.example.com/a/b".to_string(),
            ""
        )));
        assert!(!should_use_api(&cfg(
            "git@gitlab.example.com:a/b.git".to_string(),
            "tok"
        )));
    }

    #[tokio::test]
    async fn push_then_pull_round_trips_via_api() {
        let server = MockServer::start().await;
        let repo_url = format!("{}/acme/contoso-web", server.uri());
        let config = cfg(repo_url, "test-token");
        let project_id = "acme%2Fcontoso-web";

        // Push: remote starts empty.
        Mock::given(method("GET"))
            .and(path(format!(
                "/api/v4/projects/{project_id}/repository/tree"
            )))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path(format!(
                "/api/v4/projects/{project_id}/repository/commits"
            )))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .respond_with(
                ResponseTemplate::new(201).set_body_json(serde_json::json!({"id": "abc123"})),
            )
            .mount(&server)
            .await;

        let pack_src = tempfile::TempDir::new().unwrap();
        std::fs::write(
            pack_src.path().join("manifest.json"),
            r#"{"version":1,"rules":[],"skills":[]}"#,
        )
        .unwrap();

        let written = push_gitlab_api(&config, pack_src.path(), None)
            .await
            .expect("push should succeed");
        assert_eq!(written, 1);

        // Pull: remote now reports the file we just "pushed".
        server.reset().await;
        Mock::given(method("GET"))
            .and(path(format!(
                "/api/v4/projects/{project_id}/repository/tree"
            )))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .and(query_param("page", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"id": "1", "type": "blob", "path": ".ax/policy/shared/manifest.json"}
            ])))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path(format!(
                "/api/v4/projects/{project_id}/repository/files/.ax%2Fpolicy%2Fshared%2Fmanifest.json/raw"
            )))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(r#"{"version":1,"rules":[],"skills":[]}"#),
            )
            .mount(&server)
            .await;

        let dest = tempfile::TempDir::new().unwrap();
        let pulled = pull_gitlab_api(&config, dest.path())
            .await
            .expect("pull should succeed");
        let pack_dir = pulled.pack_dir.expect("pack_dir should be set");
        assert!(pack_dir.join("manifest.json").is_file());
    }

    #[tokio::test]
    async fn pull_fails_clearly_on_sso_redirect_instead_of_following_it() {
        let server = MockServer::start().await;
        let repo_url = format!("{}/acme/contoso-web", server.uri());
        let config = cfg(repo_url, "test-token");
        let project_id = "acme%2Fcontoso-web";

        Mock::given(method("GET"))
            .and(path(format!(
                "/api/v4/projects/{project_id}/repository/tree"
            )))
            .respond_with(
                ResponseTemplate::new(302)
                    .insert_header("Location", "https://auth.example.com/login?req=abc"),
            )
            .mount(&server)
            .await;

        let dest = tempfile::TempDir::new().unwrap();
        let err = pull_gitlab_api(&config, dest.path())
            .await
            .expect_err("redirect should be rejected, not followed");
        assert!(err.contains("auth.example.com"), "error was: {err}");
        assert!(err.contains("redirected"), "error was: {err}");
    }

    #[tokio::test]
    async fn push_with_no_changes_is_a_noop() {
        let server = MockServer::start().await;
        let repo_url = format!("{}/acme/contoso-web", server.uri());
        let config = cfg(repo_url, "test-token");
        let project_id = "acme%2Fcontoso-web";

        Mock::given(method("GET"))
            .and(path(format!(
                "/api/v4/projects/{project_id}/repository/tree"
            )))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"id": "1", "type": "blob", "path": ".ax/policy/shared/manifest.json"}
            ])))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path(format!(
                "/api/v4/projects/{project_id}/repository/files/.ax%2Fpolicy%2Fshared%2Fmanifest.json/raw"
            )))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(r#"{"version":1,"rules":[],"skills":[]}"#),
            )
            .mount(&server)
            .await;

        let pack_src = tempfile::TempDir::new().unwrap();
        std::fs::write(
            pack_src.path().join("manifest.json"),
            r#"{"version":1,"rules":[],"skills":[]}"#,
        )
        .unwrap();

        let written = push_gitlab_api(&config, pack_src.path(), None)
            .await
            .expect("push should succeed");
        assert_eq!(written, 0);
    }

    #[tokio::test]
    async fn pull_refuses_tree_paths_that_escape_the_pull_dir() {
        let server = MockServer::start().await;
        let repo_url = format!("{}/acme/contoso-web", server.uri());
        let config = cfg(repo_url, "test-token");
        let project_id = "acme%2Fcontoso-web";

        Mock::given(method("GET"))
            .and(path(format!(
                "/api/v4/projects/{project_id}/repository/tree"
            )))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"id": "1", "type": "blob", "path": ".ax/policy/shared/../../../escaped.txt"}
            ])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(wiremock::matchers::path_regex(r"/repository/files/.*/raw$"))
            .respond_with(ResponseTemplate::new(200).set_body_string("owned"))
            .mount(&server)
            .await;

        let dest = tempfile::TempDir::new().unwrap();
        let err = pull_gitlab_api(&config, dest.path())
            .await
            .expect_err("an escaping tree path must be refused");
        assert!(err.contains("escaped.txt"), "error was: {err}");
        assert!(!dest.path().join("escaped.txt").exists());
    }

    #[tokio::test]
    async fn pull_refuses_parent_dirs_below_the_pack_prefix() {
        let server = MockServer::start().await;
        let repo_url = format!("{}/acme/contoso-web", server.uri());
        let mut config = cfg(repo_url, "test-token");
        config.subpath = String::new();
        let project_id = "acme%2Fcontoso-web";

        Mock::given(method("GET"))
            .and(path(format!(
                "/api/v4/projects/{project_id}/repository/tree"
            )))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"id": "1", "type": "blob", "path": "policy/shared/x/../../../../abs.txt"}
            ])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(wiremock::matchers::path_regex(r"/repository/files/.*/raw$"))
            .respond_with(ResponseTemplate::new(200).set_body_string("owned"))
            .mount(&server)
            .await;

        let dest = tempfile::TempDir::new().unwrap();
        pull_gitlab_api(&config, dest.path())
            .await
            .expect_err("an escaping tree path must be refused");
        assert!(!dest.path().join("abs.txt").exists());
    }

    /// Mounts a remote holding one pack file and the shared memory file, and
    /// records the commit body for inspection.
    async fn mount_remote_with_pack_and_memory(server: &MockServer) {
        let project_id = "acme%2Fcontoso-web";
        Mock::given(method("GET"))
            .and(path(format!(
                "/api/v4/projects/{project_id}/repository/tree"
            )))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"id": "1", "type": "blob", "path": ".ax/policy/shared/manifest.json"},
                {"id": "2", "type": "blob", "path": ".ax/memory/shared.jsonl"}
            ])))
            .mount(server)
            .await;
        Mock::given(method("GET"))
            .and(wiremock::matchers::path_regex(r"/repository/files/.*/raw$"))
            .respond_with(ResponseTemplate::new(200).set_body_string("remote"))
            .mount(server)
            .await;
        Mock::given(method("POST"))
            .and(path(format!(
                "/api/v4/projects/{project_id}/repository/commits"
            )))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({"id": "c"})))
            .mount(server)
            .await;
    }

    async fn committed_deletes(server: &MockServer) -> Vec<String> {
        let mut out = Vec::new();
        for req in server.received_requests().await.unwrap_or_default() {
            if req.method.as_str() != "POST" {
                continue;
            }
            let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap();
            for a in body["actions"].as_array().unwrap() {
                if a["action"] == "delete" {
                    out.push(a["file_path"].as_str().unwrap().to_string());
                }
            }
        }
        out
    }

    #[tokio::test]
    async fn push_without_memory_keeps_remote_shared_memory() {
        let server = MockServer::start().await;
        mount_remote_with_pack_and_memory(&server).await;
        let config = cfg(format!("{}/acme/contoso-web", server.uri()), "test-token");

        let pack_src = tempfile::TempDir::new().unwrap();
        std::fs::write(pack_src.path().join("manifest.json"), "local").unwrap();

        push_gitlab_api(&config, pack_src.path(), None)
            .await
            .expect("push should succeed");
        let deletes = committed_deletes(&server).await;
        assert!(
            !deletes.iter().any(|d| d.ends_with("memory/shared.jsonl")),
            "memory was not pushed, so it must not be deleted: {deletes:?}"
        );
    }

    #[tokio::test]
    async fn push_without_a_pack_keeps_the_remote_pack() {
        let server = MockServer::start().await;
        mount_remote_with_pack_and_memory(&server).await;
        let config = cfg(format!("{}/acme/contoso-web", server.uri()), "test-token");

        let root = tempfile::TempDir::new().unwrap();
        let missing_pack = root.path().join("policy").join("shared");
        let mem = root.path().join("shared.jsonl");
        std::fs::write(&mem, "local").unwrap();

        push_gitlab_api(&config, &missing_pack, Some(&mem))
            .await
            .expect("push should succeed");
        let deletes = committed_deletes(&server).await;
        assert!(
            !deletes.iter().any(|d| d.contains("policy/shared/")),
            "no pack was exported, so the remote pack must not be deleted: {deletes:?}"
        );
    }
}
