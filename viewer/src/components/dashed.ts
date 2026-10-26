// The graph's edge programs: sigma's clamped edge (it stops short of the target for the arrow head),
// straight or bent by the edge's `curve`, with a dash pattern in screen pixels from the edge's `dash`
// attribute (0 draws a solid line), and sigma's arrow head. Both shift `offset` pixels to the edge's
// left, so the two edges of an opposite pair (A→B, B→A) run side by side instead of on one line.

import { createEdgeCompoundProgram, EdgeArrowHeadProgram, EdgeProgram } from "sigma/rendering";
import type { RenderParams } from "sigma/types";
import { floatColor } from "sigma/utils";
import { guide } from "../model/sigma";
import type { EdgeDisplayData, NodeDisplayData } from "sigma/types";

// Each edge is `segments` quads along a quadratic Bézier from the source to the arrow head's base,
// its control point `a_curve` × the length to the left of the middle: 0 draws the straight line.
const VERTEX = /*glsl*/ `
attribute vec4 a_id;
attribute vec4 a_color;
attribute vec2 a_normal;
attribute vec2 a_positionStart;
attribute vec2 a_positionEnd;
attribute float a_radius;
attribute float a_dash;
attribute float a_offset;
attribute float a_curve;
attribute float a_t;
attribute float a_side;
uniform mat3 u_matrix;
uniform float u_zoomRatio;
uniform float u_sizeRatio;
uniform float u_pixelRatio;
uniform float u_correctionRatio;
uniform float u_minEdgeThickness;
uniform float u_feather;
uniform vec2 u_dimensions;
varying vec4 v_color;
varying vec2 v_normal;
varying float v_thickness;
varying float v_feather;
varying float v_along;
varying float v_dash;
const float bias = 255.0 / 254.0;
const float arrowHead = 2.5;
// A unit vector, or zero for a zero vector (a node linked to itself, a degenerate tangent): no NaN.
vec2 unit(vec2 v) {
  float l = length(v);
  return l > 1e-9 ? v / l : vec2(0.0);
}
void main() {
  vec2 p0 = a_positionStart;
  vec2 p2 = a_positionEnd;
  vec2 chord = p2 - p0;
  vec2 left = unit(vec2(-chord.y, chord.x));
  float pixelsThickness = max(length(a_normal), u_minEdgeThickness * u_sizeRatio);
  float webGLThickness = pixelsThickness * u_correctionRatio / u_sizeRatio;
  // The control point (the arrow head aims from it, see guide()), then the line's end at the head's
  // base: the curve's end tangent, end - control, is then parallel to the head (no kink).
  vec2 c = (p0 + p2) * 0.5 + left * a_curve * length(chord);
  vec2 into = unit(p2 - c);
  vec2 end = p2 - into * (a_radius * 2.0 * u_correctionRatio / u_sizeRatio + webGLThickness * arrowHead * 2.0);
  // A straight line keeps its control in the middle, so its dashes run at an even pace.
  if (a_curve == 0.0) c = (p0 + end) * 0.5;
  float t = a_t;
  vec2 position = (1.0 - t) * (1.0 - t) * p0 + 2.0 * (1.0 - t) * t * c + t * t * end;
  vec2 tangent = (1.0 - t) * (c - p0) + t * (end - c);
  vec2 normal = unit(vec2(-tangent.y, tangent.x));
  if (normal == vec2(0.0)) normal = left;
  #ifdef PICKING_MODE
  // Thin edges are picked over at least 4 px each side, so they can be hovered.
  webGLThickness = max(webGLThickness, 4.0 * u_correctionRatio);
  #endif
  // An opposite pair's shift, square to the arrow's direction so line and head stay together.
  vec2 shift = vec2(-into.y, into.x) * a_offset * u_correctionRatio;
  gl_Position = vec4((u_matrix * vec3(position + shift + normal * a_side * webGLThickness, 1)).xy, 0, 1);
  vec2 s = (u_matrix * vec3(p0, 1)).xy;
  vec2 e = (u_matrix * vec3(end, 1)).xy;
  v_along = t * length((e - s) * u_dimensions * 0.5);
  v_dash = a_dash;
  v_thickness = webGLThickness / u_zoomRatio;
  v_normal = vec2(a_side, 0.0);
  v_feather = u_feather * u_correctionRatio / u_zoomRatio / u_pixelRatio * 2.0;
  #ifdef PICKING_MODE
  v_color = a_id;
  #else
  v_color = a_color;
  #endif
  v_color.a *= bias;
}`;

const FRAGMENT = /*glsl*/ `
precision mediump float;
varying vec4 v_color;
varying vec2 v_normal;
varying float v_thickness;
varying float v_feather;
varying float v_along;
varying float v_dash;
void main(void) {
  #ifdef PICKING_MODE
  gl_FragColor = v_color;
  #else
  if (v_dash > 0.0 && mod(v_along, 2.0 * v_dash) > v_dash) discard;
  float t = smoothstep(v_thickness - v_feather, v_thickness, length(v_normal) * v_thickness);
  // Premultiplied first (sigma blends ONE, ONE_MINUS_SRC_ALPHA), then faded at the rim: no dark fringe.
  gl_FragColor = mix(vec4(v_color.rgb * v_color.a, v_color.a), vec4(0.0), t);
  #endif
}`;

const UNIFORMS = ["u_matrix", "u_zoomRatio", "u_sizeRatio", "u_correctionRatio", "u_pixelRatio", "u_feather", "u_minEdgeThickness", "u_dimensions"] as const;
const { FLOAT, UNSIGNED_BYTE, TRIANGLES } = WebGLRenderingContext;

type Extra = EdgeDisplayData & { dash?: number; offset?: number; curve?: number };

/** The line program, drawn as `segments` quads (1 for straight lines). */
function lineProgram(segments: number) {
  const steps: number[][] = [];
  for (let k = 0; k < segments; k++) {
    const [t0, t1] = [k / segments, (k + 1) / segments];
    steps.push([t0, 1], [t0, -1], [t1, 1], [t1, 1], [t0, -1], [t1, -1]);
  }
  return class EdgeDashProgram extends EdgeProgram<(typeof UNIFORMS)[number]> {
    getDefinition() {
      return {
        VERTICES: 6 * segments, VERTEX_SHADER_SOURCE: VERTEX, FRAGMENT_SHADER_SOURCE: FRAGMENT, METHOD: TRIANGLES, UNIFORMS,
        ATTRIBUTES: [
          { name: "a_positionStart", size: 2, type: FLOAT }, { name: "a_positionEnd", size: 2, type: FLOAT },
          { name: "a_normal", size: 2, type: FLOAT }, { name: "a_color", size: 4, type: UNSIGNED_BYTE, normalized: true },
          { name: "a_id", size: 4, type: UNSIGNED_BYTE, normalized: true }, { name: "a_radius", size: 1, type: FLOAT },
          { name: "a_dash", size: 1, type: FLOAT }, { name: "a_offset", size: 1, type: FLOAT }, { name: "a_curve", size: 1, type: FLOAT },
        ],
        CONSTANT_ATTRIBUTES: [{ name: "a_t", size: 1, type: FLOAT }, { name: "a_side", size: 1, type: FLOAT }],
        CONSTANT_DATA: steps,
      };
    }

    processVisibleItem(edgeIndex: number, i: number, src: NodeDisplayData, dst: NodeDisplayData, data: EdgeDisplayData) {
      const t = data.size || 1;
      const [dx, dy] = [dst.x - src.x, dst.y - src.y];
      const len = Math.hypot(dx, dy);
      const a = this.array;
      const d = data as Extra;
      a[i++] = src.x;
      a[i++] = src.y;
      a[i++] = dst.x;
      a[i++] = dst.y;
      a[i++] = len ? (-dy / len) * t : 0;
      a[i++] = len ? (dx / len) * t : 0;
      a[i++] = floatColor(data.color);
      a[i++] = edgeIndex;
      a[i++] = dst.size || 1;
      a[i++] = d.dash || 0;
      a[i++] = d.offset || 0;
      a[i++] = d.curve || 0;
    }

    setUniforms(p: RenderParams, { gl, uniformLocations: u }: { gl: WebGLRenderingContext; uniformLocations: Record<string, WebGLUniformLocation> }) {
      gl.uniformMatrix3fv(u.u_matrix, false, p.matrix);
      gl.uniform1f(u.u_zoomRatio, p.zoomRatio);
      gl.uniform1f(u.u_sizeRatio, p.sizeRatio);
      gl.uniform1f(u.u_correctionRatio, p.correctionRatio);
      gl.uniform1f(u.u_pixelRatio, p.pixelRatio);
      gl.uniform1f(u.u_feather, p.antiAliasingFeather);
      gl.uniform1f(u.u_minEdgeThickness, p.minEdgeThickness);
      gl.uniform2f(u.u_dimensions, p.width, p.height);
    }
  };
}
const EdgeDashProgram = lineProgram(1);

/** sigma blends with (ONE, ONE_MINUS_SRC_ALPHA), for premultiplied colours: a translucent edge
 *  drawn straight would come out lighter than the background (white lines on the light theme). */
const PREMULTIPLIED = "gl_FragColor = v_color;\n  #ifndef PICKING_MODE\n  gl_FragColor.rgb *= gl_FragColor.a;\n  #endif";

/** sigma's arrow head, shifted like its line (its `a_normal` points the other way). */
// Typed opaque by sigma: seen as a concrete edge program of the same shape.
const ArrowHeadBase = EdgeArrowHeadProgram as unknown as new (...a: ConstructorParameters<typeof EdgeDashProgram>) => InstanceType<typeof EdgeDashProgram>;

class ArrowHeadProgram extends ArrowHeadBase {
  getDefinition() {
    const d = super.getDefinition();
    return {
      ...d,
      VERTEX_SHADER_SOURCE: d.VERTEX_SHADER_SOURCE.replace("attribute float a_radius;", "attribute float a_radius;\nattribute float a_offset;")
        .replace("vec3(a_position + delta, 1)", "vec3(a_position + delta - normalize(a_normal) * a_offset * u_correctionRatio, 1)"),
      FRAGMENT_SHADER_SOURCE: d.FRAGMENT_SHADER_SOURCE.replace("gl_FragColor = v_color;", PREMULTIPLIED),
      ATTRIBUTES: [...d.ATTRIBUTES, { name: "a_offset", size: 1, type: FLOAT }],
    };
  }

  processVisibleItem(edgeIndex: number, i: number, src: NodeDisplayData, dst: NodeDisplayData, data: EdgeDisplayData) {
    // A curve's head points along its end: from the control point, not the source.
    const curve = (data as Extra).curve || 0;
    super.processVisibleItem(edgeIndex, i, curve ? { ...src, ...guide(src, dst, curve) } : src, dst, data);
    this.array[i + 7] = (data as Extra).offset || 0; // after its 7 floats
  }
}

/** Dashed or solid line, then the arrow head. */
export const EdgeProgramWithDash = createEdgeCompoundProgram([EdgeDashProgram, ArrowHeadProgram]);
/** The same, bent by the edge's `curve` (skip-row edges). */
export const CurvedEdgeProgram = createEdgeCompoundProgram([lineProgram(16), ArrowHeadProgram]);
