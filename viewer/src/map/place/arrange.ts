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

export type Across = "right" | "down";
export type Stage = { w: number; h: number };
export const STAGE: Stage = { w: 1400, h: 875 };
export const WRAP = 6;

// the sub-lines a long layer wraps into, row by row so the barycentre order still reads across
function wrap(layer: string[], lim: number): string[][] {
  const k = Math.ceil(layer.length / lim);
  return Array.from({ length: k }, (_, j) => layer.filter((_, i) => i % k === j));
}

function place(cols: string[][], links: Link[], size: (id: string) => Place, tall: (id: string, w: number) => number, grid: number, across: Across, lim: number): Record<string, Place> {
  const up = (v: number) => Math.ceil(v / grid) * grid;
  const margin = 4 * grid, layerGap = 4 * grid, lineGap = 2 * grid, gap = 2 * grid, right = across === "right";
  const near = new Map<string, Map<string, number>>();
  for (const l of links) for (const [a, b] of [[l.from, l.to], [l.to, l.from]]) {
    const m = near.get(a) ?? new Map<string, number>();
    m.set(b, (m.get(b) ?? 0) + l.weight);
    near.set(a, m);
  }
  // u runs along the flow, v across it
  const box: Record<string, { u: number; v: number; du: number; dv: number; w: number; h: number }> = {};
  const layers = cols.map((c) => wrap(c, c.length > lim ? lim : c.length || 1));
  let u = margin;
  for (const lines of layers) {
    lines.forEach((line, j) => {
      const sized = line.map((id) => {
        const w = right ? Math.max(...line.map((o) => size(o).w)) : size(id).w;
        return { id, w, h: tall(id, w) };
      });
      const du = Math.max(...sized.map((b) => (right ? b.w : b.h)));
      let v = margin;
      for (const b of sized) {
        box[b.id] = { u, v, du, dv: right ? b.h : b.w, w: b.w, h: b.h };
        v = up(v + box[b.id].dv + gap);
      }
      u = up(u + du + (j < lines.length - 1 ? lineGap : layerGap));
    });
  }
  const done = new Set<string>();
  const mid = (id: string) => box[id].v + box[id].dv / 2;
  for (const lines of layers) {
    if (lines.length > 1) {
      const rows = Math.max(...lines.map((l) => l.length)), slot = Math.max(...lines.flat().map((id) => box[id].dv));
      let s = 0, w = 0;
      for (const id of lines.flat()) for (const [o, ow] of near.get(id) ?? []) if (done.has(o)) [s, w] = [s + mid(o) * ow, w + ow];
      const span = rows * slot + (rows - 1) * gap, v0 = w ? up(s / w - span / 2) : margin;
      for (const line of lines) line.forEach((id, r) => (box[id].v = v0 + r * up(slot + gap) + (slot - box[id].dv) / 2));
    } else {
      let floor = -Infinity;
      for (const id of lines[0]) {
        const to = strongest(id, near, (o) => done.has(o)) ?? strongest(id, near, () => true);
        // level centres, not grid tops: the arrow between them is then straight
        box[id].v = to ? Math.max(floor, mid(to) - box[id].dv / 2) : Math.max(floor, margin);
        floor = up(box[id].v + box[id].dv + gap);
      }
    }
    for (const id of lines.flat()) done.add(id);
  }
  const top = Math.floor((Math.min(...Object.values(box).map((b) => b.v)) - margin) / grid) * grid;
  const out: Record<string, Place> = {};
  for (const [id, b] of Object.entries(box)) {
    const v = b.v - top;
    out[id] = right ? { x: b.u, y: v, w: b.w, h: b.h } : { x: v, y: b.u, w: b.w, h: b.h };
  }
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

function crossCount(ls: Link[], at: (id: string) => Place): number {
  let n = 0;
  for (let i = 0; i < ls.length; i++) for (let j = i + 1; j < ls.length; j++) {
    const a = ls[i], b = ls[j];
    if (a.from === b.from || a.from === b.to || a.to === b.from || a.to === b.to) continue;
    if (cross(at(a.from), at(a.to), at(b.from), at(b.to))) n++;
  }
  return n;
}

export function crossings(view: View, places: Record<string, Place>, parent: string, members?: string[]): number {
  const t = treeOf(view, places), ids = members ?? t.kidsOf.get(parent) ?? [];
  return crossCount(linksAmong(view, t, ids, kindsOf(view)), (id) => places[id] ?? view.placed.frames[id]);
}

// 0.8: fitTo's margin
export const fitZoom = (w: number, h: number, stage: Stage = STAGE) => Math.min((stage.w * 0.8) / w, (stage.h * 0.8) / h);

export const COMFORT = 0.6;
export type Score = { fit: number; readable: number; back: number; cross: number; wire: number };
// past a comfortable zoom a larger one buys nothing: arrows that flow one way, few crossings, short wires do
export function scoreOf(room: { w: number; h: number }, ls: Link[], at: (id: string) => Place, stage: Stage = STAGE, comfort = COMFORT): Score {
  const fit = fitZoom(room.w, room.h, stage);
  let wire = 0, right = 0, down = 0;
  for (const l of ls) {
    const a = at(l.from), b = at(l.to), p = centre(a), q = centre(b);
    // the wire seen runs between the boxes' edges: centres would favour the shorter axis of a wide box
    wire += Math.hypot(Math.max(0, a.x - b.x - b.w, b.x - a.x - a.w), Math.max(0, a.y - b.y - b.h, b.y - a.y - a.h));
    if (q.x <= p.x) right++;
    if (q.y <= p.y) down++;
  }
  return { fit, readable: Math.min(fit, comfort), back: Math.min(right, down), cross: crossCount(ls, at), wire };
}

// between new layouts the larger zoom settles a tie; what is on screen keeps it unless 10% smaller
const beats = (a: Score, b: Score, fresh = false) => {
  if (Math.abs(a.readable - b.readable) > 1e-6) return a.readable > b.readable;
  if (a.back !== b.back) return a.back < b.back;
  if (a.cross !== b.cross) return a.cross < b.cross;
  // within 15% the wires are a wash (rows sit closer than columns): the larger zoom decides
  if (a.wire < b.wire * 0.85 || b.wire < a.wire * 0.85) return a.wire < b.wire;
  return a.fit > b.fit * (fresh ? 1 : 1.1) + 1e-6;
};

// the head an open box keeps above its inside (fitInner's, in the box's own units)
const withHead = (c: number) => (c >= 3 * HEAD ? c + HEAD : (c * 4) / 3);

type Lay = { view: View; t: Tree; kindOf: (id: string) => string; inner: Map<string, [number, number]>; kept: Set<string>; grid: number; stage: Stage };
type Laid = { places: Record<string, Place>; room: { w: number; h: number }; links: Link[]; score: Score };

function layout(c: Lay, ids: string[], places: Record<string, Place>, stage: Stage = c.stage, fits: (room: { w: number; h: number }) => boolean = () => true, comfort = COMFORT): Laid | undefined {
  const { view, t, kindOf, inner, kept, grid } = c;
  const up = (v: number) => Math.ceil(v / grid) * grid;
  const size = (id: string) => places[id] ?? view.placed.frames[id];
  const tall = (id: string, w: number) => {
    const r = inner.get(id), f = size(id);
    if (kept.has(id) && w === f.w) return f.h;
    if (r) return withHead((r[1] * w) / r[0]);
    return t.kidsOf.get(id)?.length ? withHead(((f.h - Math.min(HEAD, f.h / 4)) * w) / f.w) : f.h;
  };
  const links = linksAmong(view, t, ids, kindOf), dag = flow(ids, links, kindOf);
  const cols = layers(ids, dag, kindOf);
  const longest = Math.max(...cols.map((col) => col.length));
  const lims = [...new Set([Math.min(longest, WRAP), ...[5, 4, 3, 2].filter((n) => n < longest)])];
  let best: Laid | undefined;
  for (const across of ["right", "down"] as const) {
    // the candidates never depend on where boxes are now, so arranging twice finds the same best
    for (const o of order(cols, links)) for (const lim of lims) {
      const p = place(o, links, size, tall, grid, across, lim);
      const room = { w: up(Math.max(...ids.map((id) => p[id].x + p[id].w)) + 4 * grid), h: up(Math.max(...ids.map((id) => p[id].y + p[id].h)) + 4 * grid) };
      if (!fits(room)) continue;
      const score = scoreOf(room, dag, (id) => p[id], stage, comfort);
      if (!best || beats(score, best.score, true)) best = { places: p, room, links: dag, score };
    }
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

function pass(scope: string[], view: View, t: Tree, places: Record<string, Place>, grid: number, stage: Stage): { out: Record<string, Place>; c: Lay } {
  const out = { ...places };
  const c: Lay = { view, t, kindOf: kindsOf(view), inner: new Map(), kept: new Set(), grid, stage };
  const was = (id: string) => places[id] ?? view.placed.frames[id];
  const frameOf = (g: string, { w, h }: { w: number; h: number }, f: Place) => {
    const fw = widthOf(view, g, w, h);
    return { x: f.x, y: f.y, w: fw, h: withHead((h * fw) / w) };
  };
  for (const g of groupsOf(scope, t, patterned(view))) {
    const kids = t.kidsOf.get(g)!, f = out[g] ?? view.placed.frames[g], top = g === t.root || (scope.length === 1 && scope[0] === g);
    const room = roomOf(view, g, kids, places, out, grid);
    // a group inside the one arranged keeps its frame or shrinks it, so what holds it never gets worse
    const own = top || !f ? stage : { w: f.w / 0.8, h: (f.h - Math.min(HEAD, f.h / 4)) / 0.8 };
    const fits = (r: { w: number; h: number }) => {
      const n = top || !f ? undefined : frameOf(g, r, f);
      return !n || (n.w <= f.w + 0.5 && n.h <= f.h + 0.5);
    };
    // a group's inside is read at its parent's zoom: there only the larger scale counts
    const comfort = top || !f ? COMFORT : Infinity, laid = layout(c, kids, out, own, fits, comfort);
    const now = room ? scoreOf(room, laid?.links ?? [], (id) => out[id], own, comfort) : undefined;
    if (!laid || (now && !beats(laid.score, now))) {
      if (kids.every((id) => out[id].w === was(id).w && out[id].h === was(id).h)) c.kept.add(g);
      continue;
    }
    Object.assign(out, laid.places);
    out[inside(g)] = { x: 0, y: 0, w: laid.room.w, h: laid.room.h };
    c.inner.set(g, [laid.room.w, laid.room.h]);
    if (f && g !== t.root) out[g] = frameOf(g, laid.room, f);
  }
  return { out, c };
}

function roomOf(view: View, g: string, kids: string[], stated: Record<string, Place>, at: Record<string, Place>, grid: number) {
  const r = stated[inside(g)] ?? (view.placed.inner[g] && { w: view.placed.inner[g][0], h: view.placed.inner[g][1] });
  return r && kids.every((id) => at[id]) ? {
    w: Math.max(r.w, ...kids.map((id) => at[id].x + at[id].w + 4 * grid)),
    h: Math.max(r.h, ...kids.map((id) => at[id].y + at[id].h + 4 * grid)),
  } : undefined;
}

// ig's own layout of a group's inside, all the way down; a patterned box keeps the inside it has
function igLayout(g: string, view: View, t: Tree, places: Record<string, Place>): Record<string, Place> {
  const ig = settle({}, view, {}, null), out = { ...places }, kept = patterned(view);
  const walk = (id: string) => {
    if (ig[inside(id)]) out[inside(id)] = ig[inside(id)];
    for (const k of t.kidsOf.get(id) ?? []) {
      if (ig[k]) out[k] = ig[k];
      if (!kept.has(k)) walk(k);
    }
  };
  walk(g);
  if (g !== t.root && out[g] && ig[g]) out[g] = { ...out[g], w: ig[g].w, h: ig[g].h };
  return out;
}

export function arrange(scope: string[], view: View, places: Record<string, Place>, grid: number, stage: Stage = STAGE): Record<string, Place> {
  const t = treeOf(view, places);
  const { out: mine, c } = pass(scope, view, t, places, grid, stage);
  let out = mine;
  const g = scope[0], kids = t.kidsOf.get(g) ?? [];
  if (scope.length === 1 && kids.length && !patterned(view).has(g)) {
    const { out: ig } = pass(scope, view, t, igLayout(g, view, t, places), grid, stage);
    const dag = flow(kids, linksAmong(view, t, kids, c.kindOf), c.kindOf);
    const score = (p: Record<string, Place>) => scoreOf(roomOf(view, g, kids, p, p, grid) ?? { w: 1, h: 1 }, dag, (id) => p[id], stage);
    const a = score(mine), d = score(ig);
    // ig's own layout is one more candidate: a map left in a bad state goes back to it
    if (beats(d, a)) out = ig;
  }
  // a group arranged alone changed size: its siblings make room and its parents grow
  if (scope.length === 1 && t.parentOf.has(g) && out[g]) return settle(out, view, { [g]: out[g] }, g);
  const sel = scope.filter((id) => t.parentOf.has(id)), parent = t.parentOf.get(sel[0]);
  if (scope.length < 2 || sel.length < 2 || sel.some((id) => t.parentOf.get(id) !== parent)) return out;
  const before = sel.map((id) => places[id] ?? view.placed.frames[id]);
  const x0 = Math.round(Math.min(...before.map((p) => p.x)) / grid) * grid, y0 = Math.round(Math.min(...before.map((p) => p.y)) / grid) * grid;
  const laid = layout(c, [...sel].sort(), out)!;
  const lx = Math.min(...sel.map((id) => laid.places[id].x)), ly = Math.min(...sel.map((id) => laid.places[id].y));
  const block = (p: Record<string, Place>) => {
    const l = Math.min(...sel.map((id) => p[id].x)), tp = Math.min(...sel.map((id) => p[id].y));
    return { w: Math.max(...sel.map((id) => p[id].x + p[id].w)) - l + 8 * grid, h: Math.max(...sel.map((id) => p[id].y + p[id].h)) - tp + 8 * grid };
  };
  if (!beats(laid.score, scoreOf(block(out), laid.links, (id) => out[id], stage))) return out;
  for (const id of sel) out[id] = { ...laid.places[id], x: laid.places[id].x - lx + x0, y: laid.places[id].y - ly + y0 };
  const hold = "\0these", r = Math.max(...sel.map((id) => out[id].x + out[id].w)), b = Math.max(...sel.map((id) => out[id].y + out[id].h));
  const others: Record<string, Place> = { [hold]: { x: x0, y: y0, w: r - x0, h: b - y0 } };
  for (const k of t.kidsOf.get(parent!) ?? []) if (!sel.includes(k) && out[k]) others[k] = out[k];
  const moved = nudge(others, hold, grid);
  delete moved[hold];
  return { ...out, ...moved };
}
