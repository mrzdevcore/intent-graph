import { HEAD, OPEN, fitInner, fitTo, placer, showsInside, type Cam } from "./camera";
import { sides, type P, type Side } from "./shapes/path";
import { inside, spreadAt, type InWire, type Port } from "./shapes/inside";
import { lineOf } from "./shapes/auto";
import type { ArrowShape, Edit, Frame, MapArrow, View } from "./types";
import { isRead } from "./db/wiring";

export type PadEnd = { from: string; to: string; key: "from" | "to" };
export type Pad = {
  way: "in" | "out" | "both"; edge: Side; other: string; title: string; child?: string; down: boolean; at: number; more?: number;
  arrows: string[]; ends: PadEnd[]; shape?: ArrowShape; read?: boolean;
};

// a pad's height where it is smallest on screen, at the box width pads start at (showsInside), and its label's width
const ROOM = 24 / OPEN, WIDE = 128 / OPEN;
// the most pads shown on a side; the rest fold into one more
const CAP = 12;
/** In rem: the widest pad label beside the edge (Pads.svelte: dot, name, chip), the details card and its
 *  offset from the view's right (DetailsCard.svelte), that card when it shows code, and a gap all round. */
export const LABEL = 12, CARD = 22.25, CARD_WIDE = 31, GUTTER = 1, TALL = 2.5;
export const ANSWER_CARD = 20;

/** Maps a child's frame (in the box's inner units) to the box's own units, as Map's `local` places it. */
export function inBox(view: View, id: string, frame: { w: number; h: number }) {
  const [iw, ih] = view.placed.inner[id] ?? [0, 0];
  const t = iw && ih ? fitInner(frame, iw, ih) : { x: 0, y: 0, s: 1 };
  return (f: Frame): Frame => ({ x: t.x + f.x * t.s, y: t.y + f.y * t.s, w: f.w * t.s, h: f.h * t.s });
}

/**
 * The camera for opening a box with pads: its whole frame `f` between the view's left and the details
 * card (`card` rem, 0 with none), with room for the labels on the sides that have pads, and the same
 * margin above and below as fitTo. Never so small the box closes, as its pads go with it: a window too
 * narrow for that lets the frame run under the card instead.
 */
export function fitBeside(f: Frame, view: { w: number; h: number }, pads: Pad[], card: number, rem: number, leftCard = 0): Cam {
  if (!(f.w > 0 && f.h > 0)) return fitTo(f, view);
  const on = (e: Side) => pads.some((p) => p.edge === e);
  const left = (leftCard + GUTTER + (on("l") ? LABEL : 0)) * rem;
  const right = (card + GUTTER + (on("r") ? LABEL : 0)) * rem;
  const top = (on("t") ? TALL : 0) * rem, bottom = (on("b") ? TALL : 0) * rem;
  // the floor a hair above OPEN: w * (OPEN / w) can round just under it, and close the box
  const k = Math.max((OPEN + 1e-6) / f.w, Math.min((view.w - left - right) / f.w, (view.h * 0.8 - top - bottom) / f.h));
  return { k, x: (view.w + left - right) / 2 - (f.x + f.w / 2) * k, y: (view.h + top - bottom) / 2 - (f.y + f.h / 2) * k };
}

/**
 * Whether a box at `s` on screen, opened and with pads, is the one you are in: it holds the middle of
 * the space left of the card (`card` rem) and fills 60% of it, less the room for labels on both sides.
 * Whatever fitBeside gave passes this with or without the card still open, so closing it changes nothing.
 */
export function fillsBeside(s: Frame, view: { w: number; h: number }, card: number, rem: number) {
  const x = (view.w - card * rem) / 2, y = view.h / 2;
  const w = Math.max(view.w - (card + 2 * (LABEL + GUTTER)) * rem, OPEN);
  return showsInside(s.w) && s.x <= x && x <= s.x + s.w && s.y <= y && y <= s.y + s.h && Math.max(s.w / w, s.h / view.h) >= 0.6;
}

function placed(all: Pad[], start: number, end: number, room: number) {
  const rows = Math.max(1, Math.floor((end - start) / room));
  const fits = all.length <= Math.min(CAP, rows);
  const pads = fits ? all : all.slice(0, Math.min(CAP, rows - 1));
  const more = all.length - pads.length;
  if (more) pads.push({ way: all[0].way, edge: all[0].edge, other: "", title: `+${more} more`, down: false, at: Infinity, more, arrows: [], ends: [] });
  const even = pads.some((p, i) => !Number.isFinite(p.at) || (i > 0 && p.at - pads[i - 1].at < room));
  return even ? pads.map((p, i) => ({ ...p, at: start + ((i + 0.5) * (end - start)) / pads.length })) : pads;
}

const overlap = (a: Frame, b: Frame) => a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
const across = (e: Side) => e === "l" || e === "r";

export const padPoint = (p: Pad, frame: { w: number; h: number }): [number, number] =>
  p.edge === "l" ? [0, p.at] : p.edge === "r" ? [frame.w, p.at] : p.edge === "t" ? [p.at, 0] : [p.at, frame.h];

export function assign(p: Pad, child: string): Edit {
  const edits: Edit[] = p.ends.map((e) => ({ op: "arrowset", from: e.from, to: e.to, key: e.key, value: child }));
  return edits.length === 1 ? edits[0] : { op: "batch", edits };
}

export const sideAt = (f: Frame, [x, y]: P): Side => {
  const d: [Side, number][] = [["l", Math.abs(x - f.x)], ["r", Math.abs(x - f.x - f.w)], ["t", Math.abs(y - f.y)], ["b", Math.abs(y - f.y - f.h)]];
  return d.reduce((m, o) => (o[1] < m[1] ? o : m))[0];
};

export type Outside = { frame: Frame; wire: (a: MapArrow) => { ends: [P, P] } | undefined };

export function padsOf(
  view: View, id: string, frame: { w: number; h: number }, childFrame: (id: string) => Frame | undefined, on: Set<string>,
  worldOf?: (id: string) => Frame | undefined, outside?: Outside,
): Pad[] {
  const parent = new Map(view.boxes.map((b) => [b.id, b.parent]));
  const title = new Map(view.boxes.map((b) => [b.id, b.title.value]));
  // who holds `end` in the box: null when it is outside, no child when it is the box itself
  const holder = (end: string) => {
    const seen = new Set<string>(); // a bad map's parent loop ends the walk
    let child: string | undefined;
    for (let p: string | undefined = end; p && !seen.has(p); child = p, p = parent.get(p)) {
      if (p === id) return { child };
      seen.add(p);
    }
    return null;
  };
  const world = worldOf ?? (() => {
    const pl = placer(view);
    return (b: string) => (view.placed.frames[b] ? pl.frame(b) : undefined);
  })();
  const here = world(id);
  const place = inBox(view, id, frame);
  const ended = (a: MapArrow) => {
    const w = outside && (a.from === id || a.to === id) ? outside.wire(a) : undefined;
    return w ? sideAt(outside!.frame, w.ends[a.from === id ? 0 : 1]) : undefined;
  };
  const byKey = new Map<string, Pad & { far?: Frame; wired?: boolean }>();
  for (const a of view.arrows) {
    for (const part of a.parts?.length ? a.parts : [a]) {
      const from = holder(part.from), to = holder(part.to);
      if (!from === !to) continue;
      const other = from ? part.to : part.from, child = (from ?? to)!.child, way = from ? "out" : "in";
      const key = JSON.stringify([other, child]);
      let p = byKey.get(key);
      if (!p) {
        const f = world(other), far = here && f && !overlap(here, f) ? f : undefined;
        p = {
          way, edge: far ? sides(here!, far)[0] : way === "out" ? "r" : "l", other, title: title.get(other) ?? other, child,
          down: view.marks.some((m) => m.target === other && m.state === "down" && on.has(m.layer)), at: Infinity, arrows: [], ends: [], shape: part.shape, far, read: true,
        };
        byKey.set(key, p);
      } else if (p.way !== way) {
        p.way = "both";
        if (!p.far && !p.wired) p.edge = "r";
      }
      if (!p.wired) {
        const e = ended(a);
        if (e) [p.edge, p.wired] = [e, true];
      }
      p.shape ??= part.shape;
      p.read &&= isRead(part, view.tables);
      if (!p.arrows.includes(a.id)) p.arrows.push(a.id);
      if (!p.ends.some((e) => e.from === part.from && e.to === part.to)) p.ends.push({ from: part.from, to: part.to, key: from ? "from" : "to" });
    }
  }
  const head = Math.min(HEAD, frame.h / 4);
  const out: Pad[] = [];
  for (const edge of ["r", "l", "t", "b"] as Side[]) {
    const mine = [...byKey.values()].filter((p) => p.edge === edge), x = !across(edge);
    const mid = (f: Frame | undefined) => (f ? (x ? f.x + f.w / 2 : f.y + f.h / 2) : Infinity);
    const kid = new Map(mine.map((p) => {
      const f = p.child ? childFrame(p.child) : undefined;
      return [p, f && place(f)] as const;
    }));
    mine.sort((a, b) => mid(kid.get(a)) - mid(kid.get(b)) || mid(a.far) - mid(b.far) || (a.child ?? "").localeCompare(b.child ?? "") || a.other.localeCompare(b.other));
    for (const p of mine) {
      const c = kid.get(p);
      if (!c) continue;
      const same = mine.filter((q) => q.child === p.child), t = spreadAt(same.indexOf(p), same.length);
      p.at = x ? c.x + c.w * t : c.y + c.h * t;
    }
    const pads = mine.map(({ far: _, wired: __, ...p }) => p);
    out.push(...(x ? placed(pads, 0, frame.w, frame.w * WIDE) : placed(pads, head, frame.h, frame.w * ROOM)));
  }
  return out;
}

export function wiresOf(pads: Pad[], frame: { w: number; h: number }, kids: Record<string, Frame>, shape: ArrowShape): (InWire | null)[] {
  const ports: Port[] = [], at: number[] = [];
  pads.forEach((p, i) => {
    if (p.more || !p.child || !kids[p.child]) return;
    ports.push({ at: padPoint(p, frame), edge: p.edge, child: p.child, shape: lineOf(p, shape) });
    at.push(i);
  });
  const drawn = inside(ports, frame, kids, Math.min(HEAD, frame.h / 4)), out: (InWire | null)[] = pads.map(() => null);
  at.forEach((i, k) => (out[i] = drawn[k]));
  return out;
}
