# Contributing

## Build and run

```sh
scripts/build.sh              # release build
scripts/build.sh --debug      # debug build
cargo run -- ~/some/repo      # run on a repository
```

## Checks

Run before committing:

```sh
scripts/build.sh --check
```

It runs `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
`cargo test`, then the build.

## Tests

Unit tests live in inline `#[cfg(test)]` modules: diff parsing and patch
building, conflict resolution, graph lanes, rebase todo, keymap parsing, text
input, hit map, OSC 52 encoding, config example parsing. `git/read.rs` has
integration tests running against temporary repositories (requires `git`).

## Conventions

- New action → `action.rs` + default key in `keymap.rs` + `dispatch.rs` +
  [keybindings.md](keybindings.md).
- New clickable element → push its hit after the area it sits on.
- Code, identifiers, comments and commit messages are in English.
