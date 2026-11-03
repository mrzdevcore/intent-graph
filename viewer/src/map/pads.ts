import { HEAD, OPEN, fitInner, fitTo, showsInside, type Cam } from "./camera";
import type { Frame, View } from "./types";

/** A connection on an open box's edge. `other`: the box at its far end, outside this one. `child`:
 *  the direct child holding this end, absent for a stub. `y`: in the box's own units. `more`: not a
 *  connection but the fold of that many, with no far box. */
export type Pad = { side: "in" | "out"; other: string; title: string; child?: string; down: boolean; y: number; more?: number };

// a pad's height where it is smallest on screen, at the box width pads start at (showsInside)
const ROOM = 24 / OPEN;
// the most pads shown on a side; the rest fold into one more
const CAP = 12;
/** In rem: the widest pad label beside the edge (Pads.svelte: dot, name, chip), the details card and its
 *  offset from the view's right (DetailsCard.svelte), that card when it shows code, and a gap all round. */
export const LABEL = 12, CARD = 22.25, CARD_WIDE = 31, GUTTER = 1;
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
  const left = (leftCard + GUTTER + (pads.some((p) => p.side === "in") ? LABEL : 0)) * rem;
  const right = (card + GUTTER + (pads.some((p) => p.side === "out") ? LABEL : 0)) * rem;
  // the floor a hair above OPEN: w * (OPEN / w) can round just under it, and close the box
  const k = Math.max((OPEN + 1e-6) / f.w, Math.min((view.w - left - right) / f.w, (view.h * 0.8) / f.h));
  return { k, x: (view.w + left - right) / 2 - (f.x + f.w / 2) * k, y: view.h / 2 - (f.y + f.h / 2) * k };
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

/** Level with their children, top to bottom; evenly below the header when too close, or when a stub has no
 *  level. Past CAP, or past what the height has room for, the last ones fold into a last pad. */
function placed(all: Pad[], frame: { w: number; h: number }) {
  all.sort((a, b) => a.y - b.y || 0);
  const head = Math.min(HEAD, frame.h / 4), room = frame.w * ROOM;
  const rows = Math.max(1, Math.floor((frame.h - head) / room));
  const fits = all.length <= Math.min(CAP, rows);
  const pads = fits ? all : all.slice(0, Math.min(CAP, rows - 1));
  const more = all.length - pads.length;
  if (more) pads.push({ side: all[0].side, other: "", title: `+${more} more`, down: false, y: Infinity, more });
  const even = pads.some((p, i) => !Number.isFinite(p.y) || (i > 0 && p.y - pads[i - 1].y < room));
  return even ? pads.map((p, i) => ({ ...p, y: head + ((i + 0.5) * (frame.h - head)) / pads.length })) : pads;
}

/**
 * The pads of box `id`: one for every arrow as written (a part of a merged arrow, or the arrow if it
 * has none) with one end at or under the box and the other outside; alike ones are one pad.
 * `childFrame` gives a child's frame as drawn, in the box's inner units; `on`: the layers shown.
 */
export function padsOf(view: View, id: string, frame: { w: number; h: number }, childFrame: (id: string) => Frame | undefined, on: Set<string>): Pad[] {
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
  const place = inBox(view, id, frame);
  const pads: Pad[] = [], seen = new Set<string>();
  for (const a of view.arrows) {
    for (const part of a.parts?.length ? a.parts : [a]) {
      const from = holder(part.from), to = holder(part.to);
      if (!from === !to) continue;
      const other = from ? part.to : part.from, child = (from ?? to)!.child;
      const key = JSON.stringify([!!from, other, child]);
      if (seen.has(key)) continue;
      seen.add(key);
      const f = child ? childFrame(child) : undefined, at = f && place(f);
      pads.push({
        side: from ? "out" : "in", other, title: title.get(other) ?? other, child,
        down: view.marks.some((m) => m.target === other && m.state === "down" && on.has(m.layer)),
        y: at ? at.y + at.h / 2 : Infinity,
      });
    }
  }
  return [...placed(pads.filter((p) => p.side === "out"), frame), ...placed(pads.filter((p) => p.side === "in"), frame)];
}
