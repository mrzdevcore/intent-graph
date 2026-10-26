//! Gestures, a small state machine (idle, placing, dragging, linking, marquee, panning), and
//! the keys of the architecture board.

use crate::engine::{EditRequest, Engine, Out};
use crate::hit::Target;
use crate::snap::{default_size, rect_of, takes_layer};
use crate::{RowKind, Step, default_link_kind, move_ops, row_at, snap};
use ig_design::{Board, ElementKind, LinkKind, Op, is_element_id, new_id};

/// Modifier bits of `Input`.
pub mod mods {
    pub const SHIFT: u8 = 1;
    pub const ALT: u8 = 2;
    pub const CTRL: u8 = 4;
    pub const META: u8 = 8;
    /// The second press of a double-click (`PointerEvent.detail >= 2`).
    pub const DOUBLE: u8 = 16;
}
use mods::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Select,
    Module,
    Function,
    Route,
    System,
    Note,
    Frame,
    Link,
}

/// Pointer coordinates are screen (canvas CSS) px; keys are `KeyboardEvent.key`.
#[derive(Clone, Debug, PartialEq)]
pub enum Input {
    Down {
        x: f32,
        y: f32,
        button: u8,
        mods: u8,
    },
    Move {
        x: f32,
        y: f32,
        mods: u8,
    },
    Up {
        x: f32,
        y: f32,
        mods: u8,
    },
    Wheel {
        x: f32,
        y: f32,
        dx: f32,
        dy: f32,
        mods: u8,
    },
    Key {
        key: String,
        mods: u8,
    },
    Tool(Tool),
}

/// World points, except `Press.at` and `Pan.last` (screen).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Gesture {
    Idle,
    Press {
        at: (f32, f32),
        target: Target,
    },
    Drag {
        start: (f32, f32),
        ids: Vec<String>,
        dx: f32,
        dy: f32,
    },
    Link {
        from: String,
        kind: String,
        to: (f32, f32),
    },
    Marquee {
        start: (f32, f32),
        end: (f32, f32),
    },
    Pan {
        last: (f32, f32),
    },
}

const KINDS: [LinkKind; 5] = [
    LinkKind::Calls,
    LinkKind::Imports,
    LinkKind::Renders,
    LinkKind::Http,
    LinkKind::Uses,
];

fn element_kind(t: Tool) -> Option<ElementKind> {
    Some(match t {
        Tool::Module => ElementKind::Module,
        Tool::Function => ElementKind::Function,
        Tool::Route => ElementKind::Route,
        Tool::System => ElementKind::System,
        Tool::Note => ElementKind::Note,
        Tool::Frame => ElementKind::Frame,
        Tool::Select | Tool::Link => return None,
    })
}

fn kind_name(k: ElementKind) -> String {
    serde_json::to_value(k).unwrap().as_str().unwrap().into()
}

impl Engine {
    pub(crate) fn cursor(&self) -> &'static str {
        match (&self.gesture, self.tool) {
            (Gesture::Pan { .. }, _) => "grabbing",
            (Gesture::Drag { .. }, _) => "move",
            (_, Tool::Select) => "default",
            _ => "crosshair",
        }
    }

    pub fn input(&mut self, i: Input) -> Out {
        let mut o = match i {
            Input::Tool(t) => {
                (self.tool, self.gesture) = (t, Gesture::Idle);
                self.out()
            }
            Input::Down { x, y, button, mods } => self.down(x, y, button, mods),
            Input::Move { x, y, .. } => self.drag_to(x, y),
            Input::Up { x, y, mods } => self.up(x, y, mods),
            Input::Wheel { x, y, dx, dy, mods } => {
                if mods & (CTRL | META) != 0 {
                    self.camera.zoom_at(x, y, (-dy * 0.01).exp());
                } else {
                    self.camera.x += dx / self.camera.zoom;
                    self.camera.y += dy / self.camera.zoom;
                }
                self.redraw()
            }
            Input::Key { key, mods } => self.key(&key, mods),
        };
        o.redraw |= o.changed;
        if o.redraw {
            self.dirty();
        }
        o.cursor = self.cursor();
        o.selection = self.selection.clone();
        o
    }

    fn redraw(&self) -> Out {
        Out {
            redraw: true,
            ..self.out()
        }
    }

    fn edit(&self, id: &str) -> Option<EditRequest> {
        let e = self.board().design.elements.iter().find(|e| e.id == id)?;
        Some(EditRequest {
            id: id.into(),
            rect: self.camera.to_screen(rect_of(e)?),
            text: e.name.clone(),
        })
    }

    fn kind_of(&self, t: &Target) -> Option<String> {
        match t {
            Target::El(id) => (self.board().design.elements.iter())
                .find(|e| &e.id == id)
                .map(|e| kind_name(e.kind)),
            Target::Ctx(id) => (self.ctx.boxes.iter())
                .find(|c| &c.id == id)
                .map(|c| c.kind.clone()),
            Target::Link(_) => None,
        }
    }

    fn down(&mut self, x: f32, y: f32, button: u8, mods: u8) -> Out {
        if button == 1 {
            self.gesture = Gesture::Pan { last: (x, y) };
            return self.out();
        }
        let w = self.camera.to_world(x, y);
        if let Some(k) = element_kind(self.tool) {
            return self.place(k, w);
        }
        let t = self.target(x, y);
        let linkable = t
            .as_ref()
            .and_then(|t| Some((t.id().to_string(), self.kind_of(t)?)));
        // the link tool, or the handle at an element's right middle
        let handle = match &t {
            Some(Target::El(id)) => self.index.rect(&Target::El(id.clone())).is_some_and(|r| {
                (w.0 - r[0] - r[2]).hypot(w.1 - r[1] - r[3] / 2.0) <= 8.0 / self.camera.zoom
            }),
            _ => false,
        };
        if let Some((from, kind)) = linkable.filter(|_| self.tool == Tool::Link || handle) {
            self.gesture = Gesture::Link { from, kind, to: w };
            return self.redraw();
        }
        let Some(t) = t else {
            if mods & SHIFT == 0 {
                self.selection.clear();
            }
            self.gesture = Gesture::Marquee { start: w, end: w };
            return self.redraw();
        };
        let id = t.id().to_string();
        if mods & SHIFT != 0 {
            match self.selection.iter().position(|s| *s == id) {
                Some(i) => _ = self.selection.remove(i),
                None => self.selection.push(id.clone()),
            }
        } else if !self.selection.contains(&id) {
            self.selection = vec![id.clone()];
        }
        let mut o = self.redraw();
        if mods & DOUBLE != 0 {
            o.edit = self.edit(&id);
        }
        self.gesture = Gesture::Press {
            at: (x, y),
            target: t,
        };
        o
    }

    fn place(&mut self, k: ElementKind, w: (f32, f32)) -> Out {
        self.tool = Tool::Select;
        let lane = self
            .ctx
            .lanes
            .iter()
            .find(|l| w.1 >= l.top && w.1 <= l.bottom);
        if lane.is_some_and(|l| l.kind == RowKind::External) && k != ElementKind::System {
            let mut o = self.out();
            o.error = Some("Only external systems go in External services".into());
            return o;
        }
        let layer = row_at(&self.ctx.lanes, w.1)
            .filter(|_| takes_layer(k))
            .map(String::from);
        let mut name = kind_name(k);
        name[..1].make_ascii_uppercase();
        let id = new_id(&self.board().design, k, &name);
        let (sw, sh) = default_size(k);
        let add = Op::Add {
            id: Some(id.clone()),
            kind: k,
            name,
            file: None,
            method: None,
            path: None,
            layer: layer.clone(),
            on: None,
            x: Some(snap(w.0 - sw as f32 / 2.0)),
            y: Some(snap(w.1 - sh as f32 / 2.0)),
            w: None,
            h: None,
        };
        let mut o = self.ops(&[add], Step::User);
        if o.error.is_none() {
            self.selection = vec![id.clone()];
            o.edit = self.edit(&id);
            o.placed = Some((id, layer));
        }
        o
    }

    fn drag_to(&mut self, x: f32, y: f32) -> Out {
        let w = self.camera.to_world(x, y);
        if let Gesture::Press { at, target } = &self.gesture {
            let (at, link) = (*at, matches!(target, Target::Link(_)));
            if (x - at.0).hypot(y - at.1) < 3.0 || link {
                return self.out();
            }
            // only elements move; selected context boxes stay put
            let ids = self.selected_elements();
            if ids.is_empty() {
                return self.out();
            }
            let start = self.camera.to_world(at.0, at.1);
            let (dx, dy) = (w.0 - start.0, w.1 - start.1);
            self.gesture = Gesture::Drag { start, ids, dx, dy };
            return self.redraw();
        }
        match &mut self.gesture {
            Gesture::Drag { start, dx, dy, .. } => (*dx, *dy) = (w.0 - start.0, w.1 - start.1),
            Gesture::Link { to, .. } => *to = w,
            Gesture::Marquee { end, .. } => *end = w,
            Gesture::Pan { last } => {
                self.camera.x -= (x - last.0) / self.camera.zoom;
                self.camera.y -= (y - last.1) / self.camera.zoom;
                *last = (x, y);
            }
            Gesture::Idle | Gesture::Press { .. } => return self.out(),
        }
        self.redraw()
    }

    fn up(&mut self, x: f32, y: f32, mods: u8) -> Out {
        match std::mem::replace(&mut self.gesture, Gesture::Idle) {
            Gesture::Press {
                target: Target::Link(key),
                ..
            } => self.cycle(&key),
            Gesture::Drag { ids, dx, dy, .. } => {
                let ops = move_ops(self.board(), &ids, dx, dy, &self.ctx.lanes);
                let mut o = self.ops(&ops, Step::User);
                o.redraw = true; // the drag offset is gone even when nothing moved
                o
            }
            Gesture::Link { from, kind, .. } => {
                let to = self.target(x, y).filter(|t| t.id() != from);
                let Some((to, k)) = to.and_then(|t| Some((t.id().to_string(), self.kind_of(&t)?)))
                else {
                    return self.redraw();
                };
                let kind = default_link_kind(&kind, &k);
                let mut o = self.ops(&[Op::Link { from, to, kind }], Step::User);
                o.redraw = true;
                o
            }
            Gesture::Marquee { start, .. } => {
                let end = self.camera.to_world(x, y);
                let r = [
                    start.0.min(end.0),
                    start.1.min(end.1),
                    (start.0 - end.0).abs(),
                    (start.1 - end.1).abs(),
                ];
                if mods & SHIFT == 0 {
                    self.selection.clear();
                }
                for t in self.index.within(r) {
                    if !self.selection.iter().any(|s| s == t.id()) {
                        self.selection.push(t.id().to_string());
                    }
                }
                self.redraw()
            }
            _ => self.out(),
        }
    }

    /// A click on a link steps its kind: calls, imports, renders, http, uses.
    fn cycle(&mut self, key: &str) -> Out {
        let Some(l) = self.board().design.links.iter().find(|l| l.key() == key) else {
            return self.out();
        };
        let i = KINDS.iter().position(|k| *k == l.kind).unwrap_or(0);
        let next = Op::Link {
            from: l.from.clone(),
            to: l.to.clone(),
            kind: KINDS[(i + 1) % KINDS.len()],
        };
        let new_key = format!("{}|{}|{}", l.from, KINDS[(i + 1) % 5].as_str(), l.to);
        let o = self.ops(&[Op::Unlink { key: key.into() }, next], Step::User);
        if o.error.is_some() {
            return o;
        }
        self.selection = vec![new_key];
        Out {
            selection: self.selection.clone(),
            ..o
        }
    }

    fn selected_elements(&self) -> Vec<String> {
        let d = &self.board().design;
        (self.selection.iter())
            .filter(|s| d.elements.iter().any(|e| &e.id == *s))
            .cloned()
            .collect()
    }

    fn key(&mut self, key: &str, mods: u8) -> Out {
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
                self.fit();
                self.redraw()
            }
            _ => self.out(), // Cmd/Ctrl+Enter (Plan) is the host's
        }
    }

    fn fit(&mut self) {
        let rects = (self
            .board()
            .design
            .elements
            .iter()
            .filter_map(|e| self.shown_rect(e)))
        .chain(self.ctx.boxes.iter().map(|c| [c.x, c.y, c.w, c.h]));
        let b = rects.fold(None, |a: Option<[f32; 4]>, r| {
            let [x0, y0, x1, y1] = a.unwrap_or([r[0], r[1], r[0] + r[2], r[1] + r[3]]);
            Some([
                x0.min(r[0]),
                y0.min(r[1]),
                x1.max(r[0] + r[2]),
                y1.max(r[1] + r[3]),
            ])
        });
        if let Some([x0, y0, x1, y1]) = b {
            self.camera
                .fit([x0, y0, x1 - x0, y1 - y0], self.view[2], self.view[3]);
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

#[cfg(test)]
#[path = "input_tests.rs"]
mod tests;
