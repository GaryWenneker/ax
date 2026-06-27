//! Styled terminal output: glyphs, colors, and progress spinners.

mod glyphs;
mod progress;
mod style;

pub use glyphs::{glyphs, supports_unicode, Glyphs};
pub use progress::{finish_progress_bar, index_progress_bar, index_progress_callback};
pub use style::{accent, bold, dim, err_line, info_line, ok_line, warn_line};
