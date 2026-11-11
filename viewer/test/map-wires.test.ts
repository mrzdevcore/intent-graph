import { expect, test } from "vitest";
import { wires, type Mode } from "../src/map/wires";
import { autoShape, lineOf, mapShape } from "../src/map/shapes/auto";
import type { Frame, MapArrow } from "../src/map/types";

const arrow = (from: string, to: string): MapArrow => ({ id: `${from}->${to}`, parent: "p", from, to, kind: "uses", count: 1, kinds: ["uses"] });
const points = (d: string) => [...d.matchAll(/(-?[\d.]+),(-?[\d.]+)/g)].map((m) => [Number(m[1]), Number(m[2])] as [number, number]);

// three rows of boxes 100 wide, 96 apart; the middle row has a hole in its middle
const box = (x: number, y: number): Frame => ({ x, y, w: 100, h: 60 });
const frames: Record<string, Frame> = {
  a: box(0, 0), b: box(196, 0), c: box(392, 0),
  d: box(0, 156), e: box(392, 156),
  f: box(0, 312), g: box(196, 312), h: box(392, 312),
};
const all = [arrow("a", "g"), arrow("b", "h"), arrow("c", "f"), arrow("a", "d"), arrow("b", "e"), arrow("g", "b"), arrow("d", "e"), arrow("a", "c")];

/** Every leg of every wire is level, upright or a 45° cut, and passes no box but its own two. */
function check(mode: Mode, arrows = all, fs = frames) {
  const w = wires(arrows, fs, mode).wires;
  expect(w.size).toBe(arrows.length);
  for (const a of arrows) {
    const p = points(w.get(a.id)!.d);
    for (let i = 0; i < p.length - 1; i++) {
      const [dx, dy] = [Math.abs(p[i + 1][0] - p[i][0]), Math.abs(p[i + 1][1] - p[i][1])];
      expect(dx === 0 || dy === 0 || Math.abs(dx - dy) < 0.02, `${a.id} leg ${i}: ${dx}x${dy}`).toBe(true);
      for (const [id, f] of Object.entries(fs)) {
        if (id === a.from || id === a.to) continue;
        for (const t of [0.25, 0.5, 0.75]) {
          const [x, y] = [p[i][0] + (p[i + 1][0] - p[i][0]) * t, p[i][1] + (p[i + 1][1] - p[i][1]) * t];
          expect(x > f.x && x < f.x + f.w && y > f.y && y < f.y + f.h, `${a.id} crosses ${id}`).toBe(false);
        }
      }
    }
  }
  return w;
}

test("routed wires leave the facing sides, go round the rows past a box in the way, and cross no box", () => {
  const w = check("routed");
  const p = points(w.get("a->g")!.d);
  expect(p[0][1]).toBe(60);
  expect(p[p.length - 1][1]).toBe(312);
  // pointing up: from g's top into b's bottom
  const up = points(w.get("g->b")!.d);
  expect(up[0][1]).toBe(312);
  expect(up[up.length - 1][1]).toBe(60);
  // b stands between a and c: out of one bottom, into the other's
  const side = points(w.get("a->c")!.d);
  expect([side[0][1], side[side.length - 1][1]]).toEqual([60, 60]);
  // nothing between d and e: one straight run, right side to left side
  expect(w.get("d->e")!.d).toBe("M100,186 L392,186");
  check("ribbon");
});

test("two boxes side by side: one straight wire between their facing sides", () => {
  const w = wires([arrow("a", "b")], { a: box(0, 0), b: box(200, 0) }, "routed").wires.get("a->b")!;
  expect(w.d).toBe("M100,30 L200,30");
  expect(w.ends).toEqual([[100, 30], [200, 30]]);
  const back = wires([arrow("b", "a")], { a: box(0, 0), b: box(200, 0) }, "routed").wires.get("b->a")!;
  expect(back.d).toBe("M200,30 L100,30");
});

test("stacked boxes: bottom to top, straight when the pins line up", () => {
  const w = wires([arrow("a", "b")], { a: box(0, 0), b: box(0, 156) }, "routed").wires.get("a->b")!;
  expect(w.d).toBe("M50,60 L50,156");
});

test("a person left of a box: from the figure's right edge into the box's left side, one bend", () => {
  const frames = { u: { x: 0, y: 0, w: 200, h: 160 }, s: { x: 400, y: 100, w: 100, h: 160 } };
  const p = points(wires([arrow("u", "s")], frames, "routed", { u: "person" }).wires.get("u->s")!.d);
  expect(p[0]).toEqual([160, 80]);
  expect(p[p.length - 1]).toEqual([400, 180]);
  // level out, down the track between them, level in: two 45° cuts, so six points
  expect(p.length).toBe(6);
  expect(p[1][1]).toBe(80);
  expect(p[4][1]).toBe(180);
  expect(p[2][0]).toBeGreaterThan(200);
  expect(p[3][0]).toBeLessThan(400);
});

test("wires from different boxes never share a pin", () => {
  const w = wires(all, frames, "routed").wires;
  const ends = [...w.values()].flatMap((x) => x.ends.map((e) => e.join()));
  expect(new Set(ends).size).toBe(ends.length);
  const into = wires([arrow("a", "c"), arrow("b", "c")], { a: box(0, 0), b: box(200, 0), c: box(100, 200) }, "routed").wires;
  expect(into.get("a->c")!.ends[1]).not.toEqual(into.get("b->c")!.ends[1]);
});

test("runs along one channel take lanes of their own, a pitch apart", () => {
  const w = wires(all, frames, "routed").wires;
  // the level runs under the top row
  const runs = all.map((a) => points(w.get(a.id)!.d)).flatMap((p) => p.slice(0, -1).map((q, i) => [q, p[i + 1]]))
    .filter(([q, r]) => q[1] === r[1] && q[1] > 60 && q[1] < 156);
  const ys = new Set(runs.map(([q]) => q[1]));
  expect(ys.size).toBeGreaterThan(1);
  for (const y of ys) for (const z of ys) if (y !== z) expect(Math.abs(y - z)).toBeGreaterThanOrEqual(1);
});

test("one side's wires spread along it; ribbons gather wires into fewer gaps than routing", () => {
  const w = wires(all, frames, "routed").wires;
  const starts = ["a->g", "a->d", "a->c"].map((id) => points(w.get(id)!.d)[0][0]);
  expect(new Set(starts).size).toBe(3);
  // the wires dropping past the middle row: which x they drop at
  const drops = (m: Mode) => {
    const ww = wires(all, frames, m).wires;
    return new Set(["a->g", "b->h", "c->f", "g->b"].map((id) => {
      const p = points(ww.get(id)!.d);
      const leg = p.slice(0, -1).findIndex((q, i) => q[0] === p[i + 1][0] && Math.min(q[1], p[i + 1][1]) < 156 && Math.max(q[1], p[i + 1][1]) > 216);
      return Math.round(p[leg][0] / 50);
    }));
  };
  expect(drops("ribbon").size).toBeLessThanOrEqual(drops("routed").size);
  expect(wires(all, frames, "ribbon").sleeves.length).toBeGreaterThan(0);
  expect(wires(all, frames, "routed").sleeves).toEqual([]);
});

test("a box's arrow to itself, or to a box with no frame, has no wire", () => {
  expect(wires([arrow("a", "a"), arrow("a", "zz")], frames, "routed").wires.size).toBe(0);
});

test("two stacked boxes a third chains into one row: a bend between them, through no box", () => {
  const fs = { a: box(0, 0), c: box(300, 40), b: box(40, 90) };
  const w = check("routed", [arrow("a", "b")], fs);
  const p = points(w.get("a->b")!.d);
  expect(p[0][1]).toBe(60);
  expect(p[p.length - 1][1]).toBe(90);
});

test("in a chained row with a box under one end, a wire blocked sideways goes over the top, through no box", () => {
  const fs = { a: box(0, 0), c: box(150, 0), b: box(300, 0), e: { x: 450, y: 0, w: 100, h: 200 }, d: box(0, 90) };
  const p = points(check("routed", [arrow("a", "b")], fs).get("a->b")!.d);
  expect([p[0][1], p[p.length - 1][1]]).toEqual([0, 0]);
});

test("a crowded row: every wire past a box in the way goes round, through no box", () => {
  const fs = Object.fromEntries(["p", "q", "r", "s", "t"].map((id, i) => [id, box(i * 140, (i % 2) * 20)]));
  const as = ["q", "r", "s", "t"].flatMap((id) => [arrow("p", id), arrow(id, "p")]).concat([arrow("q", "s"), arrow("r", "t"), arrow("t", "q")]);
  check("routed", as, fs);
});

test("Auto: curved up to 12 boxes and 16 arrows at a level, circuit past either", () => {
  expect(autoShape(12, 16)).toBe("curved");
  expect(autoShape(0, 0)).toBe("curved");
  expect(autoShape(13, 16)).toBe("circuit");
  expect(autoShape(12, 17)).toBe("circuit");
  expect(mapShape("auto", 6, 4)).toBe("curved");
  expect(mapShape("elbow", 6, 4)).toBe("elbow");
  expect(mapShape("auto", 40, 4)).toBe("circuit");
});

test("an arrow's own shape wins over the map's", () => {
  expect(lineOf({ shape: "curved" }, "circuit")).toBe("curved");
  expect(lineOf({ shape: "circuit" }, "curved")).toBe("circuit");
  expect(lineOf({}, "elbow")).toBe("elbow");
});

test("arrows not drawn as circuit take `path` from the same pins: one pin each, the rest still circuit", () => {
  const curved = new Set(["a->g", "d->e", "a->c"]);
  const w = wires(all, frames, "routed", {}, (a) => (curved.has(a.id) ? "curved" : "circuit")).wires;
  expect(w.size).toBe(all.length);
  for (const a of all) expect(w.get(a.id)!.d.includes(" C"), a.id).toBe(curved.has(a.id));
  const ends = [...w.values()].flatMap((x) => x.ends.map((e) => e.join()));
  expect(new Set(ends).size).toBe(ends.length);
  const de = w.get("d->e")!;
  expect(de.ends).toEqual([[100, 186], [392, 186]]);
  expect(de.mid).toEqual([246, 186]);
  const ribbon = wires(all, frames, "ribbon").wires;
  expect([...ribbon.values()].some((x) => x.d.includes(" C"))).toBe(false);
});
