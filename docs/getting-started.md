# Getting started

## Build

Requires a Rust toolchain (edition 2024) and `git` in `PATH`. libgit2 is built
from source by the `git2` crate (no OpenSSL needed: network operations use the
`git` executable).

```sh
scripts/build.sh            # release -> target/release/gitmux
scripts/build.sh --debug    # debug build
cargo install --path .      # install into ~/.cargo/bin
```

## Command line

```sh
gitmux [PATH] [--nerd-font]
```

| Option        | Meaning                                                        |
| ------------- | -------------------------------------------------------------- |
| `PATH`        | repository (or any directory inside it); default: current dir  |
| `--nerd-font` | Nerd Font icons and pill buttons; also `GITMUX_NERD_FONT=1`    |
| `-h`, `-V`    | help, version                                                  |

## The screen

```
╭[1] Status─────────╮╭[0] main view ─────────────── [unstaged][side-by-side][word]╮
│repo → main ↑1 ↓0  ││ diff / commit / blame / history / conflict                 │
╭[2] Files──────────╮│                                                            │
╭[3] Local│Remote│Tags│Remotes╮                                                   │
╭[4] Commits│Reflog─╮╰────────────────────────────────────────────────────────────╯
╭[5] Stash│Submodules│Worktrees╮ Command log (every git command run)             │
[hints: clickable]                                            spinner · messages
```

- Left: five panels, `1`–`5` (or click) to focus, `[` / `]` (or click a tab
  title) to switch tabs. The focused panel grows.
- Right: the main view previews the selection of the focused panel. `0` or
  `enter` focuses it; `esc` goes back.
- Bottom: the command log (`@` to toggle) and a hint bar whose entries are
  buttons.

## Typical workflows

- **Stage lines**: select a file, `enter`, move to a line, `space` (line) or
  `a` (hunk); `v` starts a range; or drag with the mouse and double-click.
- **Commit**: `c`, type, `enter`. `A` amends. Nothing staged → offers to stage all.
- **Push / pull / fetch**: `P` / `p` / `f`. A rejected push offers
  `--force-with-lease`. Credential prompts appear as popups.
- **Rebase**: in commits, `i` on the oldest commit to edit, set actions, `enter`.
  `r` rewords, `s` squashes into parent, `d` drops, `F` creates a `fixup!`.
- **Conflicts**: select the conflicted file, `enter`, then `o` / `t` / `b`
  (or double-click a side). When all are resolved the file is staged;
  `m` → continue.
- **Undo**: `z` undoes the last reflog action (checkout or reset `--keep`).
