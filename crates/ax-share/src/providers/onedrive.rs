//! Pull shared policy pack from OneDrive / SharePoint via Microsoft Graph.

use std::path::Path;

use reqwest::Client;
use serde::Deserialize;

use crate::auth::microsoft::{encode_share_id, get_access_token};
use crate::config::OneDriveShareConfig;
use crate::providers::PullResult;

const GRAPH_V1: &str = "https://graph.microsoft.com/v1.0";

pub async fn pull_onedrive(config: &OneDriveShareConfig, dest_root: &Path) -> Result<PullResult, String> {
    let share_url = config.share_url.trim();
    if share_url.is_empty() {
        return Err("OneDrive share URL is not configured".into());
    }
    let token = get_access_token().await?;
    pull_onedrive_with_token(config, dest_root, GRAPH_V1, &token).await
}

/// Pull from OneDrive/SharePoint using an explicit Graph base URL (for tests).
pub(crate) async fn pull_onedrive_with_token(
    config: &OneDriveShareConfig,
    dest_root: &Path,
    graph_base: &str,
    token: &str,
) -> Result<PullResult, String> {
    let share_url = config.share_url.trim();
    if share_url.is_empty() {
        return Err("OneDrive share URL is not configured".into());
    }
    let client = Client::new();
    let share_id = encode_share_id(share_url);
    let graph_base = graph_base.trim_end_matches('/');

    let item: DriveItem = client
        .get(format!(
            "{graph_base}/shares/{share_id}/driveItem"
        ))
        .bearer_auth(&token)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    let drive_id = item
        .parent_reference
        .as_ref()
        .and_then(|p| p.drive_id.clone())
        .unwrap_or_else(|| "me".to_string());

    let pull_dir = dest_root.join("pull");
    if pull_dir.exists() {
        std::fs::remove_dir_all(&pull_dir).map_err(|e| e.to_string())?;
    }
    std::fs::create_dir_all(&pull_dir).map_err(|e| e.to_string())?;

    let mut files = 0usize;
    let pack_dest = pull_dir.join("policy").join("shared");
    let memory_dest = pull_dir.join("memory").join("shared.jsonl");

    if item.folder.is_some() {
        let mut queue = vec![(item.id.clone(), pull_dir.to_path_buf())];
        while let Some((item_id, local_dir)) = queue.pop() {
            let children = list_children(&client, token, graph_base, &drive_id, &item_id).await?;
            for child in children {
                let name = child.name.unwrap_or_else(|| "unnamed".to_string());
                if child.folder.is_some() {
                    let sub = local_dir.join(sanitize_name(&name));
                    std::fs::create_dir_all(&sub).map_err(|e| e.to_string())?;
                    queue.push((child.id, sub));
                } else if let Some(download_url) = child.download_url {
                    let dest = local_dir.join(sanitize_name(&name));
                    let bytes = client
                        .get(&download_url)
                        .send()
                        .await
                        .map_err(|e| e.to_string())?
                        .bytes()
                        .await
                        .map_err(|e| e.to_string())?;
                    std::fs::write(&dest, &bytes).map_err(|e| e.to_string())?;
                    files += 1;
                }
            }
        }
    } else {
        return Err("Share URL does not point to a folder".into());
    }

    let pack_dir = if pack_dest.join("manifest.json").exists() {
        Some(pack_dest)
    } else {
        None
    };
    let memory_file = if memory_dest.is_file() {
        Some(memory_dest)
    } else {
        None
    };

    if pack_dir.is_none() && memory_file.is_none() {
        return Err(
            "Remote folder has no policy/shared/manifest.json or memory/shared.jsonl".into(),
        );
    }

    Ok(PullResult {
        pack_dir,
        memory_file,
        files_copied: files,
    })
}

async fn list_children(
    client: &Client,
    token: &str,
    graph_base: &str,
    drive_id: &str,
    item_id: &str,
) -> Result<Vec<DriveItemChild>, String> {
    let url = if drive_id == "me" {
        format!("{graph_base}/me/drive/items/{item_id}/children?$top=200")
    } else {
        format!("{graph_base}/drives/{drive_id}/items/{item_id}/children?$top=200")
    };
    let list: ChildrenList = client
        .get(&url)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    Ok(list.value)
}

fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[derive(Debug, Deserialize)]
struct DriveItem {
    id: String,
    #[serde(default)]
    name: Option<String>,
    folder: Option<FolderFacet>,
    #[serde(rename = "parentReference")]
    parent_reference: Option<ParentReference>,
}

#[derive(Debug, Deserialize)]
struct ParentReference {
    #[serde(rename = "driveId")]
    drive_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FolderFacet {
    #[serde(rename = "childCount")]
    child_count: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct ChildrenList {
    #[serde(default)]
    value: Vec<DriveItemChild>,
}

#[derive(Debug, Deserialize)]
struct DriveItemChild {
    id: String,
    name: Option<String>,
    folder: Option<FolderFacet>,
    #[serde(rename = "@microsoft.graph.downloadUrl")]
    download_url: Option<String>,
}

/// Push local pack directory to OneDrive share root (overwrite files).
pub async fn push_onedrive(
    config: &OneDriveShareConfig,
    pack_dir: &Path,
    memory_file: Option<&Path>,
) -> Result<usize, String> {
    let token = get_access_token().await?;
    let client = Client::new();
    let share_id = encode_share_id(config.share_url.trim());
    let root: DriveItem = client
        .get(format!(
            "https://graph.microsoft.com/v1.0/shares/{share_id}/driveItem"
        ))
        .bearer_auth(&token)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    let mut uploaded = 0usize;
    if pack_dir.is_dir() {
        let policy_folder =
            ensure_child_folder(&client, &token, &root.id, "policy").await?;
        let shared_folder =
            ensure_child_folder(&client, &token, &policy_folder, "shared").await?;
        uploaded += upload_dir(&client, &token, &shared_folder, pack_dir).await?;
    }
    if let Some(mem) = memory_file {
        if mem.is_file() {
            let memory_folder =
                ensure_child_folder(&client, &token, &root.id, "memory").await?;
            let content = std::fs::read(mem).map_err(|e| e.to_string())?;
            upload_file(&client, &token, &memory_folder, "shared.jsonl", &content).await?;
            uploaded += 1;
        }
    }
    Ok(uploaded)
}

async fn ensure_child_folder(
    client: &Client,
    token: &str,
    parent_id: &str,
    name: &str,
) -> Result<String, String> {
    let url = format!(
        "https://graph.microsoft.com/v1.0/me/drive/items/{parent_id}/children"
    );
    let list: ChildrenList = client
        .get(&url)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    for child in &list.value {
        if child.name.as_deref() == Some(name) && child.folder.is_some() {
            return Ok(child.id.clone());
        }
    }
    #[derive(serde::Serialize)]
    struct NewFolder<'a> {
        name: &'a str,
        folder: EmptyObj,
        #[serde(rename = "@microsoft.graph.conflictBehavior")]
        conflict_behavior: &'static str,
    }
    #[derive(serde::Serialize)]
    struct EmptyObj {}
    let body = NewFolder {
        name,
        folder: EmptyObj {},
        conflict_behavior: "replace",
    };
    let created: DriveItem = client
        .post(&url)
        .bearer_auth(token)
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    Ok(created.id)
}

async fn upload_dir(
    client: &Client,
    token: &str,
    parent_id: &str,
    local_dir: &Path,
) -> Result<usize, String> {
    let mut count = 0usize;
    let mut queue = vec![(parent_id.to_string(), local_dir.to_path_buf())];
    while let Some((parent, dir)) = queue.pop() {
        for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if path.is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                let sub = ensure_child_folder(client, token, &parent, &name).await?;
                queue.push((sub, path));
            } else if path.is_file() {
                let name = entry.file_name().to_string_lossy().to_string();
                let content = std::fs::read(&path).map_err(|e| e.to_string())?;
                upload_file(client, token, &parent, &name, &content).await?;
                count += 1;
            }
        }
    }
    Ok(count)
}

async fn upload_file(
    client: &Client,
    token: &str,
    parent_id: &str,
    name: &str,
    content: &[u8],
) -> Result<(), String> {
    let url = format!(
        "https://graph.microsoft.com/v1.0/me/drive/items/{parent_id}:/{name}:/content"
    );
    client
        .put(&url)
        .bearer_auth(token)
        .body(content.to_vec())
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;
    use wiremock::matchers::{method, path_regex};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn pull_onedrive_downloads_pack_from_mock_graph() {
        let server = MockServer::start().await;
        let share_url = "https://contoso.sharepoint.com/:f:/r/personal/user/Documents/.ax";
        let share_id = encode_share_id(share_url);
        let manifest_body = r#"{"version":1,"rules":[],"skills":[]}"#;

        Mock::given(method("GET"))
            .and(path_regex(format!(r"^/v1\.0/shares/{share_id}/driveItem$")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "root-folder",
                "name": ".ax",
                "folder": { "childCount": 1 },
                "parentReference": { "driveId": "drive-1" }
            })))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path_regex(r"^/v1\.0/drives/drive-1/items/root-folder/children$"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "value": [{
                    "id": "policy-folder",
                    "name": "policy",
                    "folder": { "childCount": 1 }
                }]
            })))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path_regex(r"^/v1\.0/drives/drive-1/items/policy-folder/children$"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "value": [{
                    "id": "shared-folder",
                    "name": "shared",
                    "folder": { "childCount": 1 }
                }]
            })))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path_regex(r"^/v1\.0/drives/drive-1/items/shared-folder/children$"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "value": [{
                    "id": "manifest-file",
                    "name": "manifest.json",
                    "@microsoft.graph.downloadUrl": format!("{}/download/manifest.json", server.uri())
                }]
            })))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path_regex(r"^/download/manifest\.json$"))
            .respond_with(ResponseTemplate::new(200).set_body_string(manifest_body))
            .mount(&server)
            .await;

        let dest = TempDir::new().unwrap();
        let config = OneDriveShareConfig {
            share_url: share_url.to_string(),
        };
        let graph_base = format!("{}/v1.0", server.uri());
        let result = pull_onedrive_with_token(&config, dest.path(), &graph_base, "test-token")
            .await
            .expect("pull should succeed");

        assert!(result.files_copied >= 1);
        let manifest = dest
            .path()
            .join("pull")
            .join("policy")
            .join("shared")
            .join("manifest.json");
        assert!(manifest.is_file(), "expected manifest at {}", manifest.display());
        let pack_dir = result.pack_dir.expect("pack_dir should be set");
        assert_eq!(pack_dir, PathBuf::from(dest.path()).join("pull").join("policy").join("shared"));
    }

    #[tokio::test]
    async fn pull_onedrive_errors_when_no_pack_or_memory() {
        let server = MockServer::start().await;
        let share_url = "https://contoso.sharepoint.com/empty";
        let share_id = encode_share_id(share_url);

        Mock::given(method("GET"))
            .and(path_regex(format!(r"^/v1\.0/shares/{share_id}/driveItem$")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "empty-root",
                "folder": { "childCount": 0 },
                "parentReference": { "driveId": "drive-1" }
            })))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path_regex(r"^/v1\.0/drives/drive-1/items/empty-root/children$"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "value": [] })))
            .mount(&server)
            .await;

        let dest = TempDir::new().unwrap();
        let config = OneDriveShareConfig {
            share_url: share_url.to_string(),
        };
        let graph_base = format!("{}/v1.0", server.uri());
        let err = pull_onedrive_with_token(&config, dest.path(), &graph_base, "test-token")
            .await
            .expect_err("empty folder should fail");
        assert!(err.contains("manifest.json") || err.contains("shared.jsonl"));
    }
}
