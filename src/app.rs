//! Application state, repository loading, selection helpers and main-view previews.

use crate::action::Action;
use crate::askpass::Bridge;
use crate::config::Config;
use crate::git::diff::{self, Conflict, FileDiff, Row};
use crate::git::read::{self, CommitInfo, RepoData};
use crate::git::rebase::TodoEntry;
use crate::git::{CmdLog, Git};
use crate::highlight;
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
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
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

/// One `diff --git` section of a text view: its rows map 1:1 to `text[start..]`.
#[derive(Clone, Debug)]
pub struct DiffSection {
    pub start: usize,
    pub path: String,
    pub rows: Vec<Row>,
}

/// Visual row of a side-by-side text view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextRow {
    /// Plain text line (index into `text`).
    Line(usize),
    /// Diff rows (left, right) of a section.
    Pair(usize, Option<usize>, Option<usize>),
}

/// Side-by-side layout of a text view with diff sections.
pub fn text_rows(len: usize, sections: &[DiffSection]) -> Vec<TextRow> {
    let mut out = Vec::with_capacity(len);
    let mut i = 0;
    for (si, s) in sections.iter().enumerate() {
        out.extend((i..s.start).map(TextRow::Line));
        out.extend(
            diff::side_by_side(&s.rows)
                .into_iter()
                .map(|(l, r)| TextRow::Pair(si, l, r)),
        );
        i = s.start + s.rows.len();
    }
    out.extend((i..len).map(TextRow::Line));
    out
}

#[derive(Clone, Debug)]
pub struct MainView {
    pub title: String,
    pub kind: MainKind,
    pub text: Vec<Line<'static>>,
    /// Diff sections inside `text` (commit / stash views).
    pub sections: Vec<DiffSection>,
    /// Side-by-side layout of `text`, used when `sbs` is on.
    pub pairs: Vec<TextRow>,
    /// Side-by-side display of a text view with diff sections.
    pub sbs: bool,
    /// Raw text of a text view (re-styled when word diff is toggled).
    pub source: String,
    /// Commit shown by a commit view (re-read for full context).
    pub rev: Option<String>,
    pub cursor: usize,
    pub anchor: Option<usize>,
    pub scroll: usize,
    pub hscroll: usize,
    pub height: usize,
    /// Pending background highlight id (0 = none).
    pub hl_gen: u64,
}

impl Default for MainView {
    fn default() -> Self {
        MainView {
            title: String::new(),
            kind: MainKind::Empty,
            text: vec![],
            sections: vec![],
            pairs: vec![],
            sbs: false,
            source: String::new(),
            rev: None,
            cursor: 0,
            anchor: None,
            scroll: 0,
            hscroll: 0,
            height: 10,
            hl_gen: 0,
        }
    }
}

impl MainView {
    pub fn len(&self) -> usize {
        match &self.kind {
            MainKind::Empty => 0,
            MainKind::Text if self.sbs_text() => self.pairs.len(),
            MainKind::Text => self.text.len(),
            MainKind::Diff { rows, .. } => rows.len(),
            MainKind::Conflict { lines, .. } => lines.len(),
            MainKind::Blame { lines, .. } => lines.len(),
            MainKind::History { commits, .. } => commits.len(),
        }
    }
    /// Text view currently drawn side by side.
    pub fn sbs_text(&self) -> bool {
        self.sbs && matches!(self.kind, MainKind::Text) && !self.sections.is_empty()
    }
    /// Text line shown at visual row `v`.
    pub fn text_line(&self, v: usize) -> usize {
        if !self.sbs_text() {
            return v;
        }
        match self.pairs.get(v) {
            Some(TextRow::Line(i)) => *i,
            Some(TextRow::Pair(s, l, r)) => self.sections[*s].start + l.or(*r).unwrap_or(0),
            None => self.text.len(),
        }
    }
    /// Diff section containing text line `l`.
    pub fn section_at(&self, l: usize) -> Option<&DiffSection> {
        self.sections
            .iter()
            .find(|s| l >= s.start && l < s.start + s.rows.len())
    }
    /// Scroll so that text line `l` is at the top.
    pub fn scroll_to_line(&mut self, l: usize) {
        self.scroll = if self.sbs_text() {
            (0..self.pairs.len())
                .find(|&v| self.text_line(v) >= l)
                .unwrap_or(self.pairs.len())
        } else {
            l
        };
        self.scroll_by(0);
    }
    /// Text line starting the next (`fwd`) or previous diff section / hunk
    /// relative to the top of the view.
    pub fn text_jump(&self, fwd: bool, hunk: bool) -> Option<usize> {
        let top = self.text_line(self.scroll);
        // a hunk is shown one row below the sticky file header
        let shown = |l: usize| if hunk { l.saturating_sub(1) } else { l };
        let mut marks: Vec<usize> = vec![];
        for s in &self.sections {
            if hunk {
                marks.extend(
                    s.rows
                        .iter()
                        .enumerate()
                        .filter(|(_, r)| r.kind == diff::RowKind::HunkHeader)
                        .map(|(i, _)| s.start + i),
                );
            } else {
                marks.push(s.start);
            }
        }
        if fwd {
            marks.into_iter().find(|&m| shown(m) > top)
        } else {
            marks.into_iter().rev().find(|&m| shown(m) < top)
        }
    }
    /// Diff section listed on a `--stat` line (` path | 3 ++-`).
    pub fn stat_target(&self, l: usize) -> Option<usize> {
        let line: String = self
            .text
            .get(l)?
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        let (name, _) = line.split_once(" | ")?;
        let name = name.trim();
        let name = name.strip_prefix("...").unwrap_or(name);
        // renames are shown as `a => b` or `dir/{a => b}`
        let name = name.rsplit("=> ").next()?.trim_end_matches('}');
        self.sections
            .iter()
            .find(|s| s.path.ends_with(name))
            .map(|s| s.start)
    }
    /// Switch side-by-side display, keeping the top text line in view.
    pub fn set_sbs(&mut self, on: bool) {
        if self.sbs == on {
            return;
        }
        let top = self.text_line(self.scroll);
        self.sbs = on;
        if self.sbs_text() {
            self.scroll = (0..self.pairs.len())
                .find(|&v| self.text_line(v) >= top)
                .unwrap_or(0);
        } else if matches!(self.kind, MainKind::Text) {
            self.scroll = top;
        }
        self.scroll_by(0);
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
    /// Filterable list of the files of a commit / stash view.
    Files(FilePicker),
}

/// One file of a commit / stash view.
#[derive(Clone, Debug)]
pub struct PickerFile {
    pub path: String,
    /// First text line of its diff section.
    pub line: usize,
    pub added: usize,
    pub removed: usize,
}

#[derive(Clone, Debug)]
pub struct FilePicker {
    pub input: TextInput,
    pub files: Vec<PickerFile>,
    /// Index into `matches()`.
    pub sel: usize,
}

impl FilePicker {
    pub fn new(view: &MainView) -> Self {
        let files: Vec<PickerFile> = view
            .sections
            .iter()
            .map(|s| PickerFile {
                path: s.path.clone(),
                line: s.start,
                added: s
                    .rows
                    .iter()
                    .filter(|r| r.kind == diff::RowKind::Add)
                    .count(),
                removed: s
                    .rows
                    .iter()
                    .filter(|r| r.kind == diff::RowKind::Del)
                    .count(),
            })
            .collect();
        // preselect the file being read
        let top = view.text_line(view.scroll);
        let sel = files.iter().rposition(|f| f.line <= top).unwrap_or(0);
        FilePicker {
            input: TextInput::new("", false),
            files,
            sel,
        }
    }
    /// Indices of the files matching the query (case-insensitive subsequence).
    pub fn matches(&self) -> Vec<usize> {
        let q: Vec<char> = self.input.text().to_lowercase().chars().collect();
        self.files
            .iter()
            .enumerate()
            .filter(|(_, f)| fuzzy_match(&f.path.to_lowercase(), &q))
            .map(|(i, _)| i)
            .collect()
    }
    pub fn selected(&self) -> Option<&PickerFile> {
        self.matches().get(self.sel).map(|&i| &self.files[i])
    }
    pub fn move_sel(&mut self, d: isize) {
        let n = self.matches().len() as isize;
        self.sel = (self.sel as isize + d).clamp(0, (n - 1).max(0)) as usize;
    }
}

fn fuzzy_match(hay: &str, q: &[char]) -> bool {
    let mut it = hay.chars();
    q.iter()
        .filter(|c| !c.is_whitespace())
        .all(|c| it.any(|h| h == *c))
}

#[derive(Clone, Copy, Debug)]
pub enum Drag {
    SplitV,
    SplitH,
    Scroll(PanelId, Rect),
    /// Top popup scrollbar: track rect and max scroll offset.
    PopupScroll(Rect, usize),
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
    /// Background syntax highlighting finished (see `App::spawn_highlight`).
    Highlighted {
        gen_id: u64,
        result: Highlighted,
    },
}

pub enum HlJob {
    Rows { rows: Vec<Row>, path: String },
    Text(String),
}

pub enum Highlighted {
    Rows(Vec<Row>),
    Lines(Vec<Line<'static>>, Vec<DiffSection>),
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
    pub left_width: u16,
    pub log_height: u16,
    pub drag: Option<Drag>,
    pub last_click: Option<(Instant, u16, u16)>,
    pub tx: Sender<Event>,
    /// Id of the latest background highlight request.
    pub hl_gen: Arc<AtomicU64>,
    /// Selection changed by the wheel; preview deferred until the burst is handled.
    pub preview_pending: bool,
    pub jobs: usize,
    pub spinner: usize,
    pub cherry: Vec<(Oid, String)>,
    pub collapsed: HashSet<String>,
    pub tree_view: bool,
    pub side_by_side: bool,
    pub word_diff: bool,
    /// Diff with the whole file as context.
    pub full_file: bool,
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
            theme: Theme::from(&cfg),
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
            left_width: cfg.left_width.max(20),
            log_height: 7,
            drag: None,
            last_click: None,
            tx,
            jobs: 0,
            hl_gen: Arc::new(AtomicU64::new(0)),
            preview_pending: false,
            spinner: 0,
            cherry: vec![],
            collapsed: HashSet::new(),
            tree_view: cfg.tree_view,
            side_by_side: cfg.side_by_side,
            word_diff: cfg.word_diff,
            full_file: false,
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
        self.move_sel_lazy(p, delta);
        self.flush_preview();
    }

    /// Run a preview deferred by `move_sel_lazy`.
    pub fn flush_preview(&mut self) {
        if std::mem::take(&mut self.preview_pending) {
            self.preview();
        }
    }

    /// Move the selection; the preview is refreshed by the next `flush_preview`.
    pub fn move_sel_lazy(&mut self, p: PanelId, delta: isize) {
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
        self.preview_pending = true;
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
        let hl_gen = if self.theme.syntax.is_some() && text.contains("diff --git ") {
            self.spawn_highlight(HlJob::Text(text.to_string()))
        } else {
            0
        };
        let text_src = text.to_string();
        let (text, sections) = colorize(text, &self.theme, self.word_diff, false);
        MainView {
            title: title.into(),
            kind: MainKind::Text,
            pairs: text_rows(text.len(), &sections),
            text,
            sections,
            sbs: self.side_by_side,
            source: text_src,
            hl_gen,
            ..Default::default()
        }
    }

    /// Syntax-highlight a view in a background thread; the result comes back as
    /// `Event::Highlighted` and is dropped when another view was requested meanwhile.
    pub fn spawn_highlight(&self, job: HlJob) -> u64 {
        let gen_id = self.hl_gen.fetch_add(1, AtomicOrdering::SeqCst) + 1;
        let latest = self.hl_gen.clone();
        let (tx, theme, words) = (self.tx.clone(), self.theme.clone(), self.word_diff);
        let stale = move || latest.load(AtomicOrdering::SeqCst) != gen_id;
        std::thread::spawn(move || {
            // debounce fast selection changes (wheel / held key)
            std::thread::sleep(std::time::Duration::from_millis(30));
            if stale() {
                return;
            }
            let result = match job {
                HlJob::Rows { mut rows, path } => {
                    highlight::decorate(&mut rows, &path, &theme, words, true);
                    Highlighted::Rows(rows)
                }
                HlJob::Text(t) => {
                    let (l, s) = colorize(&t, &theme, words, true);
                    Highlighted::Lines(l, s)
                }
            };
            if !stale() {
                let _ = tx.send(Event::Highlighted { gen_id, result });
            }
        });
        gen_id
    }

    pub fn apply_highlight(&mut self, gen_id: u64, result: Highlighted) {
        if self.main.hl_gen != gen_id {
            return;
        }
        match (&mut self.main.kind, result) {
            (MainKind::Diff { rows, .. }, Highlighted::Rows(new)) if rows.len() == new.len() => {
                *rows = new
            }
            (MainKind::Text, Highlighted::Lines(l, s)) if l.len() == self.main.text.len() => {
                self.main.text = l;
                self.main.sections = s;
            }
            _ => {}
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
        let mut args = vec!["show", "--no-color", "--stat", "-p", "--format=fuller"];
        if self.full_file {
            args.push("--unified=100000000");
        }
        args.extend([rev, "--"]);
        let s = self.git.read(&args)?;
        let mut v = self.text_view(&format!("Commit {}", &rev[..rev.len().min(8)]), &s);
        v.rev = Some(rev.to_string());
        Ok(v)
    }

    /// Re-read / re-style the current text view after a display option changed.
    pub fn reload_text_view(&mut self) {
        let top = self.main.text_line(self.main.scroll);
        let file = self.main.section_at(top).map(|s| s.path.clone());
        let v = match self.main.rev.clone() {
            Some(rev) => self.commit_view(&rev),
            None => Ok(self.text_view(&self.main.title, &self.main.source)),
        };
        let mut v = match v {
            Ok(v) => v,
            Err(e) => return self.show_text("Error", &e.to_string()),
        };
        // full context moves lines: keep the same file on top
        let line = if v.text.len() == self.main.text.len() {
            top
        } else {
            file.and_then(|p| v.sections.iter().find(|s| s.path == p))
                .map_or(0, |s| s.start)
        };
        v.height = self.main.height;
        v.hscroll = self.main.hscroll;
        self.main = v;
        self.main.scroll_to_line(line);
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
        if self.full_file {
            args.push("--unified=100000000");
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
        let fd = diff::parse(&text);
        let mut rows = fd.rows();
        highlight::decorate(&mut rows, &f.path, &self.theme, self.word_diff, false);
        let hl_gen = match &self.theme.syntax {
            Some(_) => self.spawn_highlight(HlJob::Rows {
                rows: rows.clone(),
                path: f.path.clone(),
            }),
            None => 0,
        };
        let mut v = MainView {
            title,
            kind: MainKind::Diff {
                path: f.path.clone(),
                staged,
                untracked,
                fd,
                rows,
            },
            hl_gen,
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

/// Style a text view; `diff --git` sections are also returned as decorated rows.
pub fn colorize(
    text: &str,
    theme: &Theme,
    words: bool,
    syntax: bool,
) -> (Vec<Line<'static>>, Vec<DiffSection>) {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::with_capacity(lines.len());
    let mut sections = vec![];
    let mut i = 0;
    while i < lines.len() {
        if lines[i].starts_with("diff --git ") {
            // one file section: up to the next `diff --git`
            let end = (i + 1..lines.len())
                .find(|&k| lines[k].starts_with("diff --git "))
                .unwrap_or(lines.len());
            let section = &lines[i..end];
            let path = section
                .iter()
                .find_map(|l| l.strip_prefix("+++ b/"))
                .or_else(|| lines[i].rsplit_once(" b/").map(|(_, p)| p))
                .unwrap_or("");
            let mut rows = diff::parse(&section.join("\n")).rows();
            highlight::decorate(&mut rows, path, theme, words, syntax);
            out.extend(rows.iter().map(|r| highlight::row_line(r, theme)));
            sections.push(DiffSection {
                start: i,
                path: path.to_string(),
                rows,
            });
            i = end;
            continue;
        }
        let l = lines[i].replace('\t', "    ");
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
        out.push(Line::styled(l, style));
        i += 1;
    }
    (out, sections)
}

#[cfg(test)]
mod colorize_tests {
    use super::*;

    #[test]
    fn commit_view_diff_is_decorated() {
        let theme = Theme::from(&Config::default());
        let text = "commit abc\nAuthor: x\n\ndiff --git a/f.rs b/f.rs\n--- a/f.rs\n+++ b/f.rs\n@@ -1 +1 @@\n-let a = 1;\n+let a = 2;\n";
        let (lines, sections) = colorize(text, &theme, true, true);
        assert_eq!(lines.len(), text.lines().count());
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].start, 3);
        assert_eq!(sections[0].path, "f.rs");
        assert_eq!(sections[0].start + sections[0].rows.len(), lines.len());
        let pairs = text_rows(lines.len(), &sections);
        assert_eq!(
            pairs[..3],
            [TextRow::Line(0), TextRow::Line(1), TextRow::Line(2)]
        );
        assert_eq!(*pairs.last().unwrap(), TextRow::Pair(0, Some(4), Some(5)));
        let add = lines.last().unwrap();
        assert_eq!(add.style.bg, Some(theme.added_bg));
        assert!(
            add.spans
                .iter()
                .any(|s| s.style.bg == Some(theme.added_word_bg))
        );
        let flat: String = add.spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(flat, "+let a = 2;");
    }

    #[test]
    fn text_view_navigation() {
        let theme = Theme::from(&Config::default());
        let text = "commit abc\n\n a.rs | 2 +-\n .../b.rs | 1 +\n\n\
diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n-x\n+y\n@@ -9 +9 @@\n-z\n+w\n\
diff --git a/d/b.rs b/d/b.rs\n--- a/d/b.rs\n+++ b/d/b.rs\n@@ -1 +1,2 @@\n a\n+b\n";
        let (text, sections) = colorize(text, &theme, false, false);
        let mut v = MainView {
            kind: MainKind::Text,
            pairs: text_rows(text.len(), &sections),
            text,
            sections,
            height: 3,
            ..Default::default()
        };
        assert_eq!(v.stat_target(2), Some(5));
        assert_eq!(v.stat_target(3), Some(14));
        assert_eq!(v.text_jump(true, false), Some(5));
        assert_eq!(v.text_jump(true, true), Some(8));
        v.scroll_to_line(7);
        assert_eq!(v.text_jump(true, true), Some(11));
        assert_eq!(v.text_jump(false, false), Some(5));
        v.set_sbs(true);
        assert_eq!(v.text_line(v.scroll), 7);
        v.scroll_to_line(14);
        assert_eq!(v.text_line(v.scroll), 14);
        // file picker: preselects the file on top, filters by subsequence
        let mut fp = FilePicker::new(&v);
        assert_eq!(fp.sel, 1);
        assert_eq!(fp.files[1].path, "d/b.rs");
        assert_eq!((fp.files[0].added, fp.files[0].removed), (2, 2));
        fp.input.insert_str("DB");
        assert_eq!(fp.matches(), vec![1]);
        fp.sel = 0;
        assert_eq!(fp.selected().map(|f| f.line), Some(14));
        fp.input.insert_str("x");
        assert!(fp.selected().is_none());
    }
}
