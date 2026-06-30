//! Styled terminal output: glyphs, colors, and progress spinners.

pub mod install_log;
mod glyphs;
mod progress;
mod spinner;
mod style;
mod terminal;

pub use progress::{finish_progress_bar, format_duration_ms, index_progress_bar, index_progress_callback};
pub use spinner::SpinnerGuard;
pub use style::{accent, bold, dim, err_line, info_line, kv_line, ok_line, warn_line};
pub use terminal::{configure_clap, init as init_terminal};
