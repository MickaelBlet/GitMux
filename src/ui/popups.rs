//! Popup rendering (confirm, input, menu, commit editor, help, messages, rebase todo).

use super::hit::{Btn, Hit};
use super::{FOCUS, button, hovered};
use crate::app::{App, Popup};
use crate::git::rebase::TodoAction;
use crate::keymap::Ctx;
use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};
use unicode_width::UnicodeWidthStr;

pub fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width.saturating_sub(2)).max(10.min(area.width));
    let h = h.min(area.height.saturating_sub(2)).max(3.min(area.height));
    Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    )
}

fn frame(f: &mut Frame, app: &mut App, r: Rect, title: &str, color: Color) -> Rect {
    f.render_widget(Clear, r);
    let b = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(color))
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ));
    let inner = b.inner(r);
    f.render_widget(b, r);
    app.hits.push(r, Hit::Popup);
    inner
}

/// OK / Cancel buttons on the bottom border.
fn buttons(f: &mut Frame, app: &mut App, r: Rect, ok: &str, cancel: Option<&str>) {
    let mut w = ok.width() as u16 + 3;
    if let Some(c) = cancel {
        w += c.width() as u16 + 3;
    }
    let mut x = r.right().saturating_sub(w + 1);
    let y = r.bottom() - 1;
    x += button(
        f,
        app,
        x,
        y,
        ok,
        Color::LightGreen,
        false,
        Hit::Button(Btn::Ok),
    ) + 1;
    if let Some(c) = cancel {
        button(
            f,
            app,
            x,
            y,
            c,
            Color::LightRed,
            false,
            Hit::Button(Btn::Cancel),
        );
    }
}

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    // draw every popup of the stack, the top one last
    let n = app.popups.len();
    for i in 0..n {
        let p = app.popups[i].clone();
        let top = i + 1 == n;
        if !top {
            // only the top popup is interactive
            let saved = app.hits.hits.len();
            draw_one(f, app, area, &p, false);
            app.hits.hits.truncate(saved);
            let r = Rect::new(0, 0, area.width, area.height);
            app.hits.push(r, Hit::Popup);
        } else {
            draw_one(f, app, area, &p, true);
        }
    }
}

fn draw_one(f: &mut Frame, app: &mut App, area: Rect, p: &Popup, top: bool) {
    match p {
        Popup::Confirm { title, msg, .. } => {
            let lines: Vec<&str> = msg.lines().collect();
            let w = lines
                .iter()
                .map(|l| l.width())
                .max()
                .unwrap_or(10)
                .max(title.width() + 4)
                .max(30) as u16
                + 4;
            let r = centered(area, w, lines.len() as u16 + 4);
            let inner = frame(f, app, r, title, Color::Yellow);
            f.render_widget(
                Paragraph::new(msg.as_str()).wrap(Wrap { trim: false }),
                inner.inner(ratatui::layout::Margin::new(1, 0)),
            );
            buttons(f, app, r, "y Yes", Some("n No"));
        }
        Popup::Input {
            title, input, mask, ..
        } => {
            let w = (title.width() as u16 + 6).max(60);
            let r = centered(area, w, 3);
            let inner = frame(f, app, r, title, FOCUS);
            let text = if *mask {
                "•".repeat(input.lines[0].chars().count())
            } else {
                input.lines[0].clone()
            };
            let cw = input.lines[0]
                .chars()
                .take(input.col)
                .collect::<String>()
                .width() as u16;
            let start = cw.saturating_sub(inner.width.saturating_sub(1));
            let shown: String = text.chars().skip(start as usize).collect();
            f.buffer_mut().set_stringn(
                inner.x,
                inner.y,
                shown,
                inner.width as usize,
                Style::default(),
            );
            app.hits.push(inner, Hit::InputText);
            app.hits.input_area = Some((inner, start as usize));
            if top {
                f.set_cursor_position(Position::new(inner.x + cw - start, inner.y));
            }
            buttons(f, app, r, "enter OK", Some("esc Cancel"));
        }
        Popup::Menu { title, items, sel } => {
            let w = items
                .iter()
                .map(|i| i.label.width() + 4)
                .max()
                .unwrap_or(10)
                .max(title.width() + 6) as u16
                + 2;
            let r = centered(area, w, items.len() as u16 + 2);
            let inner = frame(f, app, r, title, FOCUS);
            let h = inner.height as usize;
            let off = if *sel >= h { sel + 1 - h } else { 0 };
            for (i, it) in items.iter().enumerate().skip(off).take(h) {
                let row = Rect::new(inner.x, inner.y + (i - off) as u16, inner.width, 1);
                let st = if i == *sel {
                    Style::default().bg(app.theme.selected_bg)
                } else if hovered(app, row) {
                    Style::default().bg(Color::Indexed(236))
                } else {
                    Style::default()
                };
                f.buffer_mut().set_style(row, st);
                let key = it.key.map(|k| format!("{k} ")).unwrap_or_default();
                let line = Line::from(vec![
                    Span::styled(key, Style::default().fg(Color::LightCyan)),
                    Span::raw(it.label.clone()),
                ]);
                f.buffer_mut()
                    .set_line(row.x + 1, row.y, &line, row.width.saturating_sub(1));
                app.hits.push(row, Hit::MenuItem(i));
            }
        }
        Popup::Commit(c) => {
            let title = if c.reword.is_some() || c.only {
                "Reword commit"
            } else if c.amend {
                "Amend commit"
            } else {
                "Commit"
            };
            let r = centered(area, 76, 16);
            let inner = frame(f, app, r, title, FOCUS);
            // toggles
            let mut x = inner.x;
            let y = inner.y;
            if c.reword.is_none() && !c.only {
                for (label, on, b) in [
                    ("amend", c.amend, Btn::Amend),
                    ("no-verify", c.no_verify, Btn::NoVerify),
                    ("signoff", c.signoff, Btn::Signoff),
                ] {
                    let l = format!("{} {label}", if on { "x" } else { " " });
                    x += button(f, app, x, y, &l, Color::Gray, on, Hit::Button(b)) + 1;
                }
            }
            let summary = c
                .input
                .lines
                .first()
                .map(|l| l.chars().count())
                .unwrap_or(0);
            let count = format!("{summary}");
            let st = if summary > 72 {
                Style::default().fg(Color::LightRed)
            } else {
                Style::default().fg(app.theme.dim)
            };
            f.buffer_mut()
                .set_string(inner.right() - count.len() as u16, y, &count, st);
            // editor
            let ed = Rect::new(
                inner.x,
                inner.y + 2,
                inner.width,
                inner.height.saturating_sub(3),
            );
            f.buffer_mut()
                .set_style(ed, Style::default().bg(Color::Indexed(235)));
            let h = ed.height as usize;
            let off = if c.input.row >= h {
                c.input.row + 1 - h
            } else {
                0
            };
            for (i, l) in c.input.lines.iter().enumerate().skip(off).take(h) {
                let st = if i == 0 {
                    Style::default().add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                f.buffer_mut()
                    .set_stringn(ed.x, ed.y + (i - off) as u16, l, ed.width as usize, st);
            }
            app.hits.push(ed, Hit::InputText);
            app.hits.input_area = Some((ed, off));
            if top {
                let cw = c.input.lines[c.input.row]
                    .chars()
                    .take(c.input.col)
                    .collect::<String>()
                    .width() as u16;
                f.set_cursor_position(Position::new(
                    (ed.x + cw).min(ed.right() - 1),
                    ed.y + (c.input.row - off) as u16,
                ));
            }
            let hint = "enter: commit · ctrl+j / alt+enter: new line · alt+a/n/s: toggles";
            f.buffer_mut().set_stringn(
                inner.x,
                inner.bottom() - 1,
                hint,
                inner.width as usize,
                Style::default().fg(app.theme.dim),
            );
            buttons(f, app, r, "Commit", Some("Cancel"));
        }
        Popup::Help { scroll } => {
            let lines = help_lines(app);
            let r = centered(area, 80, area.height.saturating_sub(4));
            let inner = frame(f, app, r, "Help — keys (esc to close)", FOCUS);
            let h = inner.height as usize;
            let s = (*scroll).min(lines.len().saturating_sub(h));
            for (i, l) in lines.into_iter().skip(s).take(h).enumerate() {
                f.buffer_mut().set_line(
                    inner.x + 1,
                    inner.y + i as u16,
                    &l,
                    inner.width.saturating_sub(1),
                );
            }
        }
        Popup::Message {
            title,
            lines,
            scroll,
            error,
        } => {
            let w = lines
                .iter()
                .map(|l| l.width())
                .max()
                .unwrap_or(10)
                .max(title.width() + 4)
                .max(30) as u16
                + 4;
            let r = centered(area, w.min(area.width - 4), lines.len() as u16 + 3);
            let color = if *error { Color::LightRed } else { FOCUS };
            let inner = frame(f, app, r, title, color);
            let h = inner.height.saturating_sub(1) as usize;
            let s = (*scroll).min(lines.len().saturating_sub(h));
            for (i, l) in lines.iter().skip(s).take(h).enumerate() {
                f.buffer_mut().set_stringn(
                    inner.x + 1,
                    inner.y + i as u16,
                    l,
                    inner.width.saturating_sub(1) as usize,
                    Style::default(),
                );
            }
            buttons(f, app, r, "OK", None);
        }
        Popup::Rebase { entries, sel, .. } => {
            let r = centered(area, 100, entries.len() as u16 + 5);
            let inner = frame(f, app, r, "Interactive rebase (newest first)", FOCUS);
            let h = inner.height.saturating_sub(2) as usize;
            let off = if *sel >= h { sel + 1 - h } else { 0 };
            for (i, e) in entries.iter().enumerate().skip(off).take(h) {
                let y = inner.y + (i - off) as u16;
                let row = Rect::new(inner.x, y, inner.width, 1);
                if i == *sel {
                    f.buffer_mut()
                        .set_style(row, Style::default().bg(app.theme.selected_bg));
                }
                app.hits.push(row, Hit::TodoRow(i));
                let mut x = inner.x + 1;
                for a in TodoAction::ALL {
                    let on = e.action == a;
                    let label = a.key().to_string();
                    let st = if on {
                        Style::default().fg(Color::Black).bg(action_color(a))
                    } else {
                        Style::default().fg(app.theme.dim)
                    };
                    let br = Rect::new(x, y, 3, 1);
                    let st = if hovered(app, br) && !on {
                        Style::default().fg(Color::Black).bg(Color::Yellow)
                    } else {
                        st
                    };
                    f.buffer_mut().set_string(x, y, format!(" {label} "), st);
                    app.hits.push(br, Hit::TodoAction(i, a));
                    x += 3;
                }
                let msg = match &e.message {
                    Some(m) => format!(" → {}", m.lines().next().unwrap_or("")),
                    None => String::new(),
                };
                let line = Line::from(vec![
                    Span::styled(
                        format!(" {:<6} ", e.action.name()),
                        Style::default()
                            .fg(action_color(e.action))
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        e.oid.to_string()[..8].to_string(),
                        Style::default().fg(Color::Yellow),
                    ),
                    Span::styled(
                        format!(" {}", e.summary),
                        if e.action == TodoAction::Drop {
                            Style::default()
                                .add_modifier(Modifier::CROSSED_OUT)
                                .fg(app.theme.dim)
                        } else {
                            Style::default()
                        },
                    ),
                    Span::styled(msg, Style::default().fg(Color::LightCyan)),
                ]);
                f.buffer_mut()
                    .set_line(x, y, &line, inner.right().saturating_sub(x));
            }
            let hint = "p r e s f d: action · ctrl+↑/↓ or K/J or drag: move · enter: start";
            f.buffer_mut().set_stringn(
                inner.x + 1,
                inner.bottom() - 1,
                hint,
                inner.width as usize,
                Style::default().fg(app.theme.dim),
            );
            let y = r.bottom() - 1;
            let mut x = r.x + 2;
            x += button(
                f,
                app,
                x,
                y,
                "↑",
                Color::Gray,
                false,
                Hit::Button(Btn::TodoUp),
            ) + 1;
            button(
                f,
                app,
                x,
                y,
                "↓",
                Color::Gray,
                false,
                Hit::Button(Btn::TodoDown),
            );
            buttons(f, app, r, "Start rebase", Some("Cancel"));
        }
    }
}

fn action_color(a: TodoAction) -> Color {
    match a {
        TodoAction::Pick => Color::LightGreen,
        TodoAction::Reword => Color::LightCyan,
        TodoAction::Edit => Color::LightYellow,
        TodoAction::Squash | TodoAction::Fixup => Color::LightMagenta,
        TodoAction::Drop => Color::LightRed,
    }
}

pub fn help_lines(app: &App) -> Vec<Line<'static>> {
    let cur = app.ctx_of(app.focus);
    let mut order = vec![cur, Ctx::Global];
    order.extend(
        Ctx::ALL
            .iter()
            .copied()
            .filter(|c| *c != cur && *c != Ctx::Global),
    );
    let mut out = Vec::new();
    for c in order {
        let b = app.keymap.of(c);
        if b.is_empty() {
            continue;
        }
        out.push(Line::styled(
            format!("── {} ──", c.name()),
            Style::default().fg(FOCUS).add_modifier(Modifier::BOLD),
        ));
        for (k, a) in b {
            out.push(Line::from(vec![
                Span::styled(
                    format!("  {:<10}", k.display()),
                    Style::default().fg(Color::LightCyan),
                ),
                Span::raw(a.label()),
            ]));
        }
        out.push(Line::default());
    }
    out.push(Line::styled(
        "── mouse ──",
        Style::default().fg(FOCUS).add_modifier(Modifier::BOLD),
    ));
    for l in [
        "  click          focus panel / select row / tab / button",
        "  double-click   primary action (stage, checkout, open, pick conflict side)",
        "  right-click    actions menu for the item under the cursor",
        "  wheel          scroll the panel under the cursor (shift: horizontal)",
        "  drag           resize panels, select diff lines, reorder rebase todo",
    ] {
        out.push(Line::raw(l));
    }
    out
}
