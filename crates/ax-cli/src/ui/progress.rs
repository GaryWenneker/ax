//! indicatif progress bars with glyph-aware spinners and colors.

use std::sync::{Arc, Mutex};

use ax_types::{IndexPhase, IndexProgress};
use indicatif::{ProgressBar, ProgressStyle};

use super::glyphs;

pub fn index_progress_bar(quiet: bool) -> Option<Arc<Mutex<ProgressBar>>> {
    if quiet {
        return None;
    }

    let pb = ProgressBar::new(0);
    let g = glyphs();
    let tick_strings: Vec<String> = g.spinner_ticks.iter().map(|s| s.to_string()).collect();
    let tick_refs: Vec<&str> = tick_strings.iter().map(|s| s.as_str()).collect();
    let progress_chars = format!("{}{}", g.bar_filled, g.bar_empty);

    let mut style = ProgressStyle::with_template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} {msg:.dim}")
        .unwrap()
        .progress_chars(&progress_chars);
    if !tick_refs.is_empty() {
        style = style.tick_strings(&tick_refs);
    }
    pb.set_style(style);

    Some(Arc::new(Mutex::new(pb)))
}

pub fn index_progress_callback(
    pb: Arc<Mutex<ProgressBar>>,
) -> Box<dyn FnMut(IndexProgress) + Send> {
    Box::new(move |p: IndexProgress| {
        let label = match p.phase {
            IndexPhase::Scanning => "scanning",
            IndexPhase::Extracting => "indexing",
            IndexPhase::Resolving => "resolving",
        };
        if let Ok(guard) = pb.lock() {
            if p.total > 0 {
                guard.set_length(p.total as u64);
            }
            guard.set_position(p.current as u64);
            let msg = p
                .file_path
                .as_deref()
                .map(|f| format!("{} {}", label, f))
                .unwrap_or_else(|| label.to_string());
            guard.set_message(msg);
        }
    })
}

pub fn finish_progress_bar(pb: Option<Arc<Mutex<ProgressBar>>>) {
    if let Some(pb) = pb {
        if let Ok(guard) = pb.lock() {
            guard.finish_and_clear();
        }
    }
}
