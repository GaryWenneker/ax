use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitHost {
    GitHub,
    GitLab,
    Azure,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillSource {
    Git {
        host: GitHost,
        clone_url: String,
        /// owner/repo, group/repo, or org/project/repo
        source_id: String,
        reference: Option<String>,
        path: Option<String>,
    },
    WellKnown {
        base_url: String,
        skill_name: Option<String>,
    },
    Local {
        path: String,
    },
}

fn looks_local(raw: &str) -> bool {
    raw == "."
        || raw.starts_with("./")
        || raw.starts_with(".\\")
        || raw.starts_with('/')
        || raw.starts_with('~')
        || (raw.len() > 2 && raw.as_bytes()[1] == b':')
        || Path::new(raw).exists()
}

fn parse_github_shorthand(raw: &str) -> Result<SkillSource, String> {
    let mut parts = raw.split('/');
    let owner = parts.next().unwrap_or("");
    let repo = parts.next().unwrap_or("");
    if owner.is_empty()
        || repo.is_empty()
        || parts.next().is_some()
        || owner.contains(' ')
        || repo.contains(' ')
    {
        return Err(format!("unrecognized skill source: {raw}"));
    }
    let repo = repo.trim_end_matches(".git");
    Ok(SkillSource::Git {
        host: GitHost::GitHub,
        clone_url: format!("https://github.com/{owner}/{repo}.git"),
        source_id: format!("{owner}/{repo}"),
        reference: None,
        path: None,
    })
}

fn parse_url_full(raw: &str) -> Option<SkillSource> {
    if raw.starts_with("git@github.com:") {
        let path = raw.trim_start_matches("git@github.com:");
        let path = path.trim_end_matches(".git");
        let mut parts = path.split('/');
        let owner = parts.next()?;
        let repo = parts.next()?;
        if parts.next().is_some() {
            return None;
        }
        return Some(SkillSource::Git {
            host: GitHost::GitHub,
            clone_url: format!("https://github.com/{owner}/{repo}.git"),
            source_id: format!("{owner}/{repo}"),
            reference: None,
            path: None,
        });
    }
    if !raw.starts_with("http://") && !raw.starts_with("https://") {
        return None;
    }
    let (base, query) = raw.split_once('?').unwrap_or((raw, ""));
    let without_scheme = base.split_once("://")?.1;
    let (host, path) = without_scheme
        .split_once('/')
        .unwrap_or((without_scheme, ""));
    let host = host.split('@').next_back()?.split(':').next()?;
    if host == "github.com" {
        return parse_github_url(path);
    }
    if host == "dev.azure.com" || host.ends_with(".visualstudio.com") {
        return parse_azure_url(host, path, query);
    }
    if host == "gitlab.com" || host.contains("gitlab") || path.contains("/-/") {
        return parse_gitlab_url(host, path);
    }
    Some(parse_well_known_url(raw))
}

fn parse_github_url(path: &str) -> Option<SkillSource> {
    let path = path.trim_end_matches('/');
    let mut parts = path.split('/').filter(|p| !p.is_empty());
    let owner = parts.next()?;
    let repo = parts.next()?.trim_end_matches(".git");
    let mut reference = None;
    let mut skill_path = None;
    if parts.next() == Some("tree") {
        reference = parts.next().map(str::to_string);
        let rest: Vec<&str> = parts.collect();
        if !rest.is_empty() {
            skill_path = Some(rest.join("/"));
        }
    }
    Some(SkillSource::Git {
        host: GitHost::GitHub,
        clone_url: format!("https://github.com/{owner}/{repo}.git"),
        source_id: format!("{owner}/{repo}"),
        reference,
        path: skill_path,
    })
}

fn parse_gitlab_url(host: &str, path: &str) -> Option<SkillSource> {
    let path = path.trim_end_matches('/');
    let (project, reference, skill_path) =
        if let Some((project, rest)) = path.split_once("/-/tree/") {
            let mut segs = rest.split('/');
            let reference = segs.next().map(str::to_string);
            let rest: Vec<&str> = segs.collect();
            let skill_path = if rest.is_empty() {
                None
            } else {
                Some(rest.join("/"))
            };
            (project, reference, skill_path)
        } else if let Some((project, rest)) = path.split_once("/-/blob/") {
            let mut segs = rest.split('/');
            let reference = segs.next().map(str::to_string);
            let rest: Vec<&str> = segs.collect();
            let skill_path = if rest.is_empty() {
                None
            } else {
                Some(rest.join("/"))
            };
            (project, reference, skill_path)
        } else {
            (path.trim_end_matches(".git"), None, None)
        };
    if project.is_empty() || !project.contains('/') {
        return None;
    }
    Some(SkillSource::Git {
        host: GitHost::GitLab,
        clone_url: format!("https://{host}/{project}.git"),
        source_id: project.to_string(),
        reference,
        path: skill_path,
    })
}

fn parse_azure_url(host: &str, path: &str, query: &str) -> Option<SkillSource> {
    let path = path.trim_matches('/');
    let (org, project, repo) = if host == "dev.azure.com" {
        let mut parts = path.split('/');
        let org = parts.next()?;
        let project = parts.next()?;
        let git = parts.next()?;
        if git != "_git" {
            return None;
        }
        let repo = parts.next()?.trim_end_matches(".git");
        (org.to_string(), project.to_string(), repo.to_string())
    } else {
        let org = host.trim_end_matches(".visualstudio.com");
        let mut parts = path.split('/');
        let project = parts.next()?;
        let git = parts.next()?;
        if git != "_git" {
            return None;
        }
        let repo = parts.next()?.trim_end_matches(".git");
        (org.to_string(), project.to_string(), repo.to_string())
    };
    let mut skill_path = None;
    let mut reference = None;
    for pair in query.split('&') {
        let Some((k, v)) = pair.split_once('=') else {
            continue;
        };
        if k == "path" {
            skill_path = Some(v.trim_start_matches('/').to_string());
        }
        if k == "version" {
            reference = Some(v.strip_prefix("GB").unwrap_or(v).to_string());
        }
    }
    let clone_url = if host == "dev.azure.com" {
        format!("https://dev.azure.com/{org}/{project}/_git/{repo}")
    } else {
        format!("https://{org}.visualstudio.com/{project}/_git/{repo}")
    };
    Some(SkillSource::Git {
        host: GitHost::Azure,
        clone_url,
        source_id: format!("{org}/{project}/{repo}"),
        reference,
        path: skill_path.filter(|p| !p.is_empty()),
    })
}

fn parse_well_known_url(raw: &str) -> SkillSource {
    let (base, _) = raw.split_once('?').unwrap_or((raw, ""));
    let base = base.trim_end_matches('/');
    let skill_name = base
        .rsplit_once("/.well-known/agent-skills/")
        .or_else(|| base.rsplit_once("/.well-known/skills/"))
        .map(|(_, name)| name)
        .filter(|name| !name.is_empty() && *name != "index.json")
        .map(str::to_string);
    let base_url = if let Some(name) = &skill_name {
        base.trim_end_matches(name.as_str())
            .trim_end_matches('/')
            .to_string()
    } else {
        base.to_string()
    };
    SkillSource::WellKnown {
        base_url,
        skill_name,
    }
}

pub fn parse_source(input: &str) -> Result<SkillSource, String> {
    let raw = input.trim();
    if raw.is_empty() {
        return Err("source is empty".into());
    }
    if let Some(source) = parse_url_full(raw) {
        return Ok(source);
    }
    if looks_local(raw) {
        return Ok(SkillSource::Local {
            path: raw.to_string(),
        });
    }
    parse_github_shorthand(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_shorthand_and_tree_url() {
        let short = parse_source("vercel-labs/agent-skills").unwrap();
        assert_eq!(
            short,
            SkillSource::Git {
                host: GitHost::GitHub,
                clone_url: "https://github.com/vercel-labs/agent-skills.git".into(),
                source_id: "vercel-labs/agent-skills".into(),
                reference: None,
                path: None,
            }
        );
        let tree = parse_source(
            "https://github.com/vercel-labs/agent-skills/tree/main/skills/web-design-guidelines",
        )
        .unwrap();
        match tree {
            SkillSource::Git {
                reference, path, ..
            } => {
                assert_eq!(reference.as_deref(), Some("main"));
                assert_eq!(path.as_deref(), Some("skills/web-design-guidelines"));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn gitlab_nested_group() {
        let src = parse_source("https://gitlab.com/org/group/repo/-/tree/main/skills/x").unwrap();
        match src {
            SkillSource::Git {
                host,
                source_id,
                reference,
                path,
                ..
            } => {
                assert_eq!(host, GitHost::GitLab);
                assert_eq!(source_id, "org/group/repo");
                assert_eq!(reference.as_deref(), Some("main"));
                assert_eq!(path.as_deref(), Some("skills/x"));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn azure_query_path_and_branch() {
        let src = parse_source(
            "https://dev.azure.com/org/project/_git/repo?path=/skills/web-design&version=GBmain",
        )
        .unwrap();
        match src {
            SkillSource::Git {
                host,
                reference,
                path,
                source_id,
                ..
            } => {
                assert_eq!(host, GitHost::Azure);
                assert_eq!(source_id, "org/project/repo");
                assert_eq!(reference.as_deref(), Some("main"));
                assert_eq!(path.as_deref(), Some("skills/web-design"));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn well_known_and_other_https() {
        let named = parse_source("https://example.com/.well-known/agent-skills/pdf").unwrap();
        assert_eq!(
            named,
            SkillSource::WellKnown {
                base_url: "https://example.com/.well-known/agent-skills".into(),
                skill_name: Some("pdf".into()),
            }
        );
        let other = parse_source("https://example.com/docs").unwrap();
        assert!(matches!(other, SkillSource::WellKnown { .. }));
    }

    #[test]
    fn local_dot() {
        assert!(matches!(
            parse_source("./skills/pdf").unwrap(),
            SkillSource::Local { .. }
        ));
    }
}
