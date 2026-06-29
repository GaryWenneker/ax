//! Comment stripping for framework regex passes.

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
    let re = regex::Regex::new(r"//.*?$|/\*[\s\S]*?\*/").unwrap();
    re.replace_all(source, "").to_string()
}

fn strip_py_comments(source: &str) -> String {
    let re = regex::Regex::new(r"#.*?$").unwrap();
    re.replace_all(source, "").to_string()
}

fn strip_php_comments(source: &str) -> String {
    let block = regex::Regex::new(r"/\*[\s\S]*?\*/").unwrap();
    let line = regex::Regex::new(r"//.*?$|#.*?$").unwrap();
    let s = block.replace_all(source, "");
    line.replace_all(&s, "").to_string()
}


fn strip_java_comments(source: &str) -> String {
    let block = regex::Regex::new(r"/\*[\s\S]*?\*/").unwrap();
    let line = regex::Regex::new(r"//.*?$").unwrap();
    let s = block.replace_all(source, "");
    line.replace_all(&s, "").to_string()
}
fn strip_js_comments(source: &str) -> String {
    let block = regex::Regex::new(r"/\*[\s\S]*?\*/").unwrap();
    let line = regex::Regex::new(r"//.*?$").unwrap();
    let s = block.replace_all(source, "");
    line.replace_all(&s, "").to_string()
}