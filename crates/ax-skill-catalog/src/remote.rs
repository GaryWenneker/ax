use crate::source::{ref_candidates, GitHost, SkillSource};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

pub struct Fetched {
    pub tree: PathBuf,
    pub sha: String,
    pub source_id: String,
    pub source_type: String,
    pub well_known_index: Option<String>,
    pub file_base: String,
    /// The skill folder inside `tree` that a git source points at, after
    /// any branch name with a slash has been split off.
    pub skill_path: Option<String>,
    _temp: Option<tempfile::TempDir>,
}

impl Fetched {
    /// `tree`, narrowed to the folder the source URL points at. A path that
    /// is absolute, contains `..` or runs through a link out of `tree` is
    /// refused.
    pub fn skill_tree(&self) -> Result<PathBuf, String> {
        match &self.skill_path {
            Some(path) => ax_utils::contained_join(&self.tree, path)
                .map_err(|e| format!("skill path {path:?} is not inside the source: {e}")),
            None => Ok(self.tree.clone()),
        }
    }
}

/// [`fetch_source_allowing`] with no private hosts allowed.
pub fn fetch_source(source: &SkillSource) -> Result<Fetched, String> {
    fetch_source_allowing(source, &[])
}

/// Fetches a source. A well-known index is downloaded with
/// [`http_bytes_allowing`], so `allow_private_hosts` names the hosts that may
/// resolve to a private address.
pub fn fetch_source_allowing(
    source: &SkillSource,
    allow_private_hosts: &[String],
) -> Result<Fetched, String> {
    match source {
        SkillSource::Local { path } => Ok(Fetched {
            tree: PathBuf::from(path),
            sha: "local".into(),
            source_id: path.clone(),
            source_type: "local".into(),
            well_known_index: None,
            file_base: String::new(),
            skill_path: None,
            _temp: None,
        }),
        SkillSource::Git {
            clone_url,
            source_id,
            reference,
            host,
            path,
        } => {
            if let Some(path) = path {
                crate::source::clean_skill_path(path)?;
            }
            let temp = tempfile::tempdir().map_err(|e| e.to_string())?;
            let (tree, skill_path) = clone_first_ref(
                reference.as_deref(),
                path.as_deref(),
                temp.path(),
                |r, dest| git_clone(clone_url, r, dest),
            )?;
            let sha = git_rev(&tree).unwrap_or_else(|_| "unknown".into());
            let source_type = match host {
                GitHost::GitHub => "github",
                GitHost::GitLab => "gitlab",
                GitHost::Azure => "azure",
            };
            Ok(Fetched {
                tree,
                sha,
                source_id: source_id.clone(),
                source_type: source_type.into(),
                well_known_index: None,
                file_base: String::new(),
                skill_path,
                _temp: Some(temp),
            })
        }
        SkillSource::WellKnown { base_url, .. } => {
            let (index_url, body) = fetch_index(base_url, allow_private_hosts)?;
            let file_base = index_url.trim_end_matches("/index.json").to_string();
            Ok(Fetched {
                tree: PathBuf::new(),
                sha: "well-known".into(),
                source_id: base_url.clone(),
                source_type: "well-known".into(),
                well_known_index: Some(body),
                file_base,
                skill_path: None,
                _temp: None,
            })
        }
    }
}

fn fetch_index(base: &str, allow_private_hosts: &[String]) -> Result<(String, String), String> {
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
        match http_text_allowing(&url, allow_private_hosts) {
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

/// Deadline for git commands that talk to a remote (clone, push).
const GIT_NETWORK_TIMEOUT: Duration = Duration::from_secs(300);
/// Deadline for git commands that only touch the local repository.
const GIT_LOCAL_TIMEOUT: Duration = Duration::from_secs(60);

/// A git command that never asks for input: terminal prompts and interactive
/// credential managers are off, so a private or mistyped repository fails
/// instead of waiting for a username on the server's terminal. Messages are
/// in the C locale, so [`clone_first_ref`] can recognize a missing branch on
/// a system whose git is translated.
fn git_command() -> Command {
    let mut cmd = Command::new("git");
    cmd.env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "never")
        .env("LC_ALL", "C")
        .env("LANGUAGE", "C");
    cmd
}

fn run_git(cmd: &mut Command, timeout: Duration) -> Result<crate::process::CmdOutput, String> {
    crate::process::run_with_timeout(cmd, timeout)
}

/// Clones with each (reference, path) pair from [`ref_candidates`] until one
/// works, so a tree URL whose branch name holds a slash still resolves. Only a
/// missing remote branch moves on to the next pair; any other error stops.
/// Returns the clone folder (under `root`) and the skill path inside it.
fn clone_first_ref(
    reference: Option<&str>,
    path: Option<&str>,
    root: &Path,
    mut clone: impl FnMut(Option<&str>, &Path) -> Result<(), String>,
) -> Result<(PathBuf, Option<String>), String> {
    let mut first_err = None;
    for (i, (reference, path)) in ref_candidates(reference, path).into_iter().enumerate() {
        let dest = root.join(format!("repo-{i}"));
        match clone(reference.as_deref(), &dest) {
            Ok(()) => return Ok((dest, path)),
            Err(e) => {
                let missing_branch = e.contains("not found in upstream");
                first_err.get_or_insert(e);
                if !missing_branch {
                    break;
                }
            }
        }
    }
    Err(first_err.unwrap_or_else(|| "git clone failed".into()))
}

pub fn git_clone(url: &str, reference: Option<&str>, dest: &Path) -> Result<(), String> {
    git_clone_within(url, reference, dest, GIT_NETWORK_TIMEOUT)
}

/// Accepts only `https://`, `ssh://` and scp-style `user@host:path`
/// remotes. Anything else (a value starting with `-`, `ext::`, `file://`, a
/// local path, whitespace or control characters) is refused before git sees
/// it, because the remote can come from a project file.
fn check_git_remote(url: &str) -> Result<(), String> {
    let refuse = || {
        Err(format!(
            "unsupported git remote {url:?}: use https://, ssh:// or user@host:path"
        ))
    };
    if url.is_empty()
        || url.starts_with('-')
        || url.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return refuse();
    }
    let lower = url.to_ascii_lowercase();
    for scheme in ["https://", "ssh://"] {
        if let Some(rest) = lower.strip_prefix(scheme) {
            let host = rest.split(['/', '?', '#']).next().unwrap_or("");
            let host = host.rsplit('@').next().unwrap_or("");
            if host.is_empty() || host.starts_with('-') {
                return refuse();
            }
            return Ok(());
        }
    }
    // scp-style: user@host:path, with no scheme and no `::` transport.
    let Some((user_host, path)) = url.split_once(':') else {
        return refuse();
    };
    let Some((user, host)) = user_host.split_once('@') else {
        return refuse();
    };
    let simple = |s: &str| {
        !s.is_empty()
            && !s.starts_with('-')
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
    };
    if !simple(user)
        || !simple(host)
        || path.is_empty()
        || path.starts_with(':')
        || path.starts_with('-')
    {
        return refuse();
    }
    Ok(())
}

fn git_clone_within(
    url: &str,
    reference: Option<&str>,
    dest: &Path,
    timeout: Duration,
) -> Result<(), String> {
    check_git_remote(url)?;
    let mut cmd = git_command();
    cmd.arg("clone").arg("--depth").arg("1");
    if let Some(reference) = reference {
        if reference.is_empty() || reference.starts_with('-') {
            return Err(format!("not a git ref: {reference}"));
        }
        cmd.arg("--branch").arg(reference);
    }
    cmd.arg("--").arg(url).arg(dest);
    let output = run_git(&mut cmd, timeout).map_err(|e| format!("git clone: {e}"))?;
    if !output.status.success() {
        return Err(format!("git clone failed: {}", output.stderr.trim()));
    }
    Ok(())
}

pub fn git_rev(dest: &Path) -> Result<String, String> {
    let mut cmd = git_command();
    cmd.args(["rev-parse", "HEAD"]).current_dir(dest);
    let output = run_git(&mut cmd, GIT_LOCAL_TIMEOUT)?;
    if !output.status.success() {
        return Err("git rev-parse failed".into());
    }
    Ok(output.stdout.trim().to_string())
}

pub fn git_publish(repo: &Path, name: &str) -> Result<(), String> {
    let mut add = git_command();
    add.args(["add", "--", &format!("skills/{name}")])
        .current_dir(repo);
    let add = run_git(&mut add, GIT_LOCAL_TIMEOUT)?;
    if !add.status.success() {
        return Err(add.stderr.trim().to_string());
    }
    let mut commit = git_command();
    commit
        .args(["commit", "-m", &format!("Publish skill {name}")])
        .current_dir(repo);
    let commit = run_git(&mut commit, GIT_LOCAL_TIMEOUT)?;
    if !commit.status.success()
        && !commit.stderr.contains("nothing to commit")
        && !commit.stdout.contains("nothing to commit")
    {
        return Err(commit.stderr.trim().to_string());
    }
    let mut push = git_command();
    push.args(["push"]).current_dir(repo);
    let push = run_git(&mut push, GIT_NETWORK_TIMEOUT).map_err(|e| format!("git push: {e}"))?;
    if !push.status.success() {
        return Err(format!("git push failed: {}", push.stderr.trim()));
    }
    Ok(())
}

pub fn http_text(url: &str) -> Result<String, String> {
    http_text_allowing(url, &[])
}

/// [`http_bytes_allowing`], decoded as UTF-8.
pub fn http_text_allowing(url: &str, allow_private_hosts: &[String]) -> Result<String, String> {
    String::from_utf8(http_bytes_allowing(url, allow_private_hosts)?).map_err(|e| e.to_string())
}

/// Largest body a skill download may have.
const DOWNLOAD_MAX_BYTES: u64 = 25 * 1024 * 1024;
/// Redirects followed per download; each hop is vetted again.
const MAX_REDIRECTS: usize = 5;
/// Deadline for one HTTP request, from connect to the last body byte.
const HTTP_TIMEOUT: Duration = Duration::from_secs(120);

type Resolve<'a> = &'a dyn Fn(&str, u16) -> Result<Vec<SocketAddr>, String>;
type Blocked<'a> = &'a dyn Fn(IpAddr) -> bool;

/// A URL that passed the address checks, with the addresses the connection
/// must use when the host is a name.
struct Vetted {
    url: reqwest::Url,
    pin: Option<(String, Vec<SocketAddr>)>,
}

fn system_resolve(host: &str, port: u16) -> Result<Vec<SocketAddr>, String> {
    (host, port)
        .to_socket_addrs()
        .map(Iterator::collect)
        .map_err(|e| format!("could not resolve {host}: {e}"))
}

/// GET `url`, refusing local hosts and private addresses.
pub fn http_bytes(url: &str) -> Result<Vec<u8>, String> {
    http_bytes_allowing(url, &[])
}

/// The URL of one file listed in a well-known skill index. A relative entry
/// resolves under the index; an absolute entry must share the index's origin
/// (scheme, host and port), so an index cannot point the download at another
/// server or downgrade it from https to http. The CLI and the web server both
/// resolve index files with this.
pub fn well_known_file_url(base: &str, name: &str, file: &str) -> Result<String, String> {
    if !(file.starts_with("http://") || file.starts_with("https://")) {
        return Ok(format!("{base}/{name}/{file}"));
    }
    let index = reqwest::Url::parse(base).map_err(|e| format!("bad index URL {base}: {e}"))?;
    let target = reqwest::Url::parse(file).map_err(|e| format!("bad file URL {file}: {e}"))?;
    if index.origin() != target.origin() {
        return Err(format!(
            "skill file {file} is not on the index origin {}",
            index.origin().ascii_serialization()
        ));
    }
    Ok(file.to_string())
}

/// [`http_bytes_same_origin_allowing`] with no private hosts allowed.
pub fn http_bytes_same_origin(url: &str) -> Result<Vec<u8>, String> {
    http_bytes_same_origin_allowing(url, &[])
}

/// GET `url`, refusing local hosts and private addresses unless the URL's host
/// (or a redirect hop's host) is named in `allow_private_hosts`. The names
/// come from `allowPrivateHosts` in `.ax/skill-catalog.json`; each use is
/// logged.
pub fn http_bytes_allowing(url: &str, allow_private_hosts: &[String]) -> Result<Vec<u8>, String> {
    fetch_vetted(url, &system_resolve, &private_ip, allow_private_hosts)
}

/// [`http_bytes_allowing`] for a file listed in a well-known skill index:
/// redirects are followed only while they stay on the origin (scheme, host
/// and port) of `url`, so a listed file cannot hand the download to another
/// server or downgrade it to http.
pub fn http_bytes_same_origin_allowing(
    url: &str,
    allow_private_hosts: &[String],
) -> Result<Vec<u8>, String> {
    fetch_vetted_with(url, &system_resolve, &private_ip, allow_private_hosts, true)
}

/// GET `url`, refusing private targets. The host is resolved once, the
/// addresses are checked, and the connection is pinned to exactly those
/// addresses, so a second DNS answer cannot point it somewhere else.
/// Redirects are followed by hand so every hop goes through the same check.
fn fetch_vetted(
    url: &str,
    resolve: Resolve,
    blocked: Blocked,
    allow: &[String],
) -> Result<Vec<u8>, String> {
    fetch_vetted_with(url, resolve, blocked, allow, false)
}

/// [`fetch_vetted`]; with `same_origin`, a redirect that leaves the origin of
/// `url` is refused.
fn fetch_vetted_with(
    url: &str,
    resolve: Resolve,
    blocked: Blocked,
    allow: &[String],
    same_origin: bool,
) -> Result<Vec<u8>, String> {
    let origin = reqwest::Url::parse(url)
        .map_err(|e| e.to_string())?
        .origin();
    let mut current = url.to_string();
    for _ in 0..=MAX_REDIRECTS {
        let vetted = vet(&current, resolve, blocked, allow)?;
        let mut builder = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(HTTP_TIMEOUT);
        if let Some((host, addrs)) = &vetted.pin {
            builder = builder.resolve_to_addrs(host, addrs);
        }
        let client = builder.build().map_err(|e| e.to_string())?;
        let response = client
            .get(vetted.url.clone())
            .send()
            .map_err(|e| e.to_string())?;
        if response.status().is_redirection() {
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .ok_or_else(|| format!("GET {current} redirected without a location"))?;
            let next = vetted
                .url
                .join(location)
                .map_err(|e| format!("GET {current} redirected to a bad url: {e}"))?;
            if same_origin && next.origin() != origin {
                return Err(format!(
                    "redirect to {} leaves the index origin",
                    next.origin().ascii_serialization()
                ));
            }
            current = next.to_string();
            continue;
        }
        if !response.status().is_success() {
            return Err(format!("GET {current} returned {}", response.status()));
        }
        if response
            .content_length()
            .is_some_and(|len| len > DOWNLOAD_MAX_BYTES)
        {
            return Err("download is larger than 25 MiB".into());
        }
        // Read at most the cap, whatever the server claims, so a large body is
        // never held in memory.
        return ax_utils::read_to_end_bounded(response, DOWNLOAD_MAX_BYTES).map_err(|e| {
            if e.kind() == std::io::ErrorKind::InvalidData {
                "download is larger than 25 MiB".to_string()
            } else {
                e.to_string()
            }
        });
    }
    Err(format!(
        "GET {url} redirected more than {MAX_REDIRECTS} times"
    ))
}

#[cfg(test)]
fn vetted_download(url: &str) -> Result<reqwest::Url, String> {
    vet(url, &system_resolve, &private_ip, &[]).map(|vetted| vetted.url)
}

/// Normal form of a host name for comparing with the allow list.
fn host_key(host: &str) -> String {
    host.trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim_end_matches('.')
        .to_ascii_lowercase()
}

fn vet(url: &str, resolve: Resolve, blocked: Blocked, allow: &[String]) -> Result<Vetted, String> {
    const PRIVATE: &str = "skill downloads cannot target a private address";
    let parsed = reqwest::Url::parse(url).map_err(|e| e.to_string())?;
    if parsed.scheme() != "https" && parsed.scheme() != "http" {
        return Err("skill downloads use http or https".into());
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("skill download urls cannot embed credentials".into());
    }
    let host = parsed
        .host_str()
        .filter(|host| !host.is_empty())
        .ok_or("skill download url has no host")?;
    // IPv6 literals come back in brackets; strip them before parsing.
    let bare = host
        .strip_prefix('[')
        .and_then(|inner| inner.strip_suffix(']'))
        .unwrap_or(host);
    let allowed = {
        let key = host_key(host);
        allow.iter().any(|entry| host_key(entry) == key)
    };
    if allowed {
        tracing::warn!(
            host,
            "skill download allowed to a private host by allowPrivateHosts"
        );
    }
    if let Ok(ip) = bare.parse::<IpAddr>() {
        if blocked(ip) && !allowed {
            return Err(PRIVATE.into());
        }
        return Ok(Vetted {
            url: parsed,
            pin: None,
        });
    }
    let domain = host.to_string();
    let name = domain.trim_end_matches('.').to_ascii_lowercase();
    if !allowed && (name == "localhost" || name.ends_with(".localhost") || name.ends_with(".local"))
    {
        return Err("skill downloads cannot target a local host".into());
    }
    let port = parsed.port_or_known_default().unwrap_or(443);
    let addrs = resolve(&domain, port)?;
    if addrs.is_empty() {
        return Err("skill download host did not resolve".into());
    }
    if !allowed && addrs.iter().any(|addr| blocked(addr.ip())) {
        return Err(PRIVATE.into());
    }
    Ok(Vetted {
        url: parsed,
        pin: Some((domain, addrs)),
    })
}

fn private_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_unspecified()
        || ip.is_documentation()
        || ip.is_multicast()
        // 0.0.0.0/8 "this network"
        || a == 0
        // 100.64.0.0/10 shared address space (carrier-grade NAT)
        || (a == 100 && (b & 0xc0) == 64)
        // 192.0.0.0/24 protocol assignments
        || (a == 192 && b == 0 && c == 0)
        // 198.18.0.0/15 benchmarking
        || (a == 198 && (b & 0xfe) == 18)
        // 240.0.0.0/4 reserved
        || a >= 240
}

fn private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => private_v4(ip),
        IpAddr::V6(ip) => {
            if let Some(v4) = ip.to_ipv4_mapped() {
                return private_v4(v4);
            }
            let s = ip.segments();
            // 64:ff9b::/96 NAT64 carries an IPv4 address in the last 32 bits.
            if s[..6] == [0x64, 0xff9b, 0, 0, 0, 0] {
                let [_, _, _, _, _, _, hi, lo] = s;
                return private_v4(Ipv4Addr::from((u32::from(hi) << 16) | u32::from(lo)));
            }
            ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_multicast()
                || ip.is_unicast_link_local()
                // fc00::/7 unique local
                || (s[0] & 0xfe00) == 0xfc00
                // fec0::/10 site local
                || (s[0] & 0xffc0) == 0xfec0
                // 2001:db8::/32 documentation
                || (s[0] == 0x2001 && s[1] == 0x0db8)
        }
    }
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

#[cfg(test)]
mod tests {
    use super::{vetted_download, well_known_file_url};

    /// A same-origin fetch that lets the loopback test servers through; the
    /// address vetting is tested on its own below.
    fn fetch_any(url: &str) -> Result<Vec<u8>, String> {
        super::fetch_vetted_with(url, &super::system_resolve, &|_| false, &[], true)
    }

    /// Serve one HTTP response: `head` (status line and headers), then
    /// `body_len` bytes of body, then keep the connection open for 40 s.
    fn stalling_server(head: &'static str, body_len: usize) -> String {
        use std::io::{Read as _, Write as _};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            let _ = stream.write_all(head.as_bytes());
            let chunk = vec![b'a'; 64 * 1024];
            let mut sent = 0;
            while sent < body_len {
                let n = chunk.len().min(body_len - sent);
                if stream.write_all(&chunk[..n]).is_err() {
                    return;
                }
                sent += n;
            }
            std::thread::sleep(std::time::Duration::from_secs(40));
        });
        format!("http://{addr}/file")
    }

    #[test]
    fn a_declared_oversize_body_is_refused_without_reading_it() {
        let url = stalling_server("HTTP/1.1 200 OK\r\nContent-Length: 104857600\r\n\r\n", 1024);
        let started = std::time::Instant::now();
        let err = fetch_any(&url).unwrap_err();
        assert!(err.contains("25 MiB"), "{err}");
        assert!(started.elapsed() < std::time::Duration::from_secs(10));
    }

    #[test]
    fn an_endless_body_stops_at_the_limit() {
        let url = stalling_server(
            "HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n",
            26 * 1024 * 1024,
        );
        let started = std::time::Instant::now();
        let err = fetch_any(&url).unwrap_err();
        assert!(err.contains("25 MiB"), "{err}");
        assert!(started.elapsed() < std::time::Duration::from_secs(10));
    }

    /// Answer one request with `response`, return the server's base URL.
    fn one_shot_server(response: String) -> String {
        use std::io::{Read as _, Write as _};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf);
                let _ = stream.write_all(response.as_bytes());
            }
        });
        format!("http://{addr}")
    }

    #[test]
    fn a_same_origin_fetch_refuses_a_redirect_to_another_origin() {
        let other = one_shot_server(
            "HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nevil!".to_string(),
        );
        let start = one_shot_server(format!(
            "HTTP/1.1 302 Found\r\nLocation: {other}/SKILL.md\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        ));
        let err = fetch_any(&format!("{start}/x/SKILL.md")).unwrap_err();
        assert!(err.contains("origin"), "{err}");
    }

    #[test]
    fn a_same_origin_fetch_follows_a_same_origin_redirect() {
        use std::io::{Read as _, Write as _};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let replies = [
                "HTTP/1.1 302 Found\r\nLocation: /moved/SKILL.md\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
            ];
            for reply in replies {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf);
                let _ = stream.write_all(reply.as_bytes());
            }
        });
        let got = fetch_any(&format!("http://{addr}/x/SKILL.md")).unwrap();
        assert_eq!(got, b"ok");
    }

    #[test]
    fn well_known_file_urls_stay_on_the_index_origin() {
        let base = "https://skills.example.org/.well-known/agent-skills";
        assert_eq!(
            well_known_file_url(base, "x", "SKILL.md").unwrap(),
            "https://skills.example.org/.well-known/agent-skills/x/SKILL.md"
        );
        assert_eq!(
            well_known_file_url(base, "x", "https://skills.example.org/cdn/x/SKILL.md").unwrap(),
            "https://skills.example.org/cdn/x/SKILL.md"
        );
        for other in [
            "https://evil.example.net/SKILL.md",
            "http://skills.example.org/x/SKILL.md",
            "https://skills.example.org:8443/x/SKILL.md",
        ] {
            let err = well_known_file_url(base, "x", other).unwrap_err();
            assert!(err.contains("index origin"), "{other}: {err}");
        }
    }

    fn fetched(tree: &std::path::Path, skill_path: &str) -> super::Fetched {
        super::Fetched {
            tree: tree.to_path_buf(),
            sha: String::new(),
            source_id: String::new(),
            source_type: "github".into(),
            well_known_index: None,
            file_base: String::new(),
            skill_path: Some(skill_path.into()),
            _temp: None,
        }
    }

    #[test]
    fn skill_tree_stays_inside_the_clone() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("skills/pdf")).unwrap();
        assert_eq!(
            fetched(root.path(), "skills/pdf").skill_tree().unwrap(),
            root.path().join("skills/pdf")
        );
        assert!(fetched(root.path(), "../../etc").skill_tree().is_err());
        assert!(fetched(root.path(), "/etc").skill_tree().is_err());
    }

    #[cfg(unix)]
    #[test]
    fn skill_tree_refuses_a_link_out_of_the_clone() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("skills")).unwrap();
        assert!(fetched(root.path(), "skills").skill_tree().is_err());
    }

    #[test]
    fn a_git_source_with_an_escaping_path_is_refused_before_cloning() {
        let source = crate::source::SkillSource::Git {
            host: crate::source::GitHost::GitHub,
            clone_url: "https://github.com/o/r.git".into(),
            source_id: "o/r".into(),
            reference: None,
            path: Some("../../x".into()),
        };
        let err = super::fetch_source(&source).err().unwrap();
        assert!(err.contains("skill path"), "{err}");
    }

    #[test]
    fn a_slashed_branch_in_a_tree_url_is_found_by_retrying() {
        let root = tempfile::tempdir().unwrap();
        let mut tried = Vec::new();
        let (tree, path) = super::clone_first_ref(
            Some("feature"),
            Some("x/skills/pdf"),
            root.path(),
            |reference, _dest| {
                let reference = reference.unwrap().to_string();
                tried.push(reference.clone());
                if reference == "feature/x" {
                    Ok(())
                } else {
                    Err(format!(
                        "git clone failed: fatal: Remote branch {reference} not found in upstream origin"
                    ))
                }
            },
        )
        .unwrap();
        assert_eq!(tried, ["feature", "feature/x"]);
        assert_eq!(path.as_deref(), Some("skills/pdf"));
        assert!(tree.starts_with(root.path()));
    }

    #[test]
    fn a_clone_error_that_is_not_a_missing_branch_is_not_retried() {
        let root = tempfile::tempdir().unwrap();
        let mut calls = 0;
        let err = super::clone_first_ref(Some("main"), Some("a/b"), root.path(), |_, _| {
            calls += 1;
            Err("git clone failed: repository not found".into())
        })
        .unwrap_err();
        assert_eq!(calls, 1);
        assert!(err.contains("repository not found"), "{err}");
    }

    /// A server that accepts connections and never answers.
    fn silent_server() -> (std::net::TcpListener, u16) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        (listener, port)
    }

    #[test]
    fn a_git_clone_from_a_silent_server_stops_at_the_deadline() {
        let (_listener, port) = silent_server();
        let dest = tempfile::tempdir().unwrap();
        let path = dest.path().join("clone");
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let result = super::git_clone_within(
                &format!("https://127.0.0.1:{port}/repo.git"),
                None,
                &path,
                std::time::Duration::from_secs(2),
            );
            let _ = tx.send(result);
        });
        let result = rx
            .recv_timeout(std::time::Duration::from_secs(20))
            .expect("git clone must stop at its deadline");
        let err = result.unwrap_err();
        assert!(err.contains("timed out"), "{err}");
    }

    #[test]
    fn git_clone_refuses_option_like_and_unsupported_remotes() {
        let dest = tempfile::tempdir().unwrap();
        let marker = dest.path().join("pwned");
        for url in [
            format!("--upload-pack=touch {};", marker.display()),
            "-c".to_string(),
            "ext::sh -c touch% /tmp/x".to_string(),
            "file:///etc".to_string(),
            "/tmp/repo".to_string(),
            "http://example.com/repo.git".to_string(),
            "https://example.com/repo.git\n--upload-pack=x".to_string(),
            String::new(),
        ] {
            let err = super::git_clone_within(
                &url,
                None,
                &dest.path().join("clone"),
                std::time::Duration::from_secs(5),
            )
            .unwrap_err();
            assert!(err.contains("git remote"), "{url}: {err}");
        }
        assert!(!marker.exists());
    }

    #[test]
    fn git_remote_check_accepts_https_ssh_and_scp_forms() {
        for url in [
            "https://github.com/you/skills.git",
            "ssh://git@example.com/you/skills.git",
            "git@github.com:you/skills.git",
        ] {
            assert!(super::check_git_remote(url).is_ok(), "{url}");
        }
    }

    #[test]
    fn git_commands_run_in_the_c_locale() {
        let cmd = super::git_command();
        let envs: Vec<_> = cmd.get_envs().collect();
        for key in ["LC_ALL", "LANGUAGE"] {
            assert!(
                envs.contains(&(std::ffi::OsStr::new(key), Some(std::ffi::OsStr::new("C")))),
                "{key}"
            );
        }
    }

    #[test]
    fn git_commands_never_prompt() {
        let cmd = super::git_command();
        let envs: Vec<_> = cmd.get_envs().collect();
        assert!(envs.contains(&(
            std::ffi::OsStr::new("GIT_TERMINAL_PROMPT"),
            Some(std::ffi::OsStr::new("0"))
        )));
        assert!(envs.contains(&(
            std::ffi::OsStr::new("GCM_INTERACTIVE"),
            Some(std::ffi::OsStr::new("never"))
        )));
    }

    /// Serves `count` requests: `/redirect` answers 302 to the location given,
    /// every other path answers 200 with the path as the body.
    fn tiny_http(count: usize, location: String) -> u16 {
        use std::io::{BufRead, BufReader, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for stream in listener.incoming().take(count) {
                let mut stream = stream.unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
                loop {
                    let mut header = String::new();
                    if reader.read_line(&mut header).unwrap() == 0 || header == "\r\n" {
                        break;
                    }
                }
                let reply = if path == "/redirect" {
                    format!("HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                } else {
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{path}",
                        path.len()
                    )
                };
                stream.write_all(reply.as_bytes()).unwrap();
            }
        });
        port
    }

    fn v4_private_only(ip: std::net::IpAddr) -> bool {
        matches!(ip, std::net::IpAddr::V4(v4) if v4.is_private())
    }

    #[test]
    fn the_connection_uses_the_vetted_addresses() {
        let port = tiny_http(1, String::new());
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let resolve = |host: &str, _port: u16| {
            calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            assert_eq!(host, "skills.invalid");
            Ok(vec![std::net::SocketAddr::from(([127, 0, 0, 1], port))])
        };
        // `.invalid` never resolves in real DNS: the request only lands when the
        // client connects to the address the check approved.
        let body = super::fetch_vetted(
            &format!("http://skills.invalid:{port}/index.json"),
            &resolve,
            &v4_private_only,
            &[],
        )
        .unwrap();
        assert_eq!(body, b"/index.json");
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test]
    fn a_body_past_the_cap_is_refused_without_a_content_length() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n");
            let chunk = vec![b'x'; 1024 * 1024];
            for _ in 0..30 {
                if stream.write_all(&chunk).is_err() {
                    break;
                }
            }
        });
        let resolve =
            move |_: &str, _: u16| Ok(vec![std::net::SocketAddr::from(([127, 0, 0, 1], port))]);
        let err = super::fetch_vetted(
            &format!("http://big.invalid:{port}/skill.zip"),
            &resolve,
            &v4_private_only,
            &[],
        )
        .unwrap_err();
        assert!(err.contains("larger than 25 MiB"), "{err}");
    }

    #[test]
    fn a_redirect_is_vetted_and_pinned_again() {
        let port = tiny_http(2, "/next".into());
        let resolve =
            move |_: &str, _: u16| Ok(vec![std::net::SocketAddr::from(([127, 0, 0, 1], port))]);
        let body = super::fetch_vetted(
            &format!("http://a.invalid:{port}/redirect"),
            &resolve,
            &v4_private_only,
            &[],
        )
        .unwrap();
        assert_eq!(body, b"/next");

        let port = tiny_http(1, "http://evil.invalid/x".into());
        let resolve = move |host: &str, _: u16| {
            Ok(vec![if host == "evil.invalid" {
                std::net::SocketAddr::from(([10, 0, 0, 1], 80))
            } else {
                std::net::SocketAddr::from(([127, 0, 0, 1], port))
            }])
        };
        let err = super::fetch_vetted(
            &format!("http://a.invalid:{port}/redirect"),
            &resolve,
            &v4_private_only,
            &[],
        )
        .unwrap_err();
        assert!(err.contains("private address"), "{err}");
    }

    #[test]
    fn an_allow_listed_private_host_is_fetched_and_others_are_refused() {
        let port = tiny_http(1, String::new());
        let resolve = move |host: &str, _: u16| {
            Ok(vec![if host == "skills.intranet.invalid" {
                std::net::SocketAddr::from(([127, 0, 0, 1], port))
            } else {
                std::net::SocketAddr::from(([10, 0, 0, 1], port))
            }])
        };
        let allow = vec!["Skills.Intranet.invalid".to_string()];
        let body = super::fetch_vetted(
            &format!("http://skills.intranet.invalid:{port}/index.json"),
            &resolve,
            &super::private_ip,
            &allow,
        )
        .unwrap();
        assert_eq!(body, b"/index.json");

        // A host on the list that resolves to 10.0.0.1 passes the check.
        let ten = |_: &str, _: u16| Ok(vec![std::net::SocketAddr::from(([10, 0, 0, 1], 443))]);
        let allow = vec!["skills.corp.invalid".to_string()];
        assert!(super::vet(
            "https://skills.corp.invalid/index.json",
            &ten,
            &super::private_ip,
            &allow
        )
        .is_ok());
        // A host not on the list that resolves to 10.0.0.1 is refused.
        let err = super::vet(
            "https://other.corp.invalid/index.json",
            &ten,
            &super::private_ip,
            &allow,
        )
        .err()
        .unwrap();
        assert!(err.contains("private address"), "{err}");
        // The strict default has no list.
        let err = super::vet(
            "https://skills.corp.invalid/index.json",
            &ten,
            &super::private_ip,
            &[],
        )
        .err()
        .unwrap();
        assert!(err.contains("private address"), "{err}");
    }

    #[test]
    fn mapped_shared_and_this_network_addresses_are_private() {
        for ip in [
            "::ffff:127.0.0.1",
            "::ffff:169.254.169.254",
            "::ffff:10.0.0.1",
            "100.64.0.1",
            "100.127.255.254",
            "0.1.2.3",
            "198.18.0.1",
            "192.0.0.1",
            "240.0.0.1",
            "64:ff9b::7f00:1",
            "fec0::1",
            "2001:db8::1",
        ] {
            let ip: std::net::IpAddr = ip.parse().unwrap();
            assert!(super::private_ip(ip), "{ip}");
        }
        for ip in [
            "8.8.8.8",
            "100.128.0.1",
            "2606:4700:4700::1111",
            "::ffff:8.8.8.8",
        ] {
            let ip: std::net::IpAddr = ip.parse().unwrap();
            assert!(!super::private_ip(ip), "{ip}");
        }
    }

    #[test]
    fn ipv6_literals_are_checked_as_addresses_not_resolved() {
        for url in [
            "https://[::1]/index.json",
            "https://[::ffff:127.0.0.1]/index.json",
            "https://[fd00::1]/index.json",
        ] {
            let err = vetted_download(url).unwrap_err();
            assert!(err.contains("private address"), "{url}: {err}");
        }
        assert!(vetted_download("https://[2606:4700:4700::1111]/index.json").is_ok());
        assert!(vetted_download("https://1.1.1.1/index.json").is_ok());
    }

    #[test]
    fn skill_downloads_reject_private_and_non_http_targets() {
        for url in [
            "http://127.0.0.1/latest",
            "http://169.254.169.254/latest/meta-data",
            "https://10.1.2.3/index.json",
            "https://[::1]/index.json",
            "file:///etc/passwd",
            "https://user:secret@example.com/index.json",
            "http://localhost/index.json",
        ] {
            assert!(vetted_download(url).is_err(), "{url}");
        }
    }
}
