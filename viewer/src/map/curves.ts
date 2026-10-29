import type { Frame, MapArrow } from "./types";

/** An arrow drawn as one cubic curve: its SVG path and the point halfway along it. */
export type Curve = { d: string; mid: [number, number] };

type Side = "left" | "right" | "top" | "bottom";
const NORMAL: Record<Side, [number, number]> = { left: [-1, 0], right: [1, 0], top: [0, -1], bottom: [0, 1] };
const OPPOSITE: Record<Side, Side> = { left: "right", right: "left", top: "bottom", bottom: "top" };

const centre = (f: Frame): [number, number] => [f.x + f.w / 2, f.y + f.h / 2];

/** The side of `a` facing `b`: across the wider of the gaps between them. */
function facing(a: Frame, b: Frame): Side {
  const [ax, ay] = centre(a), [bx, by] = centre(b);
  const gx = Math.abs(bx - ax) - (a.w + b.w) / 2, gy = Math.abs(by - ay) - (a.h + b.h) / 2;
  if (gx >= gy) return bx >= ax ? "right" : "left";
  return by >= ay ? "bottom" : "top";
}

/** The point at `t` (0 to 1) along side `s` of `f`. */
function along(f: Frame, s: Side, t: number): [number, number] {
  switch (s) {
    case "left": return [f.x, f.y + f.h * t];
    case "right": return [f.x + f.w, f.y + f.h * t];
    case "top": return [f.x + f.w * t, f.y];
    case "bottom": return [f.x + f.w * t, f.y + f.h];
  }
}

/**
 * Each arrow between two of `frames` (siblings, in their parent's inner units) as a curve leaving
 * the side of its start box facing its end box and entering the opposite side of the end box.
 * Arrows sharing a side spread along its middle, ordered by where their other end sits, so they
 * neither stack nor cross at the box. An arrow from a box to itself, or to a box with no frame, has none.
 */
export function curves(arrows: MapArrow[], frames: Record<string, Frame>): Map<string, Curve> {
  type End = { arrow: string; box: string; side: Side; other: [number, number]; t: number };
  const ends = new Map<string, End[]>();
  const pairs: [MapArrow, End, End][] = [];
  for (const a of arrows) {
    const f = frames[a.from], g = frames[a.to];
    if (!f || !g || a.from === a.to) continue;
    const s = facing(f, g);
    const from: End = { arrow: a.id, box: a.from, side: s, other: centre(g), t: 0.5 };
    const to: End = { arrow: a.id, box: a.to, side: OPPOSITE[s], other: centre(f), t: 0.5 };
    for (const e of [from, to]) {
      const k = `${e.box}\u0000${e.side}`;
      ends.set(k, [...(ends.get(k) ?? []), e]);
    }
    pairs.push([a, from, to]);
  }
  for (const list of ends.values()) {
    const vertical = list[0].side === "left" || list[0].side === "right";
    list.sort((x, y) => (vertical ? x.other[1] - y.other[1] : x.other[0] - y.other[0]) || x.arrow.localeCompare(y.arrow));
    list.forEach((e, i) => (e.t = 0.2 + (0.6 * (i + 1)) / (list.length + 1)));
  }
  const out = new Map<string, Curve>();
  for (const [a, s, e] of pairs) {
    const p0 = along(frames[a.from], s.side, s.t), p3 = along(frames[a.to], e.side, e.t);
    const [n0, n3] = [NORMAL[s.side], NORMAL[e.side]];
    const reach = Math.max(24, 0.45 * Math.hypot(p3[0] - p0[0], p3[1] - p0[1]));
    const c1: [number, number] = [p0[0] + n0[0] * reach, p0[1] + n0[1] * reach];
    const c2: [number, number] = [p3[0] + n3[0] * reach, p3[1] + n3[1] * reach];
    const r = (n: number) => Math.round(n * 100) / 100;
    const pt = (p: [number, number]) => `${r(p[0])},${r(p[1])}`;
    out.set(a.id, {
      d: `M${pt(p0)} C${pt(c1)} ${pt(c2)} ${pt(p3)}`,
      mid: [(p0[0] + 3 * c1[0] + 3 * c2[0] + p3[0]) / 8, (p0[1] + 3 * c1[1] + 3 * c2[1] + p3[1]) / 8],
    });
  }
  return out;
}
