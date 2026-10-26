//! The three WebGL2 programs: instanced rounded rectangles with a border, thick lines as quads
//! with round caps and an optional dash, and textured quads. Positions are world px; the camera
//! (`cam` = x, y, zoom) and the canvas size in CSS px (`view`) map them to clip space, so the
//! device pixel ratio only sizes the backing store.

use web_sys::{WebGl2RenderingContext as G, WebGlProgram, WebGlShader};

const CLIP: &str = "
uniform vec3 cam;
uniform vec2 view;
vec4 clip(vec2 s) { return vec4(s.x / view.x * 2.0 - 1.0, 1.0 - s.y / view.y * 2.0, 0.0, 1.0); }
";

/// Per instance: rect (world), fill, stroke (straight alpha), radius (world px) and border (CSS px).
pub const RECT_VS: &str = "
layout(location = 0) in vec2 corner;
layout(location = 1) in vec4 rect;
layout(location = 2) in vec4 fill;
layout(location = 3) in vec4 stroke;
layout(location = 4) in vec2 style;
out vec2 p; out vec2 halfsize; out vec4 vfill; out vec4 vstroke; out vec2 vstyle;
void main() {
  halfsize = rect.zw * cam.z * 0.5;
  p = (corner * 2.0 - 1.0) * (halfsize + 1.0);
  vec2 center = (rect.xy - cam.xy) * cam.z + halfsize;
  vfill = fill; vstroke = stroke; vstyle = vec2(style.x * cam.z, style.y);
  gl_Position = clip(center + p);
}";

pub const RECT_FS: &str = "
in vec2 p; in vec2 halfsize; in vec4 vfill; in vec4 vstroke; in vec2 vstyle;
out vec4 o;
void main() {
  float r = min(vstyle.x, min(halfsize.x, halfsize.y));
  vec2 q = abs(p) - halfsize + r;
  float d = length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;
  float aa = max(fwidth(d), 0.0001);
  float outside = smoothstep(-aa * 0.5, aa * 0.5, d);
  float inner = smoothstep(-aa * 0.5, aa * 0.5, d + vstyle.y);
  vec4 f = vec4(vfill.rgb * vfill.a, vfill.a);
  vec4 s = vec4(vstroke.rgb * vstroke.a, vstroke.a);
  o = mix(f, s, vstyle.y > 0.0 ? inner : 0.0) * (1.0 - outside);
}";

/// Per instance: ends a and b (world), colour, width and dash (CSS px; 0 is solid).
pub const LINE_VS: &str = "
layout(location = 0) in vec2 corner;
layout(location = 1) in vec4 ends;
layout(location = 2) in vec4 color;
layout(location = 3) in vec2 style;
out float along; out float across; out float len; out vec4 vcolor; out vec2 vstyle;
void main() {
  vec2 a = (ends.xy - cam.xy) * cam.z;
  vec2 b = (ends.zw - cam.xy) * cam.z;
  len = length(b - a);
  vec2 dir = len > 0.0 ? (b - a) / len : vec2(1.0, 0.0);
  vec2 n = vec2(-dir.y, dir.x);
  float r = style.x * 0.5 + 1.0;
  along = mix(-r, len + r, corner.x);
  across = (corner.y * 2.0 - 1.0) * r;
  vcolor = color; vstyle = style;
  gl_Position = clip(a + dir * along + n * across);
}";

pub const LINE_FS: &str = "
in float along; in float across; in float len; in vec4 vcolor; in vec2 vstyle;
out vec4 o;
void main() {
  if (vstyle.y > 0.0 && mod(along, vstyle.y * 2.0) > vstyle.y) discard;
  float dx = max(max(-along, along - len), 0.0);
  float d = length(vec2(dx, across)) - vstyle.x * 0.5;
  float a = vcolor.a * (1.0 - smoothstep(-0.5, 0.5, d));
  o = vec4(vcolor.rgb * a, a);
}";

/// One quad per draw: its world rect and texture (images, Task 14).
pub const QUAD_VS: &str = "
layout(location = 0) in vec2 corner;
uniform vec4 rect;
out vec2 uv;
void main() {
  uv = corner;
  gl_Position = clip((rect.xy + corner * rect.zw - cam.xy) * cam.z);
}";

pub const QUAD_FS: &str = "
in vec2 uv;
uniform sampler2D tex;
out vec4 o;
void main() { vec4 c = texture(tex, uv); o = vec4(c.rgb * c.a, c.a); }";

fn shader(gl: &G, kind: u32, body: &str) -> Result<WebGlShader, String> {
    let s = gl.create_shader(kind).ok_or("create_shader failed")?;
    let head = "#version 300 es\nprecision highp float;\n";
    let src = if kind == G::VERTEX_SHADER {
        format!("{head}{CLIP}{body}")
    } else {
        format!("{head}{body}")
    };
    gl.shader_source(&s, &src);
    gl.compile_shader(&s);
    if gl
        .get_shader_parameter(&s, G::COMPILE_STATUS)
        .as_bool()
        .unwrap_or(false)
    {
        Ok(s)
    } else {
        Err(gl.get_shader_info_log(&s).unwrap_or_default())
    }
}

pub fn program(gl: &G, vs: &str, fs: &str) -> Result<WebGlProgram, String> {
    let p = gl.create_program().ok_or("create_program failed")?;
    gl.attach_shader(&p, &shader(gl, G::VERTEX_SHADER, vs)?);
    gl.attach_shader(&p, &shader(gl, G::FRAGMENT_SHADER, fs)?);
    gl.link_program(&p);
    if gl
        .get_program_parameter(&p, G::LINK_STATUS)
        .as_bool()
        .unwrap_or(false)
    {
        Ok(p)
    } else {
        Err(gl.get_program_info_log(&p).unwrap_or_default())
    }
}
