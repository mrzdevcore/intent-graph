// Call chain: calls followed from a page or function across modules, through HTTP into
// routes, down to systems; one lifeline per blueprint lane (else layer, else module).

import type { ViewEdge, ViewNode } from "../data";
import { base, type Index } from "./graph";

const KINDS: Record<string, boolean> = { calls: true, dispatch: true, http: true, uses: true, renders: true };

export interface Lifeline {
  key: string;
  label: string;
  system?: boolean;
}
export interface Step {
  n: number;
  from: number;
  to: number;
  label: string;
  target: string;
  kind: "calls" | "dispatch" | "http" | "uses" | "renders";
  file: string;
  line: number;
}
export interface CallChain {
  lifelines: Lifeline[];
  steps: Step[];
  truncated: number;
}

export function clean(s: string): string {
  return String(s).replace(/[;#"]/g, " ").replace(/\s+/g, " ").trim();
}

export function callChain(ix: Index, start: string, opts: { depth?: number; max?: number } = {}): CallChain {
  const depth = opts.depth ?? 8;
  const max = opts.max ?? 60;
  const lifelines: Lifeline[] = [];
  const at: Record<string, number> = {};
  const steps: Step[] = [];
  let truncated = 0;
  const seen: Record<string, boolean> = {};

  const lifeline = (id: string): number => {
    const n = ix.byId[id];
    let l: Lifeline;
    if (!n) l = { key: id, label: id };
    else if (n.kind === "system") l = { key: id, label: n.name, system: true };
    else if (n.lane != null && ix.D.lanes[n.lane]) l = { key: "lane:" + n.lane, label: ix.D.lanes[n.lane] };
    else if (n.layer) l = { key: "layer:" + n.layer, label: n.layer };
    else l = { key: n.file, label: base(n.file).replace(/\.[a-z]+$/, "") };
    if (at[l.key] === undefined) {
      at[l.key] = lifelines.length;
      lifelines.push(l);
    }
    return at[l.key];
  };

  const okEdge = (e: ViewEdge) => {
    const t = ix.byId[e.to];
    return KINDS[e.kind] && t && !t.test && e.confidence !== "guessed" && !(t.kind === "class" && /(Error|Exception)$/.test(t.name));
  };
  const same = (n: ViewNode, e: ViewEdge) => ix.byId[e.to].kind !== "system" && ix.byId[e.to].file === n.file;
  const edgesOf = (id: string) => (ix.outEdges[id] || []).filter(okEdge).sort((a, b) => (a.line || 0) - (b.line || 0));

  // The cross-file edges a node makes as its own, with same-file calls folded in, then its inner members'
  // (a module's members, a function's closures, a class's methods). Computed once per node (own visited set,
  // so cycles end and shared helpers are not repeated within one caller), reused by every caller.
  const memo: Record<string, { e: ViewEdge; file: string }[]> = {};
  const reach: Record<string, Record<string, boolean>> = {};
  const flat = (id: string) => {
    if (memo[id]) return memo[id];
    const n = ix.byId[id];
    const res: { e: ViewEdge; file: string }[] = [];
    const visited: Record<string, boolean> = {};
    const go = (x: string) => {
      if (visited[x]) return;
      visited[x] = true;
      const m = ix.byId[x];
      let out = edgesOf(x);
      if (x === id && !n.module) out = out.concat(edgesOf(n.file).filter((e) => e.kind === "uses")).sort((a, b) => (a.line || 0) - (b.line || 0));
      out.forEach((e) => (same(m, e) ? go(e.to) : res.push({ e, file: m.file })));
    };
    go(id);
    // Inner members are start points too: first those nothing in the file calls, then any still
    // unreached (members only calling each other in a cycle). A module's are its top-level members.
    const members = (ix.membersOf[n.file] || []).filter((m) => {
      const p = ix.byId[m].parent;
      return p === id || (n.module && !p);
    });
    const called: Record<string, boolean> = {};
    members.concat(id).forEach((m) => edgesOf(m).forEach((e) => same(n, e) && (called[e.to] = true)));
    const covered: Record<string, boolean> = { ...visited };
    members.filter((m) => !called[m]).concat(members).forEach((m) => {
      if (covered[m]) return;
      res.push(...flat(m));
      Object.assign(covered, reach[m]);
    });
    reach[id] = covered;
    return (memo[id] = res);
  };

  // Each edge once per chain (a file's `uses` folded into each member, a method walked with its class
  // and on its own), and each caller → target hop once (several same-file helpers calling it).
  const shown = new Set<ViewEdge | string>();
  const collect = (id: string, caller: string, hops: number) => {
    flat(id).forEach(({ e, file }) => {
      const t = ix.byId[e.to];
      const hop = caller + "\n" + e.kind + "\n" + e.to;
      if (shown.has(e) || shown.has(hop)) return;
      shown.add(e).add(hop);
      // ponytail: once full, only edges already in hand are counted in `truncated`; deeper calls are not walked.
      if (steps.length >= max) return void truncated++;
      const kind = e.kind as Step["kind"];
      steps.push({
        n: steps.length + 1, from: lifeline(caller), to: lifeline(t.id), label: kind === "uses" ? "uses" : kind === "renders" ? component(t.file) : t.name,
        target: t.id, kind, file, line: e.line || 0,
      });
      if (t.kind !== "system" && hops < depth && !seen[t.id]) {
        seen[t.id] = true;
        collect(t.id, t.id, hops + 1);
      }
    });
  };

  if (!ix.byId[start]) return { lifelines, steps, truncated };
  lifeline(start);
  seen[start] = true;
  collect(start, start, 0);
  return { lifelines, steps, truncated };
}

/** A component's name from its file: `Card.vue` → Card, `Card/index.vue` → Card. */
function component(file: string): string {
  const name = base(file).replace(/\.[a-z]+$/, "");
  return name === "index" ? (base(file.slice(0, -base(file).length - 1)) || name) : name;
}

export function chainToMermaid(c: CallChain): string {
  const out = ["sequenceDiagram", "  autonumber"];
  c.lifelines.forEach((p, i) => out.push("  participant p" + i + " as " + clean(p.label)));
  c.steps.forEach((s) => out.push("  p" + s.from + (s.kind === "http" ? "-)" : "->>") + "p" + s.to + ": " + clean(s.label)));
  return out.join("\n");
}

export function chainToPlantUml(c: CallChain): string {
  const out = ["@startuml", "autonumber"];
  c.lifelines.forEach((p, i) => out.push((p.system ? "database" : "participant") + ' "' + clean(p.label) + '" as p' + i));
  c.steps.forEach((s) => out.push("p" + s.from + (s.kind === "http" ? " ->> " : " -> ") + "p" + s.to + " : " + clean(s.label)));
  out.push("@enduml");
  return out.join("\n");
}

/** One list row per step, shared by the side panel and the overlay. */
export function stepRows(c: CallChain): { n: number; text: string; target: string }[] {
  return c.steps.map((s) => ({
    n: s.n,
    text: `${c.lifelines[s.from].label} → ${c.lifelines[s.to].label}  ${s.label}${s.file ? "  " + s.file + ":" + s.line : ""}`,
    target: s.target,
  }));
}
