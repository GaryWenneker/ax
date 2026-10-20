//! Search, install, and publish Agent Skills (skills.sh) without a review queue.

mod audit;
mod install;
mod lockfile;
mod publish;
mod remote;
mod search;
mod settings;
mod source;
mod telemetry;
mod validate;
mod wellknown;

pub use audit::{
    agent_brief, audit_directory, audit_directory_present, audit_rule_file, audit_rule_present,
    audit_skill_text, find_rule_file, find_skill_dir, list_latest, load_latest, needs_edit, open_briefs,
    parse_verdict, project_briefs, require_one_skill, store_report, AuditSummary, SkillAuditReport,
    SkillPresence,
};
pub use install::{install_tree, InstallReceipt, COPY_MAX_BYTES, COPY_MAX_FILES};
pub use lockfile::{catalog_id, LockEntry, LockFile};
pub use publish::stage_one_skill;
pub use remote::{
    attach_catalog_cards, fetch_source, git_clone, git_publish, http_bytes, http_text, search_body,
    Fetched,
};
pub use search::{parse_search_body, SearchHit};
pub use settings::{load_settings, save_settings, CatalogSettings};
pub use source::{parse_source, GitHost, SkillSource};
pub use telemetry::install_report_url;
pub use validate::{read_skill_meta, validate_skill_name, SkillMeta};
pub use wellknown::{materialize_well_known, WellKnownSkill};

pub const TELEMETRY_URL: &str = "https://www.skills.sh/tele/t";
pub const SEARCH_URL: &str = "https://skills.sh/api/search";
