// Labels on a small map: one per line (twins merged), each slid along its line off boxes and other labels.
import type { Frame, MapArrow, Mark } from "./types";
import type { P } from "./shapes/path";
import { arrowLook, type ArrowLook } from "./looks";

const ALONG = [0.5, 0.4, 0.6, 0.3, 0.7, 0.22, 0.78];

/** The label of a line and of the arrows drawn on it: their own labels, a ⚠ for a risk, ×n for more than one. */
export function pairLook(members: MapArrow[], marks: Mark[], on: Set<string>): ArrowLook {
  const looks = members.map((a) => arrowLook(a, marks, on));
  const loud = looks.find((l) => l.tone !== "neutral");
  const names = [...new Set(members.map((a) => a.label).filter((l): l is string => !!l))];
  const own = (loud ?? looks[0]).label ?? "";
  const text = names.join(" · ") || (loud ? own.replace(/^risk:\s*/i, "") : own);
  const label = `${loud ? "⚠ " : ""}${text}${members.length > 1 ? ` ×${members.length}` : ""}`.trim();
  return { ...(loud ?? looks[0]), label: label || null };
}

export type Pill = { id: string; pts: P[]; w: number; h: number };

function at(pts: P[], t: number): P {
  const segs = pts.slice(1).map((q, i) => Math.hypot(q[0] - pts[i][0], q[1] - pts[i][1]));
  let left = segs.reduce((s, l) => s + l, 0) * t;
  for (let i = 0; i < segs.length; i++) {
    if (left <= segs[i] && segs[i] > 0) {
      const k = left / segs[i];
      return [pts[i][0] + (pts[i + 1][0] - pts[i][0]) * k, pts[i][1] + (pts[i + 1][1] - pts[i][1]) * k];
    }
    left -= segs[i];
  }
  return pts[pts.length - 1];
}

const overlap = (a: Frame, b: Frame) =>
  Math.max(0, Math.min(a.x + a.w, b.x + b.w) - Math.max(a.x, b.x)) * Math.max(0, Math.min(a.y + a.h, b.y + b.h) - Math.max(a.y, b.y));

/** Where each pill's centre goes: the first spot along its line touching no box and no pill placed before, else the least covered. */
export function spots(pills: Pill[], boxes: Frame[]): Map<string, P> {
  const out = new Map<string, P>(), taken: Frame[] = [];
  for (const p of [...pills].sort((a, b) => a.id.localeCompare(b.id))) {
    if (p.pts.length < 2) continue;
    let best: { c: P; r: Frame; cost: number } | null = null;
    for (const t of ALONG) {
      const c = at(p.pts, t), r = { x: c[0] - p.w / 2, y: c[1] - p.h / 2, w: p.w, h: p.h };
      const cost = [...boxes, ...taken].reduce((s, f) => s + overlap(r, f), 0);
      if (!best || cost < best.cost) best = { c, r, cost };
      if (cost === 0) break;
    }
    out.set(p.id, best!.c);
    taken.push(best!.r);
  }
  return out;
}
