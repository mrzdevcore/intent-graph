import { expect, test } from "vitest";
import { Camera, glide } from "../src/map/glide";

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
  // log space: k and the eased progress agree (k = 4^e, x = -500e)
  const e = glide(a, b, 200).x / -500;
  expect(glide(a, b, 200).k).toBeCloseTo(4 ** e, 6);
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
