// Architecture is flat: the children of the current scope, plus, dimmed as context, the siblings at
// every level above it that connect to the scope. Opening a directory makes it the scope.

import { aggregateEdges, ancestors, nodeData, type El, type Group, type Index } from "./graph";

/** The box that stands for node `id` at `scope`: a group, a module or a system. */
export function rep(ix: Index, scope: Group, id: string): string | null {
  const n = ix.byId[id];
  if (!n) return null;
  if (n.kind === "system") return id;
  const path = ancestors(ix, ix.treeOf[n.file]);
  const chain = ancestors(ix, scope);
  for (let i = 0; i < chain.length; i++) {
    if (i >= path.length) return n.file;
    if (path[i] !== chain[i]) return path[i].id;
  }
  return path.length > chain.length ? path[chain.length].id : n.file;
}

function insideScope(ix: Index, scope: Group, repId: string): boolean {
  if (ix.byId[repId] && ix.byId[repId].kind === "system") return false;
  const g = ix.groups[repId];
  if (g) return g.parent === scope;
  return ix.treeOf[repId] === scope;
}

export function architecture(ix: Index, scope: Group): El[] {
  const els: El[] = [];
  const shown: Record<string, boolean> = {};
  const inner: Record<string, boolean> = {};
  const repOf = (id: string) => rep(ix, scope, id);
  Object.keys(ix.modules).forEach((f) => {
    const r = repOf(f);
    if (r && insideScope(ix, scope, r)) inner[r] = true;
  });
  const edges = aggregateEdges(ix, repOf, () => true).filter((e) => inner[e.source] || inner[e.target]);
  const linked: Record<string, boolean> = {};
  edges.forEach((e) => (linked[e.source] = linked[e.target] = true));
  const add = (id: string, outer: boolean) => {
    if (shown[id]) return;
    shown[id] = true;
    const g = ix.groups[id];
    if (ix.byId[id] && ix.byId[id].kind === "system") {
      const sd = nodeData(ix, ix.byId[id]);
      sd.kind = "system";
      sd.outer = outer;
      els.push({ group: "nodes", data: sd });
    } else if (g) {
      els.push({ group: "nodes", data: { id, label: g.label + " · " + g.count, kind: g.layer ? "layer" : "dir", count: g.count,
        status: g.dirty ? "changed" : "unchanged", findings: g.findings, outer } });
    } else {
      const m = ix.modules[id];
      const d = nodeData(ix, m.node);
      d.status = m.node.status !== "unchanged" ? m.node.status : m.dirty ? "changed" : "unchanged";
      d.findings = m.findings;
      d.outer = outer;
      els.push({ group: "nodes", data: d });
    }
  };
  Object.keys(inner).forEach((id) => add(id, false));
  Object.keys(linked).forEach((id) => add(id, true));
  edges.forEach((e, i) => {
    if (shown[e.source] && shown[e.target]) {
      e.id = "e" + i;
      e.ctx = !(inner[e.source] && inner[e.target]);
      els.push({ group: "edges", data: e });
    }
  });
  return els;
}

/** The first scope: the root, skipping levels with a single box so the first screen is never one box. */
export function initialScope(ix: Index): Group {
  let scope = ix.root;
  while (Object.keys(scope.children).length === 1 && scope.mods.length === 0) scope = scope.children[Object.keys(scope.children)[0]];
  return scope;
}
