// Sequence diagrams: calls walked from a node, as Mermaid and PlantUML text.

import { base, topMember, type Index } from "./graph";

const SEQ_KINDS: Record<string, boolean> = { calls: true, dispatch: true, http: true, uses: true };
const MAX_MSGS = 60;

export interface Participant {
  key: string;
  label: string;
  system?: boolean;
}
export interface Message {
  from: number;
  to: number;
  label: string;
  kind: string;
}
export interface Sequence {
  parts: Participant[];
  msgs: Message[];
}

function participantOf(ix: Index, id: string): Participant {
  const n = ix.byId[id];
  if (!n) return { key: id, label: id };
  if (n.kind === "system") return { key: id, label: n.name, system: true };
  if (n.kind === "route") return { key: n.file, label: base(n.file).replace(/\.[a-z]+$/, "") };
  const t = n.module ? n : topMember(ix, n);
  const label = !n.module && t.kind === "class" ? t.name : base(n.file).replace(/\.[a-z]+$/, "");
  return { key: !n.module && t.kind === "class" ? t.id : n.file, label };
}

export function sequence(ix: Index, start: string): Sequence {
  const parts: Participant[] = [];
  const index: Record<string, number> = {};
  const msgs: Message[] = [];
  const seen: Record<string, boolean> = {};
  const usesDone: Record<string, boolean> = {};
  const part = (id: string) => {
    const p = participantOf(ix, id);
    if (index[p.key] === undefined) {
      index[p.key] = parts.length;
      parts.push(p);
    }
    return index[p.key];
  };
  const walk = (id: string, depth: number) => {
    if (seen[id] || msgs.length >= MAX_MSGS) return;
    seen[id] = true;
    const n = ix.byId[id];
    if (!n) return;
    const from = part(id);
    // Guessed edges and error construction only add noise to a sequence.
    let out = (ix.outEdges[id] || []).filter((e) => {
      const t = ix.byId[e.to];
      return SEQ_KINDS[e.kind] && t && !t.test && e.confidence !== "guessed" && !(t.kind === "class" && /(Error|Exception)$/.test(t.name));
    });
    if (!n.module && !usesDone[n.file]) {
      usesDone[n.file] = true;
      out = out.concat((ix.outEdges[n.file] || []).filter((e) => e.kind === "uses"));
    }
    out.sort((a, b) => (a.line || 0) - (b.line || 0));
    out.forEach((e) => {
      if (msgs.length >= MAX_MSGS) return;
      const t = ix.byId[e.to];
      const to = part(e.to);
      const label = e.kind === "uses" ? "uses" : t.name;
      msgs.push({ from, to, label, kind: e.kind });
      if (depth < 4 && t.kind !== "system") walk(e.to, depth + 1);
    });
  };
  part(start);
  walk(start, 0);
  return { parts, msgs };
}

function clean(s: string): string {
  return String(s).replace(/[;#"]/g, " ").replace(/\s+/g, " ").trim();
}

export function toMermaid(sq: Sequence): string {
  const out = ["sequenceDiagram"];
  sq.parts.forEach((p, i) => out.push("  participant p" + i + " as " + clean(p.label)));
  sq.msgs.forEach((m) => out.push("  p" + m.from + (m.kind === "http" ? "-)" : "->>") + "p" + m.to + ": " + clean(m.label)));
  return out.join("\n");
}

export function toPlantUml(sq: Sequence): string {
  const out = ["@startuml"];
  sq.parts.forEach((p, i) => out.push((p.system ? "database" : "participant") + ' "' + clean(p.label) + '" as p' + i));
  sq.msgs.forEach((m) => out.push("p" + m.from + (m.kind === "http" ? " ->> " : " -> ") + "p" + m.to + " : " + clean(m.label)));
  out.push("@enduml");
  return out.join("\n");
}
