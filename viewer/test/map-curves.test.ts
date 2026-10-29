import { expect, test } from "vitest";
import { curves } from "../src/map/curves";
import type { MapArrow } from "../src/map/types";

const arrow = (from: string, to: string): MapArrow => ({ id: `${from}->${to}`, parent: "p", from, to, kind: "uses", count: 1, kinds: ["uses"] });
const frames = {
  a: { x: 0, y: 0, w: 100, h: 60 },
  b: { x: 300, y: 0, w: 100, h: 60 },
  c: { x: 300, y: 200, w: 100, h: 60 },
  d: { x: 0, y: 300, w: 100, h: 60 },
};

test("an arrow leaves the side facing its end and enters the opposite one, as a curve", () => {
  const c = curves([arrow("a", "b")], frames).get("a->b")!;
  expect(c.d).toBe("M100,30 C190,30 210,30 300,30");
  expect(c.mid).toEqual([200, 30]);
  // below: from the bottom side to the top side
  expect(curves([arrow("a", "d")], frames).get("a->d")!.d).toMatch(/^M50,60 C.* 50,300$/);
});

test("arrows sharing a side spread along it, ordered by their other end", () => {
  const out = curves([arrow("a", "c"), arrow("a", "b")], frames);
  const start = (id: string) => out.get(id)!.d.match(/^M([\d.]+),([\d.]+)/)!.slice(1).map(Number);
  expect(start("a->b")[0]).toBe(100);
  expect(start("a->c")[0]).toBe(100);
  // b is higher than c: its arrow leaves higher
  expect(start("a->b")[1]).toBeLessThan(start("a->c")[1]);
});

test("a box's arrow to itself, or to a box with no frame, has no curve", () => {
  expect(curves([arrow("a", "a"), arrow("a", "zz")], frames).size).toBe(0);
});
