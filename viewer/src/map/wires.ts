import type { Frame, MapArrow } from "./types";

/** An arrow drawn as a wire: its SVG path and the point halfway along it. */
export type Wire = { d: string; mid: [number, number]; len: number; ends: [[number, number], [number, number]] };
/** "routed": each wire on its own track, spread over the gaps; "ribbon": wires gathered into as
 *  few trunks as they can share, packed close, a sleeve behind each trunk. */
export type Mode = "routed" | "ribbon";
export type Wiring = { wires: Map<string, Wire>; sleeves: { d: string; w: number }[] };

type P = [number, number];
const LOOK = {
  routed: { pitch: 8, bend: 8, load: 60 },
  // a negative load: a gap already used is cheaper, so wires gather
  ribbon: { pitch: 5, bend: 5, load: -200 },
};
/** How far a wire running past a box keeps from it. */
const CLEAR = 16;
/** The share of a box's side its wires spread over, around the middle. */
const SPREAD = 0.6;
/** The room taken left or right of every box, and under the last row, when a wire goes round. */
const OUTSIDE = 48;

/**
 * The wires of `arrows` between sibling boxes `frames` (in their parent's inner units), laid like a
 * circuit board: the boxes fall into rows, a wire runs along the channel between two rows on a
 * track of its own, and one that skips rows drops down a gap clear of the rows between. A wire
 * leaves the bottom of the upper box and enters the top of the lower (both bottoms in one row);
 * the wires of one side spread along it in the order they head off, and bends are cut at 45°.
 * An arrow from a box to itself, or to a box with no frame, has none.
 */
export function wires(arrows: MapArrow[], frames: Record<string, Frame>, mode: Mode): Wiring {
  const { pitch, bend, load: weight } = LOOK[mode];
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
  const channel = (c: number) => ({ top: rows[c].bottom, bottom: rows[c + 1]?.top ?? rows[c].bottom + 2 * OUTSIDE });
  /** The x spans clear of every box in rows `a` to `b`, as (centre, width). */
  function gaps(a: number, b: number): [number, number][] {
    let spans: P[] = [[-Infinity, Infinity]];
    for (let r = a; r <= b; r++)
      for (const id of rows[r].ids) {
        const f = frames[id], lo = f.x - CLEAR, hi = f.x + f.w + CLEAR;
        spans = spans.flatMap(([x, y]): P[] => (hi <= x || lo >= y ? [[x, y]] : ([[x, lo], [hi, y]] as P[]).filter(([s, t]) => t > s)));
      }
    return spans.map(([x, y]) => (x === -Infinity ? [y - OUTSIDE, 2 * OUTSIDE] : y === Infinity ? [x + OUTSIDE, 2 * OUTSIDE] : [(x + y) / 2, y - x]));
  }
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
      gap = gaps(ra + 1, rb - 1).reduce((m, g) => (cost(g) < cost(m) ? g : m));
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
  // tracks: the runs along one channel, then the drops down one gap, each on a lane of its own
  type Run = { w: W; lo: number; hi: number; at: "y1" | "y2" | "xg" };
  const sleeves: Wiring["sleeves"] = [];
  function lay(runs: Run[], mid: number, room: number, vertical: boolean) {
    const ends: number[] = [], lane = new Map<Run, number>();
    for (const r of [...runs].sort((x, y) => x.lo - y.lo || x.hi - y.hi || x.w.a.id.localeCompare(y.w.a.id))) {
      const k = ends.findIndex((e) => e + 2 * pitch < r.lo);
      lane.set(r, k < 0 ? ends.push(r.hi) - 1 : k);
      if (k >= 0) ends[k] = r.hi;
    }
    const n = ends.length, step = n > 1 ? Math.max(1, Math.min(pitch, (room - 2 * CLEAR) / (n - 1))) : 0;
    for (const r of runs) r.w[r.at] = mid + (lane.get(r)! - (n - 1) / 2) * step;
    if (mode === "ribbon" && n > 1) {
      const lo = Math.min(...runs.map((r) => r.lo)), hi = Math.max(...runs.map((r) => r.hi));
      sleeves.push({ d: vertical ? `M${r2(mid)},${r2(lo)} L${r2(mid)},${r2(hi)}` : `M${r2(lo)},${r2(mid)} L${r2(hi)},${r2(mid)}`, w: (n - 1) * step + 2 * pitch });
    }
  }
  const byChannel = new Map<number, Run[]>();
  const run = (c: number, r: Run) => byChannel.set(c, [...(byChannel.get(c) ?? []), r]);
  for (const w of ws) {
    const [ra, rb] = [rowOf.get(w.top)!, rowOf.get(w.bot)!];
    const span = (x: number, y: number) => ({ lo: Math.min(x, y), hi: Math.max(x, y) });
    if (!w.gap) run(ra, { w, ...span(w.xa, w.xz), at: "y1" });
    else {
      run(ra, { w, ...span(w.xa, w.gap[0]), at: "y1" });
      run(rb - 1, { w, ...span(w.gap[0], w.xz), at: "y2" });
    }
  }
  for (const [c, runs] of byChannel) {
    const ch = channel(c);
    lay(runs, (ch.top + ch.bottom) / 2, ch.bottom - ch.top, false);
  }
  const byGap = new Map<number, W[]>();
  for (const w of ws) if (w.gap) byGap.set(w.gap[0], [...(byGap.get(w.gap[0]) ?? []), w]);
  for (const [c, list] of byGap) lay(list.map((w) => ({ w, lo: w.y1, hi: w.y2, at: "xg" })), c, list[0].gap![1], true);

  const out = new Map<string, Wire>();
  for (const w of ws) {
    const [ft, fb] = [frames[w.top], frames[w.bot]];
    const ya = ft.y + ft.h, yz = w.same ? fb.y + fb.h : fb.y;
    const pts: P[] = w.gap
      ? [[w.xa, ya], [w.xa, w.y1], [w.xg, w.y1], [w.xg, w.y2], [w.xz, w.y2], [w.xz, yz]]
      : [[w.xa, ya], [w.xa, w.y1], [w.xz, w.y1], [w.xz, yz]];
    const p = straight(pts);
    if (w.flip) p.reverse();
    out.set(w.a.id, { d: chamfer(p, bend), mid: halfway(p), len: r2(length(p)), ends: [p[0], p[p.length - 1]] });
  }
  return { wires: out, sleeves };
}

const r2 = (n: number) => Math.round(n * 100) / 100;
const pt = (p: P) => `${r2(p[0])},${r2(p[1])}`;

/** Without repeated points or the middle points of straight runs. */
function straight(pts: P[]): P[] {
  const out: P[] = [];
  for (const p of pts) {
    if (out.length && out[out.length - 1][0] === p[0] && out[out.length - 1][1] === p[1]) continue;
    const [a, b] = [out[out.length - 2], out[out.length - 1]];
    if (a && ((a[0] === b[0] && b[0] === p[0]) || (a[1] === b[1] && b[1] === p[1]))) out.pop();
    out.push(p);
  }
  return out;
}

/** The polyline with each corner cut at 45°, `r` back along both legs (at most half of either). */
function chamfer(p: P[], r: number): string {
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

const length = (p: P[]) => p.slice(1).reduce((s, q, i) => s + Math.hypot(q[0] - p[i][0], q[1] - p[i][1]), 0);

/** The point halfway along the polyline. */
function halfway(p: P[]): [number, number] {
  const len = (i: number) => Math.hypot(p[i + 1][0] - p[i][0], p[i + 1][1] - p[i][1]);
  let left = length(p) / 2;
  for (let i = 0; i < p.length - 1; i++) {
    const l = len(i);
    if (left <= l && l > 0) return [r2(p[i][0] + ((p[i + 1][0] - p[i][0]) * left) / l), r2(p[i][1] + ((p[i + 1][1] - p[i][1]) * left) / l)];
    left -= l;
  }
  return [r2(p[0][0]), r2(p[0][1])];
}
