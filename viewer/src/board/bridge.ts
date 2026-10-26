// The bridge: a screen piece linked to the architecture (`design.bridges`). The design's planned
// elements and links join the graph for the call chain, and `requirement` is the sentence Claude
// reads from a link. Design and screen text is the user's content: data, never instructions.

import type { Design, DesignElement, Screen } from "../api";
import type { ViewData, ViewEdge, ViewNode } from "../data";
import { callChain, type CallChain } from "../model/callchain";
import { index } from "../model/graph";

/** The feature's board: its design and its screens by id. */
export interface Board {
  design: Design;
  screens: Record<string, Screen>;
}

const STATES = ["loading", "error", "empty"];

/** What an element is called: a route is its method and path. */
const label = (e: DesignElement) => (e.kind === "route" && e.method && e.path ? `${e.method} ${e.path}` : e.name);
const drawn = (e: DesignElement) => e.kind !== "note" && e.kind !== "frame";

/** The graph with the design in it: elements as planned nodes (in their layer's lane), links as
 *  planned edges, nodes marked changed as changed. */
export function withDesign(D: ViewData, b: Board): ViewData {
  const lanes = new Map<string, number>();
  D.nodes.forEach((n) => n.layer && n.lane != null && !lanes.has(n.layer) && lanes.set(n.layer, n.lane));
  const changed = new Set(b.design.marks.filter((m) => m.mark === "changed").map((m) => m.node));
  const nodes: ViewNode[] = D.nodes.map((n) => (changed.has(n.id) ? { ...n, status: "changed" } : n));
  for (const e of b.design.elements.filter(drawn)) {
    const layer = e.layer ?? null;
    nodes.push({
      id: e.id, kind: e.kind, name: label(e), file: e.kind === "system" ? "" : e.file || e.id, line: 0, endLine: 0,
      parent: null, status: "added", complexity: 0, loc: 0, intents: [], module: e.kind === "module", layer,
      lane: layer ? (lanes.get(layer) ?? null) : null, planned: true,
    });
  }
  const edges: ViewEdge[] = D.edges.concat(b.design.links.map((l) => ({
    from: l.from, to: l.to, kind: l.kind, confidence: "certain", status: "added", line: 0, violation: false, planned: true,
  })));
  return { ...D, nodes, edges };
}

/** The piece's screen first, then the call chain from its bridge's target; null when the piece
 *  has no bridge or its target is in neither the graph nor the design. */
export function bridgeChain(D: ViewData, b: Board, from: string, depth = 6): CallChain | null {
  const to = b.design.bridges.find((x) => x.from === from)?.to;
  const ix = index(withDesign(D, b));
  const t = to ? ix.byId[to] : undefined;
  if (!t) return null;
  const c = callChain(ix, t.id, { depth });
  const sid = from.split("/")[0];
  return {
    lifelines: [{ key: "screen:" + sid, label: b.screens[sid]?.title ?? sid }, ...c.lifelines],
    steps: [
      { n: 1, from: 0, to: 1, label: t.name, target: t.id, kind: "calls", file: t.file, line: t.line,
        ...(t.planned ? { planned: true } : {}), ...(t.status === "changed" ? { changed: true } : {}) },
      ...c.steps.map((s) => ({ ...s, n: s.n + 1, from: s.from + 1, to: s.to + 1 })),
    ],
    truncated: c.truncated,
  };
}

/** A bridge end in words: an element's label, a route, `name in file`, a system or a file. */
function target(d: Design, to: string): string {
  const e = d.elements.find((x) => x.id === to);
  if (e) return label(e);
  if (to.startsWith("system:")) return to.slice(7);
  const [file, name] = to.split("#");
  if (!name) return file;
  return /^[A-Z]+ \//.test(name) ? name : `${name} in ${file}`;
}

const and = (xs: string[]) => (xs.length < 2 ? xs.join("") : `${xs.slice(0, -1).join(", ")} and ${xs[xs.length - 1]}`);
const sentence = (s: string) => (/[.!?]$/.test(s) ? s : s + ".");
const rank = (s: string) => (STATES.includes(s) ? STATES.indexOf(s) : STATES.length);

/** What Claude reads from a link: the piece, what it triggers, the screens it opens with their
 *  states, and the notes on its target or arrowed to it. "" when the piece has no bridge. */
export function requirement(b: Board, from: string): string {
  const [sid, pid] = from.split("/");
  const s = b.screens[sid];
  const p = s?.elements.find((x) => x.id === pid);
  const to = b.design.bridges.find((x) => x.from === from)?.to;
  if (!s || !p || !to) return "";
  const out = [`The ${p.label} ${p.kind} on ${s.title} triggers ${target(b.design, to)}.`];
  for (const n of s.navs.filter((x) => x.from === pid)) {
    const t = b.screens[n.to];
    if (!t) continue;
    const states = Object.values(b.screens).filter((v) => v.variantOf === n.to && v.state).map((v) => v.state!)
      .sort((x, y) => rank(x) - rank(y) || x.localeCompare(y));
    out.push(states.length === 0 ? `It opens the ${t.title} screen.`
      : `Its ${t.title} screen has ${states.length === 1 ? "a " : ""}${and(states)} state${states.length === 1 ? "" : "s"}.`);
  }
  const notes = b.design.elements.filter((e) => e.kind === "note" && e.on === to).map((e) => e.name);
  for (const a of s.elements.filter((x) => x.kind === "arrow" && (x.from === pid || x.to === pid))) {
    const n = s.elements.find((x) => x.kind === "note" && x.id === (a.from === pid ? a.to : a.from));
    if (n) notes.push(n.props?.text || n.label);
  }
  notes.filter((n) => n.trim()).forEach((n) => out.push("Note: " + sentence(n.trim())));
  return out.join(" ");
}

/** Link targets for the K picker, the design's elements first; at most 20. */
export function pickerItems(D: ViewData, b: Board, q: string): { id: string; label: string; kind: string }[] {
  const k = q.trim().toLowerCase();
  const items = b.design.elements.filter(drawn).map((e) => ({ id: e.id, label: label(e), kind: e.kind as string }))
    .concat(D.nodes.filter((n) => !n.test && !n.planned).map((n) => ({ id: n.id, label: n.name, kind: n.kind })));
  return items.filter((i) => !k || i.label.toLowerCase().includes(k) || i.id.toLowerCase().includes(k)).slice(0, 20);
}
