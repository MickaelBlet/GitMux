//! Rendering. Every clickable element registers itself in `app.hits` while drawing.

pub mod hit;
pub mod icons;
mod popups;

use crate::app::{App, Drag, FileRow, MainKind, PanelId, TextRow};
use crate::git::diff::{self, RowKind};
use crate::git::read::{format_time, relative_time};
use crate::highlight;
use hit::{Btn, Hit};
use icons::Icons;
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, Scrollbar, ScrollbarOrientation, ScrollbarState, StatefulWidget,
};
use unicode_width::UnicodeWidthStr;

pub const FOCUS: Color = Color::LightBlue;
pub const PILL_BG: Color = Color::Indexed(238);
/// Color of a hovered / dragged scrollbar or splitter.
pub const GRAB: Color = Color::Yellow;
const LANE_COLORS: [Color; 6] = [
    Color::LightBlue,
    Color::LightMagenta,
    Color::LightGreen,
    Color::LightYellow,
    Color::LightCyan,
    Color::LightRed,
];

pub fn icons(app: &App) -> &'static Icons {
    if app.nerd_font {
        &icons::NERD
    } else {
        &icons::PLAIN
    }
}

pub fn hovered(app: &App, r: Rect) -> bool {
    app.hover
        .is_some_and(|(x, y)| r.contains(Position::new(x, y)))
}

/// Draw a `[label]` button (or a Nerd Font pill) and register its hit. Returns its width.
pub fn button(
    f: &mut Frame,
    app: &mut App,
    x: u16,
    y: u16,
    label: &str,
    fg: Color,
    active: bool,
    hit: Hit,
) -> u16 {
    let w = label.width() as u16 + 2;
    let r = Rect::new(x, y, w, 1).intersection(f.area());
    let hot = hovered(app, r);
    let buf = f.buffer_mut();
    if app.nerd_font {
        let bg = if hot {
            Color::Yellow
        } else if active {
            FOCUS
        } else {
            PILL_BG
        };
        let fg = if hot || active { Color::Black } else { fg };
        buf.set_string(x, y, icons::PILL_LEFT, Style::default().fg(bg));
        buf.set_string(x + 1, y, label, Style::default().fg(fg).bg(bg));
        buf.set_string(
            x + 1 + label.width() as u16,
            y,
            icons::PILL_RIGHT,
            Style::default().fg(bg),
        );
    } else {
        let style = if hot {
            Style::default().bg(Color::Yellow).fg(Color::Black)
        } else if active {
            Style::default().bg(FOCUS).fg(Color::Black)
        } else {
            Style::default().fg(fg)
        };
        buf.set_string(x, y, format!("[{label}]"), style);
    }
    app.hits.push(r, hit);
    w
}

fn border_style(app: &App, focused: bool) -> Style {
    Style::default().fg(if focused { FOCUS } else { app.theme.border })
}

pub fn draw(f: &mut Frame, app: &mut App) {
    app.hits.clear();
    let area = f.area();
    app.screen = area;
    let [body, bar] = Layout::vertical([Constraint::Min(3), Constraint::Length(1)]).areas(area);
    let left_w = app.left_width.min(body.width.saturating_sub(10)).max(20);
    let [left, right] =
        Layout::horizontal([Constraint::Length(left_w), Constraint::Min(10)]).areas(body);

    // left column: status (fixed) + 4 panels, focused one larger
    let weight = |p: PanelId| {
        if app.focus == p || (app.focus == PanelId::Main && app.last_left == p) {
            3
        } else {
            2
        }
    };
    let rows = Layout::vertical([
        Constraint::Length(3),
        Constraint::Fill(weight(PanelId::Files)),
        Constraint::Fill(weight(PanelId::Branches)),
        Constraint::Fill(weight(PanelId::Commits)),
        Constraint::Fill(weight(PanelId::Stash) - 1),
    ])
    .split(left);
    let mut tip = None;
    for (i, p) in PanelId::LEFT.iter().enumerate() {
        if let Some(h) = draw_list(f, app, *p, rows[i]) {
            tip = Some(h);
        }
    }

    let log_h = if app.show_log {
        app.log_height
            .clamp(3, right.height.saturating_sub(5).max(3))
    } else {
        0
    };
    let [main_area, log_area] =
        Layout::vertical([Constraint::Min(3), Constraint::Length(log_h)]).areas(right);
    draw_main(f, app, main_area);
    if log_h > 0 {
        draw_log(f, app, log_area);
        let r = Rect::new(log_area.x, log_area.y, log_area.width, 1);
        if splitter_hot(app, r, Drag::SplitH) {
            paint_fg(f, r, GRAB);
        }
        app.hits.push(r, Hit::SplitH);
    }
    // vertical splitter on the main panel's left border
    let r = Rect::new(right.x, right.y, 1, right.height);
    if splitter_hot(app, r, Drag::SplitV) {
        paint_fg(f, r, GRAB);
    }
    app.hits.push(r, Hit::SplitV);
    draw_bar(f, app, bar);
    if let Some((idx, row)) = tip
        && app.popups.is_empty()
        && app.drag.is_none()
    {
        draw_commit_tip(f, app, idx, row);
    }
    popups::draw(f, app, area);
}

/// Whether a splitter is being dragged, or hovered with nothing else going on.
fn splitter_hot(app: &App, r: Rect, kind: Drag) -> bool {
    match app.drag {
        Some(d) => std::mem::discriminant(&d) == std::mem::discriminant(&kind),
        None => app.popups.is_empty() && hovered(app, r),
    }
}

/// Recolor the foreground of every cell in `r`.
fn paint_fg(f: &mut Frame, r: Rect, fg: Color) {
    let r = r.intersection(f.area());
    f.buffer_mut().set_style(r, Style::default().fg(fg));
}

/// Tooltip with the full details of the hovered commit, below (or above) its row.
fn draw_commit_tip(f: &mut Frame, app: &App, idx: usize, row: Rect) {
    let Some(c) = app.data.commits.get(idx) else {
        return;
    };
    let Some((mx, _)) = app.hover else { return };
    let area = f.area();
    let key = Style::default().fg(app.theme.dim);
    let field = |k: &'static str, v: String, st: Style| {
        Line::from(vec![
            Span::styled(format!("{k:<8}"), key),
            Span::styled(v, st),
        ])
    };
    let mut lines = vec![
        field(
            "Commit",
            c.oid.to_string(),
            Style::default().fg(Color::Yellow),
        ),
        field(
            "Author",
            if c.email.is_empty() {
                c.author.clone()
            } else {
                format!("{} <{}>", c.author, c.email)
            },
            Style::default(),
        ),
        field(
            "Date",
            format!(
                "{} ({} ago)",
                format_time(c.time, c.tz),
                relative_time(c.time)
            ),
            Style::default(),
        ),
    ];
    if !c.refs.is_empty() {
        lines.push(field(
            "Refs",
            c.refs.join(", "),
            Style::default().fg(app.theme.accent),
        ));
    }
    if !c.parents.is_empty() {
        let p: Vec<String> = c
            .parents
            .iter()
            .map(|o| o.to_string()[..8].to_string())
            .collect();
        lines.push(field(
            "Parents",
            p.join(" "),
            Style::default().fg(Color::Yellow),
        ));
    }
    lines.push(Line::default());
    const MAX_MSG: usize = 20;
    let msg: Vec<&str> = c.message.lines().collect();
    for (i, l) in msg.iter().take(MAX_MSG).enumerate() {
        let st = if i == 0 {
            Style::default().add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        lines.push(Line::from(Span::styled(l.to_string(), st)));
    }
    if msg.len() > MAX_MSG {
        lines.push(Line::from(Span::styled("…", key)));
    }

    let content_w = lines.iter().map(|l| l.width()).max().unwrap_or(0) as u16;
    let w = (content_w + 4).min(area.width.saturating_sub(2)).min(100);
    let h = (lines.len() as u16 + 2).min(area.height);
    if w < 10 || h < 3 {
        return;
    }
    let x = mx.saturating_add(2).min(area.right().saturating_sub(w));
    let y = if row.bottom() + h <= area.bottom() {
        row.bottom()
    } else if row.y >= area.y + h {
        row.y - h
    } else {
        area.bottom().saturating_sub(h)
    };
    let r = Rect::new(x, y, w, h);
    f.render_widget(ratatui::widgets::Clear, r);
    let b = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(app.theme.dim));
    let inner = b.inner(r);
    f.render_widget(b, r);
    let buf = f.buffer_mut();
    for (i, l) in lines.iter().enumerate().take(inner.height as usize) {
        buf.set_line(
            inner.x + 1,
            inner.y + i as u16,
            l,
            inner.width.saturating_sub(2),
        );
    }
}

fn tab_title(f: &mut Frame, app: &mut App, p: PanelId, area: Rect, focused: bool) {
    let num = match p {
        PanelId::Status => "1",
        PanelId::Files => "2",
        PanelId::Branches => "3",
        PanelId::Commits => "4",
        PanelId::Stash => "5",
        _ => "",
    };
    let mut x = area.x + 1;
    let buf = f.buffer_mut();
    let num_s = format!("[{num}]");
    buf.set_string(x, area.y, &num_s, border_style(app, focused));
    x += num_s.width() as u16;
    let cur = app.tabs[p.index()];
    for (i, t) in p.tabs().iter().enumerate() {
        if x + 2 >= area.right() {
            break;
        }
        let label = format!(" {t} ");
        let active = i == cur;
        let r = Rect::new(x, area.y, label.width() as u16, 1).intersection(area);
        let style = if active {
            Style::default()
                .fg(if focused { FOCUS } else { Color::White })
                .add_modifier(Modifier::BOLD)
        } else if hovered(app, r) {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::Gray)
        };
        f.buffer_mut()
            .set_stringn(x, area.y, &label, (area.right() - x - 1) as usize, style);
        app.hits.push(r, Hit::Tab(p, i));
        x += label.width() as u16;
        if i + 1 < p.tabs().len() {
            f.buffer_mut()
                .set_string(x, area.y, "│", border_style(app, focused));
            x += 1;
        }
    }
}

fn panel_block(app: &App, focused: bool) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style(app, focused))
}

fn scrollbar(
    f: &mut Frame,
    app: &mut App,
    p: PanelId,
    area: Rect,
    len: usize,
    pos: usize,
    view: usize,
    focused: bool,
) {
    let style = border_style(app, focused);
    scrollbar_with(f, app, Hit::Scrollbar(p), style, area, len, pos, view);
}

/// Vertical scrollbar on the right border of `area`, clickable as `hit`.
#[allow(clippy::too_many_arguments)]
pub(super) fn scrollbar_with(
    f: &mut Frame,
    app: &mut App,
    hit: Hit,
    style: Style,
    area: Rect,
    len: usize,
    pos: usize,
    view: usize,
) {
    if len <= view || area.height < 3 {
        return;
    }
    let r = Rect::new(area.right() - 1, area.y + 1, 1, area.height - 2);
    let dragged = match app.drag {
        Some(Drag::Scroll(_, d) | Drag::PopupScroll(d, _)) => d == r,
        _ => false,
    };
    // panel scrollbars are under any open popup
    let reachable = !matches!(hit, Hit::Scrollbar(_)) || app.popups.is_empty();
    let style = if dragged || (app.drag.is_none() && reachable && hovered(app, r)) {
        style.fg(GRAB)
    } else {
        style
    };
    let mut st = ScrollbarState::new(len.saturating_sub(view))
        .position(pos)
        .viewport_content_length(view);
    Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .begin_symbol(None)
        .end_symbol(None)
        .track_symbol(Some("│"))
        .thumb_symbol("┃")
        .style(style)
        .render(r, f.buffer_mut(), &mut st);
    app.hits.push(r, hit);
}

/// Returns the hovered commit (index, row rect) of the commit log, if any.
fn draw_list(f: &mut Frame, app: &mut App, p: PanelId, area: Rect) -> Option<(usize, Rect)> {
    let focused = app.focus == p;
    let block = panel_block(app, focused);
    let inner = block.inner(area);
    f.render_widget(block, area);
    app.hits.push(area, Hit::Panel(p));
    tab_title(f, app, p, area, focused);

    let vis = app.visible(p);
    let h = inner.height as usize;
    let file_rows = if p == PanelId::Files {
        app.file_rows()
    } else {
        vec![]
    };
    {
        let l = app.list_mut(p);
        l.height = h;
        if l.sel < l.off {
            l.off = l.sel;
        } else if h > 0 && l.sel >= l.off + h {
            l.off = l.sel + 1 - h;
        }
        l.off = l.off.min(vis.len().saturating_sub(h.max(1)));
    }
    let (sel, off, filter) = {
        let l = app.list(p);
        (l.sel, l.off, l.filter.clone())
    };
    let commit_log = p == PanelId::Commits && app.tabs[p.index()] == 0;
    let mut tip = None;
    for (row, &idx) in vis.iter().enumerate().skip(off).take(h) {
        let y = inner.y + (row - off) as u16;
        let r = Rect::new(inner.x, y, inner.width, 1);
        let line = row_line(app, p, idx, &file_rows);
        let buf = f.buffer_mut();
        if row == sel {
            let bg = if focused {
                app.theme.selected_bg
            } else {
                Color::Indexed(236)
            };
            buf.set_style(r, Style::default().bg(bg));
        } else if hovered(app, r) {
            buf.set_style(r, Style::default().bg(Color::Indexed(235)));
        }
        buf.set_line(r.x, r.y, &line, r.width);
        app.hits.push(r, Hit::Row(p, row));
        if commit_log && hovered(app, r) {
            tip = Some((idx, r));
        }
    }
    if vis.is_empty() && h > 0 {
        let msg = if filter.is_empty() {
            empty_msg(app, p)
        } else {
            "no match"
        };
        f.buffer_mut().set_stringn(
            inner.x + 1,
            inner.y,
            msg,
            inner.width as usize,
            Style::default().fg(app.theme.dim),
        );
    }
    // footer: filter + position
    let mut footer = String::new();
    if !filter.is_empty() {
        footer += &format!(" /{filter} ");
    }
    if !vis.is_empty() && p != PanelId::Status {
        footer += &format!(" {} of {} ", sel + 1, vis.len());
    }
    if !footer.is_empty() && area.width as usize > footer.width() + 2 {
        let x = area.right() - 1 - footer.width() as u16;
        f.buffer_mut()
            .set_string(x, area.bottom() - 1, &footer, border_style(app, focused));
    }
    scrollbar(f, app, p, area, vis.len(), off, h, focused);
    tip
}

fn empty_msg(app: &App, p: PanelId) -> &'static str {
    match (p, app.tabs[p.index()]) {
        (PanelId::Files, _) => "working tree clean",
        (PanelId::Branches, 3) => "no remotes (n: add)",
        (PanelId::Stash, 0) => "no stash entries",
        (PanelId::Stash, 1) => "no submodules (n: add)",
        _ => "empty",
    }
}

fn status_style(c: char, staged: bool, app: &App) -> Style {
    match c {
        ' ' => Style::default(),
        '?' => Style::default().fg(app.theme.removed),
        'U' => Style::default()
            .fg(Color::LightRed)
            .add_modifier(Modifier::BOLD),
        _ if staged => Style::default().fg(app.theme.added),
        _ => Style::default().fg(app.theme.removed),
    }
}

fn row_line(app: &App, p: PanelId, idx: usize, file_rows: &[FileRow]) -> Line<'static> {
    let ic = icons(app);
    let d = &app.data;
    let dim = Style::default().fg(app.theme.dim);
    let t = app.tabs[p.index()];
    match (p, t) {
        (PanelId::Status, _) => {
            let name = d
                .workdir
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let mut s = vec![
                Span::styled(name, Style::default().fg(Color::LightCyan)),
                Span::raw(" → "),
                Span::styled(
                    format!("{}{}", ic.branch, d.head),
                    Style::default()
                        .fg(app.theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
            ];
            if d.upstream.is_some() {
                s.push(Span::styled(
                    format!(" {}{} {}{}", ic.ahead, d.ahead, ic.behind, d.behind),
                    Style::default().fg(Color::Yellow),
                ));
            }
            if !d.state.is_empty() {
                s.push(Span::styled(
                    format!(" [{}]", d.state),
                    Style::default()
                        .fg(Color::LightRed)
                        .add_modifier(Modifier::BOLD),
                ));
            }
            Line::from(s)
        }
        (PanelId::Files, _) => match &file_rows[idx] {
            FileRow::Dir {
                path,
                depth,
                collapsed,
            } => {
                let name = path.rsplit('/').next().unwrap_or(path);
                let prefix = " ".repeat(depth * 2);
                let icon = if *collapsed {
                    ic.dir_closed
                } else {
                    ic.dir_open
                };
                // aggregate status
                let pre = format!("{path}/");
                let files: Vec<_> = d
                    .files
                    .iter()
                    .filter(|f| f.path.starts_with(&pre))
                    .collect();
                let st = if files.iter().all(|f| f.has_staged() && !f.has_unstaged()) {
                    Style::default().fg(app.theme.added)
                } else {
                    Style::default().fg(app.theme.removed)
                };
                Line::from(vec![
                    Span::raw(prefix),
                    Span::styled(format!("{icon}{name}/"), st),
                ])
            }
            FileRow::File { idx, depth } => {
                let f = &d.files[*idx];
                let prefix = " ".repeat(depth * 2);
                let name = if app.tree_view {
                    f.path.rsplit('/').next().unwrap_or(&f.path).to_string()
                } else {
                    f.path.clone()
                };
                let name = match &f.orig {
                    Some(o) => format!("{o} → {name}"),
                    None => name,
                };
                let icon = if f.conflicted { ic.conflict } else { ic.file };
                let name_style = if f.conflicted {
                    Style::default().fg(Color::LightRed)
                } else if !f.has_unstaged() {
                    Style::default().fg(app.theme.added)
                } else {
                    Style::default()
                };
                Line::from(vec![
                    Span::raw(prefix),
                    Span::styled(f.x.to_string(), status_style(f.x, true, app)),
                    Span::styled(f.y.to_string(), status_style(f.y, false, app)),
                    Span::raw(" "),
                    Span::styled(format!("{icon}{name}"), name_style),
                ])
            }
        },
        (PanelId::Branches, 0) => {
            let b = &d.branches[idx];
            let mut s = vec![
                Span::styled(
                    if b.is_head { ic.head } else { "  " },
                    Style::default().fg(app.theme.accent),
                ),
                Span::styled(
                    format!("{}{}", ic.branch, b.name),
                    if b.is_head {
                        Style::default()
                            .fg(app.theme.accent)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    },
                ),
            ];
            if b.upstream.is_some() {
                let sync = if b.ahead == 0 && b.behind == 0 {
                    " ✓".to_string()
                } else {
                    format!(" {}{} {}{}", ic.ahead, b.ahead, ic.behind, b.behind)
                };
                s.push(Span::styled(sync, Style::default().fg(Color::Yellow)));
            }
            s.push(Span::styled(format!(" {}", b.summary), dim));
            Line::from(s)
        }
        (PanelId::Branches, 1) => {
            let b = &d.remote_branches[idx];
            Line::from(vec![
                Span::styled(
                    format!("{}{}", ic.remote_branch, b.name),
                    Style::default().fg(Color::LightRed),
                ),
                Span::styled(format!(" {}", b.summary), dim),
            ])
        }
        (PanelId::Branches, 2) => {
            let tg = &d.tags[idx];
            Line::from(vec![
                Span::styled(
                    format!("{}{}", ic.tag, tg.name),
                    Style::default().fg(Color::Yellow),
                ),
                Span::styled(format!(" {}", tg.summary), dim),
            ])
        }
        (PanelId::Branches, _) => {
            let r = &d.remotes[idx];
            Line::from(vec![
                Span::styled(
                    format!("{}{}", ic.remote, r.name),
                    Style::default().fg(Color::LightCyan),
                ),
                Span::styled(format!(" {}", r.url), dim),
            ])
        }
        (PanelId::Commits, 0) => {
            let c = &d.commits[idx];
            let marked = app.cherry_marked(c.oid);
            let mut s = vec![
                Span::styled(
                    if marked {
                        format!("{}{}", ic.cherry, &c.short[..7])
                    } else {
                        c.short.clone()
                    },
                    if marked {
                        Style::default().fg(Color::Black).bg(Color::LightCyan)
                    } else {
                        Style::default().fg(Color::Yellow)
                    },
                ),
                Span::styled(format!(" {:>3} ", relative_time(c.time)), dim),
            ];
            if let Some(g) = d.graph.get(idx) {
                for (i, ch) in g.chars().enumerate() {
                    s.push(Span::styled(
                        ch.to_string(),
                        Style::default().fg(LANE_COLORS[(i / 2) % LANE_COLORS.len()]),
                    ));
                }
            }
            for r in &c.refs {
                let st = if r.starts_with("tag:") {
                    Style::default().fg(Color::Yellow)
                } else if *r == d.head {
                    Style::default()
                        .fg(app.theme.accent)
                        .add_modifier(Modifier::BOLD)
                } else if d.remote_branches.iter().any(|b| b.name == *r) {
                    Style::default().fg(Color::LightRed)
                } else {
                    Style::default().fg(app.theme.accent)
                };
                s.push(Span::styled(format!("({r}) "), st));
            }
            s.push(Span::raw(c.summary.clone()));
            s.push(Span::styled(format!("  {}", c.author), dim));
            Line::from(s)
        }
        (PanelId::Commits, _) => {
            let r = &d.reflog[idx];
            Line::from(vec![
                Span::styled(
                    r.oid.to_string()[..8].to_string(),
                    Style::default().fg(Color::Yellow),
                ),
                Span::styled(format!(" {} ", r.selector), dim),
                Span::raw(r.message.clone()),
            ])
        }
        (PanelId::Stash, 0) => {
            let s = &d.stashes[idx];
            Line::from(vec![
                Span::styled(format!("{}stash@{{{}}} ", ic.stash, s.index), dim),
                Span::raw(s.message.clone()),
            ])
        }
        (PanelId::Stash, 1) => {
            let s = &d.submodules[idx];
            let st = match s.status.as_str() {
                "ok" => Style::default().fg(app.theme.added),
                "modified" => Style::default().fg(Color::Yellow),
                _ => Style::default().fg(app.theme.removed),
            };
            Line::from(vec![
                Span::raw(format!("{}{} ", ic.submodule, s.name)),
                Span::styled(s.status.clone(), st),
                Span::styled(format!(" {}", s.path.display()), dim),
            ])
        }
        (PanelId::Stash, _) => {
            let w = &d.worktrees[idx];
            Line::from(vec![
                Span::styled(
                    if w.current { ic.head } else { "  " },
                    Style::default().fg(app.theme.accent),
                ),
                Span::raw(format!("{}{} ", ic.worktree, w.path.display())),
                Span::styled(w.branch.clone(), Style::default().fg(app.theme.accent)),
            ])
        }
        _ => Line::default(),
    }
}

fn diff_style(kind: RowKind, app: &App) -> Style {
    highlight::row_style(kind, &app.theme)
}

/// Width of the line-number gutter for a diff.
fn gutter_width(rows: &[diff::Row]) -> usize {
    rows.iter()
        .filter_map(|r| r.old_no.max(r.new_no))
        .max()
        .map_or(1, |n| n.to_string().len())
        .max(3)
}

fn line_no(n: Option<usize>, w: usize) -> String {
    n.map_or_else(|| " ".repeat(w), |n| format!("{n:>w$}"))
}

fn clip(s: &str, h: usize) -> String {
    s.replace('\t', "    ").chars().skip(h).collect()
}

/// One side-by-side diff row: headers span the full width, otherwise old | new.
#[allow(clippy::too_many_arguments)]
fn draw_pair(
    buf: &mut Buffer,
    app: &App,
    rows: &[diff::Row],
    (l, r): (Option<usize>, Option<usize>),
    area: Rect,
    nw: usize,
    hs: usize,
    sel_bg: impl Fn(usize) -> Option<Color>,
) {
    let y = area.y;
    if let (Some(li), true) = (l, l == r)
        && matches!(rows[li].kind, RowKind::Header | RowKind::HunkHeader)
    {
        let row = &rows[li];
        let mut st = diff_style(row.kind, app);
        if let Some(bg) = sel_bg(li) {
            st = st.bg(bg);
            buf.set_style(area, st);
        }
        buf.set_stringn(area.x, y, clip(&row.text, hs), area.width as usize, st);
        return;
    }
    let half = area.width / 2;
    for (side, left, x, w) in [
        (l, true, area.x, half.saturating_sub(1)),
        (r, false, area.x + half, area.width - half),
    ] {
        let Some(ri) = side else {
            buf.set_style(
                Rect::new(x, y, w, 1),
                Style::default().bg(Color::Indexed(234)),
            );
            continue;
        };
        let row = &rows[ri];
        let bg = sel_bg(ri).or_else(|| highlight::row_bg(row.kind, &app.theme));
        buf.set_style(
            Rect::new(x, y, w, 1),
            Style::default().bg(bg.unwrap_or(Color::Reset)),
        );
        let no = if left { row.old_no } else { row.new_no };
        let mut spans = vec![Span::styled(
            format!("{} ", line_no(no, nw)),
            Style::default().fg(app.theme.dim),
        )];
        spans.extend(highlight::clip_spans(
            highlight::row_spans(row, &app.theme),
            hs,
        ));
        buf.set_line(x, y, &Line::from(spans), w);
    }
    buf.set_string(
        area.x + half - 1,
        y,
        "│",
        Style::default().fg(app.theme.border),
    );
}

/// Pin the file of the diff section scrolled past on the first row of a text view.
fn draw_sticky(f: &mut Frame, app: &mut App, inner: Rect) {
    if inner.height < 2 {
        return;
    }
    let top = app.main.text_line(app.main.scroll);
    let Some(sec) = app.main.section_at(top) else {
        return;
    };
    if top == sec.start {
        return;
    }
    let (start, path) = (sec.start, sec.path.clone());
    let r = Rect::new(inner.x, inner.y, inner.width, 1);
    let st = Style::default()
        .fg(app.theme.accent)
        .bg(Color::Indexed(237))
        .add_modifier(Modifier::BOLD);
    let buf = f.buffer_mut();
    // overwrite the whole row: the line underneath must not show through
    buf.set_string(r.x, r.y, " ".repeat(r.width as usize), st);
    buf.set_stringn(
        r.x,
        r.y,
        format!(" {}{}", icons(app).file, path),
        r.width as usize,
        st,
    );
    app.hits.push(r, Hit::Button(Btn::StickyFile(start)));
}

fn draw_main(f: &mut Frame, app: &mut App, area: Rect) {
    let sbs = app.side_by_side;
    app.main.set_sbs(sbs);
    let focused = app.focus == PanelId::Main;
    let block = panel_block(app, focused);
    let inner = block.inner(area);
    f.render_widget(block, area);
    app.main_rect = inner;
    app.hits.push(area, Hit::Panel(PanelId::Main));
    let title = format!("[0] {} ", app.main.title);
    f.buffer_mut().set_stringn(
        area.x + 1,
        area.y,
        &title,
        area.width.saturating_sub(2) as usize,
        border_style(app, focused).add_modifier(Modifier::BOLD),
    );

    // title buttons (right-aligned)
    let mut btns: Vec<(String, Hit, bool)> = vec![];
    if !app.main_stack.is_empty() {
        btns.push(("← back".into(), Hit::Button(Btn::Back), false));
    }
    match &app.main.kind {
        MainKind::Diff {
            staged, untracked, ..
        } => {
            if !*untracked {
                btns.push((
                    if *staged { "staged" } else { "unstaged" }.into(),
                    Hit::Button(Btn::StageView),
                    false,
                ));
            }
            btns.push((
                "side-by-side".into(),
                Hit::Button(Btn::SideBySide),
                app.side_by_side,
            ));
            btns.push(("word".into(), Hit::Button(Btn::WordDiff), app.word_diff));
            if !*untracked {
                btns.push(("full".into(), Hit::Button(Btn::FullFile), app.full_file));
            }
        }
        MainKind::Text if !app.main.sections.is_empty() => {
            btns.push((
                "side-by-side".into(),
                Hit::Button(Btn::SideBySide),
                app.side_by_side,
            ));
            btns.push(("word".into(), Hit::Button(Btn::WordDiff), app.word_diff));
            if app.main.rev.is_some() {
                btns.push(("full".into(), Hit::Button(Btn::FullFile), app.full_file));
            }
        }
        MainKind::Conflict { .. } => {
            btns.push((
                "ours".into(),
                Hit::Button(Btn::Conflict(diff::Pick::Ours)),
                false,
            ));
            btns.push((
                "theirs".into(),
                Hit::Button(Btn::Conflict(diff::Pick::Theirs)),
                false,
            ));
            btns.push((
                "both".into(),
                Hit::Button(Btn::Conflict(diff::Pick::Both)),
                false,
            ));
        }
        _ => {}
    }
    let total: u16 = btns.iter().map(|(l, _, _)| l.width() as u16 + 3).sum();
    if total + title.width() as u16 + 4 < area.width {
        let mut x = area.right() - 1 - total;
        for (l, h, active) in btns {
            x += button(f, app, x, area.y, &l, Color::LightYellow, active, h) + 1;
        }
    }

    app.main.height = inner.height as usize;
    let h = inner.height as usize;
    let hs = app.main.hscroll;
    let (sel_a, sel_b) = app.main.selection();
    let cursor_bg = if focused {
        app.theme.selected_bg
    } else {
        Color::Indexed(236)
    };
    let range_bg = Color::Indexed(238);
    let has_cursor = app.main.has_cursor();
    let len = app.main.len();
    if has_cursor && app.main.scroll + h > len.max(h) {
        app.main.scroll = len.saturating_sub(h);
    }

    // side-by-side diff
    if let (MainKind::Diff { rows, .. }, true) = (&app.main.kind, app.side_by_side) {
        let pairs = diff::side_by_side(rows);
        let cur = app.main.cursor;
        let vis_cur = pairs
            .iter()
            .position(|(l, r)| *l == Some(cur) || *r == Some(cur))
            .unwrap_or(0);
        let start = if vis_cur >= h { vis_cur + 1 - h } else { 0 }.max(
            vis_cur
                .saturating_sub(h / 2)
                .min(pairs.len().saturating_sub(h)),
        );
        let nw = gutter_width(rows);
        let mut hits = vec![];
        let sel_bg = |i: usize| {
            (i >= sel_a && i <= sel_b).then_some(if i == cur { cursor_bg } else { range_bg })
        };
        for (i, (l, r)) in pairs.iter().enumerate().skip(start).take(h) {
            let row = Rect::new(inner.x, inner.y + (i - start) as u16, inner.width, 1);
            draw_pair(f.buffer_mut(), app, rows, (*l, *r), row, nw, hs, sel_bg);
            hits.push((row, l.or(*r).unwrap_or(0)));
        }
        for (r, i) in hits {
            app.hits.push(r, Hit::MainLine(i));
        }
        scrollbar(f, app, PanelId::Main, area, pairs.len(), start, h, focused);
        return;
    }

    // side-by-side commit / stash text
    if app.main.sbs_text() {
        let scroll = app.main.scroll;
        let mut hits = vec![];
        let gutters: Vec<usize> = app
            .main
            .sections
            .iter()
            .map(|s| gutter_width(&s.rows))
            .collect();
        for (i, tr) in app.main.pairs.iter().enumerate().skip(scroll).take(h) {
            let r = Rect::new(inner.x, inner.y + (i - scroll) as u16, inner.width, 1);
            let buf = f.buffer_mut();
            match *tr {
                TextRow::Line(li) => {
                    let mut line = app.main.text[li].clone();
                    if let Some(bg) = line.style.bg.take() {
                        buf.set_style(r, Style::default().bg(bg));
                    }
                    line.spans = highlight::clip_spans(line.spans, hs);
                    buf.set_line(r.x, r.y, &line, r.width);
                }
                TextRow::Pair(si, l, rr) => {
                    let rows = &app.main.sections[si].rows;
                    draw_pair(buf, app, rows, (l, rr), r, gutters[si], hs, |_| None);
                }
            }
            hits.push((r, app.main.text_line(i)));
        }
        for (r, i) in hits {
            app.hits.push(r, Hit::MainLine(i));
        }
        draw_sticky(f, app, inner);
        scrollbar(f, app, PanelId::Main, area, len, scroll, h, focused);
        return;
    }

    let scroll = app.main.scroll;
    let cursor = app.main.cursor;
    let mut lines: Vec<(Line<'static>, Style)> = Vec::with_capacity(h);
    match &app.main.kind {
        MainKind::Empty => {}
        MainKind::Text => {
            let dim = Style::default().fg(app.theme.dim);
            let gutters: Vec<usize> = app
                .main
                .sections
                .iter()
                .map(|s| gutter_width(&s.rows))
                .collect();
            for (i, l) in app.main.text.iter().enumerate().skip(scroll).take(h) {
                let mut l = l.clone();
                l.spans = highlight::clip_spans(l.spans, hs);
                // diff lines get the old / new line-number gutter
                if let Some((si, sec)) = app
                    .main
                    .sections
                    .iter()
                    .enumerate()
                    .find(|(_, s)| i >= s.start && i < s.start + s.rows.len())
                {
                    let (r, nw) = (&sec.rows[i - sec.start], gutters[si]);
                    l.spans.insert(
                        0,
                        Span::styled(
                            format!("{} {} │", line_no(r.old_no, nw), line_no(r.new_no, nw)),
                            dim,
                        ),
                    );
                }
                lines.push((l, Style::default()));
            }
        }
        MainKind::Diff { rows, .. } => {
            let nw = gutter_width(rows);
            let dim = Style::default().fg(app.theme.dim);
            for r in rows.iter().skip(scroll).take(h) {
                let l = highlight::row_line(r, &app.theme);
                let mut spans = vec![Span::styled(
                    format!("{} {} │", line_no(r.old_no, nw), line_no(r.new_no, nw)),
                    dim,
                )];
                spans.extend(highlight::clip_spans(l.spans, hs));
                lines.push((Line::from(spans).style(l.style), Style::default()));
            }
        }
        MainKind::Conflict {
            lines: ls,
            conflicts,
            ..
        } => {
            for (i, l) in ls.iter().enumerate().skip(scroll).take(h) {
                let c = conflicts.iter().find(|c| i >= c.start && i <= c.end);
                let st = match c {
                    Some(c) if i == c.start || i == c.mid || i == c.end || Some(i) == c.base => {
                        Style::default()
                            .fg(Color::LightMagenta)
                            .add_modifier(Modifier::BOLD)
                    }
                    Some(c) if c.base.is_some_and(|b| i > b) && i < c.mid => {
                        Style::default().fg(app.theme.dim)
                    }
                    Some(c) if i < c.mid => Style::default().fg(app.theme.added),
                    Some(_) => Style::default().fg(Color::LightBlue),
                    None => Style::default(),
                };
                lines.push((Line::styled(clip(l, hs), st), Style::default()));
            }
        }
        MainKind::Blame { lines: ls, .. } => {
            let mut last = git2::Oid::ZERO_SHA1;
            for (i, (oid, short, author, text)) in ls.iter().enumerate().skip(scroll).take(h) {
                let same = i > scroll && *oid == last;
                last = *oid;
                let color = LANE_COLORS[(oid.as_bytes()[0] as usize) % LANE_COLORS.len()];
                let meta = if same {
                    format!("{:8} {:12}", "", "")
                } else {
                    format!("{short} {:12.12}", author)
                };
                lines.push((
                    Line::from(vec![
                        Span::styled(meta, Style::default().fg(color)),
                        Span::styled(
                            format!(" {:>5} │ ", i + 1),
                            Style::default().fg(app.theme.dim),
                        ),
                        Span::raw(clip(text, hs)),
                    ]),
                    Style::default(),
                ));
            }
        }
        MainKind::History { commits, .. } => {
            for c in commits.iter().skip(scroll).take(h) {
                lines.push((
                    Line::from(vec![
                        Span::styled(c.short.clone(), Style::default().fg(Color::Yellow)),
                        Span::styled(
                            format!(" {:>3} {:12.12} ", relative_time(c.time), c.author),
                            Style::default().fg(app.theme.dim),
                        ),
                        Span::raw(c.summary.clone()),
                    ]),
                    Style::default(),
                ));
            }
        }
    }
    for (i, (mut line, _)) in lines.into_iter().enumerate() {
        let y = inner.y + i as u16;
        let idx = scroll + i;
        let r = Rect::new(inner.x, y, inner.width, 1);
        let buf: &mut Buffer = f.buffer_mut();
        // full-width line background; the selection wins over the diff tint
        let line_bg = line.style.bg.take();
        let bg = if has_cursor && idx >= sel_a && idx <= sel_b {
            Some(if idx == cursor { cursor_bg } else { range_bg })
        } else {
            line_bg
        };
        if let Some(bg) = bg {
            buf.set_style(r, Style::default().bg(bg));
        }
        buf.set_line(r.x, r.y, &line, r.width);
        app.hits.push(r, Hit::MainLine(idx));
    }
    if matches!(app.main.kind, MainKind::Text) {
        draw_sticky(f, app, inner);
    }
    if len == 0 && h > 0 {
        f.buffer_mut().set_string(
            inner.x + 1,
            inner.y,
            "nothing to show",
            Style::default().fg(app.theme.dim),
        );
    }
    scrollbar(f, app, PanelId::Main, area, len, scroll, h, focused);
}

fn draw_log(f: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == PanelId::Log;
    let block = panel_block(app, focused).title(" Command log ");
    let inner = block.inner(area);
    f.render_widget(block, area);
    app.hits.push(area, Hit::Panel(PanelId::Log));
    let log = app.log.lock().map(|l| l.clone()).unwrap_or_default();
    let h = inner.height as usize;
    app.log_scroll = app.log_scroll.min(log.len().saturating_sub(h));
    let end = log.len().saturating_sub(app.log_scroll);
    let start = end.saturating_sub(h);
    for (i, l) in log[start..end].iter().enumerate() {
        let (mark, st) = if l.ok {
            ("✓ ", Style::default().fg(app.theme.added))
        } else {
            ("✗ ", Style::default().fg(app.theme.removed))
        };
        let line = Line::from(vec![
            Span::styled(mark, st),
            Span::styled(l.cmd.clone(), Style::default().fg(Color::Gray)),
        ]);
        f.buffer_mut()
            .set_line(inner.x, inner.y + i as u16, &line, inner.width);
    }
}

fn draw_bar(f: &mut Frame, app: &mut App, area: Rect) {
    f.buffer_mut()
        .set_style(area, Style::default().bg(Color::Indexed(234)));
    // right side: jobs + message
    let mut right = String::new();
    if app.jobs > 0 {
        let sp = icons(app).spinner;
        right += &format!("{} {} running ", sp[app.spinner % sp.len()], app.jobs);
    }
    if let Some((m, _, _)) = &app.message {
        right += m;
        right.push(' ');
    }
    let rw = (right.width() as u16).min(area.width / 2);
    let rx = area.right() - rw;
    f.buffer_mut().set_stringn(
        rx,
        area.y,
        &right,
        rw as usize,
        Style::default()
            .fg(Color::LightCyan)
            .bg(Color::Indexed(234)),
    );

    // left side: clickable hints
    let ctx = app.ctx_of(app.focus);
    let mut hints: Vec<(String, crate::action::Action)> = app
        .keymap
        .of(ctx)
        .into_iter()
        .take(9)
        .map(|(k, a)| (format!("{} {}", k.display(), a.label()), a))
        .collect();
    hints.push(("x menu".into(), crate::action::Action::ContextMenu));
    hints.push(("? help".into(), crate::action::Action::Help));
    let mut x = area.x;
    for (label, a) in hints {
        let w = label.width() as u16 + 2;
        if x + w >= rx {
            break;
        }
        x += button(f, app, x, area.y, &label, Color::Gray, false, Hit::Hint(a)) + 1;
    }
}
