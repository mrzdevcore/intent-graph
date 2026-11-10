import { expect, test } from "vitest";
import { GRID, gridLevel, momentum, softK, wheelIntent } from "../src/map/nav";

const ev = (o: Partial<Parameters<typeof wheelIntent>[0]> = {}) => ({ deltaX: 0, deltaY: 0, deltaMode: 0, ctrlKey: false, metaKey: false, shiftKey: false, ...o });

test("wheelIntent: pinch and modifier zoom, trackpad scroll pans, mouse wheel zooms", () => {
  expect(wheelIntent(ev({ deltaY: 2.5, ctrlKey: true }), "auto")).toBe("zoom");
  expect(wheelIntent(ev({ deltaY: 3, metaKey: true }), "pan")).toBe("zoom");
  expect(wheelIntent(ev({ deltaY: 100 }), "auto")).toBe("zoom");
  expect(wheelIntent(ev({ deltaY: 3, deltaMode: 1 }), "auto")).toBe("zoom");
  expect(wheelIntent(ev({ deltaY: 7.25 }), "auto")).toBe("pan");
  expect(wheelIntent(ev({ deltaX: 6, deltaY: 4 }), "auto")).toBe("pan");
  expect(wheelIntent(ev({ deltaY: 4 }), "auto")).toBe("pan");
  expect(wheelIntent(ev({ deltaY: 100, shiftKey: true }), "auto")).toBe("pan");
});

test("wheelIntent: the preference overrides the guess", () => {
  expect(wheelIntent(ev({ deltaY: 100 }), "pan")).toBe("pan");
  expect(wheelIntent(ev({ deltaY: 7.25 }), "zoom")).toBe("zoom");
});

test("gridLevel: spacing stays in range, the halfway dots fade in as it grows", () => {
  for (const k of [0.1, 0.5, 1, 4]) {
    const { step, fade } = gridLevel(k, GRID), sp = GRID * k * step;
    expect(sp).toBeGreaterThanOrEqual(32);
    expect(sp).toBeLessThan(64);
    expect(fade).toBeGreaterThanOrEqual(0);
    expect(fade).toBeLessThanOrEqual(1);
  }
  expect(gridLevel(0.1, 24).step).toBeGreaterThan(gridLevel(0.5, 24).step);
  expect(gridLevel(4, 24).step).toBeLessThan(1);
  expect(gridLevel(4, 24).step).toBeLessThan(gridLevel(1, 24).step);
});

test("gridLevel: no pop at a level change", () => {
  const out = gridLevel(32 / 24 + 1e-6, 24), inn = gridLevel(32 / 24 - 1e-6, 24);
  expect(out.fade).toBeLessThan(0.01);
  expect(inn.fade).toBeGreaterThan(0.99);
  expect(inn.step).toBeCloseTo(out.step * 2, 5);
});

test("momentum decays and stops", () => {
  expect(momentum({ x: 1, y: 0 }, 16).x).toBeCloseTo(0.92);
  expect(momentum({ x: 0.04, y: 0 }, 16)).toEqual({ x: 0, y: 0 });
  let v = { x: 2, y: -1 };
  for (let i = 0; i < 200 && (v.x || v.y); i++) v = momentum(v, 16);
  expect(v).toEqual({ x: 0, y: 0 });
});

test("softK resists beyond the limits", () => {
  expect(softK(1, 0.5, 2)).toBe(1);
  expect(softK(4, 0.5, 2)).toBeLessThan(4);
  expect(softK(4, 0.5, 2)).toBeGreaterThan(2);
});
