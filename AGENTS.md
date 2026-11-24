# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

GitMux: git TUI in Rust (edition 2024), built on ratatui/crossterm and git2 (libgit2, no default features). Single binary crate, no lib target. Full keyboard and mouse support.

## Commands

```sh
scripts/build.sh            # release build -> target/release/gitmux
scripts/build.sh --debug    # debug build
scripts/build.sh --check    # fmt --check + clippy -D warnings + tests, then build
cargo test                  # all unit / integration tests (inline #[cfg(test)] modules)
cargo run -- <repo> [--nerd-font]
```

## Architecture

Event loop (`main.rs`): one `mpsc` channel of `app::Event`. An input thread forwards crossterm events and emits `Tick` every 200 ms (paused while `$EDITOR` runs). Background git jobs send `JobDone`. The loop draws, blocks on an event, then drains pending events before redrawing.

Git access (`git/`): **reads go through git2** (`read.rs`: status, refs, log, reflog, stash, submodules, blame), **writes shell out to `git`** (`cmd.rs`, every command is recorded in the command log). Diffs are read as text with `git diff` and parsed in `diff.rs`, which also builds partial patches for hunk / line staging (`git apply --cached --recount`) and resolves conflict markers. `graph.rs` computes commit-graph lanes; `rebase.rs` renders interactive-rebase todo files (applied with `GIT_SEQUENCE_EDITOR="cp <todo>"`, rewords become `exec git commit --amend -F`).

State (`app.rs`, `App`): `RepoData` snapshot, focused `PanelId`, per-(panel, tab) `ListState`, the main view (`MainView` / `MainKind`: text, diff, conflict, blame, file history) with a back stack, and a popup stack (`Popup`). `Op` is a concrete git operation, `Then` is what to do when it finishes.

Input: keys resolve through `keymap.rs` (context-sensitive `Ctx` → `Action`, overridable from config) and mouse clicks through the hit map; both end in `App::dispatch(Action)` (`dispatch.rs`). `ops.rs` executes `Op`s synchronously or in a thread (network, commit, rebase, merge). `input.rs` handles keys, mouse, popups and ticks.

Rendering (`ui/`): `ui::draw` registers every clickable rect in `app.hits` (`ui/hit.rs`, later-pushed hits win) while drawing; mouse handling resolves clicks with `hits.at(x, y)`. Popups are in `ui/popups.rs`. `--nerd-font` switches icons (`ui/icons.rs`) and draws `[x]` buttons as rounded pills.

Askpass (`askpass.rs`): `GIT_ASKPASS` / `SSH_ASKPASS` point to the gitmux binary itself; it exchanges prompt / answer files with the running TUI in a private temp dir, so credentials are asked in a popup.

## Conventions

- New action → add it to `action.rs`, a default key in `keymap.rs`, handle it in `dispatch.rs`, document it in `docs/keybindings.md`.
- New clickable element → push its hit after the area it sits on.
- Config schema change → update `config.rs` and `docs/configuration.md` / `examples/config.toml`.
- Code, identifiers, comments and commit messages are in English.
