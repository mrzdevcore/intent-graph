import type { Frame, View } from "./types";

/** screen = world * k + (x, y) */
export type Cam = { x: number; y: number; k: number };

const pos = (n: number) => Number.isFinite(n) && n > 0;

export function fitTo(f: Frame, view: { w: number; h: number }, margin = 0.1): Cam {
  const k = Math.min((view.w * (1 - 2 * margin)) / f.w, (view.h * (1 - 2 * margin)) / f.h);
  const kk = pos(k) ? k : 1; // a zero-size frame or view: no zoom, still finite
  return { k: kk, x: view.w / 2 - (f.x + f.w / 2) * kk, y: view.h / 2 - (f.y + f.h / 2) * kk };
}

/**
 * Where a parent's inner space sits in the world: the inner box (inner units) is scaled
 * uniformly by s = min(frame.w/inner.w, frame.h/inner.h) and centred in the parent's world
 * frame. world = (x, y) + s * innerPoint. The system has no frame: identity.
 */
export function innerTransform(parentId: string, view: View): { x: number; y: number; s: number } {
  const inner = view.placed.inner[parentId];
  const own = view.placed.frames[parentId];
  if (!own) return { x: 0, y: 0, s: 1 };
  const f = worldFrame(parentId, view);
  const s = inner ? Math.min(f.w / inner[0], f.h / inner[1]) : 1;
  const ss = pos(s) ? s : 1;
  const [iw, ih] = inner ?? [0, 0];
  return { x: f.x + (f.w - iw * ss) / 2, y: f.y + (f.h - ih * ss) / 2, s: ss };
}

/** The frame in system units: each frame is in its parent's inner units, mapped by innerTransform. */
export function worldFrame(id: string, view: View): Frame {
  const f = view.placed.frames[id];
  const p = view.boxes.find((b) => b.id === id)?.parent;
  if (!f) { const [w, h] = view.placed.inner[id] ?? [0, 0]; return { x: 0, y: 0, w, h }; } // the frame-less root: its inner box is the world
  if (!p) return f;
  const t = innerTransform(p, view);
  return { x: t.x + f.x * t.s, y: t.y + f.y * t.s, w: f.w * t.s, h: f.h * t.s };
}

export const detailFor = (w: number): "far" | "mid" | "near" => (w < 140 ? "far" : w < 220 ? "mid" : "near");
export const showsInside = (w: number): boolean => w >= 560;
