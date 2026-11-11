import type { Frame, View } from "./types";

/** screen = world * k + (x, y) */
export type Cam = { x: number; y: number; k: number };

const pos = (n: number) => Number.isFinite(n) && n > 0;

export function fitTo(f: Frame, view: { w: number; h: number }, margin = 0.1): Cam {
  const k = Math.min((view.w * (1 - 2 * margin)) / f.w, (view.h * (1 - 2 * margin)) / f.h);
  const kk = pos(k) ? k : 1; // a zero-size frame or view: no zoom, still finite
  return { k: kk, x: view.w / 2 - (f.x + f.w / 2) * kk, y: view.h / 2 - (f.y + f.h / 2) * kk };
}

// an opened box's inside fills the screen: zoomed in up to where its frame (below the inside's top)
// covers the screen, then kept on the frame, so no stretch of the level above shows past its edge
export function fitInside(area: Frame, frame: Frame, view: { w: number; h: number }): Cam {
  const fit = fitTo(area, view), most = fitTo(area, view, 0);
  const below = frame.y + frame.h - area.y;
  const cover = pos(frame.w) && pos(below) ? Math.max(view.w / frame.w, view.h / below) : fit.k;
  const k = Math.max(fit.k, Math.min(most.k, cover));
  const axis = (a0: number, as: number, f0: number, fs: number, v: number) => {
    let c = v / 2 - (a0 + as / 2) * k;
    if (fs * k >= v) c = Math.min(-f0 * k, Math.max(v - (f0 + fs) * k, c));
    return Math.max(-a0 * k, Math.min(v - (a0 + as) * k, c));
  };
  return { k, x: axis(area.x, area.w, frame.x, frame.w, view.w), y: axis(area.y, area.h, frame.y, frame.h, view.h) };
}

/**
 * Where a parent's inner space sits in the world: the inner box (inner units) is placed in the
 * parent's world frame by fitInner, below its header. world = (x, y) + s * innerPoint. The
 * system has no frame: identity.
 */
export function innerTransform(parentId: string, view: View): { x: number; y: number; s: number } {
  return placer(view).inside(parentId);
}

/** An open box's header, in its frame's units: its title stays there, above its inside. */
export const HEAD = 30;

/**
 * Where an inner space of iw × ih sits in frame f (sized in units `unit` times its own): below
 * the header, scaled uniformly by s, centred. Relative to the frame's top left.
 */
export function fitInner(f: { w: number; h: number }, iw: number, ih: number, unit = 1): { x: number; y: number; s: number } {
  const u = pos(unit) ? unit : 1;
  const head = Math.min(HEAD * u, f.h / 4);
  const s0 = Math.min(f.w / iw, (f.h - head) / ih), s = pos(s0) ? s0 : 1;
  return { x: (f.w - iw * s) / 2, y: head + (f.h - head - ih * s) / 2, s };
}

/** The frame in system units: each frame is in its parent's inner units, mapped by innerTransform. */
export function worldFrame(id: string, view: View): Frame {
  return placer(view).frame(id);
}

/**
 * worldFrame and innerTransform of one view, each worked out once: build it once per view and
 * ask it for every box, in time linear in the boxes. A box's parent is that of the first box of
 * its id; a bad map's parent loop ends the walk at the box that closes it.
 */
export function placer(view: View) {
  const parent = new Map<string, string | undefined>();
  for (const b of view.boxes) if (!parent.has(b.id)) parent.set(b.id, b.parent);
  const frames = new Map<string, Frame>(), insides = new Map<string, { x: number; y: number; s: number }>();
  const walking = new Set<string>();
  function inside(pid: string): { x: number; y: number; s: number } {
    const done = insides.get(pid);
    if (done) return done;
    const own = view.placed.frames[pid];
    let t = { x: 0, y: 0, s: 1 };
    if (own) {
      const f = frame(pid), [iw, ih] = view.placed.inner[pid] ?? [0, 0];
      const r = fitInner(f, iw, ih, f.h / own.h);
      t = { x: f.x + r.x, y: f.y + r.y, s: r.s };
    }
    insides.set(pid, t);
    return t;
  }
  function frame(id: string): Frame {
    const done = frames.get(id);
    if (done) return done;
    const f = view.placed.frames[id];
    let out: Frame;
    if (!f) {
      const [w, h] = view.placed.inner[id] ?? [0, 0]; // the frame-less root: its inner box is the world
      out = { x: 0, y: 0, w, h };
    } else {
      const p = parent.get(id);
      if (!p || walking.has(p) || p === id) out = f;
      else {
        walking.add(id);
        const t = inside(p);
        walking.delete(id);
        out = { x: t.x + f.x * t.s, y: t.y + f.y * t.s, w: f.w * t.s, h: f.h * t.s };
      }
    }
    frames.set(id, out);
    return out;
  }
  return { frame, inside };
}

/** What a size in units must be scaled by to keep a steady size on screen at `zoom` screen
 *  pixels per unit: it follows the zoom only a little. */
export const steady = (zoom: number) => Math.min(1.6, Math.max(0.6, zoom)) / (zoom > 0 ? zoom : 1);

export const detailFor = (w: number): "far" | "mid" | "near" => (w < 140 ? "far" : w < 220 ? "mid" : "near");
/** The screen width from which a box shows its inside. */
export const OPEN = 560;
export const showsInside = (w: number): boolean => w >= OPEN;
