// The graph's edge program: sigma's clamped edge (it stops short of the target for the arrow head)
// with a dash pattern in screen pixels, from the edge's `dash` attribute (0 draws a solid line).
// Planned, removed, HTTP and guessed edges use it; the arrow head is sigma's.

import { createEdgeCompoundProgram, EdgeArrowHeadProgram, EdgeProgram } from "sigma/rendering";
import type { RenderParams } from "sigma/types";
import { floatColor } from "sigma/utils";
import type { EdgeDisplayData, NodeDisplayData } from "sigma/types";

const VERTEX = /*glsl*/ `
attribute vec4 a_id;
attribute vec4 a_color;
attribute vec2 a_normal;
attribute float a_normalCoef;
attribute vec2 a_positionStart;
attribute vec2 a_positionEnd;
attribute float a_positionCoef;
attribute float a_radius;
attribute float a_radiusCoef;
attribute float a_dash;
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
void main() {
  float radius = a_radius * a_radiusCoef;
  vec2 normal = a_normal * a_normalCoef;
  vec2 position = a_positionStart * (1.0 - a_positionCoef) + a_positionEnd * a_positionCoef;
  float normalLength = length(normal);
  vec2 unitNormal = normal / normalLength;
  float pixelsThickness = max(normalLength, u_minEdgeThickness * u_sizeRatio);
  float webGLThickness = pixelsThickness * u_correctionRatio / u_sizeRatio;
  float direction = sign(radius);
  float webGLNodeRadius = direction * radius * 2.0 * u_correctionRatio / u_sizeRatio;
  float webGLArrowHeadLength = webGLThickness * arrowHead * 2.0;
  vec2 compensationVector = vec2(-direction * unitNormal.y, direction * unitNormal.x) * (webGLNodeRadius + webGLArrowHeadLength);
  gl_Position = vec4((u_matrix * vec3(position + unitNormal * webGLThickness + compensationVector, 1)).xy, 0, 1);
  vec2 s = (u_matrix * vec3(a_positionStart, 1)).xy;
  vec2 e = (u_matrix * vec3(a_positionEnd, 1)).xy;
  v_along = a_positionCoef * length((e - s) * u_dimensions * 0.5);
  v_dash = a_dash;
  v_thickness = webGLThickness / u_zoomRatio;
  v_normal = unitNormal;
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
  gl_FragColor = mix(v_color, vec4(0.0), t);
  #endif
}`;

const UNIFORMS = ["u_matrix", "u_zoomRatio", "u_sizeRatio", "u_correctionRatio", "u_pixelRatio", "u_feather", "u_minEdgeThickness", "u_dimensions"] as const;
const { FLOAT, UNSIGNED_BYTE, TRIANGLES } = WebGLRenderingContext;

class EdgeDashProgram extends EdgeProgram<(typeof UNIFORMS)[number]> {
  getDefinition() {
    return {
      VERTICES: 6, VERTEX_SHADER_SOURCE: VERTEX, FRAGMENT_SHADER_SOURCE: FRAGMENT, METHOD: TRIANGLES, UNIFORMS,
      ATTRIBUTES: [
        { name: "a_positionStart", size: 2, type: FLOAT }, { name: "a_positionEnd", size: 2, type: FLOAT },
        { name: "a_normal", size: 2, type: FLOAT }, { name: "a_color", size: 4, type: UNSIGNED_BYTE, normalized: true },
        { name: "a_id", size: 4, type: UNSIGNED_BYTE, normalized: true }, { name: "a_radius", size: 1, type: FLOAT },
        { name: "a_dash", size: 1, type: FLOAT },
      ],
      CONSTANT_ATTRIBUTES: [{ name: "a_positionCoef", size: 1, type: FLOAT }, { name: "a_normalCoef", size: 1, type: FLOAT },
        { name: "a_radiusCoef", size: 1, type: FLOAT }],
      CONSTANT_DATA: [[0, 1, 0], [0, -1, 0], [1, 1, 1], [1, 1, 1], [0, -1, 0], [1, -1, -1]],
    };
  }

  processVisibleItem(edgeIndex: number, i: number, src: NodeDisplayData, dst: NodeDisplayData, data: EdgeDisplayData) {
    const t = data.size || 1;
    const [dx, dy] = [dst.x - src.x, dst.y - src.y];
    const len = Math.hypot(dx, dy);
    const a = this.array;
    a[i++] = src.x;
    a[i++] = src.y;
    a[i++] = dst.x;
    a[i++] = dst.y;
    a[i++] = len ? (-dy / len) * t : 0;
    a[i++] = len ? (dx / len) * t : 0;
    a[i++] = floatColor(data.color);
    a[i++] = edgeIndex;
    a[i++] = dst.size || 1;
    a[i++] = (data as EdgeDisplayData & { dash?: number }).dash || 0;
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
}

/** Dashed or solid line, then sigma's arrow head. */
export const EdgeProgramWithDash = createEdgeCompoundProgram([EdgeDashProgram, EdgeArrowHeadProgram]);
