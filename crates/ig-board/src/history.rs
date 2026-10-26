//! Undo history with agent steps: changes of one agent turn undo together.

use ig_design::Board;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    User,
    /// An agent turn's id.
    Agent(String),
    External,
}

/// Undo history: `limit` undo steps above the initial board.
// ponytail: whole-board snapshots; switch to patches past ~2k elements
pub struct History {
    entries: Vec<(Board, Step)>,
    at: usize,
    limit: usize,
    /// The agent turn the top entry may still absorb; any other push, undo or redo closes it.
    open: Option<String>,
}

impl History {
    pub fn new(b: Board, limit: usize) -> Self {
        History {
            entries: vec![(b, Step::External)],
            at: 0,
            limit,
            open: None,
        }
    }

    /// Truncates redo.
    pub fn push(&mut self, b: Board, step: Step) {
        self.entries.truncate(self.at + 1);
        self.open = match &step {
            Step::Agent(t) => Some(t.clone()),
            _ => None,
        };
        self.entries.push((b, step));
        if self.entries.len() > self.limit + 1 {
            self.entries.remove(0);
        }
        self.at = self.entries.len() - 1;
    }

    /// Joins the top step only while it is this turn's and still current; else a new step.
    pub fn agent(&mut self, b: Board, turn: &str) {
        if self.open.as_deref() == Some(turn) && self.at + 1 == self.entries.len() {
            self.entries[self.at].0 = b;
        } else {
            self.push(b, Step::Agent(turn.into()));
        }
    }

    pub fn undo(&mut self) -> Option<&Board> {
        self.open = None;
        if self.at == 0 {
            return None;
        }
        self.at -= 1;
        Some(&self.entries[self.at].0)
    }

    pub fn redo(&mut self) -> Option<&Board> {
        self.open = None;
        if self.at + 1 >= self.entries.len() {
            return None;
        }
        self.at += 1;
        Some(&self.entries[self.at].0)
    }

    /// The step that made the current board.
    pub fn top_step(&self) -> &Step {
        &self.entries[self.at].1
    }

    pub fn current(&self) -> &Board {
        &self.entries[self.at].0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ig_design::{Scope, empty};

    fn b(n: i32) -> Board {
        Board {
            design: empty(&n.to_string(), Scope::Layer { name: "x".into() }),
        }
    }
    fn title(b: Option<&Board>) -> Option<String> {
        b.map(|b| b.design.title.clone())
    }
    fn steps(h: &mut History) -> usize {
        let mut n = 0;
        while h.undo().is_some() {
            n += 1;
        }
        n
    }

    #[test]
    fn undo_redo_truncation() {
        let mut h = History::new(b(0), 200);
        for i in 1..=3 {
            h.push(b(i), Step::User);
        }
        assert_eq!(title(h.undo()), Some("2".into()));
        h.push(b(9), Step::User);
        assert_eq!(title(h.redo()), None);
        assert_eq!(title(h.undo()), Some("2".into()));
    }

    #[test]
    fn caps_at_200_steps() {
        let mut h = History::new(b(0), 200);
        for i in 1..=250 {
            h.push(b(i), Step::User);
        }
        assert_eq!(steps(&mut h), 200);
    }

    #[test]
    fn push_plus_two_agent_changes_of_one_turn_undo_in_one_step() {
        let mut h = History::new(b(0), 200);
        h.push(b(1), Step::Agent("t1".into()));
        h.agent(b(2), "t1");
        h.agent(b(3), "t1");
        assert_eq!(title(h.undo()), Some("0".into()));
        assert_eq!(title(h.redo()), Some("3".into()));
    }

    #[test]
    fn agent_joins_only_the_last_agent_step() {
        let mut h = History::new(b(0), 200);
        h.agent(b(1), "t1");
        h.push(b(2), Step::User);
        h.agent(b(3), "t1");
        assert_eq!(steps(&mut h), 3);
    }

    #[test]
    fn agent_truncates_redo() {
        let mut h = History::new(b(0), 200);
        h.agent(b(1), "t1");
        assert_eq!(title(h.undo()), Some("0".into()));
        h.agent(b(2), "t1");
        assert_eq!(title(h.redo()), None);
        assert_eq!(title(h.undo()), Some("0".into()), "the undone state stays");
        assert_eq!(h.top_step(), &Step::External);
    }

    #[test]
    fn a_different_turn_starts_a_new_step() {
        let mut h = History::new(b(0), 200);
        h.agent(b(1), "t1");
        h.agent(b(2), "t2");
        assert_eq!(h.top_step(), &Step::Agent("t2".into()));
        assert_eq!(title(h.undo()), Some("1".into()));
    }

    #[test]
    fn redo_does_not_reopen_the_agent_step() {
        let mut h = History::new(b(0), 200);
        h.agent(b(1), "t1");
        h.undo();
        h.redo();
        h.agent(b(2), "t1");
        assert_eq!(steps(&mut h), 2);
    }
}
