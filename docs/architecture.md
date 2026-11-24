# Architecture

```
main.rs        args, terminal setup, event loop, $EDITOR suspension
app.rs         App state, RepoData snapshot, selection helpers, main-view previews
action.rs      Action enum (every user intent)
keymap.rs      context-sensitive key → Action, config overrides
dispatch.rs    Action → Op / popups
ops.rs         Op execution, sync or in background threads
input.rs       keyboard, mouse, popups, ticks (askpass polling, auto refresh/fetch)
askpass.rs     GIT_ASKPASS / SSH_ASKPASS bridge
config.rs      config file, recent repositories
textinput.rs   single / multi-line editor
git/cmd.rs     `git` runner + command log
git/read.rs    libgit2 queries
git/diff.rs    diff parsing, partial patches, side-by-side pairing, conflict markers
git/graph.rs   commit graph lanes
git/rebase.rs  interactive rebase todo
ui/mod.rs      panels, main view, command log, hint bar
ui/popups.rs   popups
ui/hit.rs      hit map for mouse handling
ui/icons.rs    plain / Nerd Font glyphs
```

## Event loop

One `mpsc` channel of `app::Event`: terminal events and 200 ms ticks from the
input thread, `JobDone` from background jobs. Each iteration draws, waits for
one event, then drains the queue before the next draw. When an action asks for
`$EDITOR`, the input thread is paused, the terminal restored, the editor run,
and the TUI re-initialised.

## Reads and writes

Reads use libgit2 (fast, no process spawn) and are reloaded as a whole
`RepoData` after every operation, on focus gain and when a 2 s poll of the
status / HEAD detects external changes. Writes always run the `git` binary so
hooks, config, credential helpers and signing behave as on the command line.
Git runs with `GIT_EDITOR=true`, `GIT_TERMINAL_PROMPT=0` and askpass pointing to
GitMux itself.

## Line staging

The diff of one file is parsed into hunks. To stage a selection, a patch is
built that keeps only the selected `+` / `-` lines: unselected `-` lines become
context and unselected `+` lines are dropped (reverse for unstage / discard,
applied with `-R`). It is applied with `git apply [--cached] --recount`.

## Mouse

While drawing, each clickable element pushes `(Rect, Hit)` into `app.hits`;
later pushes win. Mouse events look up the hit and map it to an `Action` or
direct state change, so keyboard and mouse share the same code paths.
