# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.0.1] - 2026-11-24

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
- Syntax highlighting (syntect) in diffs and commit / stash views, computed
  in the background (`syntax_highlight`, `theme.syntax_theme`).
- Colored backgrounds for added / removed lines and changed-word emphasis
  (`word_diff`, `theme.added_bg` / `removed_bg` / `added_word_bg` /
  `removed_word_bg`).
- Side-by-side, word diff and full-context (`F`) display in commit / stash
  views, with toggle buttons in the main view title.
- Next / previous file (`}` / `{`) and hunk (`n` / `N`) navigation in commit
  views; the current file header stays pinned on top (click to jump back),
  double-click a `--stat` line to jump to its diff.
- Commit tooltip on hover: full hash, author / email, date, refs, parents,
  full message.
- Draggable scrollbar in help / message popups.
- Go-to-file picker (`o` in the main view, commits and reflog panels):
  fuzzy-filterable list of the commit / stash files with `+/-` counts,
  jumps to the selected file's diff (keyboard and mouse).

### Changed

- `left_width_pct` config option replaced by `left_width` (columns, min 20).

[Unreleased]: https://github.com/MickaelBlet/GitMux/compare/v0.0.1...HEAD
[0.0.1]: https://github.com/MickaelBlet/GitMux/releases/tag/v0.0.1
