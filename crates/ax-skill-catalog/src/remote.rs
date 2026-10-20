use crate::source::{GitHost, SkillSource};
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct Fetched {
    pub tree: PathBuf,
    pub sha: String,
    pub source_id: String,
    pub source_type: String,
    pub well_known_index: Option<String>,
    pub file_base: String,
    _temp: Option<tempfile::TempDir>,
}

pub fn fetch_source(source: &SkillSource) -> Result<Fetched, String> {
    match source {
        SkillSource::Local { path } => Ok(Fetched {
            tree: PathBuf::from(path),
            sha: "local".into(),
            source_id: path.clone(),
            source_type: "local".into(),
            well_known_index: None,
            file_base: String::new(),
            _temp: None,
        }),
        SkillSource::Git {
            clone_url,
            source_id,
            reference,
            host,
            ..
        } => {
            let temp = tempfile::tempdir().map_err(|e| e.to_string())?;
            git_clone(clone_url, reference.as_deref(), temp.path())?;
            let sha = git_rev(temp.path()).unwrap_or_else(|_| "unknown".into());
            let source_type = match host {
                GitHost::GitHub => "github",
                GitHost::GitLab => "gitlab",
                GitHost::Azure => "azure",
            };
            Ok(Fetched {
                tree: temp.path().to_path_buf(),
                sha,
                source_id: source_id.clone(),
                source_type: source_type.into(),
                well_known_index: None,
                file_base: String::new(),
                _temp: Some(temp),
            })
        }
        SkillSource::WellKnown { base_url, .. } => {
            let (index_url, body) = fetch_index(base_url)?;
            let file_base = index_url.trim_end_matches("/index.json").to_string();
            Ok(Fetched {
                tree: PathBuf::new(),
                sha: "well-known".into(),
                source_id: base_url.clone(),
                source_type: "well-known".into(),
                well_known_index: Some(body),
                file_base,
                _temp: None,
            })
        }
    }
}

pub fn fetch_index(base: &str) -> Result<(String, String), String> {
    let base = base.trim_end_matches('/');
    let urls = if base.contains("/.well-known/") {
        vec![format!("{base}/index.json")]
    } else {
        vec![
            format!("{base}/.well-known/agent-skills/index.json"),
            format!("{base}/.well-known/skills/index.json"),
        ]
    };
    let mut errors = Vec::new();
    for url in urls {
        match http_text(&url) {
            Ok(body) if body.contains("skills") => return Ok((url, body)),
            Ok(_) => errors.push(format!("{url} was not a skill index")),
            Err(err) => errors.push(err),
        }
    }
    Err(errors.join("; "))
}

pub fn attach_catalog_cards(hits: &mut [crate::SearchHit]) {
    let client = match reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .redirect(reqwest::redirect::Policy::limited(2))
        .build()
    {
        Ok(client) => client,
        Err(_) => return,
    };
    let jobs: Vec<(usize, String)> = hits
        .iter()
        .enumerate()
        .filter_map(|(index, hit)| crate::search::catalog_card_url(&hit.id).map(|url| (index, url)))
        .take(12)
        .collect();
    let pages: Vec<(usize, String)> = std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for (index, url) in jobs {
            let client = &client;
            handles.push(scope.spawn(move || {
                let page = client.get(&url).send().ok().and_then(|response| {
                    if !response.status().is_success() {
                        return None;
                    }
                    response.text().ok()
                });
                (index, page)
            }));
        }
        handles
            .into_iter()
            .filter_map(|handle| handle.join().ok())
            .filter_map(|(index, page)| page.map(|html| (index, html)))
            .collect()
    });
    for (index, html) in pages {
        if let Some(hit) = hits.get_mut(index) {
            crate::search::apply_skill_card(hit, &html);
        }
    }
}

pub fn search_body(query: &str, owner: Option<&str>, limit: u32) -> Result<String, String> {
    let limit = limit.clamp(1, 50);
    let mut url = format!("{}?q={}&limit={limit}", crate::SEARCH_URL, encode(query));
    if let Some(owner) = owner.filter(|owner| !owner.is_empty()) {
        url.push_str("&owner=");
        url.push_str(&encode(owner));
    }
    http_text(&url)
}

pub fn git_clone(url: &str, reference: Option<&str>, dest: &Path) -> Result<(), String> {
    let mut cmd = Command::new("git");
    cmd.arg("clone").arg("--depth").arg("1");
    if let Some(reference) = reference {
        cmd.arg("--branch").arg(reference);
    }
    cmd.arg(url).arg(dest);
    let output = cmd.output().map_err(|e| format!("git clone: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "git clone failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

pub fn git_rev(dest: &Path) -> Result<String, String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dest)
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err("git rev-parse failed".into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub fn git_publish(repo: &Path, name: &str) -> Result<(), String> {
    let add = Command::new("git")
        .args(["add", "--", &format!("skills/{name}")])
        .current_dir(repo)
        .output()
        .map_err(|e| e.to_string())?;
    if !add.status.success() {
        return Err(String::from_utf8_lossy(&add.stderr).trim().to_string());
    }
    let commit = Command::new("git")
        .args(["commit", "-m", &format!("Publish skill {name}")])
        .current_dir(repo)
        .output()
        .map_err(|e| e.to_string())?;
    if !commit.status.success() {
        let stderr = String::from_utf8_lossy(&commit.stderr);
        let stdout = String::from_utf8_lossy(&commit.stdout);
        if !stderr.contains("nothing to commit") && !stdout.contains("nothing to commit") {
            return Err(stderr.trim().to_string());
        }
    }
    let push = Command::new("git")
        .args(["push"])
        .current_dir(repo)
        .output()
        .map_err(|e| e.to_string())?;
    if !push.status.success() {
        return Err(format!(
            "git push failed: {}",
            String::from_utf8_lossy(&push.stderr).trim()
        ));
    }
    Ok(())
}

pub fn http_text(url: &str) -> Result<String, String> {
    String::from_utf8(http_bytes(url)?).map_err(|e| e.to_string())
}

pub fn http_bytes(url: &str) -> Result<Vec<u8>, String> {
    let response = reqwest::blocking::get(url).map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("GET {url} returned {}", response.status()));
    }
    let bytes = response.bytes().map_err(|e| e.to_string())?;
    if bytes.len() > 25 * 1024 * 1024 {
        return Err("download is larger than 25 MiB".into());
    }
    Ok(bytes.to_vec())
}

pub fn encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}
