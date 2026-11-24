//! Commit graph lane computation (one column per lane).

use git2::Oid;

/// Returns one graph prefix string per commit (commits in topological order).
pub fn compute(commits: &[(Oid, Vec<Oid>)]) -> Vec<String> {
    let mut lanes: Vec<Option<Oid>> = Vec::new();
    let mut out = Vec::with_capacity(commits.len());
    for (id, parents) in commits {
        let col = match lanes.iter().position(|l| *l == Some(*id)) {
            Some(c) => c,
            None => match lanes.iter().position(|l| l.is_none()) {
                Some(c) => c,
                None => {
                    lanes.push(None);
                    lanes.len() - 1
                }
            },
        };
        // other lanes converging into this commit
        let merging: Vec<usize> = lanes
            .iter()
            .enumerate()
            .filter(|(i, l)| *i != col && **l == Some(*id))
            .map(|(i, _)| i)
            .collect();
        let mut s = String::new();
        for (i, l) in lanes.iter().enumerate() {
            let ch = if i == col {
                if parents.len() > 1 { '◎' } else { '●' }
            } else if merging.contains(&i) {
                '┘'
            } else if l.is_some() {
                '│'
            } else {
                ' '
            };
            s.push(ch);
            s.push(' ');
        }
        if col == lanes.len() {
            s.push_str("● ");
        }
        for i in &merging {
            lanes[*i] = None;
        }
        // first parent continues in this lane
        lanes[col] = parents.first().copied();
        // extra parents: reuse existing lane or open a new one
        let mut opened = false;
        for p in parents.iter().skip(1) {
            if !lanes.contains(&Some(*p)) {
                match lanes.iter().position(|l| l.is_none()) {
                    Some(c) => lanes[c] = Some(*p),
                    None => lanes.push(Some(*p)),
                }
                opened = true;
            }
        }
        if opened && !s.ends_with("┐ ") {
            // mark branch-off on the row for readability
            s = s.trim_end().to_string();
            s.push('┐');
            s.push(' ');
        }
        while lanes.last() == Some(&None) {
            lanes.pop();
        }
        out.push(s);
    }
    // pad to equal width
    let w = out.iter().map(|s| s.chars().count()).max().unwrap_or(0);
    out.into_iter()
        .map(|s| {
            let n = s.chars().count();
            s + &" ".repeat(w - n)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn o(n: u8) -> Oid {
        Oid::from_bytes(&[n; 20]).unwrap()
    }
    #[test]
    fn linear_and_merge() {
        // 4 merges 3 and 2; 3 -> 1; 2 -> 1
        let c = vec![
            (o(4), vec![o(3), o(2)]),
            (o(3), vec![o(1)]),
            (o(2), vec![o(1)]),
            (o(1), vec![]),
        ];
        let g = compute(&c);
        assert_eq!(g.len(), 4);
        assert!(g[0].starts_with('◎'));
        assert!(g[1].starts_with('●'));
        assert!(g[2].contains('●') && g[2].starts_with('│'));
        assert!(g[3].starts_with('●'));
    }
}
