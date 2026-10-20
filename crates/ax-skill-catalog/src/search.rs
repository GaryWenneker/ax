#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SearchHit {
    pub id: String,
    pub name: String,
    pub source: String,
    pub installs: u64,
    pub install_url: Option<String>,
    /// What the skill does, from the public skill page.
    pub summary: Option<String>,
    /// GitHub stars for the skill's repository, from the public skill page.
    pub stars: Option<u64>,
    /// Compact star count, such as `35.7K`.
    pub stars_label: Option<String>,
    /// Date the directory first listed the skill, such as `Jan 26, 2026`.
    pub first_seen: Option<String>,
    /// Security audits published on the skill page.
    pub audits: Vec<SkillAudit>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SkillAudit {
    pub name: String,
    pub status: String,
    pub url: String,
}

pub fn parse_search_body(body: &str) -> Result<Vec<SearchHit>, String> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|e| format!("search response is not JSON: {e}"))?;
    let rows = value
        .get("skills")
        .or_else(|| value.get("data"))
        .and_then(|v| v.as_array())
        .ok_or_else(|| "search response has no skills array".to_string())?;
    let mut hits = Vec::new();
    for row in rows {
        let name = row
            .get("slug")
            .and_then(|v| v.as_str())
            .or_else(|| row.get("name").and_then(|v| v.as_str()))
            .or_else(|| {
                row.get("id")
                    .and_then(|v| v.as_str())
                    .and_then(|id| id.rsplit('/').next())
            })
            .unwrap_or("")
            .trim()
            .to_string();
        if name.is_empty() {
            continue;
        }
        let source = row
            .get("source")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let installs = row.get("installs").and_then(|v| v.as_u64()).unwrap_or(0);
        let install_url = row
            .get("installUrl")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let id = row
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let summary = text_field(row, &["summary", "description"]);
        let stars = row.get("stars").and_then(|v| v.as_u64());
        let stars_label = text_field(row, &["starsLabel", "stars_label"])
            .or_else(|| stars.map(format_star_count));
        hits.push(SearchHit {
            id: if id.is_empty() { name.clone() } else { id },
            name,
            source,
            installs,
            install_url,
            summary,
            stars,
            stars_label,
            first_seen: text_field(row, &["firstSeen", "first_seen"]),
            audits: Vec::new(),
        });
    }
    Ok(hits)
}

fn text_field(row: &serde_json::Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        row.get(*key)
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string)
    })
}

/// `owner/repo/skill` ids that are safe to request from skills.sh.
pub fn catalog_card_url(id: &str) -> Option<String> {
    let mut parts = id.split('/');
    let owner = parts.next()?;
    let repo = parts.next()?;
    let skill = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    if [owner, repo, skill]
        .iter()
        .any(|part| !safe_path_part(part))
    {
        return None;
    }
    Some(format!("https://www.skills.sh/{owner}/{repo}/{skill}"))
}

pub fn apply_skill_card(hit: &mut SearchHit, html: &str) {
    let card = parse_skill_card(html);
    if hit.summary.is_none() {
        hit.summary = card.summary;
    }
    if hit.stars.is_none() {
        hit.stars = card.stars;
    }
    if hit.stars_label.is_none() {
        hit.stars_label = card
            .stars_label
            .or_else(|| hit.stars.map(format_star_count));
    }
    if hit.first_seen.is_none() {
        hit.first_seen = card.first_seen;
    }
    if hit.audits.is_empty() {
        hit.audits = card.audits;
    }
}

pub struct SkillCard {
    pub summary: Option<String>,
    pub stars: Option<u64>,
    pub stars_label: Option<String>,
    pub first_seen: Option<String>,
    pub audits: Vec<SkillAudit>,
}

pub fn parse_skill_card(html: &str) -> SkillCard {
    let (stars, stars_label) = github_stars(html);
    SkillCard {
        summary: json_ld_summary(html),
        stars,
        stars_label,
        first_seen: field_after_label(html, "First Seen"),
        audits: security_audits(html),
    }
}

/// Compact display for a star count: `999`, `35.7K`, `1.8M`.
pub fn format_star_count(count: u64) -> String {
    if count >= 1_000_000 {
        compact_count(count, 1_000_000, 'M')
    } else if count >= 1_000 {
        compact_count(count, 1_000, 'K')
    } else {
        count.to_string()
    }
}

fn compact_count(count: u64, unit: u64, suffix: char) -> String {
    let scaled = (count as f64 / unit as f64 * 10.0).round() / 10.0;
    if (scaled - scaled.floor()).abs() < 0.05 {
        format!("{}{suffix}", scaled as u64)
    } else {
        format!("{scaled:.1}{suffix}")
    }
}

fn json_ld_summary(html: &str) -> Option<String> {
    let marker = "<script type=\"application/ld+json\">";
    let mut rest = html;
    while let Some(start) = rest.find(marker) {
        rest = &rest[start + marker.len()..];
        let end = rest.find("</script>")?;
        let raw = &rest[..end];
        rest = &rest[end..];
        let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) else {
            continue;
        };
        if value.get("@type").and_then(|kind| kind.as_str()) != Some("SoftwareApplication") {
            continue;
        }
        let summary = value
            .get("description")
            .and_then(|text| text.as_str())
            .map(str::trim)
            .filter(|text| !text.is_empty())?;
        return Some(truncate_summary(summary));
    }
    None
}

fn github_stars(html: &str) -> (Option<u64>, Option<String>) {
    let Some(idx) = html.find("GitHub Stars") else {
        return (None, None);
    };
    let mut rest = &html[idx..];
    for _ in 0..8 {
        let Some(start) = rest.find("<span>") else {
            break;
        };
        rest = &rest[start + "<span>".len()..];
        let Some(end) = rest.find("</span>") else {
            break;
        };
        let inner = rest[..end].trim();
        rest = &rest[end..];
        if inner.contains('<') {
            continue;
        }
        if let Some(stars) = parse_compact_count(inner) {
            return (Some(stars), Some(inner.to_string()));
        }
    }
    (None, None)
}

fn field_after_label(html: &str, label: &str) -> Option<String> {
    let idx = html.find(label)?;
    let rest = &html[idx + label.len()..];
    let marker = "text-foreground\">";
    let start = rest.find(marker)?;
    let rest = &rest[start + marker.len()..];
    let end = rest.find('<')?;
    let value = rest[..end].trim();
    if value.is_empty() {
        None
    } else {
        Some(decode_basic_entities(value))
    }
}

fn security_audits(html: &str) -> Vec<SkillAudit> {
    let Some(idx) = html.find("Security Audits") else {
        return Vec::new();
    };
    let window = &html[idx..];
    let end = window.find("</main>").unwrap_or(window.len().min(12_000));
    let mut rest = &window[..end];
    let mut audits = Vec::new();
    while let Some(start) = rest.find("<a ") {
        rest = &rest[start..];
        let Some(close) = rest.find("</a>") else {
            break;
        };
        let block = &rest[..close];
        rest = &rest[close + 4..];
        let Some(href) = attr_value(block, "href") else {
            continue;
        };
        let spans = span_texts(block);
        if spans.len() < 2 {
            continue;
        }
        let status = spans[1].to_ascii_lowercase();
        if !matches!(status.as_str(), "pass" | "warn" | "fail") {
            continue;
        }
        let Some(url) = audit_url(&href) else {
            continue;
        };
        audits.push(SkillAudit {
            name: decode_basic_entities(&spans[0]),
            status,
            url,
        });
    }
    audits
}

fn span_texts(block: &str) -> Vec<String> {
    let mut rest = block;
    let mut texts = Vec::new();
    while let Some(start) = rest.find("<span") {
        rest = &rest[start..];
        let Some(open) = rest.find('>') else {
            break;
        };
        rest = &rest[open + 1..];
        let Some(end) = rest.find("</span>") else {
            break;
        };
        let inner = rest[..end].trim();
        rest = &rest[end + 7..];
        if inner.is_empty() || inner.contains('<') {
            continue;
        }
        texts.push(inner.to_string());
    }
    texts
}

fn attr_value(tag: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let start = tag.find(&needle)?;
    let rest = &tag[start + needle.len()..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn audit_url(href: &str) -> Option<String> {
    if href.starts_with("https://www.skills.sh/") || href.starts_with("https://skills.sh/") {
        return Some(href.to_string());
    }
    if !href.starts_with('/') || href.contains("..") || href.contains('\\') {
        return None;
    }
    Some(format!("https://www.skills.sh{href}"))
}

fn decode_basic_entities(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
}

fn parse_compact_count(raw: &str) -> Option<u64> {
    let raw = raw.trim().replace(',', "");
    if raw.is_empty() {
        return None;
    }
    let (number, multiplier) = if let Some(number) = raw.strip_suffix(['K', 'k']) {
        (number, 1_000.0)
    } else if let Some(number) = raw.strip_suffix(['M', 'm']) {
        (number, 1_000_000.0)
    } else {
        (raw.as_str(), 1.0)
    };
    let value: f64 = number.parse().ok()?;
    if !value.is_finite() || value < 0.0 {
        return None;
    }
    Some((value * multiplier).round() as u64)
}

fn truncate_summary(summary: &str) -> String {
    const MAX: usize = 420;
    if summary.chars().count() <= MAX {
        return summary.to_string();
    }
    let mut end = summary
        .char_indices()
        .nth(MAX)
        .map(|(i, _)| i)
        .unwrap_or(summary.len());
    if let Some(space) = summary[..end].rfind(' ') {
        if space > 240 {
            end = space;
        }
    }
    format!("{}…", summary[..end].trim_end())
}

fn safe_path_part(part: &str) -> bool {
    !part.is_empty()
        && part != "."
        && part != ".."
        && !part.contains("..")
        && part
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_public_and_v1_shapes() {
        let public_body = r#"{"skills":[{"id":"vercel-labs/agent-skills/web","name":"web","installs":3,"source":"vercel-labs/agent-skills"}]}"#;
        let hits = parse_search_body(public_body).unwrap();
        assert_eq!(hits[0].id, "vercel-labs/agent-skills/web");
        assert_eq!(hits[0].name, "web");
        assert_eq!(hits[0].installs, 3);
        let v1 = r#"{"data":[{"slug":"react-native","name":"React Native","source":"expo/skills","installs":9,"installUrl":"https://github.com/expo/skills"}]}"#;
        let hits = parse_search_body(v1).unwrap();
        assert_eq!(hits[0].name, "react-native");
        assert_eq!(
            hits[0].install_url.as_deref(),
            Some("https://github.com/expo/skills")
        );
    }

    #[test]
    fn rejects_a_shape_change() {
        assert!(parse_search_body(r#"{"ok":true}"#).is_err());
    }

    #[test]
    fn reads_the_summary_and_star_count_from_a_skill_page() {
        let html = r#"
            <script type="application/ld+json">{"@type":"SoftwareApplication","name":"pdf","description":"Read and merge PDF files."}</script>
            <span>GitHub Stars</span><svg><path d="m8 .43"/></svg><span>182.2K</span>
        "#;
        let card = parse_skill_card(html);
        assert_eq!(card.summary.as_deref(), Some("Read and merge PDF files."));
        assert_eq!(card.stars, Some(182_200));
        assert_eq!(card.stars_label.as_deref(), Some("182.2K"));
        assert_eq!(format_star_count(35_700), "35.7K");
        assert_eq!(format_star_count(999), "999");
        assert_eq!(format_star_count(1_000), "1K");
    }

    #[test]
    fn reads_first_seen_and_security_audits() {
        let html = r#"
            <span>First Seen</span></div><div class="text-sm font-mono text-foreground">Jan 26, 2026</div>
            <div>Security Audits</div>
            <a href="/vercel-labs/skills/find-skills/security/agent-trust-hub"><span>Gen Agent Trust Hub</span><span>Pass</span></a>
            <a href="/vercel-labs/skills/find-skills/security/snyk"><span>Snyk</span><span>Warn</span></a>
            <a href="https://evil.example/audit"><span>Evil</span><span>Fail</span></a>
        "#;
        let card = parse_skill_card(html);
        assert_eq!(card.first_seen.as_deref(), Some("Jan 26, 2026"));
        assert_eq!(card.audits.len(), 2);
        assert_eq!(card.audits[0].name, "Gen Agent Trust Hub");
        assert_eq!(card.audits[0].status, "pass");
        assert_eq!(
            card.audits[0].url,
            "https://www.skills.sh/vercel-labs/skills/find-skills/security/agent-trust-hub"
        );
        assert_eq!(card.audits[1].status, "warn");
    }

    #[test]
    fn ignores_a_page_that_has_no_card() {
        let card = parse_skill_card("<span>1,234</span>");
        assert_eq!(card.summary, None);
        assert_eq!(card.stars, None);
        assert!(card.audits.is_empty());
        assert_eq!(
            catalog_card_url("anthropics/skills/pdf").as_deref(),
            Some("https://www.skills.sh/anthropics/skills/pdf")
        );
        assert_eq!(catalog_card_url("https://evil.example/x"), None);
        assert_eq!(catalog_card_url("../secrets/pdf"), None);
    }
}
