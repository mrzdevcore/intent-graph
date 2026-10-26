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
  /** Forces the hovered node's label; <Graph> leaves it null, as sigma draws hovers itself. */
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

/** An Architecture box's height and width (as Cytoscape drew them). */
function boxSize(d: ElData): { w: number; h: number } {
  const c = Math.min(300, Math.max(1, d.count ?? 1));
  return { w: Math.max(70, (d.label ?? "").length * 8.5 + 24), h: 34 + ((c - 1) / 299) * 46 };
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
    const c = Math.min(40, Math.max(1, d.count ?? 1));
    g.addEdgeWithKey(d.id, d.source, d.target, {
      from: d.source!, to: d.target!, size: d.violation ? 2 + ((c - 1) / 39) * 4 : 1 + ((c - 1) / 39) * 5, color: palette.muted,
      kind: d.kind ?? "", status: d.status ?? "unchanged", planned: !!d.planned, violation: !!d.violation, guessed: !!d.guessed,
      ctx: !!d.ctx, count: d.count, boxed: isBoxEnd(g.getNodeAttributes(d.source!)) && isBoxEnd(g.getNodeAttributes(d.target!)),
    });
  });
  // Opposite edges run side by side: each shifts to its own left, so the pair lands on both sides.
  g.forEachEdge((id, a) => g.setEdgeAttribute(id, "offset", g.outEdges(a.to, a.from).length ? OFFSET : 0));
  return g;
}

/** Layer rows, and group boxes: Architecture's layer and directory boxes, and compounds around their
 *  children's bounds plus 24 px, nested, outermost first. */
export function background(els: El[]): Background {
  const labelX: Record<string, number> = {};
  els.forEach((e) => e.data.kind === "lanelabel" && e.position && (labelX[e.data.id.slice("lanelabel:".length)] = e.position.x));
  const lanes = els.filter((e) => e.data.kind === "lane" && e.position).map((e) => ({ label: e.data.label ?? "",
    y0: e.position!.y - (e.data.h ?? 0) / 2, y1: e.position!.y + (e.data.h ?? 0) / 2, x: labelX[e.data.id.slice("lane:".length)] }));
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
    if (!s.links && s.selected !== a.from && s.selected !== a.to) out.hidden = true;
    let [color, op] = [palette.muted, 0.55];
    if (a.kind === "http") [color, op] = [palette.note, 0.95];
    else if (a.kind === "uses") [color, op] = [palette.warning, 0.9];
    if (a.status === "added") [color, op] = [palette.added, 0.9];
    if (a.status === "removed") color = palette.removed;
    if (a.ctx) op = 0.22;
    if (a.violation) [color, op] = [palette.error, 1];
    out.dash = a.guessed ? 2 : a.planned || a.status === "removed" || a.kind === "http" || a.kind === "dispatch" ? 6 : 0;
    out.label = a.step ?? (a.count && a.count > 1 ? String(a.count) : undefined);
    out.forceLabel = !!a.step || (!!a.boxed && !!out.label);
    if (s.show) {
      if (s.show.edges.has(a.from + "|" + a.to)) [color, op, out.size, out.zIndex] = [palette.brand, 1, 3, 3];
      else op = 0.15;
    } else if (s.focus && !(s.focus.has(a.from) && s.focus.has(a.to))) op = 0.12;
    out.color = alpha(color, op);
    return out;
  };
}

/** ①, ②, … for step numbers (plain "(n)" past 20). */
export const badge = (ns: number[]) => ns.map((k) => (k <= 20 ? String.fromCodePoint(0x245f + k) : "(" + k + ")")).join("");
