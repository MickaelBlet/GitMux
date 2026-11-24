use crate::config;
use ratatui::style::Color;
use std::str::FromStr;

#[derive(Clone)]
pub struct Theme {
    pub accent: Color,
    pub selected_bg: Color,
    pub added: Color,
    pub removed: Color,
    pub hunk: Color,
    pub border: Color,
    pub dim: Color,
    /// Line background of added / removed diff lines.
    pub added_bg: Color,
    pub removed_bg: Color,
    /// Background of changed words inside added / removed lines.
    pub added_word_bg: Color,
    pub removed_word_bg: Color,
    /// syntect theme name; `None` disables syntax highlighting.
    pub syntax: Option<String>,
}

fn c(v: &Option<String>, d: Color) -> Color {
    v.as_deref()
        .and_then(|s| Color::from_str(s).ok())
        .unwrap_or(d)
}

impl Theme {
    pub fn from(cfg: &config::Config) -> Theme {
        let t = &cfg.theme;
        Theme {
            accent: c(&t.accent, Color::Green),
            selected_bg: c(&t.selected_bg, Color::Rgb(50, 60, 90)),
            added: c(&t.added, Color::Green),
            removed: c(&t.removed, Color::Red),
            hunk: c(&t.hunk, Color::Cyan),
            border: c(&t.border, Color::DarkGray),
            dim: c(&t.dim, Color::DarkGray),
            added_bg: c(&t.added_bg, Color::Rgb(18, 40, 24)),
            removed_bg: c(&t.removed_bg, Color::Rgb(48, 20, 22)),
            added_word_bg: c(&t.added_word_bg, Color::Rgb(30, 90, 45)),
            removed_word_bg: c(&t.removed_word_bg, Color::Rgb(110, 35, 40)),
            syntax: cfg.syntax_highlight.then(|| {
                t.syntax_theme
                    .clone()
                    .unwrap_or_else(|| "base16-ocean.dark".into())
            }),
        }
    }
}
