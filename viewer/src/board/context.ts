// The real system around a design, as the engine's Context JSON: the Architecture rows of the
// scope (lanes from placeRows), the faded boxes in them and their edges. Drawn locked.

import type { DesignScope as Scope } from "../api";
import { architecture, assignRows, placeRows } from "../model/architecture";
import { exact } from "../model/changed";
import { focusIds } from "../model/focus";
import { aggregateEdges, type El, type ElData, type Group, type Index } from "../model/graph";

/** The group a `dir` or `layer` scope names; the root for the whole system (`.`) or one gone. */
export function scopeGroup(ix: Index, scope: Scope): Group {
  if (scope.kind === "dir") return ix.groups[scope.id] ?? ix.root;
  if (scope.kind === "layer") return Object.values(ix.root.children).find((g) => g.layer && g.label === scope.name) ?? ix.root;
  return ix.root;
}

/** Box size from its label, as the Architecture view sizes its slots. */
function size(d: ElData): [number, number] {
  const n = (d.label || "").length;
  if (d.kind === "system") return [Math.max(80, Math.min(n, 20) * 6.5 + 14), 36];
  if (d.kind === "layer" || d.kind === "dir") return [Math.max(80, n * 9 + 40), 48];
  return [Math.max(80, Math.min(n, 24) * 7 + 20), 40];
}

/** The modules and systems around `id`, edges between them by module. */
function around(ix: Index, id: string, depth: number): { nodes: El[]; edges: { source: string; target: string }[] } {
  const nodes = exact(ix, focusIds(ix, id, depth)).filter((e) => e.group === "nodes" && !e.data.parent);
  const shown = new Set(nodes.map((e) => e.data.id));
  const rep = (x: string) => {
    const n = ix.byId[x];
    return !n ? null : n.kind === "system" || n.module ? x : n.file;
  };
  return { nodes, edges: aggregateEdges(ix, rep, (x) => shown.has(x)) };
}

export function buildContext(ix: Index, scope: Scope, width: number): string {
  let nodes: El[], edges: { source: string; target: string }[];
  if (scope.kind === "around") ({ nodes, edges } = around(ix, scope.id, scope.depth));
  else {
    const els = architecture(ix, scopeGroup(ix, scope), width);
    nodes = els.filter((e) => e.group === "nodes" && e.data.kind !== "lane" && e.data.kind !== "lanelabel");
    edges = els.filter((e) => e.group === "edges").map((e) => ({ source: e.data.source!, target: e.data.target! }));
  }
  const rows = assignRows(ix, nodes);
  const { lanes } = placeRows(nodes, rows, width);
  const kind = new Map<string, string>();
  const boxes = nodes.map((e) => {
    const [w, h] = size(e.data);
    const k = e.data.kind === "openmodule" || e.data.kind === "member" ? "module" : (e.data.kind ?? "module");
    kind.set(e.data.id, k);
    const row = rows[e.data.id];
    return {
      id: e.data.id, label: e.data.label ?? e.data.id, kind: k, layer: row.kind === "layer" ? row.name : null,
      x: e.position!.x - w / 2, y: e.position!.y - h / 2, w, h,
    };
  });
  const links = edges.filter((e) => kind.has(e.source) && kind.has(e.target))
    .map((e) => [e.source, e.target, kind.get(e.target) === "system" ? "uses" : "calls"]);
  return JSON.stringify({ lanes, boxes, edges: links });
}
