//! Git blame for reviewer suggestions.

use std::collections::HashMap;
use std::path::Path;

use crate::GitError;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BlameLine {
    pub line: u32,
    pub author: String,
    pub author_email: String,
}

pub fn blame_authors(
    repo_path: &Path,
    file_path: &str,
    start_line: u32,
    end_line: u32,
) -> Result<Vec<BlameLine>, GitError> {
    let output = std::process::Command::new("git")
        .current_dir(repo_path)
        .args([
            "blame",
            "-L",
            &format!("{start_line},{end_line}"),
            "--line-porcelain",
            file_path,
        ])
        .output()
        .map_err(|e| GitError::Other(format!("git blame failed: {e}")))?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    Ok(parse_porcelain_blame(&String::from_utf8_lossy(&output.stdout)))
}

pub fn aggregate_authors(lines: &[BlameLine]) -> HashMap<String, u32> {
    let mut counts = HashMap::new();
    for line in lines {
        *counts.entry(line.author_email.clone()).or_insert(0) += 1;
    }
    counts
}

fn parse_porcelain_blame(text: &str) -> Vec<BlameLine> {
    let mut result = Vec::new();
    let mut current_line: u32 = 0;
    let mut author = String::new();
    let mut email = String::new();

    for line in text.lines() {
        if line.starts_with('\t') {
            result.push(BlameLine {
                line: current_line,
                author: author.clone(),
                author_email: email.clone(),
            });
            continue;
        }
        if let Some(rest) = line.strip_prefix("author ") {
            author = rest.to_string();
        } else if let Some(rest) = line.strip_prefix("author-mail ") {
            email = rest.trim_matches('<').trim_matches('>').to_string();
        } else if let Some(first) = line.split_whitespace().next() {
            if first.len() >= 40 {
                if let Ok(n) = line.split_whitespace().nth(2).unwrap_or("0").parse::<u32>() {
                    current_line = n;
                }
            }
        }
    }
    result
}
