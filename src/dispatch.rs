//! Action → operation mapping.

use crate::action::Action;
use crate::app::*;
use crate::git::diff::{self, Pick, RowKind};
use crate::git::rebase::{TodoAction, TodoEntry};
use crate::ops::{git_bg, git_op, sv};
use crate::textinput::TextInput;

const PANEL_ORDER: [PanelId; 6] = [
    PanelId::Status,
    PanelId::Files,
    PanelId::Branches,
    PanelId::Commits,
    PanelId::Stash,
    PanelId::Main,
];

impl App {
    pub fn focus_panel(&mut self, p: PanelId) {
        if p != PanelId::Main && p != PanelId::Log {
            self.last_left = p;
        }
        let was_main = self.focus == PanelId::Main;
        self.focus = p;
        if p != PanelId::Main && was_main {
            self.main.anchor = None;
            self.preview();
        } else if p != PanelId::Main {
            self.preview();
        }
    }

    pub fn set_tab(&mut self, p: PanelId, t: usize) {
        let n = p.tabs().len();
        self.tabs[p.index()] = t % n;
        self.focus_panel(p);
    }

    fn page(&self) -> isize {
        let h = if self.focus == PanelId::Main {
            self.main.height
        } else {
            self.list(self.focus).height
        };
        (h.max(2) - 1) as isize
    }

    pub fn dispatch(&mut self, a: Action) {
        use Action::*;
        let p = self.focus;
        match a {
            Quit => self.quit = true,
            Help => self.popups.push(Popup::Help { scroll: 0 }),
            NextPanel | PrevPanel => {
                let i = PANEL_ORDER.iter().position(|x| *x == p).unwrap_or(0) as isize;
                let d = if a == NextPanel { 1 } else { -1 };
                let n = PANEL_ORDER.len() as isize;
                self.focus_panel(PANEL_ORDER[((i + d + n) % n) as usize]);
            }
            FocusStatus => self.focus_panel(PanelId::Status),
            FocusFiles => self.focus_panel(PanelId::Files),
            FocusBranches => self.focus_panel(PanelId::Branches),
            FocusCommits => self.focus_panel(PanelId::Commits),
            FocusStash => self.focus_panel(PanelId::Stash),
            FocusMain => self.focus_panel(PanelId::Main),
            NextTab | PrevTab => {
                let tp = if p == PanelId::Main {
                    self.last_left
                } else {
                    p
                };
                let n = tp.tabs().len();
                let t = self.tabs[tp.index()];
                self.set_tab(tp, if a == NextTab { t + 1 } else { t + n - 1 });
            }
            Refresh => {
                self.refresh();
                self.info("refreshed");
            }
            Push => self.run_op(Op::Push { force: false }),
            ForcePush => self.confirm(
                "Force push",
                "Push with --force-with-lease?",
                Op::Push { force: true },
            ),
            Pull => self.run_op(git_bg(&["pull"])),
            PullRebase => self.run_op(git_bg(&["pull", "--rebase"])),
            PullFfOnly => self.run_op(git_bg(&["pull", "--ff-only"])),
            Fetch => self.run_op(git_bg(&["fetch", "--all", "--prune"])),
            Undo => match self.data.reflog.first() {
                Some(e) => {
                    let msg = format!(
                        "Undo last action?\n\n{}\n\n(checkout → previous branch, otherwise git reset --keep HEAD@{{1}})",
                        e.message
                    );
                    self.confirm("Undo", msg, Op::Undo)
                }
                None => self.info("nothing to undo"),
            },
            ToggleLog => self.show_log = !self.show_log,
            CustomCommand => {
                self.input("git command (without 'git')", "", InputKind::CustomCommand)
            }
            Filter => {
                let tp = if p == PanelId::Main {
                    self.last_left
                } else {
                    p
                };
                let cur = self.list(tp).filter.clone();
                self.input("Filter", &cur, InputKind::Filter(tp));
            }
            Up => self.nav(-1),
            Down => self.nav(1),
            PageUp => self.nav(-self.page()),
            PageDown => self.nav(self.page()),
            Top => self.nav(-1_000_000),
            Bottom => self.nav(1_000_000),
            Enter => self.enter(),
            ContextMenu => self.context_menu(),
            RepoStateMenu => self.repo_state_menu(),
            CopySha => self.copy(),
            RecentRepo => {
                let mut items: Vec<MenuItem> = crate::config::recent()
                    .into_iter()
                    .filter(|r| std::path::Path::new(r) != self.data.workdir)
                    .take(9)
                    .enumerate()
                    .map(|(i, r)| {
                        MenuItem::new(
                            char::from_digit(i as u32 + 1, 10).unwrap(),
                            r.clone(),
                            MenuCmd::Op(Op::OpenRepo(r.into())),
                        )
                    })
                    .collect();
                items.push(MenuItem::new(
                    'o',
                    "open path…",
                    MenuCmd::Input(InputKind::OpenRepo, "Repository path".into(), String::new()),
                ));
                self.menu("Open repository", items);
            }
            ParentRepo => match self.repo_stack.pop() {
                Some(path) => self.run_op(Op::OpenRepo(path)),
                None => self.info("no parent repository"),
            },

            // ---------- files ----------
            StageToggle => self.stage_toggle(),
            StageAll => {
                let any_unstaged = self.data.files.iter().any(|f| f.has_unstaged());
                if any_unstaged {
                    self.run_op(git_op(&["add", "-A"]));
                } else {
                    self.run_op(git_op(&["reset", "-q"]));
                }
            }
            Discard => self.discard_menu(),
            Commit => self.open_commit(false),
            Amend => {
                if self.data.commits.is_empty() {
                    return self.info("no commit to amend");
                }
                self.confirm(
                    "Amend",
                    "Amend last commit with staged changes (keep message)?",
                    Op::Commit {
                        msg: String::new(),
                        amend: true,
                        no_verify: false,
                        signoff: false,
                        only: false,
                        stage_all: false,
                    },
                );
            }
            Ignore => {
                if let Some(r) = self.sel_file_row() {
                    let path = self.file_row_path(&r);
                    self.confirm(
                        "Ignore",
                        format!("Add /{path} to .gitignore?"),
                        Op::Ignore(path),
                    );
                }
            }
            ToggleTree => {
                self.tree_view = !self.tree_view;
                self.list_mut(PanelId::Files).sel = 0;
                self.preview();
            }
            FileHistory => self.file_history(),
            Blame => self.blame(),
            EditFile => self.edit_file(),
            UseOurs | UseTheirs => {
                if let Some(f) = self
                    .sel_file()
                    .filter(|f| f.conflicted)
                    .map(|f| f.path.clone())
                {
                    let side = if a == UseOurs { "--ours" } else { "--theirs" };
                    self.run_op(Op::Git {
                        cmds: vec![sv(&["checkout", side, "--", &f]), sv(&["add", "--", &f])],
                        bg: false,
                        then: Then::Refresh,
                    });
                } else {
                    self.info("select a conflicted file");
                }
            }
            StashAll => self.input(
                "Stash message (optional)",
                "",
                InputKind::StashMessage(sv(&["--include-untracked"])),
            ),
            StashMenu => {
                let item = |k, l: &str, flags: &[&str]| {
                    MenuItem::new(
                        k,
                        l,
                        MenuCmd::Input(
                            InputKind::StashMessage(sv(flags)),
                            "Stash message (optional)".into(),
                            String::new(),
                        ),
                    )
                };
                self.menu(
                    "Stash",
                    vec![
                        item(
                            'a',
                            "stash all changes (incl. untracked)",
                            &["--include-untracked"],
                        ),
                        item('t', "stash tracked changes", &[]),
                        item('k', "stash unstaged only (--keep-index)", &["--keep-index"]),
                        item(
                            's',
                            "stash staged only (--staged, git ≥ 2.35)",
                            &["--staged"],
                        ),
                    ],
                );
            }
            ResetMenu => {
                let c = |k, l: &str, args: &[&str]| {
                    MenuItem::new(k, l, MenuCmd::Confirm(format!("{l}?"), git_op(args)))
                };
                self.menu(
                    "Reset",
                    vec![
                        c('u', "unstage all", &["reset", "-q"]),
                        c(
                            'd',
                            "discard unstaged changes (checkout -- .)",
                            &["checkout", "--", "."],
                        ),
                        c('c', "remove untracked files (clean -fd)", &["clean", "-fd"]),
                        c('h', "hard reset to HEAD", &["reset", "--hard", "HEAD"]),
                        MenuItem::new(
                            'n',
                            "nuke working tree (reset --hard + clean -fd)",
                            MenuCmd::Confirm(
                                "Discard ALL changes and untracked files?".into(),
                                Op::Git {
                                    cmds: vec![
                                        sv(&["reset", "--hard", "HEAD"]),
                                        sv(&["clean", "-fd"]),
                                    ],
                                    bg: false,
                                    then: Then::Refresh,
                                },
                            ),
                        ),
                        c(
                            's',
                            "soft reset HEAD~1 (undo commit, keep staged)",
                            &["reset", "--soft", "HEAD~1"],
                        ),
                        c(
                            'm',
                            "mixed reset HEAD~1 (undo commit, keep changes)",
                            &["reset", "--mixed", "HEAD~1"],
                        ),
                    ],
                );
            }

            // ---------- branches / tags / remotes ----------
            Checkout => self.checkout_selected(),
            CheckoutByName => self.input("Checkout branch / rev", "", InputKind::CheckoutByName),
            NewBranch => {
                let from = self.selected_ref();
                let title = match &from {
                    Some(f) => format!("New branch from {f}"),
                    None => "New branch".into(),
                };
                let init = match (self.tabs[1], &from) {
                    (1, Some(f)) => f
                        .split_once('/')
                        .map(|x| x.1.to_string())
                        .unwrap_or_default(),
                    _ => String::new(),
                };
                self.input(&title, &init, InputKind::NewBranch { from });
            }
            DeleteBranch => self.delete_branch(),
            RenameBranch => {
                if let Some(b) = self.sel_local_branch() {
                    self.input(
                        &format!("Rename {b}"),
                        &b,
                        InputKind::RenameBranch(b.clone()),
                    );
                }
            }
            MergeBranch => {
                if let Some(r) = self.selected_ref() {
                    let op = Op::Git {
                        cmds: vec![sv(&["merge", "--no-edit", &r])],
                        bg: true,
                        then: Then::Refresh,
                    };
                    self.menu(
                        &format!("Merge {r} into {}", self.data.head),
                        vec![
                            MenuItem::new('m', "merge", MenuCmd::Op(op)),
                            MenuItem::new(
                                'n',
                                "merge --no-ff",
                                MenuCmd::Op(git_bg(&["merge", "--no-ff", "--no-edit", &r])),
                            ),
                            MenuItem::new(
                                'f',
                                "merge --ff-only",
                                MenuCmd::Op(git_bg(&["merge", "--ff-only", &r])),
                            ),
                            MenuItem::new(
                                's',
                                "squash merge (stage only)",
                                MenuCmd::Op(git_bg(&["merge", "--squash", &r])),
                            ),
                        ],
                    );
                }
            }
            RebaseOnto => {
                if let Some(r) = self.selected_ref() {
                    self.confirm(
                        "Rebase",
                        format!("Rebase {} onto {r}?", self.data.head),
                        git_bg(&["rebase", "--autostash", &r]),
                    );
                }
            }
            SetUpstream => {
                if let Some(b) = self.sel_local_branch() {
                    let init = format!("{}/{b}", self.cfg.default_remote);
                    self.menu(
                        &format!("Upstream of {b}"),
                        vec![
                            MenuItem::new(
                                's',
                                "set upstream…",
                                MenuCmd::Input(
                                    InputKind::SetUpstream(b.clone()),
                                    "Upstream (remote/branch)".into(),
                                    init,
                                ),
                            ),
                            MenuItem::new(
                                'u',
                                "unset upstream",
                                MenuCmd::Op(git_op(&["branch", "--unset-upstream", &b])),
                            ),
                        ],
                    );
                }
            }
            FastForward => {
                let Some(i) = self
                    .selected(PanelId::Branches)
                    .filter(|_| self.tabs[2] == 0)
                else {
                    return;
                };
                let b = self.data.branches[i].clone();
                let Some(up) = b.upstream.clone() else {
                    return self.info("branch has no upstream");
                };
                if b.is_head {
                    self.run_op(git_bg(&["merge", "--ff-only", &up]));
                } else if let Some((remote, rb)) = up.split_once('/') {
                    self.run_op(git_bg(&["fetch", remote, &format!("{rb}:{}", b.name)]));
                }
            }
            NewTag | TagCommit => {
                let at = if a == TagCommit {
                    self.selected_ref()
                } else {
                    None
                };
                self.input("Tag name", "", InputKind::NewTag { at });
            }
            DeleteTag => {
                if let Some(t) = self
                    .selected(PanelId::Branches)
                    .and_then(|i| self.data.tags.get(i))
                    .map(|t| t.name.clone())
                {
                    let mut items = vec![MenuItem::new(
                        'l',
                        "delete local tag",
                        MenuCmd::Confirm(format!("Delete tag {t}?"), git_op(&["tag", "-d", &t])),
                    )];
                    for r in &self.data.remotes {
                        items.push(MenuItem::new(
                            'r',
                            format!("delete on remote {}", r.name),
                            MenuCmd::Confirm(
                                format!("Delete tag {t} on {}?", r.name),
                                git_bg(&["push", &r.name, "--delete", &format!("refs/tags/{t}")]),
                            ),
                        ));
                    }
                    self.menu(&format!("Delete tag {t}"), items);
                }
            }
            PushTag => {
                if let Some(t) = self
                    .selected(PanelId::Branches)
                    .and_then(|i| self.data.tags.get(i))
                    .map(|t| t.name.clone())
                {
                    let items = self
                        .data
                        .remotes
                        .iter()
                        .enumerate()
                        .map(|(i, r)| {
                            MenuItem::new(
                                char::from_digit(i as u32 + 1, 10).unwrap_or('r'),
                                format!("push to {}", r.name),
                                MenuCmd::Op(git_bg(&["push", &r.name, &format!("refs/tags/{t}")])),
                            )
                        })
                        .collect();
                    self.menu(&format!("Push tag {t}"), items);
                }
            }
            AddRemote => self.input("Remote name", "", InputKind::AddRemoteName),
            RemoveRemote => {
                if let Some(r) = self.sel_remote() {
                    self.confirm(
                        "Remove remote",
                        format!("Remove remote {r}?"),
                        git_op(&["remote", "remove", &r]),
                    );
                }
            }
            EditRemote => {
                if let Some(r) = self
                    .selected(PanelId::Branches)
                    .and_then(|i| self.data.remotes.get(i))
                    .cloned()
                {
                    self.input(
                        &format!("URL of {}", r.name),
                        &r.url,
                        InputKind::EditRemoteUrl(r.name),
                    );
                }
            }
            FetchRemote => {
                if let Some(r) = self.sel_remote() {
                    self.run_op(git_bg(&["fetch", "--prune", &r]));
                }
            }

            // ---------- commits ----------
            CheckoutCommit => {
                if let Some((oid, _)) = self.sel_commit() {
                    self.confirm(
                        "Checkout",
                        format!("Checkout {} (detached HEAD)?", &oid.to_string()[..8]),
                        git_op(&["checkout", &oid.to_string()]),
                    );
                }
            }
            CherryCopy => {
                if let Some((oid, s)) = self.sel_commit() {
                    if let Some(i) = self.cherry.iter().position(|(o, _)| *o == oid) {
                        self.cherry.remove(i);
                    } else {
                        self.cherry.push((oid, s));
                    }
                    self.info(format!(
                        "{} commit(s) copied for cherry-pick",
                        self.cherry.len()
                    ));
                    self.move_sel(PanelId::Commits, 1);
                }
            }
            CherryClear => {
                self.cherry.clear();
                self.info("cherry-pick selection cleared");
            }
            CherryPaste => {
                if self.cherry.is_empty() {
                    return self.info("nothing copied (press C on commits first)");
                }
                let mut list = self.cherry.clone();
                // oldest first: by position in commit list (higher index = older)
                list.sort_by_key(|(o, _)| std::cmp::Reverse(self.commit_index(*o).unwrap_or(0)));
                let mut args = sv(&["cherry-pick"]);
                args.extend(list.iter().map(|(o, _)| o.to_string()));
                let msg = list
                    .iter()
                    .map(|(o, s)| format!("{} {s}", &o.to_string()[..8]))
                    .collect::<Vec<_>>()
                    .join("\n");
                self.cherry.clear();
                self.confirm(
                    "Cherry-pick",
                    format!("Cherry-pick onto {}:\n\n{msg}", self.data.head),
                    Op::Git {
                        cmds: vec![args],
                        bg: true,
                        then: Then::Refresh,
                    },
                );
            }
            Revert => {
                if let Some((oid, s)) = self.sel_commit() {
                    let mut args = sv(&["revert", "--no-edit"]);
                    if self
                        .commit_index(oid)
                        .map(|i| self.data.commits[i].parents.len() > 1)
                        .unwrap_or(false)
                    {
                        args.extend(sv(&["-m", "1"]));
                    }
                    args.push(oid.to_string());
                    self.confirm(
                        "Revert",
                        format!("Revert {} {s}?", &oid.to_string()[..8]),
                        Op::Git {
                            cmds: vec![args],
                            bg: true,
                            then: Then::Refresh,
                        },
                    );
                }
            }
            ResetToCommit => {
                if let Some((oid, _)) = self.sel_commit() {
                    let o = oid.to_string();
                    let short = &o[..8];
                    self.menu(
                        &format!("Reset {} to {short}", self.data.head),
                        vec![
                            MenuItem::new(
                                's',
                                "soft (keep index + worktree)",
                                MenuCmd::Op(git_op(&["reset", "--soft", &o])),
                            ),
                            MenuItem::new(
                                'm',
                                "mixed (keep worktree)",
                                MenuCmd::Op(git_op(&["reset", "--mixed", &o])),
                            ),
                            MenuItem::new(
                                'k',
                                "keep (keep local changes, safe)",
                                MenuCmd::Op(git_op(&["reset", "--keep", &o])),
                            ),
                            MenuItem::new(
                                'h',
                                "hard (discard everything)",
                                MenuCmd::Confirm(
                                    format!("Hard reset to {short}? Local changes are lost."),
                                    git_op(&["reset", "--hard", &o]),
                                ),
                            ),
                        ],
                    );
                }
            }
            InteractiveRebase => self.start_rebase(None),
            Reword => {
                let Some((oid, _)) = self.sel_commit() else {
                    return;
                };
                let msg = self
                    .git
                    .read(&["log", "-1", "--format=%B", &oid.to_string()])
                    .unwrap_or_default();
                let is_head = self.data.commits.first().map(|c| c.oid) == Some(oid);
                if !is_head && self.rebase_range(oid).is_none() {
                    return;
                }
                self.popups.push(Popup::Commit(CommitPopup {
                    input: TextInput::new(msg.trim_end(), true),
                    amend: is_head,
                    no_verify: false,
                    signoff: false,
                    reword: if is_head { None } else { Some(oid) },
                    only: is_head,
                }));
            }
            Fixup => {
                if let Some((oid, _)) = self.sel_commit() {
                    if !self.data.files.iter().any(|f| f.has_staged()) {
                        return self.info("stage changes first to create a fixup commit");
                    }
                    self.run_op(git_op(&["commit", "--fixup", &oid.to_string()]));
                }
            }
            AutosquashRebase => {
                if let Some((oid, _)) = self.sel_commit() {
                    let o = oid.to_string();
                    self.confirm(
                        "Autosquash",
                        format!("Rebase --autosquash on top of {}?", &o[..8]),
                        Op::Git {
                            cmds: vec![sv(&[
                                "-c",
                                "sequence.editor=true",
                                "rebase",
                                "-i",
                                "--autosquash",
                                "--autostash",
                                &o,
                            ])],
                            bg: true,
                            then: Then::Refresh,
                        },
                    );
                }
            }
            Squash | DropCommit => {
                let Some((oid, s)) = self.sel_commit() else {
                    return;
                };
                let act = if a == Squash {
                    TodoAction::Squash
                } else {
                    TodoAction::Drop
                };
                let Some((mut entries, base)) = self.rebase_range(oid) else {
                    return;
                };
                if act == TodoAction::Squash && entries.len() < 2 {
                    return self.info("cannot squash the root commit");
                }
                if act == TodoAction::Squash {
                    // squash needs the parent in the todo as well
                    let Some(parent) = self
                        .commit_index(oid)
                        .and_then(|i| self.data.commits.get(i + 1))
                        .map(|c| c.oid)
                    else {
                        return;
                    };
                    let Some((e2, b2)) = self.rebase_range(parent) else {
                        return;
                    };
                    entries = e2;
                    let base2 = b2;
                    if let Some(e) = entries.iter_mut().find(|e| e.oid == oid) {
                        e.action = act;
                    }
                    self.confirm(
                        "Squash",
                        format!("Squash '{s}' into its parent?"),
                        Op::Rebase {
                            base: base2,
                            todo: entries,
                        },
                    );
                } else {
                    if let Some(e) = entries.iter_mut().find(|e| e.oid == oid) {
                        e.action = act;
                    }
                    self.confirm(
                        "Drop commit",
                        format!("Drop '{s}'?"),
                        Op::Rebase {
                            base,
                            todo: entries,
                        },
                    );
                }
            }
            BranchFromCommit => {
                if let Some((oid, _)) = self.sel_commit() {
                    self.input(
                        "New branch at commit",
                        "",
                        InputKind::BranchAt(oid.to_string()),
                    );
                }
            }

            // ---------- stash ----------
            StashApply | StashPop | StashDrop => {
                let Some(s) = self
                    .selected(PanelId::Stash)
                    .and_then(|i| self.data.stashes.get(i))
                    .cloned()
                else {
                    return;
                };
                let r = format!("stash@{{{}}}", s.index);
                match a {
                    StashApply => self.run_op(git_op(&["stash", "apply", &r])),
                    StashPop => self.confirm(
                        "Pop stash",
                        format!("Pop {r}?"),
                        git_op(&["stash", "pop", &r]),
                    ),
                    _ => self.confirm(
                        "Drop stash",
                        format!("Drop {r}: {}?", s.message),
                        git_op(&["stash", "drop", &r]),
                    ),
                }
            }
            StashBranch => {
                if let Some(s) = self
                    .selected(PanelId::Stash)
                    .and_then(|i| self.data.stashes.get(i))
                {
                    let i = s.index;
                    self.input("Branch name for stash", "", InputKind::StashBranch(i));
                }
            }

            // ---------- submodules / worktrees ----------
            EnterSubmodule => {
                if let Some(s) = self
                    .selected(PanelId::Stash)
                    .and_then(|i| self.data.submodules.get(i))
                    .cloned()
                {
                    let path = self.data.workdir.join(&s.path);
                    if s.status == "uninitialized" {
                        return self.info("submodule not initialized (press u to update)");
                    }
                    self.repo_stack.push(self.data.workdir.clone());
                    self.run_op(Op::OpenRepo(path));
                }
            }
            SubmoduleUpdate | SubmoduleInit => {
                if let Some(s) = self
                    .selected(PanelId::Stash)
                    .and_then(|i| self.data.submodules.get(i))
                    .cloned()
                {
                    let p = s.path.to_string_lossy().into_owned();
                    if a == SubmoduleUpdate {
                        self.run_op(git_bg(&[
                            "submodule",
                            "update",
                            "--init",
                            "--recursive",
                            "--",
                            &p,
                        ]));
                    } else {
                        self.run_op(git_op(&["submodule", "init", "--", &p]));
                    }
                }
            }
            SubmoduleAdd => self.input("Submodule URL", "", InputKind::SubmoduleUrl),
            SwitchWorktree => {
                if let Some(w) = self
                    .selected(PanelId::Stash)
                    .and_then(|i| self.data.worktrees.get(i))
                    .cloned()
                    && !w.current
                {
                    self.run_op(Op::OpenRepo(w.path));
                }
            }
            AddWorktree => self.input("New worktree path", "", InputKind::AddWorktreePath),
            RemoveWorktree => {
                if let Some(w) = self
                    .selected(PanelId::Stash)
                    .and_then(|i| self.data.worktrees.get(i))
                    .cloned()
                {
                    if w.current {
                        return self.info("cannot remove the current worktree");
                    }
                    let p = w.path.to_string_lossy().into_owned();
                    self.confirm(
                        "Remove worktree",
                        format!("Remove worktree {p}?"),
                        git_op(&["worktree", "remove", &p]),
                    );
                }
            }

            // ---------- main ----------
            StageSelection | StageHunk => self.stage_selection(a == StageHunk, false),
            DiscardSelection => self.stage_selection(false, true),
            RangeSelect => {
                self.main.anchor = match self.main.anchor {
                    Some(_) => None,
                    None => Some(self.main.cursor),
                }
            }
            ToggleStagedView => {
                if let MainKind::Diff { staged, .. } = self.main.kind {
                    self.force_staged = Some(!staged);
                    self.reload_main_file();
                }
            }
            SideBySide => self.side_by_side = !self.side_by_side,
            WordDiff => {
                self.word_diff = !self.word_diff;
                self.reload_main_file();
            }
            NextHunk | PrevHunk => {
                if let MainKind::Diff { rows, .. } = &self.main.kind {
                    let c = self.main.cursor;
                    let pos = if a == NextHunk {
                        rows.iter()
                            .enumerate()
                            .skip(c + 1)
                            .find(|(_, r)| r.kind == RowKind::HunkHeader)
                            .map(|(i, _)| i)
                    } else {
                        rows.iter()
                            .enumerate()
                            .take(c)
                            .rev()
                            .find(|(_, r)| r.kind == RowKind::HunkHeader)
                            .map(|(i, _)| i)
                    };
                    if let Some(i) = pos {
                        self.main.set_cursor(i);
                    }
                }
            }
            ScrollLeft => self.main.hscroll = self.main.hscroll.saturating_sub(8),
            ScrollRight => self.main.hscroll += 8,
            PickOurs => self.pick_conflict(Pick::Ours, None),
            PickTheirs => self.pick_conflict(Pick::Theirs, None),
            PickBoth => self.pick_conflict(Pick::Both, None),
            NextConflict | PrevConflict => {
                if let MainKind::Conflict { conflicts, .. } = &self.main.kind {
                    let c = self.main.cursor;
                    let pos = if a == NextConflict {
                        conflicts.iter().find(|x| x.start > c).map(|x| x.start)
                    } else {
                        conflicts
                            .iter()
                            .rev()
                            .find(|x| x.start < c)
                            .map(|x| x.start)
                    };
                    if let Some(i) = pos {
                        self.main.set_cursor(i);
                    }
                }
            }
            MarkResolved => {
                if let MainKind::Conflict {
                    path, conflicts, ..
                } = &self.main.kind
                {
                    let path = path.clone();
                    if conflicts.is_empty() {
                        self.run_op(git_op(&["add", "--", &path]));
                        self.focus_panel(PanelId::Files);
                    } else {
                        self.confirm(
                            "Mark resolved",
                            format!(
                                "{path} still has {} conflict(s). Mark resolved anyway?",
                                conflicts.len()
                            ),
                            git_op(&["add", "--", &path]),
                        );
                    }
                }
            }
        }
    }

    fn nav(&mut self, d: isize) {
        match self.focus {
            PanelId::Main => self.main.move_cursor(d),
            PanelId::Log => self.log_scroll = (self.log_scroll as isize - d).max(0) as usize,
            p => self.move_sel(p, d),
        }
    }

    fn enter(&mut self) {
        match self.focus {
            PanelId::Main => self.main_enter(),
            PanelId::Files => match self.sel_file_row() {
                Some(FileRow::Dir { path, .. }) => {
                    if !self.collapsed.remove(&path) {
                        self.collapsed.insert(path);
                    }
                    self.preview();
                }
                Some(_) => self.focus_panel(PanelId::Main),
                None => {}
            },
            PanelId::Stash if self.tabs[4] == 1 => self.dispatch(Action::EnterSubmodule),
            PanelId::Stash if self.tabs[4] == 2 => self.dispatch(Action::SwitchWorktree),
            PanelId::Status => self.dispatch(Action::RecentRepo),
            _ => self.focus_panel(PanelId::Main),
        }
    }

    pub fn main_enter(&mut self) {
        let next = match &self.main.kind {
            MainKind::Blame { lines, .. } => lines
                .get(self.main.cursor)
                .filter(|l| !l.0.is_zero())
                .map(|l| l.0.to_string()),
            MainKind::History { commits, path } => commits
                .get(self.main.cursor)
                .map(|c| format!("{}\0{path}", c.oid)),
            _ => None,
        };
        if let Some(rev) = next {
            let v = match rev.split_once('\0') {
                Some((r, path)) => self
                    .git
                    .read(&["show", "--no-color", "--format=fuller", r, "--", path])
                    .map(|s| self.text_view(&format!("{} {path}", &r[..8]), &s)),
                None => self.commit_view(&rev),
            };
            match v {
                Ok(v) => {
                    let old = std::mem::replace(&mut self.main, v);
                    self.main.height = old.height;
                    self.main_stack.push(old);
                }
                Err(e) => self.error("show", e.to_string()),
            }
        }
    }

    pub fn main_back(&mut self) -> bool {
        if let Some(v) = self.main_stack.pop() {
            self.main = v;
            true
        } else {
            false
        }
    }

    fn sel_local_branch(&self) -> Option<String> {
        if self.tabs[2] != 0 {
            return None;
        }
        self.selected(PanelId::Branches)
            .and_then(|i| self.data.branches.get(i))
            .map(|b| b.name.clone())
    }

    fn sel_remote(&self) -> Option<String> {
        if self.tabs[2] != 3 {
            return None;
        }
        self.selected(PanelId::Branches)
            .and_then(|i| self.data.remotes.get(i))
            .map(|r| r.name.clone())
    }

    /// A ref name / sha usable as a revision for the focused selection.
    pub fn selected_ref(&self) -> Option<String> {
        let p = if self.focus == PanelId::Main {
            self.last_left
        } else {
            self.focus
        };
        match p {
            PanelId::Branches => {
                let i = self.selected(PanelId::Branches)?;
                match self.tabs[2] {
                    0 => self.data.branches.get(i).map(|b| b.name.clone()),
                    1 => self.data.remote_branches.get(i).map(|b| b.name.clone()),
                    2 => self.data.tags.get(i).map(|t| t.name.clone()),
                    _ => None,
                }
            }
            PanelId::Commits => self.sel_commit().map(|(o, _)| o.to_string()),
            _ => None,
        }
    }

    fn checkout_selected(&mut self) {
        let Some(i) = self.selected(PanelId::Branches) else {
            return;
        };
        match self.tabs[2] {
            0 => {
                let b = self.data.branches[i].name.clone();
                self.run_op(git_op(&["checkout", &b]));
            }
            1 => {
                let rb = self.data.remote_branches[i].name.clone();
                let local = rb
                    .split_once('/')
                    .map(|x| x.1.to_string())
                    .unwrap_or(rb.clone());
                if self.data.branches.iter().any(|b| b.name == local) {
                    self.run_op(git_op(&["checkout", &local]));
                } else {
                    self.run_op(git_op(&["checkout", "-b", &local, "--track", &rb]));
                }
            }
            2 => {
                let t = self.data.tags[i].name.clone();
                self.confirm(
                    "Checkout tag",
                    format!("Checkout tag {t} (detached HEAD)?"),
                    git_op(&["checkout", &format!("tags/{t}")]),
                );
            }
            _ => self.dispatch(Action::FetchRemote),
        }
    }

    fn delete_branch(&mut self) {
        let Some(i) = self.selected(PanelId::Branches) else {
            return;
        };
        match self.tabs[2] {
            0 => {
                let b = self.data.branches[i].clone();
                if b.is_head {
                    return self.info("cannot delete the checked out branch");
                }
                let mut items = vec![MenuItem::new(
                    'l',
                    "delete local branch",
                    MenuCmd::Confirm(
                        format!("Delete branch {}?", b.name),
                        Op::Git {
                            cmds: vec![sv(&["branch", "-d", &b.name])],
                            bg: false,
                            then: Then::DeleteBranch(b.name.clone()),
                        },
                    ),
                )];
                if let Some(up) = &b.upstream
                    && let Some((remote, rb)) = up.split_once('/')
                {
                    items.push(MenuItem::new(
                        'r',
                        format!("delete remote branch {up}"),
                        MenuCmd::Confirm(
                            format!("Delete remote branch {up}?"),
                            git_bg(&["push", remote, "--delete", rb]),
                        ),
                    ));
                }
                self.menu(&format!("Delete {}", b.name), items);
            }
            1 => {
                let rb = self.data.remote_branches[i].name.clone();
                if let Some((remote, b)) = rb.split_once('/') {
                    self.confirm(
                        "Delete remote branch",
                        format!("Delete {rb} on the remote?"),
                        git_bg(&["push", remote, "--delete", b]),
                    );
                }
            }
            2 => self.dispatch(Action::DeleteTag),
            _ => self.dispatch(Action::RemoveRemote),
        }
    }

    fn stage_toggle(&mut self) {
        let Some(row) = self.sel_file_row() else {
            return;
        };
        let op = match row {
            FileRow::Dir { path, .. } => {
                let prefix = format!("{path}/");
                let any_unstaged = self
                    .data
                    .files
                    .iter()
                    .any(|f| f.path.starts_with(&prefix) && f.has_unstaged());
                if any_unstaged {
                    git_op(&["add", "-A", "--", &path])
                } else {
                    self.unstage_op(&path)
                }
            }
            FileRow::File { idx, .. } => {
                let f = &self.data.files[idx];
                if f.has_unstaged() {
                    git_op(&["add", "-A", "--", &f.path])
                } else {
                    let mut op = self.unstage_op(&f.path);
                    if let (Some(o), Op::Git { cmds, .. }) = (&f.orig, &mut op) {
                        cmds[0].push(o.clone());
                    }
                    op
                }
            }
        };
        self.run_op(op);
    }

    fn unstage_op(&self, path: &str) -> Op {
        if self.data.commits.is_empty() {
            git_op(&["rm", "--cached", "-r", "-q", "--", path])
        } else {
            git_op(&["reset", "-q", "HEAD", "--", path])
        }
    }

    fn discard_menu(&mut self) {
        let Some(row) = self.sel_file_row() else {
            return;
        };
        let path = self.file_row_path(&row);
        let file = match row {
            FileRow::File { idx, .. } => Some(self.data.files[idx].clone()),
            _ => None,
        };
        let mut items = Vec::new();
        match &file {
            Some(f) if f.untracked() => {
                items.push(MenuItem::new(
                    'd',
                    "delete untracked file",
                    MenuCmd::Confirm(
                        format!("Delete {path}?"),
                        git_op(&["clean", "-f", "--", &path]),
                    ),
                ));
            }
            Some(f) if f.x == 'A' => {
                items.push(MenuItem::new(
                    'd',
                    "discard new file (delete)",
                    MenuCmd::Confirm(
                        format!("Delete {path}?"),
                        git_op(&["rm", "-f", "--", &path]),
                    ),
                ));
            }
            _ => {
                items.push(MenuItem::new(
                    'd',
                    "discard all changes",
                    MenuCmd::Confirm(
                        format!("Discard all changes to {path}?"),
                        Op::Git {
                            cmds: vec![sv(&["checkout", "HEAD", "--", &path])],
                            bg: false,
                            then: Then::Refresh,
                        },
                    ),
                ));
                items.push(MenuItem::new(
                    'u',
                    "discard unstaged changes",
                    MenuCmd::Confirm(
                        format!("Discard unstaged changes to {path}?"),
                        git_op(&["checkout", "--", &path]),
                    ),
                ));
                if file.is_none() {
                    items.push(MenuItem::new(
                        'c',
                        "remove untracked files in dir",
                        MenuCmd::Confirm(
                            format!("Remove untracked files in {path}?"),
                            git_op(&["clean", "-fd", "--", &path]),
                        ),
                    ));
                }
            }
        }
        self.menu(&format!("Discard {path}"), items);
    }

    fn open_commit(&mut self, amend: bool) {
        self.popups.push(Popup::Commit(CommitPopup {
            input: TextInput::new("", true),
            amend,
            no_verify: false,
            signoff: false,
            reword: None,
            only: false,
        }));
    }

    pub fn submit_commit(&mut self, c: CommitPopup) {
        let msg = c.input.text();
        if let Some(oid) = c.reword {
            if msg.trim().is_empty() {
                return self.info("empty message");
            }
            if let Some((mut entries, base)) = self.rebase_range(oid) {
                if let Some(e) = entries.iter_mut().find(|e| e.oid == oid) {
                    e.action = TodoAction::Reword;
                    e.message = Some(msg);
                }
                self.run_op(Op::Rebase {
                    base,
                    todo: entries,
                });
            }
            return;
        }
        if msg.trim().is_empty() && !c.amend {
            return self.info("empty commit message");
        }
        let staged = self.data.files.iter().any(|f| f.has_staged());
        let op = Op::Commit {
            msg,
            amend: c.amend,
            no_verify: c.no_verify,
            signoff: c.signoff,
            only: c.only,
            stage_all: false,
        };
        if !staged && !c.amend && !self.data.files.is_empty() {
            if let Op::Commit {
                msg,
                no_verify,
                signoff,
                ..
            } = op
            {
                self.confirm(
                    "Nothing staged",
                    "No staged changes. Stage all and commit?",
                    Op::Commit {
                        msg,
                        amend: false,
                        no_verify,
                        signoff,
                        only: false,
                        stage_all: true,
                    },
                );
            }
        } else {
            self.run_op(op);
        }
    }

    /// Todo entries (newest first) from HEAD down to `oid` inclusive, and the rebase base.
    pub fn rebase_range(&mut self, oid: git2::Oid) -> Option<(Vec<TodoEntry>, Option<String>)> {
        let Some(idx) = self.commit_index(oid) else {
            self.info("commit not in current branch history");
            return None;
        };
        let cs = &self.data.commits[..=idx];
        if cs.iter().any(|c| c.parents.len() > 1) {
            self.error(
                "Rebase",
                "Range contains merge commits; interactive rebase on merges is not supported.",
            );
            return None;
        }
        let entries = cs
            .iter()
            .map(|c| TodoEntry {
                action: TodoAction::Pick,
                oid: c.oid,
                summary: c.summary.clone(),
                message: None,
            })
            .collect();
        let base = self.data.commits[idx]
            .parents
            .first()
            .map(|p| p.to_string());
        Some((entries, base))
    }

    fn start_rebase(&mut self, _: Option<()>) {
        let Some((oid, _)) = self.sel_commit() else {
            return;
        };
        if !self.data.state.is_empty() {
            return self.info(format!("repository is {}", self.data.state));
        }
        if let Some((entries, base)) = self.rebase_range(oid) {
            self.popups.push(Popup::Rebase {
                entries,
                sel: 0,
                base,
            });
        }
    }

    fn file_history(&mut self) {
        let Some(row) = self.sel_file_row() else {
            return;
        };
        let path = self.file_row_path(&row);
        match crate::git::read::commits(&self.repo, None, 1000, Some(std::path::Path::new(&path))) {
            Ok(commits) => {
                let v = MainView {
                    title: format!("History {path}"),
                    kind: MainKind::History { path, commits },
                    ..Default::default()
                };
                self.set_main(v);
                self.focus = PanelId::Main;
            }
            Err(e) => self.error("File history", e.to_string()),
        }
    }

    fn blame(&mut self) {
        let Some(f) = self.sel_file().map(|f| f.path.clone()) else {
            return self.info("select a file");
        };
        match crate::git::read::blame(&self.repo, &f) {
            Ok(lines) => {
                let v = MainView {
                    title: format!("Blame {f}"),
                    kind: MainKind::Blame { lines },
                    ..Default::default()
                };
                self.set_main(v);
                self.focus = PanelId::Main;
            }
            Err(e) => self.error("Blame", e.to_string()),
        }
    }

    fn edit_file(&mut self) {
        let path = match &self.main.kind {
            MainKind::Diff { path, .. } | MainKind::Conflict { path, .. }
                if self.focus == PanelId::Main =>
            {
                Some(path.clone())
            }
            _ => self.sel_file_row().map(|r| self.file_row_path(&r)),
        };
        if let Some(p) = path {
            self.needs_editor = Some(self.data.workdir.join(p));
        }
    }

    pub fn reload_main_file(&mut self) {
        let path = match &self.main.kind {
            MainKind::Diff { path, .. } | MainKind::Conflict { path, .. } => path.clone(),
            _ => match self.sel_file() {
                Some(f) => f.path.clone(),
                None => return,
            },
        };
        if let Some(idx) = self.data.files.iter().position(|f| f.path == path) {
            match self.file_view(idx) {
                Ok(v) => self.set_main(v),
                Err(e) => self.error("diff", e.to_string()),
            }
        }
    }

    /// Stage/unstage (or discard) the selected lines or hunk of the diff in the main view.
    fn stage_selection(&mut self, hunk: bool, discard: bool) {
        let MainKind::Diff {
            path,
            staged,
            untracked,
            fd,
            ..
        } = &self.main.kind
        else {
            return;
        };
        let (path, staged, untracked) = (path.clone(), *staged, *untracked);
        let Some(sel) = self.diff_selection(hunk) else {
            return;
        };
        if sel.is_empty() {
            return;
        }
        let all = (0..fd.hunks.len())
            .flat_map(|h| fd.hunk_lines(h))
            .collect::<std::collections::BTreeSet<_>>();
        if discard {
            if staged {
                return self.info("switch to the unstaged view (s) to discard");
            }
            let op = if untracked && sel == all {
                git_op(&["clean", "-f", "--", &path])
            } else {
                let Some(patch) = fd.build_patch(&sel, true) else {
                    return;
                };
                Op::Patch {
                    patch,
                    args: sv(&["-R"]),
                }
            };
            self.confirm(
                "Discard",
                format!("Discard {} selected line(s) in {path}?", sel.len()),
                op,
            );
        } else if staged {
            let Some(patch) = fd.build_patch(&sel, true) else {
                return;
            };
            self.run_op(Op::Patch {
                patch,
                args: sv(&["--cached", "-R"]),
            });
        } else if untracked && sel == all {
            self.run_op(git_op(&["add", "--", &path]));
        } else {
            let Some(patch) = fd.build_patch(&sel, false) else {
                return;
            };
            self.run_op(Op::Patch {
                patch,
                args: sv(&["--cached"]),
            });
        }
        self.main.anchor = None;
        // stay on this file even if it moved to the other side completely
        self.focus = PanelId::Main;
        if self.data.files.iter().any(|f| f.path == path) {
            self.reload_main_file_path(&path);
        }
    }

    fn reload_main_file_path(&mut self, path: &str) {
        if let Some(idx) = self.data.files.iter().position(|f| f.path == path)
            && let Ok(v) = self.file_view(idx)
        {
            self.set_main(v);
        }
    }

    pub fn pick_conflict(&mut self, pick: Pick, at: Option<usize>) {
        let MainKind::Conflict {
            path,
            lines,
            conflicts,
        } = &self.main.kind
        else {
            return;
        };
        let cur = at.unwrap_or(self.main.cursor);
        let Some(c) = conflicts.iter().find(|c| cur <= c.end).or(conflicts.last()) else {
            return self.info("no conflicts left — press space to mark resolved");
        };
        let new = diff::resolve(lines, c, pick);
        let path = path.clone();
        let full = self.data.workdir.join(&path);
        let mut content = new.join("\n");
        content.push('\n');
        if let Err(e) = std::fs::write(&full, content) {
            return self.error("Write", e.to_string());
        }
        let conflicts = diff::find_conflicts(&new);
        let start = c.start;
        if conflicts.is_empty() {
            self.run_op(git_op(&["add", "--", &path]));
            self.info(format!("{path}: all conflicts resolved and staged"));
            self.focus_panel(PanelId::Files);
        } else {
            self.main.kind = MainKind::Conflict {
                path,
                lines: new,
                conflicts,
            };
            self.main.set_cursor(start);
        }
    }

    fn repo_state_menu(&mut self) {
        let st = self.data.state.clone();
        let st = st.as_str();
        let cmd = match st {
            "REBASING" => "rebase",
            "MERGING" => "merge",
            "CHERRY-PICKING" => "cherry-pick",
            "REVERTING" => "revert",
            "APPLYING" => "am",
            "BISECTING" => {
                return self.menu(
                    "Bisect",
                    vec![
                        MenuItem::new('g', "good", MenuCmd::Op(git_op(&["bisect", "good"]))),
                        MenuItem::new('b', "bad", MenuCmd::Op(git_op(&["bisect", "bad"]))),
                        MenuItem::new('s', "skip", MenuCmd::Op(git_op(&["bisect", "skip"]))),
                        MenuItem::new(
                            'r',
                            "reset (stop bisecting)",
                            MenuCmd::Op(git_op(&["bisect", "reset"])),
                        ),
                    ],
                );
            }
            _ => {
                let mut items = vec![];
                if let Some((o, _)) = self.sel_commit() {
                    let o = o.to_string();
                    items.push(MenuItem::new(
                        'b',
                        "start bisect: selected commit is bad",
                        MenuCmd::Op(Op::Git {
                            cmds: vec![sv(&["bisect", "start"]), sv(&["bisect", "bad", &o])],
                            bg: false,
                            then: Then::Refresh,
                        }),
                    ));
                }
                if items.is_empty() {
                    return self.info("no merge/rebase in progress");
                }
                return self.menu("Repository", items);
            }
        };
        let mut items = vec![
            MenuItem::new(
                'c',
                "continue",
                MenuCmd::Op(Op::Git {
                    cmds: vec![sv(&[cmd, "--continue"])],
                    bg: true,
                    then: Then::Refresh,
                }),
            ),
            MenuItem::new(
                'a',
                "abort",
                MenuCmd::Confirm(format!("Abort {cmd}?"), git_op(&[cmd, "--abort"])),
            ),
        ];
        if cmd != "merge" {
            items.push(MenuItem::new(
                's',
                "skip",
                MenuCmd::Op(git_bg(&[cmd, "--skip"])),
            ));
        }
        self.menu(st, items);
    }

    fn copy(&mut self) {
        let text = match self.focus {
            PanelId::Commits => self.sel_commit().map(|(o, _)| o.to_string()),
            PanelId::Files => self.sel_file_row().map(|r| self.file_row_path(&r)),
            PanelId::Branches => self.selected_ref().or_else(|| self.sel_remote()),
            PanelId::Main => match &self.main.kind {
                MainKind::Diff { rows, .. } => {
                    let (a, b) = self.main.selection();
                    Some(
                        rows[a..=b.min(rows.len().saturating_sub(1))]
                            .iter()
                            .map(|r| r.text.clone())
                            .collect::<Vec<_>>()
                            .join("\n"),
                    )
                }
                MainKind::Blame { lines, .. } => {
                    lines.get(self.main.cursor).map(|l| l.0.to_string())
                }
                MainKind::History { commits, .. } => {
                    commits.get(self.main.cursor).map(|c| c.oid.to_string())
                }
                _ => Some(self.main.title.clone()),
            },
            _ => None,
        };
        if let Some(t) = text {
            crate::clipboard::copy(&t);
            self.info(format!("copied: {}", t.lines().next().unwrap_or("")));
        }
    }

    /// Actions of the focused context as a clickable menu.
    pub fn context_menu(&mut self) {
        let ctx = self.ctx_of(self.focus);
        let items: Vec<MenuItem> = self
            .keymap
            .of(ctx)
            .into_iter()
            .map(|(k, a)| MenuItem {
                label: format!("{:<8} {}", k.display(), a.label()),
                key: None,
                cmd: MenuCmd::Act(a),
            })
            .collect();
        if items.is_empty() {
            return self.dispatch(Action::Help);
        }
        self.menu(&format!("Actions — {}", ctx.name()), items);
    }

    pub fn run_menu(&mut self, cmd: MenuCmd) {
        match cmd {
            MenuCmd::Act(a) => self.dispatch(a),
            MenuCmd::Op(op) => self.run_op(op),
            MenuCmd::Confirm(msg, op) => self.confirm("Confirm", msg, op),
            MenuCmd::Input(kind, title, init) => self.input(&title, &init, kind),
        }
    }

    pub fn submit_input(&mut self, kind: InputKind, v: String) {
        let t = v.trim().to_string();
        use InputKind::*;
        match kind {
            Filter(_) => {}
            Askpass(id) => {
                if let Some(b) = &self.askpass {
                    b.answer(&id, &v);
                }
            }
            StashMessage(flags) => {
                let mut a = sv(&["stash", "push"]);
                a.extend(flags);
                if !t.is_empty() {
                    a.extend(["-m".to_string(), t]);
                }
                self.run_op(Op::Git {
                    cmds: vec![a],
                    bg: false,
                    then: Then::Refresh,
                });
            }
            _ if t.is_empty() => {}
            NewBranch { from } => {
                let mut a = sv(&["checkout", "-b", &t]);
                a.extend(from);
                self.run_op(Op::Git {
                    cmds: vec![a],
                    bg: false,
                    then: Then::Refresh,
                });
            }
            BranchAt(oid) => self.run_op(git_op(&["checkout", "-b", &t, &oid])),
            RenameBranch(old) => self.run_op(git_op(&["branch", "-m", &old, &t])),
            NewTag { at } => self.input(
                &format!("Message for tag {t} (empty = lightweight)"),
                "",
                TagMessage { name: t, at },
            ),
            TagMessage { .. } => unreachable!(),
            AddRemoteName => self.input(&format!("URL for remote {t}"), "", AddRemoteUrl(t)),
            AddRemoteUrl(name) => self.run_op(Op::Git {
                cmds: vec![sv(&["remote", "add", &name, &t]), sv(&["fetch", &name])],
                bg: true,
                then: Then::Refresh,
            }),
            EditRemoteUrl(name) => self.run_op(git_op(&["remote", "set-url", &name, &t])),
            SetUpstream(b) => {
                self.run_op(git_op(&["branch", &format!("--set-upstream-to={t}"), &b]))
            }
            CustomCommand => {
                let args = crate::git::cmd::split_args(t.strip_prefix("git ").unwrap_or(&t));
                self.run_op(Op::Git {
                    cmds: vec![args],
                    bg: true,
                    then: Then::ShowOutput,
                });
            }
            CheckoutByName => self.run_op(git_op(&["checkout", &t])),
            StashBranch(i) => {
                self.run_op(git_op(&["stash", "branch", &t, &format!("stash@{{{i}}}")]))
            }
            AddWorktreePath => self.input(
                "Branch for worktree (existing or new)",
                "",
                AddWorktreeBranch(t),
            ),
            AddWorktreeBranch(path) => {
                let exists = self.data.branches.iter().any(|b| b.name == t);
                if exists {
                    self.run_op(git_op(&["worktree", "add", &path, &t]));
                } else {
                    self.run_op(git_op(&["worktree", "add", "-b", &t, &path]));
                }
            }
            SubmoduleUrl => {
                let name = t
                    .trim_end_matches('/')
                    .trim_end_matches(".git")
                    .rsplit('/')
                    .next()
                    .unwrap_or("")
                    .to_string();
                self.input("Submodule path", &name, SubmodulePath(t));
            }
            SubmodulePath(url) => self.run_op(git_bg(&["submodule", "add", &url, &t])),
            OpenRepo => {
                let p = shellexpand(&t);
                self.run_op(Op::OpenRepo(p.into()));
            }
            TodoReword(i) => {
                if let Some(Popup::Rebase { entries, .. }) = self.popups.last_mut()
                    && let Some(e) = entries.get_mut(i)
                {
                    e.action = TodoAction::Reword;
                    e.message = Some(v);
                }
            }
        }
    }

    pub fn submit_tag_message(&mut self, name: String, at: Option<String>, msg: String) {
        let mut a = sv(&["tag"]);
        if !msg.trim().is_empty() {
            a.extend(sv(&["-a", "-m", msg.trim()]));
        }
        a.push(name);
        a.extend(at);
        self.run_op(Op::Git {
            cmds: vec![a],
            bg: false,
            then: Then::Refresh,
        });
    }
}

fn shellexpand(s: &str) -> String {
    match s.strip_prefix("~") {
        Some(rest) => format!("{}{rest}", std::env::var("HOME").unwrap_or_default()),
        None => s.to_string(),
    }
}
