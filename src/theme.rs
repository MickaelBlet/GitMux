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
}

fn c(v: &Option<String>, d: Color) -> Color {
    v.as_deref()
        .and_then(|s| Color::from_str(s).ok())
        .unwrap_or(d)
}

impl Theme {
    pub fn from(t: &config::Theme) -> Theme {
        Theme {
            accent: c(&t.accent, Color::Green),
            selected_bg: c(&t.selected_bg, Color::Rgb(50, 60, 90)),
            added: c(&t.added, Color::Green),
            removed: c(&t.removed, Color::Red),
            hunk: c(&t.hunk, Color::Cyan),
            border: c(&t.border, Color::DarkGray),
            dim: c(&t.dim, Color::DarkGray),
        }
    }
}
