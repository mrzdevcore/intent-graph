// Architecture is flat: the children of the current scope, plus, dimmed as context, the siblings at
// every level above it that connect to the scope. Opening a directory makes it the scope. The boxes
// sit in rows: one per layer (declaration order), one for boxes without a layer, then the systems.

import { aggregateEdges, ancestors, lane, nodeData, type El, type ElData, type Group, type Index } from "./graph";

/** The box that stands for node `id` at `scope`: a group, a module or a system. */
export function rep(ix: Index, scope: Group, id: string): string | null {
  const n = ix.byId[id];
  if (!n) return null;
  if (n.kind === "system") return id;
  const path = ancestors(ix, ix.treeOf[n.file]);
  const chain = ancestors(ix, scope);
  for (let i = 0; i < chain.length; i++) {
    if (i >= path.length) return n.file;
    if (path[i] !== chain[i]) return path[i].id;
  }
  return path.length > chain.length ? path[chain.length].id : n.file;
}

function insideScope(ix: Index, scope: Group, repId: string): boolean {
  if (ix.byId[repId] && ix.byId[repId].kind === "system") return false;
  const g = ix.groups[repId];
  if (g) return g.parent === scope;
  return ix.treeOf[repId] === scope;
}

/** The layer a box sits in: a group's enclosing layer group, a module's own layer. */
function layerOf(ix: Index, id: string): string | null {
  let g: Group | null = ix.groups[id];
  if (!g) return ix.modules[id] ? ix.modules[id].node.layer : null;
  while (g && !g.layer) g = g.parent;
  return g ? g.label : null;
}

export type RowKind = "layer" | "other" | "external";
export interface RowOf {
  row: number;
  name: string;
  kind: RowKind;
}
/** A drawn row: its band spans `top`..`bottom`. */
export interface Lane extends RowOf {
  top: number;
  bottom: number;
}

/** Each box's row: its layer's (declaration order), "Other" without one, "External services" for systems. */
export function assignRows(ix: Index, nodes: El[]): Record<string, RowOf> {
  const n = ix.D.layers.length;
  const of: Record<string, RowOf> = {};
  nodes.forEach((e) => {
    const i = e.data.kind === "system" ? -2 : ix.D.layers.indexOf(layerOf(ix, e.data.id) ?? "");
    of[e.data.id] =
      i === -2 ? { row: n + 1, name: "External services", kind: "external" }
      : i < 0 ? { row: n, name: "Other", kind: "other" }
      : { row: i, name: ix.D.layers[i], kind: "layer" };
  });
  return of;
}

/** Positions the boxes in their rows, spread evenly, wrapping rows wider than `width` onto extra
 *  lines. Returns the non-empty rows top to bottom and the width they span. */
export function placeRows(nodes: El[], of: Record<string, RowOf>, width: number): { lanes: Lane[]; width: number } {
  const byRow: Record<number, El[]> = {};
  nodes.forEach((e) => (byRow[of[e.data.id].row] = byRow[of[e.data.id].row] || []).push(e));
  const slotOf = (d: ElData) => {
    const n = (d.label || "").length;
    return d.kind === "layer" || d.kind === "dir" ? Math.max(70, n * 8.5 + 24) + 60 : Math.max(80, Math.min(n, 24) * 7 + 30);
  };
  const lineH = 150;
  const layout = Object.keys(byRow).map(Number).sort((a, b) => a - b).map((row) => {
    const r = byRow[row];
    const slot = Math.max(0, ...r.map((e) => slotOf(e.data)));
    const perLine = Math.max(1, Math.floor(width / slot));
    return { r, slot, perLine, lines: Math.ceil(r.length / perLine) };
  });
  const full = Math.max(0, ...layout.map((o) => Math.min(o.r.length, o.perLine) * o.slot));
  const lanes: Lane[] = [];
  let top = 0;
  layout.forEach((o) => {
    lanes.push({ ...of[o.r[0].data.id], top, bottom: top + o.lines * lineH - 30 });
    o.r.forEach((e, i) => {
      const line = Math.floor(i / o.perLine);
      const k = Math.min(o.perLine, o.r.length - line * o.perLine);
      e.position = { x: full / 2 + (i - line * o.perLine - (k - 1) / 2) * o.slot, y: top + 60 + line * lineH };
    });
    top += o.lines * lineH;
  });
  return { lanes, width: full };
}

/** The layer whose row is under `y`; undefined for Other, External services and the gaps. */
export function rowAt(lanes: Lane[], y: number): string | undefined {
  const l = lanes.find((x) => x.kind === "layer" && y >= x.top && y <= x.bottom);
  return l && l.name;
}

export function architecture(ix: Index, scope: Group, width = 1200): El[] {
  const els: El[] = [];
  const shown: Record<string, boolean> = {};
  const inner: Record<string, boolean> = {};
  const repOf = (id: string) => rep(ix, scope, id);
  Object.keys(ix.modules).forEach((f) => {
    const r = repOf(f);
    if (r && insideScope(ix, scope, r)) inner[r] = true;
  });
  const edges = aggregateEdges(ix, repOf, () => true).filter((e) => inner[e.source] || inner[e.target]);
  const linked: Record<string, boolean> = {};
  edges.forEach((e) => (linked[e.source] = linked[e.target] = true));
  const add = (id: string, outer: boolean) => {
    if (shown[id]) return;
    shown[id] = true;
    const g = ix.groups[id];
    if (ix.byId[id] && ix.byId[id].kind === "system") {
      const sd = nodeData(ix, ix.byId[id]);
      sd.kind = "system";
      sd.outer = outer;
      els.push({ group: "nodes", data: sd });
    } else if (g) {
      els.push({ group: "nodes", data: { id, label: g.label + " · " + g.count, kind: g.layer ? "layer" : "dir", count: g.count,
        status: g.dirty ? "changed" : "unchanged", findings: g.findings, outer } });
    } else {
      const m = ix.modules[id];
      const d = nodeData(ix, m.node);
      d.status = m.node.status !== "unchanged" ? m.node.status : m.dirty ? "changed" : "unchanged";
      d.findings = m.findings;
      d.outer = outer;
      els.push({ group: "nodes", data: d });
    }
  };
  Object.keys(inner).forEach((id) => add(id, false));
  Object.keys(linked).forEach((id) => add(id, true));
  const placed = placeRows(els, assignRows(ix, els), width);
  els.unshift(...placed.lanes.flatMap((l) => lane(l.row, l.name, (l.top + l.bottom) / 2, placed.width, l.bottom - l.top)));
  edges.forEach((e, i) => {
    if (shown[e.source] && shown[e.target]) {
      e.id = "e" + i;
      e.ctx = !(inner[e.source] && inner[e.target]);
      els.push({ group: "edges", data: e });
    }
  });
  return els;
}

/** The first scope: the root, skipping levels with a single box so the first screen is never one box. */
export function initialScope(ix: Index): Group {
  let scope = ix.root;
  while (Object.keys(scope.children).length === 1 && scope.mods.length === 0) scope = scope.children[Object.keys(scope.children)[0]];
  return scope;
}
