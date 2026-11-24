//! Syntax highlighting (syntect) and intra-line word emphasis for diff rows.

use crate::git::diff::{self, Row, RowKind};
use crate::theme::Theme;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use std::ops::Range;
use std::sync::OnceLock;
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, ThemeSet};
use syntect::parsing::SyntaxSet;
use unicode_width::UnicodeWidthChar;

/// Skip syntax highlighting above this many rows (keeps big diffs responsive).
const MAX_ROWS: usize = 20_000;

fn syntaxes() -> &'static SyntaxSet {
    static S: OnceLock<SyntaxSet> = OnceLock::new();
    S.get_or_init(SyntaxSet::load_defaults_nonewlines)
}

fn themes() -> &'static ThemeSet {
    static T: OnceLock<ThemeSet> = OnceLock::new();
    T.get_or_init(ThemeSet::load_defaults)
}

/// Names of the bundled syntect themes.
#[cfg(test)]
pub fn theme_names() -> Vec<String> {
    themes().themes.keys().cloned().collect()
}

pub fn expand_tabs(s: &str) -> String {
    s.replace('\t', "    ")
}

/// Highlight consecutive lines of one file; `None` when the language is unknown.
fn highlight(
    path: &str,
    first: &str,
    lines: &[&str],
    theme: &str,
) -> Option<Vec<Vec<Span<'static>>>> {
    let ss = syntaxes();
    let syn = ss
        .find_syntax_for_file(path)
        .ok()
        .flatten()
        .or_else(|| ss.find_syntax_by_first_line(first))?;
    let ts = themes();
    let th = ts
        .themes
        .get(theme)
        .or_else(|| ts.themes.get("base16-ocean.dark"))?;
    let mut h = HighlightLines::new(syn, th);
    let mut out = Vec::with_capacity(lines.len());
    for l in lines {
        let regions = h.highlight_line(l, ss).ok()?;
        out.push(
            regions
                .into_iter()
                .map(|(st, text)| {
                    let mut s = Style::default().fg(Color::Rgb(
                        st.foreground.r,
                        st.foreground.g,
                        st.foreground.b,
                    ));
                    if st.font_style.contains(FontStyle::BOLD) {
                        s = s.add_modifier(Modifier::BOLD);
                    }
                    if st.font_style.contains(FontStyle::ITALIC) {
                        s = s.add_modifier(Modifier::ITALIC);
                    }
                    Span::styled(text.to_string(), s)
                })
                .collect(),
        );
    }
    Some(out)
}

/// Patch `style` onto the byte `ranges` of a span list.
fn emphasize(
    spans: Vec<Span<'static>>,
    ranges: &[Range<usize>],
    style: Style,
) -> Vec<Span<'static>> {
    if ranges.is_empty() {
        return spans;
    }
    let mut out = Vec::with_capacity(spans.len() + ranges.len() * 2);
    let mut off = 0;
    for sp in spans {
        let text = sp.content.as_ref();
        let (start, end) = (off, off + text.len());
        off = end;
        // cut points inside this span
        let mut cuts = vec![0];
        for r in ranges {
            for p in [r.start, r.end] {
                if p > start && p < end && text.is_char_boundary(p - start) {
                    cuts.push(p - start);
                }
            }
        }
        cuts.push(text.len());
        cuts.sort_unstable();
        cuts.dedup();
        for w in cuts.windows(2) {
            let (a, b) = (w[0], w[1]);
            if a == b {
                continue;
            }
            let inside = ranges
                .iter()
                .any(|r| r.start <= start + a && start + b <= r.end);
            let st = if inside {
                sp.style.patch(style)
            } else {
                sp.style
            };
            out.push(Span::styled(text[a..b].to_string(), st));
        }
    }
    out
}

/// Fill `Row::spans` of content rows: syntax colors (or plain diff colors) plus
/// word-level emphasis between paired removed / added lines.
/// `syntax` = run syntect (slow: done off the UI thread, see `App::spawn_highlight`).
pub fn decorate(rows: &mut [Row], path: &str, theme: &Theme, words: bool, syntax: bool) {
    let content: Vec<String> = rows
        .iter()
        .map(|r| match r.kind {
            RowKind::Add | RowKind::Del | RowKind::Context => {
                expand_tabs(r.text.get(1..).unwrap_or(""))
            }
            _ => String::new(),
        })
        .collect();
    let mut spans: Vec<Vec<Span<'static>>> = rows
        .iter()
        .zip(&content)
        .map(|(r, c)| {
            let st = match r.kind {
                RowKind::Add => Style::default().fg(theme.added),
                RowKind::Del => Style::default().fg(theme.removed),
                _ => Style::default(),
            };
            vec![Span::styled(c.clone(), st)]
        })
        .collect();

    if let Some(name) = theme.syntax.as_ref().filter(|_| syntax)
        && rows.len() <= MAX_ROWS
    {
        let first = content.iter().find(|c| !c.is_empty()).map_or("", |s| s);
        // highlight each hunk twice: old side (context + removed), new side (context + added)
        let mut i = 0;
        while i < rows.len() {
            let Some(h) = rows[i].hunk else {
                i += 1;
                continue;
            };
            let end = (i..rows.len())
                .find(|&k| rows[k].hunk != Some(h))
                .unwrap_or(rows.len());
            let has_del = (i..end).any(|k| rows[k].kind == RowKind::Del);
            for keep in [RowKind::Del, RowKind::Add] {
                // the old side is only needed for removed lines
                if keep == RowKind::Del && !has_del {
                    continue;
                }
                let idx: Vec<usize> = (i..end)
                    .filter(|&k| rows[k].kind == keep || rows[k].kind == RowKind::Context)
                    .collect();
                let lines: Vec<&str> = idx.iter().map(|&k| content[k].as_str()).collect();
                let Some(hl) = highlight(path, first, &lines, name) else {
                    break;
                };
                for (k, sp) in idx.into_iter().zip(hl) {
                    // context rows take the new-side highlighting
                    if rows[k].kind == keep || keep == RowKind::Add {
                        spans[k] = sp;
                    }
                }
            }
            i = end;
        }
    }

    if words {
        let del = Style::default().bg(theme.removed_word_bg);
        let add = Style::default().bg(theme.added_word_bg);
        for (dels, adds) in diff::change_blocks(rows) {
            for (&d, &a) in dels.iter().zip(&adds) {
                let (ro, rn) = diff::word_ranges(&content[d], &content[a]);
                spans[d] = emphasize(std::mem::take(&mut spans[d]), &ro, del);
                spans[a] = emphasize(std::mem::take(&mut spans[a]), &rn, add);
            }
        }
    }

    for (r, sp) in rows.iter_mut().zip(spans) {
        if matches!(r.kind, RowKind::Add | RowKind::Del | RowKind::Context) {
            r.spans = sp;
        }
    }
}

/// Base style of a diff row (headers, prefix, rows without spans).
pub fn row_style(kind: RowKind, theme: &Theme) -> Style {
    match kind {
        RowKind::Header => Style::default().add_modifier(Modifier::BOLD),
        RowKind::HunkHeader => Style::default().fg(theme.hunk),
        RowKind::Add => Style::default().fg(theme.added),
        RowKind::Del => Style::default().fg(theme.removed),
        RowKind::Context => Style::default(),
        RowKind::NoNewline => Style::default().fg(theme.dim),
    }
}

/// Line background of a diff row.
pub fn row_bg(kind: RowKind, theme: &Theme) -> Option<Color> {
    match kind {
        RowKind::Add => Some(theme.added_bg),
        RowKind::Del => Some(theme.removed_bg),
        _ => None,
    }
}

/// Row content (without prefix) as spans; headers fall back to their raw text.
pub fn row_spans(row: &Row, theme: &Theme) -> Vec<Span<'static>> {
    if row.spans.is_empty() {
        vec![Span::styled(
            expand_tabs(&row.text),
            row_style(row.kind, theme),
        )]
    } else {
        row.spans.clone()
    }
}

/// Unified rendering of a row: `+` / `-` / ` ` prefix + content; line bg in `Line::style`.
pub fn row_line(row: &Row, theme: &Theme) -> Line<'static> {
    let mut spans = Vec::new();
    if matches!(row.kind, RowKind::Add | RowKind::Del | RowKind::Context) {
        let p = &row.text[..row.text.len().min(1)];
        spans.push(Span::styled(p.to_string(), row_style(row.kind, theme)));
    }
    spans.extend(row_spans(row, theme));
    let mut line = Line::from(spans);
    if let Some(bg) = row_bg(row.kind, theme) {
        line.style = Style::default().bg(bg);
    }
    line
}

/// Drop the first `h` display columns of a span list.
pub fn clip_spans(spans: Vec<Span<'static>>, h: usize) -> Vec<Span<'static>> {
    if h == 0 {
        return spans;
    }
    let mut skip = h;
    let mut out = Vec::with_capacity(spans.len());
    for sp in spans {
        if skip == 0 {
            out.push(sp);
            continue;
        }
        let text = sp.content.as_ref();
        let mut cut = text.len();
        for (i, c) in text.char_indices() {
            if skip == 0 {
                cut = i;
                break;
            }
            skip = skip.saturating_sub(c.width().unwrap_or(0).max(1));
        }
        if cut < text.len() {
            out.push(Span::styled(text[cut..].to_string(), sp.style));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    const D: &str = "diff --git a/f.rs b/f.rs\n--- a/f.rs\n+++ b/f.rs\n@@ -1,2 +1,2 @@\n fn main() {\n-    let x = foo(a);\n+    let x = foo(bar);\n";

    fn text(spans: &[Span]) -> String {
        spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn decorate_keeps_text() {
        for syntax in [false, true] {
            let cfg = Config {
                syntax_highlight: syntax,
                ..Config::default()
            };
            let theme = Theme::from(&cfg);
            let mut rows = diff::parse(D).rows();
            decorate(&mut rows, "f.rs", &theme, true, true);
            for r in &rows {
                if matches!(r.kind, RowKind::Add | RowKind::Del | RowKind::Context) {
                    assert_eq!(text(&r.spans), expand_tabs(&r.text[1..]));
                }
            }
            // the changed word is emphasized on the added line
            let add = rows.iter().find(|r| r.kind == RowKind::Add).unwrap();
            let emph: String = add
                .spans
                .iter()
                .filter(|s| s.style.bg == Some(theme.added_word_bg))
                .map(|s| s.content.as_ref())
                .collect();
            assert_eq!(emph, "bar");
        }
    }

    #[test]
    fn default_theme_exists() {
        assert!(theme_names().iter().any(|n| n == "base16-ocean.dark"));
    }

    #[test]
    fn clip() {
        let s = vec![Span::raw("ab"), Span::raw("cdé")];
        assert_eq!(text(&clip_spans(s.clone(), 3)), "dé");
        assert_eq!(text(&clip_spans(s, 10)), "");
    }
}
