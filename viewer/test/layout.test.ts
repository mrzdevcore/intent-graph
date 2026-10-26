import { expect, test } from "vitest";
import type { ViewData } from "../src/data";
import { changedIds, exact } from "../src/model/changed";
import { index } from "../src/model/graph";
import { positions } from "../src/model/layout";
import { background, toGraphology } from "../src/model/sigma";
import fixture from "./fixture-data.json";
import { synthGraph } from "./synth";

test("positions is deterministic for a seed, and members sit inside their module's box", () => {
  const ix = index(fixture as ViewData);
  const els = exact(ix, changedIds(ix));
  const a = positions(els, 7);
  expect(positions(els, 7)).toEqual(a);
  const leaves = els.filter((e) => e.group === "nodes" && !els.some((c) => c.data.parent === e.data.id));
  leaves.forEach((e) => expect(Number.isFinite(a[e.data.id].x) && Number.isFinite(a[e.data.id].y)).toBe(true));
  expect(Object.keys(a).sort()).toEqual(leaves.map((e) => e.data.id).sort());
  // Two module boxes never overlap.
  els.forEach((e) => e.group === "nodes" && a[e.data.id] && (e.position = a[e.data.id]));
  const boxes = background(els).groups;
  boxes.forEach((p, i) => boxes.slice(i + 1).forEach((q) =>
    expect(p.x1 <= q.x0 || q.x1 <= p.x0 || p.y1 <= q.y0 || q.y1 <= p.y0).toBe(true)));
  // A previous layout is kept for the nodes it has.
  const id = leaves[0].data.id;
  expect(positions(els, 7, { [id]: { x: 5000, y: 5000 } })[id]).toEqual({ x: 5000, y: 5000 });
});

test("3,000 modules convert and lay out under 400 ms", () => {
  const ix = index(synthGraph(3000));
  const ids = Object.fromEntries(ix.D.nodes.map((n) => [n.id, true]));
  const els = exact(ix, ids);
  expect(els.filter((e) => e.data.kind === "openmodule").length).toBe(3000);
  const t0 = performance.now();
  const p = positions(els, 1);
  els.forEach((e) => e.group === "nodes" && p[e.data.id] && (e.position = p[e.data.id]));
  const g = toGraphology(els);
  background(els);
  const ms = performance.now() - t0;
  expect(g.order).toBe(3000);
  expect(g.size).toBeGreaterThan(8000);
  expect(ms).toBeLessThan(400);
});
