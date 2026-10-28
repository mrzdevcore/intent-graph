import type { Frame, View } from "./types";

/** screen = world * k + (x, y) */
export type Cam = { x: number; y: number; k: number };

export function fitTo(f: Frame, view: { w: number; h: number }, margin = 0.1): Cam {
  const k = Math.min((view.w * (1 - 2 * margin)) / f.w, (view.h * (1 - 2 * margin)) / f.h);
  return { k, x: view.w / 2 - (f.x + f.w / 2) * k, y: view.h / 2 - (f.y + f.h / 2) * k };
}

/** The frame in system units: each frame is in its parent's inner units, so scale by parent world w / inner w. */
export function worldFrame(id: string, view: View): Frame {
  const parent = new Map(view.boxes.map((b) => [b.id, b.parent]));
  const walk = (i: string): Frame => {
    const f = view.placed.frames[i];
    const p = parent.get(i);
    if (!f) return { x: 0, y: 0, w: view.placed.inner[i]?.[0] ?? 0, h: view.placed.inner[i]?.[1] ?? 0 }; // the system: no frame, its units are the world's
    if (!p) return f;
    const pw = walk(p);
    const inner = view.placed.inner[p];
    const s = inner ? pw.w / inner[0] : 1;
    return { x: pw.x + f.x * s, y: pw.y + f.y * s, w: f.w * s, h: f.h * s };
  };
  return walk(id);
}

export const detailFor = (w: number): "far" | "mid" | "near" => (w < 140 ? "far" : w < 220 ? "mid" : "near");
export const showsInside = (w: number): boolean => w >= 560;
