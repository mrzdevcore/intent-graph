import { HEAD, fitInner } from "./camera";
import type { Frame, View } from "./types";

/** A connection on an open box's edge. `other`: the box at its far end, outside this one. `child`:
 *  the direct child holding this end, absent for a stub. `y`: in the box's own units. */
export type Pad = { side: "in" | "out"; other: string; title: string; child?: string; down: boolean; y: number };

// a pad's height where it is smallest on screen, at the box width pads start at (showsInside)
const ROOM = 24 / 560;

/** Maps a child's frame (in the box's inner units) to the box's own units, as Map's `local` places it. */
export function inBox(view: View, id: string, frame: { w: number; h: number }) {
  const [iw, ih] = view.placed.inner[id] ?? [0, 0];
  const t = iw && ih ? fitInner(frame, iw, ih) : { x: 0, y: 0, s: 1 };
  return (f: Frame): Frame => ({ x: t.x + f.x * t.s, y: t.y + f.y * t.s, w: f.w * t.s, h: f.h * t.s });
}

/** Level with their children, top to bottom; evenly below the header when too close, or when a stub has no level. */
function placed(pads: Pad[], frame: { w: number; h: number }) {
  pads.sort((a, b) => a.y - b.y || 0);
  const head = Math.min(HEAD, frame.h / 4);
  const even = pads.some((p, i) => !Number.isFinite(p.y) || (i > 0 && p.y - pads[i - 1].y < frame.w * ROOM));
  return even ? pads.map((p, i) => ({ ...p, y: head + ((i + 0.5) * (frame.h - head)) / pads.length })) : pads;
}

/**
 * The pads of box `id`: one for every arrow as written (a part of a merged arrow, or the arrow if it
 * has none) with one end at or under the box and the other outside. `childFrame` gives a child's
 * frame as drawn, in the box's inner units; `on`: the layers shown.
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
  const pads: Pad[] = [];
  for (const a of view.arrows) {
    for (const part of a.parts?.length ? a.parts : [a]) {
      const from = holder(part.from), to = holder(part.to);
      if (!from === !to) continue;
      const other = from ? part.to : part.from, child = (from ?? to)!.child;
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
