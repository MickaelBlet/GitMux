//! Interactive rebase todo model.

use git2::Oid;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TodoAction {
    Pick,
    Reword,
    Edit,
    Squash,
    Fixup,
    Drop,
}

impl TodoAction {
    pub const ALL: [TodoAction; 6] = [
        Self::Pick,
        Self::Reword,
        Self::Edit,
        Self::Squash,
        Self::Fixup,
        Self::Drop,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Pick => "pick",
            Self::Reword => "reword",
            Self::Edit => "edit",
            Self::Squash => "squash",
            Self::Fixup => "fixup",
            Self::Drop => "drop",
        }
    }
    pub fn key(self) -> char {
        self.name().chars().next().unwrap()
    }
}

#[derive(Clone, Debug)]
pub struct TodoEntry {
    pub action: TodoAction,
    pub oid: Oid,
    pub summary: String,
    /// new message for `reword`
    pub message: Option<String>,
}

/// Build the todo file. Entries are given newest-first (as displayed) and
/// written oldest-first. Rewords become `pick` + `exec git commit --amend -F <file>`.
pub fn render(entries: &[TodoEntry], msg_dir: &std::path::Path) -> std::io::Result<String> {
    let mut s = String::new();
    for (i, e) in entries.iter().rev().enumerate() {
        match (e.action, &e.message) {
            (TodoAction::Reword, Some(m)) => {
                let f = msg_dir.join(format!("msg-{i}.txt"));
                std::fs::write(&f, m)?;
                s.push_str(&format!("pick {} {}\n", e.oid, e.summary));
                s.push_str(&format!(
                    "exec git commit --amend --allow-empty -F {}\n",
                    shell_quote(&f.to_string_lossy())
                ));
            }
            (a, _) => {
                let a = if a == TodoAction::Reword {
                    TodoAction::Pick
                } else {
                    a
                };
                s.push_str(&format!("{} {} {}\n", a.name(), e.oid, e.summary));
            }
        }
    }
    Ok(s)
}

pub fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn order_and_reword() {
        let d = tempfile::tempdir().unwrap();
        let a = Oid::from_bytes(&[1; 20]).unwrap();
        let b = Oid::from_bytes(&[2; 20]).unwrap();
        let e = vec![
            TodoEntry {
                action: TodoAction::Reword,
                oid: b,
                summary: "new".into(),
                message: Some("msg".into()),
            },
            TodoEntry {
                action: TodoAction::Pick,
                oid: a,
                summary: "old".into(),
                message: None,
            },
        ];
        let s = render(&e, d.path()).unwrap();
        let lines: Vec<_> = s.lines().collect();
        assert!(lines[0].starts_with(&format!("pick {a}")));
        assert!(lines[1].starts_with(&format!("pick {b}")));
        assert!(lines[2].starts_with("exec git commit --amend"));
    }
}
