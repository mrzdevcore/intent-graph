import { expect, test } from "vitest";
import { Camera, glide, wheelFactor } from "../src/map/glide";

const a = { x: 0, y: 0, k: 1 }, b = { x: -500, y: -200, k: 4 };

test("glide starts at from, ends at to", () => {
  expect(glide(a, b, 0)).toEqual(a);
  expect(glide(a, b, 800)).toEqual(b);
  expect(glide(a, b, 5000)).toEqual(b);
});

test("scale interpolates in log space, eased", () => {
  // cubic-bezier(0.2,0.7,0.2,1) is front-loaded: at 400 ms the eased progress is ~0.93, so k = 4^0.93.
  const half = glide(a, b, 400).k;
  expect(half).toBeGreaterThan(3.4);
  expect(half).toBeLessThan(3.8);
});

test("a deep zoom heads straight in: the world point at the view centre moves monotonically", () => {
  const C = 500, to = { x: C - 3000 * 16, y: C - 2000 * 16, k: 16 }; // centres world (3000, 2000)
  const at = (c: { x: number; y: number; k: number }) => [(C - c.x) / c.k, (C - c.y) / c.k];
  let prev = at(a);
  for (let t = 20; t <= 800; t += 20) {
    const cur = at(glide(a, to, t));
    expect(cur[0]).toBeGreaterThanOrEqual(prev[0] - 1e-9);
    expect(cur[1]).toBeGreaterThanOrEqual(prev[1] - 1e-9);
    expect(cur[0]).toBeLessThanOrEqual(3000 + 1e-6);
    prev = cur;
  }
  expect(at(glide(a, to, 800))).toEqual([3000, 2000]);
});

test("same scale: a plain eased pan", () => {
  const m = glide(a, { x: 100, y: 0, k: 1 }, 400);
  expect(m.k).toBe(1);
  expect(m.x).toBeGreaterThan(50);
  expect(m.x).toBeLessThan(100);
});

test("wheel: lines and pages are normalised to pixels, a pinch zooms harder", () => {
  const f = (deltaY: number, deltaMode = 0, ctrlKey = false) => wheelFactor({ deltaY, deltaMode, ctrlKey });
  expect(f(16 * 3)).toBeCloseTo(f(3, 1), 12);
  expect(f(400)).toBeCloseTo(f(1, 2), 12);
  expect(f(-10)).toBeGreaterThan(1);
  expect(f(-10, 0, true)).toBeGreaterThan(f(-10));
});

test("camera_retarget_mid_glide", () => {
  const c = new Camera(a);
  const A = { x: 100, y: 100, k: 2 }, B = { x: -50, y: 30, k: 0.5 };
  c.to(A);
  const mid = c.tick(300);
  expect(mid).not.toEqual(a);
  c.to(B);
  expect(c.tick(300)).toEqual(mid); // restarts from where it is
  expect(c.tick(1100)).toEqual(B);
  expect(c.moving).toBe(false);
});
