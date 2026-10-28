import { expect, test } from "vitest";
import { detailFor, fitTo, showsInside, worldFrame } from "../src/map/camera";
import type { View } from "../src/map/types";
import shop from "./fixtures/shop-view.json";

const view = shop as unknown as View;

test("fitTo centers a frame, the width limits", () => {
  const c = fitTo({ x: 0, y: 0, w: 100, h: 50 }, { w: 1000, h: 800 });
  expect(c.k).toBe(8);
  expect(c.x).toBe(100);
  expect(c.y).toBe(200);
});

test("worldFrame composes three scales", () => {
  const code = "apps/api/src/application/checkout/checkout.ts#checkout";
  const f = view.placed.frames;
  const inner = view.placed.inner;
  const comp = worldFrame("api/application-checkout", view);
  const s1 = f["api"].w / inner["api"][0];
  expect(comp.w).toBeCloseTo(f["api/application-checkout"].w * s1);
  const w = worldFrame(code, view);
  const s2 = comp.w / inner["api/application-checkout"][0];
  expect(w.w).toBeCloseTo(f[code].w * s2);
  expect(w.x).toBeCloseTo(comp.x + f[code].x * s2);
  expect(w.w).toBeLessThan(comp.w);
});

test("detail thresholds", () => {
  expect([139, 140, 219, 220].map(detailFor)).toEqual(["far", "mid", "mid", "near"]);
  expect([559, 560].map(showsInside)).toEqual([false, true]);
});
