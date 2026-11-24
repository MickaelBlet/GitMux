//! Keyboard, mouse and timer event handling.

use crate::action::Action;
use crate::app::*;
use crate::askpass;
use crate::git::diff::Pick;
use crate::git::rebase::TodoAction;
use crate::keymap::Key;
use crate::ui::hit::{Btn, Hit};
use crossterm::event::{
    Event as TermEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use std::time::{Duration, Instant};

const DOUBLE_CLICK: Duration = Duration::from_millis(400);

impl App {
    pub fn handle(&mut self, ev: Event) {
        match ev {
            Event::Term(TermEvent::Key(k)) if k.kind != KeyEventKind::Release => self.on_key(k),
            Event::Term(TermEvent::Mouse(m)) => self.on_mouse(m),
            Event::Term(TermEvent::Paste(s)) => self.on_paste(&s),
            Event::Term(TermEvent::FocusGained) => self.refresh(),
            Event::Term(_) => {}
            Event::Tick => self.on_tick(),
            Event::JobDone {
                label,
                result,
                then,
            } => self.job_done(label, result, then),
        }
    }

    fn on_tick(&mut self) {
        self.spinner = self.spinner.wrapping_add(1);
        if let Some((_, t, _)) = &self.message
            && t.elapsed() > Duration::from_secs(5)
            && self.jobs == 0
        {
            self.message = None;
        }
        let prompts = self.askpass.as_ref().map(|b| b.poll()).unwrap_or_default();
        for (id, prompt) in prompts {
            let lower = prompt.to_lowercase();
            let mask = ["password", "passphrase", "pin", "token"]
                .iter()
                .any(|w| lower.contains(w));
            self.popups.push(Popup::Input {
                title: prompt.trim().trim_end_matches(':').to_string(),
                input: crate::textinput::TextInput::new("", false),
                kind: InputKind::Askpass(id),
                mask,
            });
        }
        if self.popups.is_empty()
            && self.jobs == 0
            && self.last_poll.elapsed() > Duration::from_secs(2)
        {
            self.last_poll = Instant::now();
            self.poll_changes();
        }
        let af = self.cfg.auto_fetch_secs;
        if af > 0 && self.jobs == 0 && self.last_fetch.elapsed() > Duration::from_secs(af) {
            self.last_fetch = Instant::now();
            self.run_op(crate::ops::git_bg(&[
                "fetch", "--all", "--prune", "--quiet",
            ]));
        }
    }

    fn on_paste(&mut self, s: &str) {
        match self.popups.last_mut() {
            Some(Popup::Input { input, .. }) => input.insert_str(&s.replace('\n', " ")),
            Some(Popup::Commit(c)) => c.input.insert_str(s),
            _ => {}
        }
        self.after_input_edit();
    }

    // ---------------- keyboard ----------------

    fn on_key(&mut self, k: KeyEvent) {
        if !self.popups.is_empty() {
            return self.popup_key(k);
        }
        if k.code == KeyCode::Esc {
            return self.escape();
        }
        if k.code == KeyCode::Char('c') && k.modifiers.contains(KeyModifiers::CONTROL) {
            self.quit = true;
            return;
        }
        let key = Key::from_event(&k);
        if let Some(a) = self.keymap.lookup(&self.contexts(), key) {
            self.dispatch(a);
        }
    }

    fn escape(&mut self) {
        if self.focus == PanelId::Main {
            if self.main.anchor.take().is_some() {
                return;
            }
            if self.main_back() {
                return;
            }
            let p = self.last_left;
            self.focus_panel(p);
            return;
        }
        let p = self.focus;
        if p != PanelId::Log && !self.list(p).filter.is_empty() {
            self.list_mut(p).filter.clear();
            self.preview();
        } else if !self.cherry.is_empty() {
            self.cherry.clear();
            self.info("cherry-pick selection cleared");
        }
    }

    fn popup_cancel(&mut self) {
        match self.popups.pop() {
            Some(Popup::Input {
                kind: InputKind::Askpass(id),
                ..
            }) => {
                if let Some(b) = &self.askpass {
                    b.answer(&id, askpass::CANCEL);
                }
            }
            Some(Popup::Input {
                kind: InputKind::Filter(p),
                ..
            }) => {
                let t = self.tabs[p.index()];
                self.lists[Self::list_idx(p, t)].filter.clear();
                self.preview();
            }
            _ => {}
        }
    }

    fn popup_ok(&mut self) {
        let Some(top) = self.popups.last().cloned() else {
            return;
        };
        match top {
            Popup::Confirm { .. } => self.accept_confirm(),
            Popup::Input { input, kind, .. } => {
                self.popups.pop();
                let v = input.text();
                match kind {
                    InputKind::TagMessage { name, at } => self.submit_tag_message(name, at, v),
                    k => self.submit_input(k, v),
                }
            }
            Popup::Commit(c) => {
                self.popups.pop();
                self.submit_commit(c);
            }
            Popup::Menu { items, sel, .. } => {
                self.popups.pop();
                if let Some(it) = items.get(sel) {
                    self.run_menu(it.cmd.clone());
                }
            }
            Popup::Rebase { entries, base, .. } => {
                self.popups.pop();
                self.run_op(Op::Rebase {
                    base,
                    todo: entries,
                });
            }
            Popup::Help { .. } | Popup::Message { .. } => {
                self.popups.pop();
            }
        }
    }

    fn after_input_edit(&mut self) {
        if let Some(Popup::Input {
            input,
            kind: InputKind::Filter(p),
            ..
        }) = self.popups.last()
        {
            let (p, text) = (*p, input.text());
            let t = self.tabs[p.index()];
            let l = &mut self.lists[Self::list_idx(p, t)];
            l.filter = text;
            l.sel = 0;
            l.off = 0;
            self.preview();
        }
    }

    fn popup_key(&mut self, k: KeyEvent) {
        let alt = k.modifiers.contains(KeyModifiers::ALT);
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let shift = k.modifiers.contains(KeyModifiers::SHIFT);
        if k.code == KeyCode::Esc {
            return self.popup_cancel();
        }
        let Some(top) = self.popups.last_mut() else {
            return;
        };
        match top {
            Popup::Confirm { .. } => match k.code {
                KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => self.accept_confirm(),
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Char('q') => self.popup_cancel(),
                _ => {}
            },
            Popup::Input { input, .. } => {
                if k.code == KeyCode::Enter {
                    return self.popup_ok();
                }
                input.handle_key(&k);
                self.after_input_edit();
            }
            Popup::Menu { items, sel, .. } => match k.code {
                KeyCode::Up | KeyCode::Char('k') => *sel = sel.saturating_sub(1),
                KeyCode::Down | KeyCode::Char('j') => {
                    *sel = (*sel + 1).min(items.len().saturating_sub(1))
                }
                KeyCode::Enter | KeyCode::Char(' ') => self.popup_ok(),
                KeyCode::Char('q') => self.popup_cancel(),
                KeyCode::Char(c) => {
                    if let Some(i) = items.iter().position(|it| it.key == Some(c)) {
                        *sel = i;
                        self.popup_ok();
                    }
                }
                _ => {}
            },
            Popup::Commit(c) => match k.code {
                KeyCode::Enter if alt || shift => c.input.newline(),
                KeyCode::Enter => self.popup_ok(),
                KeyCode::Char('a') if alt => self.toggle_commit(Btn::Amend),
                KeyCode::Char('n') if alt => self.toggle_commit(Btn::NoVerify),
                KeyCode::Char('s') if alt => self.toggle_commit(Btn::Signoff),
                _ => {
                    c.input.handle_key(&k);
                }
            },
            Popup::Help { scroll } | Popup::Message { scroll, .. } => match k.code {
                KeyCode::Up | KeyCode::Char('k') => *scroll = scroll.saturating_sub(1),
                KeyCode::Down | KeyCode::Char('j') => *scroll += 1,
                KeyCode::PageUp => *scroll = scroll.saturating_sub(20),
                KeyCode::PageDown | KeyCode::Char(' ') => *scroll += 20,
                KeyCode::Enter | KeyCode::Char('q') | KeyCode::Char('?') => self.popup_cancel(),
                _ => {}
            },
            Popup::Rebase { entries, sel, .. } => {
                let n = entries.len();
                match k.code {
                    KeyCode::Up if ctrl => self.todo_move(-1),
                    KeyCode::Down if ctrl => self.todo_move(1),
                    KeyCode::Char('K') => self.todo_move(-1),
                    KeyCode::Char('J') => self.todo_move(1),
                    KeyCode::Up | KeyCode::Char('k') => *sel = sel.saturating_sub(1),
                    KeyCode::Down | KeyCode::Char('j') => {
                        *sel = (*sel + 1).min(n.saturating_sub(1))
                    }
                    KeyCode::Enter => self.popup_ok(),
                    KeyCode::Char(c) => {
                        if let Some(a) = TodoAction::ALL.iter().find(|a| a.key() == c) {
                            let i = *sel;
                            self.todo_set(i, *a);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn toggle_commit(&mut self, b: Btn) {
        let head_msg = self
            .git
            .read(&["log", "-1", "--format=%B"])
            .unwrap_or_default();
        if let Some(Popup::Commit(c)) = self.popups.last_mut() {
            if c.reword.is_some() || c.only {
                return;
            }
            match b {
                Btn::Amend => {
                    c.amend = !c.amend;
                    if c.amend && c.input.is_empty() {
                        c.input = crate::textinput::TextInput::new(head_msg.trim_end(), true);
                    }
                }
                Btn::NoVerify => c.no_verify = !c.no_verify,
                Btn::Signoff => c.signoff = !c.signoff,
                _ => {}
            }
        }
    }

    fn todo_move(&mut self, d: isize) {
        if let Some(Popup::Rebase { entries, sel, .. }) = self.popups.last_mut() {
            let j = *sel as isize + d;
            if j >= 0 && (j as usize) < entries.len() {
                entries.swap(*sel, j as usize);
                *sel = j as usize;
            }
        }
    }

    fn todo_set(&mut self, i: usize, a: TodoAction) {
        let Some(Popup::Rebase { entries, sel, .. }) = self.popups.last_mut() else {
            return;
        };
        *sel = i;
        let Some(e) = entries.get_mut(i) else { return };
        if a == TodoAction::Reword {
            let init = e.message.clone().unwrap_or_else(|| e.summary.clone());
            self.input("New commit message", &init, InputKind::TodoReword(i));
        } else {
            e.action = a;
            e.message = None;
        }
    }

    // ---------------- mouse ----------------

    fn on_mouse(&mut self, m: MouseEvent) {
        let (x, y) = (m.column, m.row);
        let shift = m.modifiers.contains(KeyModifiers::SHIFT);
        match m.kind {
            MouseEventKind::Moved => self.hover = Some((x, y)),
            MouseEventKind::Down(MouseButton::Left) => self.left_down(x, y),
            MouseEventKind::Down(MouseButton::Right) => self.right_down(x, y),
            MouseEventKind::Drag(MouseButton::Left) => {
                self.hover = Some((x, y));
                self.on_drag(x, y);
            }
            MouseEventKind::Up(_) => self.drag = None,
            MouseEventKind::ScrollUp => self.wheel(x, y, -3, shift),
            MouseEventKind::ScrollDown => self.wheel(x, y, 3, shift),
            MouseEventKind::ScrollLeft => self.main.hscroll = self.main.hscroll.saturating_sub(4),
            MouseEventKind::ScrollRight => self.main.hscroll += 4,
            _ => {}
        }
    }

    fn is_popup_hit(h: &Hit) -> bool {
        matches!(
            h,
            Hit::Popup
                | Hit::MenuItem(_)
                | Hit::Button(_)
                | Hit::TodoRow(_)
                | Hit::TodoAction(..)
                | Hit::InputText
        )
    }

    fn left_down(&mut self, x: u16, y: u16) {
        let now = Instant::now();
        let double = self
            .last_click
            .is_some_and(|(t, lx, ly)| now.duration_since(t) < DOUBLE_CLICK && lx == x && ly == y);
        self.last_click = if double { None } else { Some((now, x, y)) };
        let hit = self.hits.at(x, y);
        if !self.popups.is_empty() && !hit.as_ref().is_some_and(|(_, h)| Self::is_popup_hit(h)) {
            // click outside a popup closes informational popups
            if matches!(
                self.popups.last(),
                Some(Popup::Menu { .. } | Popup::Help { .. } | Popup::Message { .. })
            ) {
                self.popups.pop();
            }
            return;
        }
        let Some((rect, hit)) = hit else { return };
        match hit {
            Hit::Panel(p) => {
                if self.focus != p {
                    self.focus_panel(p);
                }
            }
            Hit::Row(p, i) => {
                if self.focus != p {
                    self.focus_panel(p);
                }
                self.set_sel(p, i);
                if double {
                    self.primary(p);
                }
            }
            Hit::Tab(p, i) => self.set_tab(p, i),
            Hit::Scrollbar(p) => {
                self.drag = Some(Drag::Scroll(p, rect));
                self.scroll_to(p, rect, y);
            }
            Hit::SplitV => self.drag = Some(Drag::SplitV),
            Hit::SplitH => self.drag = Some(Drag::SplitH),
            Hit::Hint(a) => self.dispatch(a),
            Hit::MenuItem(i) => {
                if let Some(Popup::Menu { sel, .. }) = self.popups.last_mut() {
                    *sel = i;
                }
                self.popup_ok();
            }
            Hit::Button(b) => self.button(b),
            Hit::TodoRow(i) => {
                if let Some(Popup::Rebase { sel, .. }) = self.popups.last_mut() {
                    *sel = i;
                }
                self.drag = Some(Drag::Todo(i));
            }
            Hit::TodoAction(i, a) => self.todo_set(i, a),
            Hit::MainLine(i) => {
                if self.focus != PanelId::Main {
                    self.focus = PanelId::Main;
                }
                self.main.anchor = None;
                self.main.cursor = i.min(self.main.len().saturating_sub(1));
                self.drag = Some(Drag::MainSelect(i));
                if double {
                    self.main_double(i);
                }
            }
            Hit::InputText => self.click_input(x, y),
            Hit::Popup => {}
        }
    }

    fn primary(&mut self, p: PanelId) {
        let t = self.tabs[p.index()];
        let a = match (p, t) {
            (PanelId::Files, _) => match self.sel_file_row() {
                Some(FileRow::Dir { .. }) => Action::Enter,
                _ => Action::StageToggle,
            },
            (PanelId::Branches, 0..=2) => Action::Checkout,
            (PanelId::Stash, 1) => Action::EnterSubmodule,
            (PanelId::Stash, 2) => Action::SwitchWorktree,
            (PanelId::Status, _) => Action::RecentRepo,
            _ => Action::Enter,
        };
        self.dispatch(a);
    }

    fn main_double(&mut self, i: usize) {
        match &self.main.kind {
            MainKind::Diff { .. } => self.dispatch(Action::StageSelection),
            MainKind::Conflict { conflicts, .. } => {
                let Some(c) = conflicts.iter().find(|c| i >= c.start && i <= c.end) else {
                    return;
                };
                let pick = if i == c.start || i == c.mid || i == c.end {
                    Pick::Both
                } else if i < c.base.unwrap_or(c.mid) {
                    Pick::Ours
                } else if i > c.mid {
                    Pick::Theirs
                } else {
                    return;
                };
                self.pick_conflict(pick, Some(i));
            }
            MainKind::Blame { .. } | MainKind::History { .. } => self.main_enter(),
            _ => {}
        }
    }

    fn right_down(&mut self, x: u16, y: u16) {
        if !self.popups.is_empty() {
            return;
        }
        match self.hits.at(x, y).map(|h| h.1) {
            Some(Hit::Row(p, i)) => {
                if self.focus != p {
                    self.focus_panel(p);
                }
                self.set_sel(p, i);
                self.context_menu();
            }
            Some(Hit::MainLine(i)) => {
                self.focus = PanelId::Main;
                if self.main.anchor.is_none() {
                    self.main.cursor = i.min(self.main.len().saturating_sub(1));
                }
                self.context_menu();
            }
            Some(Hit::Panel(p)) => {
                if self.focus != p {
                    self.focus_panel(p);
                }
                self.context_menu();
            }
            _ => {}
        }
    }

    fn on_drag(&mut self, x: u16, y: u16) {
        let Some(d) = self.drag else { return };
        match d {
            Drag::SplitV => {
                let w = self.screen.width.max(1) as u32;
                self.left_pct = ((x as u32 * 100 / w) as u16).clamp(15, 85);
            }
            Drag::SplitH => {
                let bottom = self.screen.height.saturating_sub(1);
                self.log_height = bottom
                    .saturating_sub(y)
                    .clamp(3, bottom.saturating_sub(5).max(3));
            }
            Drag::Scroll(p, r) => self.scroll_to(p, r, y),
            Drag::MainSelect(start) => {
                let r = self.main_rect;
                if y < r.y {
                    self.main.move_cursor(-1);
                } else if y >= r.bottom() {
                    self.main.move_cursor(1);
                } else if let Some((_, Hit::MainLine(j))) =
                    self.hits.at(x.clamp(r.x, r.right().saturating_sub(1)), y)
                {
                    self.main.cursor = j.min(self.main.len().saturating_sub(1));
                }
                if self.main.has_cursor() && self.main.cursor != start {
                    self.main.anchor = Some(start);
                }
            }
            Drag::Todo(from) => {
                if let Some((_, Hit::TodoRow(j) | Hit::TodoAction(j, _))) = self.hits.at(x, y)
                    && j != from
                {
                    if let Some(Popup::Rebase { entries, sel, .. }) = self.popups.last_mut() {
                        let e = entries.remove(from);
                        entries.insert(j, e);
                        *sel = j;
                    }
                    self.drag = Some(Drag::Todo(j));
                }
            }
        }
    }

    fn scroll_to(&mut self, p: PanelId, r: ratatui::layout::Rect, y: u16) {
        let frac = (y.saturating_sub(r.y) as f64 / (r.height.max(2) - 1) as f64).clamp(0.0, 1.0);
        if p == PanelId::Main {
            let max = self.main.len().saturating_sub(self.main.height);
            let target = (frac * max as f64).round() as isize;
            let cur = self.main.scroll as isize;
            self.main.scroll_by(target - cur);
        } else {
            let n = self.visible(p).len();
            if n > 0 {
                self.set_sel(p, (frac * (n - 1) as f64).round() as usize);
            }
        }
    }

    fn wheel(&mut self, x: u16, y: u16, d: isize, shift: bool) {
        if let Some(top) = self.popups.last_mut() {
            match top {
                Popup::Menu { items, sel, .. } => {
                    *sel = (*sel as isize + d.signum()).clamp(0, items.len() as isize - 1) as usize
                }
                Popup::Help { scroll } | Popup::Message { scroll, .. } => {
                    *scroll = (*scroll as isize + d).max(0) as usize
                }
                Popup::Rebase { entries, sel, .. } => {
                    *sel =
                        (*sel as isize + d.signum()).clamp(0, entries.len() as isize - 1) as usize
                }
                _ => {}
            }
            return;
        }
        match self.hits.panel_at(x, y) {
            Some(PanelId::Main) => {
                if shift {
                    self.main.hscroll = (self.main.hscroll as isize + d * 2).max(0) as usize;
                } else if self.side_by_side && matches!(self.main.kind, MainKind::Diff { .. }) {
                    self.main.move_cursor(d);
                } else {
                    self.main.scroll_by(d);
                }
            }
            Some(PanelId::Log) => self.log_scroll = (self.log_scroll as isize - d).max(0) as usize,
            Some(p) => self.move_sel(p, d.signum()),
            None => {}
        }
    }

    fn button(&mut self, b: Btn) {
        match b {
            Btn::Ok => self.popup_ok(),
            Btn::Cancel => self.popup_cancel(),
            Btn::Amend | Btn::NoVerify | Btn::Signoff => self.toggle_commit(b),
            Btn::Conflict(p) => self.pick_conflict(p, None),
            Btn::Back => {
                self.main_back();
            }
            Btn::StageView => self.dispatch(Action::ToggleStagedView),
            Btn::SideBySide => self.dispatch(Action::SideBySide),
            Btn::WordDiff => self.dispatch(Action::WordDiff),
            Btn::TodoUp => self.todo_move(-1),
            Btn::TodoDown => self.todo_move(1),
        }
    }

    fn click_input(&mut self, x: u16, y: u16) {
        let Some((r, off)) = self.hits.input_area else {
            return;
        };
        let col_w = (x.saturating_sub(r.x)) as usize;
        let row = off + y.saturating_sub(r.y) as usize;
        let place = |t: &mut crate::textinput::TextInput, row: usize, start: usize| {
            let row = row.min(t.lines.len() - 1);
            // convert display column to char index
            let mut w = 0;
            let mut col = 0;
            for c in t.lines[row].chars() {
                let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
                if w + cw > col_w + start {
                    break;
                }
                w += cw;
                col += 1;
            }
            t.set_cursor(row, col);
        };
        match self.popups.last_mut() {
            Some(Popup::Input { input, .. }) => place(input, 0, off),
            Some(Popup::Commit(c)) => place(&mut c.input, row, 0),
            _ => {}
        }
    }
}
