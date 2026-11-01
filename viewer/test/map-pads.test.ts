// @vitest-environment happy-dom
import { expect, test, vi } from "vitest";
import { flushSync, mount } from "svelte";
import Pads from "../src/map/Pads.svelte";
import { inBox, padsOf } from "../src/map/pads";
import { fitInner } from "../src/map/camera";
import type { Frame, MapArrow, MapBox, Mark, View } from "../src/map/types";

const box = (id: string, parent?: string): MapBox => ({
  id, parent, level: "container", madeOf: [], files: 0, kind: { value: "service", source: "ig" }, title: { value: id.toUpperCase(), source: "ig" },
});
const arrow = (from: string, to: string, parts?: MapArrow[], parent = "system"): MapArrow => ({ id: `${from}->${to}`, parent, from, to, kind: "calls", count: 0, kinds: ["calls"], parts });
const make = (arrows: MapArrow[], marks: Mark[] = [], extra: MapBox[] = []): View => ({
  version: 1,
  boxes: [box("system"), box("host", "system"), box("traefik", "host"), box("traefik/r1", "traefik"), box("traefik/r2", "traefik"), box("public", "system"), box("a", "public"), box("b", "public"), ...extra],
  arrows, marks, placed: { frames: {}, inner: { traefik: [100, 100] }, regions: [] }, notices: [], warnings: [], kinds: [],
});

const F = { w: 200, h: 200 };
const frames: Record<string, Frame> = { "traefik/r1": { x: 0, y: 0, w: 40, h: 20 }, "traefik/r2": { x: 0, y: 60, w: 40, h: 20 } };
const cf = (id: string) => frames[id];
const on = new Set(["state"]);
const out = make([arrow("host", "public", [arrow("traefik/r1", "a"), arrow("traefik/r2", "b")])]);

test("each part crossing the edge is a pad, wired from the child holding its end", () => {
  // traefik holds routes r1, r2; the arrow host->public has parts r1->a, r2->b
  expect(padsOf(out, "traefik", F, cf, on).map((p) => [p.side, p.other, p.child])).toEqual([["out", "a", "traefik/r1"], ["out", "b", "traefik/r2"]]);
});

test("a part's end deep in the box is wired from the direct child holding it", () => {
  const v = make([arrow("host", "public", [arrow("traefik/r1", "a")])]);
  expect(padsOf(v, "host", F, cf, on).map((p) => p.child)).toEqual(["traefik"]);
});

test("a pad sits level with its child", () => {
  const t = fitInner(F, 100, 100);
  const pads = padsOf(out, "traefik", F, cf, on);
  expect(pads[0].y).toBeCloseTo(t.y + t.s * 10);
  expect(pads[1].y).toBeCloseTo(t.y + t.s * 70);
  expect(inBox(out, "traefik", F)(frames["traefik/r2"]).y).toBeCloseTo(t.y + t.s * 60);
});

test("the frames given are the ones used: a moved child takes its pad along", () => {
  const moved = (id: string) => (id === "traefik/r1" ? { ...frames[id], y: 40 } : undefined);
  const t = fitInner(F, 100, 100);
  expect(padsOf(make([arrow("host", "public", [arrow("traefik/r1", "a")])]), "traefik", F, moved, on)[0].y).toBeCloseTo(t.y + t.s * 50);
});

test("incoming parts are on the other side", () => {
  const v = make([arrow("public", "host", [arrow("a", "traefik/r1")])]);
  expect(padsOf(v, "traefik", F, cf, on).map((p) => [p.side, p.other, p.title, p.child])).toEqual([["in", "a", "A", "traefik/r1"]]);
});

test("an arrow with no parts is one pad, with no wire inside", () => {
  const v = make([arrow("host", "public")]);
  const pads = padsOf(v, "host", F, cf, on);
  expect(pads.map((p) => [p.side, p.other, p.title])).toEqual([["out", "public", "PUBLIC"]]);
  expect(pads[0].child).toBeUndefined();
  expect(pads[0].y).toBeGreaterThanOrEqual(0);
  expect(pads[0].y).toBeLessThanOrEqual(F.h);
});

test("an arrow inside the box, or elsewhere, has no pad", () => {
  const v = make([arrow("traefik/r1", "traefik/r2", undefined, "traefik"), arrow("a", "b", undefined, "public")]);
  expect(padsOf(v, "traefik", F, cf, on)).toEqual([]);
});

test("a target marked down gives a down pad, unless its layer is off", () => {
  const marks: Mark[] = [{ layer: "state", target: "a", state: "down" }, { layer: "state", target: "b", state: "ok" }];
  const v = make([arrow("host", "public", [arrow("traefik/r1", "a"), arrow("traefik/r2", "b")])], marks);
  expect(padsOf(v, "traefik", F, cf, on).map((p) => p.down)).toEqual([true, false]);
  expect(padsOf(v, "traefik", F, cf, new Set()).map((p) => p.down)).toEqual([false, false]);
});

test("a box with no connections has no pads", () => {
  expect(padsOf(make([]), "traefik", F, cf, on)).toEqual([]);
});

test("pads too close for their height spread out, in the order of their children", () => {
  const close = (id: string) => ({ x: 0, y: id === "traefik/r1" ? 41 : 40, w: 40, h: 20 });
  const pads = padsOf(out, "traefik", { w: 100, h: 200 }, close, on);
  expect(pads.map((p) => p.other)).toEqual(["b", "a"]);
  expect(pads[1].y - pads[0].y).toBeGreaterThan(20);
});

test("forty pads stay apart and inside the box", () => {
  const extra = Array.from({ length: 40 }, (_, i) => box(`t${i}`, "public"));
  const crowded = make(extra.map((b) => arrow("host", "public", [arrow("x", b.id)])), [], [...extra, box("x", "host")]);
  const ys = padsOf(crowded, "x", { w: 100, h: 200 }, cf, on).map((p) => p.y);
  expect(new Set(ys).size).toBe(40);
  expect(Math.min(...ys)).toBeGreaterThanOrEqual(0);
  expect(Math.max(...ys)).toBeLessThanOrEqual(200);
});

function drawn(n: number, onopen = (_: string) => {}) {
  const extra = Array.from({ length: n }, (_, i) => box(`t${i}`, "public"));
  const v = make(extra.map((b) => arrow("host", "public", [arrow("x", b.id)])), extra.map((b) => ({ layer: "state", target: b.id, state: "down" })), [...extra, box("x", "host")]);
  const target = document.createElement("div");
  mount(Pads, { target, props: { view: v, id: "x", frame: { w: 100, h: 200 }, childFrame: cf, on, scale: 1, onopen } });
  flushSync();
  return target;
}

test("a pad is a button that opens the far box, and a stopped one says so", () => {
  const onopen = vi.fn(), t = drawn(1, onopen), b = t.querySelector("button")!;
  expect(b.textContent).toContain("stopped");
  expect(b.getAttribute("aria-label")).toBe("Open T0, stopped");
  b.click();
  expect(onopen).toHaveBeenCalledWith("t0");
});

test("above 12 pads the labels stay and the chips go", () => {
  expect(drawn(12).querySelectorAll(".chip").length).toBe(12);
  const t = drawn(13);
  expect(t.querySelectorAll(".chip").length).toBe(0);
  expect(t.querySelectorAll(".name").length).toBe(13);
});
