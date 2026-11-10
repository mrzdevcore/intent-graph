//! Putting nodes into blocks and taking them out, and the layout of what an edit puts in;
//! nodes it did not touch keep their text.

use super::{Loc, Spec, box_id, sibling_id};
use crate::map::spec::{self, HEADER, OLD_HEADER};
use kdl::{KdlDocument, KdlDocumentFormat, KdlNode, KdlNodeFormat};

/// Lays out at `indent` every node in `block` that has no layout yet (built in code), and its
/// children a level deeper: the first child's `leading` starts with the line break after `{`,
/// each node ends its line. A first child that has a layout gets that line break if it lacks it.
pub(crate) fn tidy(block: &mut KdlDocument, indent: &str) {
    for (i, n) in block.nodes_mut().iter_mut().enumerate() {
        let first = if i == 0 { "\n" } else { "" };
        if laid_out(n) {
            let f = n.format_mut().expect("laid out");
            if i == 0 {
                let at = head_len(&f.leading);
                if !f.leading[at..].starts_with('\n') {
                    f.leading.insert(at, '\n');
                }
            }
            continue;
        }
        n.set_format(KdlNodeFormat {
            leading: format!("{first}{indent}"),
            before_children: if n.children().is_some() { " " } else { "" }.into(),
            terminator: "\n".into(),
            ..Default::default()
        });
        if let Some(c) = n.children_mut() {
            c.set_format(KdlDocumentFormat {
                leading: String::new(),
                trailing: indent.into(),
            });
            tidy(c, &format!("{indent}    "));
        }
    }
}

/// Whether `n` was read or laid out; a node built in code (`KdlNode::new`) ends no line.
pub(crate) fn laid_out(n: &KdlNode) -> bool {
    n.format()
        .is_some_and(|f| !f.terminator.is_empty() || !f.leading.is_empty())
}

/// The indent of `n`'s own line.
pub(crate) fn indent_of(n: &KdlNode) -> String {
    let lead = n.format().map(|f| f.leading.as_str()).unwrap_or_default();
    lead.rsplit('\n').next().unwrap_or_default().into()
}

/// `n` (with its comments and children) re-indented so that its line starts at `to`.
pub(crate) fn reindent(n: KdlNode, to: &str) -> KdlNode {
    let from = indent_of(&n);
    if from == to {
        return n;
    }
    // ponytail: a multi-line raw string inside the subtree is re-indented too; the writer
    // never makes one.
    let text: String = n
        .to_string()
        .split_inclusive('\n')
        .map(|l| match l.strip_prefix(from.as_str()) {
            Some(rest) if !l.trim().is_empty() => format!("{to}{rest}"),
            _ => l.to_string(),
        })
        .collect();
    match text.parse::<KdlDocument>() {
        Ok(mut d) if d.nodes().len() == 1 => d.nodes_mut().remove(0),
        _ => n,
    }
}

/// Keeps the lines after a file's top node (`tail`) with the node as it leaves the file: at
/// the end of its block, at the node's own indent (the block's own comments sit deeper), so
/// that `split_tail` finds them again. kdl cannot read a comment after a `}` that has
/// siblings after it, nor tell it from the next node's own comments.
fn carry_tail(n: &mut KdlNode, tail: Option<String>) {
    let Some(mut tail) = tail.filter(|t| !t.trim().is_empty()) else {
        return;
    };
    if !tail.ends_with('\n') {
        tail.push('\n');
    }
    if n.children().is_none() {
        if let Some(f) = n.format_mut() {
            f.before_children = " ".into();
        }
        n.set_children(KdlDocument::new());
    }
    let block = n.children_mut().as_mut().expect("just made");
    let mut f = block.format().cloned().unwrap_or_default();
    // a top node's `}` sits at column 0: nothing after the block's last line break
    f.trailing.push_str(&tail);
    block.set_format(f);
}

/// The tail `carry_tail` put in the block of `n`, a node at column 0, taken back out: the
/// comment lines at column 0 at the block's end, with the blank lines just above them.
fn split_tail(n: &mut KdlNode) -> String {
    let Some(block) = n.children_mut().as_mut() else {
        return String::new();
    };
    let Some(mut f) = block.format().cloned() else {
        return String::new();
    };
    let lines: Vec<&str> = f.trailing.split_inclusive('\n').collect();
    let Some(mut i) = lines
        .iter()
        .position(|l| !l.is_empty() && !l.starts_with(char::is_whitespace))
    else {
        return String::new();
    };
    while i > 0 && lines[i - 1].trim().is_empty() && lines[i - 1].ends_with('\n') {
        i -= 1;
    }
    let tail = lines[i..].concat();
    f.trailing = lines[..i].concat();
    block.set_format(f);
    tail
}

/// Length of the comment a first child's `leading` carries from the `{` line.
fn head_len(leading: &str) -> usize {
    match leading.find('\n') {
        Some(k) if !leading[..k].trim().is_empty() => k,
        _ => 0,
    }
}

/// `n`'s leading without the line break that ends the `{` line (a first child has it); returns
/// the comment that was on the `{` line.
fn unfirst(n: &mut KdlNode) -> String {
    let Some(f) = n.format_mut() else {
        return String::new();
    };
    let head: String = f.leading.drain(..head_len(&f.leading)).collect();
    if f.leading.starts_with('\n') {
        f.leading.remove(0);
    }
    head
}

impl Spec {
    /// The block to put a node into, and where: before the sibling `before` names, or last.
    pub(super) fn place(&self, parent: &str, before: Option<&str>) -> Result<(Loc, usize), String> {
        let loc = self.must(parent)?;
        let nodes = (self.node(&loc).children())
            .map(|c| c.nodes())
            .unwrap_or_default();
        let i = match before {
            None => nodes.len(),
            Some(b) => (nodes
                .iter()
                .position(|n| sibling_id(parent, n).as_deref() == Some(b)))
            .ok_or_else(|| format!("`{parent}` has no `{b}` to go before"))?,
        };
        Ok((loc, i))
    }

    /// Puts `n` at index `i` of the block of the box at `loc`, at that block's indent.
    pub(super) fn put(&mut self, loc: &Loc, i: usize, n: KdlNode) {
        let node = self.node_mut(loc);
        let ind = indent_of(node);
        let mut head = String::new();
        if node.children().is_none() {
            if let Some(f) = node.format_mut() {
                f.before_children = " ".into();
                if f.terminator.trim_start().starts_with("//") {
                    let end = f.terminator.trim_end_matches('\n').to_string();
                    head = std::mem::take(&mut f.before_terminator) + &end;
                    f.terminator = "\n".into();
                }
            }
            let mut d = KdlDocument::new();
            d.set_format(KdlDocumentFormat {
                leading: String::new(),
                trailing: ind.clone(),
            });
            node.set_children(d);
        }
        let block = node.children_mut().as_mut().expect("just made");
        let inner = (block.nodes().first())
            .map(indent_of)
            .unwrap_or(format!("{ind}    "));
        let laid_out = laid_out(&n);
        let mut n = if laid_out { reindent(n, &inner) } else { n };
        if i == 0 {
            if let Some(old) = block.nodes_mut().first_mut() {
                head = unfirst(old);
            }
            if let Some(f) = n.format_mut().filter(|_| laid_out) {
                f.leading.insert(0, '\n');
            }
        }
        block.nodes_mut().insert(i, n);
        tidy(block, &inner);
        if let Some(f) = block.nodes_mut()[0].format_mut() {
            f.leading.insert_str(0, &head);
        }
    }

    /// Takes the node at `loc` out, with the comments above it and without the line break of
    /// a `{` line; and the id of the sibling it was before. A block left empty goes.
    pub(super) fn take(&mut self, loc: &Loc) -> (KdlNode, Option<String>) {
        if self.own_file(loc) {
            let mut d = self.docs.remove(&loc.0).unwrap_or_default();
            let mut n = d.nodes_mut().remove(0);
            if let Some(f) = n.format_mut() {
                let l = &f.leading;
                let l = (l.strip_prefix(HEADER).or(l.strip_prefix(OLD_HEADER))).unwrap_or(l);
                f.leading = l.into();
            }
            carry_tail(&mut n, d.format().map(|f| f.trailing.clone()));
            return (n, None);
        }
        let parent = self.parent(loc).unwrap_or_default();
        let (p, at) = loc;
        let (up, i) = ((p.clone(), at[..at.len() - 1].to_vec()), at[at.len() - 1]);
        let pn = self.node_mut(&up);
        let block = pn.children_mut().as_mut().expect("a located node's block");
        let mut n = block.nodes_mut().remove(i);
        let before = block.nodes()[i..]
            .iter()
            .find_map(|s| sibling_id(&parent, s));
        let mut head = String::new();
        if i == 0 {
            head = unfirst(&mut n);
            if let Some(f) = block.nodes_mut().first_mut().and_then(|f| f.format_mut()) {
                f.leading.insert(0, '\n');
                f.leading.insert_str(0, &head);
                head.clear();
            }
        }
        let blank = block.format().is_none_or(|f| f.trailing.trim().is_empty());
        if block.nodes().is_empty() && blank {
            pn.clear_children();
            if let Some(f) = pn.format_mut() {
                f.before_children.clear();
                if let Some(k) = head.find("//") {
                    f.before_terminator = head[..k].into();
                    f.terminator = format!("{}\n", &head[k..]);
                }
            }
        }
        (n, before)
    }

    /// Keeps a top-level container where the writer puts it: in `system.kdl` while it has no
    /// child box, in `ig/<id>.kdl` once it has one.
    pub(super) fn settle(&mut self, id: &str) -> Result<(), String> {
        let Some(loc) = self.find(id) else {
            return Ok(());
        };
        if self.parent(&loc).as_deref() != Some(self.sys.as_str()) {
            return Ok(());
        }
        let full = !self.kids(id).is_empty();
        if full && !self.own_file(&loc) {
            let path = spec::path(id);
            if self.docs.contains_key(&path) {
                return Err(format!("{path} already exists"));
            }
            let (n, _) = self.take(&loc);
            let mut n = reindent(n, "");
            if let Some(f) = n.format_mut() {
                f.leading.insert_str(0, HEADER);
            }
            let mut d = KdlDocument::new();
            d.set_format(KdlDocumentFormat {
                leading: String::new(),
                trailing: split_tail(&mut n),
            });
            d.nodes_mut().push(n);
            self.docs.insert(path, d);
        } else if !full && self.own_file(&loc) {
            let (n, _) = self.take(&loc);
            // ponytail: back among its siblings in id order, as the writer puts them; a
            // hand-ordered system block gets it in that place, not the one it left.
            let sys = self.sys.clone();
            let (at, _) = self.place(&sys, None)?;
            let nodes = self
                .node(&at)
                .children()
                .map(|c| c.nodes())
                .unwrap_or_default();
            let i = (nodes.iter().position(|s| box_id(s).is_some_and(|b| b > id)))
                .unwrap_or(nodes.len());
            self.put(&at, i, n);
        }
        Ok(())
    }
}
