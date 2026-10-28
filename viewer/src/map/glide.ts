import type { Cam } from "./camera";

export const GLIDE_MS = 800;

/** cubic-bezier(0.2, 0.7, 0.2, 1): progress p in [0,1] -> eased progress. */
function ease(p: number): number {
  const bz = (u: number, a: number, b: number) => 3 * (1 - u) ** 2 * u * a + 3 * (1 - u) * u * u * b + u ** 3;
  let lo = 0, hi = 1;
  for (let i = 0; i < 30; i++) {
    const m = (lo + hi) / 2;
    if (bz(m, 0.2, 0.2) < p) lo = m; else hi = m;
  }
  return bz((lo + hi) / 2, 0.7, 1);
}

/**
 * The camera t ms into an 800 ms glide; scale moves in log space. Translation zooms about the
 * fixed point (the world point both cameras put on the same screen spot), so a deep zoom heads
 * straight in instead of swinging past; with no change of scale it is a plain eased pan.
 */
export function glide(from: Cam, to: Cam, t: number): Cam {
  const p = t / GLIDE_MS;
  if (!(p > 0)) return { ...from };
  if (p >= 1) return { ...to };
  const e = ease(p);
  const k = Math.exp(Math.log(from.k) + (Math.log(to.k) - Math.log(from.k)) * e);
  const dk = to.k - from.k;
  if (Math.abs(dk) < 1e-9 * Math.max(from.k, to.k)) return { x: from.x + (to.x - from.x) * e, y: from.y + (to.y - from.y) * e, k };
  const wx = (from.x - to.x) / dk, wy = (from.y - to.y) / dk;
  return { x: from.x + (from.k - k) * wx, y: from.y + (from.k - k) * wy, k };
}

/** A wheel event as a zoom factor: lines and pages become pixels; a pinch (ctrlKey) zooms faster. */
export function wheelFactor(e: { deltaY: number; deltaMode: number; ctrlKey: boolean }): number {
  const px = e.deltaY * (e.deltaMode === 1 ? 16 : e.deltaMode === 2 ? 400 : 1);
  return Math.exp(-px * (e.ctrlKey ? 0.01 : 0.002));
}

/** The map's camera. `to` glides from wherever it is now; `set` jumps (wheel, drag). */
export class Camera {
  cam: Cam;
  private from: Cam;
  private target: Cam | null = null;
  private start = 0;
  private last = 0;

  constructor(cam: Cam = { x: 0, y: 0, k: 1 }) {
    this.cam = { ...cam };
    this.from = this.cam;
  }
  get moving(): boolean {
    return this.target !== null;
  }
  to(target: Cam, now = this.last): void {
    this.from = this.cam;
    this.target = target;
    this.start = now;
  }
  set(cam: Cam): void {
    this.cam = cam;
    this.target = null;
  }
  tick(now: number): Cam {
    this.last = now;
    if (this.target) {
      this.cam = glide(this.from, this.target, now - this.start);
      if (now - this.start >= GLIDE_MS) this.target = null;
    }
    return this.cam;
  }
}
