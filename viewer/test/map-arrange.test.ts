// @vitest-environment happy-dom
import { expect, test, vi } from "vitest";
import { flushSync, mount } from "svelte";
import { arrange, arranged, crossings, flow } from "../src/map/place/arrange";
import { inside, type Place } from "../src/map/place/places";
import { overlaps, settle } from "../src/map/place/rules";
import { Clock, PlaceHistory, apply, changes } from "../src/map/place/history";
import { GRID } from "../src/map/nav";
import type { EditApi } from "../src/api";
import type { View } from "../src/map/types";
import MapHarness from "./MapHarness.svelte";
import shop from "./fixtures/shop-view.json";

const view = shop as unknown as View;
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

test("no more crossings than before, nothing overlaps, everything on the grid inside its margin", () => {
  const before = start(), out = arrange(["system"], view, before, GRID);
  for (const g of groups) {
    expect(crossings(view, out, g)).toBeLessThanOrEqual(crossings(view, before, g));
    const ids = kidsOf(g), room = out[inside(g)];
    for (const a of ids) {
      for (const b of ids) if (a !== b) expect(overlaps(out[a], out[b], GRID)).toBe(false);
      expect(out[a].x % GRID).toBe(0);
      expect(out[a].y % GRID).toBe(0);
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
  for (const id of ["web", "api", "postgres", "stripe", "api-client", "web/pages"]) expect(out[id]).toEqual(before[id]);
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

test("a place no box has keeps its key", () => {
  const extra: Record<string, Place> = { ...start(), gone: { x: 0, y: 0, w: 10, h: 10 } };
  expect(arrange(["system"], view, extra, GRID).gone).toEqual(extra.gone);
});
