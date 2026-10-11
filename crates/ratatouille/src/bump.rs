use std::cmp::Ordering;
use std::path::Path;
use std::process::Command;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    None,
    Micro,
    Minor,
    Major,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Keep,
    AlreadyBumped,
    Bump,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Semver {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl Semver {
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim().trim_start_matches('v');
        let mut parts = text.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next()?.parse().ok()?;
        let patch = parts.next()?.parse().ok()?;
        if parts.next().is_some() {
            return None;
        }
        Some(Self {
            major,
            minor,
            patch,
        })
    }

    pub fn bump(self, level: Level) -> Self {
        match level {
            Level::Micro => Self {
                patch: self.patch + 1,
                ..self
            },
            Level::Minor => Self {
                minor: self.minor + 1,
                patch: 0,
                ..self
            },
            Level::Major => Self {
                major: self.major + 1,
                minor: 0,
                patch: 0,
            },
            Level::None => self,
        }
    }
}

impl std::fmt::Display for Semver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl PartialOrd for Semver {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Semver {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.patch).cmp(&(other.major, other.minor, other.patch))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeltaStatus {
    Added,
    Modified,
    Deleted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileDelta {
    pub status: DeltaStatus,
    pub path: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Commit {
    pub subject: String,
    pub body: String,
    pub files: Vec<FileDelta>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Versions {
    pub published: Semver,
    pub on_main: Semver,
    pub same_commit: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Decision {
    pub level: Level,
    pub kind: Kind,
    pub from: Semver,
    pub to: Semver,
}

impl Decision {
    pub fn summary(self) -> String {
        match self.kind {
            Kind::Keep => format!(
                "No product changes since v{}. Publish {} and deploy the website.",
                self.from, self.to
            ),
            Kind::AlreadyBumped => format!(
                "main is already {}, ahead of published v{}. Publish {} without a second bump.",
                self.to, self.from, self.to
            ),
            Kind::Bump => {
                let word = match self.level {
                    Level::Micro => "micro",
                    Level::Minor => "minor",
                    Level::Major => "major",
                    Level::None => "unchanged",
                };
                format!(
                    "Changes since v{} are a {word} bump: {} → {}.",
                    self.from, self.from, self.to
                )
            }
        }
    }

    pub fn publishes_current_main(self) -> bool {
        matches!(self.kind, Kind::Keep | Kind::AlreadyBumped)
    }
}

pub fn decide(versions: Versions, commits: &[Commit]) -> Decision {
    if versions.on_main > versions.published {
        return Decision {
            level: Level::None,
            kind: Kind::AlreadyBumped,
            from: versions.published,
            to: versions.on_main,
        };
    }
    if versions.same_commit || versions.on_main != versions.published {
        return Decision {
            level: Level::None,
            kind: Kind::Keep,
            from: versions.published,
            to: versions.published,
        };
    }
    let level = classify(commits);
    if level == Level::None {
        return Decision {
            level: Level::None,
            kind: Kind::Keep,
            from: versions.published,
            to: versions.published,
        };
    }
    Decision {
        level,
        kind: Kind::Bump,
        from: versions.published,
        to: versions.published.bump(level),
    }
}

fn classify(commits: &[Commit]) -> Level {
    let mut best = Level::None;
    for commit in commits {
        best = higher(best, commit_level(commit));
    }
    best
}

fn higher(left: Level, right: Level) -> Level {
    if rank(right) > rank(left) {
        right
    } else {
        left
    }
}

fn rank(level: Level) -> u8 {
    match level {
        Level::None => 0,
        Level::Micro => 1,
        Level::Minor => 2,
        Level::Major => 3,
    }
}

fn commit_level(commit: &Commit) -> Level {
    if commit.subject.starts_with("Merge ") {
        return Level::None;
    }
    let product: Vec<&FileDelta> = commit
        .files
        .iter()
        .filter(|file| product_path(&file.path))
        .collect();
    if product.is_empty() {
        return Level::None;
    }
    if is_breaking(&commit.subject, &commit.body)
        || product.iter().any(|file| removed_command(file))
        || says_removed_public_surface(&commit.subject)
    {
        return Level::Major;
    }
    if is_feat(&commit.subject) || product.iter().any(|file| added_command(file)) {
        return Level::Minor;
    }
    Level::Micro
}

fn product_path(path: &str) -> bool {
    let path = normalize(path);
    if !ships(&path) || is_test_path(&path) {
        return false;
    }
    true
}

fn ships(path: &str) -> bool {
    if path.starts_with("site/")
        || path.starts_with("docs/")
        || path.starts_with("benchmarks/")
        || path.starts_with("test-smoke/")
        || path.starts_with("crates/ratatouille/")
        || path.starts_with(".github/")
        || path.starts_with(".agents/")
        || path.starts_with(".cursor/")
        || path.starts_with(".codex/")
    {
        return false;
    }
    path.starts_with("crates/")
        || path == "Cargo.toml"
        || path == "Cargo.lock"
        || path == "install.sh"
        || path == "install.ps1"
}

fn is_test_path(path: &str) -> bool {
    path.contains("/tests/")
        || path.contains("/test/")
        || path.ends_with("_test.rs")
        || path.ends_with(".test.ts")
        || path.ends_with(".test.tsx")
        || path.ends_with(".spec.ts")
        || path.ends_with(".spec.tsx")
}

fn normalize(path: &str) -> String {
    path.replace('\\', "/").trim_start_matches("./").to_string()
}

fn is_breaking(subject: &str, body: &str) -> bool {
    let subject_lower = subject.to_lowercase();
    let body_lower = body.to_lowercase();
    if subject_lower.contains("breaking change") || body_lower.contains("breaking change") {
        return true;
    }
    conventional_breaking(subject)
}

fn conventional_breaking(subject: &str) -> bool {
    let Some(head) = subject.split(':').next() else {
        return false;
    };
    let head = head.trim();
    let bang = head.strip_suffix('!').unwrap_or(head);
    if !head.ends_with('!') {
        return false;
    }
    let name = bang.split('(').next().unwrap_or(bang).trim();
    !name.is_empty() && name.chars().all(|ch| ch.is_ascii_alphabetic())
}

fn is_feat(subject: &str) -> bool {
    if conventional_breaking(subject) {
        return false;
    }
    let Some(head) = subject.split(':').next() else {
        return false;
    };
    let name = head.split('(').next().unwrap_or(head).trim();
    name == "feat"
}

fn command_file(path: &str) -> bool {
    let path = normalize(path);
    let Some(rest) = path.strip_prefix("crates/ax-cli/src/commands/") else {
        return false;
    };
    !rest.contains('/') && rest.ends_with(".rs") && rest != "mod.rs"
}

fn removed_command(file: &FileDelta) -> bool {
    file.status == DeltaStatus::Deleted && command_file(&file.path)
}

fn added_command(file: &FileDelta) -> bool {
    file.status == DeltaStatus::Added && command_file(&file.path)
}

fn says_removed_public_surface(subject: &str) -> bool {
    let subject = subject.to_lowercase();
    let mut removes = false;
    let mut surface = false;
    for word in subject.split(|ch: char| !ch.is_ascii_alphanumeric()) {
        if word.is_empty() {
            continue;
        }
        removes |= matches!(
            word,
            "remove"
                | "removes"
                | "removed"
                | "removing"
                | "delete"
                | "deletes"
                | "deleted"
                | "deleting"
                | "drop"
                | "drops"
                | "dropped"
                | "dropping"
        );
        surface |= matches!(
            word,
            "endpoint" | "endpoints" | "command" | "commands" | "api" | "apis" | "flag" | "flags"
        ) || word.ends_with("command");
    }
    removes && surface
}

pub fn parse_log(text: &str) -> Vec<Commit> {
    text.split('\u{1e}').filter_map(parse_record).collect()
}

fn parse_record(record: &str) -> Option<Commit> {
    let record = record.trim_start_matches(['\n', '\r']);
    if record.is_empty() {
        return None;
    }
    let (head, rest) = record.split_once('\u{1f}').unwrap_or((record, ""));
    let subject = head.trim().to_string();
    if subject.is_empty() {
        return None;
    }
    let mut body = String::new();
    let mut files = Vec::new();
    let mut seen_file = false;
    for line in rest.lines() {
        if let Some(deltas) = parse_status_line(line) {
            seen_file = true;
            files.extend(deltas);
            continue;
        }
        if seen_file || line.trim().is_empty() {
            continue;
        }
        if !body.is_empty() {
            body.push('\n');
        }
        body.push_str(line);
    }
    Some(Commit {
        subject,
        body,
        files,
    })
}

fn parse_status_line(line: &str) -> Option<Vec<FileDelta>> {
    let line = line.trim_end_matches('\r');
    let mut parts = line.split('\t');
    let status = parts.next()?;
    let first = parts.next()?.to_string();
    let second = parts.next().map(str::to_string);
    if parts.next().is_some() || status.is_empty() {
        return None;
    }
    let code = status.chars().next()?;
    if !code.is_ascii_uppercase() || !status.chars().skip(1).all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    match code {
        'A' => Some(vec![delta(DeltaStatus::Added, first)]),
        'M' | 'T' => Some(vec![delta(DeltaStatus::Modified, first)]),
        'D' => Some(vec![delta(DeltaStatus::Deleted, first)]),
        'R' | 'C' => {
            let new_path = second?;
            Some(rename_deltas(&first, &new_path))
        }
        _ => None,
    }
}

fn delta(status: DeltaStatus, path: String) -> FileDelta {
    FileDelta { status, path }
}

fn rename_deltas(old: &str, new_path: &str) -> Vec<FileDelta> {
    if command_file(old) && command_file(new_path) {
        return vec![delta(DeltaStatus::Modified, new_path.to_string())];
    }
    if command_file(old) && !command_file(new_path) {
        return vec![delta(DeltaStatus::Deleted, old.to_string())];
    }
    if !command_file(old) && command_file(new_path) {
        return vec![delta(DeltaStatus::Added, new_path.to_string())];
    }
    vec![delta(DeltaStatus::Modified, new_path.to_string())]
}

pub fn version_in_manifest(toml: &str) -> Option<Semver> {
    for line in toml.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("version") else {
            continue;
        };
        let rest = rest.trim_start();
        let Some(rest) = rest.strip_prefix('=') else {
            continue;
        };
        let rest = rest.trim().trim_matches('"');
        return Semver::parse(rest);
    }
    None
}

pub fn tag_from_release_json(text: &str) -> Option<String> {
    let key = "\"tagName\"";
    let start = text.find(key)? + key.len();
    let rest = text[start..].trim_start();
    let rest = rest.strip_prefix(':')?.trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    let tag = &rest[..end];
    if tag.is_empty() {
        None
    } else {
        Some(tag.to_string())
    }
}

pub fn inspect(root: &Path) -> std::io::Result<Decision> {
    git(root, &["fetch", "origin", "main"])?;
    let release = command(root, "gh", &["release", "view", "--json", "tagName"])?;
    let tag = tag_from_release_json(&release).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "the published release has no tag",
        )
    })?;
    let published = Semver::parse(&tag).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("published tag {tag} is not x.y.z"),
        )
    })?;
    let manifest = git(root, &["show", "origin/main:crates/ax-cli/Cargo.toml"])?;
    let on_main = version_in_manifest(&manifest).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "crates/ax-cli/Cargo.toml on origin/main has no version",
        )
    })?;
    let tag_commit = git(root, &["rev-parse", &format!("{tag}^{{commit}}")])?;
    let main_commit = git(root, &["rev-parse", "origin/main"])?;
    let same_commit = tag_commit.trim() == main_commit.trim();
    let log = if same_commit {
        String::new()
    } else {
        git(
            root,
            &[
                "log",
                &format!("{tag}..origin/main"),
                "--name-status",
                "--pretty=format:%x1e%s%x1f%b",
            ],
        )?
    };
    Ok(decide(
        Versions {
            published,
            on_main,
            same_commit,
        },
        &parse_log(&log),
    ))
}

fn git(root: &Path, args: &[&str]) -> std::io::Result<String> {
    command(root, "git", args)
}

fn command(root: &Path, program: &str, args: &[&str]) -> std::io::Result<String> {
    let output = Command::new(program)
        .args(args)
        .current_dir(root)
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(std::io::Error::other(format!(
            "{program} {} failed: {}",
            args.join(" "),
            stderr.trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ver(text: &str) -> Semver {
        Semver::parse(text).expect(text)
    }

    fn versions(same: bool) -> Versions {
        Versions {
            published: ver("8.0.0"),
            on_main: ver("8.0.0"),
            same_commit: same,
        }
    }

    fn file(status: DeltaStatus, path: &str) -> FileDelta {
        FileDelta {
            status,
            path: path.to_string(),
        }
    }

    fn commit(subject: &str, body: &str, files: Vec<FileDelta>) -> Commit {
        Commit {
            subject: subject.to_string(),
            body: body.to_string(),
            files,
        }
    }

    #[test]
    fn same_commit_keeps_the_published_version() {
        let decision = decide(versions(true), &[]);
        assert_eq!(decision.kind, Kind::Keep);
        assert_eq!(decision.level, Level::None);
        assert_eq!(decision.to, ver("8.0.0"));
        assert!(decision.publishes_current_main());
        assert!(decision
            .summary()
            .contains("No product changes since v8.0.0"));
    }

    #[test]
    fn site_only_feat_does_not_bump_ax() {
        let decision = decide(
            versions(false),
            &[commit(
                "feat(site): redesign the public site",
                "",
                vec![file(DeltaStatus::Modified, "site/src/pages/index.astro")],
            )],
        );
        assert_eq!(decision.kind, Kind::Keep);
        assert_eq!(decision.level, Level::None);
    }

    #[test]
    fn a_product_fix_is_micro() {
        let decision = decide(
            versions(false),
            &[commit(
                "fix(web): refresh portal screenshots",
                "",
                vec![file(DeltaStatus::Modified, "crates/ax-web/src/lib.rs")],
            )],
        );
        assert_eq!(decision.kind, Kind::Bump);
        assert_eq!(decision.level, Level::Micro);
        assert_eq!(decision.from, ver("8.0.0"));
        assert_eq!(decision.to, ver("8.0.1"));
        assert!(!decision.publishes_current_main());
        assert_eq!(
            decision.summary(),
            "Changes since v8.0.0 are a micro bump: 8.0.0 → 8.0.1."
        );
    }

    #[test]
    fn a_freeform_fix_subject_is_still_micro() {
        let decision = decide(
            versions(false),
            &[commit(
                "Fix IDE MCP seeding when the folder is not expanded.",
                "",
                vec![file(DeltaStatus::Modified, "crates/ax-cli/src/main.rs")],
            )],
        );
        assert_eq!(decision.level, Level::Micro);
        assert_eq!(decision.to, ver("8.0.1"));
    }

    #[test]
    fn a_product_feat_is_minor() {
        let decision = decide(
            versions(false),
            &[commit(
                "feat(access): mount access management routes",
                "",
                vec![file(DeltaStatus::Modified, "crates/ax-web/src/lib.rs")],
            )],
        );
        assert_eq!(decision.level, Level::Minor);
        assert_eq!(decision.to, ver("8.1.0"));
        assert_eq!(
            decision.summary(),
            "Changes since v8.0.0 are a minor bump: 8.0.0 → 8.1.0."
        );
    }

    #[test]
    fn feat_and_fix_together_stay_minor() {
        let decision = decide(
            versions(false),
            &[
                commit(
                    "fix(access): keep revoked replicas blocked",
                    "",
                    vec![file(DeltaStatus::Modified, "crates/ax-cli/src/lib.rs")],
                ),
                commit(
                    "feat(settings): expose team clients",
                    "",
                    vec![file(DeltaStatus::Added, "crates/ax-web/src/settings.rs")],
                ),
            ],
        );
        assert_eq!(decision.level, Level::Minor);
        assert_eq!(decision.to, ver("8.1.0"));
    }

    #[test]
    fn breaking_change_is_major() {
        let decision = decide(
            versions(false),
            &[commit(
                "feat(access): replace the enrollment API",
                "BREAKING CHANGE: old clients cannot enroll",
                vec![file(DeltaStatus::Modified, "crates/ax-cli/src/lib.rs")],
            )],
        );
        assert_eq!(decision.level, Level::Major);
        assert_eq!(decision.to, ver("9.0.0"));
        assert_eq!(
            decision.summary(),
            "Changes since v8.0.0 are a major bump: 8.0.0 → 9.0.0."
        );
    }

    #[test]
    fn a_breaking_bang_beats_a_feat() {
        let decision = decide(
            versions(false),
            &[
                commit(
                    "feat(web): add a portal page",
                    "",
                    vec![file(DeltaStatus::Modified, "crates/ax-web/src/lib.rs")],
                ),
                commit(
                    "feat(cli)!: drop the old init flow",
                    "",
                    vec![file(DeltaStatus::Modified, "crates/ax-cli/src/main.rs")],
                ),
            ],
        );
        assert_eq!(decision.level, Level::Major);
        assert_eq!(decision.to, ver("9.0.0"));
    }

    #[test]
    fn removing_an_endpoint_is_major() {
        let decision = decide(
            versions(false),
            &[commit(
                "feat(access): remove manual node approval endpoint",
                "",
                vec![file(DeltaStatus::Modified, "crates/ax-web/src/lib.rs")],
            )],
        );
        assert_eq!(decision.level, Level::Major);
        assert_eq!(decision.to, ver("9.0.0"));
    }

    #[test]
    fn removing_a_capital_helper_stays_micro() {
        let decision = decide(
            versions(false),
            &[commit(
                "refactor: remove the capital helper",
                "",
                vec![file(DeltaStatus::Modified, "crates/ax-cli/src/lib.rs")],
            )],
        );
        assert_eq!(decision.level, Level::Micro);
        assert_eq!(decision.to, ver("8.0.1"));
    }

    #[test]
    fn deleting_a_command_file_is_major() {
        let decision = decide(
            versions(false),
            &[commit(
                "refactor(cli): fold ship into release",
                "",
                vec![file(
                    DeltaStatus::Deleted,
                    "crates/ax-cli/src/commands/ship.rs",
                )],
            )],
        );
        assert_eq!(decision.level, Level::Major);
    }

    #[test]
    fn adding_a_command_file_is_minor_even_without_feat() {
        let decision = decide(
            versions(false),
            &[commit(
                "chore(cli): add the share command",
                "",
                vec![file(
                    DeltaStatus::Added,
                    "crates/ax-cli/src/commands/share.rs",
                )],
            )],
        );
        assert_eq!(decision.level, Level::Minor);
        assert_eq!(decision.to, ver("8.1.0"));
    }

    #[test]
    fn renaming_a_command_file_is_not_major() {
        let log = "\u{1e}refactor(cli): rename ship\u{1f}\nR100\tcrates/ax-cli/src/commands/ship.rs\tcrates/ax-cli/src/commands/release.rs\n";
        let decision = decide(versions(false), &parse_log(log));
        assert_eq!(decision.level, Level::Micro);
        assert_eq!(decision.to, ver("8.0.1"));
    }

    #[test]
    fn main_already_ahead_does_not_bump_again() {
        let versions = Versions {
            published: ver("8.0.0"),
            on_main: ver("8.1.0"),
            same_commit: false,
        };
        let decision = decide(
            versions,
            &[commit(
                "feat(cli): add share",
                "",
                vec![file(DeltaStatus::Modified, "crates/ax-cli/src/main.rs")],
            )],
        );
        assert_eq!(decision.kind, Kind::AlreadyBumped);
        assert_eq!(decision.level, Level::None);
        assert_eq!(decision.to, ver("8.1.0"));
        assert!(decision.publishes_current_main());
        assert!(decision.summary().contains("without a second bump"));
    }

    #[test]
    fn docs_and_tests_do_not_bump() {
        let decision = decide(
            versions(false),
            &[
                commit(
                    "docs(access): mark implementation complete",
                    "",
                    vec![file(DeltaStatus::Modified, "docs/access.md")],
                ),
                commit(
                    "test(web): cover portal pages",
                    "",
                    vec![file(
                        DeltaStatus::Modified,
                        "crates/ax-web/web-ui/src/pages/Savings.test.tsx",
                    )],
                ),
            ],
        );
        assert_eq!(decision.kind, Kind::Keep);
    }

    #[test]
    fn a_lockfile_change_is_micro() {
        let decision = decide(
            versions(false),
            &[commit(
                "chore: lock access credential dependencies",
                "",
                vec![file(DeltaStatus::Modified, "Cargo.lock")],
            )],
        );
        assert_eq!(decision.level, Level::Micro);
        assert_eq!(decision.to, ver("8.0.1"));
    }

    #[test]
    fn merge_commits_are_ignored() {
        let decision = decide(
            versions(false),
            &[
                commit(
                    "Merge pull request #19 from GaryWenneker/feature/settings",
                    "",
                    vec![file(DeltaStatus::Modified, "crates/ax-cli/src/main.rs")],
                ),
                commit(
                    "fix(settings): repair replication controls markup",
                    "",
                    vec![file(DeltaStatus::Modified, "crates/ax-web/src/lib.rs")],
                ),
            ],
        );
        assert_eq!(decision.level, Level::Micro);
    }

    #[test]
    fn micro_and_minor_carry_the_right_numbers() {
        assert_eq!(ver("8.0.9").bump(Level::Micro), ver("8.0.10"));
        assert_eq!(ver("8.2.4").bump(Level::Minor), ver("8.3.0"));
        assert_eq!(ver("8.2.4").bump(Level::Major), ver("9.0.0"));
    }

    #[test]
    fn parse_log_keeps_the_breaking_body_with_its_files() {
        let log = "\u{1e}feat(cli): replace init\u{1f}Body line\n\nBREAKING CHANGE: flags moved\nM\tcrates/ax-cli/src/main.rs\nA\tcrates/ax-cli/src/commands/init.rs\n";
        let commits = parse_log(log);
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].subject, "feat(cli): replace init");
        assert!(commits[0].body.contains("BREAKING CHANGE"));
        assert_eq!(commits[0].files.len(), 2);
        let decision = decide(versions(false), &commits);
        assert_eq!(decision.level, Level::Major);
    }

    #[test]
    fn manifest_version_is_the_first_version_field() {
        let toml = "[package]\nname = \"ax-cli\"\nversion = \"8.0.0\"\nedition = \"2021\"\n";
        assert_eq!(version_in_manifest(toml), Some(ver("8.0.0")));
    }

    #[test]
    fn release_json_yields_the_tag() {
        let json = "{\n  \"tagName\": \"v8.0.0\"\n}\n";
        assert_eq!(tag_from_release_json(json).as_deref(), Some("v8.0.0"));
    }
}
