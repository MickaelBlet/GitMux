//! Read-only repository queries through libgit2.

use anyhow::Result;
use git2::{BranchType, Oid, Repository, RepositoryState, Sort, Status, StatusOptions};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct FileEntry {
    pub path: String,
    pub orig: Option<String>,
    /// index status char (' ' if none)
    pub x: char,
    /// worktree status char (' ' if none)
    pub y: char,
    pub conflicted: bool,
}

impl FileEntry {
    pub fn untracked(&self) -> bool {
        self.y == '?'
    }
    pub fn has_staged(&self) -> bool {
        self.x != ' ' && !self.untracked() && !self.conflicted
    }
    pub fn has_unstaged(&self) -> bool {
        self.y != ' ' || self.conflicted
    }
}

#[derive(Clone, Debug)]
pub struct BranchInfo {
    pub name: String,
    pub is_head: bool,
    pub upstream: Option<String>,
    pub ahead: usize,
    pub behind: usize,
    pub summary: String,
}

#[derive(Clone, Debug)]
pub struct TagInfo {
    pub name: String,
    pub summary: String,
}

#[derive(Clone, Debug)]
pub struct RemoteInfo {
    pub name: String,
    pub url: String,
}

#[derive(Clone, Debug)]
pub struct CommitInfo {
    pub oid: Oid,
    pub short: String,
    pub summary: String,
    pub author: String,
    pub email: String,
    /// Full commit message.
    pub message: String,
    pub time: i64,
    /// Commit timezone offset, in minutes.
    pub tz: i32,
    pub parents: Vec<Oid>,
    pub refs: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct ReflogInfo {
    pub oid: Oid,
    pub selector: String,
    pub message: String,
}

#[derive(Clone, Debug)]
pub struct StashInfo {
    pub index: usize,
    pub message: String,
}

#[derive(Clone, Debug)]
pub struct SubmoduleInfo {
    pub name: String,
    pub path: PathBuf,
    pub url: String,
    pub status: String,
}

#[derive(Clone, Debug)]
pub struct WorktreeInfo {
    pub path: PathBuf,
    pub branch: String,
    pub head: String,
    pub current: bool,
}

#[derive(Default, Debug)]
pub struct RepoData {
    pub workdir: PathBuf,
    pub head: String,
    pub detached: bool,
    pub upstream: Option<String>,
    pub ahead: usize,
    pub behind: usize,
    pub state: String,
    pub files: Vec<FileEntry>,
    pub branches: Vec<BranchInfo>,
    pub remote_branches: Vec<BranchInfo>,
    pub tags: Vec<TagInfo>,
    pub remotes: Vec<RemoteInfo>,
    pub commits: Vec<CommitInfo>,
    pub graph: Vec<String>,
    pub reflog: Vec<ReflogInfo>,
    pub stashes: Vec<StashInfo>,
    pub submodules: Vec<SubmoduleInfo>,
    pub worktrees: Vec<WorktreeInfo>,
}

pub fn state_name(s: RepositoryState) -> &'static str {
    use RepositoryState::*;
    match s {
        Clean => "",
        Merge => "MERGING",
        Revert | RevertSequence => "REVERTING",
        CherryPick | CherryPickSequence => "CHERRY-PICKING",
        Bisect => "BISECTING",
        Rebase | RebaseInteractive | RebaseMerge => "REBASING",
        ApplyMailbox | ApplyMailboxOrRebase => "APPLYING",
    }
}

fn status_chars(s: Status) -> (char, char) {
    let x = if s.contains(Status::INDEX_NEW) {
        'A'
    } else if s.contains(Status::INDEX_MODIFIED) {
        'M'
    } else if s.contains(Status::INDEX_DELETED) {
        'D'
    } else if s.contains(Status::INDEX_RENAMED) {
        'R'
    } else if s.contains(Status::INDEX_TYPECHANGE) {
        'T'
    } else {
        ' '
    };
    let y = if s.contains(Status::WT_NEW) {
        if x == ' ' { '?' } else { 'A' }
    } else if s.contains(Status::WT_MODIFIED) {
        'M'
    } else if s.contains(Status::WT_DELETED) {
        'D'
    } else if s.contains(Status::WT_RENAMED) {
        'R'
    } else if s.contains(Status::WT_TYPECHANGE) {
        'T'
    } else {
        ' '
    };
    if y == '?' { ('?', '?') } else { (x, y) }
}

pub fn files(repo: &Repository) -> Result<Vec<FileEntry>> {
    let mut opts = StatusOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .renames_head_to_index(true)
        .include_ignored(false);
    let st = repo.statuses(Some(&mut opts))?;
    let mut out = Vec::new();
    for e in st.iter() {
        let s = e.status();
        let Ok(path) = e.path() else { continue };
        let conflicted = s.contains(Status::CONFLICTED);
        let (x, y) = if conflicted {
            ('U', 'U')
        } else {
            status_chars(s)
        };
        if x == ' ' && y == ' ' {
            continue;
        }
        let orig = e
            .head_to_index()
            .and_then(|d| {
                d.old_file()
                    .path()
                    .map(|p| p.to_string_lossy().into_owned())
            })
            .filter(|o| o != path && x == 'R');
        out.push(FileEntry {
            path: path.to_string(),
            orig,
            x,
            y,
            conflicted,
        });
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

fn summary_of(repo: &Repository, oid: Option<Oid>) -> String {
    oid.and_then(|o| repo.find_commit(o).ok())
        .and_then(|c| c.summary().ok().flatten().map(String::from))
        .unwrap_or_default()
}

pub fn branches(repo: &Repository, kind: BranchType) -> Result<Vec<BranchInfo>> {
    let mut out = Vec::new();
    for b in repo.branches(Some(kind))? {
        let (b, _) = b?;
        let Some(name) = b.name()?.map(String::from) else {
            continue;
        };
        if kind == BranchType::Remote && name.ends_with("/HEAD") {
            continue;
        }
        let target = b.get().target();
        let (mut upstream, mut ahead, mut behind) = (None, 0, 0);
        if kind == BranchType::Local
            && let Ok(u) = b.upstream()
        {
            upstream = u.name().ok().flatten().map(String::from);
            if let (Some(l), Some(r)) = (target, u.get().target())
                && let Ok((a, bh)) = repo.graph_ahead_behind(l, r)
            {
                ahead = a;
                behind = bh;
            }
        }
        out.push(BranchInfo {
            name,
            is_head: b.is_head(),
            upstream,
            ahead,
            behind,
            summary: summary_of(repo, target),
        });
    }
    // current branch first, then by name
    out.sort_by(|a, b| b.is_head.cmp(&a.is_head).then(a.name.cmp(&b.name)));
    Ok(out)
}

pub fn tags(repo: &Repository) -> Result<Vec<TagInfo>> {
    let mut out = Vec::new();
    for n in repo.tag_names(None)?.iter().flatten().flatten() {
        let target = repo
            .revparse_single(&format!("refs/tags/{n}"))
            .ok()
            .and_then(|o| o.peel_to_commit().ok())
            .map(|c| c.id());
        out.push(TagInfo {
            name: n.to_string(),
            summary: summary_of(repo, target),
        });
    }
    out.reverse();
    Ok(out)
}

pub fn remotes(repo: &Repository) -> Result<Vec<RemoteInfo>> {
    let mut out = Vec::new();
    for n in repo.remotes()?.iter().flatten().flatten() {
        let url = repo
            .find_remote(n)
            .ok()
            .and_then(|r| r.url().ok().map(String::from))
            .unwrap_or_default();
        out.push(RemoteInfo {
            name: n.to_string(),
            url,
        });
    }
    Ok(out)
}

pub fn ref_map(repo: &Repository) -> HashMap<Oid, Vec<String>> {
    let mut m: HashMap<Oid, Vec<String>> = HashMap::new();
    if let Ok(refs) = repo.references() {
        for r in refs.flatten() {
            let Ok(name) = r.shorthand().map(String::from) else {
                continue;
            };
            if name.ends_with("/HEAD") || r.name().ok() == Some("refs/stash") {
                continue;
            }
            let label = if r.is_tag() {
                format!("tag:{name}")
            } else {
                name
            };
            if let Ok(c) = r.peel_to_commit() {
                m.entry(c.id()).or_default().push(label);
            }
        }
    }
    m
}

/// Walk commits from `start` (or HEAD) up to `limit`; optional path filter (file history).
pub fn commits(
    repo: &Repository,
    start: Option<Oid>,
    limit: usize,
    path: Option<&Path>,
) -> Result<Vec<CommitInfo>> {
    let mut walk = repo.revwalk()?;
    walk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;
    match start {
        Some(o) => walk.push(o)?,
        None => {
            if walk.push_head().is_err() {
                return Ok(vec![]);
            }
        }
    }
    let refs = ref_map(repo);
    let mut out = Vec::new();
    for id in walk {
        let id = id?;
        let c = repo.find_commit(id)?;
        if let Some(p) = path
            && !touches(repo, &c, p)
        {
            continue;
        }
        out.push(CommitInfo {
            oid: id,
            short: id.to_string()[..8].to_string(),
            summary: c.summary().ok().flatten().unwrap_or("").to_string(),
            author: c.author().name().unwrap_or("").to_string(),
            email: c.author().email().unwrap_or("").to_string(),
            message: c.message().unwrap_or("").trim_end().to_string(),
            time: c.time().seconds(),
            tz: c.time().offset_minutes(),
            parents: c.parent_ids().collect(),
            refs: refs.get(&id).cloned().unwrap_or_default(),
        });
        if out.len() >= limit {
            break;
        }
    }
    Ok(out)
}

fn touches(repo: &Repository, c: &git2::Commit, path: &Path) -> bool {
    let entry = |t: Option<git2::Tree>| t.and_then(|t| t.get_path(path).ok().map(|e| e.id()));
    let mine = entry(c.tree().ok());
    if c.parent_count() == 0 {
        return mine.is_some();
    }
    c.parents().any(|p| entry(p.tree().ok()) != mine) && {
        let _ = repo;
        true
    }
}

pub fn reflog(repo: &Repository, limit: usize) -> Result<Vec<ReflogInfo>> {
    let log = repo.reflog("HEAD")?;
    Ok(log
        .iter()
        .take(limit)
        .enumerate()
        .map(|(i, e)| ReflogInfo {
            oid: e.id_new(),
            selector: format!("HEAD@{{{i}}}"),
            message: e.message().ok().flatten().unwrap_or("").to_string(),
        })
        .collect())
}

pub fn stashes(repo: &mut Repository) -> Result<Vec<StashInfo>> {
    let mut out = Vec::new();
    repo.stash_foreach(|i, msg, _| {
        out.push(StashInfo {
            index: i,
            message: msg.to_string(),
        });
        true
    })?;
    Ok(out)
}

pub fn submodules(repo: &Repository) -> Vec<SubmoduleInfo> {
    let Ok(subs) = repo.submodules() else {
        return vec![];
    };
    subs.iter()
        .map(|s| {
            let name = s.name().unwrap_or("").to_string();
            let status = match repo.submodule_status(&name, git2::SubmoduleIgnore::None) {
                Ok(st) if st.contains(git2::SubmoduleStatus::WD_UNINITIALIZED) => "uninitialized",
                Ok(st)
                    if st.intersects(
                        git2::SubmoduleStatus::WD_MODIFIED
                            | git2::SubmoduleStatus::WD_WD_MODIFIED
                            | git2::SubmoduleStatus::WD_INDEX_MODIFIED
                            | git2::SubmoduleStatus::WD_UNTRACKED,
                    ) =>
                {
                    "modified"
                }
                Ok(_) => "ok",
                Err(_) => "?",
            };
            SubmoduleInfo {
                name,
                path: s.path().to_path_buf(),
                url: s.url().ok().flatten().unwrap_or("").to_string(),
                status: status.into(),
            }
        })
        .collect()
}

/// Parse `git worktree list --porcelain`.
pub fn parse_worktrees(text: &str, current: &Path) -> Vec<WorktreeInfo> {
    let mut out = Vec::new();
    for block in text.split("\n\n") {
        let mut path = None;
        let mut branch = String::from("(detached)");
        let mut head = String::new();
        for l in block.lines() {
            if let Some(p) = l.strip_prefix("worktree ") {
                path = Some(PathBuf::from(p));
            } else if let Some(b) = l.strip_prefix("branch ") {
                branch = b.trim_start_matches("refs/heads/").to_string();
            } else if let Some(h) = l.strip_prefix("HEAD ") {
                head = h.chars().take(8).collect();
            } else if l == "bare" {
                branch = "(bare)".into();
            }
        }
        if let Some(p) = path {
            let cur = canon(&p) == canon(current);
            out.push(WorktreeInfo {
                path: p,
                branch,
                head,
                current: cur,
            });
        }
    }
    out
}

fn canon(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

pub fn load(repo: &mut Repository, commit_limit: usize, git: &super::Git) -> Result<RepoData> {
    let mut d = RepoData {
        workdir: repo.workdir().unwrap_or(repo.path()).to_path_buf(),
        ..Default::default()
    };
    d.state = state_name(repo.state()).to_string();
    match repo.head() {
        Ok(h) => {
            d.detached = repo.head_detached().unwrap_or(false);
            d.head = if d.detached {
                h.target()
                    .map(|o| o.to_string()[..8].to_string())
                    .unwrap_or_default()
            } else {
                h.shorthand().unwrap_or("HEAD").to_string()
            };
        }
        Err(_) => {
            // unborn branch
            d.head = repo
                .find_reference("HEAD")
                .ok()
                .and_then(|r| {
                    r.symbolic_target()
                        .ok()
                        .flatten()
                        .map(|s| s.trim_start_matches("refs/heads/").to_string())
                })
                .unwrap_or_else(|| "HEAD".into());
        }
    }
    d.files = files(repo)?;
    d.branches = branches(repo, BranchType::Local)?;
    if let Some(b) = d.branches.iter().find(|b| b.is_head) {
        d.upstream = b.upstream.clone();
        d.ahead = b.ahead;
        d.behind = b.behind;
    }
    d.remote_branches = branches(repo, BranchType::Remote)?;
    d.tags = tags(repo)?;
    d.remotes = remotes(repo)?;
    d.commits = commits(repo, None, commit_limit, None)?;
    let g: Vec<(Oid, Vec<Oid>)> = d
        .commits
        .iter()
        .map(|c| (c.oid, c.parents.clone()))
        .collect();
    d.graph = super::graph::compute(&g);
    d.reflog = reflog(repo, 1000).unwrap_or_default();
    d.stashes = stashes(repo).unwrap_or_default();
    d.submodules = submodules(repo);
    if let Ok(t) = git.read(&["worktree", "list", "--porcelain"]) {
        d.worktrees = parse_worktrees(&t, &d.workdir);
    }
    Ok(d)
}

/// Blame lines: (short sha, author, line text).
pub fn blame(repo: &Repository, path: &str) -> Result<Vec<(Oid, String, String, String)>> {
    let head = repo.head()?.peel_to_commit()?;
    let entry = head.tree()?.get_path(Path::new(path))?;
    let blob = repo.find_blob(entry.id())?;
    let text = String::from_utf8_lossy(blob.content()).into_owned();
    let b = repo.blame_file(Path::new(path), None)?;
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let (oid, author) = match b.get_line(i + 1) {
            Some(h) => (
                h.final_commit_id(),
                h.final_signature()
                    .and_then(|s| s.name().ok().map(String::from))
                    .unwrap_or_default(),
            ),
            None => (Oid::ZERO_SHA1, String::new()),
        };
        out.push((
            oid,
            oid.to_string()[..8].to_string(),
            author,
            line.to_string(),
        ));
    }
    Ok(out)
}

/// Absolute date `YYYY-MM-DD HH:MM +HHMM` in the given timezone offset (minutes).
pub fn format_time(t: i64, tz: i32) -> String {
    let local = t + tz as i64 * 60;
    let (days, secs) = (local.div_euclid(86400), local.rem_euclid(86400));
    // civil-from-days (Howard Hinnant)
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    let sign = if tz < 0 { '-' } else { '+' };
    let a = tz.abs();
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02} {sign}{:02}{:02}",
        secs / 3600,
        secs % 3600 / 60,
        a / 60,
        a % 60
    )
}

pub fn relative_time(t: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(t);
    let d = (now - t).max(0);
    match d {
        0..60 => format!("{d}s"),
        60..3600 => format!("{}m", d / 60),
        3600..86400 => format!("{}h", d / 3600),
        86400..2592000 => format!("{}d", d / 86400),
        2592000..31536000 => format!("{}mo", d / 2592000),
        _ => format!("{}y", d / 31536000),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_time_civil() {
        assert_eq!(format_time(0, 0), "1970-01-01 00:00 +0000");
        assert_eq!(format_time(951782400, 60), "2000-02-29 01:00 +0100");
        assert_eq!(format_time(1700000000, -300), "2023-11-14 17:13 -0500");
    }
    use crate::git::{Git, diff};
    use std::collections::BTreeSet;

    fn repo() -> (tempfile::TempDir, Git) {
        let d = tempfile::tempdir().unwrap();
        let g = Git::new(d.path(), vec![], Default::default());
        g.run(&["init", "-q", "-b", "main"]).unwrap();
        g.run(&["config", "user.email", "t@t"]).unwrap();
        g.run(&["config", "user.name", "T"]).unwrap();
        std::fs::write(d.path().join("f"), "a\nb\nc\n").unwrap();
        g.run(&["add", "."]).unwrap();
        g.run(&["commit", "-qm", "init"]).unwrap();
        (d, g)
    }

    #[test]
    fn load_repo_data() {
        let (d, g) = repo();
        std::fs::write(d.path().join("new"), "x").unwrap();
        g.run(&["branch", "dev"]).unwrap();
        g.run(&["tag", "v1"]).unwrap();
        let mut r = Repository::open(d.path()).unwrap();
        let data = load(&mut r, 100, &g).unwrap();
        assert_eq!(data.head, "main");
        assert_eq!(data.files.len(), 1);
        assert!(data.files[0].untracked());
        assert_eq!(data.branches.len(), 2);
        assert!(data.branches[0].is_head);
        assert_eq!(data.tags[0].name, "v1");
        assert_eq!(data.commits.len(), 1);
        assert_eq!(data.worktrees.len(), 1);
        assert!(data.commits[0].refs.iter().any(|r| r == "tag:v1"));
    }

    #[test]
    fn stage_single_line() {
        let (d, g) = repo();
        std::fs::write(d.path().join("f"), "A\nb\nC\n").unwrap();
        let fd = diff::parse(&g.read(&["diff", "--no-color", "--", "f"]).unwrap());
        let rows = fd.rows();
        // stage only "+A" (and the matching "-a")
        let sel: BTreeSet<_> = rows
            .iter()
            .filter(|r| r.text == "+A" || r.text == "-a")
            .map(|r| (r.hunk.unwrap(), r.line.unwrap()))
            .collect();
        let patch = fd.build_patch(&sel, false).unwrap();
        g.run_input(&["apply", "--cached", "--recount", "-"], &patch)
            .unwrap();
        let staged = g.read(&["diff", "--cached", "--no-color"]).unwrap();
        assert!(staged.contains("+A") && !staged.contains("+C"), "{staged}");
        // unstage it again (reverse)
        let fd = diff::parse(
            &g.read(&["diff", "--cached", "--no-color", "--", "f"])
                .unwrap(),
        );
        let all = fd.hunk_lines(0);
        let patch = fd.build_patch(&all, true).unwrap();
        g.run_input(&["apply", "--cached", "-R", "--recount", "-"], &patch)
            .unwrap();
        assert!(g.read(&["diff", "--cached"]).unwrap().is_empty());
    }
}
