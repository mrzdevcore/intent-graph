import { expect, test } from "vitest";
import { detailFor, fitInside, fitTo, innerTransform, showsInside, worldFrame } from "../src/map/camera";
import type { View } from "../src/map/types";
import shop from "./fixtures/shop-view.json";

const view = shop as unknown as View;

test("fitTo centers a frame, the width limits", () => {
  const c = fitTo({ x: 0, y: 0, w: 100, h: 50 }, { w: 1000, h: 800 });
  expect(c.k).toBe(8);
  expect(c.x).toBe(100);
  expect(c.y).toBe(200);
});

test("worldFrame composes three uniform, centred scales", () => {
  const code = "apps/api/src/application/checkout/checkout.ts#checkout";
  const f = view.placed.frames;
  const comp = worldFrame("api/application-checkout", view);
  const t1 = innerTransform("api", view);
  expect(comp.y).toBeCloseTo(t1.y + f["api/application-checkout"].y * t1.s);
  expect(comp.h).toBeCloseTo(f["api/application-checkout"].h * t1.s);
  const t2 = innerTransform("api/application-checkout", view);
  const w = worldFrame(code, view);
  expect(w.x).toBeCloseTo(t2.x + f[code].x * t2.s);
  expect(w.y).toBeCloseTo(t2.y + f[code].y * t2.s);
  expect(w.w).toBeCloseTo(f[code].w * t2.s);
  expect(w.h).toBeCloseTo(f[code].h * t2.s);
});

test("every box sits inside its parent's world frame", () => {
  const e = 1e-6;
  for (const b of view.boxes) {
    if (!b.parent || !view.placed.frames[b.parent]) continue;
    const c = worldFrame(b.id, view), p = worldFrame(b.parent, view);
    expect(c.x).toBeGreaterThanOrEqual(p.x - e);
    expect(c.y).toBeGreaterThanOrEqual(p.y - e);
    expect(c.x + c.w).toBeLessThanOrEqual(p.x + p.w + e);
    expect(c.y + c.h).toBeLessThanOrEqual(p.y + p.h + e);
  }
});

test("zero sizes stay finite", () => {
  const z = { x: 5, y: 5, w: 0, h: 0 };
  for (const c of [fitTo(z, { w: 1000, h: 800 }), fitTo({ x: 0, y: 0, w: 10, h: 10 }, { w: 0, h: 0 })])
    expect(Object.values(c).every(Number.isFinite) && c.k === 1).toBe(true);
  const v = structuredClone(view);
  v.placed.inner["api"] = [0, 0];
  const w = worldFrame("api/application-checkout", v);
  expect(Object.values(w).every(Number.isFinite)).toBe(true);
});

test("detail thresholds", () => {
  expect([139, 140, 219, 220].map(detailFor)).toEqual(["far", "mid", "mid", "near"]);
  expect([559, 560].map(showsInside)).toEqual([false, true]);
});

test("the system's world frame is its inner box, and fits", () => {
  const [w, h] = view.placed.inner["system"];
  const f = worldFrame("system", view);
  expect(f).toEqual({ x: 0, y: 0, w, h });
  const c = fitTo(f, { w: 1000, h: 800 });
  expect(Number.isFinite(c.k) && c.k > 0).toBe(true);
});

test("a parent loop or a self-parent ends the walk", () => {
  const v = structuredClone(view);
  const api = v.boxes.find((b) => b.id === "api")!;
  v.boxes.push({ ...api, id: "zzz", parent: "zzz" }, { ...api, id: "x", parent: "y" }, { ...api, id: "y", parent: "x" });
  for (const id of ["zzz", "x", "y"]) v.placed.frames[id] = v.placed.frames.api;
  for (const id of ["zzz", "x", "y"]) {
    expect(Number.isFinite(worldFrame(id, v).w)).toBe(true);
    expect(Number.isFinite(innerTransform(id, v).s)).toBe(true);
  }
});

test("fitInside zooms an opened box's inside until its frame covers the screen, never cutting the inside", () => {
  const frame = { x: 0, y: 0, w: 200, h: 110 }, area = { x: 40, y: 20, w: 120, h: 90 }, view = { w: 1600, h: 1000 };
  const c = fitInside(area, frame, view);
  expect(c.k).toBeCloseTo(1000 / 90);
  expect(c.y).toBeCloseTo(-20 * c.k);
  expect(c.x).toBeLessThanOrEqual(0);
  expect(200 * c.k + c.x).toBeGreaterThanOrEqual(1600 - 1e-6);
  expect(area.x * c.k + c.x).toBeGreaterThanOrEqual(0);
  expect((area.x + area.w) * c.k + c.x).toBeLessThanOrEqual(1600 + 1e-6);
  const small = fitInside({ x: 0, y: 50, w: 100, h: 100 }, { x: 0, y: 0, w: 1000, h: 1000 }, view);
  expect(small.k).toBe(fitTo({ x: 0, y: 50, w: 100, h: 100 }, view).k);
  expect(small.x).toBeCloseTo(0);
});

test("fitInside keeps the frame's bottom on the screen's when the inside is wide", () => {
  const frame = { x: 0, y: 0, w: 200, h: 200 }, area = { x: 0, y: 40, w: 200, h: 100 }, view = { w: 1000, h: 1000 };
  const c = fitInside(area, frame, view);
  expect(c.k).toBeCloseTo(5);
  expect(200 * c.k + c.y).toBeCloseTo(1000);
  expect(40 * c.k + c.y).toBeGreaterThanOrEqual(0);
});
