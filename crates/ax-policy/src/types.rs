use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PolicyLevel {
    Info,
    Warning,
    Critical,
}

impl PolicyLevel {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "INFO" => Some(Self::Info),
            "WARNING" => Some(Self::Warning),
            "CRITICAL" => Some(Self::Critical),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Info => "INFO",
            Self::Warning => "WARNING",
            Self::Critical => "CRITICAL",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleFrontmatter {
    pub id: String,
    pub level: String,
    #[serde(default)]
    pub always_apply: bool,
    #[serde(default)]
    pub globs: Vec<String>,
    #[serde(default)]
    pub triggers: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default = "default_priority")]
    pub priority: i32,
}

fn default_priority() -> i32 {
    50
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillFrontmatter {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub triggers: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default = "default_priority")]
    pub priority: i32,
    #[serde(default)]
    pub context_task: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyRuleDoc {
    pub frontmatter: RuleFrontmatter,
    pub body: String,
    pub raw: String,
    pub source_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicySkillDoc {
    pub frontmatter: SkillFrontmatter,
    pub body: String,
    pub raw: String,
    pub source_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyRuleRow {
    pub id: String,
    pub level: String,
    pub always_apply: bool,
    pub globs: Vec<String>,
    pub triggers: Vec<String>,
    pub tags: Vec<String>,
    pub priority: i32,
    pub body: String,
    pub source_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicySkillRow {
    pub name: String,
    pub description: String,
    pub triggers: Vec<String>,
    pub tags: Vec<String>,
    pub priority: i32,
    pub context_task: Option<String>,
    pub body: String,
    pub source_path: String,
}

#[derive(Debug, Clone, Default)]
pub struct MatchInput {
    pub prompt: String,
    pub cwd: PathBuf,
    pub open_files: Vec<PathBuf>,
    pub changed_files: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchedRule {
    pub id: String,
    pub level: String,
    pub score: i32,
    pub reason: String,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchedSkill {
    pub name: String,
    pub score: i32,
    pub reason: String,
    pub description: String,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchResult {
    pub rules: Vec<MatchedRule>,
    pub skills: Vec<MatchedSkill>,
    pub inject: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyIndexResult {
    pub rules_indexed: u32,
    pub skills_indexed: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardViolation {
    pub rule_id: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardResult {
    pub allowed: bool,
    pub violations: Vec<GuardViolation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardOp {
    Write,
    Delete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    pub error: String,
    pub fields: std::collections::HashMap<String, String>,
}
