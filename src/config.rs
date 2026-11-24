//! ~/.config/gitmux/config.toml

use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Deserialize, Default, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct Theme {
    pub accent: Option<String>,
    pub selected_bg: Option<String>,
    pub added: Option<String>,
    pub removed: Option<String>,
    pub hunk: Option<String>,
    pub border: Option<String>,
    pub dim: Option<String>,
    pub added_bg: Option<String>,
    pub removed_bg: Option<String>,
    pub added_word_bg: Option<String>,
    pub removed_word_bg: Option<String>,
    pub syntax_theme: Option<String>,
}

#[derive(Deserialize, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub commit_limit: usize,
    pub tree_view: bool,
    pub side_by_side: bool,
    pub syntax_highlight: bool,
    pub word_diff: bool,
    pub left_width: u16,
    pub show_command_log: bool,
    pub auto_fetch_secs: u64,
    pub default_remote: String,
    pub theme: Theme,
    pub keybindings: HashMap<String, HashMap<String, String>>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            commit_limit: 300,
            tree_view: true,
            side_by_side: false,
            syntax_highlight: true,
            word_diff: true,
            left_width: 40,
            show_command_log: true,
            auto_fetch_secs: 0,
            default_remote: "origin".into(),
            theme: Theme::default(),
            keybindings: HashMap::new(),
        }
    }
}

pub fn path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "gitmux").map(|d| d.config_dir().join("config.toml"))
}

pub fn load() -> (Config, Option<String>) {
    let Some(p) = path() else {
        return (Config::default(), None);
    };
    match std::fs::read_to_string(&p) {
        Ok(s) => match toml::from_str(&s) {
            Ok(c) => (c, None),
            Err(e) => (Config::default(), Some(format!("{}: {e}", p.display()))),
        },
        Err(_) => (Config::default(), None),
    }
}

/// Recent repositories are stored separately so the user's config is never rewritten.
pub fn recent_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "gitmux").map(|d| d.data_dir().join("recent.txt"))
}

pub fn recent() -> Vec<String> {
    recent_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|s| s.lines().map(String::from).collect())
        .unwrap_or_default()
}

pub fn add_recent(repo: &str) {
    let Some(p) = recent_path() else { return };
    let mut r = recent();
    r.retain(|x| x != repo);
    r.insert(0, repo.to_string());
    r.truncate(20);
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(p, r.join("\n"));
}

#[cfg(test)]
mod tests {
    #[test]
    fn example_parses() {
        let c: super::Config = toml::from_str(include_str!("../examples/config.toml")).unwrap();
        assert_eq!(c.commit_limit, 500);
        let (_, errs) = crate::keymap::Keymap::new(&c.keybindings);
        assert!(errs.is_empty(), "{errs:?}");
    }
}
