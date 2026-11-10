import type { View } from "../types";
import { HEAD, fitInner } from "../camera";
import { inside, type Place } from "./places";
import { nudge, settle } from "./rules";

export type Link = { from: string; to: string; weight: number };
type Tree = { root: string; parentOf: Map<string, string>; kidsOf: Map<string, string[]> };

const LAST = new Set(["store", "external"]);
const SWEEPS = 4;

function treeOf(view: View, places: Record<string, Place>): Tree {
  const root = view.boxes.find((b) => !b.parent)?.id ?? "system";
  const parentOf = new Map<string, string>(), kidsOf = new Map<string, string[]>();
  for (const b of [...view.boxes].sort((a, c) => (a.id < c.id ? -1 : 1))) {
    if (b.id === root || b.parent === b.id || parentOf.has(b.id) || !(places[b.id] ?? view.placed.frames[b.id])) continue;
    const p = b.parent ?? root;
    parentOf.set(b.id, p);
    kidsOf.set(p, [...(kidsOf.get(p) ?? []), b.id]);
  }
  return { root, parentOf, kidsOf };
}

const depth = (t: Tree, id: string) => {
  let n = 0;
  for (let p = t.parentOf.get(id); p !== undefined && n < 64; p = t.parentOf.get(p)) n++;
  return n;
};

// an arrow between two boxes deep inside counts between the members that hold them
function linksAmong(view: View, t: Tree, members: string[], kindOf: (id: string) => string): Link[] {
  const set = new Set(members), sum = new Map<string, Link>();
  const lift = (id: string) => {
    for (let q: string | undefined = id, n = 0; q !== undefined && n < 64; q = t.parentOf.get(q), n++) if (set.has(q)) return q;
    return undefined;
  };
  for (const a of view.arrows) {
    if (a.ghost) continue;
    let from = lift(a.from), to = lift(a.to);
    if (!from || !to || from === to) continue;
    // a reader's arrow points at the queue, but the messages flow from the queue to it
    if (a.kind === "reads" && kindOf(to) === "queue") [from, to] = [to, from];
    const k = `${from}\n${to}`, l = sum.get(k);
    if (l) l.weight += a.count || 1;
    else sum.set(k, { from, to, weight: a.count || 1 });
  }
  return [...sum.values()].sort((a, b) => (a.from === b.from ? (a.to < b.to ? -1 : 1) : a.from < b.from ? -1 : 1));
}

function components(ids: string[], links: Link[]): Map<string, number> {
  const out = new Map<string, string[]>(ids.map((id) => [id, []]));
  for (const l of links) out.get(l.from)!.push(l.to);
  const index = new Map<string, number>(), low = new Map<string, number>(), comp = new Map<string, number>();
  const stack: string[] = [], on = new Set<string>();
  let n = 0, c = 0;
  const visit = (v: string) => {
    index.set(v, n);
    low.set(v, n++);
    stack.push(v);
    on.add(v);
    for (const w of out.get(v)!) {
      if (!index.has(w)) {
        visit(w);
        low.set(v, Math.min(low.get(v)!, low.get(w)!));
      } else if (on.has(w)) low.set(v, Math.min(low.get(v)!, index.get(w)!));
    }
    if (low.get(v) === index.get(v)) {
      for (let w = stack.pop(); w !== undefined; w = w === v ? undefined : stack.pop()) {
        on.delete(w);
        comp.set(w, c);
      }
      c++;
    }
  };
  for (const id of ids) if (!index.has(id)) visit(id);
  return comp;
}

export function flow(ids: string[], links: Link[], kindOf: (id: string) => string): Link[] {
  const comp = components(ids, links);
  return links.flatMap((l): Link[] => {
    if (kindOf(l.to) === "person") return [];
    return comp.get(l.from) === comp.get(l.to) && l.from > l.to ? [{ ...l, from: l.to, to: l.from }] : [l];
  });
}

function layers(ids: string[], dag: Link[], kindOf: (id: string) => string): string[][] {
  const preds = new Map<string, string[]>(ids.map((id) => [id, []]));
  for (const l of dag) preds.get(l.to)!.push(l.from);
  const layer = new Map<string, number>();
  const of = (id: string): number => {
    if (layer.has(id)) return layer.get(id)!;
    const ps = preds.get(id)!;
    const v = !ps.length ? 0 : Math.max(...ps.map((p) => of(p) + 1));
    layer.set(id, v);
    return v;
  };
  for (const id of ids) of(id);
  const calls = new Set(dag.map((l) => l.from));
  const last = Math.max(1, ...[...layer.entries()].filter(([id]) => !LAST.has(kindOf(id))).map(([, v]) => v + 1));
  if (ids.some((id) => !LAST.has(kindOf(id)))) for (const id of ids) if (LAST.has(kindOf(id)) && !calls.has(id)) layer.set(id, Math.max(layer.get(id)!, last));
  const used = [...new Set(layer.values())].sort((a, b) => a - b);
  return used.map((v) => ids.filter((id) => layer.get(id) === v).sort());
}

function order(cols: string[][], links: Link[]): string[][][] {
  const near = new Map<string, Map<string, number>>();
  const add = (a: string, b: string, w: number) => {
    const m = near.get(a) ?? new Map<string, number>();
    m.set(b, (m.get(b) ?? 0) + w);
    near.set(a, m);
  };
  for (const l of links) {
    add(l.from, l.to, l.weight);
    add(l.to, l.from, l.weight);
  }
  const colOf = new Map<string, number>();
  cols.forEach((c, i) => c.forEach((id) => colOf.set(id, i)));
  let cur = cols.map((c) => [...c]);
  const tries = [cur];
  const pos = () => new Map(cur.flatMap((c) => c.map((id, i): [string, number] => [id, (i + 0.5) / c.length])));
  const sortBy = (i: number, side: (j: number) => boolean) => {
    const p = pos();
    const bary = (id: string) => {
      let s = 0, w = 0;
      for (const [o, ow] of near.get(id) ?? []) if (side(colOf.get(o)!)) [s, w] = [s + p.get(o)! * ow, w + ow];
      return w ? s / w : p.get(id)!;
    };
    const b = new Map(cur[i].map((id) => [id, bary(id)]));
    cur[i] = [...cur[i]].sort((x, y) => b.get(x)! - b.get(y)! || (x < y ? -1 : 1));
  };
  for (let s = 0; s < SWEEPS; s++) {
    cur = cur.map((c) => [...c]);
    for (let i = 1; i < cur.length; i++) sortBy(i, (j) => j < i);
    for (let i = cur.length - 2; i >= 0; i--) sortBy(i, (j) => j > i);
    tries.push(cur);
  }
  return tries;
}

function strongest(id: string, near: Map<string, Map<string, number>>, ok: (o: string) => boolean): string | undefined {
  let best: string | undefined, bw = 0;
  for (const [o, w] of near.get(id) ?? []) if (ok(o) && (w > bw || (w === bw && best !== undefined && o < best))) [best, bw] = [o, w];
  return best;
}

function place(cols: string[][], links: Link[], size: (id: string) => Place, tall: (id: string, w: number) => number, grid: number): Record<string, Place> {
  const up = (v: number) => Math.ceil(v / grid) * grid;
  const margin = 4 * grid, colGap = 4 * grid, rowGap = 2 * grid;
  const near = new Map<string, Map<string, number>>();
  for (const l of links) for (const [a, b] of [[l.from, l.to], [l.to, l.from]]) {
    const m = near.get(a) ?? new Map<string, number>();
    m.set(b, (m.get(b) ?? 0) + l.weight);
    near.set(a, m);
  }
  const out: Record<string, Place> = {};
  let x = margin;
  for (const c of cols) {
    const w = Math.max(...c.map((id) => size(id).w));
    let y = margin;
    for (const id of c) {
      out[id] = { x, y, w, h: tall(id, w) };
      y = up(y + out[id].h + rowGap);
    }
    x = up(x + w + colGap);
  }
  const done = new Set<string>();
  const mid = (id: string) => out[id].y + out[id].h / 2;
  for (const c of cols) {
    let floor = -Infinity;
    for (const id of c) {
      const to = strongest(id, near, (o) => done.has(o)) ?? strongest(id, near, () => true);
      // level centres, not grid tops: the arrow between them is then straight
      const y = to ? Math.max(floor, mid(to) - out[id].h / 2) : Math.max(floor, margin);
      out[id] = { ...out[id], y };
      floor = up(y + out[id].h + rowGap);
    }
    for (const id of c) done.add(id);
  }
  const top = Math.floor((Math.min(...Object.values(out).map((p) => p.y)) - margin) / grid) * grid;
  if (top) for (const id of Object.keys(out)) out[id] = { ...out[id], y: out[id].y - top };
  return out;
}

const centre = (p: Place) => ({ x: p.x + p.w / 2, y: p.y + p.h / 2 });
function cross(a: Place, b: Place, c: Place, d: Place): boolean {
  const p = centre(a), q = centre(b), r = centre(c), s = centre(d);
  const o = (u: typeof p, v: typeof p, w: typeof p) => Math.sign((v.x - u.x) * (w.y - u.y) - (v.y - u.y) * (w.x - u.x));
  return o(p, q, r) * o(p, q, s) < 0 && o(r, s, p) * o(r, s, q) < 0;
}

const kindsOf = (view: View) => {
  const kinds = new Map(view.boxes.map((b) => [b.id, b.kind.value as string]));
  return (id: string) => kinds.get(id) ?? "box";
};

export function crossings(view: View, places: Record<string, Place>, parent: string, members?: string[]): number {
  const t = treeOf(view, places), ids = members ?? t.kidsOf.get(parent) ?? [];
  const at = (id: string) => places[id] ?? view.placed.frames[id];
  const ls = linksAmong(view, t, ids, kindsOf(view));
  let n = 0;
  for (let i = 0; i < ls.length; i++) for (let j = i + 1; j < ls.length; j++) {
    const a = ls[i], b = ls[j];
    if (a.from === b.from || a.from === b.to || a.to === b.from || a.to === b.to) continue;
    if (cross(at(a.from), at(a.to), at(b.from), at(b.to))) n++;
  }
  return n;
}

// the head an open box keeps above its inside (fitInner's, in the box's own units)
const withHead = (c: number) => (c >= 3 * HEAD ? c + HEAD : (c * 4) / 3);

type Lay = { view: View; t: Tree; kindOf: (id: string) => string; inner: Map<string, [number, number]>; grid: number };

function layout(c: Lay, ids: string[], places: Record<string, Place>): Record<string, Place> {
  const { view, t, kindOf, inner, grid } = c;
  const size = (id: string) => places[id] ?? view.placed.frames[id];
  const tall = (id: string, w: number) => {
    const r = inner.get(id), f = size(id);
    if (r) return withHead((r[1] * w) / r[0]);
    return t.kidsOf.get(id)?.length ? withHead(((f.h - Math.min(HEAD, f.h / 4)) * w) / f.w) : f.h;
  };
  const links = linksAmong(view, t, ids, kindOf);
  const cols = layers(ids, flow(ids, links, kindOf), kindOf);
  const now = cols.map((col) => [...col].sort((a, b) => size(a).y - size(b).y || (a < b ? -1 : 1)));
  let best: Record<string, Place> = {}, fewest = Infinity;
  for (const o of [now, ...order(cols, links)]) {
    const p = place(o, links, size, tall, grid), n = crossings(view, p, "", ids);
    if (n < fewest) [best, fewest] = [p, n];
  }
  return best;
}

// a box with a pattern keeps the inside the server drew its rings around
const patterned = (view: View) => new Set(view.boxes.filter((b) => b.pattern && b.pattern.value.kind !== "none").map((b) => b.id));

export function arranged(scope: string[], view: View, places: Record<string, Place>): string[] {
  const t = treeOf(view, places);
  return groupsOf(scope, t, patterned(view)).flatMap((g) => t.kidsOf.get(g) ?? []).concat(scope.length > 1 ? scope.filter((id) => t.parentOf.has(id)) : []);
}

function groupsOf(scope: string[], t: Tree, kept: Set<string>): string[] {
  const out = new Set<string>(), walk = (id: string) => {
    if (out.has(id) || kept.has(id) || !t.kidsOf.get(id)?.length) return;
    out.add(id);
    for (const k of t.kidsOf.get(id)!) walk(k);
  };
  for (const id of scope) {
    if (scope.length === 1) walk(id);
    else for (const k of t.kidsOf.get(id) ?? []) walk(k);
  }
  return [...out].sort((a, b) => depth(t, b) - depth(t, a) || (a < b ? -1 : 1));
}

// at least as wide as the server drew it, its inside at least at the server's scale
function widthOf(view: View, id: string, iw: number, ih: number): number {
  const f = view.placed.frames[id], r = view.placed.inner[id];
  return f && r ? Math.max(f.w, iw * fitInner(f, r[0], r[1]).s) : Math.max(iw, ih) / 4;
}

export function arrange(scope: string[], view: View, places: Record<string, Place>, grid: number): Record<string, Place> {
  const t = treeOf(view, places), out = { ...places }, up = (v: number) => Math.ceil(v / grid) * grid;
  const c: Lay = { view, t, kindOf: kindsOf(view), inner: new Map(), grid };
  for (const g of groupsOf(scope, t, patterned(view))) {
    const kids = t.kidsOf.get(g)!, laid = layout(c, kids, out);
    Object.assign(out, laid);
    const r = Math.max(...kids.map((id) => laid[id].x + laid[id].w)), b = Math.max(...kids.map((id) => laid[id].y + laid[id].h));
    const w = up(r + 4 * grid), h = up(b + 4 * grid), f = out[g] ?? view.placed.frames[g];
    out[inside(g)] = { x: 0, y: 0, w, h };
    c.inner.set(g, [w, h]);
    if (f && g !== t.root) {
      const fw = widthOf(view, g, w, h);
      out[g] = { x: f.x, y: f.y, w: fw, h: withHead((h * fw) / w) };
    }
  }
  const sel = scope.filter((id) => t.parentOf.has(id));
  // a group arranged alone changed size: its siblings make room and its parents grow
  if (scope.length === 1 && t.parentOf.has(scope[0]) && out[scope[0]]) return settle(out, view, { [scope[0]]: out[scope[0]] }, scope[0]);
  const parent = t.parentOf.get(sel[0]);
  if (scope.length < 2 || sel.length < 2 || sel.some((id) => t.parentOf.get(id) !== parent)) return out;
  const was = sel.map((id) => places[id] ?? view.placed.frames[id]);
  const x0 = Math.round(Math.min(...was.map((p) => p.x)) / grid) * grid, y0 = Math.round(Math.min(...was.map((p) => p.y)) / grid) * grid;
  const laid = layout(c, [...sel].sort(), out);
  const lx = Math.min(...sel.map((id) => laid[id].x)), ly = Math.min(...sel.map((id) => laid[id].y));
  for (const id of sel) out[id] = { ...laid[id], x: laid[id].x - lx + x0, y: laid[id].y - ly + y0 };
  const block = "\0these", r = Math.max(...sel.map((id) => out[id].x + out[id].w)), b = Math.max(...sel.map((id) => out[id].y + out[id].h));
  const others: Record<string, Place> = { [block]: { x: x0, y: y0, w: r - x0, h: b - y0 } };
  for (const k of t.kidsOf.get(parent!) ?? []) if (!sel.includes(k) && out[k]) others[k] = out[k];
  const moved = nudge(others, block, grid);
  delete moved[block];
  return { ...out, ...moved };
}
