import type { MapArrow, View } from "../types";
import { fitInner } from "../camera";
import { GRID } from "../nav";
import { inside, isInside, type Place } from "./places";

export const overlaps = (a: Place, b: Place, gap = 0) =>
  a.x < b.x + b.w + gap && b.x < a.x + a.w + gap && a.y < b.y + b.h + gap && b.y < a.y + a.h + gap;

const centre = (p: Place) => ({ x: p.x + p.w / 2, y: p.y + p.h / 2 });
const snap = (v: number) => Math.round(v / GRID) * GRID;

// `placed`: the parent's children, `box` (for its size) and the parent's inside when known
export function freeSpot(parent: string, box: string, placed: Record<string, Place>, arrows: Pick<MapArrow, "from" | "to">[]): Place {
  const me = placed[box], room = placed[inside(parent)];
  const others = Object.entries(placed).filter(([id]) => id !== box && !isInside(id));
  const by = new Map(others);
  const partners = arrows.flatMap((a) => (a.from === box ? [a.to] : a.to === box ? [a.from] : [])).filter((id) => by.has(id));
  const pts = (partners.length ? [...new Set(partners)].map((id) => by.get(id)!) : others.map(([, p]) => p)).map(centre);
  const aim = pts.length
    ? { x: pts.reduce((s, p) => s + p.x, 0) / pts.length, y: pts.reduce((s, p) => s + p.y, 0) / pts.length }
    : room ? centre(room) : { x: GRID + me.w / 2, y: GRID + me.h / 2 };
  const x0 = snap(aim.x - me.w / 2), y0 = snap(aim.y - me.h / 2);
  const free = (c: Place) => !others.some(([, o]) => overlaps(c, o, GRID));
  let best: Place | null = null, bestD = Infinity;
  for (let r = 0; r < 10_000 && (r - 1) * GRID <= bestD; r++) {
    for (let i = -r; i <= r; i++) {
      for (let j = -r; j <= r; j++) {
        if (Math.max(Math.abs(i), Math.abs(j)) !== r) continue;
        const c = { x: x0 + i * GRID, y: y0 + j * GRID, w: me.w, h: me.h }, m = centre(c);
        const d = Math.hypot(m.x - aim.x, m.y - aim.y);
        if (d < bestD && free(c)) [best, bestD] = [c, d];
      }
    }
  }
  return best ?? { x: x0, y: y0, w: me.w, h: me.h };
}

const DIRS = [[1, 0], [0, 1], [-1, 0], [0, -1]] as const;
const past = (b: Place, a: Place, [dx, dy]: readonly [number, number], gap: number): Place =>
  dx > 0 ? { ...b, x: a.x + a.w + gap } : dx < 0 ? { ...b, x: a.x - gap - b.w } : dy > 0 ? { ...b, y: a.y + a.h + gap } : { ...b, y: a.y - gap - b.h };

// each way slides on past whatever it lands on, so the result is clear of every box in `avoid`
function away(b: Place, a: Place, gap: number, avoid: Place[]): Place {
  let best = b, bestD = Infinity;
  for (const dir of DIRS) {
    let c = past(b, a, dir, gap);
    for (let hit = avoid.find((s) => overlaps(c, s)); hit; hit = avoid.find((s) => overlaps(c, s))) c = past(c, hit, dir, gap);
    const d = Math.hypot(c.x - b.x, c.y - b.y);
    if (d < bestD) [best, bestD] = [c, d];
  }
  return best;
}

// several held boxes stay where they are, together: only the others make room
export function nudge(placed: Record<string, Place>, held: string | string[], gap: number): Record<string, Place> {
  const out = { ...placed }, hold = (typeof held === "string" ? [held] : held).filter((id) => out[id]);
  if (!hold.length) return out;
  const ids = Object.keys(out).filter((id) => !hold.includes(id) && !isInside(id)).sort();
  const fixed = new Set(hold), queue = [...hold];
  while (queue.length) {
    const a = out[queue.shift()!];
    for (const id of ids) {
      if (fixed.has(id) || !overlaps(a, out[id])) continue;
      out[id] = away(out[id], a, gap, [...fixed].map((f) => out[f]));
      fixed.add(id);
      queue.push(id);
    }
  }
  for (const id of ids) {
    const others = [...hold, ...ids].filter((o) => o !== id), hit = others.find((o) => overlaps(out[o], out[id]));
    if (hit) out[id] = away(out[id], out[hit], gap, others.map((o) => out[o]));
  }
  return out;
}

type Tree = { root: string; parentOf: Map<string, string>; kidsOf: Map<string, string[]> };

function tree(view: View): Tree {
  const root = view.boxes.find((b) => !b.parent)?.id ?? "system";
  const parentOf = new Map<string, string>(), kidsOf = new Map<string, string[]>();
  for (const b of view.boxes) {
    if (!view.placed.frames[b.id] || b.parent === b.id || parentOf.has(b.id)) continue;
    const p = b.parent ?? root;
    parentOf.set(b.id, p);
    const kids = kidsOf.get(p);
    if (kids) kids.push(b.id);
    else kidsOf.set(p, [b.id]);
  }
  return { root, parentOf, kidsOf };
}

export function reparented(prev: View, next: View): string[] {
  const was = new Map(prev.boxes.map((b) => [b.id, b.parent]));
  return next.boxes.filter((b) => was.has(b.id) && was.get(b.id) !== b.parent).map((b) => b.id);
}

// the frame grows at the inside's scale, so nothing inside moves on screen
function grow(out: Record<string, Place>, t: Tree, p: string, gap: number): boolean {
  const room = out[inside(p)], kids = (t.kidsOf.get(p) ?? []).filter((id) => out[id]);
  if (!room || !kids.length) return false;
  const l = Math.min(...kids.map((id) => out[id].x)), top = Math.min(...kids.map((id) => out[id].y));
  const r = Math.max(...kids.map((id) => out[id].x + out[id].w)), b = Math.max(...kids.map((id) => out[id].y + out[id].h));
  const dl = Math.max(0, room.x + gap - l), dt = Math.max(0, room.y + gap - top);
  const dr = Math.max(0, r + gap - (room.x + room.w)), db = Math.max(0, b + gap - (room.y + room.h));
  if (!(dl || dt || dr || db)) return false;
  out[inside(p)] = { x: room.x - dl, y: room.y - dt, w: room.w + dl + dr, h: room.h + dt + db };
  const f = out[p];
  if (!f || p === t.root) return false;
  const s = fitInner(f, room.w, room.h).s;
  out[p] = { x: f.x - dl * s, y: f.y - dt * s, w: f.w + (dl + dr) * s, h: f.h + (dt + db) * s };
  return true;
}

// a child whose id starts with the old one follows only when the view renamed it too
export function rekey(places: Record<string, Place>, renames: [string, string][], has: (id: string) => boolean): Record<string, Place> {
  let out = places;
  for (const [from, to] of renames) {
    if (from === to) continue;
    const next: Record<string, Place> = {};
    for (const [k, p] of Object.entries(out)) {
      let key = k;
      if (k === from || k === inside(from)) key = to + k.slice(from.length);
      else if (k.startsWith(inside(from))) {
        const moved = to + k.slice(from.length), bare = (id: string) => (isInside(id) ? id.slice(0, -1) : id);
        if (!has(bare(k)) && has(bare(moved))) key = moved;
      }
      next[key] = p;
    }
    out = next;
  }
  return out;
}

function nudgeIn(out: Record<string, Place>, t: Tree, parent: string, held: string | string[]) {
  const sib: Record<string, Place> = {};
  for (const id of t.kidsOf.get(parent) ?? []) if (out[id]) sib[id] = out[id];
  Object.assign(out, nudge(sib, held, GRID));
}

function growUp(out: Record<string, Place>, t: Tree, p: string) {
  for (let q: string | undefined = p, n = 0; q !== undefined && n < 64; n++) {
    if (!grow(out, t, q, GRID)) return;
    const up = t.parentOf.get(q);
    if (up === undefined) return;
    nudgeIn(out, t, up, q);
    q = up;
  }
}

export function settle(prev: Record<string, Place>, next: View, frames: Record<string, Place>, held: string | null): Record<string, Place> {
  const t = tree(next), out: Record<string, Place> = {}, known = new Set<string>();
  for (const p of new Set([...t.kidsOf.keys(), ...Object.keys(next.placed.inner)])) {
    const k = inside(p), was = prev[k], [w, h] = next.placed.inner[p] ?? [0, 0];
    if (was) known.add(p);
    if (was || next.placed.inner[p]) out[k] = was ?? { x: 0, y: 0, w, h };
  }
  const fresh: string[] = [];
  for (const [id, p] of t.parentOf) {
    if (frames[id]) out[id] = { ...frames[id] };
    else if (prev[id]) out[id] = prev[id];
    else if (known.has(p)) fresh.push(id);
    else out[id] = { ...next.placed.frames[id] };
  }
  const touched = new Set<string>();
  for (const id of fresh) {
    const p = t.parentOf.get(id)!, sib: Record<string, Place> = { [id]: next.placed.frames[id] };
    for (const k of t.kidsOf.get(p) ?? []) if (out[k]) sib[k] = out[k];
    if (out[inside(p)]) sib[inside(p)] = out[inside(p)];
    out[id] = freeSpot(p, id, sib, next.arrows);
    touched.add(p);
  }
  const holds = new Map<string, string[]>();
  for (const id of new Set([...Object.keys(frames), ...(held ? [held] : [])])) {
    const p = t.parentOf.get(id);
    if (p !== undefined && out[id]) holds.set(p, [...(holds.get(p) ?? []), id]);
  }
  for (const [p, ids] of holds) {
    nudgeIn(out, t, p, ids);
    touched.add(p);
  }
  for (const p of touched) growUp(out, t, p);
  return out;
}

// an inside's origin moves to 0, so a box grown left or up keeps its children where they were
export function shownView(view: View, places: Record<string, Place>): View {
  const t = tree(view), origin = (p: string) => (p === t.root ? undefined : places[inside(p)]);
  const frames = { ...view.placed.frames }, inner = { ...view.placed.inner };
  for (const [id, p] of t.parentOf) {
    const at = places[id];
    if (!at) continue;
    const o = origin(p);
    frames[id] = o ? { x: at.x - o.x, y: at.y - o.y, w: at.w, h: at.h } : at;
  }
  for (const [k, r] of Object.entries(places)) {
    if (!isInside(k)) continue;
    const p = k.slice(0, -1);
    inner[p] = p === t.root ? [r.x + r.w, r.y + r.h] : [r.w, r.h];
  }
  const regions = view.placed.regions.map((r) => {
    const o = origin(r.parent);
    return o ? { ...r, frame: { ...r.frame, x: r.frame.x - o.x, y: r.frame.y - o.y } } : r;
  });
  return { ...view, placed: { frames, inner, regions } };
}
