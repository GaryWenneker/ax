//! Obsidian-style `[[links]]` between rules, skills and memories. Pure: no IO.

use std::collections::{HashMap, HashSet};

use serde::Serialize;

/// One `[[...]]` occurrence in a body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WikiLink {
    /// The exact source text, e.g. `![[pr#Steps|the PR skill]]`.
    pub text: String,
    pub target: String,
    pub heading: Option<String>,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LinkKind {
    Rule,
    Skill,
    Memory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LinkOrigin {
    Project,
    Global,
}

/// Something a link can point to. `page` is its file name in the vault, without `.md`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct LinkItem {
    pub kind: LinkKind,
    pub origin: LinkOrigin,
    pub id: String,
    #[serde(skip)]
    pub page: String,
}

impl LinkItem {
    pub fn new(kind: LinkKind, origin: LinkOrigin, id: &str, page: &str) -> Self {
        Self {
            kind,
            origin,
            id: id.to_string(),
            page: page.to_string(),
        }
    }

    /// `rule/english-only`, as used in link reasons.
    pub fn label(&self) -> String {
        let kind = match self.kind {
            LinkKind::Rule => "rule",
            LinkKind::Skill => "skill",
            LinkKind::Memory => "memory",
        };
        format!("{kind}/{}", self.id)
    }
}

/// Every `[[link]]` in `body`, in order, skipping fenced code blocks and inline code.
pub fn parse_links(body: &str) -> Vec<WikiLink> {
    let mut out = Vec::new();
    let mut fence: Option<(char, usize)> = None;
    for line in body.lines() {
        if let Some(marker) = fence_marker(line) {
            fence = match fence {
                None => Some(marker),
                Some((c, n)) if marker.0 == c && marker.1 >= n => None,
                open => open,
            };
            continue;
        }
        if fence.is_none() {
            links_in_line(line, &mut out);
        }
    }
    out
}

fn fence_marker(line: &str) -> Option<(char, usize)> {
    let t = line.trim_start();
    let c = t.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let n = t.chars().take_while(|x| *x == c).count();
    (n >= 3).then_some((c, n))
}

fn links_in_line(line: &str, out: &mut Vec<WikiLink>) {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'`' {
            let run = bytes[i..].iter().take_while(|b| **b == b'`').count();
            let fence = &line[i..i + run];
            match line[i + run..].find(fence) {
                Some(end) => i += run + end + run,
                None => i += run,
            }
            continue;
        }
        if bytes[i..].starts_with(b"[[") {
            if let Some(end) = line[i + 2..].find("]]") {
                let inner = &line[i + 2..i + 2 + end];
                let start = if i > 0 && bytes[i - 1] == b'!' {
                    i - 1
                } else {
                    i
                };
                if let Some(link) = wiki_link(&line[start..i + 2 + end + 2], inner) {
                    out.push(link);
                }
                i += 2 + end + 2;
                continue;
            }
        }
        i += 1;
    }
}

fn wiki_link(text: &str, inner: &str) -> Option<WikiLink> {
    if inner.contains("[[") {
        return None;
    }
    let (dest, label) = match inner.split_once('|') {
        Some((d, l)) => (d, Some(l.trim().to_string()).filter(|l| !l.is_empty())),
        None => (inner, None),
    };
    let (target, heading) = match dest.split_once('#') {
        Some((t, h)) => (t, Some(h.trim().to_string()).filter(|h| !h.is_empty())),
        None => (dest, None),
    };
    let target = target.trim();
    (!target.is_empty()).then(|| WikiLink {
        text: text.to_string(),
        target: target.to_string(),
        heading,
        label,
    })
}

/// Resolves link targets against a fixed set of items.
#[derive(Debug, Default)]
pub struct LinkIndex {
    items: Vec<LinkItem>,
}

impl LinkIndex {
    pub fn new(items: Vec<LinkItem>) -> Self {
        Self { items }
    }

    pub fn items(&self) -> &[LinkItem] {
        &self.items
    }

    /// The item `target` points to, or `None` when it points to nothing.
    pub fn resolve(&self, target: &str) -> Option<&LinkItem> {
        let mut t = target.trim().to_lowercase();
        if t.ends_with(".md") {
            t.truncate(t.len() - 3);
        }
        if t.is_empty() {
            return None;
        }
        for (prefix, kind, origin) in FOLDERS {
            if let Some(page) = t.strip_prefix(prefix) {
                return self.items.iter().find(|i| {
                    i.kind == kind && i.origin == origin && i.page.to_lowercase() == page
                });
            }
        }
        if t.contains('/') {
            return None;
        }
        self.items
            .iter()
            .filter(|i| i.page.to_lowercase() == t)
            .min_by_key(|i| bare_rank(i))
    }
}

impl LinkIndex {
    /// The text to put inside `[[…]]` so it resolves to `item`: the bare page when that
    /// is unambiguous, else the folder path. Memories always use `memories/<stem>`.
    pub fn link_target(&self, item: &LinkItem) -> String {
        if item.kind != LinkKind::Memory && self.resolve(&item.page) == Some(item) {
            return item.page.clone();
        }
        let prefix = FOLDERS
            .iter()
            .find(|(_, kind, origin)| *kind == item.kind && *origin == item.origin)
            .map_or("", |(p, _, _)| p);
        format!("{prefix}{}", item.page)
    }
}

/// Longest prefix first, so `global/rules/` is not read as a project folder.
const FOLDERS: [(&str, LinkKind, LinkOrigin); 5] = [
    ("global/rules/", LinkKind::Rule, LinkOrigin::Global),
    ("global/skills/", LinkKind::Skill, LinkOrigin::Global),
    ("rules/", LinkKind::Rule, LinkOrigin::Project),
    ("skills/", LinkKind::Skill, LinkOrigin::Project),
    ("memories/", LinkKind::Memory, LinkOrigin::Project),
];

/// A bare name resolves to a project rule, then a skill (the global one wins, as in
/// preflight), then a memory, then a global rule.
fn bare_rank(item: &LinkItem) -> u8 {
    match (item.kind, item.origin) {
        (LinkKind::Rule, LinkOrigin::Project) => 0,
        (LinkKind::Skill, LinkOrigin::Global) => 1,
        (LinkKind::Skill, LinkOrigin::Project) => 2,
        (LinkKind::Memory, _) => 3,
        (LinkKind::Rule, LinkOrigin::Global) => 4,
    }
}

/// A body whose links are followed, and the item it belongs to.
pub struct LinkSource<'a> {
    pub item: LinkItem,
    pub body: &'a str,
}

/// One hop: the items `sources` link to, as `(target, linked from)`, in source order and
/// then link order. Skips targets in `delivered` or rejected by `eligible`; stops at `cap`.
pub fn follow_links(
    sources: &[LinkSource<'_>],
    index: &LinkIndex,
    delivered: &HashSet<(LinkKind, String)>,
    eligible: impl Fn(&LinkItem) -> bool,
    cap: usize,
) -> Vec<(LinkItem, LinkItem)> {
    let mut seen = delivered.clone();
    let mut out = Vec::new();
    for source in sources {
        for link in parse_links(source.body) {
            if out.len() >= cap {
                return out;
            }
            let Some(target) = index.resolve(&link.target) else {
                continue;
            };
            if !eligible(target) || !seen.insert((target.kind, target.id.clone())) {
                continue;
            }
            out.push((target.clone(), source.item.clone()));
        }
    }
    out
}

/// A title as a file stem Obsidian accepts.
pub fn safe_stem(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '#' | '^' | '[' | ']' => '-',
            c if c.is_control() => '-',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim().trim_start_matches('.').trim();
    if trimmed.is_empty() {
        "untitled".into()
    } else {
        trimmed.chars().take(120).collect()
    }
}

/// File stem per `(id, title, created_at)`. Clashing titles get ` (2)`, ` (3)` in creation order.
pub fn unique_stems(entries: &[(&str, &str, i64)]) -> Vec<(String, String)> {
    let mut sorted: Vec<&(&str, &str, i64)> = entries.iter().collect();
    sorted.sort_by(|a, b| a.2.cmp(&b.2).then(a.0.cmp(b.0)));
    let mut used: HashMap<String, usize> = HashMap::new();
    let mut out = Vec::with_capacity(sorted.len());
    for (id, title, _) in sorted {
        let base = safe_stem(title);
        let mut stem = base.clone();
        let mut n = 1;
        while used.contains_key(&stem.to_lowercase()) {
            n += 1;
            stem = format!("{base} ({n})");
        }
        used.insert(stem.to_lowercase(), 1);
        out.push((id.to_string(), stem));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn w7_link_target_resolves_back_to_the_item() {
        let items = vec![
            LinkItem::new(LinkKind::Rule, LinkOrigin::Project, "review", "review"),
            LinkItem::new(LinkKind::Rule, LinkOrigin::Global, "review", "review"),
            LinkItem::new(
                LinkKind::Rule,
                LinkOrigin::Global,
                "only-global",
                "only-global",
            ),
            LinkItem::new(LinkKind::Skill, LinkOrigin::Project, "pr", "pr"),
            LinkItem::new(LinkKind::Memory, LinkOrigin::Project, "m1", "Use SQLite"),
        ];
        let index = LinkIndex::new(items.clone());
        let want = [
            "review",
            "global/rules/review",
            "only-global",
            "pr",
            "memories/Use SQLite",
        ];
        for (item, want) in items.iter().zip(want) {
            let target = index.link_target(item);
            assert_eq!(target, want);
            assert_eq!(index.resolve(&target), Some(item), "{target}");
        }
    }

    fn link(text: &str, target: &str, heading: Option<&str>, label: Option<&str>) -> WikiLink {
        WikiLink {
            text: text.into(),
            target: target.into(),
            heading: heading.map(Into::into),
            label: label.map(Into::into),
        }
    }

    #[test]
    fn l1_parses_every_form() {
        let body = "See [[pre-pr-check]], [[skills/pr|the PR skill]], [[pr#Steps]], \
                    [[pr#Steps|steps]] and ![[english-only]]. Not [[]] or [[ ]].";
        assert_eq!(
            parse_links(body),
            vec![
                link("[[pre-pr-check]]", "pre-pr-check", None, None),
                link(
                    "[[skills/pr|the PR skill]]",
                    "skills/pr",
                    None,
                    Some("the PR skill")
                ),
                link("[[pr#Steps]]", "pr", Some("Steps"), None),
                link("[[pr#Steps|steps]]", "pr", Some("Steps"), Some("steps")),
                link("![[english-only]]", "english-only", None, None),
            ]
        );
    }

    #[test]
    fn l1_link_does_not_span_lines() {
        assert!(parse_links("[[a\nb]]").is_empty());
        assert_eq!(parse_links("x [[a]] y [[b]]").len(), 2);
    }

    #[test]
    fn l2_code_is_not_a_link() {
        let body = "Use [[real]].\n\n```md\n[[in-fence]]\n```\n\n~~~\n[[tilde-fence]]\n~~~\n\
                    Inline `[[in-code]]` and ``[[double]]`` but [[after]].\n";
        let targets: Vec<String> = parse_links(body).into_iter().map(|l| l.target).collect();
        assert_eq!(targets, vec!["real", "after"]);
    }

    #[test]
    fn l1_non_ascii_text_around_links() {
        let targets: Vec<String> = parse_links("Café ☕ [[naïve-rule]] — `é` [[Ünïcode note]]")
            .into_iter()
            .map(|l| l.target)
            .collect();
        assert_eq!(targets, vec!["naïve-rule", "Ünïcode note"]);
    }

    #[test]
    fn l1_never_panics_on_any_short_input() {
        let alphabet = ['[', ']', '`', '!', '|', '#', 'é', 'a', '\n', '~'];
        let mut stack = vec![String::new()];
        while let Some(s) = stack.pop() {
            for link in parse_links(&s) {
                assert!(s.contains(&link.text), "{s:?} -> {link:?}");
                assert!(!link.target.is_empty());
            }
            if s.chars().count() < 5 {
                for c in alphabet {
                    stack.push(format!("{s}{c}"));
                }
            }
        }
    }

    #[test]
    fn l2_unclosed_backtick_is_literal() {
        let targets: Vec<String> = parse_links("a ` b [[x]]")
            .into_iter()
            .map(|l| l.target)
            .collect();
        assert_eq!(targets, vec!["x"]);
    }

    fn index() -> LinkIndex {
        use LinkKind::*;
        use LinkOrigin::*;
        LinkIndex::new(vec![
            LinkItem::new(Rule, Project, "pr", "pr"),
            LinkItem::new(Skill, Project, "pr", "pr"),
            LinkItem::new(Skill, Project, "pre-pr-check", "pre-pr-check"),
            LinkItem::new(Skill, Project, "review-loop", "review-loop"),
            LinkItem::new(Skill, Global, "review-loop", "review-loop"),
            LinkItem::new(Memory, Project, "m1", "Use SQLite"),
            LinkItem::new(Rule, Global, "english-comments", "english-comments"),
            LinkItem::new(Memory, Project, "m2", "english-comments"),
        ])
    }

    fn resolved(target: &str) -> Option<(LinkKind, LinkOrigin, String)> {
        index()
            .resolve(target)
            .map(|i| (i.kind, i.origin, i.id.clone()))
    }

    #[test]
    fn l3_path_links_stay_in_their_folder() {
        use LinkKind::*;
        use LinkOrigin::*;
        assert_eq!(resolved("skills/pr"), Some((Skill, Project, "pr".into())));
        assert_eq!(resolved("rules/pr"), Some((Rule, Project, "pr".into())));
        assert_eq!(resolved("rules/pre-pr-check"), None);
        assert_eq!(
            resolved("memories/Use SQLite"),
            Some((Memory, Project, "m1".into()))
        );
        assert_eq!(
            resolved("global/skills/review-loop"),
            Some((Skill, Global, "review-loop".into()))
        );
        assert_eq!(
            resolved("skills/review-loop"),
            Some((Skill, Project, "review-loop".into()))
        );
        assert_eq!(
            resolved("global/rules/english-comments"),
            Some((Rule, Global, "english-comments".into()))
        );
        assert_eq!(resolved("global/rules/pr"), None);
        assert_eq!(resolved("elsewhere/pr"), None);
    }

    #[test]
    fn l4_bare_name_order() {
        use LinkKind::*;
        use LinkOrigin::*;
        assert_eq!(resolved("pr"), Some((Rule, Project, "pr".into())));
        assert_eq!(
            resolved("review-loop"),
            Some((Skill, Global, "review-loop".into()))
        );
        assert_eq!(
            resolved("english-comments"),
            Some((Memory, Project, "m2".into()))
        );
        assert_eq!(resolved("Use SQLite"), Some((Memory, Project, "m1".into())));
    }

    #[test]
    fn l5_case_and_md_suffix_are_ignored() {
        use LinkKind::*;
        use LinkOrigin::*;
        assert_eq!(
            resolved("Pre-PR-Check.md"),
            Some((Skill, Project, "pre-pr-check".into()))
        );
        assert_eq!(
            resolved("SKILLS/PR.MD"),
            Some((Skill, Project, "pr".into()))
        );
        assert_eq!(
            resolved("  pre-pr-check  "),
            Some((Skill, Project, "pre-pr-check".into()))
        );
    }

    #[test]
    fn l6_missing_target_is_none() {
        assert_eq!(resolved("nothing-here"), None);
        assert_eq!(resolved(""), None);
    }

    #[test]
    fn l7_page_names_are_safe_and_unique() {
        let stems: HashMap<_, _> =
            unique_stems(&[("b", "a/b: c?", 2), ("a", "a/b: c?", 1), ("c", "", 3)])
                .into_iter()
                .collect();
        assert_eq!(stems["a"], "a-b- c-");
        assert_eq!(stems["b"], "a-b- c- (2)");
        assert_eq!(stems["c"], "untitled");
    }

    fn src(kind: LinkKind, id: &str, body: &'static str) -> LinkSource<'static> {
        LinkSource {
            item: LinkItem::new(kind, LinkOrigin::Project, id, id),
            body,
        }
    }

    fn ids(found: &[(LinkItem, LinkItem)]) -> Vec<String> {
        found
            .iter()
            .map(|(t, from)| format!("{}<{}", t.label(), from.label()))
            .collect()
    }

    #[test]
    fn follow_one_hop_in_source_then_link_order() {
        let sources = [
            src(
                LinkKind::Rule,
                "a",
                "[[skills/pre-pr-check]] then [[Use SQLite]]",
            ),
            src(LinkKind::Skill, "b", "[[rules/pr]]"),
        ];
        let found = follow_links(&sources, &index(), &HashSet::new(), |_| true, 5);
        assert_eq!(
            ids(&found),
            vec![
                "skill/pre-pr-check<rule/a",
                "memory/m1<rule/a",
                "rule/pr<skill/b"
            ]
        );
    }

    #[test]
    fn follow_skips_delivered_duplicates_and_ineligible() {
        let sources = [
            src(
                LinkKind::Rule,
                "a",
                "[[skills/pr]] [[skills/pr]] [[pre-pr-check]] [[rules/pr]] [[missing]]",
            ),
            src(LinkKind::Rule, "b", "[[pre-pr-check]]"),
        ];
        let delivered: HashSet<_> = [(LinkKind::Skill, "pr".to_string())].into_iter().collect();
        let found = follow_links(
            &sources,
            &index(),
            &delivered,
            |i| i.kind != LinkKind::Rule,
            5,
        );
        assert_eq!(ids(&found), vec!["skill/pre-pr-check<rule/a"]);
    }

    #[test]
    fn follow_stops_at_cap() {
        let sources = [src(
            LinkKind::Rule,
            "a",
            "[[skills/pr]] [[pre-pr-check]] [[Use SQLite]] [[rules/pr]] [[global/skills/review-loop]] [[skills/review-loop]] [[english-comments]]",
        )];
        let found = follow_links(&sources, &index(), &HashSet::new(), |_| true, 5);
        assert_eq!(found.len(), 5);
        assert_eq!(found[4].0.label(), "skill/review-loop");
        assert_eq!(found[4].0.origin, LinkOrigin::Global);
    }
}
