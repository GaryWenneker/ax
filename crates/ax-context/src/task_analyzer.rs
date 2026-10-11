//! Deterministic analysis of an agent's prompt. No model, no filesystem reads.
use std::sync::OnceLock;
use regex::Regex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineRequest {
    pub path: String,
    pub line: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TaskAnalysis {
    pub line_requests: Vec<LineRequest>,
    pub terms: Vec<String>,
    pub is_code_related: bool,
}

fn location_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(
        r"(?i)(?P<path>[a-z0-9_./\\-]+\.(?:json|jsonc|toml|csproj|rs|ts|tsx|cs|js|jsx|py|go|java|xml|yaml|yml|md))(?::(?P<colon>[0-9]{1,7})|\s+(?:(?:op|at|on)\s+)?(?:regel|line)\s+(?P<word>[0-9]{1,7}))"
    ).expect("valid static location regex"))
}

/// Only project-relative source paths may be resolved from the current index.
pub fn safe_relative_path(path: &str) -> Option<String> {
    let normalized = path.replace('\\', "/");
    let normalized = normalized.strip_prefix("./").unwrap_or(&normalized);
    if normalized.is_empty() || normalized.starts_with('/') || normalized.contains(':')
        || normalized.split('/').any(|p| p.is_empty() || p == "." || p == "..")
    {
        return None;
    }
    Some(normalized.to_string())
}

pub fn analyze(prompt: &str) -> TaskAnalysis {
    let mut analysis = TaskAnalysis::default();
    for hit in location_pattern().captures_iter(prompt) {
        let Some(path) = hit.name("path").and_then(|m| safe_relative_path(m.as_str())) else { continue };
        let number = hit.name("colon").or_else(|| hit.name("word"))
            .and_then(|m| m.as_str().parse::<usize>().ok()).unwrap_or(0);
        if number > 0 && number <= 1_000_000
            && !analysis.line_requests.iter().any(|r| r.path == path && r.line == number)
        {
            analysis.line_requests.push(LineRequest { path, line: number });
        }
    }
    let keywords = [
        "code", "source", "broncode", "functie", "function", "class", "klasse", "method",
        "methode", "interface", "trait", "import", "dependency", "afhankelijkheid",
        "compile", "typescript", "rust", "csharp", "c#", ".net", "config",
        "token", "auth", "login", "inloggen", "api", "test", "bug", "build",
        "script", "component", "route", "call", "aanroep", "refactor",
    ];
    let lower = prompt.to_lowercase();
    analysis.is_code_related = !analysis.line_requests.is_empty()
        || keywords.iter().any(|k| lower.split(|c: char| !c.is_alphanumeric() && c != '#' && c != '.')
            .any(|part| part == *k))
        || prompt.contains("::")
        || prompt.contains(".ts") || prompt.contains(".rs") || prompt.contains(".cs");
    let stop = ["wat", "waar", "welke", "waarom", "wordt", "zijn", "een", "het", "de",
        "dat", "die", "dan", "uit", "met", "voor", "van", "hoe", "kan", "als", "ik",
        "and", "the", "this", "that", "what", "how", "does", "with", "when",
        "from", "are", "you", "there", "in", "on", "for", "line", "regel", "staat"];
    for raw in prompt.split(|c: char| !c.is_alphanumeric() && c != '_') {
        if raw.len() < 3 { continue; }
        let term = raw.to_lowercase();
        if stop.contains(&term.as_str()) || term.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        if !analysis.terms.contains(&term) {
            analysis.terms.push(term);
        }
    }
    analysis.terms.truncate(12);
    analysis
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dutch_manifest_line() {
        let result = analyze("Wat staat er in package.json op regel 309?");
        assert_eq!(result.line_requests, vec![LineRequest { path: "package.json".into(), line: 309 }]);
        assert!(result.is_code_related);
    }
    #[test]
    fn windows_path_and_colon() {
        let result = analyze(r"Check src\Auth\JwtValidator.cs:42");
        assert_eq!(result.line_requests[0].path, "src/Auth/JwtValidator.cs");
        assert_eq!(result.line_requests[0].line, 42);
    }
    #[test]
    fn no_cross_project_or_traversal() {
        for path in ["../other/secret.json", "/etc/passwd", "C:/secrets.json", "a//b.json"] {
            assert!(safe_relative_path(path).is_none(), "{path}");
        }
    }
    #[test]
    fn irrelevant_prompt_does_not_trigger_retrieval() {
        assert!(!analyze("Wat is het weer morgen?").is_code_related);
    }
}
