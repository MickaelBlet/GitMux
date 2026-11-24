# GitMux

Git TUI with full mouse support, written in Rust.

```sh
gitmux [PATH] [--nerd-font]
```

## Features

- Status, files (tree / flat), stage / unstage files, **hunks and single lines**, discard, ignore
- Diff viewer: unified, side-by-side, word diff, staged / unstaged toggle
- Commit, amend, reword, fixup, signoff, `--no-verify`
- Commit log with graph, reflog, commit details, file history, blame
- Branches (local / remote), tags, remotes: checkout, create, rename, delete, merge, rebase, upstream, fast-forward
- Fetch / pull / push (force-with-lease on rejection), credentials asked in a popup
- Stash (all, tracked, keep-index, staged), apply / pop / drop / branch
- Interactive rebase (pick / reword / edit / squash / fixup / drop, reorder), squash, drop, autosquash
- Cherry-pick (multi-select), revert, reset (soft / mixed / keep / hard), undo via reflog
- Merge conflict resolution: ours / theirs / both per conflict, continue / abort / skip
- Submodules and worktrees, bisect
- Filter any list, custom git command, command log, copy via OSC 52
- Mouse everywhere: click, double-click, right-click menus, wheel, drag to resize / select lines / reorder
- Configurable keybindings and colors, optional Nerd Font icons

## Quick start

```sh
scripts/build.sh                 # -> target/release/gitmux
target/release/gitmux ~/my/repo
```

Press `?` for every key, `x` (or right-click) for the actions of the focused panel.

## Documentation

See [`docs/`](docs/README.md): [Getting started](docs/getting-started.md) ·
[Keybindings](docs/keybindings.md) · [Mouse](docs/mouse.md) ·
[Configuration](docs/configuration.md) · [Architecture](docs/architecture.md) ·
[Contributing](docs/contributing.md)
