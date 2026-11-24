//! Unified diff parsing and partial patch construction (hunk / line staging).

use ratatui::text::Span;
use similar::{Algorithm, DiffTag, capture_diff_slices};
use std::collections::BTreeSet;
use std::ops::Range;

#[derive(Clone, Debug, Default)]
pub struct Hunk {
    pub header: String,
    pub old_start: usize,
    pub new_start: usize,
    pub lines: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct FileDiff {
    pub header: Vec<String>,
    pub hunks: Vec<Hunk>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowKind {
    Header,
    HunkHeader,
    Add,
    Del,
    Context,
    NoNewline,
}

#[derive(Clone, Debug)]
pub struct Row {
    pub text: String,
    pub kind: RowKind,
    pub hunk: Option<usize>,
    pub line: Option<usize>,
    /// Old / new file line numbers (content rows only).
    pub old_no: Option<usize>,
    pub new_no: Option<usize>,
    /// Styled content without the `+` / `-` / ` ` prefix, filled by `highlight::decorate`.
    pub spans: Vec<Span<'static>>,
}

fn parse_range(s: &str) -> usize {
    s.split(',')
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or(0)
}

fn parse_hunk_header(h: &str) -> (usize, usize) {
    // @@ -a,b +c,d @@
    let mut parts = h.split_whitespace().skip(1);
    let old = parts
        .next()
        .map(|p| parse_range(p.trim_start_matches('-')))
        .unwrap_or(0);
    let new = parts
        .next()
        .map(|p| parse_range(p.trim_start_matches('+')))
        .unwrap_or(0);
    (old, new)
}

/// Parse a single-file unified diff.
pub fn parse(text: &str) -> FileDiff {
    let mut fd = FileDiff::default();
    for l in text.lines() {
        if l.starts_with("@@") {
            let (o, n) = parse_hunk_header(l);
            fd.hunks.push(Hunk {
                header: l.to_string(),
                old_start: o,
                new_start: n,
                lines: vec![],
            });
        } else if let Some(h) = fd.hunks.last_mut() {
            h.lines.push(l.to_string());
        } else {
            fd.header.push(l.to_string());
        }
    }
    fd
}

impl FileDiff {
    pub fn rows(&self) -> Vec<Row> {
        let mut rows: Vec<Row> = self
            .header
            .iter()
            .map(|t| Row {
                text: t.clone(),
                kind: RowKind::Header,
                hunk: None,
                line: None,
                old_no: None,
                new_no: None,
                spans: vec![],
            })
            .collect();
        for (hi, h) in self.hunks.iter().enumerate() {
            rows.push(Row {
                text: h.header.clone(),
                kind: RowKind::HunkHeader,
                hunk: Some(hi),
                line: None,
                old_no: None,
                new_no: None,
                spans: vec![],
            });
            let (mut o, mut n) = (h.old_start, h.new_start);
            for (li, l) in h.lines.iter().enumerate() {
                let kind = match l.chars().next() {
                    Some('+') => RowKind::Add,
                    Some('-') => RowKind::Del,
                    Some('\\') => RowKind::NoNewline,
                    _ => RowKind::Context,
                };
                let (old_no, new_no) = match kind {
                    RowKind::Add => (None, Some(n)),
                    RowKind::Del => (Some(o), None),
                    RowKind::Context => (Some(o), Some(n)),
                    _ => (None, None),
                };
                o += old_no.is_some() as usize;
                n += new_no.is_some() as usize;
                rows.push(Row {
                    text: l.clone(),
                    kind,
                    hunk: Some(hi),
                    line: Some(li),
                    old_no,
                    new_no,
                    spans: vec![],
                });
            }
        }
        rows
    }

    /// Build a patch containing only `selected` (hunk, line) changes.
    ///
    /// `reverse` = the patch will be applied with `--reverse` (unstage / discard):
    /// unselected `+` lines become context and unselected `-` lines are dropped.
    /// Otherwise unselected `-` lines become context and unselected `+` lines are dropped.
    pub fn build_patch(
        &self,
        selected: &BTreeSet<(usize, usize)>,
        reverse: bool,
    ) -> Option<String> {
        let mut out = String::new();
        let mut any = false;
        for (hi, h) in self.hunks.iter().enumerate() {
            let mut body: Vec<String> = Vec::new();
            let mut changed = false;
            let (mut old_n, mut new_n) = (0usize, 0usize);
            let mut last_kept = false;
            for (li, l) in h.lines.iter().enumerate() {
                let sel = selected.contains(&(hi, li));
                let first = l.chars().next().unwrap_or(' ');
                match first {
                    '+' | '-' if sel => {
                        changed = true;
                        if first == '+' {
                            new_n += 1
                        } else {
                            old_n += 1
                        }
                        body.push(l.clone());
                        last_kept = true;
                    }
                    '+' | '-' => {
                        let to_context = (first == '+') == reverse;
                        if to_context {
                            body.push(format!(" {}", &l[1..]));
                            old_n += 1;
                            new_n += 1;
                            last_kept = true;
                        } else {
                            last_kept = false;
                        }
                    }
                    '\\' => {
                        if last_kept {
                            body.push(l.clone());
                        }
                    }
                    _ => {
                        body.push(l.clone());
                        old_n += 1;
                        new_n += 1;
                        last_kept = true;
                    }
                }
            }
            if !changed {
                continue;
            }
            any = true;
            out.push_str(&format!(
                "@@ -{},{} +{},{} @@\n",
                h.old_start, old_n, h.new_start, new_n
            ));
            for b in body {
                out.push_str(&b);
                out.push('\n');
            }
        }
        if !any {
            return None;
        }
        let mut header = String::new();
        for l in &self.header {
            header.push_str(l);
            header.push('\n');
        }
        Some(header + &out)
    }

    /// All change lines of hunk `hi`.
    pub fn hunk_lines(&self, hi: usize) -> BTreeSet<(usize, usize)> {
        let mut s = BTreeSet::new();
        if let Some(h) = self.hunks.get(hi) {
            for (li, l) in h.lines.iter().enumerate() {
                if l.starts_with('+') || l.starts_with('-') {
                    s.insert((hi, li));
                }
            }
        }
        s
    }
}

/// Consume a block of `-` lines followed by `+` lines starting at `*i`.
fn take_block(rows: &[Row], i: &mut usize) -> (Vec<usize>, Vec<usize>) {
    let mut dels = vec![];
    let mut adds = vec![];
    while *i < rows.len() && matches!(rows[*i].kind, RowKind::Del | RowKind::NoNewline) {
        if rows[*i].kind == RowKind::Del {
            dels.push(*i);
        }
        *i += 1;
    }
    while *i < rows.len() && matches!(rows[*i].kind, RowKind::Add | RowKind::NoNewline) {
        if rows[*i].kind == RowKind::Add {
            adds.push(*i);
        }
        *i += 1;
    }
    (dels, adds)
}

/// Side-by-side pairing: (left row idx, right row idx) per visual row.
pub fn side_by_side(rows: &[Row]) -> Vec<(Option<usize>, Option<usize>)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < rows.len() {
        match rows[i].kind {
            RowKind::Del | RowKind::Add => {
                let (dels, adds) = take_block(rows, &mut i);
                for k in 0..dels.len().max(adds.len()) {
                    out.push((dels.get(k).copied(), adds.get(k).copied()));
                }
            }
            RowKind::NoNewline => i += 1,
            _ => {
                out.push((Some(i), Some(i)));
                i += 1;
            }
        }
    }
    out
}

/// Blocks of consecutive removed / added rows (row indices).
pub fn change_blocks(rows: &[Row]) -> Vec<(Vec<usize>, Vec<usize>)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < rows.len() {
        if matches!(rows[i].kind, RowKind::Del | RowKind::Add) {
            out.push(take_block(rows, &mut i));
        } else {
            i += 1;
        }
    }
    out
}

/// Split a line into word / whitespace / punctuation tokens with their byte offset.
fn tokens(s: &str) -> Vec<(usize, &str)> {
    fn class(c: char) -> u8 {
        if c.is_alphanumeric() || c == '_' {
            0
        } else if c.is_whitespace() {
            1
        } else {
            2
        }
    }
    let mut out = Vec::new();
    let mut start = 0;
    let mut prev: Option<u8> = None;
    for (i, c) in s.char_indices() {
        let k = class(c);
        if prev.is_some() && (prev != Some(k) || k == 2) {
            out.push((start, &s[start..i]));
            start = i;
        }
        prev = Some(k);
    }
    if start < s.len() {
        out.push((start, &s[start..]));
    }
    out
}

/// Changed byte ranges of `old` and `new` at word granularity.
/// Empty when the lines have too little in common to be worth highlighting.
pub fn word_ranges(old: &str, new: &str) -> (Vec<Range<usize>>, Vec<Range<usize>>) {
    let (a, b) = (tokens(old), tokens(new));
    let (ta, tb): (Vec<&str>, Vec<&str>) = (
        a.iter().map(|t| t.1).collect(),
        b.iter().map(|t| t.1).collect(),
    );
    let span = |t: &[(usize, &str)], r: Range<usize>| -> Option<Range<usize>> {
        let first = t.get(r.start)?;
        let last = t.get(r.end.checked_sub(1)?)?;
        Some(first.0..last.0 + last.1.len())
    };
    let push = |v: &mut Vec<Range<usize>>, r: Range<usize>| match v.last_mut() {
        Some(l) if l.end == r.start => l.end = r.end,
        _ => v.push(r),
    };
    let (mut ro, mut rn) = (vec![], vec![]);
    let mut same = 0;
    for op in capture_diff_slices(Algorithm::Myers, &ta, &tb) {
        let (tag, o, n) = op.as_tag_tuple();
        if tag == DiffTag::Equal {
            same += ta[o].iter().map(|t| t.trim().len()).sum::<usize>();
            continue;
        }
        if let Some(r) = span(&a, o) {
            push(&mut ro, r);
        }
        if let Some(r) = span(&b, n) {
            push(&mut rn, r);
        }
    }
    let total = old.trim().len().max(new.trim().len());
    if total == 0 || same * 10 < total * 4 {
        return (vec![], vec![]);
    }
    (ro, rn)
}

/// A conflict region in a file with conflict markers (line indices).
#[derive(Clone, Debug)]
pub struct Conflict {
    pub start: usize,
    pub mid: usize,
    pub end: usize,
    /// `|||||||` marker for diff3 style
    pub base: Option<usize>,
}

pub fn find_conflicts(lines: &[String]) -> Vec<Conflict> {
    let mut out = Vec::new();
    let mut start = None;
    let mut base = None;
    let mut mid = None;
    for (i, l) in lines.iter().enumerate() {
        if l.starts_with("<<<<<<<") {
            start = Some(i);
            base = None;
            mid = None;
        } else if l.starts_with("|||||||") && start.is_some() {
            base = Some(i);
        } else if l.starts_with("=======") && start.is_some() {
            mid = Some(i);
        } else if l.starts_with(">>>>>>>") {
            if let (Some(s), Some(m)) = (start, mid) {
                out.push(Conflict {
                    start: s,
                    mid: m,
                    end: i,
                    base,
                });
            }
            start = None;
        }
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    Ours,
    Theirs,
    Both,
}

pub fn resolve(lines: &[String], c: &Conflict, pick: Pick) -> Vec<String> {
    let ours_end = c.base.unwrap_or(c.mid);
    let ours = &lines[c.start + 1..ours_end];
    let theirs = &lines[c.mid + 1..c.end];
    let mut out: Vec<String> = lines[..c.start].to_vec();
    match pick {
        Pick::Ours => out.extend_from_slice(ours),
        Pick::Theirs => out.extend_from_slice(theirs),
        Pick::Both => {
            out.extend_from_slice(ours);
            out.extend_from_slice(theirs);
        }
    }
    out.extend_from_slice(&lines[c.end + 1..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const D: &str = "diff --git a/f b/f\nindex 1..2 100644\n--- a/f\n+++ b/f\n@@ -1,3 +1,3 @@\n a\n-b\n+B\n c\n";

    #[test]
    fn parse_and_patch() {
        let fd = parse(D);
        assert_eq!(fd.hunks.len(), 1);
        assert_eq!(fd.hunks[0].lines.len(), 4);
        // stage only the addition
        let sel: BTreeSet<_> = [(0, 2)].into_iter().collect();
        let p = fd.build_patch(&sel, false).unwrap();
        assert!(p.contains("@@ -1,3 +1,4 @@\n a\n b\n+B\n c\n"), "{p}");
        // unstage only the deletion (reverse)
        let sel: BTreeSet<_> = [(0, 1)].into_iter().collect();
        let p = fd.build_patch(&sel, true).unwrap();
        assert!(p.contains("@@ -1,4 +1,3 @@\n a\n-b\n B\n c\n"), "{p}");
        assert!(fd.build_patch(&BTreeSet::new(), false).is_none());
    }

    #[test]
    fn conflicts() {
        let l: Vec<String> = "x\n<<<<<<< HEAD\nours\n=======\ntheirs\n>>>>>>> b\ny"
            .lines()
            .map(String::from)
            .collect();
        let c = find_conflicts(&l);
        assert_eq!(c.len(), 1);
        assert_eq!(resolve(&l, &c[0], Pick::Theirs), vec!["x", "theirs", "y"]);
        assert_eq!(
            resolve(&l, &c[0], Pick::Both),
            vec!["x", "ours", "theirs", "y"]
        );
    }

    #[test]
    fn sbs() {
        let rows = parse(D).rows();
        let s = side_by_side(&rows);
        assert!(s.iter().any(|(l, r)| l.is_some() && r.is_some() && l != r));
    }

    #[test]
    fn line_numbers() {
        let rows = parse(D).rows();
        let nos: Vec<_> = rows.iter().map(|r| (r.old_no, r.new_no)).collect();
        assert_eq!(
            &nos[5..],
            &[
                (Some(1), Some(1)),
                (Some(2), None),
                (None, Some(2)),
                (Some(3), Some(3))
            ]
        );
    }

    #[test]
    fn blocks() {
        let d = "@@ -1,3 +1,2 @@\n-a\n-b\n+c\n x\n";
        let rows = parse(d).rows();
        assert_eq!(change_blocks(&rows), vec![(vec![1, 2], vec![3])]);
    }

    #[test]
    fn words() {
        let (o, n) = word_ranges("let x = foo(a);", "let x = foo(bar);");
        assert_eq!(o, vec![12..13]);
        assert_eq!(n, vec![12..15]);
        // unrelated lines: no word emphasis
        assert_eq!(word_ranges("abc def", "xyz uvw"), (vec![], vec![]));
        assert_eq!(word_ranges("", "x"), (vec![], vec![]));
    }
}
