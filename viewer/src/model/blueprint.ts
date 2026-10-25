// Blueprint: one page (a journey), then everything behind it, in swimlanes.

import type { ViewEdge, ViewNode } from "../data";
import { base, lane, nodeData, type El, type Index } from "./graph";

function laneOf(n: ViewNode): number | null {
  return n.lane === null || n.lane === undefined ? null : n.lane;
}

/** The box a node is drawn as: endpoints and systems themselves, everything else its module. */
export function unitOf(ix: Index, id: string): string | null {
  const n = ix.byId[id];
  if (!n) return null;
  return n.kind === "system" || n.kind === "route" ? id : n.file;
}

/** "front/app/src/pages/accounts/[id]/index.vue" → app "front/app", route "/accounts/[id]". */
export function journeyName(file: string): { app: string; route: string } {
  const i = file.indexOf("/pages/");
  if (i < 0) {
    // Not a file-routed page (a view): app = the path before src/, name = the file without extension.
    const j = file.indexOf("/src/");
    return { app: j < 0 ? file.split("/").slice(0, -1).join("/") : file.slice(0, j), route: base(file).replace(/\.[a-z]+$/, "") };
  }
  const route = file.slice(i + 6).replace(/\.[a-z]+$/, "").replace(/\/index$/, "") || "/";
  const app = file.slice(0, i).replace(/\/(src|app)(\/application)?$/, "");
  return { app, route };
}

/** The pages: the modules in the first lane, by file. */
export function journeys(ix: Index): ViewNode[] {
  return ix.D.nodes.filter((n) => n.module && n.lane === 0 && n.status !== "removed").sort((a, b) => (a.file < b.file ? -1 : 1));
}

interface UnitEdge {
  id?: string;
  source: string;
  target: string;
  count: number;
  kind: string;
  labels: string[];
  violation: boolean;
  status?: string;
  guessed?: boolean;
}

type Step = Pick<ViewEdge, "to" | "kind" | "from"> & { violation?: boolean };

/** Walks forward from the page. Lanes only go down (except within a lane), other pages are not
 *  entered, and a node counts as backstage once an http edge or a system has been reached. */
export function blueprintGraph(ix: Index, page: string, uiOnly: boolean): { units: string[]; edges: UnitEdge[] } {
  const { byId } = ix;
  const seen: Record<string, boolean> = {};
  const backstage: Record<string, boolean> = {};
  const unitEdges: Record<string, UnitEdge> = {};
  const order: string[] = [];
  const queue = [page].concat(ix.membersOf[page] || []).map((id) => ({ id, back: false }));
  const visit = (id: string, back: boolean) => {
    const key = id + (back ? "|b" : "");
    if (seen[key]) return false;
    seen[key] = true;
    const u = unitOf(ix, id);
    if (u && order.indexOf(u) < 0) order.push(u);
    if (back && u) backstage[u] = true;
    return true;
  };
  queue.forEach((q) => visit(q.id, false));
  while (queue.length) {
    const q = queue.shift()!;
    const from = byId[q.id];
    if (!from) continue;
    const fromUnit = unitOf(ix, q.id)!;
    const next: Step[] = (ix.outEdges[q.id] || []).filter((e) => byId[e.to] && !byId[e.to].test);
    // A function's module-level code (imports of systems, markup) belongs to the journey too.
    if (!from.module && from.kind !== "system" && from.kind !== "route") next.push({ to: from.file, kind: "member", from: q.id });
    next.forEach((e) => {
      const to = byId[e.to];
      if (!to) return;
      const lf = laneOf(byId[fromUnit] || from);
      const lt = laneOf(to);
      if (lt === 0 && to.file !== page) return;
      if (lf !== null && lt !== null && lt < lf) return;
      const back = q.back || e.kind === "http" || to.kind === "system";
      const tu = unitOf(ix, e.to)!;
      if (fromUnit !== tu && e.kind !== "member") {
        const k = fromUnit + "|" + tu;
        const x = unitEdges[k] || (unitEdges[k] = { source: fromUnit, target: tu, count: 0, kind: e.kind, labels: [], violation: false });
        x.count++;
        x.violation = x.violation || !!e.violation;
        if (e.kind === "http") x.kind = "http";
      }
      if (visit(e.to, back)) queue.push({ id: e.to, back });
    });
  }
  // Keep only units that lead to the backstage (unless showing UI-only components).
  const keep: Record<string, boolean> = {};
  order.forEach((u) => (keep[u] = uiOnly || !!backstage[u] || u === page));
  if (!uiOnly) {
    let changed = true;
    while (changed) {
      changed = false;
      Object.values(unitEdges).forEach((e) => {
        if (keep[e.target] && !keep[e.source]) {
          keep[e.source] = true;
          changed = true;
        }
      });
    }
  }
  return {
    units: order.filter((u) => keep[u]),
    edges: Object.values(unitEdges).filter((e) => keep[e.source] && keep[e.target]),
  };
}

/** The journey's units in lane rows, ordered to limit crossings, with preset positions. */
export function blueprint(ix: Index, journey: string | null, uiOnly: boolean): El[] {
  const els: El[] = [];
  if (!journey) return els;
  const { byId, D } = ix;
  const lastLane = D.lanes.length - 1;
  const bg = blueprintGraph(ix, journey, uiOnly);
  const lanes = D.lanes.concat(["Other"]);
  const other = lanes.length - 1;
  const rows: Record<number, string[]> = {};
  const widths: Record<number, number> = {};
  bg.units.forEach((u) => {
    let l = laneOf(byId[u]);
    if (l === null) l = other;
    (rows[l] = rows[l] || []).push(u);
  });
  // Order each lane by the average x of what points at it, top-down, to limit crossings.
  const x: Record<string, number> = {};
  const preds: Record<string, string[]> = {};
  bg.edges.forEach((e) => (preds[e.target] = preds[e.target] || []).push(e.source));
  const rank = (a: number) => (a === other ? lastLane + 0.5 : a);
  const used = Object.keys(rows).map(Number).sort((a, b) => rank(a) - rank(b));
  const gapX = 210;
  const gapY = 150;
  let width = 0;
  const labelOf = (u: string) => {
    const n = byId[u];
    return n.kind === "route" || n.kind === "system" ? n.name : u === journey ? journeyName(u).route : base(n.file);
  };
  const boxW = (u: string) => Math.max(150, Math.min(labelOf(u).length, 26) * 8.5 + 40);
  used.forEach((l) => {
    const us = rows[l].map((u, i) => {
      const ps = (preds[u] || []).filter((p) => x[p] !== undefined);
      return { u, k: ps.length ? ps.reduce((s, p) => s + x[p], 0) / ps.length : i * gapX };
    });
    us.sort((a, b) => a.k - b.k);
    let cx = 0;
    us.forEach((o) => {
      const w = boxW(o.u);
      x[o.u] = cx + w / 2;
      cx += w;
    });
    rows[l] = us.map((o) => o.u);
    widths[l] = cx;
    width = Math.max(width, cx);
  });
  used.forEach((l, row) => {
    const y = row * gapY;
    const offset = (width - widths[l]) / 2;
    els.push(...lane(l, lanes[l], y, width));
    rows[l].forEach((u) => {
      const n = byId[u];
      const d = nodeData(ix, n);
      const m = ix.modules[u];
      d.label = labelOf(u);
      d.findings = n.module && m ? m.findings : d.findings;
      if (n.module && m && m.dirty && d.status === "unchanged") d.status = "changed";
      d.kind = n.kind === "system" ? "system" : n.kind === "route" ? "route" : u === journey ? "journey" : "module";
      d.bp = true;
      if (u === journey) d.label = journeyName(u).route;
      els.push({ group: "nodes", data: d, position: { x: x[u] + offset, y } });
    });
  });
  bg.edges.forEach((e, i) => {
    e.id = "b" + i;
    e.status = "unchanged";
    e.guessed = false;
    els.push({ group: "edges", data: e as El["data"] });
  });
  return els;
}
