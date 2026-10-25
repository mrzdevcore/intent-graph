// Call chain: calls followed from a page or function across modules, through HTTP into
// routes, down to systems; one lifeline per blueprint lane (else layer, else module).

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
  const lifelines: Lifeline[] = [{ key: "user", label: "User" }];
  const at: Record<string, number> = { user: 0 };
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

  // Walk `id`'s calls as `caller`'s own. Same-file targets are folded into the caller.
  const collect = (id: string, caller: string, hops: number) => {
    const n = ix.byId[id];
    if (!n) return;
    let out = ix.outEdges[id] || [];
    if (!n.module && !usesDone[n.file]) {
      usesDone[n.file] = true;
      out = out.concat((ix.outEdges[n.file] || []).filter((e) => e.kind === "uses"));
    }
    if (n.module)
      (ix.membersOf[id] || []).forEach((m) => {
        seen[m] = true;
        out = out.concat(ix.outEdges[m] || []);
      });
    out = out
      .filter((e) => {
        const t = ix.byId[e.to];
        return KINDS[e.kind] && t && !t.test && e.confidence !== "guessed" && !(t.kind === "class" && /(Error|Exception)$/.test(t.name));
      })
      .sort((a, b) => (a.line || 0) - (b.line || 0));
    out.forEach((e) => {
      const t = ix.byId[e.to];
      if (t.kind !== "system" && t.file === n.file) {
        if (!seen[t.id]) {
          seen[t.id] = true;
          collect(t.id, caller, hops);
        }
        return;
      }
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

  if (!ix.byId[start]) return { lifelines, steps, truncated };
  lifeline(start);
  seen[start] = true;
  collect(start, start, 0);
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
