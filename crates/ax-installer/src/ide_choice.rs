//! Which IDEs `ax init` connects: menu order, defaults, typed input, and what to remove.

use crate::targets::TARGETS;

const EDITORS: &[&str] = &["cursor", "vscode", "windsurf", "zed", "kiro", "antigravity", "continue", "jetbrains"];

/// Every installer target with its menu group, editors first.
pub fn ide_groups() -> Vec<(&'static str, &'static str)> {
    let editors = EDITORS.iter().filter(|id| TARGETS.contains(id)).map(|id| ("Editors", *id));
    let agents = TARGETS.iter().filter(|id| !EDITORS.contains(id)).map(|id| ("CLI agents", *id));
    editors.chain(agents).collect()
}

fn is_known(id: &str) -> bool {
    TARGETS.contains(&id)
}

/// Pre-checked ids: the saved list when there is one (even empty), otherwise the detected ones.
pub fn ide_defaults(saved: Option<&[String]>, detected: &[String]) -> Vec<String> {
    match saved {
        Some(list) => list.to_vec(),
        None => detected.to_vec(),
    }
}

/// Typed fallback: ids split on spaces or commas, `none`, or empty for `defaults`.
pub fn parse_ide_choice(line: &str, defaults: &[String]) -> Result<Vec<String>, String> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(defaults.to_vec());
    }
    if trimmed.eq_ignore_ascii_case("none") {
        return Ok(Vec::new());
    }
    let mut chosen: Vec<String> = Vec::new();
    for id in trimmed
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|s| !s.is_empty())
        .map(str::to_ascii_lowercase)
    {
        if !is_known(&id) {
            return Err(format!("unknown IDE '{id}'; choose from: {}", TARGETS.join(", ")));
        }
        if !chosen.contains(&id) {
            chosen.push(id);
        }
    }
    Ok(chosen)
}

/// Ids that have ax configured but were not chosen; these are uninstalled.
pub fn ides_to_remove(configured: &[String], chosen: &[String]) -> Vec<String> {
    configured
        .iter()
        .filter(|id| is_known(id) && !chosen.contains(id))
        .cloned()
        .collect()
}

/// Targets a pack import may rewrite: configured ones only, limited to the saved list if any.
pub fn pack_refresh_targets(configured: &[String], saved: Option<&[String]>) -> Vec<String> {
    configured
        .iter()
        .filter(|id| saved.is_none_or(|list| list.contains(id)))
        .cloned()
        .collect()
}

/// Split saved ids into known targets and unknown ones (to warn about).
pub fn known_ides(saved: &[String]) -> (Vec<String>, Vec<String>) {
    saved.iter().cloned().partition(|id| is_known(id))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn groups_cover_every_target_once_with_editors_first() {
        let groups = ide_groups();
        assert_eq!(groups.len(), TARGETS.len());
        for t in TARGETS {
            assert_eq!(groups.iter().filter(|(_, id)| id == t).count(), 1, "{t}");
        }
        let first_cli = groups.iter().position(|(g, _)| *g == "CLI agents").unwrap();
        assert!(groups[..first_cli].iter().all(|(g, _)| *g == "Editors"), "{groups:?}");
        assert!(groups[first_cli..].iter().all(|(g, _)| *g == "CLI agents"), "{groups:?}");
        assert_eq!(groups[0], ("Editors", "cursor"));
        assert!(groups.contains(&("CLI agents", "claude")));
    }

    #[test]
    fn saved_list_wins_even_when_empty() {
        let detected = ids(&["cursor", "claude"]);
        assert_eq!(ide_defaults(None, &detected), detected);
        assert_eq!(ide_defaults(Some(&ids(&["zed"])), &detected), ids(&["zed"]));
        assert_eq!(ide_defaults(Some(&[]), &detected), Vec::<String>::new());
    }

    #[test]
    fn typed_choice_parses_ids_none_and_empty() {
        let defaults = ids(&["cursor"]);
        assert_eq!(parse_ide_choice("  \n", &defaults).unwrap(), defaults);
        assert_eq!(parse_ide_choice("none", &defaults).unwrap(), Vec::<String>::new());
        assert_eq!(parse_ide_choice("NONE\n", &defaults).unwrap(), Vec::<String>::new());
        assert_eq!(parse_ide_choice("VSCode, zed cursor", &defaults).unwrap(), ids(&["vscode", "zed", "cursor"]));
        assert_eq!(parse_ide_choice("zed zed", &defaults).unwrap(), ids(&["zed"]));
        let err = parse_ide_choice("cursor notepad", &defaults).unwrap_err();
        assert!(err.contains("notepad") && err.contains("vscode"), "{err}");
    }

    #[test]
    fn every_configured_ide_that_is_not_chosen_is_removed() {
        let chosen = ids(&["cursor"]);
        assert_eq!(ides_to_remove(&ids(&["cursor", "zed", "claude"]), &chosen), ids(&["zed", "claude"]));
        assert_eq!(ides_to_remove(&[], &chosen), Vec::<String>::new());
        assert_eq!(ides_to_remove(&ids(&["cursor"]), &chosen), Vec::<String>::new());
        assert_eq!(ides_to_remove(&ids(&["notepad"]), &[]), Vec::<String>::new());
    }

    #[test]
    fn pack_import_only_refreshes_configured_ides() {
        let configured = ids(&["cursor", "claude"]);
        assert_eq!(pack_refresh_targets(&[], None), Vec::<String>::new());
        assert_eq!(pack_refresh_targets(&configured, None), configured);
        assert_eq!(pack_refresh_targets(&configured, Some(&ids(&["cursor", "zed"]))), ids(&["cursor"]));
        assert_eq!(pack_refresh_targets(&configured, Some(&[])), Vec::<String>::new());
    }

    #[test]
    fn takumi_is_not_a_target() {
        assert_eq!(TARGETS.len(), 13);
        assert!(TARGETS.contains(&"jetbrains"));
        assert!(!TARGETS.contains(&"takumi"));
        assert!(parse_ide_choice("takumi", &[]).unwrap_err().contains("unknown IDE 'takumi'"));
    }

    #[test]
    fn unknown_saved_ids_are_split_out() {
        assert_eq!(known_ides(&ids(&["cursor", "notepad", "zed"])), (ids(&["cursor", "zed"]), ids(&["notepad"])));
    }
}
