//! The wasm face of the engine (wasm32 only): values cross as JSON strings, the renderer draws
//! on a WebGL2 canvas. The TypeScript side codes against `viewer/src/board/engine.d.ts`.

mod gl;
mod programs;

use crate::{Context, Engine, Input, Out, Severity, Step, Tool};
use gl::{Palette, Renderer};
use ig_design::{Board, Op, Scope, empty, parse, to_text};
use serde::Deserialize;
use serde_json::json;
use std::cell::RefCell;
use std::collections::HashMap;
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
    Tool {
        tool: String,
    },
}

fn tool(s: &str) -> Option<Tool> {
    Some(match s {
        "select" => Tool::Select,
        "module" => Tool::Module,
        "function" => Tool::Function,
        "route" => Tool::Route,
        "system" => Tool::System,
        "note" => Tool::Note,
        "frame" => Tool::Frame,
        "link" => Tool::Link,
        _ => return None,
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum StepIn {
    User,
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
    })
    .to_string()
}

fn error_json(e: impl ToString) -> String {
    json!({"redraw": false, "changed": false, "meaning": false, "selection": [], "edit": null,
           "placed": null, "cursor": "default", "error": e.to_string()})
    .to_string()
}

fn board_of(text: &str) -> Result<Board, String> {
    parse(text)
        .map(|design| Board { design })
        .map_err(|e| e.to_string())
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
        let blank = empty("", Scope::Dir { id: String::new() });
        BoardEngine {
            engine: Engine::new(Context::default(), Board { design: blank }),
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

    pub fn load(&mut self, board: &str, context: &str) -> Result<(), String> {
        let ctx: Context = serde_json::from_str(context).map_err(|e| e.to_string())?;
        self.engine = Engine::new(ctx, board_of(board)?);
        self.dirty = true;
        Ok(())
    }

    pub fn input(&mut self, ev: &str) -> String {
        let i = match serde_json::from_str::<Ev>(ev) {
            Ok(Ev::Down { x, y, button, mods }) => Input::Down { x, y, button, mods },
            Ok(Ev::Move { x, y, mods }) => Input::Move { x, y, mods },
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

    /// One history step; `step` is `"user"` or `{"agent": turn}`.
    pub fn ops(&mut self, ops: &str, step: &str) -> String {
        let ops: Vec<Op> = match serde_json::from_str(ops) {
            Ok(o) => o,
            Err(e) => return error_json(e),
        };
        let step = match serde_json::from_str(step) {
            Ok(StepIn::User) => Step::User,
            Ok(StepIn::Agent(t)) => Step::Agent(t),
            Err(e) => return error_json(e),
        };
        self.after(|e| e.ops(&ops, step))
    }

    /// The file changed on disk: merged in (an agent step when `turn` is set).
    pub fn external(&mut self, disk: &str, turn: Option<String>) -> String {
        match board_of(disk) {
            Ok(b) => self.after(|e| e.external(b, turn.as_deref())),
            Err(e) => error_json(e),
        }
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

    /// The host wrote this text (a `board()` result) to disk: later merges start from it.
    pub fn saved(&mut self, written: &str) -> Result<(), String> {
        self.engine.saved(board_of(written)?);
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
        self.bitmaps.insert(asset.into(), bitmap);
        self.dirty = true;
        Ok(())
    }
}

impl BoardEngine {
    fn after(&mut self, f: impl FnOnce(&mut Engine) -> Out) -> String {
        let o = f(&mut self.engine);
        self.dirty |= o.redraw || o.changed;
        out_json(&o)
    }
}
