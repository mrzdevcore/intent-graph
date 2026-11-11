import { expect, test } from "vitest";
import { figure, path, pins, sides, type End } from "../src/map/shapes/path";
import type { Frame } from "../src/map/types";

const box = (x: number, y: number, w = 100, h = 60): Frame => ({ x, y, w, h });
const arrow = (from: string, to: string) => ({ id: `${from}->${to}`, from, to });
const nums = (d: string) => [...d.matchAll(/-?[\d.]+/g)].map((m) => Number(m[0]));
const angle = ([[x0, y0], [x1, y1]]: [[number, number], [number, number]]) => Math.atan2(y1 - y0, x1 - x0);

test("side by side boxes face left and right", () => {
  expect(sides(box(0, 0), box(200, 10))).toEqual(["r", "l"]);
  expect(sides(box(200, 10), box(0, 0))).toEqual(["l", "r"]);
});

test("stacked boxes face bottom and top", () => {
  expect(sides(box(0, 0), box(20, 200))).toEqual(["b", "t"]);
  expect(sides(box(20, 200), box(0, 0))).toEqual(["t", "b"]);
});

test("diagonal boxes take the axis with the larger gap", () => {
  expect(sides(box(0, 0), box(300, 100))).toEqual(["r", "l"]);
  expect(sides(box(0, 0), box(120, 300))).toEqual(["b", "t"]);
  expect(sides(box(300, 100), box(0, 0))).toEqual(["l", "r"]);
});

test("overlapping and equal frames still get a facing pair", () => {
  expect(sides(box(0, 0), box(70, 5))).toEqual(["r", "l"]);
  expect(sides(box(0, 0), box(5, 30))).toEqual(["b", "t"]);
  const same = sides(box(0, 0), box(0, 0));
  expect([["r", "l"], ["b", "t"]]).toContainEqual(same);
});

test("pins spread over the middle 60 % of a side, ordered by heading, one each", () => {
  const frames = { a: box(0, 100, 100, 100), x: box(300, 0), y: box(300, 120), z: box(300, 240) };
  const p = pins([arrow("a", "z"), arrow("a", "x"), arrow("a", "y")], frames);
  const ys = ["a->x", "a->y", "a->z"].map((id) => p.get(id)!.from);
  for (const [x] of ys) expect(x).toBe(100);
  expect(ys.map(([, y]) => y)).toEqual([130, 150, 170]);
  for (const id of ["a->x", "a->y", "a->z"]) expect(p.get(id)!.sides).toEqual(["r", "l"]);
  expect(p.get("a->y")!.to).toEqual([300, 150]);
});

test("wires from different boxes into one side never share a pin", () => {
  const frames = { a: box(0, 0), b: box(200, 0), c: box(100, 200, 200, 60) };
  const p = pins([arrow("a", "c"), arrow("b", "c")], frames);
  const [ta, tb] = [p.get("a->c")!.to, p.get("b->c")!.to];
  expect(ta).not.toEqual(tb);
  expect(ta[0]).toBeLessThan(tb[0]);
  expect(ta[1]).toBe(200);
});

test("an arrow to itself or to a box with no frame has no pins", () => {
  const p = pins([arrow("a", "a"), arrow("a", "q")], { a: box(0, 0) });
  expect(p.size).toBe(0);
});

test("a person's wire starts at the figure, not the frame", () => {
  const frames = { u: box(0, 0, 200, 160), s: box(400, 0, 100, 160) };
  const f = figure(frames.u, "person");
  expect(f).toEqual({ x: 40, y: 40, w: 120, h: 80 });
  const p = pins([arrow("u", "s")], frames, { u: "person" }).get("u->s")!;
  expect(p.from).toEqual([160, 80]);
  expect(p.to).toEqual([400, 80]);
});

test("straight runs pin to pin", () => {
  const r = path("straight", { from: [0, 0], to: [30, 40], sides: ["r", "l"] });
  expect(r.d).toBe("M0,0 L30,40");
  expect(r.len).toBe(50);
  expect(r.mid).toEqual([15, 20]);
  expect(r.ends).toEqual([[0, 0], [30, 40]]);
  expect(angle(r.head)).toBeCloseTo(Math.atan2(40, 30));
});

test("elbow goes across, down at the middle, across, its corners rounded", () => {
  const r = path("elbow", { from: [100, 30], to: [300, 130], sides: ["r", "l"] });
  expect(r.d).toBe("M100,30 L192,30 Q200,30 200,38 L200,122 Q200,130 208,130 L300,130");
  expect(r.len).toBe(300);
  expect(r.mid).toEqual([200, 80]);
  expect(angle(r.head)).toBeCloseTo(0);
  const v = path("elbow", { from: [50, 60], to: [150, 200], sides: ["b", "t"] });
  expect(v.d.startsWith("M50,60 L50,122 Q50,130 58,130")).toBe(true);
  expect(angle(v.head)).toBeCloseTo(Math.PI / 2);
});

test("elbow corners never round more than half a short leg", () => {
  const r = path("elbow", { from: [0, 0], to: [100, 6], sides: ["r", "l"] });
  expect(r.d).toContain("Q50,0 50,3");
});

test("elbow with lined-up pins is one straight run", () => {
  expect(path("elbow", { from: [100, 30], to: [300, 30], sides: ["r", "l"] }).d).toBe("M100,30 L300,30");
});

test("curved puts its control points along each side's normal at 40 % of the distance", () => {
  const r = path("curved", { from: [100, 0], to: [300, 0], sides: ["r", "l"] });
  expect(r.d).toBe("M100,0 C180,0 220,0 300,0");
  const s = path("curved", { from: [0, 100], to: [300, 500], sides: ["b", "t"] });
  expect(s.d).toBe("M0,100 C0,300 300,300 300,500");
  expect(angle(s.head)).toBeCloseTo(Math.PI / 2);
  expect(s.mid[0]).toBeCloseTo(150, 0);
  expect(s.mid[1]).toBeCloseTo(300, 0);
});

const crosses = (d: string, f: Frame) => {
  const [x0, y0, x1, y1, x2, y2, x3, y3] = nums(d);
  for (let i = 1; i < 48; i++) {
    const t = i / 48, u = 1 - t;
    const x = u ** 3 * x0 + 3 * u * u * t * x1 + 3 * u * t * t * x2 + t ** 3 * x3;
    const y = u ** 3 * y0 + 3 * u * u * t * y1 + 3 * u * t * t * y2 + t ** 3 * y3;
    if (x > f.x && x < f.x + f.w && y > f.y && y < f.y + f.h) return true;
  }
  return false;
};

test("curved bends around a box in its way", () => {
  const end: End = { from: [100, 30], to: [500, 30], sides: ["r", "l"] };
  const wall = box(250, 0, 100, 60);
  const r = path("curved", end, [box(0, 0), wall, box(500, 0)]);
  expect(r.d.includes(" C")).toBe(true);
  expect(crosses(r.d, wall)).toBe(false);
  expect(angle(r.head)).toBeCloseTo(Math.atan2(30 - nums(r.d)[5], 500 - nums(r.d)[4]));
});

test("curved falls back to elbow when it cannot get round", () => {
  const end: End = { from: [100, 30], to: [500, 30], sides: ["r", "l"] };
  const walls = [box(150, -3000, 100, 6000), box(350, -3000, 100, 6000)];
  const r = path("curved", end, walls);
  expect(r).toEqual(path("elbow", end));
});

test("no NaN on overlapping or zero-size frames", () => {
  const frames = { a: box(0, 0, 0, 0), b: box(0, 0, 0, 0), c: box(10, 10, 100, 60), d: box(20, 15, 100, 60) };
  const p = pins([arrow("a", "b"), arrow("c", "d"), arrow("a", "c")], frames, { a: "person" });
  expect(p.size).toBe(3);
  for (const end of p.values())
    for (const shape of ["elbow", "curved", "straight"] as const) {
      const r = path(shape, end, Object.values(frames));
      const all = [...nums(r.d), ...r.mid, r.len, ...r.head.flat()];
      expect(all.every(Number.isFinite), `${shape} ${r.d}`).toBe(true);
    }
});
