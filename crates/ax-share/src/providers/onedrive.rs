//! Pull shared policy pack from OneDrive / SharePoint via Microsoft Graph.

use std::path::Path;

use reqwest::Client;
use serde::Deserialize;

use crate::auth::microsoft::{encode_share_id, get_access_token};
use crate::config::OneDriveShareConfig;
use crate::providers::PullResult;

const GRAPH_V1: &str = "https://graph.microsoft.com/v1.0";

pub async fn pull_onedrive(
    config: &OneDriveShareConfig,
    dest_root: &Path,
) -> Result<PullResult, String> {
    let share_url = config.share_url.trim();
    if share_url.is_empty() {
        return Err("onedrive.shareUrl is not set in ax.json".into());
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
        return Err("onedrive.shareUrl is not set in ax.json".into());
    }
    let client = Client::new();
    let share_id = encode_share_id(share_url);
    let graph_base = graph_base.trim_end_matches('/');

    let item: DriveItem = client
        .get(format!("{graph_base}/shares/{share_id}/driveItem"))
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    let items = items_base(graph_base, &item);

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
            let children = list_children(&client, token, graph_base, &items, &item_id).await?;
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

/// The `.../items` URL of the drive that holds `root`: `/drives/{driveId}/items`
/// when Graph names the drive (always the case for a shared folder, which may
/// live in another user's drive), `/me/drive/items` otherwise.
fn items_base(graph_base: &str, root: &DriveItem) -> String {
    match root
        .parent_reference
        .as_ref()
        .and_then(|p| p.drive_id.as_deref())
        .filter(|id| !id.trim().is_empty())
    {
        Some(drive_id) => format!("{graph_base}/drives/{}/items", percent_encode(drive_id)),
        None => format!("{graph_base}/me/drive/items"),
    }
}

/// Encodes a Graph id or file name for use as one URL path segment.
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'!' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Lists every child of `item_id`, following `@odata.nextLink` pages. A next
/// link is only followed on the Graph origin, so the token never goes to
/// another host.
async fn list_children(
    client: &Client,
    token: &str,
    graph_base: &str,
    items: &str,
    item_id: &str,
) -> Result<Vec<DriveItemChild>, String> {
    let origin = reqwest::Url::parse(graph_base)
        .map(|u| u.origin())
        .map_err(|e| format!("invalid Graph URL: {e}"))?;
    let mut url = format!("{items}/{}/children?$top=200", percent_encode(item_id));
    let mut out = Vec::new();
    loop {
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
        out.extend(list.value);
        let Some(next) = list.next_link else {
            return Ok(out);
        };
        let same_origin = reqwest::Url::parse(&next)
            .map(|u| u.origin() == origin)
            .unwrap_or(false);
        if !same_origin {
            return Err(format!(
                "Graph returned a next page link outside {graph_base}: {next}"
            ));
        }
        url = next;
    }
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
struct FolderFacet {}

#[derive(Debug, Deserialize)]
struct ChildrenList {
    #[serde(default)]
    value: Vec<DriveItemChild>,
    #[serde(rename = "@odata.nextLink")]
    next_link: Option<String>,
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
    push_onedrive_with_token(config, pack_dir, memory_file, GRAPH_V1, &token).await
}

/// Push to OneDrive/SharePoint using an explicit Graph base URL (for tests).
pub(crate) async fn push_onedrive_with_token(
    config: &OneDriveShareConfig,
    pack_dir: &Path,
    memory_file: Option<&Path>,
    graph_base: &str,
    token: &str,
) -> Result<usize, String> {
    let token = token.to_string();
    let graph_base = graph_base.trim_end_matches('/');
    let client = Client::new();
    let share_id = encode_share_id(config.share_url.trim());
    let root: DriveItem = client
        .get(format!("{graph_base}/shares/{share_id}/driveItem"))
        .bearer_auth(&token)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    let items = items_base(graph_base, &root);
    let api = DriveApi {
        client: &client,
        token: &token,
        graph_base,
        items: &items,
    };

    let mut uploaded = 0usize;
    if pack_dir.is_dir() {
        let policy_folder = ensure_child_folder(&api, &root.id, "policy").await?;
        let shared_folder = ensure_child_folder(&api, &policy_folder, "shared").await?;
        uploaded += upload_dir(&api, &shared_folder, pack_dir).await?;
    }
    if let Some(mem) = memory_file {
        if mem.is_file() {
            let memory_folder = ensure_child_folder(&api, &root.id, "memory").await?;
            let content = std::fs::read(mem).map_err(|e| e.to_string())?;
            upload_file(&api, &memory_folder, "shared.jsonl", &content).await?;
            uploaded += 1;
        }
    }
    Ok(uploaded)
}

/// One push's view of the target drive.
struct DriveApi<'a> {
    client: &'a Client,
    token: &'a str,
    graph_base: &'a str,
    /// `.../drives/{driveId}/items` (or `.../me/drive/items`).
    items: &'a str,
}

async fn ensure_child_folder(
    api: &DriveApi<'_>,
    parent_id: &str,
    name: &str,
) -> Result<String, String> {
    let children =
        list_children(api.client, api.token, api.graph_base, api.items, parent_id).await?;
    for child in &children {
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
    let created: DriveItem = api
        .client
        .post(format!(
            "{}/{}/children",
            api.items,
            percent_encode(parent_id)
        ))
        .bearer_auth(api.token)
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    Ok(created.id)
}

async fn upload_dir(
    api: &DriveApi<'_>,
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
                let sub = ensure_child_folder(api, &parent, &name).await?;
                queue.push((sub, path));
            } else if path.is_file() {
                let name = entry.file_name().to_string_lossy().to_string();
                let content = std::fs::read(&path).map_err(|e| e.to_string())?;
                upload_file(api, &parent, &name, &content).await?;
                count += 1;
            }
        }
    }
    Ok(count)
}

async fn upload_file(
    api: &DriveApi<'_>,
    parent_id: &str,
    name: &str,
    content: &[u8],
) -> Result<(), String> {
    let url = format!(
        "{}/{}:/{}:/content",
        api.items,
        percent_encode(parent_id),
        percent_encode(name)
    );
    api.client
        .put(&url)
        .bearer_auth(api.token)
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
            .and(path_regex(
                r"^/v1\.0/drives/drive-1/items/root-folder/children$",
            ))
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
            .and(path_regex(
                r"^/v1\.0/drives/drive-1/items/policy-folder/children$",
            ))
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
        assert!(
            manifest.is_file(),
            "expected manifest at {}",
            manifest.display()
        );
        let pack_dir = result.pack_dir.expect("pack_dir should be set");
        assert_eq!(
            pack_dir,
            PathBuf::from(dest.path())
                .join("pull")
                .join("policy")
                .join("shared")
        );
    }

    #[tokio::test]
    async fn pull_onedrive_without_share_url_names_the_ax_json_key() {
        let dest = tempfile::TempDir::new().unwrap();
        let config = OneDriveShareConfig {
            share_url: "  ".into(),
        };
        let err = pull_onedrive(&config, dest.path()).await.unwrap_err();
        assert_eq!(err, "onedrive.shareUrl is not set in ax.json");
        let err = pull_onedrive_with_token(&config, dest.path(), "http://127.0.0.1:9", "t")
            .await
            .unwrap_err();
        assert_eq!(err, "onedrive.shareUrl is not set in ax.json");
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
            .and(path_regex(
                r"^/v1\.0/drives/drive-1/items/empty-root/children$",
            ))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "value": [] })),
            )
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

    /// A share that lives in another user's drive: every list, create and
    /// upload must go through `/drives/{driveId}/items`, never `/me/drive`.
    #[tokio::test]
    async fn push_onedrive_addresses_the_shared_drive_not_me_drive() {
        let server = MockServer::start().await;
        let share_url = "https://contoso.sharepoint.com/:f:/r/personal/a/Documents/.ax";
        let share_id = encode_share_id(share_url);

        Mock::given(method("GET"))
            .and(path_regex(format!(r"^/v1\.0/shares/{share_id}/driveItem$")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "root-folder",
                "folder": { "childCount": 0 },
                "parentReference": { "driveId": "drive-a" }
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path_regex(r"^/v1\.0/drives/drive-a/items/[^/]+/children$"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "value": [] })),
            )
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path_regex(r"^/v1\.0/drives/drive-a/items/[^/]+/children$"))
            .respond_with(
                ResponseTemplate::new(201)
                    .set_body_json(serde_json::json!({ "id": "new-folder", "folder": {} })),
            )
            .mount(&server)
            .await;
        Mock::given(method("PUT"))
            .and(path_regex(
                r"^/v1\.0/drives/drive-a/items/new-folder:/.+:/content$",
            ))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({"id":"f"})))
            .mount(&server)
            .await;
        Mock::given(wiremock::matchers::path_regex(r"^/v1\.0/me/drive/.*"))
            .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "error": { "code": "itemNotFound" }
            })))
            .mount(&server)
            .await;

        let pack = TempDir::new().unwrap();
        std::fs::write(pack.path().join("manifest.json"), "{}").unwrap();
        let config = OneDriveShareConfig {
            share_url: share_url.to_string(),
        };
        let graph_base = format!("{}/v1.0", server.uri());
        let uploaded =
            push_onedrive_with_token(&config, pack.path(), None, &graph_base, "test-token")
                .await
                .expect("push to a shared drive should succeed");
        assert_eq!(uploaded, 1);
    }

    #[tokio::test]
    async fn push_onedrive_reports_the_http_status_of_a_failed_listing() {
        let server = MockServer::start().await;
        let share_url = "https://contoso.sharepoint.com/denied";
        let share_id = encode_share_id(share_url);

        Mock::given(method("GET"))
            .and(path_regex(format!(r"^/v1\.0/shares/{share_id}/driveItem$")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "root-folder",
                "folder": { "childCount": 0 },
                "parentReference": { "driveId": "drive-a" }
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path_regex(r"/children$"))
            .respond_with(ResponseTemplate::new(403).set_body_string("<html>denied</html>"))
            .mount(&server)
            .await;

        let pack = TempDir::new().unwrap();
        std::fs::write(pack.path().join("manifest.json"), "{}").unwrap();
        let config = OneDriveShareConfig {
            share_url: share_url.to_string(),
        };
        let graph_base = format!("{}/v1.0", server.uri());
        let err = push_onedrive_with_token(&config, pack.path(), None, &graph_base, "t")
            .await
            .expect_err("a 403 listing must fail");
        assert!(err.contains("403"), "error was: {err}");
    }

    #[tokio::test]
    async fn pull_onedrive_follows_next_link_pages() {
        let server = MockServer::start().await;
        let share_url = "https://contoso.sharepoint.com/paged";
        let share_id = encode_share_id(share_url);

        Mock::given(method("GET"))
            .and(path_regex(format!(r"^/v1\.0/shares/{share_id}/driveItem$")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "root",
                "folder": { "childCount": 2 },
                "parentReference": { "driveId": "drive-1" }
            })))
            .mount(&server)
            .await;
        // First page holds an unrelated file and points to a second page.
        Mock::given(method("GET"))
            .and(path_regex(r"^/v1\.0/drives/drive-1/items/root/children$"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "value": [{ "id": "other", "name": "README.md" }],
                "@odata.nextLink": format!("{}/v1.0/page2", server.uri())
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path_regex(r"^/v1\.0/page2$"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "value": [{ "id": "mem-folder", "name": "memory", "folder": {} }]
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path_regex(
                r"^/v1\.0/drives/drive-1/items/mem-folder/children$",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "value": [{
                    "id": "mem",
                    "name": "shared.jsonl",
                    "@microsoft.graph.downloadUrl": format!("{}/download/mem", server.uri())
                }]
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path_regex(r"^/download/mem$"))
            .respond_with(ResponseTemplate::new(200).set_body_string("{}\n"))
            .mount(&server)
            .await;

        let dest = TempDir::new().unwrap();
        let config = OneDriveShareConfig {
            share_url: share_url.to_string(),
        };
        let graph_base = format!("{}/v1.0", server.uri());
        let result = pull_onedrive_with_token(&config, dest.path(), &graph_base, "test-token")
            .await
            .expect("entries on the second page must be pulled");
        assert!(result.memory_file.is_some());
    }
}
