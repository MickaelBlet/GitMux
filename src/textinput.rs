//! Minimal single/multi-line text editor.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Clone, Debug, Default)]
pub struct TextInput {
    pub lines: Vec<String>,
    pub row: usize,
    pub col: usize,
    pub multiline: bool,
}

fn byte_idx(s: &str, col: usize) -> usize {
    s.char_indices().nth(col).map(|(i, _)| i).unwrap_or(s.len())
}

impl TextInput {
    pub fn new(text: &str, multiline: bool) -> Self {
        let mut t = TextInput {
            lines: vec![String::new()],
            row: 0,
            col: 0,
            multiline,
        };
        t.insert_str(text);
        t
    }
    pub fn text(&self) -> String {
        self.lines.join("\n")
    }
    pub fn is_empty(&self) -> bool {
        self.lines.iter().all(|l| l.is_empty())
    }
    fn cur(&mut self) -> &mut String {
        &mut self.lines[self.row]
    }
    fn len(&self) -> usize {
        self.lines[self.row].chars().count()
    }
    pub fn insert(&mut self, c: char) {
        let col = self.col;
        let line = self.cur();
        let i = byte_idx(line, col);
        line.insert(i, c);
        self.col += 1;
    }
    pub fn insert_str(&mut self, s: &str) {
        for c in s.chars() {
            match c {
                '\n' => self.newline(),
                '\r' => {}
                c => self.insert(c),
            }
        }
    }
    pub fn newline(&mut self) {
        if !self.multiline {
            return;
        }
        let col = self.col;
        let line = self.cur();
        let i = byte_idx(line, col);
        let rest = line.split_off(i);
        self.lines.insert(self.row + 1, rest);
        self.row += 1;
        self.col = 0;
    }
    pub fn backspace(&mut self) {
        if self.col > 0 {
            let col = self.col;
            let line = self.cur();
            let i = byte_idx(line, col - 1);
            line.remove(i);
            self.col -= 1;
        } else if self.row > 0 {
            let l = self.lines.remove(self.row);
            self.row -= 1;
            self.col = self.len();
            self.cur().push_str(&l);
        }
    }
    pub fn delete(&mut self) {
        if self.col < self.len() {
            let col = self.col;
            let line = self.cur();
            let i = byte_idx(line, col);
            line.remove(i);
        } else if self.row + 1 < self.lines.len() {
            let l = self.lines.remove(self.row + 1);
            self.cur().push_str(&l);
        }
    }
    pub fn delete_word(&mut self) {
        while self.col > 0 && self.lines[self.row].chars().nth(self.col - 1) == Some(' ') {
            self.backspace();
        }
        while self.col > 0 && self.lines[self.row].chars().nth(self.col - 1) != Some(' ') {
            self.backspace();
        }
    }
    pub fn set_cursor(&mut self, row: usize, col: usize) {
        self.row = row.min(self.lines.len() - 1);
        self.col = col.min(self.len());
    }

    /// Returns true if the key was consumed.
    pub fn handle_key(&mut self, k: &KeyEvent) -> bool {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        match k.code {
            KeyCode::Char('w') if ctrl => self.delete_word(),
            KeyCode::Char('u') if ctrl => {
                self.lines[self.row].clear();
                self.col = 0;
            }
            KeyCode::Char('a') if ctrl => self.col = 0,
            KeyCode::Char('e') if ctrl => self.col = self.len(),
            KeyCode::Char('j') if ctrl => self.newline(),
            KeyCode::Char(c) if !ctrl => self.insert(c),
            KeyCode::Backspace => self.backspace(),
            KeyCode::Delete => self.delete(),
            KeyCode::Left => {
                if self.col > 0 {
                    self.col -= 1
                } else if self.row > 0 {
                    self.row -= 1;
                    self.col = self.len();
                }
            }
            KeyCode::Right => {
                if self.col < self.len() {
                    self.col += 1
                } else if self.row + 1 < self.lines.len() {
                    self.row += 1;
                    self.col = 0;
                }
            }
            KeyCode::Up if self.row > 0 => {
                self.row -= 1;
                self.col = self.col.min(self.len());
            }
            KeyCode::Down if self.row + 1 < self.lines.len() => {
                self.row += 1;
                self.col = self.col.min(self.len());
            }
            KeyCode::Home => self.col = 0,
            KeyCode::End => self.col = self.len(),
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn edit() {
        let mut t = TextInput::new("héllo\nworld", true);
        assert_eq!((t.row, t.col), (1, 5));
        t.backspace();
        t.set_cursor(0, 1);
        t.delete();
        t.insert('e');
        assert_eq!(t.text(), "hello\nworl");
        t.set_cursor(1, 0);
        t.backspace();
        assert_eq!(t.text(), "helloworl");
        let mut s = TextInput::new("a\nb", false);
        s.newline();
        assert_eq!(s.text(), "ab");
    }
}
