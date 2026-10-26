//! The architecture board's keys, delete, copy and paste (its gestures: `input.rs`).

use crate::engine::{Engine, Out};
use crate::input::{Gesture, Input, Tool, mods::*};
use crate::{Step, move_ops};
use ig_design::{Board, Op, is_element_id};

impl Engine {
    pub(crate) fn key(&mut self, key: &str, mods: u8) -> Out {
        let (cmd, shift) = (mods & (CTRL | META) != 0, mods & SHIFT != 0);
        let key = key.to_ascii_lowercase();
        let tool = |t| Input::Tool(t);
        let nudge = if shift { 40.0 } else { 8.0 };
        let arrow = match key.as_str() {
            "arrowleft" => Some((-nudge, 0.0)),
            "arrowright" => Some((nudge, 0.0)),
            "arrowup" => Some((0.0, -nudge)),
            "arrowdown" => Some((0.0, nudge)),
            _ => None,
        };
        if let (Some((dx, dy)), false) = (arrow, cmd) {
            let ops = move_ops(
                self.board(),
                &self.selected_elements(),
                dx,
                dy,
                &self.ctx.lanes,
            );
            return self.ops(&ops, Step::User);
        }
        match (cmd, key.as_str()) {
            (true, "z") if shift => self.redo(),
            (true, "z") => self.undo(),
            (true, "c") => {
                self.copy();
                self.out()
            }
            (true, "v") => self.paste(),
            (true, "d") => {
                self.copy();
                self.paste()
            }
            (true, "a") => {
                self.selection = (self.board().design.elements.iter())
                    .filter(|e| self.shown(&e.layer))
                    .map(|e| e.id.clone())
                    .collect();
                self.redraw()
            }
            (true, "0") => {
                self.camera.zoom = 1.0;
                self.redraw()
            }
            (false, "v") => self.input(tool(Tool::Select)),
            (false, "m") => self.input(tool(Tool::Module)),
            (false, "f") => self.input(tool(Tool::Function)),
            (false, "r") => self.input(tool(Tool::Route)),
            (false, "e") => self.input(tool(Tool::System)),
            (false, "n") => self.input(tool(Tool::Note)),
            (false, "g") => self.input(tool(Tool::Frame)),
            (false, "l") => self.input(tool(Tool::Link)),
            (false, "delete" | "backspace") => self.remove_selection(),
            (false, "enter") => {
                let mut o = self.out();
                o.edit = (self.selected_elements().first()).and_then(|id| self.edit(id));
                o
            }
            (false, "escape") if shift => {
                self.dismiss(None);
                self.redraw()
            }
            (false, "escape") => {
                if self.gesture != Gesture::Idle {
                    self.gesture = Gesture::Idle;
                } else if self.tool != Tool::Select {
                    self.tool = Tool::Select;
                } else {
                    self.selection.clear();
                }
                self.redraw()
            }
            (false, "!") | (false, "1") if shift => {
                self.fit(false);
                self.redraw()
            }
            (false, "@") | (false, "2") if shift => {
                self.fit(true);
                self.redraw()
            }
            _ => self.out(), // Cmd/Ctrl+Enter (Plan) is the host's
        }
    }

    fn remove_selection(&mut self) -> Out {
        let d = &self.board().design;
        let ops: Vec<Op> = (self.selection.iter())
            .filter_map(|s| {
                if d.elements.iter().any(|e| &e.id == s) {
                    Some(Op::Remove { id: s.clone() })
                } else if d.links.iter().any(|l| &l.key() == s) {
                    Some(Op::Unlink { key: s.clone() })
                } else {
                    None
                }
            })
            .collect();
        // a link that a removed element already took with it is not unlinked twice
        let gone: Vec<&String> = (ops.iter())
            .filter_map(|o| match o {
                Op::Remove { id } => Some(id),
                _ => None,
            })
            .collect();
        let ops: Vec<Op> = (ops.iter())
            .filter(|o| match o {
                Op::Unlink { key } => !gone.iter().any(|g| {
                    let mut p = key.split('|');
                    let (f, _, t) = (p.next(), p.next(), p.next());
                    f == Some(g.as_str()) || t == Some(g.as_str())
                }),
                _ => true,
            })
            .cloned()
            .collect();
        self.ops(&ops, Step::User)
    }

    fn copy(&mut self) {
        let ids = self.selected_elements();
        let d = &self.board().design;
        let inner = |s: &String| ids.contains(s) || !is_element_id(s);
        let els = d.elements.iter().filter(|e| ids.contains(&e.id)).cloned();
        let links = d.links.iter().filter(|l| inner(&l.from) && inner(&l.to));
        self.clipboard = (els.collect(), links.cloned().collect(), 0);
    }

    /// A note target or frame outside the paste lapses unless the board still has it.
    fn paste(&mut self) -> Out {
        let (els, links, n) = &mut self.clipboard;
        if els.is_empty() {
            return self.out();
        }
        *n += 1;
        let (els, links, off) = (els.clone(), links.clone(), *n * 16);
        let b: &Board = self.board();
        let keep = |s: &Option<String>| {
            s.clone().filter(|s| {
                !is_element_id(s)
                    || els.iter().any(|e| &e.id == s)
                    || b.design.elements.iter().any(|e| &e.id == s)
            })
        };
        let elements = (els.iter())
            .map(|e| {
                let mut e = e.clone();
                (e.on, e.frame) = (keep(&e.on), keep(&e.frame));
                e
            })
            .collect();
        let before: Vec<String> = b.design.elements.iter().map(|e| e.id.clone()).collect();
        let op = Op::Paste {
            elements,
            links,
            dx: off,
            dy: off,
        };
        let mut o = self.ops(&[op], Step::User);
        if o.error.is_none() {
            self.selection = (self.board().design.elements.iter())
                .filter(|e| !before.contains(&e.id))
                .map(|e| e.id.clone())
                .collect();
            o.selection = self.selection.clone();
        }
        o
    }
}
