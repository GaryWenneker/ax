//! Unicode / ASCII glyphs (CodeGraph parity — Windows-safe default).

use std::sync::OnceLock;

static CACHE: OnceLock<Glyphs> = OnceLock::new();

#[derive(Clone, Copy)]
pub struct Glyphs {
    pub ok: &'static str,
    pub err: &'static str,
    pub info: &'static str,
    pub warn: &'static str,
    pub spinner_ticks: &'static [&'static str],
    pub bar_filled: &'static str,
    pub bar_empty: &'static str,
    pub rail: &'static str,
}

const UNICODE: Glyphs = Glyphs {
    ok: "\u{2713}",
    err: "\u{2717}",
    info: "\u{2139}",
    warn: "\u{26A0}",
    spinner_ticks: &["\u{28CB}", "\u{28FB}", "\u{28F9}", "\u{28F8}", "\u{28FC}", "\u{28F4}", "\u{28F6}", "\u{28F7}", "\u{28F1}", "\u{28CF}"],
    bar_filled: "\u{2588}",
    bar_empty: "\u{2591}",
    rail: "\u{2502}",
};

const ASCII: Glyphs = Glyphs {
    ok: "[ok]",
    err: "[err]",
    info: "[i]",
    warn: "[!]",
    spinner_ticks: &[".", "*", "+", "x", "o", "O"],
    bar_filled: "#",
    bar_empty: "-",
    rail: "|",
};

/// @clack/prompts tree + note glyphs (install / uninstall UI).
#[derive(Clone, Copy)]
pub struct ClackGlyphs {
    pub bar_start: &'static str,
    pub bar_end: &'static str,
    pub bar: &'static str,
    pub bar_h: &'static str,
    pub connect_left: &'static str,
    pub corner_tr: &'static str,
    pub corner_br: &'static str,
    pub success: &'static str,
    pub info: &'static str,
    pub note_mark: &'static str,
    pub warn: &'static str,
}

const CLACK_UNICODE: ClackGlyphs = ClackGlyphs {
    bar_start: "\u{250C}",
    bar_end: "\u{2514}",
    bar: "\u{2502}",
    bar_h: "\u{2500}",
    connect_left: "\u{251C}",
    corner_tr: "\u{256E}",
    corner_br: "\u{256F}",
    success: "\u{25C6}",
    info: "\u{25CF}",
    note_mark: "\u{25C7}",
    warn: "\u{25B2}",
};

const CLACK_ASCII: ClackGlyphs = ClackGlyphs {
    bar_start: "T",
    bar_end: "L",
    bar: "|",
    bar_h: "-",
    connect_left: "+",
    corner_tr: "+",
    corner_br: "+",
    success: "*",
    info: "o",
    note_mark: "o",
    warn: "!",
};

pub fn supports_unicode() -> bool {
    if std::env::var("AX_ASCII").as_deref() == Ok("1") {
        return false;
    }
    if std::env::var("AX_UNICODE").as_deref() == Ok("1") {
        return true;
    }
    #[cfg(windows)]
    {
        // Windows Terminal sets WT_SESSION; AX_UNICODE=1 forces Unicode glyphs.
        if std::env::var("WT_SESSION").is_ok() {
            return true;
        }
        return false;
    }
    #[cfg(not(windows))]
    {
        std::env::var("TERM").map(|t| t != "linux").unwrap_or(true)
    }
}

pub fn glyphs() -> &'static Glyphs {
    CACHE.get_or_init(|| if supports_unicode() { UNICODE } else { ASCII })
}

static CLACK_CACHE: OnceLock<ClackGlyphs> = OnceLock::new();

pub fn clack_glyphs() -> &'static ClackGlyphs {
    CLACK_CACHE.get_or_init(|| {
        if supports_unicode() {
            CLACK_UNICODE
        } else {
            CLACK_ASCII
        }
    })
}
