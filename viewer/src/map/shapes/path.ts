import type { Frame } from "../types";

export type Side = "l" | "r" | "t" | "b";
export type P = [number, number];
export type End = { from: P; to: P; sides: [Side, Side] };
export type Path = { d: string; mid: P; len: number; head: [P, P]; ends: [P, P]; out: [P, P]; pts?: P[] };
export type Band = { y: number; h: number };
export type Anchor = { from?: Band; to?: Band };

export const SPREAD = 0.6;
export const BEND = 8;
const REACH = 0.4;
const HEAD = 10;
const SAMPLES = 24;
const CLEAR = 8;
export const OUT = 24;

const NORMAL: Record<Side, P> = { l: [-1, 0], r: [1, 0], t: [0, -1], b: [0, 1] };
export const across = (s: Side) => s === "l" || s === "r";

export function sides(a: Frame, b: Frame): [Side, Side] {
  const ox = Math.min(a.x + a.w, b.x + b.w) - Math.max(a.x, b.x);
  const oy = Math.min(a.y + a.h, b.y + b.h) - Math.max(a.y, b.y);
  const [dx, dy] = [b.x + b.w / 2 - (a.x + a.w / 2), b.y + b.h / 2 - (a.y + a.h / 2)];
  if (oy > ox) return dx < 0 ? ["l", "r"] : ["r", "l"];
  return dy < 0 ? ["t", "b"] : ["b", "t"];
}

// a person is drawn as a head over its name, centred in the frame: about 120 x 80 at most
export function figure(f: Frame, kind?: string): Frame {
  if (kind !== "person") return f;
  const w = Math.max(0, Math.min(f.w - 20, 120)), h = Math.max(0, Math.min(f.h - 20, 80));
  return { x: f.x + (f.w - w) / 2, y: f.y + (f.h - h) / 2, w, h };
}

function oneSide(a: Frame, b: Frame, bounds?: [number, number]): [Side, Side] {
  const right = bounds ? bounds[0] - Math.max(a.x + a.w, b.x + b.w) : Infinity, left = bounds ? Math.min(a.x, b.x) : 0;
  const s: Side = right >= 2 * OUT || (left < 2 * OUT && right >= left) ? "r" : "l";
  return [s, s];
}

const apart = (a: Frame, b: Frame): [Side, Side] | undefined => (b.x >= a.x + a.w ? ["r", "l"] : b.x + b.w <= a.x ? ["l", "r"] : undefined);

export function pins(
  arrows: { id: string; from: string; to: string }[],
  frames: Record<string, Frame>,
  kinds: Record<string, string | undefined> = {},
  chosen: Record<string, [Side, Side]> = {},
  anchors: Record<string, Anchor> = {},
  bounds?: [number, number],
): Map<string, End> {
  const rect = (id: string) => figure(frames[id], kinds[id]);
  const centre = (f: Frame): P => [f.x + f.w / 2, f.y + f.h / 2];
  type Pin = { id: string; head: number; set: (p: P) => void };
  const onSide = new Map<string, { box: string; side: Side; list: Pin[] }>();
  const onRow = new Map<string, { box: string; side: Side; band: Band; list: Pin[] }>();
  const out = new Map<string, End>();
  for (const a of [...arrows].sort((x, y) => x.id.localeCompare(y.id))) {
    if (a.from === a.to || !frames[a.from] || !frames[a.to]) continue;
    const [fa, fb] = [rect(a.from), rect(a.to)], held = anchors[a.id];
    const end: End = { from: [0, 0], to: [0, 0], sides: chosen[a.id] ?? (held && (apart(fa, fb) ?? (held.from && held.to ? oneSide(fa, fb, bounds) : undefined))) ?? sides(fa, fb) };
    out.set(a.id, end);
    for (const [box, side, far, key] of [[a.from, end.sides[0], fb, "from"], [a.to, end.sides[1], fa, "to"]] as const) {
      const c = centre(far), band = held?.[key];
      const pin = { id: a.id, head: across(side) ? c[1] : c[0], set: (p: P) => (end[key] = p) };
      if (band && across(side)) {
        const k = `${box}\u0000${side}\u0000${Math.round(band.y)}`;
        if (!onRow.has(k)) onRow.set(k, { box, side, band, list: [] });
        onRow.get(k)!.list.push(pin);
        continue;
      }
      const k = `${box}\u0000${side}`;
      if (!onSide.has(k)) onSide.set(k, { box, side, list: [] });
      onSide.get(k)!.list.push(pin);
    }
  }
  for (const { box, side, band, list } of onRow.values()) {
    const f = rect(box);
    list.sort((x, y) => x.head - y.head || x.id.localeCompare(y.id));
    list.forEach((p, i) => p.set([side === "l" ? f.x : f.x + f.w, f.y + band.y + band.h * SPREAD * ((i + 0.5) / list.length - 0.5)]));
  }
  for (const { box, side, list } of onSide.values()) {
    const f = rect(box);
    list.sort((x, y) => x.head - y.head || x.id.localeCompare(y.id));
    list.forEach((p, i) => {
      const t = (1 - SPREAD) / 2 + (SPREAD * (i + 0.5)) / list.length;
      p.set(
        side === "l" ? [f.x, f.y + f.h * t] : side === "r" ? [f.x + f.w, f.y + f.h * t]
        : side === "t" ? [f.x + f.w * t, f.y] : [f.x + f.w * t, f.y + f.h],
      );
    });
  }
  return out;
}

export function path(shape: "elbow" | "curved" | "straight", end: End, avoid: Frame[] = []): Path {
  if (shape === "straight") return line([end.from, end.to], end);
  if (shape === "elbow") return elbow(end);
  return curved(end, avoid) ?? elbow(end);
}

export const rounded = (p: P[]) => line(p, { from: p[0], to: p[p.length - 1], sides: ["r", "l"] }).d;

function elbow(end: End): Path {
  const [[x1, y1], [x2, y2]] = [end.from, end.to];
  const [h1, h2] = [across(end.sides[0]), across(end.sides[1])];
  const [s1, s2] = end.sides, out = s1 === "r" ? Math.max(x1, x2) + OUT : Math.min(x1, x2) - OUT;
  const pts: P[] =
    h1 && s1 === s2 ? [[x1, y1], [out, y1], [out, y2], [x2, y2]]
    : h1 && h2 ? [[x1, y1], [(x1 + x2) / 2, y1], [(x1 + x2) / 2, y2], [x2, y2]]
    : !h1 && !h2 ? [[x1, y1], [x1, (y1 + y2) / 2], [x2, (y1 + y2) / 2], [x2, y2]]
    : h1 ? [[x1, y1], [x2, y1], [x2, y2]]
    : [[x1, y1], [x1, y2], [x2, y2]];
  return line(straight(pts), end);
}

function line(p: P[], end: End): Path {
  let d = `M${pt(p[0])}`;
  for (let i = 1; i < p.length - 1; i++) {
    const [a, c, b] = [p[i - 1], p[i], p[i + 1]];
    const la = dist(a, c), lb = dist(c, b), k = Math.min(BEND, la / 2, lb / 2);
    d += ` L${pt(towards(c, a, k / la))} Q${pt(c)} ${pt(towards(c, b, k / lb))}`;
  }
  d += ` L${pt(p[p.length - 1])}`;
  const tip = p[p.length - 1];
  return { d, mid: halfway(p), len: r2(length(p)), head: head(tip, p.length > 1 ? sub(tip, p[p.length - 2]) : [0, 0], end), ends: [end.from, end.to], out: leaving(p, end), pts: p };
}

export function curved(end: End, avoid: Frame[]): Path | undefined {
  const { from, to, sides: [sa, sz] } = end;
  const reach = Math.max(sa === sz ? Math.min(dist(from, to) * REACH, 2 * OUT) : dist(from, to) * REACH, 1);
  const base: [P, P] = [add(from, NORMAL[sa], reach), add(to, NORMAL[sz], reach)];
  const walls = avoid.filter((f) => !inside(from, f, 1) && !inside(to, f, 1));
  const bezier = ([c1, c2]: [P, P]) => Array.from({ length: SAMPLES + 1 }, (_, i) => cubic(from, c1, c2, to, i / SAMPLES));
  const hit = (s: P[]) => walls.find((f) => s.slice(1, -1).some((q) => inside(q, f, CLEAR)));
  let ctl = base, s = bezier(ctl), wall = hit(s);
  if (wall) {
    const chord = sub(to, from), n = Math.hypot(...chord) || 1;
    const perp: P = [-chord[1] / n, chord[0] / n];
    const c = [wall.x + wall.w / 2 - from[0], wall.y + wall.h / 2 - from[1]];
    const away = c[0] * perp[0] + c[1] * perp[1] > 0 ? -1 : 1;
    const d = dist(from, to), push = Math.min(2 * d, Math.max(d / 2, (wall.w + wall.h) / 2));
    tries: for (const k of [1, 2])
      for (const sign of [away, -away]) {
        const o = sign * k * push;
        ctl = [add(base[0], perp, o), add(base[1], perp, o)];
        s = bezier(ctl);
        if (!(wall = hit(s))) break tries;
      }
    if (wall) return undefined;
  }
  const [c1, c2] = ctl;
  const t = [sub(to, c2), sub(to, c1), sub(to, from)].find((v) => v[0] || v[1]) ?? ([0, 0] as P);
  return { d: `M${pt(from)} C${pt(c1)} ${pt(c2)} ${pt(to)}`, mid: halfway(s), len: r2(length(s)), head: head(to, t, end), ends: [from, to], out: leaving(s, end), pts: s };
}

export function leaving(p: P[], end?: End): [P, P] {
  const away = (at: P, rest: P[], side?: Side): P => {
    const q = rest.find((r) => r[0] !== at[0] || r[1] !== at[1]);
    return q ? sub(q, at) : side ? NORMAL[side] : [1, 0];
  };
  return [away(p[0], p.slice(1), end?.sides[0]), away(p[p.length - 1], [...p].reverse().slice(1), end?.sides[1])];
}

function head(tip: P, dir: P, end: End): [P, P] {
  const n = Math.hypot(...dir);
  const u: P = n > 0 ? [dir[0] / n, dir[1] / n] : [-NORMAL[end.sides[1]][0], -NORMAL[end.sides[1]][1]];
  return [[r2(tip[0] - u[0] * HEAD), r2(tip[1] - u[1] * HEAD)], tip];
}

export const r2 = (n: number) => Math.round(n * 100) / 100;
export const pt = (p: P) => `${r2(p[0])},${r2(p[1])}`;
const sub = (a: P, b: P): P => [a[0] - b[0], a[1] - b[1]];
const add = (a: P, u: P, k: number): P => [a[0] + u[0] * k, a[1] + u[1] * k];
const dist = (a: P, b: P) => Math.hypot(b[0] - a[0], b[1] - a[1]);
const towards = (a: P, b: P, t: number): P => [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
const inside = (p: P, f: Frame, m: number) => p[0] > f.x - m && p[0] < f.x + f.w + m && p[1] > f.y - m && p[1] < f.y + f.h + m;
export const length = (p: P[]) => p.slice(1).reduce((s, q, i) => s + dist(p[i], q), 0);

function cubic(a: P, b: P, c: P, d: P, t: number): P {
  const u = 1 - t;
  return [
    u * u * u * a[0] + 3 * u * u * t * b[0] + 3 * u * t * t * c[0] + t * t * t * d[0],
    u * u * u * a[1] + 3 * u * u * t * b[1] + 3 * u * t * t * c[1] + t * t * t * d[1],
  ];
}

export function straight(pts: P[]): P[] {
  const out: P[] = [];
  for (const p of pts) {
    const b = out[out.length - 1], a = out[out.length - 2];
    if (b && b[0] === p[0] && b[1] === p[1]) continue;
    if (a && ((a[0] === b[0] && b[0] === p[0]) || (a[1] === b[1] && b[1] === p[1]))) out.pop();
    out.push(p);
  }
  return out;
}

export function halfway(p: P[]): P {
  let left = length(p) / 2;
  for (let i = 0; i < p.length - 1; i++) {
    const l = dist(p[i], p[i + 1]);
    if (left <= l && l > 0) return [r2(p[i][0] + ((p[i + 1][0] - p[i][0]) * left) / l), r2(p[i][1] + ((p[i + 1][1] - p[i][1]) * left) / l)];
    left -= l;
  }
  return [r2(p[0][0]), r2(p[0][1])];
}
