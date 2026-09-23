//! Clack-style install / uninstall log (@clack/prompts layout parity).

use owo_colors::OwoColorize;

use super::glyphs::{clack_glyphs, ClackGlyphs};
use ax_installer::report::{FileAction, InstallSummary, TargetReport};

pub fn intro(version: &str) {
    let g = clack_glyphs();
    println!(
        "{} {} {}",
        g.bar_start.cyan(),
        "ax".cyan().bold(),
        format!("v{version}").dimmed()
    );
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
            log_file(&g, &report.display_name, file.action.verb(), &tildify(&file.path));
            any = true;
        }
        for note in &report.notes {
            log_note(&g, &report.display_name, note);
            any = true;
        }
    }

    if !any {
        log_info(&g, "No agent configs were changed.");
    }

    let next_lines = vec![
        format!("{} {}", "cd".green().bold(), project_hint.magenta()),
        format!(
            "{} {}  {}",
            "ax".cyan().bold(),
            "init".green().bold(),
            "# build a project's graph (one time; auto-syncs after)".dimmed()
        ),
    ];
    clack_note(&g, "Next: index a project", &next_lines);

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
    println!(
        "{} {} {} {}",
        g.bar_start.cyan(),
        "ax".cyan().bold(),
        format!("v{version}").dimmed(),
        "uninstall".yellow().bold()
    );
    let mut any = false;

    for report in reports {
        let removed: Vec<_> = report
            .files
            .iter()
            .filter(|f| matches!(f.action, FileAction::Updated | FileAction::Created))
            .collect();
        if removed.is_empty() {
            log_note(&g, &report.display_name, "not configured — nothing to remove");
            any = true;
        } else {
            for file in removed {
                log_file(&g, &report.display_name, "Removed", &tildify(&file.path));
                any = true;
            }
        }
        for note in &report.notes {
            log_note(&g, &report.display_name, note);
            any = true;
        }
    }

    if !any {
        log_info(&g, "No agent configs were removed.");
    }

    clack_outro(&g, "Done.");
}

fn log_bar(g: &ClackGlyphs) {
    println!("{}", g.bar.bright_black());
}

fn paint_name(name: &str) -> String {
    name.cyan().bold().to_string()
}

fn paint_verb(verb: &str) -> String {
    match verb {
        "Created" => verb.green().bold().to_string(),
        "Updated" => verb.yellow().bold().to_string(),
        "Removed" => verb.red().bold().to_string(),
        "Unchanged" => verb.green().to_string(),
        "Skipped" => verb.bright_black().to_string(),
        _ => verb.white().to_string(),
    }
}

fn paint_path(path: &str) -> String {
    path.magenta().to_string()
}

fn log_file(g: &ClackGlyphs, name: &str, verb: &str, path: &str) {
    log_bar(g);
    let mark = match verb {
        "Updated" => g.success.yellow().to_string(),
        "Removed" => g.success.red().to_string(),
        "Skipped" => g.success.bright_black().to_string(),
        _ => g.success.green().to_string(),
    };
    println!(
        "{} {} {} {}",
        mark,
        paint_name(name),
        paint_verb(verb),
        paint_path(path)
    );
}

fn log_note(g: &ClackGlyphs, name: &str, message: &str) {
    log_bar(g);
    println!(
        "{} {} {}",
        g.info.cyan(),
        paint_name(name),
        message.yellow()
    );
}

fn log_info(g: &ClackGlyphs, message: &str) {
    log_bar(g);
    println!("{} {}", g.info.cyan(), message.white());
}

fn log_warn(g: &ClackGlyphs, message: &str) {
    log_bar(g);
    println!("{} {}", g.warn.yellow().bold(), message.yellow());
}

fn clack_note(g: &ClackGlyphs, title: &str, lines: &[String]) {
    log_bar(g);
    let title_painted = title.cyan().bold().to_string();
    let widths: Vec<usize> = lines.iter().map(|line| console::measure_text_width(line)).collect();
    let max_w = widths
        .iter()
        .copied()
        .max()
        .unwrap_or(0)
        .max(console::measure_text_width(&title_painted));
    let inner = max_w + 2;
    let title_pad = inner.saturating_sub(console::measure_text_width(&title_painted) + 1);
    print!("{} {} ", g.note_mark.cyan(), title_painted);
    print!("{}", g.bar_h.repeat(title_pad).cyan());
    println!("{}", g.corner_tr.cyan());
    for (line, width) in lines.iter().zip(widths) {
        let pad = inner.saturating_sub(width);
        println!(
            "{} {}{} {}",
            g.bar.cyan(),
            line,
            " ".repeat(pad),
            g.bar.cyan()
        );
    }
    print!("{}", g.connect_left.cyan());
    print!("{}", g.bar_h.repeat(inner + 2).cyan());
    println!("{}", g.corner_br.cyan());
}

fn clack_outro(g: &ClackGlyphs, message: &str) {
    log_bar(g);
    let painted = if let Some(rest) = message.strip_prefix("Done!") {
        format!("{}{}", "Done!".green().bold(), rest.white())
    } else {
        message.green().bold().to_string()
    };
    println!("{} {painted}", g.bar_end.green());
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
