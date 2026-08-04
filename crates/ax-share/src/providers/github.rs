//! Pull shared policy pack from a GitHub git repository.

use std::path::Path;
use std::process::Command;

use crate::config::GithubShareConfig;
use crate::providers::PullResult;
use crate::status::copy_dir_recursive;

pub fn pull_github(config: &GithubShareConfig, dest_root: &Path) -> Result<PullResult, String> {
    let url = config.repo_url.trim();
    if url.is_empty() {
        return Err("GitHub repo URL is not configured".into());
    }

    std::fs::create_dir_all(dest_root).map_err(|e| e.to_string())?;
    let cache_key = blake3::hash(url.as_bytes()).to_hex()[..16].to_string();
    let clone_dir = dest_root.join("github").join(&cache_key);
    if clone_dir.exists() {
        std::fs::remove_dir_all(&clone_dir).map_err(|e| e.to_string())?;
    }
    std::fs::create_dir_all(clone_dir.parent().unwrap()).map_err(|e| e.to_string())?;

    let mut args = vec![
        "clone".to_string(),
        "--depth".to_string(),
        "1".to_string(),
        "--quiet".to_string(),
    ];
    if config.branch != "main" && !config.branch.is_empty() {
        args.push("--branch".to_string());
        args.push(config.branch.clone());
    }
    args.push(url.to_string());
    args.push(clone_dir.to_string_lossy().to_string());

    let status = Command::new("git")
        .args(&args)
        .status()
        .map_err(|e| format!("failed to run git: {e}"))?;
    if !status.success() {
        return Err(format!("git clone failed for {url}"));
    }

    let sub = config.subpath.trim().trim_matches('/');
    let base = if sub.is_empty() {
        clone_dir.clone()
    } else {
        clone_dir.join(sub)
    };

    let pack_src = base.join("policy").join("shared");
    let memory_src = base.join("memory").join("shared.jsonl");
    let pull_dir = dest_root.join("pull");
    if pull_dir.exists() {
        std::fs::remove_dir_all(&pull_dir).map_err(|e| e.to_string())?;
    }
    std::fs::create_dir_all(&pull_dir).map_err(|e| e.to_string())?;

    let mut files = 0usize;
    let mut pack_dir = None;
    let mut memory_file = None;

    if pack_src.is_dir() {
        let pack_dest = pull_dir.join("policy").join("shared");
        copy_dir_recursive(&pack_src, &pack_dest)?;
        files += count_files(&pack_dest);
        pack_dir = Some(pack_dest);
    }

    if memory_src.is_file() {
        let mem_dest = pull_dir.join("memory").join("shared.jsonl");
        if let Some(parent) = mem_dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::copy(&memory_src, &mem_dest).map_err(|e| e.to_string())?;
        files += 1;
        memory_file = Some(mem_dest);
    }

    if pack_dir.is_none() && memory_file.is_none() {
        return Err(format!(
            "No policy/shared or memory/shared.jsonl found under {} in repo",
            base.display()
        ));
    }

    Ok(PullResult {
        pack_dir,
        memory_file,
        files_copied: files,
    })
}

fn count_files(dir: &Path) -> usize {
    walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .count()
}

/// Push local pack dir (+ optional memory file) to a shared git repository.
///
/// Works against any git host (GitHub, GitLab, Azure DevOps, on-prem) — it
/// shells out to the ambient `git` binary, so it reuses whatever credentials
/// (SSH key, Git Credential Manager, cached PAT) already authenticate normal
/// `git clone`/`git push` on this machine for that host.
pub fn push_github(
    config: &GithubShareConfig,
    pack_dir: &Path,
    memory_file: Option<&Path>,
) -> Result<usize, String> {
    let url = config.repo_url.trim();
    if url.is_empty() {
        return Err("GitHub repo URL is not configured".into());
    }
    let branch = if config.branch.trim().is_empty() {
        "main"
    } else {
        config.branch.trim()
    };

    let cache_root = crate::paths::share_cache_dir().join("push-git");
    std::fs::create_dir_all(&cache_root).map_err(|e| e.to_string())?;
    let cache_key = blake3::hash(url.as_bytes()).to_hex()[..16].to_string();
    let clone_dir = cache_root.join(&cache_key);
    if clone_dir.exists() {
        std::fs::remove_dir_all(&clone_dir).map_err(|e| e.to_string())?;
    }

    // Full (non-shallow) clone: pushing needs real history for the rebase
    // retry below, unlike the shallow read-only pull above.
    let status = Command::new("git")
        .args(["clone", "--quiet", url, &clone_dir.to_string_lossy()])
        .status()
        .map_err(|e| format!("failed to run git: {e}"))?;
    if !status.success() {
        return Err(format!("git clone failed for {url}"));
    }

    if run_git(&clone_dir, &["checkout", branch]).is_err() {
        run_git(&clone_dir, &["checkout", "-b", branch])?;
    }

    let sub = config.subpath.trim().trim_matches('/');
    let base = if sub.is_empty() {
        clone_dir.clone()
    } else {
        clone_dir.join(sub)
    };

    let mut written = 0usize;
    if pack_dir.is_dir() {
        let pack_dest = base.join("policy").join("shared");
        if pack_dest.exists() {
            std::fs::remove_dir_all(&pack_dest).map_err(|e| e.to_string())?;
        }
        copy_dir_recursive(pack_dir, &pack_dest)?;
        written += count_files(&pack_dest);
    }
    if let Some(mem) = memory_file {
        if mem.is_file() {
            let mem_dest = base.join("memory").join("shared.jsonl");
            if let Some(parent) = mem_dest.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::copy(mem, &mem_dest).map_err(|e| e.to_string())?;
            written += 1;
        }
    }
    if written == 0 {
        return Ok(0);
    }

    let add_target = if sub.is_empty() { "." } else { sub };
    run_git(&clone_dir, &["add", "-A", "--", add_target])?;

    let porcelain = git_output(&clone_dir, &["status", "--porcelain", "--", add_target])?;
    if porcelain.trim().is_empty() {
        return Ok(0);
    }

    run_git(
        &clone_dir,
        &[
            "-c",
            "user.name=ax-bot",
            "-c",
            "user.email=ax@localhost",
            "commit",
            "-m",
            "ax: policy share sync",
        ],
    )?;

    if push_head(&clone_dir, branch).is_err() {
        run_git(&clone_dir, &["pull", "--rebase", "origin", branch])?;
        push_head(&clone_dir, branch)?;
    }

    Ok(written)
}

fn run_git(dir: &Path, args: &[&str]) -> Result<(), String> {
    let status = Command::new("git")
        .current_dir(dir)
        .args(args)
        .status()
        .map_err(|e| format!("failed to run git {args:?}: {e}"))?;
    if !status.success() {
        return Err(format!("git {args:?} failed"));
    }
    Ok(())
}

fn git_output(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .map_err(|e| format!("failed to run git {args:?}: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn push_head(dir: &Path, branch: &str) -> Result<(), String> {
    run_git(dir, &["push", "origin", &format!("HEAD:{branch}")])
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn init_bare_remote() -> TempDir {
        let dir = TempDir::new().unwrap();
        let status = Command::new("git")
            .args(["init", "--bare", "--initial-branch=main", "."])
            .current_dir(dir.path())
            .status()
            .unwrap();
        assert!(status.success());
        dir
    }

    fn write_file(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, contents).unwrap();
    }

    #[test]
    fn push_then_pull_round_trips_pack() {
        let remote = init_bare_remote();
        let remote_url = remote.path().to_string_lossy().to_string();

        let config = GithubShareConfig {
            repo_url: remote_url.clone(),
            branch: "main".to_string(),
            subpath: ".ax".to_string(),
            token: String::new(),
        };

        let pack_src = TempDir::new().unwrap();
        write_file(
            &pack_src.path().join("manifest.json"),
            r#"{"version":1,"rules":[],"skills":[]}"#,
        );
        write_file(
            &pack_src.path().join("rules").join("example.mdc"),
            "---\nid: example\n---\n# Example\n",
        );

        let uploaded = push_github(&config, pack_src.path(), None).expect("push should succeed");
        assert_eq!(uploaded, 2);

        let dest = TempDir::new().unwrap();
        let pulled = pull_github(&config, dest.path()).expect("pull should succeed");
        let pack_dir = pulled.pack_dir.expect("pack_dir should be set");
        assert!(pack_dir.join("manifest.json").is_file());
        assert!(pack_dir.join("rules").join("example.mdc").is_file());
    }

    #[test]
    fn push_with_no_changes_is_a_noop() {
        let remote = init_bare_remote();
        let remote_url = remote.path().to_string_lossy().to_string();
        let config = GithubShareConfig {
            repo_url: remote_url,
            branch: "main".to_string(),
            subpath: ".ax".to_string(),
            token: String::new(),
        };

        let pack_src = TempDir::new().unwrap();
        write_file(
            &pack_src.path().join("manifest.json"),
            r#"{"version":1,"rules":[],"skills":[]}"#,
        );

        let first = push_github(&config, pack_src.path(), None).expect("first push should succeed");
        assert_eq!(first, 1);

        let second = push_github(&config, pack_src.path(), None).expect("second push should succeed");
        assert_eq!(second, 0);
    }

    #[test]
    fn push_requires_repo_url() {
        let config = GithubShareConfig {
            repo_url: String::new(),
            branch: "main".to_string(),
            subpath: ".ax".to_string(),
            token: String::new(),
        };
        let pack_src = TempDir::new().unwrap();
        let err = push_github(&config, pack_src.path(), None).unwrap_err();
        assert!(err.contains("not configured"));
    }
}
