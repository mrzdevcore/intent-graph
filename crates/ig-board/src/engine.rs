//! The engine: the board, its history and highlights, checks, camera and scene.

use crate::hit::{Index, Target};
use crate::input::{Gesture, Tool};
use crate::merge::only_in;
use crate::scene::{self, A11yItem, Label, Scene};
use crate::screens::{diff, split_key, split_nav};
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
    /// Pick mode: the element or context box clicked.
    pub picked: Option<String>,
    /// A piece dragged out of the view and let go there (its key and the screen point): it
    /// stays where it was; the host may link it to what is under the point.
    pub dropped: Option<(String, f32, f32)>,
}

/// Which board the engine shows and edits; both share the one board, history and highlights.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    Architecture,
    Screens,
}

pub struct Engine {
    pub(crate) ctx: Context,
    pub(crate) surface: Surface,
    /// Images the host has decoded: their size in px. The others draw as a placeholder.
    pub(crate) images: HashMap<String, (u32, u32)>,
    /// Ink triangles per screen or piece key, in its own px, with what they were made from.
    pub(crate) inks: crate::screen_scene::Inks,
    /// Pen pressure (0..1) of the latest pointer event: `PointerEvent.pressure`.
    pub(crate) pressure: f32,
    pub(crate) pieces_clip: Vec<(String, ig_design::Piece)>,
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
    /// `Some(pick)`: a read-only view; in pick mode hover highlights and a click picks.
    pub(crate) read_only: Option<bool>,
    scene: Scene,
    drawn: Option<([f32; 4], Camera)>,
}

impl Engine {
    pub fn new(ctx: Context, board: Board) -> Self {
        let mut e = Engine {
            meaning: meaning(&board),
            read_only: None,
            history: History::new(board.clone(), 200),
            base: board,
            ctx,
            surface: Surface::Architecture,
            images: HashMap::new(),
            inks: Default::default(),
            pressure: 0.5,
            pieces_clip: vec![],
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
        self.index = match self.surface {
            Surface::Architecture => Index::build(self.board(), &self.ctx, &self.hidden),
            Surface::Screens => Index::build_screens(self.board()),
        };
        self.inks.stale = true;
        self.drawn = None;
    }

    pub fn surface(&mut self, s: Surface) {
        if s != self.surface {
            (self.surface, self.tool, self.gesture) = (s, Tool::Select, Gesture::Idle);
            self.selection.clear();
            self.refresh();
        }
    }

    pub fn current_surface(&self) -> Surface {
        self.surface
    }

    /// The board the last save or outside change left on disk (the merge base).
    pub fn merge_base(&self) -> &Board {
        &self.base
    }

    /// `PointerEvent.pressure` of the next pointer events (pen strokes).
    pub fn set_pressure(&mut self, p: f32) {
        self.pressure = p.clamp(0.0, 1.0);
    }

    /// The image's texture is gone: it draws as a placeholder.
    pub fn drop_image(&mut self, asset: &str) {
        self.images.remove(asset);
        self.drawn = None;
    }

    /// The host decoded this image (`w × h` px): it draws instead of its placeholder.
    pub fn set_image(&mut self, asset: &str, w: u32, h: u32) {
        self.images.insert(asset.into(), (w, h));
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
            picked: None,
            dropped: None,
        }
    }

    /// From now on the board is only looked at (pan, zoom, fit): its inputs never edit it. In
    /// pick mode hovering highlights an element or context box and a click returns it as
    /// `Out.picked`.
    pub fn set_read_only(&mut self, pick: bool) {
        (self.read_only, self.tool, self.gesture) = (Some(pick), Tool::Select, Gesture::Idle);
    }

    /// `Some(pick)` when read-only.
    pub fn read_only(&self) -> Option<bool> {
        self.read_only
    }

    /// An element, link key or context box of the current board; on the screens, a screen,
    /// piece or nav key.
    fn live(&self, id: &str) -> bool {
        self.live_in(self.surface, id)
    }

    fn live_in(&self, s: Surface, id: &str) -> bool {
        let b = self.board();
        let d = &b.design;
        match s {
            Surface::Architecture => {
                d.elements.iter().any(|e| e.id == id)
                    || d.links.iter().any(|l| l.key() == id)
                    || self.ctx.boxes.iter().any(|c| c.id == id)
            }
            Surface::Screens => match (split_nav(id), split_key(id)) {
                (Some((s, f, t)), _) => (b.screens.get(s))
                    .is_some_and(|s| s.navs.iter().any(|n| n.from == f && n.to == t)),
                (_, Some((s, p))) => {
                    (b.screens.get(s)).is_some_and(|s| s.elements.iter().any(|e| e.id == p))
                }
                _ => b.screens.contains_key(id),
            },
        }
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
        let touched: Vec<String> = [
            t.elements,
            t.links,
            t.bridges,
            diff(&self.board().screens, &next.screens).0,
        ]
        .concat();
        match step {
            Step::Agent(_) => self.highlight(touched.clone()),
            _ => self.highlights.retain(|h| !touched.contains(h)),
        }
        self.commit(next, step)
    }

    /// The file changed on disk: merged into ours (the user wins) as an agent or external step.
    /// A drag in flight goes on and wins: its elements keep the position it started from.
    pub fn external(&mut self, disk: Board, turn: Option<&str>) -> Out {
        let base = std::mem::replace(&mut self.base, disk.clone());
        self.merge_in(&base, disk, turn)
    }

    /// A recovery copy merged in as one outside step, over the disk it was kept against
    /// (`base`; None, an older copy: over the disk now, adding and changing only, never
    /// removing). The merge base stays the disk, so the result still gets written.
    pub fn external_from(&mut self, kept: Board, base: Option<Board>) -> Out {
        let base = base.unwrap_or_else(|| only_in(&self.base, &kept));
        self.merge_in(&base, kept, None)
    }

    fn merge_in(&mut self, base: &Board, theirs: Board, turn: Option<&str>) -> Out {
        let mut m = merge(base, self.board(), &theirs);
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

    /// Selects these ids (elements, link keys, context boxes; screen, piece and nav keys);
    /// unknown ones are dropped. Ids only the other surface has switch to it (a chat card's Show).
    pub fn select(&mut self, ids: Vec<String>) {
        let other = match self.surface {
            Surface::Architecture => Surface::Screens,
            Surface::Screens => Surface::Architecture,
        };
        if !ids.iter().any(|i| self.live(i)) && ids.iter().any(|i| self.live_in(other, i)) {
            self.surface(other);
        }
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
            self.scene = match self.surface {
                Surface::Architecture => scene::build(self, viewport),
                Surface::Screens => {
                    self.update_inks();
                    crate::screen_scene::build(self, viewport)
                }
            };
            self.drawn = Some((viewport, self.camera));
        }
        &self.scene
    }

    pub(crate) fn dirty(&mut self) {
        self.drawn = None;
    }

    pub fn labels(&self) -> Vec<Label> {
        match self.surface {
            Surface::Architecture => scene::labels(self),
            Surface::Screens => crate::screen_scene::labels(self),
        }
    }

    pub fn a11y(&self) -> Vec<A11yItem> {
        match self.surface {
            Surface::Architecture => scene::a11y(self),
            Surface::Screens => crate::screen_scene::a11y(self),
        }
    }

    /// The topmost element, link key or context box at a screen point; read-only, only what can
    /// be picked (an element or context box).
    pub fn hit(&self, x: f32, y: f32) -> Option<String> {
        (self.target(x, y))
            .filter(|t| self.read_only.is_none() || matches!(t, Target::El(_) | Target::Ctx(_)))
            .map(|t| t.id().to_string())
    }

    pub(crate) fn target(&self, x: f32, y: f32) -> Option<Target> {
        let (wx, wy) = self.camera.to_world(x, y);
        self.index.at(wx, wy, 6.0 / self.camera.zoom)
    }
}

#[cfg(test)]
#[path = "engine_tests.rs"]
pub(crate) mod tests;
