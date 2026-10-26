// The graph view's model for sigma.js: the model's elements as a graphology graph (leaves only),
// the background sigma has no notion of (layer rows, group boxes), and the reducers that draw the
// view state (selection, `ig_show`, fading, the Links and Tests toggles) over it.
// Coordinates are the model's (y down); <Graph> flips y at sigma's boundary.

import Graph from "graphology";
import type { Status } from "../data";
import { iconFor, iconUri } from "../icons";
import type { El, ElData } from "./graph";

/** Theme colours; <Graph> fills them from the CSS tokens, these are the light theme's. */
export const palette = {
  added: "#2f9e44", changed: "#d98a00", removed: "#d6336c", unchanged: "#9a9a94", fg: "#1d1d1b", muted: "#6b6b66",
  line: "#e3e2de", error: "#c92a2a", warning: "#d98a00", note: "#4c6ef5", accent: "#E8A317", accentText: "#9a6a00",
  brand: "#E8A317", panel: "#ffffff", bg: "#fbfbfa",
};

/** Reads every colour from its CSS token (`accentText` from `--accent-text`). */
export function setPalette(read: (token: string) => string) {
  (Object.keys(palette) as (keyof typeof palette)[]).forEach((k) => {
    const v = read("--" + k.replace(/[A-Z]/g, (c) => "-" + c.toLowerCase()));
    if (v) palette[k] = v;
  });
}

/** `#rrggbb` at opacity `a` (unchanged at 1). */
export function alpha(color: string, a: number): string {
  if (a >= 1 || !/^#[0-9a-f]{6}$/i.test(color)) return color;
  const n = parseInt(color.slice(1), 16);
  return `rgba(${n >> 16},${(n >> 8) & 255},${n & 255},${a})`;
}

const CLEAR = "rgba(0,0,0,0)";
/** How far apart (pixels, each side) the two edges of an opposite pair run. */
export const OFFSET = 4;
/** Where <Graph>'s 2D canvases sit among sigma's layers: rows and compounds under the edges,
 *  Architecture's boxes over the edges and their count labels (box names are never covered). */
export const LAYERS = { under: "edges", over: "edgeLabels" } as const;
/** The smallest zoom a fit goes to when boxes are drawn: their 13 px names stay at 11 px or more. */
export const MIN_SCALE = 11 / 13;
const EXTERNAL = "External services";

/** An edge's curvature (its control point's offset, as a fraction of its length) when it spans
 *  `rows` rows: 0 between adjacent rows, more the more rows it skips, so skip-row edges fan aside. */
export const curveOf = (rows: number) => (rows > 1 ? Math.min(0.5, 0.12 + 0.08 * (rows - 2)) : 0);

/** Stroke width for a link count: 1 for one, capped at 3 (violations from 2). */
const widthOf = (count: number, violation: boolean) => Math.min(3, (violation ? 2 : 1) + Math.log10(Math.max(1, count)));

export interface NodeAttrs {
  x: number;
  y: number;
  size: number;
  label: string;
  color: string;
  kind: string;
  status: Status;
  planned: boolean;
  test: boolean;
  /** A system's icon (a data URI), drawn by the image program. */
  icon?: string;
  findings?: number;
  outer?: boolean;
  bp?: boolean;
  /** Stands for a group box as an edge end: invisible, under the box's members. */
  anchor?: boolean;
  /** Decorations set by <Graph>: note pins, an open move, `ig_show` step badges. */
  pins?: string;
  ghost?: boolean;
  steps?: string;
  [k: string]: unknown;
}

export interface EdgeAttrs {
  from: string;
  to: string;
  size: number;
  color: string;
  kind: string;
  status: Status;
  planned: boolean;
  violation: boolean;
  guessed?: boolean;
  ctx?: boolean;
  count?: number;
  /** An `ig_show` step badge. */
  step?: string;
  /** Between boxes (Architecture), whose names sigma doesn't draw: the count is drawn anyway. */
  boxed?: boolean;
  /** Pixels to the edge's left: set when the opposite edge exists too. */
  offset?: number;
  /** Bend, from `curveOf`: 0 or absent draws a straight line. */
  curve?: number;
  /** A box ↔ system edge, shown only when one of its ends is selected (or in `ig_show`). */
  sys?: boolean;
  /** One row's system edges folded into one edge to the External services band. */
  band?: boolean;
  /** A band edge's real ends (boxes and systems) and its number of systems. */
  via?: string[];
  systems?: number;
  [k: string]: unknown;
}

/** What the reducers add for sigma and the drawing functions. */
export interface Drawn {
  hidden?: boolean;
  forceLabel?: boolean;
  highlighted?: boolean;
  zIndex?: number;
  type?: string;
  image?: string;
  /** A ring around the node, dashed when `ringDash` is set. */
  ring?: string;
  ringDash?: number;
  labelColor?: string;
  labelSize?: number;
  /** Edges: dash length in pixels, 0 for a solid line. */
  dash?: number;
}

export interface ViewState {
  selected: string | null;
  /** The hovered node: its label forced, its edges forward with their counts. */
  hovered: string | null;
  show: { nodes: Set<string>; edges: Set<string> } | null;
  links: boolean;
  /** Test nodes shown; <Graph> passes true, since App already drops them (`withTests`). */
  tests: boolean;
  /** Fading (a click, a tree folder): only these stay bright. */
  focus: Set<string> | null;
}

export interface Box {
  id: string;
  label: string;
  x0: number;
  y0: number;
  x1: number;
  y1: number;
  status: Status;
  /** "layer" or "dir" for Architecture's boxes, else the compound's kind. */
  kind?: string;
  planned?: boolean;
  outer?: boolean;
  findings?: number;
}
export interface Background {
  /** A row, its name drawn at `x` (left of its boxes). */
  lanes: { label: string; y0: number; y1: number; x?: number }[];
  groups: Box[];
}

const isBox = (k?: string) => k === "layer" || k === "dir";

/** An Architecture box's width (its name and count at 15 px semibold, 20 px each side) and height. */
export function boxSize(d: ElData): { w: number; h: number } {
  const c = Math.min(300, Math.max(1, d.count ?? 1));
  return { w: Math.max(80, (d.label ?? "").length * 9 + 40), h: 40 + ((c - 1) / 299) * 40 };
}

/** A node's radius by kind. */
function sizeOf(d: ElData): number {
  if (isBox(d.kind)) return boxSize(d).h / 2;
  return { module: 10, system: 22, journey: 17, route: 11, laneadd: 11 }[d.kind ?? ""] ?? 7;
}

/** A system's icon with a larger intrinsic size, so the texture is not upscaled. */
function icon(d: ElData): string {
  return iconUri(iconFor(d.label ?? "", d.packages ?? [], d.category ?? "service"), palette.fg)
    .replace("width%3D%2224%22", "width%3D%2296%22").replace("height%3D%2224%22", "height%3D%2296%22");
}

function parentsOf(els: El[]): Set<string> {
  return new Set(els.map((e) => e.data.parent).filter((p): p is string => !!p));
}

const isBoxEnd = (a: NodeAttrs) => !!a.anchor || isBox(a.kind) || a.kind === "system";

export function toGraphology(els: El[]): Graph<NodeAttrs, EdgeAttrs> {
  const g = new Graph<NodeAttrs, EdgeAttrs>({ multi: true });
  const parents = parentsOf(els);
  const ends = new Set(els.flatMap((e) => (e.group === "edges" ? [e.data.source!, e.data.target!] : [])));
  const boxes: Record<string, Box> = {};
  if ([...parents].some((p) => ends.has(p))) background(els).groups.forEach((b) => (boxes[b.id] = b));
  els.forEach((e) => {
    const d = e.data;
    if (e.group !== "nodes" || d.kind === "lane" || d.kind === "lanelabel") return;
    const base = { label: d.label ?? "", kind: d.kind ?? "member", status: d.status ?? "unchanged", planned: !!d.planned, test: !!d.test,
      findings: d.findings ?? 0, outer: !!d.outer, bp: !!d.bp };
    if (parents.has(d.id)) {
      // A compound is drawn on the background; it stays a node only to end edges, at its box's centre.
      const b = boxes[d.id];
      if (!b || !ends.has(d.id)) return;
      g.addNode(d.id, { ...base, x: (b.x0 + b.x1) / 2, y: (b.y0 + b.y1) / 2, size: Math.min(b.x1 - b.x0, b.y1 - b.y0) / 2,
        color: CLEAR, anchor: true });
      return;
    }
    const p = e.position ?? { x: 0, y: 0 };
    g.addNode(d.id, { ...base, x: p.x, y: p.y, size: sizeOf(d), color: palette[d.status ?? "unchanged"] ?? palette.unchanged,
      icon: d.kind === "system" ? icon(d) : undefined });
  });
  els.forEach((e) => {
    const d = e.data;
    if (e.group !== "edges" || !g.hasNode(d.source) || !g.hasNode(d.target)) return;
    g.addEdgeWithKey(d.id, d.source, d.target, {
      from: d.source!, to: d.target!, size: widthOf(d.count ?? 1, !!d.violation), color: palette.muted,
      kind: d.kind ?? "", status: d.status ?? "unchanged", planned: !!d.planned, violation: !!d.violation, guessed: !!d.guessed,
      ctx: !!d.ctx, count: d.count, boxed: isBoxEnd(g.getNodeAttributes(d.source!)) && isBoxEnd(g.getNodeAttributes(d.target!)),
    });
  });
  // Rows (Architecture): system edges fold into one band edge per row, and skip-row edges curve aside.
  const lanes = lanesOf(els);
  if (lanes.length) {
    const rowOf = (id: string) => {
      const y = g.getNodeAttribute(id, "y");
      return lanes.findIndex((l) => y >= l.y0 && y <= l.y1);
    };
    const ext = lanes.findIndex((l) => l.label === EXTERNAL);
    if (ext >= 0) fold(g, lanes, ext, rowOf);
    g.forEachEdge((id, a) => {
      if (g.getNodeAttribute(a.from, "bp") || g.getNodeAttribute(a.to, "bp")) return; // Blueprint keeps straight lines
      const [r, s] = [rowOf(a.from), rowOf(a.to)];
      if (r >= 0 && s >= 0) g.setEdgeAttribute(id, "curve", curveOf(Math.abs(r - s)));
    });
  }
  // Opposite edges run side by side: each shifts to its own left, so the pair lands on both sides.
  g.forEachEdge((id, a) => g.setEdgeAttribute(id, "offset", g.outEdges(a.to, a.from).length ? OFFSET : 0));
  return g;
}

/** Marks the box ↔ system edges `sys` and adds, per row and direction, one band edge between an
 *  invisible anchor at the row's boxes and one at the top of the External services band. */
function fold(g: Graph<NodeAttrs, EdgeAttrs>, lanes: Background["lanes"], ext: number, rowOf: (id: string) => number) {
  type Fold = { row: number; inward: boolean; count: number; via: Set<string>; systems: Set<string>; violation: boolean; ctx: boolean };
  const folds: Record<string, Fold> = {};
  g.forEachEdge((id, a) => {
    const [fs, ts] = [g.getNodeAttribute(a.from, "kind") === "system", g.getNodeAttribute(a.to, "kind") === "system"];
    if (fs === ts) return;
    const [system, other] = fs ? [a.from, a.to] : [a.to, a.from];
    const row = rowOf(other);
    if (row < 0 || row === ext) return;
    g.setEdgeAttribute(id, "sys", true);
    const f = (folds[row + (fs ? "<" : ">")] ??= { row, inward: fs, count: 0, via: new Set(), systems: new Set(), violation: false, ctx: true });
    f.count += a.count ?? 1;
    f.via.add(other).add(system);
    f.systems.add(system);
    f.violation ||= a.violation;
    f.ctx &&= !!a.ctx;
  });
  const anchor = (id: string, label: string, x: number, y: number) => g.hasNode(id) || g.addNode(id, { x, y, size: 1, label, color: CLEAR,
    kind: "band", status: "unchanged", planned: false, test: false, anchor: true });
  const mean = (ids: string[], k: "x" | "y") => ids.reduce((t, id) => t + g.getNodeAttribute(id, k), 0) / ids.length;
  const systems = g.filterNodes((_, a) => a.kind === "system");
  if (!systems.length || !Object.keys(folds).length) return;
  anchor("band:ext", EXTERNAL, mean(systems, "x"), lanes[ext].y0 + 4);
  Object.values(folds).forEach((f) => {
    const boxes = [...f.via].filter((id) => !f.systems.has(id));
    const at = "band:" + lanes[f.row].label;
    anchor(at, lanes[f.row].label, mean(boxes, "x"), mean(boxes, "y"));
    const [from, to] = f.inward ? ["band:ext", at] : [at, "band:ext"];
    g.addEdgeWithKey(from + ">" + to, from, to, { from, to, size: widthOf(f.count, f.violation), color: palette.muted, kind: "band",
      status: "unchanged", planned: false, violation: f.violation, ctx: f.ctx, count: f.count, boxed: true, band: true, via: [...f.via],
      systems: f.systems.size });
  });
}

/** Layer rows, each with its name drawn at `x`. */
function lanesOf(els: El[]): Background["lanes"] {
  const labelX: Record<string, number> = {};
  els.forEach((e) => e.data.kind === "lanelabel" && e.position && (labelX[e.data.id.slice("lanelabel:".length)] = e.position.x));
  return els.filter((e) => e.data.kind === "lane" && e.position).map((e) => ({ label: e.data.label ?? "",
    y0: e.position!.y - (e.data.h ?? 0) / 2, y1: e.position!.y + (e.data.h ?? 0) / 2, x: labelX[e.data.id.slice("lane:".length)] }));
}

/** Layer rows, and group boxes: Architecture's layer and directory boxes, and compounds around their
 *  children's bounds plus 24 px, nested, outermost first. */
export function background(els: El[]): Background {
  const lanes = lanesOf(els);
  const groups: Box[] = [];
  const kids: Record<string, El[]> = {};
  const byId: Record<string, El> = {};
  els.forEach((e) => {
    if (e.group !== "nodes") return;
    byId[e.data.id] = e;
    if (e.data.parent) (kids[e.data.parent] = kids[e.data.parent] || []).push(e);
  });
  const box = (e: El, x0: number, y0: number, x1: number, y1: number): Box => ({
    id: e.data.id, label: e.data.label ?? "", x0, y0, x1, y1, status: e.data.status ?? "unchanged", kind: e.data.kind,
    planned: !!e.data.planned, outer: !!e.data.outer, findings: e.data.findings ?? 0,
  });
  /** A compound's box (pushed after its children's, so reversed at the end). */
  const bounds = (e: El): Box | null => {
    let [x0, y0, x1, y1] = [Infinity, Infinity, -Infinity, -Infinity];
    kids[e.data.id].forEach((c) => {
      const b = kids[c.data.id] ? bounds(c) : c.position ? { x0: c.position.x - sizeOf(c.data), y0: c.position.y - sizeOf(c.data),
        x1: c.position.x + sizeOf(c.data), y1: c.position.y + sizeOf(c.data) } : null;
      if (!b) return;
      [x0, y0, x1, y1] = [Math.min(x0, b.x0), Math.min(y0, b.y0), Math.max(x1, b.x1), Math.max(y1, b.y1)];
    });
    if (x0 === Infinity) return null;
    const b = box(e, x0 - 24, y0 - 24, x1 + 24, y1 + 24);
    groups.push(b);
    return b;
  };
  Object.keys(kids).forEach((id) => byId[id] && !byId[id].data.parent && bounds(byId[id]));
  groups.reverse();
  els.forEach((e) => {
    if (e.group !== "nodes" || !isBox(e.data.kind) || !e.position) return;
    const { w, h } = boxSize(e.data);
    groups.push(box(e, e.position.x - w / 2, e.position.y - h / 2, e.position.x + w / 2, e.position.y + h / 2));
  });
  return { lanes, groups };
}

/** The innermost (smallest) group box under a point. */
export function groupAt(bg: Background, x: number, y: number): string | null {
  let best: Box | null = null;
  const area = (b: Box) => (b.x1 - b.x0) * (b.y1 - b.y0);
  bg.groups.forEach((b) => {
    if (x >= b.x0 && x <= b.x1 && y >= b.y0 && y <= b.y1 && (!best || area(b) < area(best))) best = b;
  });
  return best ? (best as Box).id : null;
}

/** A label with its step badges before and its note pins after, the name cut at 30 characters. */
function fullLabel(a: NodeAttrs): string {
  const name = a.label.length > 30 ? a.label.slice(0, 29) + "…" : a.label;
  return (a.steps ? a.steps + "  " : "") + name + (a.pins ? "  ◉ " + a.pins : "");
}

/** The fill a node's kind and status give it. */
function fill(a: NodeAttrs): string {
  if (a.anchor || isBox(a.kind) || a.kind === "laneadd") return a.kind === "laneadd" ? palette.accent : CLEAR;
  if (a.kind === "journey") return palette.accent;
  if (a.kind === "route") return palette.note;
  if (a.kind === "system") return palette.panel;
  return palette[a.status] ?? palette.unchanged;
}

/** How faded `id` is under the view state: 1, or the dimmed or faded opacity. */
function dimOf(s: ViewState, id: string): number {
  if (s.show) return s.show.nodes.has(id) ? 1 : 0.15;
  return s.focus && !s.focus.has(id) ? 0.12 : 1;
}

/** A reduced node: no label (null, so it takes no place in sigma's label grid) for anchors and boxes. */
export type ReducedNode = Omit<NodeAttrs, "label"> & Drawn & { label: string | null };

export function nodeReducer(s: ViewState): (id: string, a: NodeAttrs) => ReducedNode {
  return (id, a) => {
    if (a.test && !s.tests) return { ...a, hidden: true };
    let op = a.ghost ? 0.4 : a.status === "removed" || a.planned ? 0.35 : a.outer ? 0.55 : 1;
    let ring: string | undefined;
    let ringDash: number | undefined;
    if (a.status === "removed" || a.planned) [ring, ringDash] = [palette[a.status] ?? palette.unchanged, 3];
    if (a.findings) [ring, ringDash] = [palette.error, undefined];
    if (a.pins) [ring, ringDash] = [palette.accent, undefined];
    if (a.ghost) [ring, ringDash] = [palette.accent, 1.5];
    if (s.selected === id) [ring, ringDash] = [palette.accent, undefined];
    const shown = !!s.show?.nodes.has(id);
    if (shown) [ring, ringDash] = [palette.brand, undefined];
    const dim = dimOf(s, id);
    op = Math.min(op, dim);
    const out: ReducedNode = {
      ...a, color: alpha(fill(a), op), label: a.anchor || isBox(a.kind) ? null : a.kind === "laneadd" ? "+" : fullLabel(a),
      labelColor: alpha(shown || a.pins ? palette.accentText : palette.fg, Math.max(dim, 0.2)), labelSize: a.bp ? 14 : 11,
      forceLabel: shown || s.selected === id || s.hovered === id || a.kind === "journey" || a.kind === "system" || a.kind === "laneadd" || !!a.steps,
      zIndex: a.anchor ? 0 : shown ? 3 : dim < 1 ? 1 : 2,
    };
    if (a.icon && !a.anchor) [out.type, out.image] = ["image", a.icon];
    if (ring && !a.anchor && !isBox(a.kind)) [out.ring, out.ringDash, out.highlighted] = [alpha(ring, dim), ringDash, true];
    if (a.anchor || isBox(a.kind)) out.highlighted = false;
    return out;
  };
}

export function edgeReducer(s: ViewState): (id: string, a: EdgeAttrs) => EdgeAttrs & Drawn {
  return (_id, a) => {
    const out: EdgeAttrs & Drawn = { ...a };
    const ends = a.via ?? [a.from, a.to];
    const sel = !!s.selected && ends.includes(s.selected);
    const hot = sel || (!!s.hovered && ends.includes(s.hovered));
    const inShow = !!s.show?.edges.has(a.from + "|" + a.to);
    // A selected end shows its own system edges instead of its row's band edge.
    out.hidden = (!s.links && !sel) || (!!a.sys && !sel && !inShow) || (!!a.band && sel);
    // Quiet by default; the hovered or selected box's edges come forward.
    let [color, op] = [palette.muted, 0.3];
    if (a.kind === "http") [color, op] = [palette.note, 0.4];
    else if (a.kind === "uses") [color, op] = [palette.warning, 0.4];
    if (a.status === "added") [color, op] = [palette.added, 0.6];
    if (a.status === "removed") color = palette.removed;
    if (a.ctx) op = 0.15;
    if (a.violation) [color, op] = [palette.error, 1];
    if (hot) [op, out.zIndex] = [Math.max(op, 0.9), 2];
    out.dash = a.guessed ? 2 : a.planned || a.status === "removed" || a.kind === "http" || a.kind === "dispatch" ? 6 : 0;
    // Counts only for the hovered or selected box's edges: never piled over the boxes.
    out.label = a.step ?? (hot && a.count && a.count > 1 ? String(a.count) : undefined);
    out.forceLabel = !!out.label;
    out.type = a.curve ? "curve" : undefined;
    if (s.show) {
      if (inShow) [color, op, out.size, out.zIndex] = [palette.brand, 1, 3, 3];
      else op = 0.15;
    } else if (s.focus && !(s.focus.has(a.from) && s.focus.has(a.to))) op = 0.12;
    out.color = alpha(color, op);
    return out;
  };
}

/** An edge's tooltip: both ends (box names without their counts) and how many links it stands for. */
export function edgeTip(a: EdgeAttrs, labelOf: (id: string) => string): string {
  const name = (id: string) => labelOf(id).replace(/ · \d+$/, "");
  const n = a.count ?? 1;
  const sys = a.systems ? ` ${a.from === "band:ext" ? "from" : "to"} ${a.systems} system${a.systems === 1 ? "" : "s"}` : "";
  return `${name(a.from)} → ${name(a.to)} · ${n} link${n === 1 ? "" : "s"}${sys}`;
}

/** The camera (home-viewport pixels and ratio) framing `p`..`q` in a W×H view, `unit` pixels per
 *  graph unit at ratio 1: fitted, never closer than 1.4x. With `minScale` it never shrinks below
 *  that scale either; a taller picture then shows its top, and the rest is a pan or a zoom away. */
export function frame(p: { x: number; y: number }, q: { x: number; y: number }, W: number, H: number, unit: number, minScale?: number) {
  let ratio = Math.max((q.x - p.x) / Math.max(W - 60, 50), (q.y - p.y) / Math.max(H - 60, 50), unit / 1.4);
  if (minScale) ratio = Math.min(ratio, unit / minScale);
  const tall = q.y - p.y > (H - 60) * ratio;
  return { x: (p.x + q.x) / 2, y: tall ? p.y + (H / 2 - 30) * ratio : (p.y + q.y) / 2, ratio };
}

/** ①, ②, … for step numbers (plain "(n)" past 20). */
export const badge = (ns: number[]) => ns.map((k) => (k <= 20 ? String.fromCodePoint(0x245f + k) : "(" + k + ")")).join("");
