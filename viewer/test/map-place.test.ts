// @vitest-environment happy-dom
import { expect, test, vi } from "vitest";
import { flushSync, mount } from "svelte";
import { Places, inside, type Place } from "../src/map/place/places";
import { freeSpot, nudge, overlaps, rekey, reparented, settle, shownView } from "../src/map/place/rules";
import { Clock, EditLog, PlaceHistory, apply, changes, redoNext, renamesOf, undoNext } from "../src/map/place/history";
import { tween } from "../src/map/glide";
import { worldFrame } from "../src/map/camera";
import { GRID } from "../src/map/nav";
import type { MapArrow, View } from "../src/map/types";
import type { EditApi } from "../src/api";
import MapHarness from "./MapHarness.svelte";
import shop from "./fixtures/shop-view.json";

const f = (v: string) => ({ value: v, source: "ig" as const });
/** A system holding container `c` (its frame at 100,100, 400×300, inner 1200×800) and `c`'s children. */
function mapOf(kids: Record<string, Place>, arrows: [string, string][] = [], parents: Record<string, string> = {}): View {
  const view: View = { version: 1, boxes: [], arrows: [], marks: [], notices: [], warnings: [], kinds: [], placed: { frames: {}, inner: {}, regions: [] } };
  const add = (id: string, parent: string | undefined) => view.boxes.push({ id, parent, level: "component", madeOf: [], files: 0, kind: f("box") as never, title: f(id) });
  add("system", undefined);
  add("c", "system");
  view.placed.inner.system = [800, 600];
  view.placed.frames.c = { x: 100, y: 100, w: 400, h: 300 };
  view.placed.inner.c = [1200, 800];
  for (const [id, p] of Object.entries(kids)) {
    add(id, parents[id] ?? "c");
    view.placed.frames[id] = p;
  }
  view.arrows = arrows.map(([from, to]): MapArrow => ({ id: `${from}->${to}`, parent: "c", from, to, kind: "uses", count: 1, kinds: ["uses"] }));
  return view;
}
const box = (x: number, y: number, w = 200, h = 100): Place => ({ x, y, w, h });

test("a new box lands next to its arrow partner, on the grid, and nobody else moves", () => {
  const v1 = mapOf({ a: box(96, 96), b: box(816, 504) });
  const prev = settle({}, v1, {}, null);
  const v2 = mapOf({ a: box(96, 96), b: box(816, 504), n: box(96, 96) }, [["n", "b"]]);
  const next = settle(prev, v2, {}, null);
  expect(next.a).toEqual(prev.a);
  expect(next.b).toEqual(prev.b);
  const n = next.n, b = next.b;
  expect(overlaps(n, b, GRID)).toBe(false);
  expect(n.x % GRID).toBe(0);
  expect(n.y % GRID).toBe(0);
  const gap = Math.max(b.x - (n.x + n.w), n.x - (b.x + b.w), b.y - (n.y + n.h), n.y - (b.y + b.h));
  expect(gap).toBeLessThanOrEqual(2 * GRID);
  expect([n.w, n.h]).toEqual([200, 100]);
});

test("a new box with no arrows goes near its siblings' centre of mass", () => {
  const v1 = mapOf({ a: box(96, 96), b: box(600, 96) });
  const next = settle(settle({}, v1, {}, null), mapOf({ a: box(96, 96), b: box(600, 96), n: box(0, 0) }), {}, null);
  const c = next.n.x + next.n.w / 2;
  expect(c).toBeGreaterThan(296);
  expect(c).toBeLessThan(600);
});

test("a free spot never overlaps", () => {
  const placed: Record<string, Place> = {};
  for (let i = 0; i < 6; i++) for (let j = 0; j < 5; j++) placed[`k${i}-${j}`] = box(i * 230 + (j % 2) * 37, j * 130 + (i % 3) * 11, 180 + (i * 7) % 40, 90);
  for (const [w, h] of [[50, 50], [200, 100], [400, 300], [24, 600]]) {
    for (const aim of ["k0-0", "k3-2", "k5-4"]) {
      const spot = freeSpot("c", "n", { ...placed, n: box(0, 0, w, h) }, [{ from: "n", to: aim }]);
      for (const p of Object.values(placed)) expect(overlaps(spot, p, GRID)).toBe(false);
    }
  }
});

test("an overlap nudges the other the fewest pixels, and repeats outward", () => {
  const out = nudge({ h: box(0, 0, 100, 100), b: box(90, 20, 100, 100), c: box(200, 20, 100, 100), far: box(0, 400, 100, 100) }, "h", GRID);
  expect(out.b).toEqual(box(100 + GRID, 20, 100, 100));
  expect(out.c).toEqual(box(100 + GRID + 100 + GRID, 20, 100, 100));
  expect(out.far).toEqual(box(0, 400, 100, 100));
  const down = nudge({ h: box(0, 0, 100, 100), b: box(10, 90, 100, 100) }, "h", GRID);
  expect(down.b).toEqual(box(10, 100 + GRID, 100, 100));
  const vals = Object.values(out);
  for (const a of vals) for (const b of vals) if (a !== b) expect(overlaps(a, b)).toBe(false);
});

test("several held boxes stay put together; a box pushed off one never pushes the other", () => {
  const h1 = box(0, 0, 100, 100), h2 = box(0, 150, 100, 100);
  const out = nudge({ h1, h2, b: box(10, 60, 100, 60) }, ["h1", "h2"], GRID);
  expect([out.h1, out.h2]).toEqual([h1, h2]);
  const vals = Object.values(out);
  for (const a of vals) for (const b of vals) if (a !== b) expect(overlaps(a, b)).toBe(false);
});

test("a dense grid nudges until nothing overlaps, the held box still", () => {
  const placed: Record<string, Place> = {};
  for (let i = 0; i < 5; i++) for (let j = 0; j < 5; j++) placed[`k${i}${j}`] = box(i * 124, j * 124, 100, 100);
  const held = box(150, 150, 250, 250);
  const out = nudge({ ...placed, h: held }, "h", GRID);
  expect(out.h).toEqual(held);
  const ids = Object.keys(out);
  for (const a of ids) for (const b of ids) if (a < b) expect(overlaps(out[a], out[b]), `${a} ${b}`).toBe(false);
});

test("the held box never moves", () => {
  const held = box(50, 50, 300, 300);
  const out = nudge({ h: held, a: box(0, 0), b: box(100, 200), c: box(250, 250) }, "h", GRID);
  expect(out.h).toEqual(held);
  for (const id of ["a", "b", "c"]) expect(overlaps(out[id], held)).toBe(false);
  const v = mapOf({ a: box(96, 96), b: box(400, 96) });
  const prev = settle({}, v, {}, null);
  const next = settle(prev, v, { a: box(300, 96) }, "a");
  expect(next.a).toEqual(box(300, 96));
  expect(next.b.x).toBeGreaterThanOrEqual(300 + 200 + GRID);
});

test("held boxes on each other part: the first stays, the next moves the least; restored at once, they end apart", () => {
  const out = nudge({ a: box(0, 0), b: box(50, 20), c: box(400, 0) }, ["a", "b"], GRID);
  expect(out.a).toEqual(box(0, 0));
  expect(overlaps(out.a, out.b)).toBe(false);
  expect(out.b).toEqual(box(50, 100 + GRID));
  const vals = Object.values(out);
  for (const x of vals) for (const y of vals) if (x !== y) expect(overlaps(x, y)).toBe(false);
  const v = mapOf({ a: box(96, 96), b: box(600, 96), c: box(96, 400) });
  const next = settle(settle({}, v, {}, null), v, { a: box(300, 300), b: box(320, 310) }, null);
  const ps = ["a", "b", "c"].map((id) => next[id]);
  for (const x of ps) for (const y of ps) if (x !== y) expect(overlaps(x, y)).toBe(false);
});

test("a child leaving its parent grows the parent, and nothing inside moves on screen", () => {
  const v = mapOf({ a: box(96, 96), b: box(600, 400) });
  const prev = settle({}, v, {}, null);
  const world = (places: Record<string, Place>, id: string) => worldFrame(id, shownView(v, places));
  for (const to of [box(1100, 400), box(-150, -80)]) {
    const next = settle(prev, v, { b: to }, "b");
    const room = next[inside("c")];
    expect(room.x).toBeLessThanOrEqual(to.x - GRID);
    expect(room.x + room.w).toBeGreaterThanOrEqual(to.x + to.w + GRID);
    expect(room.y).toBeLessThanOrEqual(to.y - GRID);
    expect(next.c.w).toBeGreaterThan(prev.c.w);
    const [was, now] = [world(prev, "a"), world(next, "a")];
    for (const k of ["x", "y", "w", "h"] as const) expect(now[k]).toBeCloseTo(was[k], 6);
    // halfway through the glide too
    const mid = world(tween(prev, next, 150) as Record<string, Place>, "a");
    for (const k of ["x", "y", "w", "h"] as const) expect(mid[k]).toBeCloseTo(was[k], 6);
  }
});

test("a parent that grows nudges its own neighbours one level up", () => {
  const v = mapOf({ a: box(96, 96), d: box(520, 100, 100, 100) }, [], { d: "system" });
  const prev = settle({}, v, {}, null);
  const next = settle(prev, v, { a: box(1400, 96) }, "a");
  expect(next.c.w).toBeGreaterThan(prev.c.w);
  expect(overlaps(next.c, next.d)).toBe(false);
  expect(next.d).not.toEqual(prev.d);
  expect(next.a).toEqual(box(1400, 96));
});

test("delete leaves the gap; a stored size is kept", () => {
  const v1 = mapOf({ a: box(96, 96), b: box(400, 96), c2: box(700, 96) });
  const prev: Record<string, Place> = { ...settle({}, v1, {}, null), b: box(400, 96, 260, 140) };
  const v2 = mapOf({ a: box(96, 96), c2: box(400, 96) });
  const next = settle(prev, v2, {}, null);
  expect(next.c2).toEqual(prev.c2);
  expect(next.a).toEqual(prev.a);
  expect(next.b).toBeUndefined();
  expect(settle(prev, v1, {}, null).b).toEqual(box(400, 96, 260, 140));
});

test("a box moved to another parent is placed anew there", () => {
  const v1 = mapOf({ a: box(96, 96), b: box(400, 96) });
  const v2 = mapOf({ a: box(96, 96), b: box(30, 30, 50, 50) }, [], { b: "system" });
  expect(reparented(v1, v2)).toEqual(["b"]);
});

test("a rename carries the place over, its inside and its renamed children with it", () => {
  const places = { api: box(1, 1), "api/": box(0, 0, 900, 600), "api/order": box(2, 2), "api/order/": box(0, 0, 50, 50), "api/kept": box(3, 3), web: box(4, 4) };
  const has = (id: string) => ["svc", "svc/order", "api/kept", "web"].includes(id);
  expect(rekey(places, [["api", "svc"]], has)).toEqual({
    svc: box(1, 1), "svc/": box(0, 0, 900, 600), "svc/order": box(2, 2), "svc/order/": box(0, 0, 50, 50), "api/kept": box(3, 3), web: box(4, 4),
  });
  expect(renamesOf([{ op: "rename", id: "a", to: "b" }, { op: "batch", edits: [{ op: "remove", id: "x" }, { op: "rename", id: "c", to: "d" }] }])).toEqual([["a", "b"], ["c", "d"]]);
  const log = new EditLog(new Clock());
  log.push([["a", "b"], ["b", "c"]]);
  expect(log.undo().renames).toEqual([["c", "b"], ["b", "a"]]);
  expect(log.redo().renames).toEqual([["a", "b"], ["b", "c"]]);
  const v = mapOf({ svc: box(500, 500) });
  expect(settle(rekey({ ...settle({}, mapOf({ api: box(96, 96) }), {}, null) }, [["api", "svc"]], () => true), v, {}, null).svc).toEqual(box(96, 96));
});

test("old ig-moves: offsets are read once and become places", () => {
  const kept: Record<string, string> = { "ig-moves:S": JSON.stringify({ a: { x: 10, y: -5 }, gone: { x: 1, y: 1 } }) };
  const store = { getItem: (k: string) => kept[k] ?? null, setItem: (k: string, v: string) => void (kept[k] = v), removeItem: (k: string) => void delete kept[k] };
  const p = new Places("S", { a: box(100, 100) }, store);
  expect(p.get("a")).toEqual(box(110, 95));
  expect(p.get("gone")).toBeUndefined();
  expect(kept["ig-moves:S"]).toBeUndefined();
  expect(JSON.parse(kept["ig-places:S"])).toEqual({ a: box(110, 95) });
  kept["ig-moves:S"] = JSON.stringify({ a: { x: 500, y: 500 } });
  expect(new Places("S", { a: box(100, 100) }, store).get("a")).toEqual(box(110, 95));
  p.set("b", box(1, 2, 3, 4));
  p.forget(["a", "b"]);
  expect(kept["ig-places:S"]).toBeUndefined();
  const broken = { getItem: () => { throw new Error("denied"); }, setItem: () => { throw new Error("denied"); }, removeItem: () => {} };
  const q = new Places("S", {}, broken);
  q.set("a", box(0, 0));
  expect(q.snapshot()).toEqual({ a: box(0, 0) });
});

test("undo takes the later of a place change and an edit; redo the one undone last", () => {
  const clock = new Clock(), edits = new EditLog(clock), places = new PlaceHistory(clock);
  expect(undoNext(edits, places)).toBe("edit");
  edits.push();
  const a = { x: box(0, 0) }, b = { x: box(24, 0) };
  const c = changes(a, b)!;
  places.push(c.before, c.after);
  expect(undoNext(edits, places)).toBe("place");
  expect(apply(b, places.undo()!)).toEqual(a);
  expect(undoNext(edits, places)).toBe("edit");
  edits.undo();
  expect(redoNext(edits, places)).toBe("edit");
  edits.redo();
  expect(redoNext(edits, places)).toBe("place");
  expect(apply(a, places.redo()!)).toEqual(b);
  edits.push();
  expect(undoNext(edits, places)).toBe("edit");
  expect(changes(a, a)).toBeNull();
  expect(apply({ x: box(0, 0) }, { x: null })).toEqual({});
});

test("new work on the page ends both redo lines; an undo from before the page still redoes", () => {
  const clock = new Clock(), edits = new EditLog(clock), places = new PlaceHistory(clock);
  expect(redoNext(edits, places)).toBe("edit");
  edits.undo();
  expect(redoNext(edits, places)).toBe("edit");
  edits.redo();
  const c = changes({ x: box(0, 0) }, { x: box(24, 0) })!;
  places.push(c.before, c.after);
  expect(redoNext(edits, places)).toBeNull();
  edits.push();
  places.undo();
  edits.undo();
  edits.push();
  expect(redoNext(edits, places)).toBeNull();
  const failed = edits.undo();
  failed.failed();
  expect(undoNext(edits, places)).toBe("edit");
  expect(redoNext(edits, places)).toBeNull();
});

// --- in the map
const size = { w: 1000, h: 800 };
const slot = (t: HTMLElement, id: string) => t.querySelector(`[data-box="${CSS.escape(id)}"]`) as HTMLElement;
const rect = (t: HTMLElement, id: string) => {
  const s = slot(t, id).style;
  return { x: parseFloat(s.left), y: parseFloat(s.top), w: parseFloat(s.width), h: parseFloat(s.height) };
};
const press = (key: string, init: KeyboardEventInit = {}) => {
  window.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, ...init }));
  flushSync();
};
function mountMap(over: Partial<EditApi> = {}, start: View = structuredClone(shop) as unknown as View) {
  vi.stubGlobal("localStorage", { getItem: () => null, setItem: () => {}, removeItem: () => {} });
  const calls: string[] = [];
  const same = async () => start;
  const edit = { apply: same, keep: same, drop: same, dropAll: same, undo: async () => (calls.push("undo"), start), redo: async () => (calls.push("redo"), start), ...over } as EditApi;
  const target = document.createElement("div");
  document.body.append(target);
  const m = mount(MapHarness, { target, props: { start, edit, size } });
  flushSync();
  const glide = () => {
    m.tick(performance.now() + 10_000);
    flushSync();
  };
  return { target, calls, glide };
}
const later = () => new Promise((r) => setTimeout(r, 0));
const pick = (t: HTMLElement, id: string) => {
  slot(t, id).querySelector("button")!.click();
  flushSync();
};

test("arrow keys move the selected box a grid step, steps in a row undo as one, before the edit before them", async () => {
  const { target, calls, glide } = mountMap();
  const was = rect(target, "stripe");
  pick(target, "stripe");
  press("ArrowRight");
  press("ArrowRight");
  glide();
  expect(rect(target, "stripe").x).toBeCloseTo(was.x + 2 * GRID);
  press("z", { metaKey: true });
  glide();
  expect(rect(target, "stripe").x).toBeCloseTo(was.x);
  expect(calls).toEqual([]);
  press("z", { metaKey: true });
  await later();
  expect(calls).toEqual(["undo"]);
  vi.unstubAllGlobals();
});

test("Keep is not an edit Cmd+Z undoes: the place change before it is undone first", async () => {
  const start = structuredClone(shop) as unknown as View;
  start.proposals = [{ n: 1, by: "claude", summary: "Adds a cache", ids: [], marks: [] }];
  const kept = structuredClone(shop) as unknown as View;
  const { target, calls, glide } = mountMap({ keep: async () => kept }, start);
  const was = rect(target, "stripe");
  pick(target, "stripe");
  press("ArrowDown");
  ([...target.querySelectorAll("button")].find((b) => b.textContent === "Keep all") as HTMLButtonElement).click();
  await later();
  flushSync();
  press("z", { metaKey: true });
  await later();
  glide();
  expect(calls).toEqual([]);
  expect(rect(target, "stripe")).toEqual(was);
  vi.unstubAllGlobals();
});

test("a box an edit adds finds room; the boxes already drawn stay", async () => {
  const added = structuredClone(shop) as unknown as View;
  added.boxes.push({ id: "mailer", parent: "system", level: "container", madeOf: [], files: 0, kind: f("service") as never, title: f("Mailer") });
  added.arrows.push({ id: "mailer->stripe", parent: "system", from: "mailer", to: "stripe", kind: "calls", count: 1, kinds: ["calls"] });
  // ig's new layout would put it on top of api and shift the others
  added.placed.frames.mailer = { ...added.placed.frames.api };
  added.placed.frames.web = { ...added.placed.frames.web, x: added.placed.frames.web.x + 300 };
  const { target } = mountMap({ redo: async () => added });
  const before = Object.fromEntries(["api", "web", "stripe", "postgres", "api-client"].map((id) => [id, rect(target, id)]));
  press("z", { metaKey: true, shiftKey: true });
  await later();
  flushSync();
  for (const [id, r] of Object.entries(before)) expect(rect(target, id)).toEqual(r);
  const m = rect(target, "mailer");
  for (const r of Object.values(before)) expect(overlaps(m, r)).toBe(false);
  vi.unstubAllGlobals();
});

test("a deleted box that comes back takes its place again, and what moved there makes room", async () => {
  const start = structuredClone(shop) as unknown as View;
  const without = structuredClone(shop) as unknown as View;
  without.boxes = without.boxes.filter((b) => b.id !== "stripe");
  delete without.placed.frames.stripe;
  without.arrows = without.arrows.filter((a) => a.from !== "stripe" && a.to !== "stripe");
  const views = [without, start];
  const { target, glide } = mountMap({ apply: async () => views.shift() ?? start }, start);
  const stripe = rect(target, "stripe");
  pick(target, "stripe");
  press("Delete");
  await later();
  flushSync();
  expect(slot(target, "stripe")).toBeNull();
  pick(target, "postgres");
  for (let i = 0; i < 11; i++) press("ArrowDown");
  glide();
  expect(overlaps(rect(target, "postgres"), stripe)).toBe(true);
  press("Delete");
  await later();
  glide();
  expect(rect(target, "stripe")).toEqual(stripe);
  expect(overlaps(rect(target, "postgres"), stripe)).toBe(false);
  vi.unstubAllGlobals();
});

const dragBy = (t: HTMLElement, id: string, dx: number, dy = 0) => {
  const at = { bubbles: true, button: 0, clientX: 500, clientY: 400 };
  slot(t, id).querySelector("button")!.dispatchEvent(new PointerEvent("pointerdown", at));
  window.dispatchEvent(new PointerEvent("pointermove", { ...at, clientX: 500 + dx, clientY: 400 + dy }));
  window.dispatchEvent(new PointerEvent("pointerup", { ...at, clientX: 500 + dx, clientY: 400 + dy }));
  flushSync();
};

test("after a delete, its undo and a drag, redo brings nothing back", async () => {
  const start = structuredClone(shop) as unknown as View;
  const without = structuredClone(shop) as unknown as View;
  without.boxes = without.boxes.filter((b) => b.id !== "stripe");
  delete without.placed.frames.stripe;
  without.arrows = without.arrows.filter((a) => a.from !== "stripe" && a.to !== "stripe");
  const { target, calls, glide } = mountMap({ apply: async () => without }, start);
  pick(target, "stripe");
  press("Delete");
  await later();
  press("z", { metaKey: true });
  await later();
  glide();
  expect(calls).toEqual(["undo"]);
  dragBy(target, "postgres", 200);
  glide();
  const moved = rect(target, "postgres");
  press("z", { metaKey: true, shiftKey: true });
  await later();
  glide();
  expect(calls).toEqual(["undo"]);
  expect(rect(target, "postgres")).toEqual(moved);
  vi.unstubAllGlobals();
});

test("undo a drag, undo the edit before it, make a new edit: redo brings no old drag back", async () => {
  const { target, calls, glide } = mountMap();
  const was = rect(target, "postgres");
  pick(target, "stripe");
  press("Delete");
  await later();
  dragBy(target, "postgres", 200);
  glide();
  expect(rect(target, "postgres")).not.toEqual(was);
  press("z", { metaKey: true });
  glide();
  expect(rect(target, "postgres")).toEqual(was);
  press("z", { metaKey: true });
  await later();
  expect(calls).toEqual(["undo"]);
  pick(target, "stripe");
  press("Delete");
  await later();
  press("z", { metaKey: true, shiftKey: true });
  await later();
  glide();
  expect(calls).toEqual(["undo"]);
  expect(rect(target, "postgres")).toEqual(was);
  vi.unstubAllGlobals();
});

test("an edit confirmed before a drag is undone after it, even when its reply lands after the drag", async () => {
  const start = structuredClone(shop) as unknown as View;
  let land = () => {};
  const { target, calls, glide } = mountMap({ apply: () => new Promise<View>((r) => (land = () => r(start))) }, start);
  const was = rect(target, "postgres");
  pick(target, "stripe");
  press("Delete");
  dragBy(target, "postgres", 200);
  glide();
  land();
  await later();
  flushSync();
  press("z", { metaKey: true });
  await later();
  glide();
  expect(calls).toEqual([]);
  expect(rect(target, "postgres")).toEqual(was);
  press("z", { metaKey: true });
  await later();
  expect(calls).toEqual(["undo"]);
  vi.unstubAllGlobals();
});

const addPick = (t: HTMLElement, id: string) => {
  slot(t, id).querySelector("button")!.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, shiftKey: true }));
  flushSync();
};

test("dragging one box of a selection moves the selected boxes of its parent together, as one undo step", async () => {
  const { target, calls, glide } = mountMap();
  const was = { web: rect(target, "web"), client: rect(target, "api-client"), api: rect(target, "api") };
  pick(target, "web");
  addPick(target, "api-client");
  dragBy(target, "web", 200);
  glide();
  const web = rect(target, "web"), client = rect(target, "api-client");
  expect(web.x - was.web.x).toBeGreaterThan(0);
  expect(client.x - was.client.x).toBeCloseTo(web.x - was.web.x);
  expect(client.y).toBeCloseTo(was.client.y);
  expect(overlaps(web, rect(target, "api"))).toBe(false);
  expect(rect(target, "api")).not.toEqual(was.api);
  press("z", { metaKey: true });
  glide();
  expect([rect(target, "web"), rect(target, "api-client"), rect(target, "api")]).toEqual([was.web, was.client, was.api]);
  expect(calls).toEqual([]);
  vi.unstubAllGlobals();
});

test("arrow keys move the whole selection a grid step", () => {
  const { target, glide } = mountMap();
  const was = { p: rect(target, "postgres"), s: rect(target, "stripe") };
  pick(target, "postgres");
  addPick(target, "stripe");
  press("ArrowRight");
  glide();
  expect(rect(target, "postgres").x).toBeCloseTo(was.p.x + GRID);
  expect(rect(target, "stripe").x).toBeCloseTo(was.s.x + GRID);
  press("z", { metaKey: true });
  glide();
  expect([rect(target, "postgres"), rect(target, "stripe")]).toEqual([was.p, was.s]);
  vi.unstubAllGlobals();
});
