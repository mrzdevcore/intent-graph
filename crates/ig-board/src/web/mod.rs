//! The wasm face of the engine (wasm32 only): values cross as JSON strings, the renderer draws
//! on a WebGL2 canvas. The TypeScript side codes against `viewer/src/board/engine.d.ts`.

mod gl;
mod programs;

use crate::{Context, Engine, Input, Out, Severity, Step, Surface, Tool};
use gl::{Palette, Renderer};
use ig_design::{
    Board, Op, PieceKind, Scope, Screen, empty, parse, parse_screen, screen_to_text, to_text,
};
use serde::Deserialize;
use serde_json::json;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{HtmlCanvasElement, ImageBitmap, WebGl2RenderingContext};

thread_local! {
    static PANIC: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// The message of the last Rust panic: wasm traps with only "unreachable", so the host asks.
#[wasm_bindgen(js_name = lastPanic)]
pub fn last_panic() -> Option<String> {
    PANIC.with(|p| p.borrow().clone())
}

#[wasm_bindgen(start)]
fn start() {
    std::panic::set_hook(Box::new(|info| {
        PANIC.with(|p| *p.borrow_mut() = Some(info.to_string()));
    }));
}

/// `EngineInput` of engine.d.ts.
#[derive(Deserialize)]
#[serde(tag = "t", rename_all = "lowercase")]
enum Ev {
    Down {
        x: f32,
        y: f32,
        button: u8,
        mods: u8,
        /// `PointerEvent.pressure` (pen strokes).
        pressure: Option<f32>,
    },
    Move {
        x: f32,
        y: f32,
        mods: u8,
        pressure: Option<f32>,
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
    Tool {
        tool: String,
    },
}

const TOOLS: [(&str, Tool); 14] = [
    ("select", Tool::Select),
    ("module", Tool::Module),
    ("function", Tool::Function),
    ("route", Tool::Route),
    ("system", Tool::System),
    ("note", Tool::Note),
    ("frame", Tool::Frame),
    ("link", Tool::Link),
    ("screen", Tool::Screen),
    ("pieces", Tool::Pieces),
    ("pen", Tool::Pen),
    ("arrow", Tool::Arrow),
    ("import", Tool::Import),
    ("bridge", Tool::Bridge),
];

/// `EngineInput`'s tool names; a UI piece is `piece:<kind>`.
fn tool(s: &str) -> Option<Tool> {
    if let Some(k) = s.strip_prefix("piece:") {
        let k: PieceKind = serde_json::from_value(json!(k)).ok()?;
        return Some(Tool::Piece(k));
    }
    TOOLS.iter().find(|t| t.0 == s).map(|t| t.1)
}

fn tool_name(t: Tool) -> String {
    match t {
        Tool::Piece(k) => format!("piece:{}", json!(k).as_str().unwrap_or_default()),
        t => TOOLS
            .iter()
            .find(|x| x.1 == t)
            .map_or("select", |x| x.0)
            .into(),
    }
}

/// Board text across the boundary: a design.json text, or
/// `{"design"?: Design, "screens"?: Screen[] | {id: Screen | null}}` (an array is every screen;
/// an object changes those ids only, null removing one). Each part is validated.
struct Part {
    design: Option<ig_design::Design>,
    all: Option<BTreeMap<String, Screen>>,
    some: BTreeMap<String, Option<Screen>>,
}

fn part(text: &str) -> Result<Part, String> {
    let v: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let screen = |v: &serde_json::Value| parse_screen(&v.to_string()).map_err(|e| e.to_string());
    let mut p = Part {
        design: None,
        all: None,
        some: BTreeMap::new(),
    };
    if v.get("version").is_some() {
        p.design = Some(parse(text).map_err(|e| e.to_string())?);
        return Ok(p);
    }
    if let Some(d) = v.get("design") {
        p.design = Some(parse(&d.to_string()).map_err(|e| e.to_string())?);
    }
    match v.get("screens") {
        Some(serde_json::Value::Array(a)) => {
            let all = a.iter().map(|s| screen(s).map(|s| (s.id.clone(), s)));
            p.all = Some(all.collect::<Result<_, _>>()?);
        }
        Some(serde_json::Value::Object(m)) => {
            for (id, s) in m {
                p.some
                    .insert(id.clone(), (!s.is_null()).then(|| screen(s)).transpose()?);
            }
        }
        _ => {}
    }
    Ok(p)
}

/// `base` with the part laid over it.
fn over(base: &Board, p: Part) -> Board {
    let mut b = base.clone();
    if let Some(d) = p.design {
        b.design = d;
    }
    if let Some(all) = p.all {
        b.screens = all;
    }
    for (id, s) in p.some {
        match s {
            Some(s) => b.screens.insert(id, s),
            None => b.screens.remove(&id),
        };
    }
    b
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum StepIn {
    User,
    Amend,
    Agent(String),
}

fn out_json(o: &Out) -> String {
    json!({
        "redraw": o.redraw,
        "changed": o.changed,
        "meaning": o.meaning,
        "selection": o.selection,
        "edit": o.edit.as_ref().map(|e| json!({"id": e.id, "rect": e.rect, "text": e.text})),
        "placed": o.placed.as_ref().map(|(id, layer)| json!({"id": id, "layer": layer})),
        "cursor": o.cursor,
        "error": o.error,
        "touched": o.touched,
    })
    .to_string()
}

fn error_json(e: impl ToString) -> String {
    json!({"redraw": false, "changed": false, "meaning": false, "selection": [], "edit": null,
           "placed": null, "cursor": "default", "error": e.to_string(), "touched": []})
    .to_string()
}

fn blank() -> Board {
    Board {
        design: empty("", Scope::Dir { id: String::new() }),
        screens: Default::default(),
    }
}

#[wasm_bindgen]
pub struct BoardEngine {
    engine: Engine,
    gl: Option<WebGl2RenderingContext>,
    renderer: Option<Renderer>,
    /// Kept so a restored context gets its textures back.
    bitmaps: HashMap<String, ImageBitmap>,
    palette: Palette,
    size: (f32, f32, f32),
    dirty: bool,
}

impl Default for BoardEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen]
impl BoardEngine {
    /// An empty board without a canvas (`attach` draws it).
    #[wasm_bindgen(constructor)]
    pub fn new() -> BoardEngine {
        BoardEngine {
            engine: Engine::new(Context::default(), blank()),
            gl: None,
            renderer: None,
            bitmaps: HashMap::new(),
            palette: Palette::new(),
            size: (1.0, 1.0, 1.0),
            dirty: true,
        }
    }

    /// Takes the canvas's WebGL2 context; the error says why there is none.
    pub fn attach(&mut self, canvas: HtmlCanvasElement) -> Result<(), String> {
        let ctx = canvas
            .get_context("webgl2")
            .map_err(|e| format!("{e:?}"))?
            .ok_or("getContext(\"webgl2\") returned null")?;
        let gl: WebGl2RenderingContext = ctx.dyn_into().map_err(|_| "not a WebGL2 context")?;
        self.gl = Some(gl);
        self.restore()
    }

    /// The context came back after `webglcontextlost`: programs, buffers and textures again.
    pub fn restore(&mut self) -> Result<(), String> {
        let gl = self.gl.clone().ok_or("no canvas attached")?;
        let mut r = Renderer::new(gl)?;
        for (asset, b) in &self.bitmaps {
            r.set_image(asset, b)?;
        }
        self.renderer = Some(r);
        self.dirty = true;
        Ok(())
    }

    /// A design.json text or `{design, screens: Screen[]}`; the surface stays.
    pub fn load(&mut self, board: &str, context: &str) -> Result<(), String> {
        let ctx: Context = serde_json::from_str(context).map_err(|e| e.to_string())?;
        let surface = self.engine.current_surface();
        self.engine = Engine::new(ctx, over(&blank(), part(board)?));
        self.engine.surface(surface);
        for (asset, b) in &self.bitmaps {
            self.engine.set_image(asset, b.width(), b.height());
        }
        self.dirty = true;
        Ok(())
    }

    /// `"architecture"` or `"screens"`: switches when given; the current one.
    pub fn surface(&mut self, name: Option<String>) -> String {
        match name.as_deref() {
            Some("screens") => self.engine.surface(Surface::Screens),
            Some("architecture") => self.engine.surface(Surface::Architecture),
            _ => {}
        }
        self.dirty = true;
        match self.engine.current_surface() {
            Surface::Architecture => "architecture",
            Surface::Screens => "screens",
        }
        .into()
    }

    /// Every screen's canonical file text (`ig_design::screen_to_text`), by id.
    pub fn screens(&self) -> String {
        let m: BTreeMap<&String, String> = (self.engine.board().screens.iter())
            .map(|(id, s)| (id, screen_to_text(s)))
            .collect();
        json!(m).to_string()
    }

    /// A state of a screen (loading, error, empty…): one step.
    pub fn variant(&mut self, screen: &str, state: &str) -> String {
        self.after(|e| e.variant(screen, state))
    }

    /// A stored image (`putAsset`'s name) becomes a screen's locked background: one step.
    #[wasm_bindgen(js_name = importImage)]
    pub fn import_image(&mut self, asset: &str) -> String {
        self.after(|e| e.import_image(asset))
    }

    /// The rename overlay's commit, on either surface.
    pub fn rename(&mut self, id: &str, text: &str) -> String {
        self.after(|e| e.rename(id, text))
    }

    /// Another scope's context; the board, its history and the camera stay.
    #[wasm_bindgen(js_name = setContext)]
    pub fn set_context(&mut self, context: &str) -> Result<(), String> {
        let ctx: Context = serde_json::from_str(context).map_err(|e| e.to_string())?;
        self.engine.set_context(ctx);
        self.dirty = true;
        Ok(())
    }

    /// The element, link key or context box under a canvas point (the hover reason).
    pub fn hit(&self, x: f32, y: f32) -> Option<String> {
        self.engine.hit(x, y)
    }

    /// The current tool, as `EngineInput`'s `tool` names it.
    pub fn tool(&self) -> String {
        tool_name(self.engine.tool())
    }

    pub fn input(&mut self, ev: &str) -> String {
        let i = match serde_json::from_str::<Ev>(ev) {
            Ok(Ev::Down {
                x,
                y,
                button,
                mods,
                pressure,
            }) => {
                self.engine.set_pressure(pressure.unwrap_or(0.5));
                Input::Down { x, y, button, mods }
            }
            Ok(Ev::Move {
                x,
                y,
                mods,
                pressure,
            }) => {
                self.engine.set_pressure(pressure.unwrap_or(0.5));
                Input::Move { x, y, mods }
            }
            Ok(Ev::Up { x, y, mods }) => Input::Up { x, y, mods },
            Ok(Ev::Wheel { x, y, dx, dy, mods }) => Input::Wheel { x, y, dx, dy, mods },
            Ok(Ev::Key { key, mods }) => Input::Key { key, mods },
            Ok(Ev::Tool { tool: t }) => match tool(&t) {
                Some(t) => Input::Tool(t),
                None => return error_json(format!("unknown tool {t}")),
            },
            Err(e) => return error_json(e),
        };
        self.after(|e| e.input(i))
    }

    /// One history step; `step` is `"user"`, `"amend"` (joins the top step) or `{"agent": turn}`.
    pub fn ops(&mut self, ops: &str, step: &str) -> String {
        let ops: Vec<Op> = match serde_json::from_str(ops) {
            Ok(o) => o,
            Err(e) => return error_json(e),
        };
        let step = match serde_json::from_str(step) {
            Ok(StepIn::User) => Step::User,
            Ok(StepIn::Amend) => Step::Amend,
            Ok(StepIn::Agent(t)) => Step::Agent(t),
            Err(e) => return error_json(e),
        };
        self.after(|e| e.ops(&ops, step))
    }

    /// A file changed on disk (a design.json text, or screens: see `Part`): merged in (an agent
    /// step when `turn` is set).
    pub fn external(&mut self, disk: &str, turn: Option<String>) -> String {
        match part(disk) {
            Ok(p) => {
                let b = over(self.engine.merge_base(), p);
                self.after(|e| e.external(b, turn.as_deref()))
            }
            Err(e) => error_json(e),
        }
    }

    /// What changed from `prev` to `next` (board texts: see `Part`) as lines for Claude; a
    /// geometry-only change gives none (`ig_board::delta`).
    pub fn delta(&self, prev: &str, next: &str) -> Result<String, String> {
        let b = |t: &str| part(t).map(|p| over(&blank(), p));
        Ok(json!(crate::delta(&b(prev)?, &b(next)?)).to_string())
    }

    /// `[[id, "error" | "warning" | "note", reason], …]`
    #[wasm_bindgen(js_name = setCheck)]
    pub fn set_check(&mut self, targets: &str) -> Result<(), String> {
        let t: Vec<(String, Severity, String)> =
            serde_json::from_str(targets).map_err(|e| e.to_string())?;
        self.engine.set_check(t);
        self.dirty = true;
        Ok(())
    }

    pub fn dismiss(&mut self, id: Option<String>) {
        self.engine.dismiss(id.as_deref());
        self.dirty = true;
    }

    pub fn highlighted(&self) -> String {
        json!(self.engine.highlighted()).to_string()
    }

    /// Selects these ids (the accessibility list's Up and Down).
    pub fn select(&mut self, ids: &str) -> Result<(), String> {
        let ids: Vec<String> = serde_json::from_str(ids).map_err(|e| e.to_string())?;
        self.engine.select(ids);
        self.dirty = true;
        Ok(())
    }

    /// Takes back agent turn `turn` as one user step; later edits stay.
    pub fn revert(&mut self, turn: &str) -> String {
        self.after(|e| e.revert(turn))
    }

    pub fn undo(&mut self) -> String {
        self.after(Engine::undo)
    }

    pub fn redo(&mut self) -> String {
        self.after(Engine::redo)
    }

    /// The canonical design.json text (`ig_design::to_text`).
    pub fn board(&self) -> String {
        to_text(&self.engine.board().design)
    }

    /// The host wrote this (a `board()` text, or screens: see `Part`) to disk: later merges
    /// start from it.
    pub fn saved(&mut self, written: &str) -> Result<(), String> {
        let b = over(self.engine.merge_base(), part(written)?);
        self.engine.saved(b);
        Ok(())
    }

    /// Theme colours: `{name: [r, g, b, a]}` (0..1).
    pub fn palette(&mut self, json: &str) -> Result<(), String> {
        self.palette = serde_json::from_str(json).map_err(|e| e.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// The canvas size in CSS px and its device pixel ratio.
    pub fn resize(&mut self, w: f32, h: f32, dpr: f32) {
        self.size = (w.max(1.0), h.max(1.0), dpr.max(1.0));
        self.dirty = true;
    }

    /// Draws when something changed since the last frame; says whether it drew.
    pub fn frame(&mut self) -> bool {
        if !self.dirty {
            return false;
        }
        let Some(r) = self.renderer.as_mut() else {
            return false;
        };
        let (w, h, dpr) = self.size;
        let cam = self.engine.camera();
        r.draw(
            self.engine.scene([0.0, 0.0, w, h]),
            cam,
            (w, h),
            dpr,
            &self.palette,
        );
        self.dirty = false;
        true
    }

    pub fn camera(&self) -> String {
        json!(self.engine.camera()).to_string()
    }

    pub fn labels(&self) -> String {
        json!(self.engine.labels()).to_string()
    }

    pub fn a11y(&self) -> String {
        json!(self.engine.a11y()).to_string()
    }

    #[wasm_bindgen(js_name = setImage)]
    pub fn set_image(&mut self, asset: &str, bitmap: ImageBitmap) -> Result<(), String> {
        if let Some(r) = self.renderer.as_mut() {
            r.set_image(asset, &bitmap)?;
        }
        self.engine
            .set_image(asset, bitmap.width(), bitmap.height());
        self.bitmaps.insert(asset.into(), bitmap);
        self.dirty = true;
        Ok(())
    }

    /// The image is gone (no screen shows it, or its file changed): its texture is freed and
    /// its bitmap closed; it draws as a placeholder until set again.
    #[wasm_bindgen(js_name = dropImage)]
    pub fn drop_image(&mut self, asset: &str) {
        if let Some(r) = self.renderer.as_mut() {
            r.drop_image(asset);
        }
        if let Some(b) = self.bitmaps.remove(asset) {
            b.close();
        }
        self.engine.drop_image(asset);
        self.dirty = true;
    }
}

impl BoardEngine {
    fn after(&mut self, f: impl FnOnce(&mut Engine) -> Out) -> String {
        let o = f(&mut self.engine);
        self.dirty |= o.redraw || o.changed;
        out_json(&o)
    }
}
