//! Application state, repository loading, selection helpers and main-view previews.

use crate::action::Action;
use crate::askpass::Bridge;
use crate::config::Config;
use crate::git::diff::{self, Conflict, FileDiff, Row};
use crate::git::read::{self, CommitInfo, RepoData};
use crate::git::rebase::TodoEntry;
use crate::git::{CmdLog, Git};
use crate::keymap::{Ctx, Keymap};
use crate::textinput::TextInput;
use crate::theme::Theme;
use crate::ui::hit::HitMap;
use anyhow::{Result, anyhow};
use git2::{Oid, Repository};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PanelId {
    Status,
    Files,
    Branches,
    Commits,
    Stash,
    Main,
    Log,
}

impl PanelId {
    pub const LEFT: [PanelId; 5] = [
        PanelId::Status,
        PanelId::Files,
        PanelId::Branches,
        PanelId::Commits,
        PanelId::Stash,
    ];
    pub fn tabs(self) -> &'static [&'static str] {
        match self {
            PanelId::Status => &["Status"],
            PanelId::Files => &["Files"],
            PanelId::Branches => &["Local", "Remote", "Tags", "Remotes"],
            PanelId::Commits => &["Commits", "Reflog"],
            PanelId::Stash => &["Stash", "Submodules", "Worktrees"],
            PanelId::Main => &["Main"],
            PanelId::Log => &["Command log"],
        }
    }
    pub fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Debug, Default)]
pub struct ListState {
    pub sel: usize,
    pub off: usize,
    pub height: usize,
    pub filter: String,
}

#[derive(Clone, Debug)]
pub enum FileRow {
    Dir {
        path: String,
        depth: usize,
        collapsed: bool,
    },
    File {
        idx: usize,
        depth: usize,
    },
}

#[derive(Clone, Debug)]
pub enum MainKind {
    Empty,
    Text,
    Diff {
        path: String,
        staged: bool,
        untracked: bool,
        fd: FileDiff,
        rows: Vec<Row>,
    },
    Conflict {
        path: String,
        lines: Vec<String>,
        conflicts: Vec<Conflict>,
    },
    Blame {
        lines: Vec<(Oid, String, String, String)>,
    },
    History {
        path: String,
        commits: Vec<CommitInfo>,
    },
}

#[derive(Clone, Debug)]
pub struct MainView {
    pub title: String,
    pub kind: MainKind,
    pub text: Vec<Line<'static>>,
    pub cursor: usize,
    pub anchor: Option<usize>,
    pub scroll: usize,
    pub hscroll: usize,
    pub height: usize,
}

impl Default for MainView {
    fn default() -> Self {
        MainView {
            title: String::new(),
            kind: MainKind::Empty,
            text: vec![],
            cursor: 0,
            anchor: None,
            scroll: 0,
            hscroll: 0,
            height: 10,
        }
    }
}

impl MainView {
    pub fn len(&self) -> usize {
        match &self.kind {
            MainKind::Empty => 0,
            MainKind::Text => self.text.len(),
            MainKind::Diff { rows, .. } => rows.len(),
            MainKind::Conflict { lines, .. } => lines.len(),
            MainKind::Blame { lines, .. } => lines.len(),
            MainKind::History { commits, .. } => commits.len(),
        }
    }
    pub fn has_cursor(&self) -> bool {
        !matches!(self.kind, MainKind::Text | MainKind::Empty)
    }
    pub fn move_cursor(&mut self, delta: isize) {
        let n = self.len();
        if n == 0 {
            return;
        }
        if self.has_cursor() {
            self.cursor = (self.cursor as isize + delta).clamp(0, n as isize - 1) as usize;
            self.ensure_visible();
        } else {
            self.scroll_by(delta);
        }
    }
    pub fn set_cursor(&mut self, c: usize) {
        self.cursor = c.min(self.len().saturating_sub(1));
        self.ensure_visible();
    }
    pub fn ensure_visible(&mut self) {
        let h = self.height.max(1);
        if self.cursor < self.scroll {
            self.scroll = self.cursor;
        } else if self.cursor >= self.scroll + h {
            self.scroll = self.cursor + 1 - h;
        }
    }
    pub fn scroll_by(&mut self, delta: isize) {
        let max = self.len().saturating_sub(self.height.max(1));
        self.scroll = (self.scroll as isize + delta).clamp(0, max as isize) as usize;
        if self.has_cursor() {
            let h = self.height.max(1);
            self.cursor = self.cursor.clamp(
                self.scroll,
                (self.scroll + h - 1).min(self.len().saturating_sub(1)),
            );
        }
    }
    pub fn selection(&self) -> (usize, usize) {
        match self.anchor {
            Some(a) => (a.min(self.cursor), a.max(self.cursor)),
            None => (self.cursor, self.cursor),
        }
    }
}

#[derive(Clone, Debug)]
pub enum InputKind {
    NewBranch { from: Option<String> },
    RenameBranch(String),
    NewTag { at: Option<String> },
    TagMessage { name: String, at: Option<String> },
    AddRemoteName,
    AddRemoteUrl(String),
    EditRemoteUrl(String),
    SetUpstream(String),
    CustomCommand,
    Filter(PanelId),
    CheckoutByName,
    StashMessage(Vec<String>),
    StashBranch(usize),
    AddWorktreePath,
    AddWorktreeBranch(String),
    SubmoduleUrl,
    SubmodulePath(String),
    Askpass(String),
    OpenRepo,
    TodoReword(usize),
    BranchAt(String),
}

#[derive(Clone, Debug)]
pub enum Op {
    /// Run git commands sequentially; `bg` runs in a background thread.
    Git {
        cmds: Vec<Vec<String>>,
        bg: bool,
        then: Then,
    },
    Patch {
        patch: String,
        args: Vec<String>,
    },
    Commit {
        msg: String,
        amend: bool,
        no_verify: bool,
        signoff: bool,
        only: bool,
        stage_all: bool,
    },
    Rebase {
        base: Option<String>,
        todo: Vec<TodoEntry>,
    },
    Push {
        force: bool,
    },
    OpenRepo(PathBuf),
    Ignore(String),
    Undo,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Then {
    Refresh,
    ShowOutput,
    PushRejected,
    DeleteBranch(String),
}

#[derive(Clone, Debug)]
pub enum MenuCmd {
    Act(Action),
    Op(Op),
    Confirm(String, Op),
    Input(InputKind, String, String),
}

#[derive(Clone, Debug)]
pub struct MenuItem {
    pub label: String,
    pub key: Option<char>,
    pub cmd: MenuCmd,
}

impl MenuItem {
    pub fn new(key: char, label: impl Into<String>, cmd: MenuCmd) -> Self {
        MenuItem {
            label: label.into(),
            key: Some(key),
            cmd,
        }
    }
}

#[derive(Clone, Debug)]
pub struct CommitPopup {
    pub input: TextInput,
    pub amend: bool,
    pub no_verify: bool,
    pub signoff: bool,
    /// Reword a non-HEAD commit through rebase.
    pub reword: Option<Oid>,
    /// Reword HEAD (message only).
    pub only: bool,
}

#[derive(Clone, Debug)]
pub enum Popup {
    Confirm {
        title: String,
        msg: String,
        op: Op,
    },
    Input {
        title: String,
        input: TextInput,
        kind: InputKind,
        mask: bool,
    },
    Menu {
        title: String,
        items: Vec<MenuItem>,
        sel: usize,
    },
    Commit(CommitPopup),
    Help {
        scroll: usize,
    },
    Message {
        title: String,
        lines: Vec<String>,
        scroll: usize,
        error: bool,
    },
    Rebase {
        entries: Vec<TodoEntry>,
        sel: usize,
        base: Option<String>,
    },
}

#[derive(Clone, Copy, Debug)]
pub enum Drag {
    SplitV,
    SplitH,
    Scroll(PanelId, Rect),
    MainSelect(usize),
    Todo(usize),
}

pub enum Event {
    Term(crossterm::event::Event),
    Tick,
    JobDone {
        label: String,
        result: Result<String, String>,
        then: Then,
    },
}

pub struct App {
    pub repo: Repository,
    pub git: Git,
    pub cfg: Config,
    pub theme: Theme,
    pub keymap: Keymap,
    pub data: RepoData,
    pub focus: PanelId,
    pub last_left: PanelId,
    pub tabs: [usize; 7],
    /// list state per (panel, tab): index = panel*4+tab
    pub lists: Vec<ListState>,
    pub main: MainView,
    pub main_stack: Vec<MainView>,
    pub popups: Vec<Popup>,
    pub log: CmdLog,
    pub log_scroll: usize,
    pub show_log: bool,
    pub hits: HitMap,
    pub left_pct: u16,
    pub log_height: u16,
    pub drag: Option<Drag>,
    pub last_click: Option<(Instant, u16, u16)>,
    pub tx: Sender<Event>,
    pub jobs: usize,
    pub spinner: usize,
    pub cherry: Vec<(Oid, String)>,
    pub collapsed: HashSet<String>,
    pub tree_view: bool,
    pub side_by_side: bool,
    pub word_diff: bool,
    pub force_staged: Option<bool>,
    pub message: Option<(String, Instant, bool)>,
    pub repo_stack: Vec<PathBuf>,
    pub askpass: Option<Bridge>,
    pub commit_limit: usize,
    pub quit: bool,
    pub needs_editor: Option<PathBuf>,
    pub last_poll: Instant,
    pub last_fetch: Instant,
    pub main_rect: Rect,
    pub screen: Rect,
    pub nerd_font: bool,
    pub hover: Option<(u16, u16)>,
}

pub fn open_repo(path: &Path) -> Result<Repository> {
    let r = Repository::discover(path)
        .map_err(|e| anyhow!("not a git repository: {} ({})", path.display(), e.message()))?;
    if r.is_bare() {
        return Err(anyhow!("bare repositories are not supported"));
    }
    Ok(r)
}

impl App {
    pub fn new(path: &Path, cfg: Config, tx: Sender<Event>) -> Result<App> {
        let repo = open_repo(path)?;
        let workdir = repo.workdir().unwrap().to_path_buf();
        let askpass = Bridge::new();
        let env = askpass.as_ref().map(|b| b.env()).unwrap_or_default();
        let log: CmdLog = Default::default();
        let git = Git::new(&workdir, env, log.clone());
        let (keymap, errs) = Keymap::new(&cfg.keybindings);
        let mut app = App {
            repo,
            git,
            theme: Theme::from(&cfg.theme),
            keymap,
            data: RepoData::default(),
            focus: PanelId::Files,
            last_left: PanelId::Files,
            tabs: [0; 7],
            lists: vec![ListState::default(); 7 * 4],
            main: MainView::default(),
            main_stack: vec![],
            popups: vec![],
            log,
            log_scroll: 0,
            show_log: cfg.show_command_log,
            hits: HitMap::default(),
            left_pct: cfg.left_width_pct.clamp(15, 85),
            log_height: 7,
            drag: None,
            last_click: None,
            tx,
            jobs: 0,
            spinner: 0,
            cherry: vec![],
            collapsed: HashSet::new(),
            tree_view: cfg.tree_view,
            side_by_side: cfg.side_by_side,
            word_diff: false,
            force_staged: None,
            message: None,
            repo_stack: vec![],
            askpass,
            commit_limit: cfg.commit_limit.max(50),
            quit: false,
            needs_editor: None,
            last_poll: Instant::now(),
            last_fetch: Instant::now(),
            main_rect: Rect::default(),
            screen: Rect::default(),
            nerd_font: false,
            hover: None,
            cfg,
        };
        if !errs.is_empty() {
            app.error("Config", errs.join("\n"));
        }
        crate::config::add_recent(&workdir.to_string_lossy());
        app.refresh();
        Ok(app)
    }

    pub fn switch_repo(&mut self, path: &Path) -> Result<()> {
        let repo = open_repo(path)?;
        let wd = repo.workdir().unwrap().to_path_buf();
        self.git.dir = wd.clone();
        self.repo = repo;
        for l in &mut self.lists {
            *l = ListState::default();
        }
        self.cherry.clear();
        self.main_stack.clear();
        self.collapsed.clear();
        crate::config::add_recent(&wd.to_string_lossy());
        self.refresh();
        self.info(format!("opened {}", wd.display()));
        Ok(())
    }

    pub fn refresh(&mut self) {
        match read::load(&mut self.repo, self.commit_limit, &self.git) {
            Ok(d) => self.data = d,
            Err(e) => self.error("Refresh failed", e.to_string()),
        }
        for p in PanelId::LEFT {
            for t in 0..p.tabs().len() {
                self.clamp_list(p, t);
            }
        }
        self.preview();
    }

    /// Cheap change detection used on ticks.
    pub fn poll_changes(&mut self) {
        let files = read::files(&self.repo).unwrap_or_default();
        let head = self.repo.head().ok().and_then(|h| h.target());
        let cur_head = self.data.commits.first().map(|c| c.oid);
        let same = files.len() == self.data.files.len()
            && files
                .iter()
                .zip(&self.data.files)
                .all(|(a, b)| a.path == b.path && a.x == b.x && a.y == b.y)
            && head == cur_head
            && read::state_name(self.repo.state()) == self.data.state;
        if !same {
            self.refresh();
        }
    }

    // ---------- messages / popups ----------

    pub fn info(&mut self, s: impl Into<String>) {
        self.message = Some((s.into(), Instant::now(), false));
    }

    pub fn error(&mut self, title: &str, s: impl Into<String>) {
        let s = s.into();
        self.popups.push(Popup::Message {
            title: title.into(),
            lines: s.lines().map(String::from).collect(),
            scroll: 0,
            error: true,
        });
    }

    pub fn confirm(&mut self, title: &str, msg: impl Into<String>, op: Op) {
        self.popups.push(Popup::Confirm {
            title: title.into(),
            msg: msg.into(),
            op,
        });
    }

    pub fn input(&mut self, title: &str, initial: &str, kind: InputKind) {
        self.popups.push(Popup::Input {
            title: title.into(),
            input: TextInput::new(initial, false),
            kind,
            mask: false,
        });
    }

    pub fn menu(&mut self, title: &str, items: Vec<MenuItem>) {
        if !items.is_empty() {
            self.popups.push(Popup::Menu {
                title: title.into(),
                items,
                sel: 0,
            });
        }
    }

    // ---------- contexts ----------

    pub fn ctx_of(&self, p: PanelId) -> Ctx {
        let t = self.tabs[p.index()];
        match p {
            PanelId::Status => Ctx::Status,
            PanelId::Files => Ctx::Files,
            PanelId::Branches => [Ctx::Branches, Ctx::RemoteBranches, Ctx::Tags, Ctx::Remotes][t],
            PanelId::Commits => [Ctx::Commits, Ctx::Reflog][t],
            PanelId::Stash => [Ctx::Stash, Ctx::Submodules, Ctx::Worktrees][t],
            PanelId::Main => match self.main.kind {
                MainKind::Diff { .. } => Ctx::Diff,
                MainKind::Conflict { .. } => Ctx::Conflict,
                _ => Ctx::Main,
            },
            PanelId::Log => Ctx::Main,
        }
    }

    pub fn contexts(&self) -> Vec<Ctx> {
        let c = self.ctx_of(self.focus);
        if c == Ctx::Global {
            vec![Ctx::Global]
        } else {
            vec![c, Ctx::Global]
        }
    }

    // ---------- lists ----------

    pub fn list_idx(p: PanelId, tab: usize) -> usize {
        p.index() * 4 + tab
    }
    pub fn list(&self, p: PanelId) -> &ListState {
        &self.lists[Self::list_idx(p, self.tabs[p.index()])]
    }
    pub fn list_mut(&mut self, p: PanelId) -> &mut ListState {
        let i = Self::list_idx(p, self.tabs[p.index()]);
        &mut self.lists[i]
    }

    pub fn file_rows(&self) -> Vec<FileRow> {
        let files = &self.data.files;
        if !self.tree_view {
            return (0..files.len())
                .map(|idx| FileRow::File { idx, depth: 0 })
                .collect();
        }
        let mut out = Vec::new();
        let mut emitted: Vec<String> = Vec::new();
        for (idx, f) in files.iter().enumerate() {
            let parts: Vec<&str> = f.path.split('/').collect();
            let mut hidden = false;
            let mut prefix = String::new();
            for (d, part) in parts[..parts.len() - 1].iter().enumerate() {
                if !prefix.is_empty() {
                    prefix.push('/');
                }
                prefix.push_str(part);
                if hidden {
                    continue;
                }
                if emitted.last().map(|s| s.as_str()) != Some(prefix.as_str())
                    && !emitted.contains(&prefix)
                {
                    let collapsed = self.collapsed.contains(&prefix);
                    out.push(FileRow::Dir {
                        path: prefix.clone(),
                        depth: d,
                        collapsed,
                    });
                    emitted.push(prefix.clone());
                }
                if self.collapsed.contains(&prefix) {
                    hidden = true;
                }
            }
            if !hidden {
                out.push(FileRow::File {
                    idx,
                    depth: parts.len() - 1,
                });
            }
        }
        out
    }

    pub fn file_row_path(&self, r: &FileRow) -> String {
        match r {
            FileRow::Dir { path, .. } => path.clone(),
            FileRow::File { idx, .. } => self.data.files[*idx].path.clone(),
        }
    }

    /// Searchable text of every item of a panel's current tab.
    pub fn item_texts(&self, p: PanelId) -> Vec<String> {
        let d = &self.data;
        let t = self.tabs[p.index()];
        match (p, t) {
            (PanelId::Status, _) => vec![d.head.clone()],
            (PanelId::Files, _) => self
                .file_rows()
                .iter()
                .map(|r| self.file_row_path(r))
                .collect(),
            (PanelId::Branches, 0) => d.branches.iter().map(|b| b.name.clone()).collect(),
            (PanelId::Branches, 1) => d.remote_branches.iter().map(|b| b.name.clone()).collect(),
            (PanelId::Branches, 2) => d.tags.iter().map(|b| b.name.clone()).collect(),
            (PanelId::Branches, _) => d
                .remotes
                .iter()
                .map(|r| format!("{} {}", r.name, r.url))
                .collect(),
            (PanelId::Commits, 0) => d
                .commits
                .iter()
                .map(|c| format!("{} {} {} {}", c.oid, c.summary, c.author, c.refs.join(" ")))
                .collect(),
            (PanelId::Commits, _) => d
                .reflog
                .iter()
                .map(|r| format!("{} {}", r.oid, r.message))
                .collect(),
            (PanelId::Stash, 0) => d.stashes.iter().map(|s| s.message.clone()).collect(),
            (PanelId::Stash, 1) => d.submodules.iter().map(|s| s.name.clone()).collect(),
            (PanelId::Stash, _) => d
                .worktrees
                .iter()
                .map(|w| format!("{} {}", w.path.display(), w.branch))
                .collect(),
            _ => vec![],
        }
    }

    /// Data indices visible after filtering.
    pub fn visible(&self, p: PanelId) -> Vec<usize> {
        let f = self.list(p).filter.to_lowercase();
        self.item_texts(p)
            .iter()
            .enumerate()
            .filter(|(_, s)| f.is_empty() || s.to_lowercase().contains(&f))
            .map(|(i, _)| i)
            .collect()
    }

    pub fn selected(&self, p: PanelId) -> Option<usize> {
        let v = self.visible(p);
        v.get(self.list(p).sel).copied()
    }

    fn clamp_list(&mut self, p: PanelId, tab: usize) {
        let saved = self.tabs[p.index()];
        self.tabs[p.index()] = tab;
        let n = self.visible(p).len();
        let l = self.list_mut(p);
        l.sel = l.sel.min(n.saturating_sub(1));
        self.tabs[p.index()] = saved;
    }

    pub fn move_sel(&mut self, p: PanelId, delta: isize) {
        let n = self.visible(p).len();
        if n == 0 {
            return;
        }
        let l = self.list_mut(p);
        l.sel = (l.sel as isize + delta).clamp(0, n as isize - 1) as usize;
        // load more commits when reaching the end
        if p == PanelId::Commits
            && self.tabs[p.index()] == 0
            && self.list(p).sel + 1 >= n
            && n >= self.commit_limit
        {
            self.commit_limit += 300;
            self.refresh();
        }
        self.preview();
    }

    pub fn set_sel(&mut self, p: PanelId, i: usize) {
        let cur = self.list(p).sel as isize;
        self.move_sel(p, i as isize - cur);
    }

    pub fn sel_file_row(&self) -> Option<FileRow> {
        let rows = self.file_rows();
        self.selected(PanelId::Files)
            .and_then(|i| rows.get(i).cloned())
    }

    pub fn sel_file(&self) -> Option<&read::FileEntry> {
        match self.sel_file_row()? {
            FileRow::File { idx, .. } => self.data.files.get(idx),
            _ => None,
        }
    }

    /// Commit id selected in the commits/reflog panel.
    pub fn sel_commit(&self) -> Option<(Oid, String)> {
        let i = self.selected(PanelId::Commits)?;
        if self.tabs[PanelId::Commits.index()] == 0 {
            self.data.commits.get(i).map(|c| (c.oid, c.summary.clone()))
        } else {
            self.data.reflog.get(i).map(|r| (r.oid, r.message.clone()))
        }
    }

    pub fn commit_index(&self, oid: Oid) -> Option<usize> {
        self.data.commits.iter().position(|c| c.oid == oid)
    }

    // ---------- main view previews ----------

    pub fn set_main(&mut self, mut v: MainView) {
        v.height = self.main.height;
        // keep cursor/scroll when re-displaying the same thing (e.g. after staging a line)
        if v.title == self.main.title {
            v.scroll = self.main.scroll;
            v.cursor = self.main.cursor.min(v.len().saturating_sub(1));
            v.hscroll = self.main.hscroll;
        }
        self.main = v;
        if self.main.has_cursor() {
            self.main.ensure_visible();
        }
    }

    pub fn text_view(&self, title: &str, text: &str) -> MainView {
        MainView {
            title: title.into(),
            kind: MainKind::Text,
            text: colorize(text, &self.theme),
            ..Default::default()
        }
    }

    pub fn show_text(&mut self, title: &str, text: &str) {
        let v = self.text_view(title, text);
        self.set_main(v);
    }

    /// Update the main view from the selection in the focused (or last) left panel.
    pub fn preview(&mut self) {
        let p = if self.focus == PanelId::Main || self.focus == PanelId::Log {
            self.last_left
        } else {
            self.focus
        };
        if self.focus == PanelId::Main {
            return;
        }
        self.main_stack.clear();
        let t = self.tabs[p.index()];
        let d = &self.data;
        let res: Result<MainView> = (|| {
            Ok(match (p, t) {
                (PanelId::Status, _) => self.text_view("Status", &self.status_text()),
                (PanelId::Files, _) => match self.sel_file_row() {
                    None => self.text_view("Diff", "No changes."),
                    Some(FileRow::Dir { path, .. }) => {
                        let mut s = self
                            .git
                            .read(&["diff", "--no-color", "--stat", "HEAD", "--", &path])
                            .unwrap_or_default();
                        s.push('\n');
                        s += &self
                            .git
                            .read(&["diff", "--no-color", "HEAD", "--", &path])
                            .unwrap_or_default();
                        self.text_view(&format!("Diff {path}/"), &s)
                    }
                    Some(FileRow::File { idx, .. }) => self.file_view(idx)?,
                },
                (PanelId::Branches, 0 | 1) => {
                    let name = if t == 0 {
                        d.branches
                            .get(self.selected(p).unwrap_or(usize::MAX))
                            .map(|b| b.name.clone())
                    } else {
                        d.remote_branches
                            .get(self.selected(p).unwrap_or(usize::MAX))
                            .map(|b| b.name.clone())
                    };
                    match name {
                        Some(n) => {
                            let s = self.git.read(&[
                                "log",
                                "--graph",
                                "--oneline",
                                "--decorate",
                                "--no-color",
                                "-100",
                                &n,
                                "--",
                            ])?;
                            self.text_view(&format!("Log {n}"), &s)
                        }
                        None => self.text_view("Log", ""),
                    }
                }
                (PanelId::Branches, 2) => match self.selected(p).and_then(|i| d.tags.get(i)) {
                    Some(tg) => {
                        let s = self.git.read(&[
                            "show",
                            "--no-color",
                            "--stat",
                            &format!("refs/tags/{}", tg.name),
                            "--",
                        ])?;
                        self.text_view(&format!("Tag {}", tg.name), &s)
                    }
                    None => self.text_view("Tag", ""),
                },
                (PanelId::Branches, _) => match self.selected(p).and_then(|i| d.remotes.get(i)) {
                    Some(r) => {
                        let mut s = format!("Remote: {}\nURL:    {}\n\nBranches:\n", r.name, r.url);
                        for b in d
                            .remote_branches
                            .iter()
                            .filter(|b| b.name.starts_with(&format!("{}/", r.name)))
                        {
                            s += &format!("  {}  {}\n", b.name, b.summary);
                        }
                        self.text_view(&format!("Remote {}", r.name), &s)
                    }
                    None => self.text_view("Remote", "No remotes. Press n to add one."),
                },
                (PanelId::Commits, _) => match self.sel_commit() {
                    Some((oid, _)) => self.commit_view(&oid.to_string())?,
                    None => self.text_view("Commit", ""),
                },
                (PanelId::Stash, 0) => match self.selected(p).and_then(|i| d.stashes.get(i)) {
                    Some(s) => {
                        let r = format!("stash@{{{}}}", s.index);
                        let txt = self
                            .git
                            .read(&[
                                "stash",
                                "show",
                                "--no-color",
                                "-p",
                                "--stat",
                                "--include-untracked",
                                &r,
                            ])
                            .or_else(|_| {
                                self.git
                                    .read(&["stash", "show", "--no-color", "-p", "--stat", &r])
                            })?;
                        self.text_view(&r, &txt)
                    }
                    None => self.text_view("Stash", "No stash entries."),
                },
                (PanelId::Stash, 1) => match self.selected(p).and_then(|i| d.submodules.get(i)) {
                    Some(s) => {
                        let mut txt = format!(
                            "Submodule: {}\nPath:   {}\nURL:    {}\nStatus: {}\n\n",
                            s.name,
                            s.path.display(),
                            s.url,
                            s.status
                        );
                        let sub = Git::new(&d.workdir.join(&s.path), vec![], self.log.clone());
                        txt += &sub
                            .read(&["log", "--oneline", "--no-color", "-30"])
                            .unwrap_or_default();
                        self.text_view(&format!("Submodule {}", s.name), &txt)
                    }
                    None => self.text_view("Submodules", "No submodules. Press n to add one."),
                },
                (PanelId::Stash, _) => match self.selected(p).and_then(|i| d.worktrees.get(i)) {
                    Some(w) => {
                        let txt = format!(
                            "Worktree: {}\nBranch:   {}\nHEAD:     {}\n{}",
                            w.path.display(),
                            w.branch,
                            w.head,
                            if w.current { "(current)" } else { "" }
                        );
                        self.text_view("Worktree", &txt)
                    }
                    None => self.text_view("Worktrees", ""),
                },
                _ => MainView::default(),
            })
        })();
        match res {
            Ok(v) => self.set_main(v),
            Err(e) => self.show_text("Error", &e.to_string()),
        }
    }

    pub fn commit_view(&self, rev: &str) -> Result<MainView> {
        let s = self.git.read(&[
            "show",
            "--no-color",
            "--stat",
            "-p",
            "--format=fuller",
            rev,
            "--",
        ])?;
        Ok(self.text_view(&format!("Commit {}", &rev[..rev.len().min(8)]), &s))
    }

    pub fn file_view(&self, idx: usize) -> Result<MainView> {
        let f = self.data.files[idx].clone();
        if f.conflicted {
            let content =
                std::fs::read_to_string(self.data.workdir.join(&f.path)).unwrap_or_default();
            let lines: Vec<String> = content.lines().map(String::from).collect();
            let conflicts = diff::find_conflicts(&lines);
            return Ok(MainView {
                title: format!("Conflict {}", f.path),
                kind: MainKind::Conflict {
                    path: f.path.clone(),
                    lines,
                    conflicts,
                },
                ..Default::default()
            });
        }
        let staged = match self.force_staged {
            Some(s) if (s && f.has_staged()) || (!s && f.has_unstaged()) => s,
            _ => !f.has_unstaged(),
        };
        let untracked = f.untracked();
        let mut args: Vec<&str> = vec!["diff", "--no-color", "--no-ext-diff"];
        if self.word_diff {
            args.push("--word-diff=plain");
        }
        if untracked {
            args.extend(["--no-index", "--", "/dev/null", &f.path]);
        } else if staged {
            args.extend(["--cached", "-M", "--"]);
            args.push(&f.path);
            if let Some(o) = &f.orig {
                args.push(o);
            }
        } else {
            args.extend(["--", &f.path]);
        }
        let text = self.git.read(&args)?;
        let title = format!("{} {}", if staged { "Staged" } else { "Unstaged" }, f.path);
        if self.word_diff {
            let mut v = self.text_view(&title, &text);
            v.text = word_colorize(&text, &self.theme);
            return Ok(v);
        }
        let fd = diff::parse(&text);
        let rows = fd.rows();
        let mut v = MainView {
            title,
            kind: MainKind::Diff {
                path: f.path.clone(),
                staged,
                untracked,
                fd,
                rows,
            },
            ..Default::default()
        };
        // start on first change
        if let MainKind::Diff { rows, .. } = &v.kind {
            v.cursor = rows
                .iter()
                .position(|r| matches!(r.kind, diff::RowKind::Add | diff::RowKind::Del))
                .unwrap_or(0);
        }
        Ok(v)
    }

    pub fn status_text(&self) -> String {
        let d = &self.data;
        let mut s = format!("Repository: {}\n", d.workdir.display());
        s += &format!(
            "Branch:     {}{}\n",
            d.head,
            if d.detached { " (detached)" } else { "" }
        );
        if let Some(u) = &d.upstream {
            s += &format!("Upstream:   {u}  ↑{} ↓{}\n", d.ahead, d.behind);
        }
        if !d.state.is_empty() {
            s += &format!("State:      {}  (press m for continue/abort)\n", d.state);
        }
        s += &format!(
            "\nFiles: {}  Branches: {}  Tags: {}  Stashes: {}  Submodules: {}  Worktrees: {}\n",
            d.files.len(),
            d.branches.len(),
            d.tags.len(),
            d.stashes.len(),
            d.submodules.len(),
            d.worktrees.len()
        );
        if !self.repo_stack.is_empty() {
            s += "\n(submodule/worktree — backspace returns to parent repository)\n";
        }
        s += "\nMouse: click to focus/select · double-click for primary action · right-click for actions menu\n";
        s += "       wheel scrolls the panel under the cursor · drag borders to resize · drag in diff to select lines\n";
        s += "\nPress ? for all key bindings, x for the actions menu of the focused panel.\n";
        s
    }

    pub fn cherry_marked(&self, oid: Oid) -> bool {
        self.cherry.iter().any(|(o, _)| *o == oid)
    }

    /// Selected diff lines (hunk, line) for staging; whole hunk when `hunk`.
    pub fn diff_selection(&self, hunk: bool) -> Option<BTreeSet<(usize, usize)>> {
        let MainKind::Diff { fd, rows, .. } = &self.main.kind else {
            return None;
        };
        let (a, b) = self.main.selection();
        let cur = rows.get(self.main.cursor)?;
        if hunk
            || (self.main.anchor.is_none()
                && !matches!(cur.kind, diff::RowKind::Add | diff::RowKind::Del))
        {
            return Some(fd.hunk_lines(cur.hunk?));
        }
        let mut s = BTreeSet::new();
        for r in &rows[a..=b.min(rows.len() - 1)] {
            if let (Some(h), Some(l), diff::RowKind::Add | diff::RowKind::Del) =
                (r.hunk, r.line, r.kind)
            {
                s.insert((h, l));
            }
        }
        Some(s)
    }
}

pub fn colorize(text: &str, theme: &Theme) -> Vec<Line<'static>> {
    text.lines()
        .map(|l| {
            let l = l.replace('\t', "    ");
            let style = if l.starts_with("+++")
                || l.starts_with("---")
                || l.starts_with("diff ")
                || l.starts_with("index ")
            {
                Style::default().add_modifier(Modifier::BOLD)
            } else if l.starts_with('+') {
                Style::default().fg(theme.added)
            } else if l.starts_with('-') {
                Style::default().fg(theme.removed)
            } else if l.starts_with("@@") {
                Style::default().fg(theme.hunk)
            } else if l.starts_with("commit ") || l.starts_with("stash@") {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            Line::styled(l, style)
        })
        .collect()
}

/// Colorize `--word-diff=plain` output: [-removed-] {+added+}.
pub fn word_colorize(text: &str, theme: &Theme) -> Vec<Line<'static>> {
    use ratatui::text::Span;
    text.lines()
        .map(|l| {
            if l.starts_with("@@") {
                return Line::styled(l.to_string(), Style::default().fg(theme.hunk));
            }
            let mut spans = Vec::new();
            let mut rest = l;
            while !rest.is_empty() {
                let next = [rest.find("[-"), rest.find("{+")]
                    .into_iter()
                    .flatten()
                    .min();
                match next {
                    Some(i) => {
                        spans.push(Span::raw(rest[..i].to_string()));
                        let (close, style) = if rest[i..].starts_with("[-") {
                            (
                                "-]",
                                Style::default()
                                    .fg(theme.removed)
                                    .add_modifier(Modifier::CROSSED_OUT),
                            )
                        } else {
                            (
                                "+}",
                                Style::default()
                                    .fg(theme.added)
                                    .add_modifier(Modifier::BOLD),
                            )
                        };
                        let body = &rest[i + 2..];
                        let end = body.find(close).unwrap_or(body.len());
                        spans.push(Span::styled(body[..end].to_string(), style));
                        rest = body.get(end + 2..).unwrap_or("");
                    }
                    None => {
                        spans.push(Span::raw(rest.to_string()));
                        rest = "";
                    }
                }
            }
            Line::from(spans)
        })
        .collect()
}
