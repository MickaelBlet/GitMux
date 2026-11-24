# Configuration

File: `~/.config/gitmux/config.toml` (XDG config dir). All keys are optional;
see [`examples/config.toml`](../examples/config.toml).

| Key                  | Default    | Meaning                                         |
| -------------------- | ---------- | ----------------------------------------------- |
| `commit_limit`       | `300`      | commits loaded at start (more load on scroll)   |
| `tree_view`          | `true`     | files panel as a tree                           |
| `side_by_side`       | `false`    | side-by-side diffs by default                   |
| `left_width_pct`     | `35`       | width of the left column (15–85)                |
| `show_command_log`   | `true`     | show the command log                            |
| `auto_fetch_secs`    | `0`        | background `fetch --all` interval (0 = off)     |
| `default_remote`     | `"origin"` | remote used when pushing a branch without upstream |

## Theme

```toml
[theme]
accent = "green"        # names, "#rrggbb" or an index "123"
selected_bg = "#323c5a"
added = "green"
removed = "red"
hunk = "cyan"
border = "darkgray"
dim = "darkgray"
```

## Keybindings

One table per context, mapping an action name to a key. A binding replaces the
default key of that action and removes any other action bound to the same key
in that context. Contexts and action names are listed in
[keybindings.md](keybindings.md).

```toml
[keybindings.files]
Commit = "C"
StageAll = "ctrl+a"

[keybindings.global]
Quit = "Q"
```

Key syntax: a character, or `enter esc tab space backspace delete up down left
right pgup pgdown home end f1..f12`, with optional `ctrl+`, `alt+`, `shift+`
prefixes.

## Nerd Font

`--nerd-font` or `GITMUX_NERD_FONT=1` uses Nerd Font icons for branches, tags,
remotes, files and folders, and draws buttons as rounded pills.

## Recent repositories

Opened repositories are remembered in the XDG data dir (`recent.txt`);
`o` in the status panel (or double-click it) opens one.
