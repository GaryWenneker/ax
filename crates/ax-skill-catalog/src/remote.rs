use crate::source::{GitHost, SkillSource};
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

/// Deadline for git commands that talk to a remote (clone, push).
const GIT_NETWORK_TIMEOUT: Duration = Duration::from_secs(300);
/// Deadline for git commands that only touch the local repository.
const GIT_LOCAL_TIMEOUT: Duration = Duration::from_secs(60);

/// A git command that never asks for input: terminal prompts and interactive
/// credential managers are off, so a private or mistyped repository fails
/// instead of waiting for a username on the server's terminal.
fn git_command() -> Command {
    let mut cmd = Command::new("git");
    cmd.env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "never");
    cmd
}

fn run_git(cmd: &mut Command, timeout: Duration) -> Result<crate::process::CmdOutput, String> {
    crate::process::run_with_timeout(cmd, timeout)
}

pub fn git_clone(url: &str, reference: Option<&str>, dest: &Path) -> Result<(), String> {
    git_clone_within(url, reference, dest, GIT_NETWORK_TIMEOUT)
}

fn git_clone_within(
    url: &str,
    reference: Option<&str>,
    dest: &Path,
    timeout: Duration,
) -> Result<(), String> {
    let mut cmd = git_command();
    cmd.arg("clone").arg("--depth").arg("1");
    if let Some(reference) = reference {
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
    String::from_utf8(http_bytes(url)?).map_err(|e| e.to_string())
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

pub fn http_bytes(url: &str) -> Result<Vec<u8>, String> {
    fetch_vetted(url, &system_resolve, &private_ip)
}

/// GET `url`, refusing private targets. The host is resolved once, the
/// addresses are checked, and the connection is pinned to exactly those
/// addresses, so a second DNS answer cannot point it somewhere else.
/// Redirects are followed by hand so every hop goes through the same check.
fn fetch_vetted(url: &str, resolve: Resolve, blocked: Blocked) -> Result<Vec<u8>, String> {
    let mut current = url.to_string();
    for _ in 0..=MAX_REDIRECTS {
        let vetted = vet(&current, resolve, blocked)?;
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
            current = vetted
                .url
                .join(location)
                .map_err(|e| format!("GET {current} redirected to a bad url: {e}"))?
                .to_string();
            continue;
        }
        if !response.status().is_success() {
            return Err(format!("GET {current} returned {}", response.status()));
        }
        let bytes = response.bytes().map_err(|e| e.to_string())?;
        if bytes.len() as u64 > DOWNLOAD_MAX_BYTES {
            return Err("download is larger than 25 MiB".into());
        }
        return Ok(bytes.to_vec());
    }
    Err(format!(
        "GET {url} redirected more than {MAX_REDIRECTS} times"
    ))
}

#[cfg(test)]
fn vetted_download(url: &str) -> Result<reqwest::Url, String> {
    vet(url, &system_resolve, &private_ip).map(|vetted| vetted.url)
}

fn vet(url: &str, resolve: Resolve, blocked: Blocked) -> Result<Vetted, String> {
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
    if let Ok(ip) = bare.parse::<IpAddr>() {
        if blocked(ip) {
            return Err(PRIVATE.into());
        }
        return Ok(Vetted {
            url: parsed,
            pin: None,
        });
    }
    let domain = host.to_string();
    let name = domain.trim_end_matches('.').to_ascii_lowercase();
    if name == "localhost" || name.ends_with(".localhost") || name.ends_with(".local") {
        return Err("skill downloads cannot target a local host".into());
    }
    let port = parsed.port_or_known_default().unwrap_or(443);
    let addrs = resolve(&domain, port)?;
    if addrs.is_empty() {
        return Err("skill download host did not resolve".into());
    }
    if addrs.iter().any(|addr| blocked(addr.ip())) {
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
    use super::vetted_download;

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
                &format!("http://127.0.0.1:{port}/repo.git"),
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
        )
        .unwrap();
        assert_eq!(body, b"/index.json");
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
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
        )
        .unwrap_err();
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
