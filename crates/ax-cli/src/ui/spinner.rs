//! Indeterminate spinners for short async CLI operations.

use std::time::Duration;

use indicatif::{ProgressBar, ProgressStyle};

use super::glyphs::glyphs;

/// Spinner shown while an async operation runs; clears on drop.
pub struct SpinnerGuard {
    pb: Option<ProgressBar>,
}

impl SpinnerGuard {
    pub fn new(message: impl Into<String>, quiet: bool) -> Self {
        if quiet {
            return Self { pb: None };
        }

        let pb = ProgressBar::new_spinner();
        let g = glyphs();
        let tick_strings: Vec<String> = g.spinner_ticks.iter().map(|s| s.to_string()).collect();
        let tick_refs: Vec<&str> = tick_strings.iter().map(|s| s.as_str()).collect();

        let mut style = ProgressStyle::with_template("{spinner:.green} {msg:.cyan}").unwrap();
        if !tick_refs.is_empty() {
            style = style.tick_strings(&tick_refs);
        }
        pb.set_style(style);
        pb.enable_steady_tick(Duration::from_millis(80));
        pb.set_message(message.into());

        Self { pb: Some(pb) }
    }

    #[allow(dead_code)]
    pub fn set_message(&self, message: impl AsRef<str>) {
        if let Some(pb) = &self.pb {
            pb.set_message(message.as_ref().to_string());
        }
    }
}

impl Drop for SpinnerGuard {
    fn drop(&mut self) {
        if let Some(pb) = self.pb.take() {
            pb.finish_and_clear();
        }
    }
}
