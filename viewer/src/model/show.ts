// The agent's `ig_show` tool input (.ig/show.json) as a typed show.

export interface Show {
  /** Assigned by the sender (ig_show's file) or by the viewer for a chat chip. */
  seq: number;
  title: string;
  nodes: string[];
  edges: [string, string][];
  steps: { target: string; text: string }[];
  mode: "focus" | "architecture" | "chain";
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
  return { seq: typeof r.seq === "number" ? r.seq : 0, title: r.title, nodes: r.nodes, edges, steps: steps.map(({ target, text }) => ({ target, text })), mode };
}
