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

pub fn supports_unicode() -> bool {
    if std::env::var("AX_ASCII").as_deref() == Ok("1") {
        return false;
    }
    if std::env::var("AX_UNICODE").as_deref() == Ok("1") {
        return true;
    }
    #[cfg(windows)]
    {
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
