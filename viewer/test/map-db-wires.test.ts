// @vitest-environment happy-dom
import { afterEach, expect, test } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import Map from "../src/map/Map.svelte";
import { Camera } from "../src/map/glide";
import { fitTo, worldFrame } from "../src/map/camera";
import { anchorsOf, rowAt } from "../src/map/db/card";
import { wires } from "../src/map/wires";
import type { ArrowShape, Column, Frame, MapArrow, MapBox as Box, Table, View } from "../src/map/types";
import shop from "./fixtures/shop-view.json";

const f = <T,>(value: T) => ({ value, source: "ig" as const });
const col = (name: string, o: Partial<Column> = {}): Column => ({ name, type: "text", line: 1, ...o });
const table = (name: string, columns: Column[], kind: Table["kind"] = "table"): Table =>
  ({ name, kind, source: "sql", file: "db/V1.sql", range: { startLine: 1, endLine: 2 }, columns });
const points = (d: string) => [...d.matchAll(/(-?[\d.]+),(-?[\d.]+)/g)].map((m) => [Number(m[1]), Number(m[2])] as [number, number]);

const S = "postgres", G = `${S}#shop_*`, H = `${S}#misc_*`;
const id = (n: string) => `${S}#${n}`;
const orders = table("shop_orders", [col("id", { pk: true }), col("note"), col("total"), col("user_id", { references: { table: "shop_users" } }), col("created_at")]);
const users = table("shop_users", [col("name"), col("email"), col("id", { pk: true })]);
const items = table("shop_items", [col("id", { pk: true }), col("order_id", { references: { table: "shop_orders" } })]);
const report = table("shop_report", [col("total")], "view");
const log = table("misc_log", [col("id", { pk: true })]);
const misc = table("misc_notes", [col("id", { pk: true }), col("order_id", { references: { table: "shop_orders" } })]);
const tables: Record<string, Table> = { [id("shop_orders")]: orders, [id("shop_users")]: users, [id("shop_items")]: items, [id("shop_report")]: report, [id("misc_notes")]: misc, [id("misc_log")]: log };

const link = (from: string, to: string, o: Partial<MapArrow> = {}): MapArrow =>
  ({ id: `${id(from)}->${id(to)}`, parent: G, from: id(from), to: id(to), kind: "uses", count: 1, kinds: ["uses"], ...o });
const fk = link("shop_orders", "shop_users", { fromColumn: "user_id", toColumn: "id", ends: { from: "many0", to: "one" } });
const unknown = link("shop_items", "shop_orders", { fromColumn: "order_id", toColumn: "id" });
const reads = link("shop_report", "shop_orders", { kind: "reads", kinds: ["reads"] });
const logged = link("shop_report", "misc_log", { parent: S, kind: "reads", kinds: ["reads"] });
const across = link("misc_notes", "shop_orders", { parent: S, fromColumn: "order_id", toColumn: "id", ends: { from: "many0", to: "one" } });

const card: Frame = { x: 0, y: 0, w: 320, h: 300 };
const frames: Record<string, Frame> = { [id("shop_orders")]: { ...card, x: 96, y: 96 }, [id("shop_users")]: { ...card, x: 512, y: 300 } };

test("a column's row on the card: every column near, keys only in the middle, none as a chip", () => {
  expect(rowAt(orders, "user_id", 2, 300)).toEqual({ y: 56 + 26 * 3.5, h: 26 });
  expect(rowAt(users, "id", 2, 300)).toEqual({ y: 56 + 26 * 2.5, h: 26 });
  const mid = rowAt(orders, "user_id", 0.9, 300)!;
  expect(mid.y).toBeCloseTo(56 + 26 * 1.5);
  expect(rowAt(orders, "user_id", 0.6, 300)!.y).toBeCloseTo((56 + 26 * 1.5) * 1.5);
  expect(rowAt(orders, "note", 0.9, 300)).toBeUndefined();
  expect(rowAt(orders, "user_id", 0.3, 300)).toBeUndefined();
  expect(rowAt(orders, "created_at", 2, 300)).toBeUndefined();
  expect(rowAt(orders, "created_at", 2, 300, false)).toEqual({ y: 56 + 26 * 4.5, h: 26 });
  const shrunk = rowAt(orders, "user_id", 2, 120)!, k = 120 / (56 + 26 * 5 + 8);
  expect([shrunk.y, shrunk.h]).toEqual([k * (56 + 26 * 3.5), k * 26]);
  expect(rowAt(orders, "user_id", 1.5, 120)).toEqual({ y: 56 + 26 * 1.5, h: 26 });
  expect(rowAt(orders, "user_id", 1.5, 100)).toBeUndefined();
});

const ends = (shape: ArrowShape, zoom: number) => {
  const anchors = anchorsOf([fk], frames, tables, zoom);
  return { w: wires([fk], frames, "routed", {}, () => shape, anchors).wires.get(fk.id)!, anchors };
};

test.each(["circuit", "curved", "elbow", "straight"] as ArrowShape[])("%s: a table arrow leaves its FK row and lands on the PK row", (shape) => {
  const { w } = ends(shape, 2);
  const [from, to] = w.ends;
  expect(from).toEqual([96 + 320, 96 + 56 + 26 * 3.5]);
  expect(to).toEqual([512, 300 + 56 + 26 * 2.5]);
  const p = points(w.d);
  expect(p[0]).toEqual(from);
  expect(p[p.length - 1]).toEqual(to);
  expect(w.out[0][0]).toBeGreaterThan(0);
  expect(w.out[1][0]).toBeLessThan(0);
});

test.each(["circuit", "curved"] as ArrowShape[])("%s: as chips the arrow takes the card's own pins", (shape) => {
  const { w, anchors } = ends(shape, 0.3);
  expect(anchors).toEqual({});
  expect(w.ends).toEqual(wires([fk], frames, "routed", {}, () => shape).wires.get(fk.id)!.ends);
});

const stacked: Record<string, Frame> = { [id("shop_orders")]: { ...card, x: 96, y: 96 }, [id("shop_users")]: { ...card, x: 96, y: 512 } };

test.each(["circuit", "curved"] as ArrowShape[])("%s: tables in one column join their rows on the same side, outside it", (shape) => {
  const anchors = anchorsOf([fk], stacked, tables, 2);
  const w = wires([fk], stacked, "routed", {}, () => shape, anchors).wires.get(fk.id)!;
  expect(w.ends).toEqual([[416, 96 + 56 + 26 * 3.5], [416, 512 + 56 + 26 * 2.5]]);
  expect(points(w.d).slice(1, -1).every(([x]) => x > 416)).toBe(true);
});

test("circuit: the lane past one side keeps clear of a box between the two tables", () => {
  const fs = { ...stacked, wide: { x: 96, y: 420, w: 400, h: 60 } };
  const w = wires([fk], fs, "routed", {}, () => "circuit", anchorsOf([fk], fs, tables, 2)).wires.get(fk.id)!;
  expect(Math.max(...points(w.d).map(([x]) => x))).toBeGreaterThan(496);
});

const column = (x: number): Record<string, Frame> => ({ [id("shop_orders")]: { ...card, x, y: 24 }, [id("shop_users")]: { ...card, x, y: 440 }, [id("shop_items")]: { ...card, x, y: 856 } });
const inner: [number, number] = [1200, 1200];
const routedIn = (fs: Record<string, Frame>, shape: ArrowShape, arrows = [fk]) =>
  wires(arrows, fs, "routed", {}, () => shape, anchorsOf(arrows, fs, tables, 2), inner).wires;

test.each(["circuit", "curved", "elbow"] as ArrowShape[])("%s: in the rightmost column the rows join on the left, inside the parent", (shape) => {
  const fs = column(1200 - 24 - 320);
  const more = Array.from({ length: 5 }, (_, i) => ({ ...fk, id: `more ${i}` }));
  const w = routedIn(fs, shape, [fk, ...more]);
  for (const a of [fk, ...more]) {
    const p = points(w.get(a.id)!.d);
    expect(p[0][0]).toBe(856);
    expect(p.every(([x]) => x >= 0 && x <= 1200)).toBe(true);
    expect(p.slice(1, -1).every(([x]) => x < 856)).toBe(true);
  }
});

test.each(["circuit", "curved"] as ArrowShape[])("%s: in a middle column the rows still join on the right", (shape) => {
  const p = points(routedIn(column(440), shape).get(fk.id)!.d);
  expect(p[0][0]).toBe(760);
  expect(p.slice(1, -1).every(([x]) => x > 760 && x <= 760 + 48)).toBe(true);
});

test("arrows anchored on the same row share it, spread across its height", () => {
  const second = { ...fk, id: "second" };
  const anchors = anchorsOf([fk, second], frames, tables, 2);
  const w = wires([fk, second], frames, "routed", {}, () => "curved", anchors).wires;
  const [a, b] = [w.get(fk.id)!.ends[1][1], w.get("second")!.ends[1][1]];
  const row = 300 + 56 + 26 * 2.5;
  expect(a).not.toBe(b);
  for (const y of [a, b]) expect(Math.abs(y - row)).toBeLessThan(13);
});

const mounted: object[] = [];
afterEach(() => mounted.splice(0).forEach((m) => unmount(m)));

export function viewOf(extra: MapArrow[] = []) {
  const view = structuredClone(shop) as unknown as View;
  view.boxes.push(
    ...[G, H].map((g): Box => ({ id: g, parent: S, level: "component", madeOf: [], files: 1, kind: f("group"), title: f(g) })),
    ...Object.entries(tables).map(([i, t]): Box => ({ id: i, parent: i.startsWith(id("misc_")) ? H : G, level: "code", madeOf: [], files: 1, kind: f("table"), title: f(t.name) })),
  );
  view.arrows.push(fk, unknown, reads, across, logged, ...extra);
  view.tables = tables;
  view.tableGroups = { [G]: "domain", [H]: "domain" };
  Object.assign(view.placed.frames, {
    [G]: { x: 0, y: 0, w: 600, h: 500 }, [H]: { x: 700, y: 0, w: 300, h: 300 }, [id("misc_notes")]: { ...card, x: 96, y: 96 }, [id("misc_log")]: { ...card, x: 96, y: 512 },
    [id("shop_orders")]: { ...card, x: 96, y: 96 }, [id("shop_users")]: { ...card, x: 512, y: 300 },
    [id("shop_items")]: { ...card, x: 96, y: 512 }, [id("shop_report")]: { ...card, x: 512, y: 712 },
  });
  Object.assign(view.placed.inner, { [S]: [1100, 600], [G]: [1000, 1100], [H]: [512, 1000] });
  return view;
}

function mapOf(at = G, extra: MapArrow[] = []) {
  const view = viewOf(extra);
  const camera = new Camera();
  const target = document.createElement("div");
  document.body.append(target);
  const m = mount(Map, { target, props: { view, camera, size: { w: 1000, h: 800 } } });
  mounted.push(m);
  flushSync();
  camera.to(fitTo(worldFrame(at, view), { w: 1000, h: 800 }), 0);
  m.tick(800);
  flushSync();
  const arrow = (a: MapArrow) => target.querySelector(`[data-arrow="${CSS.escape(a.id)}"]`);
  const notation = (n: string) => {
    const sel = [...target.querySelectorAll("label")].find((l) => l.textContent?.startsWith("Ends"))!.querySelector("select")!;
    sel.value = n;
    sel.dispatchEvent(new Event("change", { bubbles: true }));
    flushSync();
  };
  const pick = (n: string, settle = true) => {
    (target.querySelector(`[data-box="${CSS.escape(id(n))}"] button`) as HTMLButtonElement).click();
    flushSync();
    if (settle) m.tick(performance.now() + 5000);
    flushSync();
  };
  return { target, arrow, notation, pick };
}

test("on the map, known ends are drawn at both ends of a table arrow in the chosen notation, without a head", () => {
  const { arrow, notation, target } = mapOf();
  const tips = () => [...arrow(fk)!.querySelectorAll(".tip")];
  expect(tips().map((t) => t.getAttribute("data-tip"))).toEqual(["many0", "one"]);
  expect(tips()[0].querySelector("path")).not.toBeNull();
  expect(tips()[0].querySelector("circle")!.getAttribute("r")).toBe("4");
  expect(tips()[0].querySelector("text")).toBeNull();
  expect(arrow(fk)!.querySelector("g[data-state] > path")!.getAttribute("marker-end")).toBeNull();
  notation("labels");
  expect(tips().map((t) => t.querySelector("text")?.textContent)).toEqual(["0..n", "1"]);
  expect(tips()[0].querySelector("path")).toBeNull();
  notation("both");
  expect(tips().every((t) => t.querySelector("path") && t.querySelector("text"))).toBe(true);
  target.remove();
});

test("on the map, a table arrow whose ends are unknown is a plain line", () => {
  const { arrow, target } = mapOf();
  expect(arrow(unknown)).not.toBeNull();
  expect(arrow(unknown)!.querySelectorAll(".tip").length).toBe(0);
  target.remove();
});

test("on the map, a view's reads are not drawn until the view is picked, then dotted and without ends", () => {
  const { arrow, target } = mapOf();
  expect(arrow(reads)).toBeNull();
  (target.querySelector(`[data-box="${CSS.escape(id("shop_report"))}"] button`) as HTMLButtonElement).click();
  flushSync();
  const a = arrow(reads)!;
  expect(a).not.toBeNull();
  expect(a.hasAttribute("data-read")).toBe(true);
  expect(a.querySelector("g[data-state] > path")!.getAttribute("stroke-dasharray")).toBeTruthy();
  expect(a.querySelectorAll(".tip").length).toBe(0);
  target.remove();
});

test("on the map, an arrow lifted between domains keeps the bundle's drawing, with its head and no ends", () => {
  const { arrow, target } = mapOf(S);
  const a = arrow(across)!;
  expect(a).not.toBeNull();
  expect(a.querySelectorAll(".tip").length).toBe(0);
  expect(a.querySelector("g[data-state] > path")!.getAttribute("marker-end")).toMatch(/^url\(/);
  target.remove();
});

test("on the map, a view's reads into another domain leave no pad until the view is picked, then a dotted one", () => {
  const { pick, target } = mapOf();
  const pads = () => [...target.querySelector(`[data-box="${CSS.escape(G)}"]`)!.children].find((e) => e.classList.contains("pads"));
  const pad = () => pads()?.querySelector(`[data-pad="${CSS.escape(id("misc_log"))}"]`) ?? null;
  expect(pads()).toBeDefined();
  expect(pad()).toBeNull();
  pick("shop_report", false);
  expect(pad()).not.toBeNull();
  const k = [...pads()!.querySelectorAll("[data-pad]")].indexOf(pad()!);
  expect(pads()!.querySelectorAll("svg > g")[k].querySelector("[stroke-dasharray]")).not.toBeNull();
  target.remove();
});

test("on the map, picking a view leaves Auto's choice for its level as it was", () => {
  const more = Array.from({ length: 14 }, (_, i) => link("shop_orders", "shop_users", { id: `more ${i}` }));
  const { arrow, pick, target } = mapOf(G, more);
  expect(arrow(fk)!.getAttribute("data-line")).toBe("curved");
  pick("shop_report");
  expect(arrow(reads)).not.toBeNull();
  expect(arrow(fk)!.getAttribute("data-line")).toBe("curved");
  target.remove();
});
