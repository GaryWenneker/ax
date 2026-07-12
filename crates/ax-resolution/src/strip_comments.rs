//! Comment stripping for framework regex passes.

use std::sync::OnceLock;

use regex::Regex;

fn re(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("valid static regex"))
}

pub fn strip_comments(source: &str, language: ax_types::Language) -> String {
    strip_comments_for_regex(source, language)
}

pub fn strip_comments_for_regex(source: &str, language: ax_types::Language) -> String {
    match language {
        ax_types::Language::Go | ax_types::Language::Rust => strip_rust_comments(source),
        ax_types::Language::Python => strip_py_comments(source),
        ax_types::Language::Typescript | ax_types::Language::Javascript => strip_js_comments(source),
        ax_types::Language::Php => strip_php_comments(source),
        ax_types::Language::Java | ax_types::Language::Kotlin => strip_java_comments(source),
        _ => source.to_string(),
    }
}

fn strip_rust_comments(source: &str) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    re(&RE, r"//.*?$|/\*[\s\S]*?\*/").replace_all(source, "").to_string()
}

fn strip_py_comments(source: &str) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    re(&RE, r"#.*?$").replace_all(source, "").to_string()
}

fn strip_php_comments(source: &str) -> String {
    static BLOCK: OnceLock<Regex> = OnceLock::new();
    static LINE: OnceLock<Regex> = OnceLock::new();
    let s = re(&BLOCK, r"/\*[\s\S]*?\*/").replace_all(source, "");
    re(&LINE, r"//.*?$|#.*?$").replace_all(&s, "").to_string()
}

fn strip_java_comments(source: &str) -> String {
    static BLOCK: OnceLock<Regex> = OnceLock::new();
    static LINE: OnceLock<Regex> = OnceLock::new();
    let s = re(&BLOCK, r"/\*[\s\S]*?\*/").replace_all(source, "");
    re(&LINE, r"//.*?$").replace_all(&s, "").to_string()
}

fn strip_js_comments(source: &str) -> String {
    static BLOCK: OnceLock<Regex> = OnceLock::new();
    static LINE: OnceLock<Regex> = OnceLock::new();
    let s = re(&BLOCK, r"/\*[\s\S]*?\*/").replace_all(source, "");
    re(&LINE, r"//.*?$").replace_all(&s, "").to_string()
}
