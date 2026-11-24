//! Hit-testing: every clickable rect drawn in a frame is registered here.
//! Later-pushed hits win.

use crate::action::Action;
use crate::app::PanelId;
use crate::git::diff::Pick;
use crate::git::rebase::TodoAction;
use ratatui::layout::{Position, Rect};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Btn {
    Ok,
    Cancel,
    Amend,
    NoVerify,
    Signoff,
    Conflict(Pick),
    Back,
    StageView,
    SideBySide,
    WordDiff,
    FullFile,
    /// Sticky file header of a text view: jump to the file's diff.
    StickyFile(usize),
    TodoUp,
    TodoDown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    Panel(PanelId),
    Row(PanelId, usize),
    Tab(PanelId, usize),
    Scrollbar(PanelId),
    /// Scrollbar of the top popup, with its max scroll offset.
    PopupScrollbar(usize),
    SplitV,
    SplitH,
    Hint(Action),
    Popup,
    MenuItem(usize),
    Button(Btn),
    TodoRow(usize),
    TodoAction(usize, TodoAction),
    MainLine(usize),
    InputText,
}

#[derive(Default, Debug)]
pub struct HitMap {
    pub hits: Vec<(Rect, Hit)>,
    /// rect of the input text area (for cursor placement), with its first visible row
    pub input_area: Option<(Rect, usize)>,
}

impl HitMap {
    pub fn clear(&mut self) {
        self.hits.clear();
        self.input_area = None;
    }
    pub fn push(&mut self, r: Rect, h: Hit) {
        if r.width > 0 && r.height > 0 {
            self.hits.push((r, h));
        }
    }
    pub fn at(&self, x: u16, y: u16) -> Option<(Rect, Hit)> {
        self.hits
            .iter()
            .rev()
            .find(|(r, _)| r.contains(Position::new(x, y)))
            .copied()
    }
    /// The panel under a point (for wheel scrolling), ignoring popups.
    pub fn panel_at(&self, x: u16, y: u16) -> Option<PanelId> {
        self.hits.iter().rev().find_map(|(r, h)| match h {
            Hit::Panel(p) if r.contains(Position::new(x, y)) => Some(*p),
            _ => None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn later_wins() {
        let mut m = HitMap::default();
        m.push(Rect::new(0, 0, 10, 10), Hit::Panel(PanelId::Files));
        m.push(Rect::new(0, 2, 10, 1), Hit::Row(PanelId::Files, 1));
        assert_eq!(m.at(3, 2).unwrap().1, Hit::Row(PanelId::Files, 1));
        assert_eq!(m.at(3, 5).unwrap().1, Hit::Panel(PanelId::Files));
        assert_eq!(m.panel_at(3, 2), Some(PanelId::Files));
        assert!(m.at(20, 20).is_none());
    }
}
