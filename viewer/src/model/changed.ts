// Changed mode: changed nodes and their direct neighbours, drawn as exact nodes boxed in their modules.

import { aggregateEdges, base, nodeData, type El, type Index } from "./graph";

export function changedIds(ix: Index): Record<string, boolean> {
  const ids: Record<string, boolean> = {};
  const changed: Record<string, boolean> = {};
  ix.D.nodes.forEach((n) => {
    if (n.status !== "unchanged") changed[n.id] = ids[n.id] = true;
  });
  ix.D.edges.forEach((e) => {
    if (changed[e.from]) ids[e.to] = true;
    if (changed[e.to]) ids[e.from] = true;
  });
  return ids;
}

/** Exact nodes (ids), members boxed in their modules. Also used by Focus. */
export function exact(ix: Index, ids: Record<string, boolean>): El[] {
  const els: El[] = [];
  const shown: Record<string, boolean> = {};
  Object.keys(ids).forEach((id) => {
    const n = ix.byId[id];
    if (!n || n.module || n.kind === "system") return;
    if (!shown[n.file]) {
      const m = ix.byId[n.file];
      els.push({ group: "nodes", data: { id: n.file, label: base(n.file), kind: "openmodule", status: m.status, planned: !!m.planned } });
      shown[n.file] = true;
    }
    els.push({ group: "nodes", data: nodeData(ix, n, n.file) });
    shown[id] = true;
  });
  Object.keys(ids).forEach((id) => {
    const n = ix.byId[id];
    if (n && (n.module || n.kind === "system") && !shown[id]) {
      const d = nodeData(ix, n);
      if (n.kind === "system") d.kind = "system";
      els.push({ group: "nodes", data: d });
      shown[id] = true;
    }
  });
  aggregateEdges(ix, (x) => x, (x) => !!shown[x]).forEach((e, i) => {
    e.id = "e" + i;
    els.push({ group: "edges", data: e });
  });
  return els;
}
