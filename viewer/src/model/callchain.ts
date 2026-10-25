// Call chain: calls followed from a page or function across modules, through HTTP into
// routes, down to systems; one lifeline per blueprint lane (else layer, else module).

import type { ViewEdge, ViewNode } from "../data";
import { base, type Index } from "./graph";

const KINDS: Record<string, boolean> = { calls: true, dispatch: true, http: true, uses: true };

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
  kind: "calls" | "dispatch" | "http" | "uses";
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
  const depth = opts.depth ?? 6;
  const max = opts.max ?? 60;
  const lifelines: Lifeline[] = [];
  const at: Record<string, number> = {};
  const steps: Step[] = [];
  let truncated = 0;
  const seen: Record<string, boolean> = {};
  const usesDone: Record<string, boolean> = {};

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
  const path: Record<string, boolean> = {};

  // Walk `id`'s calls as `caller`'s own. Same-file targets are folded into the caller, once per caller.
  const collect = (id: string, caller: string, hops: number) => {
    const n = ix.byId[id];
    if (!n) return;
    let out = ix.outEdges[id] || [];
    if (!n.module && !usesDone[n.file]) {
      usesDone[n.file] = true;
      out = out.concat((ix.outEdges[n.file] || []).filter((e) => e.kind === "uses"));
    }
    out = out.filter(okEdge).sort((a, b) => (a.line || 0) - (b.line || 0));
    if (n.module) {
      // The module's members are all start points: fold the ones nothing else in the file calls.
      const members = ix.membersOf[id] || [];
      const called: Record<string, boolean> = {};
      members.forEach((m) => (ix.outEdges[m] || []).filter(okEdge).forEach((e) => same(n, e) && (called[e.to] = true)));
      out.forEach((e) => same(n, e) && (called[e.to] = true));
      members.filter((m) => !called[m]).forEach((m) => fold(m, caller, hops));
    }
    out.forEach((e) => {
      const t = ix.byId[e.to];
      if (same(n, e)) return fold(t.id, caller, hops);
      if (steps.length >= max) truncated++;
      else {
        const kind = e.kind as Step["kind"];
        steps.push({
          n: steps.length + 1, from: lifeline(caller), to: lifeline(t.id), label: kind === "uses" ? "uses" : t.name,
          target: t.id, kind, file: n.file, line: e.line || 0,
        });
      }
      if (t.kind !== "system" && hops < depth && !seen[t.id]) {
        seen[t.id] = true;
        collect(t.id, t.id, hops + 1);
      }
    });
  };
  const fold = (id: string, caller: string, hops: number) => {
    if (path[id]) return;
    path[id] = true;
    collect(id, caller, hops);
    delete path[id];
  };

  if (!ix.byId[start]) return { lifelines, steps, truncated };
  lifeline(start);
  seen[start] = true;
  fold(start, start, 0);
  return { lifelines, steps, truncated };
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
