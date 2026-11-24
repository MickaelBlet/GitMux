//! Context-sensitive key bindings (configurable).

use crate::action::Action;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Ctx {
    Global,
    Status,
    Files,
    Branches,
    Remotes,
    RemoteBranches,
    Tags,
    Commits,
    Reflog,
    Stash,
    Submodules,
    Worktrees,
    Diff,
    Conflict,
    Main,
}

impl Ctx {
    pub fn name(self) -> &'static str {
        match self {
            Ctx::Global => "global",
            Ctx::Status => "status",
            Ctx::Files => "files",
            Ctx::Branches => "branches",
            Ctx::Remotes => "remotes",
            Ctx::RemoteBranches => "remote_branches",
            Ctx::Tags => "tags",
            Ctx::Commits => "commits",
            Ctx::Reflog => "reflog",
            Ctx::Stash => "stash",
            Ctx::Submodules => "submodules",
            Ctx::Worktrees => "worktrees",
            Ctx::Diff => "diff",
            Ctx::Conflict => "conflict",
            Ctx::Main => "main",
        }
    }
    pub const ALL: [Ctx; 15] = [
        Ctx::Global,
        Ctx::Status,
        Ctx::Files,
        Ctx::Branches,
        Ctx::Remotes,
        Ctx::RemoteBranches,
        Ctx::Tags,
        Ctx::Commits,
        Ctx::Reflog,
        Ctx::Stash,
        Ctx::Submodules,
        Ctx::Worktrees,
        Ctx::Diff,
        Ctx::Conflict,
        Ctx::Main,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Key {
    pub code: KeyCode,
    pub mods: KeyModifiers,
}

impl Key {
    pub fn from_event(e: &KeyEvent) -> Key {
        let mut mods =
            e.modifiers & (KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT);
        // Shift is implied by the character itself.
        if let KeyCode::Char(_) = e.code {
            mods.remove(KeyModifiers::SHIFT);
        }
        let code = match e.code {
            KeyCode::BackTab => KeyCode::Tab,
            c => c,
        };
        if e.code == KeyCode::BackTab {
            mods.insert(KeyModifiers::SHIFT);
        }
        Key { code, mods }
    }

    pub fn parse(s: &str) -> Option<Key> {
        let mut mods = KeyModifiers::NONE;
        let mut rest = s;
        loop {
            if let Some(r) = rest
                .strip_prefix("ctrl+")
                .or_else(|| rest.strip_prefix("c-"))
            {
                mods |= KeyModifiers::CONTROL;
                rest = r;
            } else if let Some(r) = rest
                .strip_prefix("alt+")
                .or_else(|| rest.strip_prefix("a-"))
            {
                mods |= KeyModifiers::ALT;
                rest = r;
            } else if let Some(r) = rest.strip_prefix("shift+") {
                mods |= KeyModifiers::SHIFT;
                rest = r;
            } else {
                break;
            }
        }
        let code = match rest {
            "enter" => KeyCode::Enter,
            "esc" => KeyCode::Esc,
            "tab" => KeyCode::Tab,
            "space" => KeyCode::Char(' '),
            "backspace" => KeyCode::Backspace,
            "delete" | "del" => KeyCode::Delete,
            "up" => KeyCode::Up,
            "down" => KeyCode::Down,
            "left" => KeyCode::Left,
            "right" => KeyCode::Right,
            "pgup" | "pageup" => KeyCode::PageUp,
            "pgdown" | "pagedown" => KeyCode::PageDown,
            "home" => KeyCode::Home,
            "end" => KeyCode::End,
            s if s.starts_with('f') && s.len() > 1 && s[1..].parse::<u8>().is_ok() => {
                KeyCode::F(s[1..].parse().ok()?)
            }
            s if s.chars().count() == 1 => KeyCode::Char(s.chars().next()?),
            _ => return None,
        };
        Some(Key { code, mods })
    }

    pub fn display(&self) -> String {
        let mut s = String::new();
        if self.mods.contains(KeyModifiers::CONTROL) {
            s.push_str("C-");
        }
        if self.mods.contains(KeyModifiers::ALT) {
            s.push_str("A-");
        }
        if self.mods.contains(KeyModifiers::SHIFT) {
            s.push_str("S-");
        }
        s.push_str(&match self.code {
            KeyCode::Char(' ') => "space".into(),
            KeyCode::Char(c) => c.to_string(),
            KeyCode::Enter => "enter".into(),
            KeyCode::Esc => "esc".into(),
            KeyCode::Tab => "tab".into(),
            KeyCode::Up => "↑".into(),
            KeyCode::Down => "↓".into(),
            KeyCode::Left => "←".into(),
            KeyCode::Right => "→".into(),
            KeyCode::PageUp => "pgup".into(),
            KeyCode::PageDown => "pgdn".into(),
            KeyCode::Home => "home".into(),
            KeyCode::End => "end".into(),
            KeyCode::Backspace => "bksp".into(),
            KeyCode::F(n) => format!("F{n}"),
            other => format!("{other:?}"),
        });
        s
    }
}

pub struct Keymap {
    /// ordered bindings per context (order = display order in help/menus)
    pub bindings: HashMap<Ctx, Vec<(Key, Action)>>,
}

const DEFAULTS: &[(Ctx, &str, Action)] = {
    use Action::*;
    use Ctx::*;
    &[
        (Global, "q", Quit),
        (Global, "?", Help),
        (Global, "tab", NextPanel),
        (Global, "shift+tab", PrevPanel),
        (Global, "1", FocusStatus),
        (Global, "2", FocusFiles),
        (Global, "3", FocusBranches),
        (Global, "4", FocusCommits),
        (Global, "5", FocusStash),
        (Global, "0", FocusMain),
        (Global, "]", NextTab),
        (Global, "[", PrevTab),
        (Global, "R", Refresh),
        (Global, "P", Push),
        (Global, "p", Pull),
        (Global, "f", Fetch),
        (Global, "z", Undo),
        (Global, "@", ToggleLog),
        (Global, ":", CustomCommand),
        (Global, "/", Filter),
        (Global, "x", ContextMenu),
        (Global, "m", RepoStateMenu),
        (Global, "y", CopySha),
        (Global, "k", Up),
        (Global, "up", Up),
        (Global, "j", Down),
        (Global, "down", Down),
        (Global, "pgup", PageUp),
        (Global, "ctrl+u", PageUp),
        (Global, "pgdown", PageDown),
        (Global, "ctrl+d", PageDown),
        (Global, "home", Top),
        (Global, "g", Top),
        (Global, "end", Bottom),
        (Global, "G", Bottom),
        (Global, "enter", Enter),
        (Status, "r", PullRebase),
        (Status, "F", PullFfOnly),
        (Status, "o", RecentRepo),
        (Status, "S", ForcePush),
        (Files, "space", StageToggle),
        (Files, "a", StageAll),
        (Files, "c", Commit),
        (Files, "A", Amend),
        (Files, "d", Discard),
        (Files, "s", StashAll),
        (Files, "S", StashMenu),
        (Files, "D", ResetMenu),
        (Files, "i", Ignore),
        (Files, "e", EditFile),
        (Files, "t", ToggleTree),
        (Files, "h", FileHistory),
        (Files, "b", Blame),
        (Files, "o", UseOurs),
        (Files, "T", UseTheirs),
        (Branches, "space", Checkout),
        (Branches, "c", CheckoutByName),
        (Branches, "n", NewBranch),
        (Branches, "d", DeleteBranch),
        (Branches, "r", RenameBranch),
        (Branches, "M", MergeBranch),
        (Branches, "e", RebaseOnto),
        (Branches, "u", SetUpstream),
        (Branches, "F", FastForward),
        (Branches, "T", TagCommit),
        (RemoteBranches, "space", Checkout),
        (RemoteBranches, "n", NewBranch),
        (RemoteBranches, "d", DeleteBranch),
        (RemoteBranches, "M", MergeBranch),
        (RemoteBranches, "e", RebaseOnto),
        (Tags, "space", Checkout),
        (Tags, "n", NewTag),
        (Tags, "d", DeleteTag),
        (Tags, "P", PushTag),
        (Remotes, "n", AddRemote),
        (Remotes, "d", RemoveRemote),
        (Remotes, "e", EditRemote),
        (Remotes, "F", FetchRemote),
        (Commits, "space", CheckoutCommit),
        (Commits, "C", CherryCopy),
        (Commits, "V", CherryPaste),
        (Commits, "ctrl+r", CherryClear),
        (Commits, "t", Revert),
        (Commits, "i", InteractiveRebase),
        (Commits, "r", Reword),
        (Commits, "F", Fixup),
        (Commits, "s", Squash),
        (Commits, "d", DropCommit),
        (Commits, "S", AutosquashRebase),
        (Commits, "T", TagCommit),
        (Commits, "n", BranchFromCommit),
        (Commits, "D", ResetToCommit),
        (Reflog, "space", CheckoutCommit),
        (Reflog, "D", ResetToCommit),
        (Reflog, "C", CherryCopy),
        (Reflog, "n", BranchFromCommit),
        (Stash, "space", StashApply),
        (Stash, "a", StashApply),
        (Stash, "o", StashPop),
        (Stash, "d", StashDrop),
        (Stash, "n", StashBranch),
        (Submodules, "space", EnterSubmodule),
        (Submodules, "u", SubmoduleUpdate),
        (Submodules, "i", SubmoduleInit),
        (Submodules, "n", SubmoduleAdd),
        (Global, "backspace", ParentRepo),
        (Worktrees, "space", SwitchWorktree),
        (Worktrees, "n", AddWorktree),
        (Worktrees, "d", RemoveWorktree),
        (Diff, "space", StageSelection),
        (Diff, "a", StageHunk),
        (Diff, "v", RangeSelect),
        (Diff, "d", DiscardSelection),
        (Diff, "s", ToggleStagedView),
        (Diff, "|", SideBySide),
        (Diff, "w", WordDiff),
        (Diff, "n", NextHunk),
        (Diff, "N", PrevHunk),
        (Diff, "e", EditFile),
        (Diff, "left", ScrollLeft),
        (Diff, "right", ScrollRight),
        (Main, "left", ScrollLeft),
        (Main, "right", ScrollRight),
        (Conflict, "o", PickOurs),
        (Conflict, "t", PickTheirs),
        (Conflict, "b", PickBoth),
        (Conflict, "n", NextConflict),
        (Conflict, "N", PrevConflict),
        (Conflict, "space", MarkResolved),
        (Conflict, "e", EditFile),
    ]
};

impl Keymap {
    pub fn new(overrides: &HashMap<String, HashMap<String, String>>) -> (Keymap, Vec<String>) {
        let mut bindings: HashMap<Ctx, Vec<(Key, Action)>> = HashMap::new();
        for (ctx, k, a) in DEFAULTS {
            bindings
                .entry(*ctx)
                .or_default()
                .push((Key::parse(k).expect("bad default key"), *a));
        }
        let mut errors = Vec::new();
        for (ctx_name, map) in overrides {
            let Some(ctx) = Ctx::ALL.iter().copied().find(|c| c.name() == ctx_name) else {
                errors.push(format!("unknown keybinding context '{ctx_name}'"));
                continue;
            };
            for (act, key) in map {
                let (Some(a), Some(k)) = (Action::from_name(act), Key::parse(key)) else {
                    errors.push(format!("bad keybinding {ctx_name}.{act} = '{key}'"));
                    continue;
                };
                let v = bindings.entry(ctx).or_default();
                v.retain(|(kk, aa)| *aa != a && *kk != k);
                v.push((k, a));
            }
        }
        (Keymap { bindings }, errors)
    }

    pub fn lookup(&self, ctxs: &[Ctx], key: Key) -> Option<Action> {
        ctxs.iter().find_map(|c| {
            self.bindings
                .get(c)?
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, a)| *a)
        })
    }

    /// Bindings of a context, de-duplicated by action.
    pub fn of(&self, ctx: Ctx) -> Vec<(Key, Action)> {
        let mut seen = Vec::new();
        let mut out = Vec::new();
        for (k, a) in self.bindings.get(&ctx).into_iter().flatten() {
            if !seen.contains(a) {
                seen.push(*a);
                out.push((*k, *a));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_keys() {
        assert_eq!(
            Key::parse("ctrl+r"),
            Some(Key {
                code: KeyCode::Char('r'),
                mods: KeyModifiers::CONTROL
            })
        );
        assert_eq!(Key::parse("space").unwrap().code, KeyCode::Char(' '));
        assert_eq!(Key::parse("f5").unwrap().code, KeyCode::F(5));
        assert!(Key::parse("nope").is_none());
    }
    #[test]
    fn overrides() {
        let mut o = HashMap::new();
        o.insert(
            "files".to_string(),
            HashMap::from([("Commit".to_string(), "C".to_string())]),
        );
        let (km, errs) = Keymap::new(&o);
        assert!(errs.is_empty());
        let k = Key::parse("C").unwrap();
        assert_eq!(
            km.lookup(&[Ctx::Files, Ctx::Global], k),
            Some(Action::Commit)
        );
        let c = Key::parse("c").unwrap();
        assert_eq!(km.lookup(&[Ctx::Files, Ctx::Global], c), None);
    }
}
