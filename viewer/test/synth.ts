// A synthetic project for timings: `n` modules (one function each) in 6 layers of 10 directories,
// about 3 edges per module, some changed. Deterministic; generated at runtime, never committed.
// `flat` puts every module in one directory without a layer: Architecture then draws all of them.

import type { ViewData, ViewEdge, ViewNode } from "../src/data";

const LAYERS = ["ui", "app", "domain", "ports", "adapters", "infra"];

export function synthGraph(n: number, o: { flat?: boolean; changed?: number } = {}): ViewData {
  let s = 42;
  const rnd = () => ((s = (s * 1664525 + 1013904223) >>> 0) / 2 ** 32);
  const nodes: ViewNode[] = [];
  const edges: ViewEdge[] = [];
  const layerOf = (i: number) => Math.floor((i * LAYERS.length) / n);
  const changed = o.changed ?? 0.05;
  const node = (id: string, file: string, name: string, module: boolean, layer: string | null): ViewNode => ({
    id, kind: module ? "module" : "function", name, file, line: 1, endLine: 10, parent: module ? null : file,
    status: !module && rnd() < changed ? "changed" : "unchanged", complexity: 1, loc: 10, intents: [], module, layer, lane: null,
  });
  for (let i = 0; i < n; i++) {
    const l = layerOf(i);
    const file = o.flat ? `src/all/m${i}.ts` : `src/${LAYERS[l]}/d${i % 10}/m${i}.ts`;
    const layer = o.flat ? null : LAYERS[l];
    nodes.push(node(file, file, `m${i}.ts`, true, layer), node(file + "#f" + i, file, "f" + i, false, layer));
  }
  for (let i = 0; i < n; i++) {
    for (let k = 0; k < 3; k++) {
      // Mostly to a lower layer (higher index), sometimes upwards: a violation.
      const j = Math.min(n - 1, i + 1 + Math.floor(rnd() * (n / 3)));
      if (j === i) continue;
      edges.push({ from: nodes[2 * i + 1].id, to: nodes[2 * j + 1].id, kind: "calls", confidence: "resolved", status: "unchanged",
        line: 1, violation: rnd() < 0.01 });
    }
  }
  const counts = { added: 0, changed: nodes.filter((x) => x.status === "changed").length, removed: 0, unchanged: 0 };
  counts.unchanged = nodes.length - counts.changed;
  return {
    nodes, edges, findings: [], intents: [], intentChanges: [], notices: [], editor: "", root: "/synth", mode: "architecture",
    layers: o.flat ? [] : LAYERS, lanes: [], blocking: false, base: null, head: null, counts, state: "plain",
  };
}
