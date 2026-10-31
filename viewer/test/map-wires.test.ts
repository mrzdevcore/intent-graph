import { expect, test } from "vitest";
import { wires, type Mode } from "../src/map/wires";
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
function check(mode: Mode) {
  const w = wires(all, frames, mode).wires;
  expect(w.size).toBe(all.length);
  for (const a of all) {
    const p = points(w.get(a.id)!.d);
    for (let i = 0; i < p.length - 1; i++) {
      const [dx, dy] = [Math.abs(p[i + 1][0] - p[i][0]), Math.abs(p[i + 1][1] - p[i][1])];
      expect(dx === 0 || dy === 0 || Math.abs(dx - dy) < 0.02, `${a.id} leg ${i}: ${dx}x${dy}`).toBe(true);
      for (const [id, f] of Object.entries(frames)) {
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

test("wires leave the upper box's bottom, enter the lower box's top, and cross no box", () => {
  const w = check("routed");
  const p = points(w.get("a->g")!.d);
  expect(p[0][1]).toBe(60);
  expect(p[p.length - 1][1]).toBe(312);
  // pointing up: from g's top into b's bottom
  const up = points(w.get("g->b")!.d);
  expect(up[0][1]).toBe(312);
  expect(up[up.length - 1][1]).toBe(60);
  // two boxes of one row: out of one bottom, into the other's
  const side = points(w.get("a->c")!.d);
  expect([side[0][1], side[side.length - 1][1]]).toEqual([60, 60]);
  check("ribbon");
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
