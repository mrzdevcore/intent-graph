//! Draws a `Scene` with the three programs. Everything here is rebuilt from the in-memory scene
//! and the kept bitmaps after a lost context comes back.

use super::programs::*;
use crate::{Camera, Line, Paint, Rect, Scene, Severity};
use ig_design::{ElementKind, LinkKind, MarkKind};
use js_sys::Float32Array;
use std::collections::HashMap;
use web_sys::{
    ImageBitmap, WebGl2RenderingContext as G, WebGlBuffer, WebGlProgram, WebGlTexture,
    WebGlUniformLocation, WebGlVertexArrayObject,
};

pub type Rgba = [f32; 4];

/// Theme colours by name (`lane`, `element.route`, `selected`, …), resolved by the host from
/// the CSS tokens.
pub type Palette = HashMap<String, Rgba>;

struct Prog {
    p: WebGlProgram,
    vao: WebGlVertexArrayObject,
    buf: WebGlBuffer,
    cam: Option<WebGlUniformLocation>,
    view: Option<WebGlUniformLocation>,
}

pub struct Renderer {
    gl: G,
    rects: Prog,
    lines: Prog,
    quads: Prog,
    quad_rect: Option<WebGlUniformLocation>,
    textures: HashMap<String, WebGlTexture>,
}

const CORNERS: [f32; 8] = [0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0];
const RECT_FLOATS: i32 = 14;
const LINE_FLOATS: i32 = 10;

fn prog(
    gl: &G,
    vs: &str,
    fs: &str,
    layout: &[(u32, i32, i32)],
    stride: i32,
) -> Result<Prog, String> {
    let p = program(gl, vs, fs)?;
    let vao = gl
        .create_vertex_array()
        .ok_or("create_vertex_array failed")?;
    gl.bind_vertex_array(Some(&vao));
    let corners = gl.create_buffer().ok_or("create_buffer failed")?;
    gl.bind_buffer(G::ARRAY_BUFFER, Some(&corners));
    gl.buffer_data_with_array_buffer_view(
        G::ARRAY_BUFFER,
        &Float32Array::from(&CORNERS[..]),
        G::STATIC_DRAW,
    );
    gl.enable_vertex_attrib_array(0);
    gl.vertex_attrib_pointer_with_i32(0, 2, G::FLOAT, false, 0, 0);
    let buf = gl.create_buffer().ok_or("create_buffer failed")?;
    gl.bind_buffer(G::ARRAY_BUFFER, Some(&buf));
    for &(loc, size, offset) in layout {
        gl.enable_vertex_attrib_array(loc);
        gl.vertex_attrib_pointer_with_i32(loc, size, G::FLOAT, false, stride * 4, offset * 4);
        gl.vertex_attrib_divisor(loc, 1);
    }
    gl.bind_vertex_array(None);
    let (cam, view) = (
        gl.get_uniform_location(&p, "cam"),
        gl.get_uniform_location(&p, "view"),
    );
    Ok(Prog {
        p,
        vao,
        buf,
        cam,
        view,
    })
}

fn with(c: Rgba, a: f32) -> Rgba {
    [c[0], c[1], c[2], c[3] * a]
}

const NONE: Rgba = [0.0; 4];

impl Renderer {
    pub fn new(gl: G) -> Result<Self, String> {
        let rects = prog(
            &gl,
            RECT_VS,
            RECT_FS,
            &[(1, 4, 0), (2, 4, 4), (3, 4, 8), (4, 2, 12)],
            RECT_FLOATS,
        )?;
        let lines = prog(
            &gl,
            LINE_VS,
            LINE_FS,
            &[(1, 4, 0), (2, 4, 4), (3, 2, 8)],
            LINE_FLOATS,
        )?;
        let quads = prog(&gl, QUAD_VS, QUAD_FS, &[], 0)?;
        let quad_rect = gl.get_uniform_location(&quads.p, "rect");
        Ok(Renderer {
            gl,
            rects,
            lines,
            quads,
            quad_rect,
            textures: HashMap::new(),
        })
    }

    pub fn set_image(&mut self, asset: &str, bitmap: &ImageBitmap) -> Result<(), String> {
        let gl = &self.gl;
        let t = gl.create_texture().ok_or("create_texture failed")?;
        gl.bind_texture(G::TEXTURE_2D, Some(&t));
        gl.tex_image_2d_with_u32_and_u32_and_image_bitmap(
            G::TEXTURE_2D,
            0,
            G::RGBA as i32,
            G::RGBA,
            G::UNSIGNED_BYTE,
            bitmap,
        )
        .map_err(|e| format!("{e:?}"))?;
        gl.tex_parameteri(G::TEXTURE_2D, G::TEXTURE_MIN_FILTER, G::LINEAR as i32);
        gl.tex_parameteri(G::TEXTURE_2D, G::TEXTURE_WRAP_S, G::CLAMP_TO_EDGE as i32);
        gl.tex_parameteri(G::TEXTURE_2D, G::TEXTURE_WRAP_T, G::CLAMP_TO_EDGE as i32);
        if let Some(old) = self.textures.insert(asset.into(), t) {
            gl.delete_texture(Some(&old));
        }
        Ok(())
    }

    fn uniforms(&self, p: &Prog, cam: Camera, size: (f32, f32)) {
        self.gl.use_program(Some(&p.p));
        self.gl.uniform3f(p.cam.as_ref(), cam.x, cam.y, cam.zoom);
        self.gl.uniform2f(p.view.as_ref(), size.0, size.1);
        self.gl.bind_vertex_array(Some(&p.vao));
    }

    fn instances(&self, p: &Prog, cam: Camera, size: (f32, f32), data: &[f32], floats: i32) {
        if data.is_empty() {
            return;
        }
        self.uniforms(p, cam, size);
        self.gl.bind_buffer(G::ARRAY_BUFFER, Some(&p.buf));
        self.gl.buffer_data_with_array_buffer_view(
            G::ARRAY_BUFFER,
            &Float32Array::from(data),
            G::DYNAMIC_DRAW,
        );
        let n = data.len() as i32 / floats;
        self.gl.draw_arrays_instanced(G::TRIANGLE_STRIP, 0, 4, n);
    }

    /// `size` is the canvas in CSS px, `dpr` its device pixel ratio.
    pub fn draw(&self, s: &Scene, cam: Camera, size: (f32, f32), dpr: f32, pal: &Palette) {
        let gl = &self.gl;
        gl.viewport(0, 0, (size.0 * dpr) as i32, (size.1 * dpr) as i32);
        gl.clear_color(0.0, 0.0, 0.0, 0.0);
        gl.clear(G::COLOR_BUFFER_BIT);
        gl.enable(G::BLEND);
        gl.blend_func(G::ONE, G::ONE_MINUS_SRC_ALPHA);
        let c = |k: &str| pal.get(k).copied().unwrap_or([0.5, 0.5, 0.5, 1.0]);
        // links go over lanes, context and frames, under the other elements
        let split = (s.rects.iter())
            .position(|r| matches!(r.paint, Paint::Element(k) if k != ElementKind::Frame))
            .unwrap_or(s.rects.len());
        let rects = |rs: &[Rect]| -> Vec<f32> {
            let mut v = Vec::with_capacity(rs.len() * RECT_FLOATS as usize);
            for r in rs {
                let (fill, stroke, radius, border) = rect_style(r.paint, &c);
                v.extend_from_slice(&r.r);
                v.extend_from_slice(&fill);
                v.extend_from_slice(&stroke);
                v.extend_from_slice(&[radius, border]);
            }
            v
        };
        let lines = |ls: &[Line]| -> Vec<f32> {
            let mut v = Vec::with_capacity(ls.len() * LINE_FLOATS as usize);
            for l in ls {
                let (color, width, dash) = line_style(l.paint, &c);
                v.extend_from_slice(&[l.a[0], l.a[1], l.b[0], l.b[1]]);
                v.extend_from_slice(&color);
                v.extend_from_slice(&[width, dash]);
            }
            v
        };
        self.instances(
            &self.rects,
            cam,
            size,
            &rects(&s.rects[..split]),
            RECT_FLOATS,
        );
        self.instances(&self.lines, cam, size, &lines(&s.lines), LINE_FLOATS);
        self.instances(
            &self.rects,
            cam,
            size,
            &rects(&s.rects[split..]),
            RECT_FLOATS,
        );
        self.quads(&[], cam, size);
        gl.bind_vertex_array(None);
    }

    /// Images (Task 14 fills the scene's quads): one draw per texture.
    fn quads(&self, qs: &[([f32; 4], &str)], cam: Camera, size: (f32, f32)) {
        for (r, asset) in qs {
            let Some(t) = self.textures.get(*asset) else {
                continue;
            };
            self.uniforms(&self.quads, cam, size);
            self.gl.bind_texture(G::TEXTURE_2D, Some(t));
            self.gl
                .uniform4f(self.quad_rect.as_ref(), r[0], r[1], r[2], r[3]);
            self.gl.draw_arrays(G::TRIANGLE_STRIP, 0, 4);
        }
    }
}

fn kind(k: ElementKind) -> &'static str {
    match k {
        ElementKind::Module => "element.module",
        ElementKind::Function => "element.function",
        ElementKind::Route => "element.route",
        ElementKind::System => "element.system",
        ElementKind::Note => "element.note",
        ElementKind::Frame => "element.frame",
    }
}

fn severity(s: Severity) -> &'static str {
    match s {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Note => "note",
    }
}

/// Fill, stroke, corner radius (world px) and border (CSS px).
fn rect_style(p: Paint, c: &dyn Fn(&str) -> Rgba) -> (Rgba, Rgba, f32, f32) {
    match p {
        Paint::Lane => (c("lane"), NONE, 0.0, 0.0),
        Paint::Context => (c("context"), c("contextBorder"), 6.0, 1.0),
        Paint::Element(ElementKind::Frame) => (NONE, c(kind(ElementKind::Frame)), 10.0, 1.0),
        Paint::Element(ElementKind::Note) => {
            let k = c(kind(ElementKind::Note));
            (with(k, 0.18), k, 2.0, 1.0)
        }
        Paint::Element(k) => (c("surface"), c(kind(k)), 8.0, 1.5),
        Paint::Mark(MarkKind::Changed) => (NONE, c("mark.changed"), 8.0, 2.0),
        Paint::Mark(MarkKind::Remove) => (NONE, c("mark.remove"), 8.0, 2.0),
        Paint::Selected => (NONE, c("selected"), 8.0, 2.0),
        Paint::Highlight => (with(c("highlight"), 0.1), c("highlight"), 8.0, 2.5),
        Paint::Problem(s) => (NONE, c(severity(s)), 8.0, 2.0),
        Paint::Marquee => (with(c("selected"), 0.08), c("selected"), 0.0, 1.0),
        Paint::ContextEdge | Paint::Link(_) | Paint::Preview => (NONE, c("link"), 0.0, 1.0),
    }
}

/// Colour, width and dash (CSS px).
fn line_style(p: Paint, c: &dyn Fn(&str) -> Rgba) -> (Rgba, f32, f32) {
    match p {
        Paint::ContextEdge => (c("contextEdge"), 1.0, 0.0),
        Paint::Link(LinkKind::Imports) => (c("link"), 1.5, 5.0),
        Paint::Link(_) => (c("link"), 1.5, 0.0),
        Paint::Selected => (with(c("selected"), 0.6), 4.0, 0.0),
        Paint::Highlight => (with(c("highlight"), 0.7), 4.0, 0.0),
        Paint::Problem(s) => (c(severity(s)), 2.0, 0.0),
        Paint::Preview => (c("selected"), 1.5, 4.0),
        _ => (c("link"), 1.0, 0.0),
    }
}
