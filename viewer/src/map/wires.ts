import type { ArrowShape, Frame, MapArrow } from "./types";
import { BEND, SPREAD, across, halfway, length, path, pins, pt, r2, straight, type End, type P, type Side } from "./shapes/path";

export type Wire = { d: string; mid: [number, number]; len: number; ends: [[number, number], [number, number]] };
export type Mode = "routed" | "ribbon";
export type Wiring = { wires: Map<string, Wire>; sleeves: { d: string; w: number }[] };

const LOOK = {
  routed: { pitch: 8, bend: BEND, load: 60 },
  // a negative load: a gap already used is cheaper, so wires gather
  ribbon: { pitch: 5, bend: 5, load: -200 },
};
/** How far a wire running past a box keeps from it. */
const CLEAR = 16;
/** The room taken left or right of every box, and under the last row, when a wire goes round. */
const OUTSIDE = 48;

type Run = { id: string; span: () => P; set: (v: number) => void };
type Grid = ReturnType<typeof grid>;
type Kinds = Record<string, string | undefined>;
type ShapeOf = (a: MapArrow) => ArrowShape;

// bends cut at 45°; an arrow not drawn as circuit takes `path` from the same pins
export function wires(arrows: MapArrow[], frames: Record<string, Frame>, mode: Mode, kinds: Kinds = {}, shape: ShapeOf = () => "circuit"): Wiring {
  const g = grid(frames, mode);
  return { wires: mode === "routed" ? routed(arrows, frames, kinds, shape, g) : ribbon(arrows, frames, g), sleeves: g.sleeves };
}

function grid(frames: Record<string, Frame>, mode: Mode) {
  const { pitch } = LOOK[mode];
  // rows: boxes whose heights overlap, top to bottom
  const rows: { top: number; bottom: number; ids: string[] }[] = [];
  const ids = Object.keys(frames).sort((a, b) => frames[a].y - frames[b].y || a.localeCompare(b));
  for (const id of ids) {
    const f = frames[id], r = rows[rows.length - 1];
    if (r && f.y < r.bottom) {
      r.ids.push(id);
      r.bottom = Math.max(r.bottom, f.y + f.h);
    } else rows.push({ top: f.y, bottom: f.y + f.h, ids: [id] });
  }
  // ponytail: a hand-moved box can chain rows into one tall row; a drop out of it may then cross a box
  const rowOf = new Map(rows.flatMap((r, i) => r.ids.map((id) => [id, i] as const)));
  const channel = (c: number) =>
    c < 0 ? { top: rows[0].top - 2 * OUTSIDE, bottom: rows[0].top } : { top: rows[c].bottom, bottom: rows[c + 1]?.top ?? rows[c].bottom + 2 * OUTSIDE };
  /** The x spans clear of every box in rows `a` to `b`. */
  function spans(a: number, b: number): P[] {
    let out: P[] = [[-Infinity, Infinity]];
    for (let r = a; r <= b; r++)
      for (const id of rows[r].ids) {
        const f = frames[id], lo = f.x - CLEAR, hi = f.x + f.w + CLEAR;
        out = out.flatMap(([x, y]): P[] => (hi <= x || lo >= y ? [[x, y]] : ([[x, lo], [hi, y]] as P[]).filter(([s, t]) => t > s)));
      }
    return out;
  }
  const gaps = (sp: P[]): P[] =>
    sp.map(([x, y]) => (x === -Infinity ? [y - OUTSIDE, 2 * OUTSIDE] : y === Infinity ? [x + OUTSIDE, 2 * OUTSIDE] : [(x + y) / 2, y - x]));
  const sleeves: Wiring["sleeves"] = [];
  function lay(runs: Run[], mid: number, room: number, vertical: boolean) {
    const at = new Map(runs.map((r) => [r, r.span()] as const));
    const ends: number[] = [], lane = new Map<Run, number>();
    for (const r of [...runs].sort((x, y) => at.get(x)![0] - at.get(y)![0] || at.get(x)![1] - at.get(y)![1] || x.id.localeCompare(y.id))) {
      const [lo, hi] = at.get(r)!, k = ends.findIndex((e) => e + 2 * pitch < lo);
      lane.set(r, k < 0 ? ends.push(hi) - 1 : k);
      if (k >= 0) ends[k] = hi;
    }
    const n = ends.length, step = n > 1 ? Math.max(1, Math.min(pitch, (room - 2 * CLEAR) / (n - 1))) : 0;
    for (const r of runs) r.set(mid + (lane.get(r)! - (n - 1) / 2) * step);
    if (mode === "ribbon" && n > 1) {
      const lo = Math.min(...runs.map((r) => at.get(r)![0])), hi = Math.max(...runs.map((r) => at.get(r)![1]));
      sleeves.push({ d: vertical ? `M${r2(mid)},${r2(lo)} L${r2(mid)},${r2(hi)}` : `M${r2(lo)},${r2(mid)} L${r2(hi)},${r2(mid)}`, w: (n - 1) * step + 2 * pitch });
    }
  }
  return { rowOf, channel, spans, gaps, lay, sleeves };
}

const span = (x: number, y: number): P => [Math.min(x, y), Math.max(x, y)];
const orient = (p: P[], flip: boolean) => (flip ? [...p].reverse() : p);
const wire = (p: P[], bend: number): Wire => ({ d: chamfer(p, bend), mid: halfway(p), len: r2(length(p)), ends: [p[0], p[p.length - 1]] });

function routed(arrows: MapArrow[], frames: Record<string, Frame>, kinds: Kinds, shape: ShapeOf, g: Grid): Map<string, Wire> {
  const { pitch, bend, load } = LOOK.routed;
  const live = arrows.filter((a) => a.from !== a.to && frames[a.from] && frames[a.to]).sort((x, y) => x.id.localeCompare(y.id));
  const row = (id: string) => g.rowOf.get(id)!;
  const byX = Object.entries(frames).sort(([, a], [, b]) => a.x - b.x), widest = Math.max(0, ...byX.map(([, f]) => f.w));
  function within(x1: number, x2: number, skip: string[]): Frame[] {
    let lo = 0, hi = byX.length;
    while (lo < hi) {
      const m = (lo + hi) >> 1;
      if (byX[m][1].x <= x1 - widest) lo = m + 1;
      else hi = m;
    }
    const out: Frame[] = [];
    for (let i = lo; i < byX.length && byX[i][1].x < x2; i++) {
      const [id, f] = byX[i];
      if (f.x + f.w > x1 && id !== skip[0] && id !== skip[1]) out.push(f);
    }
    return out;
  }
  const used = new Map<string, number>(), key = (x: number) => `v${Math.round(x)}`;
  const blocked = (near: Frame[], x0: number, x1: number, y0: number, y1: number) =>
    near.some((f) => f.x < Math.max(x0, x1) && f.x + f.w > Math.min(x0, x1) && f.y < Math.max(y0, y1) && f.y + f.h > Math.min(y0, y1));
  const facing = (a: MapArrow, e: End) =>
    e.sides[0] === "l" ? { skip: [a.to, a.from], p: e.to, q: e.from, flip: true } : { skip: [a.from, a.to], p: e.from, q: e.to, flip: false };
  function sideways(a: MapArrow, e: End): { y: number } | { mid: number; room: number } | null {
    const { skip, p: [x1, y1], q: [x2, y2] } = facing(a, e), y = (y1 + y2) / 2;
    if (x2 <= x1) return null;
    const near = within(x1, x2, skip);
    if (Math.abs(y1 - y2) <= pitch / 2 && !blocked(near, x1, x2, y, y)) return { y };
    let [from, reach] = [x1, x2];
    for (const f of near) {
      if (f.y < y1 && f.y + f.h > y1) reach = Math.min(reach, f.x);
      if (f.y < y2 && f.y + f.h > y2) from = Math.max(from, f.x + f.w);
    }
    if (from >= reach) return null;
    const [lo, hi] = span(y1, y2);
    let free: P[] = [[from, reach]];
    for (const f of near) {
      if (f.y >= hi || f.y + f.h <= lo || f.x >= reach || f.x + f.w <= from) continue;
      free = free.flatMap(([s, t]): P[] => (f.x + f.w <= s || f.x >= t ? [[s, t]] : ([[s, f.x], [f.x + f.w, t]] as P[]).filter(([u, v]) => v - u >= 2 * pitch)));
    }
    let best: { mid: number; room: number } | null = null, score = Infinity;
    for (const [s, t] of free) {
      const mid = (s + t) / 2, c = Math.abs(mid - (x1 + x2) / 2) + load * (used.get(key(mid)) ?? 0);
      if (c < score) [best, score] = [{ mid, room: t - s }, c];
    }
    return best;
  }

  const crosses = (pts: P[], skip: string[]) =>
    pts.slice(1).some((q, i) => blocked(within(Math.min(pts[i][0], q[0]), Math.max(pts[i][0], q[0]), skip), pts[i][0], q[0], pts[i][1], q[1]));
  // two stacked boxes a third chains into one row: a bend between them, if no box is in the way
  function stacked(a: MapArrow, e: End): P[] | null {
    const [u, l] = e.sides[0] === "t" ? [e.to, e.from] : [e.from, e.to];
    if (l[1] <= u[1]) return null;
    const x = (u[0] + l[0]) / 2, y = (u[1] + l[1]) / 2;
    const pts: P[] = Math.abs(u[0] - l[0]) <= pitch / 2 ? [[x, u[1]], [x, l[1]]] : [u, [u[0], y], [l[0], y], l];
    return crosses(pts, [a.from, a.to]) ? null : pts;
  }

  const under = (c: number) => (g.channel(c).top + g.channel(c).bottom) / 2;
  const level = (a: MapArrow, e: End) => (e.sides[0] === "b" ? row(a.from) : row(a.from) - 1);
  const round = (a: MapArrow, e: End): P[] => {
    const y = under(level(a, e));
    return [e.from, [e.from[0], y], [e.to[0], y], e.to];
  };
  const chosen: Record<string, [Side, Side]> = {}, tried = new Set<string>();
  let ends = pins(live, frames, kinds);
  for (;;) {
    let more = false;
    for (const a of live) {
      const e = ends.get(a.id)!, [rf, rt] = [row(a.from), row(a.to)];
      if (chosen[a.id] || shape(a) !== "circuit" || (across(e.sides[0]) ? sideways(a, e) : rf !== rt || stacked(a, e))) continue;
      chosen[a.id] = rf < rt ? ["b", "t"] : rf > rt ? ["t", "b"] : ["b", "b"];
      more = true;
    }
    if (!more)
      for (const a of live) {
        const c = chosen[a.id], skip = [a.from, a.to];
        if (c?.[0] !== "b" || c[1] !== "b" || tried.has(a.id)) continue;
        tried.add(a.id);
        if (!crosses(round(a, ends.get(a.id)!), skip)) continue;
        const alt = pins(live, frames, kinds, { ...chosen, [a.id]: ["t", "t"] });
        if (crosses(round(a, alt.get(a.id)!), skip)) continue;
        chosen[a.id] = ["t", "t"];
        more = true;
      }
    if (!more) break;
    ends = pins(live, frames, kinds, chosen);
  }

  const lanes = new Map<string, { mid: number; room: number; vertical: boolean; runs: Run[] }>();
  const slot = (k: string, mid: number, room: number, vertical: boolean, id: string, sp: () => P) => {
    const s = { v: mid }, r: Run = { id, span: sp, set: (v) => (s.v = v) }, l = lanes.get(k);
    if (l) {
      l.runs.push(r);
      l.room = Math.min(l.room, room);
    } else lanes.set(k, { mid, room, vertical, runs: [r] });
    if (vertical) used.set(k, (used.get(k) ?? 0) + 1);
    return s;
  };
  const along = (c: number, id: string, sp: () => P) => {
    const ch = g.channel(c);
    return slot(`c${c}`, (ch.top + ch.bottom) / 2, ch.bottom - ch.top, false, id, sp);
  };
  const down = (mid: number, room: number, id: string, sp: () => P) => slot(key(mid), mid, room, true, id, sp);

  const draws: [string, () => P[]][] = [];
  const drawn = new Map<string, Wire>(), walls = Object.values(frames);
  for (const a of live) {
    const e = ends.get(a.id)!, [sa, sz] = e.sides, form = shape(a);
    if (form !== "circuit") drawn.set(a.id, path(form, e, walls));
    else if (across(sa)) {
      const { p, q, flip } = facing(a, e), plan = sideways(a, e)!;
      if ("y" in plan) draws.push([a.id, () => orient([[p[0], plan.y], [q[0], plan.y]], flip)]);
      else {
        const s = down(plan.mid, plan.room, a.id, () => span(p[1], q[1]));
        draws.push([a.id, () => orient([p, [s.v, p[1]], [s.v, q[1]], q], flip)]);
      }
    } else if (sa === sz) {
      const s = along(level(a, e), a.id, () => span(e.from[0], e.to[0]));
      draws.push([a.id, () => [e.from, [e.from[0], s.v], [e.to[0], s.v], e.to]]);
    } else if (row(a.from) === row(a.to)) {
      const pts = stacked(a, e)!, flip = sa === "t";
      if (pts.length === 2) draws.push([a.id, () => orient(pts, flip)]);
      else {
        const [u, l] = [pts[0], pts[3]];
        const s = slot(`m${Math.round(pts[1][1])}`, pts[1][1], l[1] - u[1], false, a.id, () => span(u[0], l[0]));
        draws.push([a.id, () => orient([u, [u[0], s.v], [l[0], s.v], l], flip)]);
      }
    } else {
      const flip = sa === "t";
      const [u, l] = flip ? [e.to, e.from] : [e.from, e.to];
      const [ra, rb] = flip ? [row(a.to), row(a.from)] : [row(a.from), row(a.to)];
      const free = rb - ra > 1 ? g.spans(ra + 1, rb - 1) : [[-Infinity, Infinity] as P];
      const clear = (x: number) => free.some(([lo, hi]) => x > lo && x < hi);
      const jog = (y: number): P[] => [u, [u[0], y], [l[0], y], l];
      // each way it can go, tried in turn until one crosses no box: [as drawn on the channels' middles, laid]
      const ways: [P[], () => () => P[]][] = [];
      const x = (u[0] + l[0]) / 2;
      if (Math.abs(u[0] - l[0]) <= pitch / 2 && clear(x)) {
        const pts: P[] = [[x, u[1]], [x, l[1]]];
        ways.push([pts, () => () => orient(pts, flip)]);
      }
      const inChannel = (c: number) =>
        ways.push([jog(under(c)), () => {
          const s = along(c, a.id, () => span(u[0], l[0]));
          return () => orient(jog(s.v), flip);
        }]);
      if (clear(l[0])) inChannel(ra);
      if (clear(u[0])) inChannel(rb - 1);
      if (rb - ra > 1) {
        const cost = ([c]: P) => Math.abs(c - u[0]) + Math.abs(c - l[0]) + load * (used.get(key(c)) ?? 0);
        const [gx, gw] = g.gaps(free).reduce((m, o) => (cost(o) < cost(m) ? o : m));
        const [y1, y2] = [under(ra), under(rb - 1)];
        ways.push([[u, [u[0], y1], [gx, y1], [gx, y2], [l[0], y2], l], () => {
          const s1 = along(ra, a.id, () => span(u[0], gx)), s2 = along(rb - 1, a.id, () => span(gx, l[0]));
          const sg = down(gx, gw, a.id, () => [s1.v, s2.v]);
          return () => orient([u, [u[0], s1.v], [sg.v, s1.v], [sg.v, s2.v], [l[0], s2.v], l], flip);
        }]);
      }
      const ym = (u[1] + l[1]) / 2;
      if (l[1] > u[1])
        ways.push([jog(ym), () => {
          const s = slot(`m${Math.round(ym)}`, ym, l[1] - u[1], false, a.id, () => span(u[0], l[0]));
          return () => orient(jog(s.v), flip);
        }]);
      draws.push([a.id, (ways.find(([pts]) => !crosses(pts, [a.from, a.to])) ?? ways[0])[1]()]);
    }
  }
  for (const t of lanes.values()) if (!t.vertical) g.lay(t.runs, t.mid, t.room, false);
  for (const t of lanes.values()) if (t.vertical) g.lay(t.runs, t.mid, t.room, true);
  for (const [id, draw] of draws) drawn.set(id, wire(straight(draw()), bend));
  return new Map(live.filter((a) => drawn.has(a.id)).map((a) => [a.id, drawn.get(a.id)!]));
}

function ribbon(arrows: MapArrow[], frames: Record<string, Frame>, g: Grid): Map<string, Wire> {
  const { bend, load: weight } = LOOK.ribbon;
  const { rowOf, channel, spans, gaps, lay } = g;
  const cx = (id: string) => frames[id].x + frames[id].w / 2;

  type W = { a: MapArrow; top: string; bot: string; flip: boolean; same: boolean; gap?: [number, number]; xa: number; xz: number; y1: number; y2: number; xg: number };
  const ws: W[] = [];
  const used = new Map<number, number>();
  for (const a of [...arrows].sort((x, y) => x.id.localeCompare(y.id))) {
    if (a.from === a.to || !frames[a.from] || !frames[a.to]) continue;
    const [rf, rt] = [rowOf.get(a.from)!, rowOf.get(a.to)!];
    const flip = rt < rf || (rt === rf && cx(a.to) < cx(a.from));
    const [top, bot] = flip ? [a.to, a.from] : [a.from, a.to];
    const [ra, rb] = [rowOf.get(top)!, rowOf.get(bot)!];
    let gap: [number, number] | undefined;
    if (rb - ra > 1) {
      const cost = ([c]: [number, number]) => Math.abs(c - cx(top)) + Math.abs(c - cx(bot)) + weight * (used.get(c) ?? 0);
      gap = gaps(spans(ra + 1, rb - 1)).reduce((m, o) => (cost(o) < cost(m) ? o : m));
      used.set(gap[0], (used.get(gap[0]) ?? 0) + 1);
    }
    ws.push({ a, top, bot, flip, same: ra === rb, gap, xa: 0, xz: 0, y1: 0, y2: 0, xg: 0 });
  }
  // each side's wires spread along it, in the order of where they head
  const sides = new Map<string, { head: number; w: W; end: "xa" | "xz" }[]>();
  const side = (k: string, e: { head: number; w: W; end: "xa" | "xz" }) => sides.set(k, [...(sides.get(k) ?? []), e]);
  for (const w of ws) {
    side(`${w.top}\u0000b`, { head: w.gap?.[0] ?? cx(w.bot), w, end: "xa" });
    side(`${w.bot}\u0000${w.same ? "b" : "t"}`, { head: w.gap?.[0] ?? cx(w.top), w, end: "xz" });
  }
  for (const [k, list] of sides) {
    const f = frames[k.split("\u0000")[0]];
    list.sort((x, y) => x.head - y.head || x.w.a.id.localeCompare(y.w.a.id));
    list.forEach((e, i) => (e.w[e.end] = f.x + f.w * ((1 - SPREAD) / 2 + (SPREAD * (i + 0.5)) / list.length)));
  }
  // tracks: the runs along one channel, then the drops down one gap
  const byChannel = new Map<number, Run[]>();
  const run = (c: number, r: Run) => byChannel.set(c, [...(byChannel.get(c) ?? []), r]);
  for (const w of ws) {
    const [ra, rb] = [rowOf.get(w.top)!, rowOf.get(w.bot)!], id = w.a.id;
    if (!w.gap) run(ra, { id, span: () => span(w.xa, w.xz), set: (v) => (w.y1 = v) });
    else {
      run(ra, { id, span: () => span(w.xa, w.gap![0]), set: (v) => (w.y1 = v) });
      run(rb - 1, { id, span: () => span(w.gap![0], w.xz), set: (v) => (w.y2 = v) });
    }
  }
  for (const [c, runs] of byChannel) {
    const ch = channel(c);
    lay(runs, (ch.top + ch.bottom) / 2, ch.bottom - ch.top, false);
  }
  const byGap = new Map<number, W[]>();
  for (const w of ws) if (w.gap) byGap.set(w.gap[0], [...(byGap.get(w.gap[0]) ?? []), w]);
  for (const [c, list] of byGap) lay(list.map((w) => ({ id: w.a.id, span: () => [w.y1, w.y2], set: (v) => (w.xg = v) })), c, list[0].gap![1], true);

  const out = new Map<string, Wire>();
  for (const w of ws) {
    const [ft, fb] = [frames[w.top], frames[w.bot]];
    const ya = ft.y + ft.h, yz = w.same ? fb.y + fb.h : fb.y;
    const pts: P[] = w.gap
      ? [[w.xa, ya], [w.xa, w.y1], [w.xg, w.y1], [w.xg, w.y2], [w.xz, w.y2], [w.xz, yz]]
      : [[w.xa, ya], [w.xa, w.y1], [w.xz, w.y1], [w.xz, yz]];
    const p = straight(pts);
    out.set(w.a.id, wire(orient(p, w.flip), bend));
  }
  return out;
}

/** The polyline with each corner cut at 45°, `r` back along both legs (at most half of either). */
export function chamfer(p: P[], r: number): string {
  let d = `M${pt(p[0])}`;
  for (let i = 1; i < p.length - 1; i++) {
    const [a, c, b] = [p[i - 1], p[i], p[i + 1]];
    const la = Math.hypot(c[0] - a[0], c[1] - a[1]), lb = Math.hypot(b[0] - c[0], b[1] - c[1]);
    const k = Math.min(r, la / 2, lb / 2);
    d += ` L${pt([c[0] - ((c[0] - a[0]) / la) * k, c[1] - ((c[1] - a[1]) / la) * k])}`;
    d += ` L${pt([c[0] + ((b[0] - c[0]) / lb) * k, c[1] + ((b[1] - c[1]) / lb) * k])}`;
  }
  return `${d} L${pt(p[p.length - 1])}`;
}
