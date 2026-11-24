//! User intents. Keys, mouse clicks, hints and menus all resolve to an `Action`.

use serde::Deserialize;

macro_rules! actions {
    ($($name:ident => $label:expr),* $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
        pub enum Action { $($name),* }
        impl Action {
            pub fn label(self) -> &'static str {
                match self { $(Action::$name => $label),* }
            }
            pub fn from_name(s: &str) -> Option<Action> {
                match s { $(stringify!($name) => Some(Action::$name),)* _ => None }
            }
        }
    };
}

actions! {
    // global
    Quit => "quit",
    Help => "help",
    NextPanel => "next panel",
    PrevPanel => "prev panel",
    FocusStatus => "status panel",
    FocusFiles => "files panel",
    FocusBranches => "branches panel",
    FocusCommits => "commits panel",
    FocusStash => "stash panel",
    FocusMain => "main panel",
    NextTab => "next tab",
    PrevTab => "prev tab",
    Refresh => "refresh",
    Push => "push",
    ForcePush => "force push (with lease)",
    Pull => "pull",
    Fetch => "fetch all",
    Undo => "undo (reflog)",
    ToggleLog => "toggle command log",
    CustomCommand => "run git command",
    Filter => "filter / search",
    Up => "up",
    Down => "down",
    PageUp => "page up",
    PageDown => "page down",
    Top => "top",
    Bottom => "bottom",
    Enter => "open / view",
    ContextMenu => "actions menu",
    RepoStateMenu => "merge/rebase menu",
    CopySha => "copy to clipboard",
    // status panel
    PullRebase => "pull --rebase",
    PullFfOnly => "pull --ff-only",
    RecentRepo => "open repository",
    // files
    StageToggle => "stage / unstage",
    StageAll => "stage / unstage all",
    Discard => "discard changes",
    Commit => "commit",
    Amend => "amend last commit",
    Ignore => "add to .gitignore",
    ToggleTree => "tree / flat view",
    FileHistory => "file history",
    Blame => "blame",
    EditFile => "open in $EDITOR",
    UseOurs => "resolve: use ours",
    UseTheirs => "resolve: use theirs",
    StashAll => "stash all",
    StashMenu => "stash options",
    ResetMenu => "reset options",
    // branches
    Checkout => "checkout",
    CheckoutByName => "checkout by name",
    NewBranch => "new branch",
    DeleteBranch => "delete branch",
    RenameBranch => "rename branch",
    MergeBranch => "merge into current",
    RebaseOnto => "rebase current onto",
    SetUpstream => "set upstream",
    FastForward => "fast-forward from upstream",
    // tags
    NewTag => "new tag",
    DeleteTag => "delete tag",
    PushTag => "push tag",
    // remotes
    AddRemote => "add remote",
    RemoveRemote => "remove remote",
    EditRemote => "edit remote url",
    FetchRemote => "fetch remote",
    // commits
    CheckoutCommit => "checkout (detached)",
    CherryCopy => "copy for cherry-pick",
    CherryPaste => "paste (cherry-pick)",
    CherryClear => "clear cherry-pick selection",
    Revert => "revert commit",
    InteractiveRebase => "interactive rebase from here",
    Reword => "reword commit",
    Fixup => "create fixup! for commit",
    Squash => "squash into parent",
    DropCommit => "drop commit",
    AutosquashRebase => "rebase --autosquash",
    TagCommit => "tag commit",
    BranchFromCommit => "new branch at commit",
    ResetToCommit => "reset to commit",
    // stash
    StashApply => "apply stash",
    StashPop => "pop stash",
    StashDrop => "drop stash",
    StashBranch => "branch from stash",
    // submodules
    EnterSubmodule => "enter submodule",
    SubmoduleUpdate => "update submodule",
    SubmoduleInit => "init submodule",
    SubmoduleAdd => "add submodule",
    ParentRepo => "back to parent repo",
    // worktrees
    SwitchWorktree => "switch to worktree",
    AddWorktree => "add worktree",
    RemoveWorktree => "remove worktree",
    // main / diff
    StageSelection => "stage / unstage line(s)",
    StageHunk => "stage / unstage hunk",
    RangeSelect => "select line range",
    DiscardSelection => "discard hunk or lines",
    ToggleStagedView => "toggle staged / unstaged",
    SideBySide => "side-by-side view",
    WordDiff => "word diff",
    FullFile => "show whole file",
    NextHunk => "next hunk",
    PrevHunk => "prev hunk",
    NextFile => "next file",
    PrevFile => "prev file",
    ScrollLeft => "scroll left",
    ScrollRight => "scroll right",
    // conflict
    PickOurs => "pick ours",
    PickTheirs => "pick theirs",
    PickBoth => "pick both",
    NextConflict => "next conflict",
    PrevConflict => "prev conflict",
    MarkResolved => "mark resolved (git add)",
}
