// The map's search: boxes by title, kind, tech or file path, and "A to B": the chain of arrows
// from A to B, at the level where they meet.
import type { MapArrow, MapBox, View } from "./types";

export type Hit = { box: MapBox; where: string; path?: string };
export type Trace = { parent: string; boxes: string[]; arrows: MapArrow[]; reversed: boolean };

const ROOT = "system";

/** "web to stripe", "web -> stripe", "web → stripe": the two ends; anything else, null. */
export function ends(q: string): [string, string] | null {
  const m = q.split(/\s+(?:to|->|→)\s+/i);
  return m.length === 2 && m[0].trim() && m[1].trim() ? [m[0].trim(), m[1].trim()] : null;
}

/** The boxes `q` names, best first: title start, title, kind or tech, then a file path. */
export function find(view: View, q: string, limit = 8): Hit[] {
  const s = q.trim().toLowerCase();
  if (!s) return [];
  const byId = new Map(view.boxes.map((b) => [b.id, b]));
  const where = (b: MapBox) => {
    const out: string[] = [];
    // `seen`: a bad map's parent loop ends the walk
    for (let p = b.parent, seen = new Set([b.id]); p && p !== ROOT && !seen.has(p); p = byId.get(p)?.parent) {
      seen.add(p);
      out.unshift(byId.get(p)?.title.value ?? p);
    }
    return out.join(" › ");
  };
  const scored: [number, Hit][] = [];
  for (const b of view.boxes) {
    if (b.id === ROOT) continue;
    const t = b.title.value.toLowerCase();
    const path = b.madeOf.find((p) => p.toLowerCase().includes(s));
    const rank = t.startsWith(s) ? 0 : t.includes(s) ? 1
      : [b.kind.value, b.tech?.value ?? ""].some((x) => x.toLowerCase().includes(s)) ? 2 : path ? 3 : -1;
    if (rank >= 0) scored.push([rank, { box: b, where: where(b), path: rank === 3 ? path : undefined }]);
  }
  return scored.sort((a, b) => a[0] - b[0] || a[1].box.title.value.length - b[1].box.title.value.length)
    .slice(0, limit).map(([, h]) => h);
}

/** `id` and its parents, the system first. */
function line(view: View, id: string): string[] {
  const byId = new Map(view.boxes.map((b) => [b.id, b]));
  const out = [id];
  for (let p = byId.get(id)?.parent ?? ROOT; !out.includes(p); p = byId.get(p)?.parent ?? ROOT) {
    out.unshift(p);
    if (p === ROOT) break;
  }
  return out;
}

/** The shortest chain of arrows from `a` to `b` (else from `b` to `a`: `reversed`), between the
 *  boxes that hold them in the box where their parents meet; null when one holds the other or
 *  nothing joins them. */
export function trace(view: View, a: string, b: string): Trace | null {
  const la = line(view, a), lb = line(view, b);
  let i = 0;
  while (i < la.length && i < lb.length && la[i] === lb[i]) i++;
  if (i === la.length || i === lb.length) return null;
  const parent = la[i - 1], from = la[i], to = lb[i];
  const arrows = view.arrows.filter((x) => x.parent === parent);
  for (const [s, e, reversed] of [[from, to, false], [to, from, true]] as const) {
    // breadth first: the first time `e` is reached, the chain is a shortest one
    const via = new Map<string, MapArrow | null>([[s, null]]);
    for (let todo = [s]; todo.length && !via.has(e); ) {
      const next: string[] = [];
      for (const n of todo) for (const x of arrows) {
        if (x.from === n && !via.has(x.to)) (via.set(x.to, x), next.push(x.to));
      }
      todo = next;
    }
    if (!via.has(e)) continue;
    const chain: MapArrow[] = [];
    for (let x = via.get(e); x; x = via.get(x.from)) chain.unshift(x);
    return { parent, boxes: [s, ...chain.map((x) => x.to)], arrows: chain, reversed };
  }
  return null;
}
