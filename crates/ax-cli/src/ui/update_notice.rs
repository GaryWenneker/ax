//! Compact update banner (matches install / uninstall clack layout).

use std::fmt::Write as _;

use owo_colors::OwoColorize;

use super::glyphs::clack_glyphs;
use super::style::{accent, bold, dim};

/// Print a boxed "update available" notice to stderr (after short commands, or at `ax web` start).
pub fn print_update_notice(current: &str, latest: &str) {
    eprint!("{}", render_update_notice(current, latest));
}

/// The notice as printed, including its blank lines before and after. The
/// closing rule is as wide as the version line on screen, at least 16 columns.
fn render_update_notice(current: &str, latest: &str) -> String {
    let g = clack_glyphs();
    let version_line = format!("{current} → {latest}");
    let inner_w = console::measure_text_width(&version_line).max(16);
    let bar = g.bar.dimmed().to_string();

    let mut out = String::from("\n");
    let _ = writeln!(out, "{bar}");
    let _ = writeln!(
        out,
        "{} {} ",
        g.warn.yellow(),
        "Update available".yellow().bold()
    );
    let _ = writeln!(out, "{bar}");
    let _ = writeln!(
        out,
        "{bar}  {} {} {}",
        dim(current),
        "→".dimmed(),
        bold(latest).yellow()
    );
    let _ = writeln!(out, "{bar}");
    let _ = writeln!(out, "{bar}  Run {}", accent("ax upgrade"));
    let _ = writeln!(out, "{bar}");
    let _ = writeln!(
        out,
        "{} {}",
        g.bar_end.dimmed(),
        g.bar_h.repeat(inner_w).dimmed()
    );
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain_lines(current: &str, latest: &str) -> Vec<String> {
        console::strip_ansi_codes(&render_update_notice(current, latest))
            .lines()
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn notice_names_both_versions_and_the_upgrade_command() {
        let lines = plain_lines("0.1.2", "0.1.3");
        assert_eq!(lines.first().map(String::as_str), Some(""));
        assert!(lines.iter().any(|l| l.contains("Update available")));
        assert!(lines.iter().any(|l| l.ends_with("0.1.2 → 0.1.3")));
        assert!(lines.iter().any(|l| l.ends_with("Run ax upgrade")));
    }

    #[test]
    fn closing_rule_matches_the_version_line_width() {
        let g = clack_glyphs();
        for (current, latest) in [
            ("0.1.2", "0.1.3"),
            ("10.1.2", "10.1.30"),
            ("1.0.0-rc.1", "12.10.100"),
        ] {
            let lines = plain_lines(current, latest);
            let rule = lines
                .iter()
                .find(|l| l.starts_with(g.bar_end))
                .expect("closing rule");
            let rule_width = console::measure_text_width(rule.trim_start_matches(g.bar_end).trim());
            let text_width = console::measure_text_width(&format!("{current} → {latest}"));
            assert_eq!(rule_width, text_width.max(16), "{current} → {latest}");
        }
    }
}
