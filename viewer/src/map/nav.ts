export type WheelPref = "auto" | "zoom" | "pan";

export function wheelIntent(
  e: { deltaX: number; deltaY: number; deltaMode: number; ctrlKey: boolean; metaKey: boolean; shiftKey: boolean },
  pref: WheelPref,
): "zoom" | "pan" {
  if (e.ctrlKey || e.metaKey) return "zoom";
  if (pref === "pan") return "pan";
  if (e.shiftKey) return "pan";
  if (pref === "zoom") return "zoom";
  if (e.deltaMode !== 0) return "zoom";
  if (!Number.isInteger(e.deltaY) || !Number.isInteger(e.deltaX)) return "pan";
  if (e.deltaX !== 0) return "pan";
  return Math.abs(e.deltaY) >= 50 ? "zoom" : "pan";
}

export const GRID = 24;
const LO = 8, HI = 64;

// step: the main dots' spacing in grid units (a power of 4); fade: how visible the dots a quarter of that apart are
export function gridLevel(k: number, unit: number): { step: number; fade: number } {
  const s = unit * k;
  if (!(s > 0) || !Number.isFinite(s)) return { step: 1, fade: 0 };
  const n = Math.ceil(Math.log2((2 * LO) / s) / 2 - 1e-9);
  const step = 4 ** n, b = s * step;
  return { step, fade: Math.min(1, Math.max(0, (b / 4 - LO) / (HI / 4 - LO))) };
}

export function edgePan(pt: { x: number; y: number }, size: { w: number; h: number }, margin: number): { x: number; y: number } {
  const axis = (v: number, len: number) => {
    if (!(margin > 0)) return 0;
    if (v < margin) return Math.min(1, (margin - v) / margin);
    if (v > len - margin) return -Math.min(1, (v - (len - margin)) / margin);
    return 0;
  };
  return { x: axis(pt.x, size.w), y: axis(pt.y, size.h) };
}

const DECAY = 0.92, FRAME = 16, REST = 0.05;

export function momentum(v: { x: number; y: number }, ms: number): { x: number; y: number } {
  const f = DECAY ** (ms / FRAME), x = v.x * f, y = v.y * f;
  return Math.hypot(x, y) < REST ? { x: 0, y: 0 } : { x, y };
}

export function softK(k: number, lo: number, hi: number): number {
  if (k < lo) return lo * (k / lo) ** 0.4;
  if (k > hi) return hi * (k / hi) ** 0.4;
  return k;
}
