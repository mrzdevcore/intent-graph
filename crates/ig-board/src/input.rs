//! Gestures, a small state machine (idle, placing, dragging, linking, marquee, panning), and
//! the clicks of the architecture board (its keys: `arch_keys.rs`; the screens': `screen_input.rs`).

use crate::engine::{EditRequest, Engine, Out, Surface};
use crate::hit::Target;
use crate::snap::{default_size, free_x, rect_of, taken, takes_layer};
use crate::{RowKind, Step, default_link_kind, move_ops, row_at, snap};
use ig_design::{ElementKind, LinkKind, Op, PieceKind, new_id};

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
    /// The screens' tools (`screen_input.rs`).
    Screen,
    /// The UI pieces palette is open (the host shows it).
    Pieces,
    Piece(PieceKind),
    Pen,
    Arrow,
    /// The host opens the file picker.
    Import,
    /// Link to the architecture (Task 15).
    Bridge,
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
    /// World points and pressure (0..1), in this screen.
    Pen {
        screen: String,
        points: Vec<[f32; 3]>,
    },
    Arrow {
        start: (f32, f32),
        to: (f32, f32),
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
        _ => return None,
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
        let mut o = match (i, self.read_only) {
            (i, Some(pick)) => self.view_input(i, pick),
            (i, None) => self.edit_input(i),
        };
        o.redraw |= o.changed;
        if o.redraw {
            self.dirty();
        }
        o.cursor = self.cursor();
        o.selection = self.selection.clone();
        o
    }

    fn wheel(&mut self, x: f32, y: f32, dx: f32, dy: f32, mods: u8) -> Out {
        if mods & (CTRL | META) != 0 {
            self.camera.zoom_at(x, y, (-dy * 0.01).exp());
        } else {
            self.camera.x += dx / self.camera.zoom;
            self.camera.y += dy / self.camera.zoom;
        }
        self.redraw()
    }

    /// A read-only board: pan, zoom and fit; in pick mode the hovered element or context box
    /// is the selection and a click picks it.
    fn view_input(&mut self, i: Input, pick: bool) -> Out {
        let at = |e: &Self, x, y| {
            (e.target(x, y))
                .filter(|t| matches!(t, Target::El(_) | Target::Ctx(_)))
                .map(|t| t.id().to_string())
        };
        match i {
            Input::Down {
                x, y, button: 1, ..
            } => {
                self.gesture = Gesture::Pan { last: (x, y) };
                self.out()
            }
            Input::Down { x, y, .. } if pick => Out {
                picked: at(self, x, y),
                ..self.out()
            },
            Input::Move { x, y, .. } if matches!(self.gesture, Gesture::Pan { .. }) => {
                self.drag_to(x, y)
            }
            Input::Move { x, y, .. } if pick => {
                let hover: Vec<String> = at(self, x, y).into_iter().collect();
                let o = Out {
                    redraw: hover != self.selection,
                    ..self.out()
                };
                self.selection = hover;
                o
            }
            Input::Up { .. } => {
                self.gesture = Gesture::Idle;
                self.out()
            }
            Input::Wheel { x, y, dx, dy, mods } => self.wheel(x, y, dx, dy, mods),
            Input::Key { key, mods } if mods & SHIFT != 0 && "!@12".contains(key.as_str()) => {
                self.key(&key, mods)
            }
            _ => self.out(),
        }
    }

    fn edit_input(&mut self, i: Input) -> Out {
        match i {
            Input::Tool(t) => {
                (self.tool, self.gesture) = (t, Gesture::Idle);
                self.out()
            }
            i
            @ (Input::Down { .. } | Input::Move { .. } | Input::Up { .. } | Input::Key { .. })
                if self.surface == Surface::Screens =>
            {
                self.screen_input(i)
            }
            Input::Down { x, y, button, mods } => self.down(x, y, button, mods),
            Input::Move { x, y, .. } => self.drag_to(x, y),
            Input::Up { x, y, mods } => self.up(x, y, mods),
            Input::Wheel { x, y, dx, dy, mods } => self.wheel(x, y, dx, dy, mods),
            Input::Key { key, mods } => self.key(&key, mods),
        }
    }

    pub(crate) fn redraw(&self) -> Out {
        Out {
            redraw: true,
            ..self.out()
        }
    }

    pub(crate) fn edit(&self, id: &str) -> Option<EditRequest> {
        if self.surface == Surface::Screens {
            return self.screen_edit(id);
        }
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
            Target::Link(_) | Target::Screen(_) => None,
        }
    }

    pub(crate) fn down(&mut self, x: f32, y: f32, button: u8, mods: u8) -> Out {
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
            if matches!(t, Target::Link(_)) {
                return o; // the first click already stepped the kind
            }
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
        let (x, y) = (snap(w.0 - sw as f32 / 2.0), snap(w.1 - sh as f32 / 2.0));
        // the nearest free slot along the row: never on a context box or another element
        let x = if k == ElementKind::Frame {
            x
        } else {
            free_x(&taken(&self.ctx, self.board()), x, y, sw, sh)
        };
        let add = Op::Add {
            id: Some(id.clone()),
            kind: k,
            name,
            file: None,
            method: None,
            path: None,
            layer: layer.clone(),
            on: None,
            x: Some(x),
            y: Some(y),
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

    pub(crate) fn drag_to(&mut self, x: f32, y: f32) -> Out {
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
            _ => return self.out(),
        }
        self.redraw()
    }

    pub(crate) fn up(&mut self, x: f32, y: f32, mods: u8) -> Out {
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

    pub(crate) fn selected_elements(&self) -> Vec<String> {
        if self.surface == Surface::Screens {
            return self.selected_movable();
        }
        let d = &self.board().design;
        (self.selection.iter())
            .filter(|s| d.elements.iter().any(|e| &e.id == *s))
            .cloned()
            .collect()
    }
}

#[cfg(test)]
#[path = "input_tests.rs"]
mod tests;
