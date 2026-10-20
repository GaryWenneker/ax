pub fn install_report_url(enabled: bool, source: &str, skill: &str) -> Option<String> {
    if !enabled || skill.is_empty() {
        return None;
    }
    let mut url = String::from(crate::TELEMETRY_URL);
    url.push_str("?event=install&agents=ax&source=");
    url.push_str(&encode(source));
    url.push_str("&skills=");
    url.push_str(&encode(skill));
    Some(url)
}

fn encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_by_default_and_on_when_asked() {
        assert!(install_report_url(false, "acme/skills", "pdf").is_none());
        let url = install_report_url(true, "acme/skills", "pdf").unwrap();
        assert!(url.starts_with("https://www.skills.sh/tele/t?"));
        assert!(url.contains("skills=pdf"));
        assert!(url.contains("source=acme%2Fskills"));
        assert!(!url.contains("SKILL.md"));
    }
}
