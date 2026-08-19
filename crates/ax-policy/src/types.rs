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

/// Where a rule/skill lives in the hierarchy (not severity — see [`PolicyLevel`]).
///
/// Precedence (later wins on same id/name): company → workspace → project →
/// private_user → private_project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PolicyScope {
    Company,
    Workspace,
    #[default]
    Project,
    PrivateUser,
    PrivateProject,
}

impl PolicyScope {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "company" | "global" => Some(Self::Company),
            "workspace" => Some(Self::Workspace),
            "project" | "" => Some(Self::Project),
            "private_user" | "private-user" | "user" => Some(Self::PrivateUser),
            "private_project" | "private-project" | "private" => Some(Self::PrivateProject),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Company => "company",
            Self::Workspace => "workspace",
            Self::Project => "project",
            Self::PrivateUser => "private_user",
            Self::PrivateProject => "private_project",
        }
    }

    /// Whether this scope may be exported in a git pack.
    pub fn is_packable(self) -> bool {
        matches!(self, Self::Project | Self::Workspace)
    }

    /// Human label for UI/docs (Company aliases on-disk `global_policy`).
    pub fn label(self) -> &'static str {
        match self {
            Self::Company => "Company",
            Self::Workspace => "Workspace",
            Self::Project => "Project",
            Self::PrivateUser => "Private (user)",
            Self::PrivateProject => "Private (project)",
        }
    }
}

fn default_scope() -> String {
    PolicyScope::Project.as_str().into()
}

/// Review / lifecycle status for a rule or skill.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum PolicyItemStatus {
    #[default]
    Approved,
    Pending,
    Rejected,
}

impl PolicyItemStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Approved => "approved",
            Self::Pending => "pending",
            Self::Rejected => "rejected",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "approved" | "" => Some(Self::Approved),
            "pending" => Some(Self::Pending),
            "rejected" => Some(Self::Rejected),
            _ => None,
        }
    }
}

fn default_priority() -> i32 {
    50
}

fn default_true() -> bool {
    true
}

fn default_approved() -> String {
    PolicyItemStatus::Approved.as_str().into()
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
    /// When false, matcher/preflight skip this rule. Default true.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// `approved` (active), `pending` (review queue), or `rejected`.
    #[serde(default = "default_approved")]
    pub status: String,
    /// Alias for tagging as shareable (`tags` gains `shared` on parse).
    #[serde(default)]
    pub share: bool,
    /// Hierarchy scope: company | workspace | project | private_user | private_project.
    #[serde(default = "default_scope")]
    pub scope: String,
    /// Per-item storage override: `files` | `database`. Empty/None → project default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage: Option<String>,
    /// External body path (absolute, project-relative, or `root:<id>/…`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Write under a configured `policy.roots` mount instead of the scope dir.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillFrontmatter {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub always_apply: bool,
    #[serde(default)]
    pub triggers: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default = "default_priority")]
    pub priority: i32,
    #[serde(default)]
    pub context_task: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_approved")]
    pub status: String,
    #[serde(default)]
    pub share: bool,
    #[serde(default = "default_scope")]
    pub scope: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyRuleDoc {
    pub frontmatter: RuleFrontmatter,
    pub body: String,
    pub raw: String,
    pub source_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stub_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicySkillDoc {
    pub frontmatter: SkillFrontmatter,
    pub body: String,
    pub raw: String,
    pub source_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stub_path: Option<String>,
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
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_approved")]
    pub status: String,
    #[serde(default = "default_scope")]
    pub scope: String,
    /// Per-item override (`files`/`database`); null means inherit project default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stub_path: Option<String>,
    /// Resolved storage after applying project default.
    #[serde(default)]
    pub effective_storage: String,
    /// True when `storage` is set on the item (not inheriting default).
    #[serde(default)]
    pub storage_is_override: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicySkillRow {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub always_apply: bool,
    pub triggers: Vec<String>,
    pub tags: Vec<String>,
    pub priority: i32,
    pub context_task: Option<String>,
    pub body: String,
    pub source_path: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_approved")]
    pub status: String,
    #[serde(default = "default_scope")]
    pub scope: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stub_path: Option<String>,
    #[serde(default)]
    pub effective_storage: String,
    #[serde(default)]
    pub storage_is_override: bool,
}

#[derive(Debug, Clone, Default)]
pub struct MatchInput {
    pub prompt: String,
    pub cwd: PathBuf,
    pub open_files: Vec<PathBuf>,
    pub changed_files: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchedRule {
    pub id: String,
    pub level: String,
    pub score: i32,
    pub reason: String,
    pub always_apply: bool,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchedSkill {
    pub name: String,
    pub score: i32,
    pub reason: String,
    pub description: String,
    pub body: String,
    #[serde(default)]
    pub always_apply: bool,
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
#[serde(rename_all = "camelCase")]
pub struct PolicyStatus {
    pub indexed: bool,
    pub rules: u32,
    pub skills: u32,
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreflightMeta {
    pub policy_status: PolicyStatus,
    pub matched_rules: usize,
    pub matched_skills: usize,
    pub guard_required: bool,
    pub mode: String,
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
