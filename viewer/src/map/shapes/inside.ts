import type { ArrowShape, Frame } from "../types";
import { chamfer } from "../wires";
import { BEND, SPREAD, curved, pt, rounded, straight, type P, type Side } from "./path";

export type Port = { at: P; edge: Side; child: string; shape: ArrowShape };
export type InWire = { d: string; pts: P[] };

const CLEAR = 8, PITCH = 6, TURN = 24, OVER = 400, CROSS = 160, ASIDE = 60;
const INTO: Record<Side, Side> = { t: "b", b: "t", l: "r", r: "l" };
const STEP: Record<Side, P> = { l: [-1, 0], r: [1, 0], t: [0, -1], b: [0, 1] };
const ALL: Side[] = ["t", "b", "l", "r"];

export const spreadAt = (i: number, n: number) => (1 - SPREAD) / 2 + (SPREAD * (i + 0.5)) / n;

const pinOn = (f: Frame, s: Side, t: number): P =>
  s === "l" ? [f.x, f.y + f.h * t] : s === "r" ? [f.x + f.w, f.y + f.h * t] : s === "t" ? [f.x + f.w * t, f.y] : [f.x + f.w * t, f.y + f.h];

const along = (s: Side, p: P) => (s === "l" || s === "r" ? p[1] : p[0]);

export function inside(ports: Port[], box: { w: number; h: number }, kids: Record<string, Frame>, head = 0): (InWire | null)[] {
  const pinOf = new Map<number, P>();
  const groups = new Map<string, number[]>();
  ports.forEach((p, i) => {
    if (!kids[p.child]) return;
    const k = `${p.child}\u0000${p.edge}`;
    groups.set(k, [...(groups.get(k) ?? []), i]);
  });
  for (const list of groups.values()) {
    list.sort((a, b) => along(ports[a].edge, ports[a].at) - along(ports[b].edge, ports[b].at));
    list.forEach((i, n) => pinOf.set(i, pinOn(kids[ports[i].child], ports[i].edge, spreadAt(n, list.length))));
  }
  const g = grid(ports, box, kids, pinOf, head, CLEAR);
  let tight: ReturnType<typeof grid> | undefined;
  const title: Frame = { x: 0, y: 0, w: box.w, h: head };
  const order = ports.map((_, i) => i).filter((i) => kids[ports[i].child]);
  order.sort((a, b) => far(ports[a], pinOf.get(a)!) - far(ports[b], pinOf.get(b)!) || a - b);
  const out: (InWire | null)[] = ports.map(() => null);
  for (const i of order) {
    const p = ports[i], f = kids[p.child], pin = pinOf.get(i)!, shape = p.shape;
    const walls = Object.entries(kids).filter(([id]) => id !== p.child).map(([, w]) => w);
    if (head > 0) walls.push(title);
    const way = g.route(p, f, pin) ?? (tight ??= grid(ports, box, kids, pinOf, head, CLEAR / 4)).route(p, f, pin);
    const s: P = p.edge === "t" && head > 0 ? [p.at[0], Math.max(head, Math.min(head + CLEAR, pin[1]))] : p.at;
    const leg: P[] = s === p.at ? [] : [p.at];
    const lead = leg.length ? `M${pt(p.at)} L` : "M";
    if (shape === "straight" && !walls.some((w) => cuts(s, pin, w))) out[i] = { d: `${lead}${pt(s)} L${pt(pin)}`, pts: [...leg, s, pin] };
    else if (shape === "curved") {
      const c = curved({ from: s, to: pin, sides: [INTO[p.edge], p.edge] }, walls);
      out[i] = c ? { d: lead + c.d.slice(1), pts: [...leg, ...c.pts] } : way && { d: rounded(way), pts: way };
    } else out[i] = way && { d: shape === "circuit" ? chamfer(way, BEND) : rounded(way), pts: way };
  }
  return out;
}

const far = (p: Port, q: P) => Math.abs(p.at[0] - q[0]) + Math.abs(p.at[1] - q[1]);

export function cuts(a: P, b: P, f: Frame, m = 0): boolean {
  const [x0, y0, x1, y1] = [f.x - m, f.y - m, f.x + f.w + m, f.y + f.h + m];
  let lo = 0, hi = 1;
  const d: P = [b[0] - a[0], b[1] - a[1]];
  for (const [p, q] of [[-d[0], a[0] - x0], [d[0], x1 - a[0]], [-d[1], a[1] - y0], [d[1], y1 - a[1]]]) {
    if (p === 0) {
      if (q <= 0) return false;
      continue;
    }
    const t = q / p;
    if (p < 0) lo = Math.max(lo, t);
    else hi = Math.min(hi, t);
    if (lo >= hi) return false;
  }
  return hi - lo > 1e-9;
}

function grid(ports: Port[], box: { w: number; h: number }, kids: Record<string, Frame>, pins: Map<number, P>, head: number, clear: number) {
  const frames = Object.values(kids);
  const xs = new Set<number>([clear, box.w - clear]), ys = new Set<number>([clear, box.h - clear, head + clear]);
  const entries = new Set(ports.filter((p) => p.edge === "t").map((p) => p.at[0]));
  const lanes = (set: Set<number>, edges: number[], top: number) => {
    const e = [...new Set([0, top, ...edges])].sort((a, b) => a - b);
    for (let i = 1; i < e.length; i++) {
      const [a, b] = [e[i - 1], e[i]], m = (a + b) / 2;
      for (let k = -2; k <= 2; k++) if (Math.abs(k * PITCH) < (b - a) / 2 - clear / 2) set.add(m + k * PITCH);
    }
  };
  for (const f of frames) {
    for (const s of ALL) {
      const q = pinOn(f, s, 0.5), [nx, ny] = STEP[s];
      xs.add(q[0] + nx * clear);
      ys.add(q[1] + ny * clear);
    }
  }
  lanes(xs, frames.flatMap((f) => [f.x, f.x + f.w]), box.w);
  lanes(ys, [head, ...frames.flatMap((f) => [f.y, f.y + f.h])], box.h);
  for (const [i, q] of pins) {
    const s = ports[i].edge, [nx, ny] = STEP[s];
    xs.add(q[0]).add(q[0] + nx * clear);
    ys.add(q[1]).add(q[1] + ny * clear);
    xs.add(ports[i].at[0]);
    ys.add(ports[i].at[1]);
  }
  const X = [...xs].filter((x) => x > 0 && x < box.w).sort((a, b) => a - b);
  const Y = [...ys].filter((y) => y > 0 && y < box.h).sort((a, b) => a - b);
  const W = X.length, H = Y.length, n = W * H;
  const free = new Uint8Array(n);
  for (let j = 0; j < H; j++)
    for (let i = 0; i < W; i++)
      free[j * W + i] = (Y[j] >= head || entries.has(X[i])) && frames.every((f) => !(X[i] > f.x - clear / 2 && X[i] < f.x + f.w + clear / 2 && Y[j] > f.y - clear / 2 && Y[j] < f.y + f.h + clear / 2)) ? 1 : 0;
  // moves 0-3: right, left, down, up; a line runs through every child's middle both ways, so a move between free neighbours passes none
  const step = (k: number, d: number) => {
    const i = k % W, j = (k - i) / W;
    if (d < 2 && Y[j] < head) return -1;
    const t = d === 0 ? (i + 1 < W ? k + 1 : -1) : d === 1 ? (i > 0 ? k - 1 : -1) : d === 2 ? (j + 1 < H ? k + W : -1) : j > 0 ? k - W : -1;
    return t >= 0 && free[t] ? t : -1;
  };
  const at = (k: number): P => [X[k % W], Y[Math.floor(k / W)]];
  const used = new Uint16Array(n * 2), crossed = new Uint8Array(n);
  const flat = (k: number, t: number) => Math.floor(k / W) === Math.floor(t / W);
  const edge = (k: number, t: number) => Math.min(k, t) * 2 + (flat(k, t) ? 0 : 1);
  const nodeOf = (p: P) => {
    const i = X.indexOf(p[0]), j = Y.indexOf(p[1]);
    return i < 0 || j < 0 ? -1 : j * W + i;
  };
  const dirOf: Record<Side, number> = { r: 0, l: 1, b: 2, t: 3 };

  function route(p: Port, f: Frame, pin: P): P[] | null {
    const d0 = dirOf[INTO[p.edge]], [sx, sy] = STEP[INTO[p.edge]];
    const x = sx > 0 ? X.find((v) => v > p.at[0]) : sx < 0 ? [...X].reverse().find((v) => v < p.at[0]) : p.at[0];
    const y = sy > 0 ? Y.find((v) => v > p.at[1]) : sy < 0 ? [...Y].reverse().find((v) => v < p.at[1]) : p.at[1];
    const k0 = x === undefined || y === undefined ? -1 : nodeOf([x, y]);
    const first = k0 >= 0 && free[k0] && !frames.some((w) => cuts(p.at, at(k0), w, clear / 2)) ? k0 : -1;
    if (first < 0) return null;
    const goals = new Map<number, { side: Side; pin: P }>();
    for (const s of ALL) {
      const q = s === p.edge ? pin : pinOn(f, s, 0.5), [nx, ny] = STEP[s];
      const k = nodeOf([q[0] + nx * clear, q[1] + ny * clear]);
      if (k >= 0 && free[k]) goals.set(k, { side: s, pin: q });
    }
    const cost = new Float64Array(n * 4).fill(Infinity), back = new Int32Array(n * 4).fill(-1);
    const heap: [number, number][] = [];
    const push = (c: number, s: number) => {
      heap.push([c, s]);
      for (let i = heap.length - 1; i > 0; ) {
        const up = (i - 1) >> 1;
        if (heap[up][0] <= heap[i][0]) break;
        [heap[up], heap[i]] = [heap[i], heap[up]];
        i = up;
      }
    };
    const pop = () => {
      const top = heap[0], last = heap.pop()!;
      if (heap.length) {
        heap[0] = last;
        for (let i = 0; ; ) {
          const l = 2 * i + 1, r = l + 1;
          let m = i;
          if (l < heap.length && heap[l][0] < heap[m][0]) m = l;
          if (r < heap.length && heap[r][0] < heap[m][0]) m = r;
          if (m === i) break;
          [heap[m], heap[i]] = [heap[i], heap[m]];
          i = m;
        }
      }
      return top;
    };
    const ends = [...goals.keys()].map(at);
    const guess = (k: number) => {
      const x = X[k % W], y = Y[Math.floor(k / W)];
      let m = Infinity;
      for (const q of ends) m = Math.min(m, Math.abs(q[0] - x) + Math.abs(q[1] - y));
      return m;
    };
    const s0 = first * 4 + d0;
    cost[s0] = Math.hypot(at(first)[0] - p.at[0], at(first)[1] - p.at[1]);
    push(cost[s0] + guess(first), s0);
    let best = Infinity, bestState = -1;
    while (heap.length) {
      const [f, s] = pop();
      if (f >= best) break;
      const k = s >> 2, d = s & 3, c = cost[s], g = goals.get(k);
      if (f > c + guess(k) + 1e-9) continue;
      if (g) {
        const into = dirOf[INTO[g.side]];
        const total = c + (into === d ? 0 : TURN) + (g.side === p.edge ? 0 : ASIDE);
        if (total < best) [best, bestState] = [total, s];
      }
      for (let nd = 0; nd < 4; nd++) {
        if (nd === (d ^ 1)) continue;
        const t = step(k, nd);
        if (t < 0) continue;
        const e = edge(k, t);
        const nc = c + Math.abs(X[k % W] - X[t % W]) + Math.abs(Y[Math.floor(k / W)] - Y[Math.floor(t / W)]) + (nd === d ? 0 : TURN) + used[e] * OVER + (crossed[t] & (nd < 2 ? 2 : 1) ? CROSS : 0);
        const ns = t * 4 + nd;
        if (nc < cost[ns]) {
          cost[ns] = nc;
          back[ns] = s;
          push(nc + guess(t), ns);
        }
      }
    }
    if (bestState < 0) return null;
    const nodes: number[] = [];
    for (let s = bestState; s >= 0; s = back[s]) nodes.push(s >> 2);
    nodes.reverse();
    for (let i = 1; i < nodes.length; i++) {
      const [k, t] = [nodes[i - 1], nodes[i]];
      used[edge(k, t)]++;
      crossed[k] |= flat(k, t) ? 1 : 2;
      crossed[t] |= flat(k, t) ? 1 : 2;
    }
    const g = goals.get(bestState >> 2)!;
    return straight([p.at, ...nodes.map(at), g.pin]);
  }
  return { route };
}
