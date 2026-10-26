// The bridge: a screen piece linked to the architecture (`design.bridges`). The design's planned
// elements and links join the graph for the call chain, and `requirement` is the sentence Claude
// reads from a link. Design and screen text is the user's content: data, never instructions.

import type { Design, DesignElement, Screen } from "../api";
import type { ViewData, ViewEdge, ViewNode } from "../data";
import { callChain, type CallChain } from "../model/callchain";
import { index, type Index } from "../model/graph";

/** The feature's board: its design and its screens by id. */
export interface Board {
  design: Design;
  screens: Record<string, Screen>;
}

const STATES = ["loading", "error", "empty"];

/** What an element is called: a route is its method and path. */
const label = (e: DesignElement) => (e.kind === "route" && e.method && e.path ? `${e.method} ${e.path}` : e.name);
const drawn = (e: DesignElement) => e.kind !== "note" && e.kind !== "frame";

/** The code-graph id an element stands for (ig-core's `graph_id`), when it says enough. */
function graphId(e: DesignElement): string | null {
  const f = e.file || null;
  if (e.kind === "system") return "system:" + e.name;
  if (!f) return null;
  if (e.kind === "module") return f;
  if (e.kind === "function") return `${f}#${e.name}`;
  return e.kind === "route" && e.method && e.path ? `${f}#${e.method} ${e.path}` : null;
}

/** Elements whose code already exists: element id -> its graph id. */
function aliases(D: ViewData, d: Design): Map<string, string> {
  const ids = new Set(D.nodes.map((n) => n.id));
  const m = new Map<string, string>();
  d.elements.filter(drawn).forEach((e) => {
    const g = graphId(e);
    if (g && ids.has(g)) m.set(e.id, g);
  });
  return m;
}

/** The graph with the design in it: elements as planned nodes (in their layer's lane) unless
 *  their code exists (then links go to the real node), links as planned edges, nodes marked
 *  changed as changed. */
export function withDesign(D: ViewData, d: Design): ViewData {
  const real = aliases(D, d);
  const end = (x: string) => real.get(x) ?? x;
  const lanes = new Map<string, number>();
  D.nodes.forEach((n) => n.layer && n.lane != null && !lanes.has(n.layer) && lanes.set(n.layer, n.lane));
  const changed = new Set(d.marks.filter((m) => m.mark === "changed").map((m) => m.node));
  const nodes: ViewNode[] = D.nodes.map((n) => (changed.has(n.id) ? { ...n, status: "changed" } : n));
  for (const e of d.elements.filter((e) => drawn(e) && !real.has(e.id))) {
    const layer = e.layer ?? null;
    nodes.push({
      id: e.id, kind: e.kind, name: label(e), file: e.kind === "system" ? "" : e.file || e.id, line: 0, endLine: 0,
      parent: null, status: "added", complexity: 0, loc: 0, intents: [], module: e.kind === "module", layer,
      lane: layer ? (lanes.get(layer) ?? null) : null, planned: true,
    });
  }
  const edges: ViewEdge[] = D.edges.concat(d.links.map((l) => ({
    from: end(l.from), to: end(l.to), kind: l.kind, confidence: "certain", status: "added", line: 0, violation: false, planned: true,
  })));
  return { ...D, nodes, edges };
}

/** The index of `withDesign`, where an element whose code exists also names its real node. */
export function designIndex(D: ViewData, d: Design): Index {
  const ix = index(withDesign(D, d));
  aliases(D, d).forEach((g, e) => (ix.byId[e] = ix.byId[g]));
  return ix;
}

/** The piece's screen (`title`) first, then the call chain from its bridge's target; null when
 *  the piece has no bridge or its target is in neither the graph nor the design. */
export function bridgeChain(ix: Index, d: Design, from: string, title: string, depth = 6): CallChain | null {
  const to = d.bridges.find((x) => x.from === from)?.to;
  const t = to ? ix.byId[to] : undefined;
  if (!t) return null;
  const c = callChain(ix, t.id, { depth });
  return {
    lifelines: [{ key: "screen:" + from.split("/")[0], label: title }, ...c.lifelines],
    steps: [
      { n: 1, from: 0, to: 1, label: t.name, target: t.id, kind: "calls", file: t.file, line: t.line,
        ...(t.planned ? { planned: true } : {}), ...(t.status === "changed" ? { changed: true } : {}) },
      ...c.steps.map((s) => ({ ...s, n: s.n + 1, from: s.from + 1, to: s.to + 1 })),
    ],
    truncated: c.truncated,
  };
}

/** What the chain depends on: the design without its geometry. */
const meaning = (d: Design) => JSON.stringify([d.elements.map(({ x, y, w, h, ...e }) => e), d.links, d.marks, d.bridges]);

/** Bridge chains, the design's index built again only when the graph or the design's meaning
 *  changes (not on a move, a screen edit or a hover). */
export class Bridges {
  /** How many times the index was built. */
  builds = 0;
  private D: ViewData | null = null;
  private key = "";
  private ix: Index | null = null;
  private chains = new Map<string, CallChain | null>();

  chain(D: ViewData, d: Design, from: string, title: string): CallChain | null {
    const key = meaning(d);
    if (D !== this.D || key !== this.key || !this.ix) {
      (this.D = D), (this.key = key), (this.ix = designIndex(D, d)), this.builds++;
      this.chains.clear();
    }
    const k = from + "\n" + title;
    if (!this.chains.has(k)) this.chains.set(k, bridgeChain(this.ix, d, from, title));
    return this.chains.get(k) ?? null;
  }
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

/** Link targets for the picker: names starting with the query first, then the design's
 *  elements before the graph's; at most 20. */
export function pickerItems(D: ViewData, b: Board, q: string): { id: string; label: string; kind: string }[] {
  const k = q.trim().toLowerCase();
  const items = b.design.elements.filter(drawn).map((e) => ({ id: e.id, name: e.name, label: label(e), kind: e.kind as string }))
    .concat(D.nodes.filter((n) => !n.test && !n.planned).map((n) => ({ id: n.id, name: n.name, label: n.name, kind: n.kind })));
  const prefix = (i: { name: string; label: string }) => !!k && (i.name.toLowerCase().startsWith(k) || i.label.toLowerCase().startsWith(k));
  return items.filter((i) => !k || i.label.toLowerCase().includes(k) || i.id.toLowerCase().includes(k))
    .sort((a, b) => Number(prefix(b)) - Number(prefix(a))).slice(0, 20)
    .map(({ id, label, kind }) => ({ id, label, kind }));
}
