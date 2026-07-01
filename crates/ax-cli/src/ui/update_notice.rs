//! Compact update banner (matches install / uninstall clack layout).

use owo_colors::OwoColorize;

use super::glyphs::clack_glyphs;
use super::style::{accent, bold, dim};

/// Print a boxed "update available" notice to stderr (non-blocking post-command hint).
pub fn print_update_notice(current: &str, latest: &str) {
    let g = clack_glyphs();
    let version_line = format!("{current} → {latest}");
    let inner_w = version_line.len().max(16);

    eprintln!();
    bar(g);
    eprintln!(
        "{} {} {}",
        g.warn.yellow(),
        "Update available".yellow().bold(),
        ""
    );
    bar(g);
    eprintln!(
        "{}  {} {} {}",
        g.bar.dimmed(),
        dim(current),
        "→".dimmed(),
        bold(latest).yellow()
    );
    bar(g);
    eprintln!(
        "{}  Run {}",
        g.bar.dimmed(),
        accent("ax upgrade")
    );
    bar(g);
    eprint!("{} {}", g.bar_end.dimmed(), g.bar_h.repeat(inner_w).dimmed());
    eprintln!();
    eprintln!();
}

fn bar(g: &super::glyphs::ClackGlyphs) {
    eprintln!("{}", g.bar.dimmed());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notice_does_not_panic() {
        print_update_notice("0.1.2", "0.1.3");
    }
}
