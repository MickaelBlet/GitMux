# Keybindings

Keys depend on the focused panel (and tab); global keys apply everywhere unless the panel overrides them. Press `?` in GitMux for this list, `x` for a clickable menu of the focused panel's actions.

The config section name (see [configuration](configuration.md)) and the action name used to rebind a key are given for each table.

## Global (`global`)

| Key | Action | Name |
| --- | --- | --- |
| `q` | quit | `Quit` |
| `?` | help | `Help` |
| `tab` | next panel | `NextPanel` |
| `shift+tab` | prev panel | `PrevPanel` |
| `1` | status panel | `FocusStatus` |
| `2` | files panel | `FocusFiles` |
| `3` | branches panel | `FocusBranches` |
| `4` | commits panel | `FocusCommits` |
| `5` | stash panel | `FocusStash` |
| `0` | main panel | `FocusMain` |
| `]` | next tab | `NextTab` |
| `[` | prev tab | `PrevTab` |
| `R` | refresh | `Refresh` |
| `P` | push | `Push` |
| `p` | pull | `Pull` |
| `f` | fetch all | `Fetch` |
| `z` | undo (reflog) | `Undo` |
| `@` | toggle command log | `ToggleLog` |
| `:` | run git command | `CustomCommand` |
| `/` | filter / search | `Filter` |
| `x` | actions menu | `ContextMenu` |
| `m` | merge/rebase menu | `RepoStateMenu` |
| `y` | copy to clipboard | `CopySha` |
| `k` | up | `Up` |
| `up` | up | `Up` |
| `j` | down | `Down` |
| `down` | down | `Down` |
| `pgup` | page up | `PageUp` |
| `ctrl+u` | page up | `PageUp` |
| `pgdown` | page down | `PageDown` |
| `ctrl+d` | page down | `PageDown` |
| `home` | top | `Top` |
| `g` | top | `Top` |
| `end` | bottom | `Bottom` |
| `G` | bottom | `Bottom` |
| `enter` | open / view | `Enter` |
| `backspace` | back to parent repo | `ParentRepo` |

## Status panel (`status`)

| Key | Action | Name |
| --- | --- | --- |
| `r` | pull --rebase | `PullRebase` |
| `F` | pull --ff-only | `PullFfOnly` |
| `o` | open repository | `RecentRepo` |
| `S` | force push (with lease) | `ForcePush` |

## Files (`files`)

| Key | Action | Name |
| --- | --- | --- |
| `space` | stage / unstage | `StageToggle` |
| `a` | stage / unstage all | `StageAll` |
| `c` | commit | `Commit` |
| `A` | amend last commit | `Amend` |
| `d` | discard changes | `Discard` |
| `s` | stash all | `StashAll` |
| `S` | stash options | `StashMenu` |
| `D` | reset options | `ResetMenu` |
| `i` | add to .gitignore | `Ignore` |
| `e` | open in $EDITOR | `EditFile` |
| `t` | tree / flat view | `ToggleTree` |
| `h` | file history | `FileHistory` |
| `b` | blame | `Blame` |
| `o` | resolve: use ours | `UseOurs` |
| `T` | resolve: use theirs | `UseTheirs` |

## Local branches (`branches`)

| Key | Action | Name |
| --- | --- | --- |
| `space` | checkout | `Checkout` |
| `c` | checkout by name | `CheckoutByName` |
| `n` | new branch | `NewBranch` |
| `d` | delete branch | `DeleteBranch` |
| `r` | rename branch | `RenameBranch` |
| `M` | merge into current | `MergeBranch` |
| `e` | rebase current onto | `RebaseOnto` |
| `u` | set upstream | `SetUpstream` |
| `F` | fast-forward from upstream | `FastForward` |
| `T` | tag commit | `TagCommit` |

## Remote branches (`remote_branches`)

| Key | Action | Name |
| --- | --- | --- |
| `space` | checkout | `Checkout` |
| `n` | new branch | `NewBranch` |
| `d` | delete branch | `DeleteBranch` |
| `M` | merge into current | `MergeBranch` |
| `e` | rebase current onto | `RebaseOnto` |

## Tags (`tags`)

| Key | Action | Name |
| --- | --- | --- |
| `space` | checkout | `Checkout` |
| `n` | new tag | `NewTag` |
| `d` | delete tag | `DeleteTag` |
| `P` | push tag | `PushTag` |

## Remotes (`remotes`)

| Key | Action | Name |
| --- | --- | --- |
| `n` | add remote | `AddRemote` |
| `d` | remove remote | `RemoveRemote` |
| `e` | edit remote url | `EditRemote` |
| `F` | fetch remote | `FetchRemote` |

## Commits (`commits`)

| Key | Action | Name |
| --- | --- | --- |
| `space` | checkout (detached) | `CheckoutCommit` |
| `C` | copy for cherry-pick | `CherryCopy` |
| `V` | paste (cherry-pick) | `CherryPaste` |
| `ctrl+r` | clear cherry-pick selection | `CherryClear` |
| `t` | revert commit | `Revert` |
| `i` | interactive rebase from here | `InteractiveRebase` |
| `r` | reword commit | `Reword` |
| `F` | create fixup! for commit | `Fixup` |
| `s` | squash into parent | `Squash` |
| `d` | drop commit | `DropCommit` |
| `S` | rebase --autosquash | `AutosquashRebase` |
| `T` | tag commit | `TagCommit` |
| `n` | new branch at commit | `BranchFromCommit` |
| `D` | reset to commit | `ResetToCommit` |

## Reflog (`reflog`)

| Key | Action | Name |
| --- | --- | --- |
| `space` | checkout (detached) | `CheckoutCommit` |
| `D` | reset to commit | `ResetToCommit` |
| `C` | copy for cherry-pick | `CherryCopy` |
| `n` | new branch at commit | `BranchFromCommit` |

## Stash (`stash`)

| Key | Action | Name |
| --- | --- | --- |
| `space` | apply stash | `StashApply` |
| `a` | apply stash | `StashApply` |
| `o` | pop stash | `StashPop` |
| `d` | drop stash | `StashDrop` |
| `n` | branch from stash | `StashBranch` |

## Submodules (`submodules`)

| Key | Action | Name |
| --- | --- | --- |
| `space` | enter submodule | `EnterSubmodule` |
| `u` | update submodule | `SubmoduleUpdate` |
| `i` | init submodule | `SubmoduleInit` |
| `n` | add submodule | `SubmoduleAdd` |

## Worktrees (`worktrees`)

| Key | Action | Name |
| --- | --- | --- |
| `space` | switch to worktree | `SwitchWorktree` |
| `n` | add worktree | `AddWorktree` |
| `d` | remove worktree | `RemoveWorktree` |

## Main view: diff (`diff`)

| Key | Action | Name |
| --- | --- | --- |
| `space` | stage / unstage line(s) | `StageSelection` |
| `a` | stage / unstage hunk | `StageHunk` |
| `v` | select line range | `RangeSelect` |
| `d` | discard hunk or lines | `DiscardSelection` |
| `s` | toggle staged / unstaged | `ToggleStagedView` |
| `\|` | side-by-side view | `SideBySide` |
| `w` | toggle changed-word highlighting | `WordDiff` |
| `F` | show whole file (full context) | `FullFile` |
| `n` | next hunk | `NextHunk` |
| `N` | prev hunk | `PrevHunk` |
| `e` | open in $EDITOR | `EditFile` |
| `left` | scroll left | `ScrollLeft` |
| `right` | scroll right | `ScrollRight` |

## Main view: other (`main`)

| Key | Action | Name |
| --- | --- | --- |
| `left` | scroll left | `ScrollLeft` |
| `right` | scroll right | `ScrollRight` |
| `\|` | side-by-side view (commit / stash diffs) | `SideBySide` |
| `w` | toggle changed-word highlighting | `WordDiff` |
| `F` | full context (commit view) | `FullFile` |
| `n` | next hunk | `NextHunk` |
| `N` | prev hunk | `PrevHunk` |
| `}` | next file | `NextFile` |
| `{` | prev file | `PrevFile` |

In commit / stash views the file being read stays pinned on the first row; click
it to jump back to the start of that file. Double-click a `--stat` line to jump
to its diff.

## Main view: conflict (`conflict`)

| Key | Action | Name |
| --- | --- | --- |
| `o` | pick ours | `PickOurs` |
| `t` | pick theirs | `PickTheirs` |
| `b` | pick both | `PickBoth` |
| `n` | next conflict | `NextConflict` |
| `N` | prev conflict | `PrevConflict` |
| `space` | mark resolved (git add) | `MarkResolved` |
| `e` | open in $EDITOR | `EditFile` |

## Popups

| Popup | Keys |
| --- | --- |
| Confirm | `y` / `enter` yes, `n` / `esc` no |
| Input | `enter` OK, `esc` cancel, `ctrl+w` delete word, `ctrl+u` clear, `ctrl+a` / `ctrl+e` home / end |
| Menu | `↑` `↓` / `j` `k`, `enter`, item letter, `esc` |
| Commit | `enter` commit, `alt+enter` / `shift+enter` / `ctrl+j` new line, `alt+a` amend, `alt+n` no-verify, `alt+s` signoff |
| Rebase todo | `p r e s f d` set action, `ctrl+↑/↓` or `K` / `J` move, `enter` start, `esc` cancel |
| Help / message | `↑` `↓` `pgup` `pgdn` scroll, `esc` / `q` close |

`esc` outside popups: leaves the main view (or goes back in its history), clears a range selection, a filter, or the cherry-pick selection. `ctrl+c` quits.
