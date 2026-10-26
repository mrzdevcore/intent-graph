// Answers shown in the graph: the agent's `ig_show` (.ig/show.json), applied to the current index,
// and the banner's Clear and Back.

import { ancestors, type Group, type Index } from "./graph";

export type Mode = "changed" | "architecture" | "focus" | "blueprint";

export interface Show {
  /** Assigned by the sender (ig_show's file) or by the viewer for a chat chip. */
  seq: number;
  title: string;
  nodes: string[];
  edges: [string, string][];
  steps: { target: string; text: string }[];
  mode: "focus" | "architecture" | "chain";
}

export interface Applied {
  mode: Mode;
  nodes: Set<string>;
  /** "from|to" */
  edges: Set<string>;
  steps: { n: number; target: string; text: string }[];
  /** Ids the show names that the graph no longer has (a rename, another branch). */
  missing: string[];
}

/** The show on this index: unknown ids dropped and listed, steps numbered in input order. */
export function applyShow(ix: Index, s: Show): Applied {
  const missing = new Set<string>();
  const has = (id: string) => !!ix.byId[id] || (missing.add(id), false);
  const nodes = new Set(s.nodes.filter(has));
  const edges = new Set(s.edges.filter(([a, b]) => [has(a), has(b)].every(Boolean)).map(([a, b]) => a + "|" + b));
  const steps = s.steps
    .map((st, i) => ({ n: i + 1, target: st.target, text: st.text }))
    .filter((st) => st.target.split("|").map(has).every(Boolean));
  // A chain is the answer's path, drawn in Focus; the side panel shows its calls down.
  const mode: Mode = s.mode === "architecture" ? "architecture" : "focus";
  return { mode, nodes, edges, steps, missing: [...missing] };
}

/** The deepest group holding every node's file: where Architecture opens for a show. */
export function commonScope(ix: Index, ids: Iterable<string>): Group {
  let common: Group[] | null = null;
  for (const id of ids) {
    const n = ix.byId[id];
    const path = n ? ancestors(ix, ix.treeOf[n.file]) : [];
    common = common ? common.filter((g, i) => path[i] === g) : path;
  }
  return common?.length ? common[common.length - 1] : ix.root;
}

export interface ShowState {
  current: Show | null;
  /** The view before the first show, for Back. */
  previous: { mode: Mode; selection: string[]; scope: string | null; focus: string[] } | null;
}

export function showReducer(
  st: ShowState,
  a: { t: "show"; s: Show; view: ShowState["previous"] } | { t: "clear" } | { t: "back" },
): { state: ShowState; restore: ShowState["previous"] | null } {
  if (a.t === "show") return { state: { current: a.s, previous: st.previous ?? a.view }, restore: null };
  return { state: { current: null, previous: null }, restore: a.t === "back" ? st.previous : null };
}

const isStr = (x: unknown): x is string => typeof x === "string";

/** A chat tool card's input as a show, when the tool is `ig_show` (or `mcp__…__ig_show`). */
export function showFromTool(call: { title?: string; rawInput?: unknown }): Show | null {
  return call.title && /^(ig_show|mcp__.*__ig_show)$/.test(call.title) ? parseShow(call.rawInput) : null;
}

/** ig_show's caps (crates/ig-app/src/show.rs), in characters: a tool input over them was refused. */
const MAX = { nodes: 60, steps: 40, title: 200, text: 500 };
const chars = (s: string) => [...s].length;

/** A show from the host or a tool input: needs a title and nodes; missing edges and steps are [].
 *  One over ig_show's caps is null. */
export function parseShow(raw: unknown): Show | null {
  const r = raw as Record<string, unknown> | null;
  if (!r || typeof r !== "object" || !isStr(r.title) || !Array.isArray(r.nodes) || !r.nodes.length || !r.nodes.every(isStr)) return null;
  if (r.nodes.length > MAX.nodes || chars(r.title) > MAX.title) return null;
  const edges: [string, string][] = [];
  for (const e of Array.isArray(r.edges) ? r.edges : []) {
    const p = isStr(e) ? e.split("|") : e;
    if (Array.isArray(p) && p.every(isStr) && (p.length === 2 || p.length === 3)) edges.push([p[0], p[p.length - 1]]);
  }
  const steps = (Array.isArray(r.steps) ? r.steps : []).filter(
    (s): s is { target: string; text: string } => !!s && isStr(s.target) && isStr(s.text));
  if (steps.length > MAX.steps || steps.some((s) => chars(s.text) > MAX.text)) return null;
  const mode = r.mode === "architecture" || r.mode === "chain" ? r.mode : "focus";
  return { seq: 0, title: r.title, nodes: r.nodes, edges, steps: steps.map(({ target, text }) => ({ target, text })), mode };
}
