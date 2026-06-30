//! Clack-style install / uninstall log (@clack/prompts layout parity).

use owo_colors::OwoColorize;

use super::glyphs::{clack_glyphs, ClackGlyphs};
use crate::installer::report::{FileAction, InstallSummary, TargetReport};

pub fn intro(version: &str) {
    let g = clack_glyphs();
    println!("{} ax v{version}", g.bar_start.dimmed());
}

pub fn render_install(summary: &InstallSummary, project_hint: &str, warning: Option<&str>) {
    let g = clack_glyphs();
    let mut any = false;

    if let Some(msg) = warning {
        log_warn(&g, msg);
        any = true;
    }

    for report in &summary.reports {
        if !report.touched() && report.notes.is_empty() {
            continue;
        }
        for file in &report.files {
            if file.action == FileAction::Skipped {
                continue;
            }
            log_success(
                &g,
                &format!("{}: {}", report.display_name, file.action.verb()),
                &tildify(&file.path),
            );
            any = true;
        }
        for note in &report.notes {
            log_info(&g, &format!("{}: {}", report.display_name, note));
            any = true;
        }
    }

    if !any {
        log_info(&g, "No agent configs were changed.");
    }

    let next_body = format!(
        "cd {project_hint}\nax init              # build a project's graph (one time; auto-syncs after)"
    );
    clack_note(&g, "Next: index a project", &next_body);

    let n = summary.configured_targets().len();
    let outro = if n > 0 {
        format!(
            "Done! Restart your agent{} to use ax.",
            if n > 1 { "s" } else { "" }
        )
    } else {
        "Done!".into()
    };
    clack_outro(&g, &outro);
}

pub fn render_uninstall(reports: &[TargetReport], version: &str) {
    let g = clack_glyphs();
    println!("{} ax v{version} — uninstall", g.bar_start.dimmed());
    let mut any = false;

    for report in reports {
        let removed: Vec<_> = report
            .files
            .iter()
            .filter(|f| matches!(f.action, FileAction::Updated | FileAction::Created))
            .collect();
        if removed.is_empty() {
            log_info(
                &g,
                &format!("{}: not configured — nothing to remove", report.display_name),
            );
            any = true;
        } else {
            for file in removed {
                log_success(
                    &g,
                    &format!("{}: Removed", report.display_name),
                    &tildify(&file.path),
                );
                any = true;
            }
        }
        for note in &report.notes {
            log_info(&g, &format!("{}: {}", report.display_name, note));
            any = true;
        }
    }

    if !any {
        log_info(&g, "No agent configs were removed.");
    }

    clack_outro(&g, "Done.");
}

fn log_bar(g: &ClackGlyphs) {
    println!("{}", g.bar.dimmed());
}

fn log_success(g: &ClackGlyphs, head: &str, detail: &str) {
    log_bar(g);
    print!("{} {} ", g.success.green(), head);
    println!("{}", detail.dimmed());
}

fn log_info(g: &ClackGlyphs, message: &str) {
    log_bar(g);
    println!("{} {message}", g.info.blue());
}

fn log_warn(g: &ClackGlyphs, message: &str) {
    log_bar(g);
    println!("{} {message}", g.warn.yellow());
}

fn clack_note(g: &ClackGlyphs, title: &str, body: &str) {
    log_bar(g);
    let lines: Vec<&str> = body.lines().collect();
    let max_w = lines
        .iter()
        .map(|l| l.len())
        .max()
        .unwrap_or(0)
        .max(title.len());
    let inner = max_w + 2;
    let title_pad = inner.saturating_sub(title.len() + 1);
    print!("{} {} ", g.note_mark.green(), title);
    print!("{}", g.bar_h.repeat(title_pad).dimmed());
    println!("{}", g.corner_tr.dimmed());
    for line in lines {
        let pad = inner.saturating_sub(line.len());
        println!(
            "{} {}{} {}",
            g.bar.dimmed(),
            line.dimmed(),
            " ".repeat(pad),
            g.bar.dimmed()
        );
    }
    print!("{}", g.connect_left.dimmed());
    print!("{}", g.bar_h.repeat(inner + 2).dimmed());
    println!("{}", g.corner_br.dimmed());
}

fn clack_outro(g: &ClackGlyphs, message: &str) {
    log_bar(g);
    println!("{} {message}", g.bar_end.dimmed());
    println!();
}

pub fn tildify(path: &std::path::Path) -> String {
    if let Some(home) = dirs::home_dir() {
        if path.starts_with(&home) {
            let rest = path.strip_prefix(&home).unwrap_or(path);
            let rest = rest.to_string_lossy().trim_start_matches(['\\', '/']).to_string();
            return format!("~/{rest}");
        }
    }
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tildify_home() {
        if let Some(home) = dirs::home_dir() {
            let p = home.join(".cursor").join("mcp.json");
            assert!(tildify(&p).starts_with("~/"));
        }
    }
}
