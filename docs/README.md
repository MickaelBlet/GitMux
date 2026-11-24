# GitMux documentation

GitMux is a git TUI written in Rust on top of ratatui/crossterm and libgit2.
It runs as a single binary and works with the mouse as well as the keyboard.

## User guide

| Page                                  | Contents                                          |
| ------------------------------------- | ------------------------------------------------- |
| [Getting started](getting-started.md) | Building, running, command line, a tour of the screen |
| [Keybindings](keybindings.md)         | Every key, per panel, and keys inside popups      |
| [Mouse](mouse.md)                     | Clicks, menus, wheel, drag gestures               |
| [Configuration](configuration.md)     | Config file, keybindings, theme, Nerd Font        |

## Developer guide

| Page                            | Contents                                          |
| ------------------------------- | ------------------------------------------------- |
| [Architecture](architecture.md) | Event loop, git layer, state, rendering, askpass  |
| [Contributing](contributing.md) | Build, checks, tests, conventions                 |
