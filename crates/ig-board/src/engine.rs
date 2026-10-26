//! The engine: the board, its history and highlights, checks, camera and scene.

use crate::hit::{Index, Target};
use crate::input::{Gesture, Tool};
use crate::scene::{self, A11yItem, Label, Scene};
use crate::snap::rect_of;
use crate::{Camera, Context, History, Severity, Step, meaning, merge, place_new};
use ig_design::{Board, Element, Link, Op, apply};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq)]
pub struct EditRequest {
    pub id: String,
    /// Screen rect for the rename overlay.
    pub rect: [f32; 4],
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Out {
    pub redraw: bool,
    /// The board changed: save it.
    pub changed: bool,
    /// Its meaning changed: re-check it.
    pub meaning: bool,
    pub selection: Vec<String>,
    pub edit: Option<EditRequest>,
    /// A placed element and its layer: the host proposes its file.
    pub placed: Option<(String, Option<String>)>,
    pub cursor: &'static str,
    pub error: Option<String>,
    /// What an outside change brought in or took out: element ids, link keys, mark nodes, bridge keys.
    pub touched: Vec<String>,
}

pub struct Engine {
    pub(crate) ctx: Context,
    history: History,
    /// The disk state ours was last in sync with: the merge base.
    base: Board,
    pub(crate) hidden: HashSet<String>,
    pub(crate) selection: Vec<String>,
    pub(crate) highlights: Vec<String>,
    pub(crate) checks: HashMap<String, (Severity, String)>,
    pub(crate) camera: Camera,
    pub(crate) tool: Tool,
    pub(crate) gesture: Gesture,
    pub(crate) index: Index,
    pub(crate) clipboard: (Vec<Element>, Vec<Link>, i32),
    pub(crate) view: [f32; 4],
    meaning: u64,
    scene: Scene,
    drawn: Option<([f32; 4], Camera)>,
}

impl Engine {
    pub fn new(ctx: Context, board: Board) -> Self {
        let mut e = Engine {
            meaning: meaning(&board),
            history: History::new(board.clone(), 200),
            base: board,
            ctx,
            hidden: HashSet::new(),
            selection: vec![],
            highlights: vec![],
            checks: HashMap::new(),
            camera: Camera::default(),
            tool: Tool::Select,
            gesture: Gesture::Idle,
            index: Index::default(),
            clipboard: (vec![], vec![], 0),
            view: [0.0, 0.0, 1200.0, 800.0],
            scene: Scene::default(),
            drawn: None,
        };
        e.refresh();
        e
    }

    pub fn board(&self) -> &Board {
        self.history.current()
    }

    pub fn camera(&self) -> Camera {
        self.camera
    }

    pub fn tool(&self) -> Tool {
        self.tool
    }

    /// The host wrote `written` to disk: it becomes the merge base. Not the current board, which
    /// may have moved on while the write was in flight.
    pub fn saved(&mut self, written: Board) {
        self.base = written;
    }

    /// A new context (another scope, or the code changed): the board and its history stay.
    pub fn set_context(&mut self, ctx: Context) {
        self.ctx = ctx;
        let sel = std::mem::take(&mut self.selection);
        self.select(sel);
        self.refresh();
    }

    /// Hides the elements and context boxes of these layers (not drawn, not hit).
    pub fn set_hidden(&mut self, layers: Vec<String>) {
        self.hidden = layers.into_iter().collect();
        self.refresh();
    }

    pub(crate) fn shown(&self, layer: &Option<String>) -> bool {
        layer.as_ref().is_none_or(|l| !self.hidden.contains(l))
    }

    /// Where an element is drawn now: hidden ones nowhere, dragged ones at the drag offset.
    pub(crate) fn shown_rect(&self, e: &Element) -> Option<[f32; 4]> {
        let mut r = rect_of(e).filter(|_| self.shown(&e.layer))?;
        if let Gesture::Drag { ids, dx, dy, .. } = &self.gesture
            && ids.contains(&e.id)
        {
            (r[0], r[1]) = (r[0] + dx, r[1] + dy);
        }
        Some(r)
    }

    pub(crate) fn refresh(&mut self) {
        self.index = Index::build(self.board(), &self.ctx, &self.hidden);
        self.drawn = None;
    }

    pub(crate) fn out(&self) -> Out {
        Out {
            redraw: false,
            changed: false,
            meaning: false,
            selection: self.selection.clone(),
            edit: None,
            placed: None,
            cursor: self.cursor(),
            error: None,
            touched: vec![],
        }
    }

    /// An element, link key or context box of the current board.
    fn live(&self, id: &str) -> bool {
        let d = &self.board().design;
        d.elements.iter().any(|e| e.id == id)
            || d.links.iter().any(|l| l.key() == id)
            || self.ctx.boxes.iter().any(|c| c.id == id)
    }

    /// A new current board (already in the history): selection, index and flags follow it.
    fn changed(&mut self) -> Out {
        self.selection = (self.selection.iter())
            .filter(|s| self.live(s))
            .cloned()
            .collect();
        self.refresh();
        let m = meaning(self.board());
        let mut o = self.out();
        (o.redraw, o.changed, o.meaning) = (true, true, m != self.meaning);
        self.meaning = m;
        o
    }

    /// Unplaced elements go to their row, then the board becomes one history step.
    fn commit(&mut self, next: Board, step: Step) -> Out {
        let unplaced: Vec<String> = (next.design.elements.iter())
            .filter(|e| e.x.is_none() || e.y.is_none())
            .map(|e| e.id.clone())
            .collect();
        let next = place_new(&next, &unplaced, &self.ctx);
        match step {
            Step::Agent(t) => self.history.agent(next, &t),
            Step::Amend => self.history.amend(next),
            s => self.history.push(next, s),
        }
        self.changed()
    }

    fn highlight(&mut self, ids: Vec<String>) {
        for id in ids {
            if !self.highlights.contains(&id) {
                self.highlights.push(id);
            }
        }
    }

    /// One history step (`Amend`: joined to the top one); a user step clears the highlights it
    /// touches, an agent step adds them.
    pub fn ops(&mut self, ops: &[Op], step: Step) -> Out {
        let (next, t) = match apply(self.board(), ops) {
            Ok(r) => r,
            Err(e) => {
                let mut o = self.out();
                o.error = Some(e.to_string());
                return o;
            }
        };
        if &next == self.board() {
            return self.out();
        }
        let touched: Vec<String> = [t.elements, t.links, t.bridges].concat();
        match step {
            Step::Agent(_) => self.highlight(touched.clone()),
            _ => self.highlights.retain(|h| !touched.contains(h)),
        }
        self.commit(next, step)
    }

    /// The file changed on disk: merged into ours (the user wins) as an agent or external step.
    /// A drag in flight goes on and wins: its elements keep the position it started from.
    pub fn external(&mut self, disk: Board, turn: Option<&str>) -> Out {
        let mut m = merge(&self.base, self.board(), &disk);
        self.base = disk;
        match &self.gesture {
            Gesture::Drag { ids, .. } => {
                for e in m
                    .board
                    .design
                    .elements
                    .iter_mut()
                    .filter(|e| ids.contains(&e.id))
                {
                    if let Some(o) = self.board().design.elements.iter().find(|o| o.id == e.id) {
                        (e.x, e.y) = (o.x, o.y);
                    }
                }
            }
            _ => self.gesture = Gesture::Idle,
        }
        if &m.board == self.board() {
            return self.out();
        }
        let touched = [m.changed.clone(), m.removed].concat();
        self.highlight(m.changed);
        let step = turn.map_or(Step::External, |t| Step::Agent(t.into()));
        let mut o = self.commit(m.board, step);
        o.touched = touched;
        o
    }

    /// Takes back agent turn `turn` (its steps still in the history, not undone) as one user
    /// step: each step's change is merged out of the board, so later edits stay.
    pub fn revert(&mut self, turn: &str) -> Out {
        self.gesture = Gesture::Idle;
        let mut next = self.board().clone();
        let mut keys = vec![];
        for (before, after) in self.history.turn_steps(turn).into_iter().rev() {
            keys.extend(Self::step_keys(&before, &after));
            next = merge(&after, &next, &before).board;
        }
        self.highlights.retain(|h| !keys.contains(h));
        self.drawn = None;
        if &next == self.board() {
            return self.out();
        }
        self.commit(next, Step::User)
    }

    pub fn set_check(&mut self, targets: Vec<(String, Severity, String)>) {
        self.checks = targets.into_iter().map(|(id, s, r)| (id, (s, r))).collect();
        self.drawn = None;
    }

    pub fn dismiss(&mut self, id: Option<&str>) {
        match id {
            Some(id) => self.highlights.retain(|h| h != id),
            None => self.highlights.clear(),
        }
        self.drawn = None;
    }

    /// Selects these ids (elements, link keys, context boxes); unknown ones are dropped.
    pub fn select(&mut self, ids: Vec<String>) {
        self.selection = ids.into_iter().filter(|s| self.live(s)).collect();
        self.drawn = None;
    }

    pub fn highlighted(&self) -> Vec<String> {
        self.highlights.clone()
    }

    /// The keys a step changed, from the board before it to the board after it.
    fn step_keys(before: &Board, after: &Board) -> Vec<String> {
        merge(before, before, after).changed
    }

    fn highlighted_step(&self) -> bool {
        !matches!(self.history.top_step(), Step::User)
    }

    /// Undoing an agent or external step clears that step's highlights only.
    pub fn undo(&mut self) -> Out {
        self.gesture = Gesture::Idle;
        let (lit, after) = (self.highlighted_step(), self.board().clone());
        if self.history.undo().is_none() {
            return self.out();
        }
        if lit {
            let keys = Self::step_keys(self.board(), &after);
            self.highlights.retain(|h| !keys.contains(h));
        }
        self.changed()
    }

    /// Redoing an agent or external step highlights what it changed again.
    pub fn redo(&mut self) -> Out {
        self.gesture = Gesture::Idle;
        let before = self.board().clone();
        if self.history.redo().is_none() {
            return self.out();
        }
        if self.highlighted_step() {
            let keys = Self::step_keys(&before, self.board());
            self.highlight(keys);
        }
        self.changed()
    }

    /// Rebuilt only when something changed or the viewport (`[x, y, w, h]` screen px) moved.
    pub fn scene(&mut self, viewport: [f32; 4]) -> &Scene {
        self.view = viewport;
        if self.drawn != Some((viewport, self.camera)) {
            self.scene = scene::build(self, viewport);
            self.drawn = Some((viewport, self.camera));
        }
        &self.scene
    }

    pub(crate) fn dirty(&mut self) {
        self.drawn = None;
    }

    pub fn labels(&self) -> Vec<Label> {
        scene::labels(self)
    }

    pub fn a11y(&self) -> Vec<A11yItem> {
        scene::a11y(self)
    }

    /// The topmost element, link key or context box at a screen point.
    pub fn hit(&self, x: f32, y: f32) -> Option<String> {
        self.target(x, y).map(|t| t.id().to_string())
    }

    pub(crate) fn target(&self, x: f32, y: f32) -> Option<Target> {
        let (wx, wy) = self.camera.to_world(x, y);
        self.index.at(wx, wy, 6.0 / self.camera.zoom)
    }
}

#[cfg(test)]
#[path = "engine_tests.rs"]
pub(crate) mod tests;
