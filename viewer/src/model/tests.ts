// The Tests toggle: with tests off, every node of a test file (`test`, set from ig.yaml `tests`)
// and every edge touching one is dropped. Findings stay, so the list still shows them.

import type { ViewData } from "../data";

export function withTests(D: ViewData, show: boolean): ViewData {
  if (show) return D;
  const gone = new Set<string>();
  D.nodes.forEach((n) => n.test && (gone.add(n.id), gone.add(n.file)));
  if (!gone.size) return D;
  return {
    ...D,
    nodes: D.nodes.filter((n) => !n.test),
    edges: D.edges.filter((e) => !gone.has(e.from) && !gone.has(e.to)),
  };
}
