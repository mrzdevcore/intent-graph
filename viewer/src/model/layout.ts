// Positions for Changed and Focus (Architecture and Blueprint come with their own). Compounds are
// laid out as units: a unit's leaves sit in a grid inside it, and the units are placed by d3-force
// with collision on their radius, so two boxes never overlap (up to 1,000 units). Seeded, so the same input gives the
// same picture; the ticks are bounded so a 3,000-unit graph stays well under half a second.

import { forceCollide, forceLink, forceManyBody, forceSimulation, forceX, forceY } from "d3-force";
import type { El } from "./graph";

type P = { x: number; y: number };
interface Unit {
  index?: number;
  x?: number;
  y?: number;
  fx?: number;
  fy?: number;
  r: number;
  leaves: string[];
  /** Each leaf's offset from the unit's centre. */
  off: P[];
}

const CELL = { w: 110, h: 60 };

/** Leaf positions; leaves in `prev` keep theirs (their unit is pinned there). */
export function positions(els: El[], seed: number, prev: Record<string, P> = {}): Record<string, P> {
  const nodes = els.filter((e) => e.group === "nodes");
  const parentOf: Record<string, string | undefined> = {};
  nodes.forEach((e) => (parentOf[e.data.id] = e.data.parent));
  const hasKids = new Set(nodes.map((e) => e.data.parent).filter(Boolean));
  const root = (id: string) => {
    while (parentOf[id]) id = parentOf[id]!;
    return id;
  };
  const byRoot: Record<string, string[]> = {};
  nodes.forEach((e) => !hasKids.has(e.data.id) && (byRoot[root(e.data.id)] = byRoot[root(e.data.id)] || []).push(e.data.id));
  let s = seed >>> 0 || 1;
  const rnd = () => ((s = (s * 1664525 + 1013904223) >>> 0) / 2 ** 32);
  const units: Unit[] = [];
  const unitOf: Record<string, number> = {};
  Object.keys(byRoot).forEach((r) => {
    const leaves = byRoot[r];
    const cols = Math.ceil(Math.sqrt(leaves.length));
    const rows = Math.ceil(leaves.length / cols);
    const off = leaves.map((_, i) => ({ x: ((i % cols) - (cols - 1) / 2) * CELL.w, y: (Math.floor(i / cols) - (rows - 1) / 2) * CELL.h }));
    const u: Unit = { r: Math.hypot(((cols - 1) * CELL.w) / 2 + 50, ((rows - 1) * CELL.h) / 2 + 50), leaves, off };
    const known = leaves.findIndex((l) => prev[l]);
    if (known >= 0) [u.fx, u.fy] = [prev[leaves[known]].x - off[known].x, prev[leaves[known]].y - off[known].y];
    leaves.forEach((l) => (unitOf[l] = units.length));
    units.push(u);
  });
  // A seeded spiral start: new units near the pinned ones' centre.
  const pinned = units.filter((u) => u.fx !== undefined);
  const c = pinned.length ? { x: pinned.reduce((a, u) => a + u.fx!, 0) / pinned.length, y: pinned.reduce((a, u) => a + u.fy!, 0) / pinned.length } : { x: 0, y: 0 };
  const k = (1.3 * units.reduce((a, u) => a + u.r, 0)) / (units.length || 1);
  units.forEach((u, i) => {
    const a = i * Math.PI * (3 - Math.sqrt(5)) + rnd() * 0.5;
    const d = k * Math.sqrt(i + 0.5);
    [u.x, u.y] = u.fx !== undefined ? [u.fx, u.fy] : [c.x + d * Math.cos(a), c.y + d * Math.sin(a)];
  });
  const seen = new Set<string>();
  const links: { source: number; target: number }[] = [];
  els.forEach((e) => {
    if (e.group !== "edges") return;
    const [a, b] = [unitOf[e.data.source!] ?? unitOf[byRoot[root(e.data.source!)]?.[0]], unitOf[e.data.target!] ?? unitOf[byRoot[root(e.data.target!)]?.[0]]];
    if (a === undefined || b === undefined || a === b || seen.has(a + "|" + b)) return;
    seen.add(a + "|" + b);
    links.push({ source: a, target: b });
  });
  if (units.some((u) => u.fx === undefined)) {
    // ponytail: ticks shrink with size (300 for small graphs, 10 at 3,000 units); a worker if big graphs need better.
    const ticks = Math.max(10, Math.min(300, Math.round(30000 / units.length)));
    const sim = forceSimulation(units).randomSource(rnd).stop()
      .force("link", forceLink<Unit, { source: number; target: number }>(links).distance((l) => (l.source as unknown as Unit).r + (l.target as unknown as Unit).r + 40).strength(0.3))
      .force("charge", forceManyBody<Unit>().strength((u) => -60 - u.r * 2).theta(1.2))
      .force("x", forceX<Unit>(c.x).strength(0.04)).force("y", forceY<Unit>(c.y).strength(0.04));
    // Collision is most of the cost; past 1,000 units the spaced spiral start keeps boxes mostly apart.
    if (units.length <= 1000) sim.force("collide", forceCollide<Unit>((u) => u.r + 12).iterations(2));
    sim.alphaDecay(1 - Math.pow(0.001, 1 / ticks)).tick(ticks);
  }
  const out: Record<string, P> = {};
  units.forEach((u) => u.leaves.forEach((l, i) => (out[l] = { x: u.x! + u.off[i].x, y: u.y! + u.off[i].y })));
  return out;
}
