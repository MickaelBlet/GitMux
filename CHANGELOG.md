# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Git TUI with status, files (tree / flat), branches, remotes, tags, commits
  with graph, reflog, stash, submodules and worktrees panels.
- Hunk and line staging / unstaging / discarding; unified, side-by-side and
  word diff views.
- Commit, amend, reword, fixup, interactive rebase, squash, drop, autosquash,
  cherry-pick, revert, reset, undo through the reflog.
- Merge / rebase conflict resolution and continue / abort / skip menu; bisect.
- Fetch / pull / push in the background, credential prompts through a
  built-in `GIT_ASKPASS` / `SSH_ASKPASS` helper.
- Blame, file history, list filtering, custom git commands, command log,
  OSC 52 clipboard.
- Full mouse support: click, double-click, right-click actions menu, wheel,
  drag to resize panels, select diff lines and reorder rebase todo.
- Configurable keybindings and theme (`~/.config/gitmux/config.toml`).
- `--nerd-font` option (or `GITMUX_NERD_FONT=1`) for Nerd Font icons and
  rounded pill buttons.
- `scripts/build.sh` (release / `--debug` / `--check`) and documentation in
  `docs/` with an example config in `examples/`.
- GitHub Actions workflow building x86_64 / aarch64 Linux binaries
  compatible with glibc 2.28.
