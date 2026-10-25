//! Line diff of two function bodies (LCS; functions are small).

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum Op {
    #[serde(rename = " ")]
    Same,
    #[serde(rename = "-")]
    Del,
    #[serde(rename = "+")]
    Add,
}

impl Op {
    pub fn mark(self) -> char {
        match self {
            Op::Same => ' ',
            Op::Del => '-',
            Op::Add => '+',
        }
    }
}

/// A diff line: op, old line number, new line number, text.
pub type Line = (Op, Option<u32>, Option<u32>, String);

/// Line diff of `a` (starting at line `a0`) and `b` (starting at `b0`). Above 2000×2000 lines it
/// degrades to "all removed, all added".
pub fn diff(a: &str, a0: u32, b: &str, b0: u32) -> Vec<Line> {
    let a: Vec<&str> = a.lines().collect();
    let b: Vec<&str> = b.lines().collect();
    let (n, m) = (a.len(), b.len());
    let mut out = vec![];
    if n * m > 4_000_000 {
        out.extend(
            a.iter()
                .enumerate()
                .map(|(i, l)| (Op::Del, Some(a0 + i as u32), None, l.to_string())),
        );
        out.extend(
            b.iter()
                .enumerate()
                .map(|(j, l)| (Op::Add, None, Some(b0 + j as u32), l.to_string())),
        );
        return out;
    }
    // lcs[i][j] = LCS length of a[i..] and b[j..]
    let mut lcs = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if a[i] == b[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    while i < n || j < m {
        if i < n && j < m && a[i] == b[j] {
            out.push((
                Op::Same,
                Some(a0 + i as u32),
                Some(b0 + j as u32),
                b[j].to_string(),
            ));
            i += 1;
            j += 1;
        } else if j < m && (i == n || lcs[i][j + 1] > lcs[i + 1][j]) {
            out.push((Op::Add, None, Some(b0 + j as u32), b[j].to_string()));
            j += 1;
        } else {
            out.push((Op::Del, Some(a0 + i as u32), None, a[i].to_string()));
            i += 1;
        }
    }
    out
}

/// Keeps changed lines plus `context` lines around them; `None` marks a skipped run.
pub fn hunks(lines: &[Line], context: usize) -> Vec<Option<&Line>> {
    let keep: Vec<bool> = (0..lines.len())
        .map(|i| {
            let lo = i.saturating_sub(context);
            let hi = (i + context + 1).min(lines.len());
            lines[lo..hi].iter().any(|l| l.0 != Op::Same)
        })
        .collect();
    let mut out = vec![];
    let mut skipped = false;
    for (l, k) in lines.iter().zip(keep) {
        if k {
            out.push(Some(l));
            skipped = false;
        } else if !skipped {
            out.push(None);
            skipped = true;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diffs_lines() {
        let d = diff("a\nb\nc\nd", 10, "a\nB\nc\nd\ne", 20);
        let ops: String = d.iter().map(|l| l.0.mark()).collect();
        // Deletions come before the additions that replace them, as in git.
        assert_eq!(ops, " -+  +");
        assert_eq!(d[1], (Op::Del, Some(11), None, "b".into()));
        assert_eq!(d[2], (Op::Add, None, Some(21), "B".into()));
        let h = hunks(&d, 0);
        assert!(h[0].is_none());
        assert_eq!(h.iter().filter(|x| x.is_some()).count(), 3);
    }
}
