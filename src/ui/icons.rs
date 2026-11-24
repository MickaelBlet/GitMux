//! Glyphs: plain Unicode by default, Nerd Font icons with `--nerd-font`.

pub struct Icons {
    pub branch: &'static str,
    pub remote_branch: &'static str,
    pub tag: &'static str,
    pub remote: &'static str,
    pub stash: &'static str,
    pub submodule: &'static str,
    pub worktree: &'static str,
    pub dir_open: &'static str,
    pub dir_closed: &'static str,
    pub file: &'static str,
    pub conflict: &'static str,
    pub head: &'static str,
    pub cherry: &'static str,
    pub ahead: &'static str,
    pub behind: &'static str,
    pub spinner: &'static [&'static str],
}

pub const PLAIN: Icons = Icons {
    branch: "",
    remote_branch: "",
    tag: "",
    remote: "",
    stash: "",
    submodule: "",
    worktree: "",
    dir_open: "▼ ",
    dir_closed: "▶ ",
    file: "",
    conflict: "! ",
    head: "* ",
    cherry: "✓",
    ahead: "↑",
    behind: "↓",
    spinner: &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"],
};

pub const NERD: Icons = Icons {
    branch: "\u{e725} ",
    remote_branch: "\u{f0c2} ",
    tag: "\u{f02b} ",
    remote: "\u{f0ac} ",
    stash: "\u{f01c} ",
    submodule: "\u{f1d3} ",
    worktree: "\u{f1bb} ",
    dir_open: "\u{f07c} ",
    dir_closed: "\u{f07b} ",
    file: "\u{f15b} ",
    conflict: "\u{f071} ",
    head: "\u{f058} ",
    cherry: "\u{f00c}",
    ahead: "\u{f062}",
    behind: "\u{f063}",
    spinner: &[
        "\u{ee06}", "\u{ee07}", "\u{ee08}", "\u{ee09}", "\u{ee0a}", "\u{ee0b}",
    ],
};

/// Nerd Font (Powerline) rounded caps used for pill buttons.
pub const PILL_LEFT: &str = "\u{e0b6}";
pub const PILL_RIGHT: &str = "\u{e0b4}";
