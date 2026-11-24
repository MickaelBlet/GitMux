//! Rendering. Every clickable element registers itself in `app.hits` while drawing.

pub mod hit;
pub mod icons;
mod popups;

use crate::app::{App, FileRow, MainKind, PanelId};
use crate::git::diff::{self, RowKind};
use crate::git::read::relative_time;
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
    let left_w = (body.width as u32 * app.left_pct as u32 / 100) as u16;
    let [left, right] =
        Layout::horizontal([Constraint::Length(left_w.max(20)), Constraint::Min(10)]).areas(body);

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
    for (i, p) in PanelId::LEFT.iter().enumerate() {
        draw_list(f, app, *p, rows[i]);
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
        app.hits.push(
            Rect::new(log_area.x, log_area.y, log_area.width, 1),
            Hit::SplitH,
        );
    }
    // vertical splitter on the main panel's left border
    app.hits
        .push(Rect::new(right.x, right.y, 1, right.height), Hit::SplitV);
    draw_bar(f, app, bar);
    popups::draw(f, app, area);
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
    if len <= view || area.height < 3 {
        return;
    }
    let r = Rect::new(area.right() - 1, area.y + 1, 1, area.height - 2);
    let mut st = ScrollbarState::new(len.saturating_sub(view))
        .position(pos)
        .viewport_content_length(view);
    Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .begin_symbol(None)
        .end_symbol(None)
        .track_symbol(Some("│"))
        .thumb_symbol("┃")
        .style(border_style(app, focused))
        .render(
            Rect::new(area.right() - 1, area.y + 1, 1, area.height - 2),
            f.buffer_mut(),
            &mut st,
        );
    app.hits.push(r, Hit::Scrollbar(p));
}

fn draw_list(f: &mut Frame, app: &mut App, p: PanelId, area: Rect) {
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
    match kind {
        RowKind::Header => Style::default().add_modifier(Modifier::BOLD),
        RowKind::HunkHeader => Style::default().fg(app.theme.hunk),
        RowKind::Add => Style::default().fg(app.theme.added),
        RowKind::Del => Style::default().fg(app.theme.removed),
        RowKind::Context => Style::default(),
        RowKind::NoNewline => Style::default().fg(app.theme.dim),
    }
}

fn clip(s: &str, h: usize) -> String {
    s.replace('\t', "    ").chars().skip(h).collect()
}

fn draw_main(f: &mut Frame, app: &mut App, area: Rect) {
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
        }
        MainKind::Text if app.word_diff && app.main.title.contains("staged") => {
            btns.push(("word".into(), Hit::Button(Btn::WordDiff), true));
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
        let half = inner.width / 2;
        let mut hits = vec![];
        for (i, (l, r)) in pairs.iter().enumerate().skip(start).take(h) {
            let y = inner.y + (i - start) as u16;
            let buf = f.buffer_mut();
            // file / hunk headers span the full width
            if let (Some(li), true) = (l, l == r)
                && matches!(rows[*li].kind, RowKind::Header | RowKind::HunkHeader)
            {
                let row = &rows[*li];
                let mut st = diff_style(row.kind, app);
                if *li >= sel_a && *li <= sel_b {
                    st = st.bg(if *li == cur { cursor_bg } else { range_bg });
                    buf.set_style(Rect::new(inner.x, y, inner.width, 1), st);
                }
                buf.set_stringn(inner.x, y, clip(&row.text, hs), inner.width as usize, st);
                hits.push((Rect::new(inner.x, y, inner.width, 1), *li));
                continue;
            }
            for (side, x, w) in [
                (l, inner.x, half.saturating_sub(1)),
                (r, inner.x + half, inner.width - half),
            ] {
                let Some(ri) = side else {
                    buf.set_style(
                        Rect::new(x, y, w, 1),
                        Style::default().bg(Color::Indexed(234)),
                    );
                    continue;
                };
                let row = &rows[*ri];
                let mut st = diff_style(row.kind, app);
                if *ri >= sel_a && *ri <= sel_b {
                    st = st.bg(if *ri == cur { cursor_bg } else { range_bg });
                }
                let text = match row.kind {
                    RowKind::Add | RowKind::Del | RowKind::Context => {
                        clip(row.text.get(1..).unwrap_or(""), hs)
                    }
                    _ => clip(&row.text, hs),
                };
                buf.set_style(
                    Rect::new(x, y, w, 1),
                    Style::default().bg(st.bg.unwrap_or(Color::Reset)),
                );
                buf.set_stringn(x, y, text, w as usize, st);
            }
            buf.set_string(
                inner.x + half - 1,
                y,
                "│",
                Style::default().fg(app.theme.border),
            );
            hits.push((Rect::new(inner.x, y, inner.width, 1), l.or(*r).unwrap_or(0)));
        }
        for (r, i) in hits {
            app.hits.push(r, Hit::MainLine(i));
        }
        scrollbar(f, app, PanelId::Main, area, pairs.len(), start, h, focused);
        return;
    }

    let scroll = app.main.scroll;
    let cursor = app.main.cursor;
    let mut lines: Vec<(Line<'static>, Style)> = Vec::with_capacity(h);
    match &app.main.kind {
        MainKind::Empty => {}
        MainKind::Text => {
            for l in app.main.text.iter().skip(scroll).take(h) {
                lines.push((l.clone(), Style::default()));
            }
        }
        MainKind::Diff { rows, .. } => {
            for r in rows.iter().skip(scroll).take(h) {
                lines.push((
                    Line::styled(clip(&r.text, hs), diff_style(r.kind, app)),
                    Style::default(),
                ));
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
    let text_mode = !has_cursor;
    for (i, (line, _)) in lines.into_iter().enumerate() {
        let y = inner.y + i as u16;
        let idx = scroll + i;
        let r = Rect::new(inner.x, y, inner.width, 1);
        let buf: &mut Buffer = f.buffer_mut();
        if has_cursor && idx >= sel_a && idx <= sel_b {
            buf.set_style(
                r,
                Style::default().bg(if idx == cursor { cursor_bg } else { range_bg }),
            );
        }
        if text_mode && hs > 0 {
            let s: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            let st = line
                .spans
                .first()
                .map(|s| s.style)
                .unwrap_or_default()
                .patch(line.style);
            buf.set_stringn(r.x, r.y, clip(&s, hs), r.width as usize, st);
        } else {
            buf.set_line(r.x, r.y, &line, r.width);
        }
        app.hits.push(r, Hit::MainLine(idx));
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
