//! indicatif progress bars with glyph-aware spinners and colors.

use std::sync::{Arc, Mutex};

use owo_colors::OwoColorize;

use ax_types::{IndexPhase, IndexProgress};
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};

use super::glyphs::glyphs;

pub fn index_progress_bar(quiet: bool) -> Option<Arc<Mutex<ProgressState>>> {
    if quiet {
        return None;
    }

    let pb = ProgressBar::new(0);
    pb.set_draw_target(ProgressDrawTarget::stderr());
    let g = glyphs();
    let tick_strings: Vec<String> = g.spinner_ticks.iter().map(|s| s.to_string()).collect();
    let tick_refs: Vec<&str> = tick_strings.iter().map(|s| s.as_str()).collect();
    let progress_chars = format!("{}{}", g.bar_filled, g.bar_empty);

    let mut style = ProgressStyle::with_template(
        "{spinner:.green} [{bar:40.cyan/blue}] {percent:>3}% {pos}/{len} {elapsed_precise} {msg:.dim}",
    )
    .unwrap()
    .progress_chars(&progress_chars);
    if !tick_refs.is_empty() {
        style = style.tick_strings(&tick_refs);
    }
    pb.set_style(style);
    pb.enable_steady_tick(std::time::Duration::from_millis(80));

    Some(Arc::new(Mutex::new(ProgressState {
        pb,
        last_phase: None,
    })))
}

pub struct ProgressState {
    pb: ProgressBar,
    last_phase: Option<IndexPhase>,
}

pub fn index_progress_callback(
    state: Arc<Mutex<ProgressState>>,
) -> Box<dyn FnMut(IndexProgress) + Send> {
    Box::new(move |p: IndexProgress| {
        if let Ok(mut guard) = state.lock() {
            if guard.last_phase != Some(p.phase) {
                guard.last_phase = Some(p.phase);
                eprintln!();
                eprint!("  {} ", phase_heading(p.phase));
                if let Some(ref path) = p.file_path {
                    if p.phase == IndexPhase::Resolving && p.current == 0 {
                        eprintln!("{}", path.dimmed());
                    }
                }
            }

            if p.total > 0 {
                guard.pb.set_length(p.total as u64);
            }
            guard.pb.set_position(p.current as u64);

            let label = phase_verb(p.phase);
            let msg = match p.file_path.as_deref() {
                Some(f) if p.phase != IndexPhase::Resolving || p.current > 0 => {
                    format!("{label} {}", truncate_path(f, 56))
                }
                _ => label.to_string(),
            };
            guard.pb.set_message(msg);
        }
    })
}

pub fn finish_progress_bar(state: Option<Arc<Mutex<ProgressState>>>) {
    if let Some(state) = state {
        if let Ok(guard) = state.lock() {
            guard.pb.finish_and_clear();
            eprintln!();
        }
    }
}

pub fn format_duration_ms(ms: u64) -> String {
    if ms < 1000 {
        return format!("{ms}ms");
    }
    let secs = ms as f64 / 1000.0;
    if secs < 60.0 {
        return format!("{secs:.1}s");
    }
    let mins = (ms / 1000 / 60) as u64;
    let sec = (ms / 1000) % 60;
    if mins < 60 {
        return format!("{mins}m {sec}s");
    }
    let hrs = mins / 60;
    let min = mins % 60;
    format!("{hrs}h {min}m")
}

fn phase_heading(phase: IndexPhase) -> String {
    match phase {
        IndexPhase::Scanning => "Scanning project for changes…".cyan().bold().to_string(),
        IndexPhase::Parsing => "Parsing source files (parallel)…".cyan().bold().to_string(),
        IndexPhase::Extracting => "Writing index to database…".cyan().bold().to_string(),
        IndexPhase::Resolving => "Resolving cross-references…".cyan().bold().to_string(),
        IndexPhase::Optimizing => "Optimizing database…".cyan().bold().to_string(),
    }
}

fn phase_verb(phase: IndexPhase) -> &'static str {
    match phase {
        IndexPhase::Scanning => "scanning",
        IndexPhase::Parsing => "parsing",
        IndexPhase::Extracting => "indexing",
        IndexPhase::Resolving => "resolving",
        IndexPhase::Optimizing => "optimizing",
    }
}

fn truncate_path(path: &str, max: usize) -> String {
    if path.chars().count() <= max {
        return path.to_string();
    }
    let tail: String = path.chars().rev().take(max.saturating_sub(1)).collect::<String>().chars().rev().collect();
    format!("…{tail}")
}
