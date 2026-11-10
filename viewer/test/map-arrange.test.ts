// @vitest-environment happy-dom
import { expect, test, vi } from "vitest";
import { flushSync, mount } from "svelte";
import { arrange, arranged, crossings, flow } from "../src/map/place/arrange";
import { inside, type Place } from "../src/map/place/places";
import { overlaps, settle } from "../src/map/place/rules";
import { Clock, PlaceHistory, apply, changes } from "../src/map/place/history";
import { GRID } from "../src/map/nav";
import { fitInner } from "../src/map/camera";
import type { EditApi } from "../src/api";
import type { View } from "../src/map/types";
import MapHarness from "./MapHarness.svelte";
import shop from "./fixtures/shop-view.json";

// api's onion keeps its inside as the server drew it: most cases arrange the fixture without it
const view = structuredClone(shop) as unknown as View;
delete view.boxes.find((b) => b.id === "api")!.pattern;
const start = () => settle({}, view, {}, null);
const groups = ["system", "api", "web", "api-client", "api/domain-order"];
const kidsOf = (p: string) => view.boxes.filter((b) => (b.parent ?? "") === p && view.placed.frames[b.id]).map((b) => b.id);

test("arrows point left to right after arrange, but those turned round to break a loop", () => {
  const out = arrange(["system"], view, start(), GRID);
  for (const g of groups) {
    const ids = kidsOf(g);
    const links = view.arrows.filter((a) => ids.includes(a.from) && ids.includes(a.to)).map((a) => ({ from: a.from, to: a.to, weight: a.count }));
    const kept = flow(ids, links, (id) => view.boxes.find((b) => b.id === id)!.kind.value);
    for (const l of kept) expect(out[l.from].x + out[l.from].w, `${l.from} -> ${l.to}`).toBeLessThan(out[l.to].x);
  }
  expect(out.web.x).toBeLessThan(out.api.x);
  expect(out.postgres.x).toBeGreaterThan(out.api.x);
  expect(out.stripe.x).toBeGreaterThan(out.api.x);
});

test("no more crossings than before, nothing overlaps, columns on the grid, everything inside its margin", () => {
  const before = start(), out = arrange(["system"], view, before, GRID);
  for (const g of groups) {
    expect(crossings(view, out, g)).toBeLessThanOrEqual(crossings(view, before, g));
    const ids = kidsOf(g), room = out[inside(g)];
    for (const a of ids) {
      for (const b of ids) if (a !== b) expect(overlaps(out[a], out[b], GRID)).toBe(false);
      expect(out[a].x % GRID).toBe(0);
      expect(out[a].y).toBeGreaterThanOrEqual(4 * GRID);
      expect(out[a].x).toBeGreaterThanOrEqual(4 * GRID);
      expect(out[a].x + out[a].w + 4 * GRID).toBeLessThanOrEqual(room.w);
      expect(out[a].y + out[a].h + 4 * GRID).toBeLessThanOrEqual(room.h);
    }
  }
});

test("arranging twice changes nothing, and the same map gives the same result", () => {
  const once = arrange(["system"], view, start(), GRID);
  expect(arrange(["system"], view, once, GRID)).toEqual(once);
  expect(arrange(["system"], structuredClone(view), start(), GRID)).toEqual(once);
  const moved = { ...start(), web: { x: 1000, y: 900, w: 264, h: 152 } };
  expect(arrange(["system"], view, moved, GRID)).toEqual(once);
});

test("arrange inside a group leaves the rest of the map where it was", () => {
  const before = start(), out = arrange(["api"], view, before, GRID);
  for (const id of ["web", "postgres", "stripe", "api-client", "web/pages"]) expect(out[id]).toEqual(before[id]);
  for (const id of ["web", "postgres", "stripe", "api-client"]) expect(overlaps(out.api, out[id])).toBe(false);
  expect(out["api/routers"]).not.toEqual(before["api/routers"]);
  expect(arranged(["api"], view, before)).toContain("api/routers");
});

test("arrange these keeps the selection where it was and the siblings make room", () => {
  const before = start(), sel = ["web", "api-client"];
  const out = arrange(sel, view, before, GRID);
  const x0 = Math.min(before.web.x, before["api-client"].x), y0 = Math.min(before.web.y, before["api-client"].y);
  expect(Math.min(out.web.x, out["api-client"].x)).toBe(Math.round(x0 / GRID) * GRID);
  expect(Math.min(out.web.y, out["api-client"].y)).toBe(Math.round(y0 / GRID) * GRID);
  expect(out.web.x + out.web.w).toBeLessThan(out["api-client"].x);
  const ids = kidsOf("system");
  for (const a of ids) for (const b of ids) if (a !== b) expect(overlaps(out[a], out[b])).toBe(false);
});

test("one undo restores the places from before", () => {
  const before = start(), after = arrange(["system"], view, before, GRID), h = new PlaceHistory(new Clock());
  const c = changes(before, after)!;
  h.push(c.before, c.after);
  expect(apply(after, h.undo()!)).toEqual(before);
});

const press = (init: KeyboardEventInit) => {
  window.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, ...init }));
  flushSync();
};
test("⌥⇧F arranges the map, a toast counts the boxes, and its Undo puts them back", () => {
  vi.stubGlobal("localStorage", { getItem: () => null, setItem: () => {}, removeItem: () => {} });
  const same = async () => view;
  const edit = { apply: same, keep: same, drop: same, dropAll: same, undo: same, redo: same } as EditApi;
  const target = document.createElement("div");
  document.body.append(target);
  const m = mount(MapHarness, { target, props: { start: structuredClone(view), edit, size: { w: 1000, h: 800 } } });
  flushSync();
  const glide = () => (m.tick(performance.now() + 10_000), flushSync());
  const at = (id: string) => (target.querySelector(`[data-box="${CSS.escape(id)}"]`) as HTMLElement).style.left;
  const was = at("api-client");
  press({ key: "Ï", code: "KeyF", altKey: true, shiftKey: true });
  glide();
  expect(at("api-client")).not.toBe(was);
  const toast = target.querySelector(".arranged") as HTMLElement;
  expect(toast.textContent).toMatch(/Arranged \d+ boxes/);
  (toast.querySelector("button") as HTMLButtonElement).click();
  flushSync();
  glide();
  expect(at("api-client")).toBe(was);
  vi.unstubAllGlobals();
});

test("the toast's Undo leaves an ig/ edit made after the arrange alone", async () => {
  vi.stubGlobal("localStorage", { getItem: () => null, setItem: () => {}, removeItem: () => {} });
  const calls: string[] = [];
  const same = async () => view;
  const edit = { apply: async () => (calls.push("apply"), view), keep: same, drop: same, dropAll: same, undo: async () => (calls.push("undo"), view), redo: same } as EditApi;
  const target = document.createElement("div");
  document.body.append(target);
  const m = mount(MapHarness, { target, props: { start: structuredClone(view), edit, size: { w: 1000, h: 800 } } });
  flushSync();
  const at = (id: string) => (target.querySelector(`[data-box="${CSS.escape(id)}"]`) as HTMLElement).style.left;
  press({ key: "Ï", code: "KeyF", altKey: true, shiftKey: true });
  m.tick(performance.now() + 10_000);
  flushSync();
  const arranged = at("api-client");
  (target.querySelector('[data-box="stripe"] button') as HTMLButtonElement).click();
  flushSync();
  press({ key: "Delete" });
  await new Promise((r) => setTimeout(r, 0));
  flushSync();
  expect(calls).toEqual(["apply"]);
  (target.querySelector(".arranged button") as HTMLButtonElement | null)?.click();
  await new Promise((r) => setTimeout(r, 0));
  flushSync();
  m.tick(performance.now() + 20_000);
  flushSync();
  expect(calls).toEqual(["apply"]);
  expect(at("api-client")).toBe(arranged);
  vi.unstubAllGlobals();
});

test("a group becomes one block its frame shows at the same scale or larger, without empty bands", () => {
  const before = start(), out = arrange(["system"], view, before, GRID);
  for (const g of ["api", "web", "api-client", "api/domain-order"]) {
    const f = out[g], r = out[inside(g)], was = fitInner(before[g], before[inside(g)].w, before[inside(g)].h).s;
    const fit = fitInner(f, r.w, r.h);
    expect(fit.x, g).toBeCloseTo(0, 6);
    expect(fit.s, g).toBeGreaterThanOrEqual(was - 1e-9);
    expect(r.w % GRID).toBe(0);
    expect(r.h % GRID).toBe(0);
  }
});

const mini = (boxes: [string, string][], arrows: [string, string, string][]): View => {
  const f = (v: string) => ({ value: v, source: "ig" as const });
  const v: View = { version: 1, boxes: [], arrows: [], marks: [], notices: [], warnings: [], kinds: [], placed: { frames: {}, inner: { system: [800, 600] }, regions: [] } };
  v.boxes.push({ id: "system", level: "system", madeOf: [], files: 0, kind: f("system") as never, title: f("system") });
  boxes.forEach(([id, kind], i) => {
    v.boxes.push({ id, parent: "system", level: "container", madeOf: [], files: 0, kind: f(kind) as never, title: f(id) });
    v.placed.frames[id] = { x: 96 + i * 360, y: 96 + ((i * 7) % 3) * 200, w: 264, h: 152 };
  });
  v.arrows = arrows.map(([from, to, kind]) => ({ id: `${from}->${to}`, parent: "system", from, to, kind: kind as never, count: 1, kinds: [kind as never] }));
  return v;
};

test("a queue sits between who sends to it and who reads it", () => {
  const v = mini([["consumer", "service"], ["producer", "service"], ["q", "queue"]], [["producer", "q", "sends"], ["consumer", "q", "reads"]]);
  const out = arrange(["system"], v, settle({}, v, {}, null), GRID);
  expect(out.producer.x).toBeLessThan(out.q.x);
  expect(out.q.x).toBeLessThan(out.consumer.x);
});

test("people sit in the first column, stores and outside systems in the last", () => {
  const v = mini([["a-db", "store"], ["b-app", "app"], ["c-svc", "service"], ["z-user", "person"], ["y-ext", "external"]],
    [["z-user", "b-app", "uses"], ["b-app", "c-svc", "calls"], ["c-svc", "a-db", "writes"], ["b-app", "y-ext", "calls"]]);
  const out = arrange(["system"], v, settle({}, v, {}, null), GRID);
  const xs = ["a-db", "b-app", "c-svc", "z-user", "y-ext"].map((id) => out[id].x);
  expect(out["z-user"].x).toBe(Math.min(...xs));
  expect(out["a-db"].x).toBe(Math.max(...xs));
  expect(out["y-ext"].x).toBe(Math.max(...xs));
});

test("a box with a pattern keeps its inside; it still moves as a whole", () => {
  const real = shop as unknown as View, before = settle({}, real, {}, null), out = arrange(["system"], real, before, GRID);
  for (const k of Object.keys(before).filter((k) => k === "api/" || (k.startsWith("api/") && !k.startsWith("api-")))) expect(out[k], k).toEqual(before[k]);
  expect([out.api.w, out.api.h]).toEqual([before.api.w, before.api.h]);
  expect(out["api-client"]).not.toEqual(before["api-client"]);
  expect(arrange(["api"], real, before, GRID)).toEqual(before);
  expect(arrange(["system"], real, out, GRID)).toEqual(out);
});

test("the order already drawn is kept when nothing beats it", () => {
  const v = mini([["a", "service"], ["b", "service"], ["c", "service"], ["d", "service"]], [["a", "d", "calls"], ["b", "c", "calls"]]);
  Object.assign(v.placed.frames, { b: { x: 96, y: 96, w: 264, h: 152 }, a: { x: 96, y: 336, w: 264, h: 152 }, c: { x: 456, y: 96, w: 264, h: 152 }, d: { x: 456, y: 336, w: 264, h: 152 } });
  const before = settle({}, v, {}, null), out = arrange(["system"], v, before, GRID);
  expect(out.b.y).toBeLessThan(out.a.y);
  expect(out.c.y).toBeLessThan(out.d.y);
  expect(crossings(v, out, "system")).toBeLessThanOrEqual(crossings(v, before, "system"));
  expect(arrange(["system"], v, out, GRID)).toEqual(out);
});

test("a place no box has keeps its key", () => {
  const extra: Record<string, Place> = { ...start(), gone: { x: 0, y: 0, w: 10, h: 10 } };
  expect(arrange(["system"], view, extra, GRID).gone).toEqual(extra.gone);
});
