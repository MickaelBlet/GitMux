//! Execution of git operations (sync or in background threads).

use crate::app::{App, Event, Op, Popup, Then};
use crate::git::Git;
use crate::git::rebase;
use anyhow::{Result, anyhow};

pub fn sv(args: &[&str]) -> Vec<String> {
    args.iter().map(|s| s.to_string()).collect()
}

pub fn git_op(args: &[&str]) -> Op {
    Op::Git {
        cmds: vec![sv(args)],
        bg: false,
        then: Then::Refresh,
    }
}

pub fn git_bg(args: &[&str]) -> Op {
    Op::Git {
        cmds: vec![sv(args)],
        bg: true,
        then: Then::Refresh,
    }
}

fn run_cmds(g: &Git, cmds: &[Vec<String>]) -> Result<String> {
    let mut out = String::new();
    for c in cmds {
        let a: Vec<&str> = c.iter().map(|s| s.as_str()).collect();
        out += &g.run(&a)?;
    }
    Ok(out)
}

impl App {
    fn spawn<F>(&mut self, label: String, then: Then, f: F)
    where
        F: FnOnce(&Git) -> Result<String> + Send + 'static,
    {
        let g = self.git.clone();
        let tx = self.tx.clone();
        self.jobs += 1;
        self.info(format!("{label}…"));
        std::thread::spawn(move || {
            let result = f(&g).map_err(|e| e.to_string());
            let _ = tx.send(Event::JobDone {
                label,
                result,
                then,
            });
        });
    }

    pub fn job_done(&mut self, label: String, result: Result<String, String>, then: Then) {
        self.jobs = self.jobs.saturating_sub(1);
        self.refresh();
        match result {
            Ok(out) => {
                if then == Then::ShowOutput {
                    self.focus = crate::app::PanelId::Main;
                    self.show_text(
                        &label,
                        if out.trim().is_empty() {
                            "(no output)"
                        } else {
                            &out
                        },
                    );
                } else {
                    self.info(format!("{label}: done"));
                }
            }
            Err(e) => self.handle_failure(&label, e, then),
        }
    }

    fn handle_failure(&mut self, label: &str, e: String, then: Then) {
        match then {
            Then::PushRejected
                if e.contains("rejected")
                    || e.contains("non-fast-forward")
                    || e.contains("fetch first") =>
            {
                self.confirm(
                    "Push rejected",
                    format!("{e}\n\nForce push with --force-with-lease?"),
                    Op::Push { force: true },
                );
            }
            Then::DeleteBranch(b) if e.contains("not fully merged") => {
                self.confirm(
                    "Branch not merged",
                    format!("'{b}' is not fully merged.\nForce delete (-D)?"),
                    crate::ops::git_op(&["branch", "-D", &b]),
                );
            }
            _ => self.error(label, e),
        }
    }

    fn finish_sync(&mut self, label: &str, r: Result<String>, then: Then) {
        self.refresh();
        match r {
            Ok(out) => {
                if then == Then::ShowOutput {
                    self.focus = crate::app::PanelId::Main;
                    self.show_text(
                        label,
                        if out.trim().is_empty() {
                            "(no output)"
                        } else {
                            &out
                        },
                    );
                }
            }
            Err(e) => self.handle_failure(label, e.to_string(), then),
        }
    }

    pub fn run_op(&mut self, op: Op) {
        match op {
            Op::Git { cmds, bg, then } => {
                let label = cmds
                    .first()
                    .map(|c| format!("git {}", c.join(" ")))
                    .unwrap_or_default();
                if bg {
                    self.spawn(label, then, move |g| run_cmds(g, &cmds));
                } else {
                    let r = run_cmds(&self.git, &cmds);
                    self.finish_sync(&label, r, then);
                }
            }
            Op::Patch { patch, args } => {
                let mut a = vec!["apply", "--recount", "--whitespace=nowarn"];
                a.extend(args.iter().map(|s| s.as_str()));
                a.push("-");
                let r = self.git.run_input(&a, &patch);
                self.finish_sync("git apply", r, Then::Refresh);
            }
            Op::Commit {
                msg,
                amend,
                no_verify,
                signoff,
                only,
                stage_all,
            } => {
                let mut a = vec!["commit".to_string()];
                if amend {
                    a.push("--amend".into());
                    if only {
                        a.push("--only".into());
                    }
                }
                if no_verify {
                    a.push("--no-verify".into());
                }
                if signoff {
                    a.push("--signoff".into());
                }
                let empty = msg.trim().is_empty();
                if empty && amend {
                    a.push("--no-edit".into());
                } else {
                    a.extend(["--cleanup=strip".into(), "-F".into(), "-".into()]);
                }
                self.spawn("commit".into(), Then::Refresh, move |g| {
                    if stage_all {
                        g.run(&["add", "-A"])?;
                    }
                    let a: Vec<&str> = a.iter().map(|s| s.as_str()).collect();
                    g.run_input(&a, &msg)
                });
            }
            Op::Rebase { base, todo } => {
                let dir = self.repo.path().join("gitmux");
                let r: Result<String> = (|| {
                    std::fs::create_dir_all(&dir)?;
                    let content = rebase::render(&todo, &dir)?;
                    let f = dir.join("todo");
                    std::fs::write(&f, content)?;
                    Ok(f.to_string_lossy().into_owned())
                })();
                let todo_file = match r {
                    Ok(f) => f,
                    Err(e) => return self.error("Rebase", e.to_string()),
                };
                let editor = format!("cp {}", rebase::shell_quote(&todo_file));
                self.spawn("interactive rebase".into(), Then::Refresh, move |g| {
                    let mut a = vec!["rebase", "-i", "--autostash"];
                    match &base {
                        Some(b) => a.push(b),
                        None => a.push("--root"),
                    }
                    g.run_full(&a, None, &[("GIT_SEQUENCE_EDITOR", &editor)])
                });
            }
            Op::Push { force } => {
                let d = &self.data;
                let branch = d.head.clone();
                if d.detached {
                    return self.error("Push", "HEAD is detached");
                }
                let remote = if d.remotes.iter().any(|r| r.name == self.cfg.default_remote) {
                    self.cfg.default_remote.clone()
                } else if let Some(r) = d.remotes.first() {
                    r.name.clone()
                } else {
                    return self.error("Push", "no remote configured");
                };
                let has_upstream = d.upstream.is_some();
                let then = if force {
                    Then::Refresh
                } else {
                    Then::PushRejected
                };
                self.spawn(
                    if force {
                        "force push".into()
                    } else {
                        "push".into()
                    },
                    then,
                    move |g| {
                        let mut a = vec!["push"];
                        if force {
                            a.push("--force-with-lease");
                        }
                        if !has_upstream {
                            a.extend(["--set-upstream", &remote, &branch]);
                        }
                        g.run(&a)
                    },
                );
            }
            Op::OpenRepo(p) => {
                if let Err(e) = self.switch_repo(&p) {
                    self.error("Open repository", e.to_string());
                }
            }
            Op::Ignore(path) => {
                let f = self.data.workdir.join(".gitignore");
                let r: Result<String> = (|| {
                    let mut s = std::fs::read_to_string(&f).unwrap_or_default();
                    if !s.is_empty() && !s.ends_with('\n') {
                        s.push('\n');
                    }
                    s.push_str(&format!("/{path}\n"));
                    std::fs::write(&f, s)?;
                    Ok(String::new())
                })();
                self.finish_sync("ignore", r, Then::Refresh);
            }
            Op::Undo => {
                let Some(e) = self.data.reflog.first().cloned() else {
                    return;
                };
                let r = if let Some(rest) = e.message.strip_prefix("checkout: moving from ") {
                    match rest.split(" to ").next() {
                        Some(from) => self.git.run(&["checkout", from]),
                        None => Err(anyhow!("cannot parse reflog")),
                    }
                } else {
                    self.git.run(&["reset", "--keep", "HEAD@{1}"])
                };
                self.finish_sync("undo", r, Then::Refresh);
                self.info(format!("undid: {}", e.message));
            }
        }
    }

    /// Execute a confirmed popup action (from Confirm popups).
    pub fn accept_confirm(&mut self) {
        if let Some(Popup::Confirm { op, .. }) = self.popups.pop() {
            self.run_op(op);
        }
    }
}
