// @vitest-environment happy-dom
import { afterEach, expect, test, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import Map from "../src/map/Map.svelte";
import MapBox from "../src/map/MapBox.svelte";
import Controls from "../src/map/Controls.svelte";
import { Camera } from "../src/map/glide";
import { fitTo, worldFrame } from "../src/map/camera";
import { look } from "../src/map/looks";
import { cardFacts } from "../src/map/db/card";
import { NOISE, applyNoise, foldNoise, loadNoise, saveNoise, type Noise } from "../src/map/db/noise";
import type { Column, MapArrow, MapBox as Box, Table, View } from "../src/map/types";
import shop from "./fixtures/shop-view.json";

const f = <T,>(value: T) => ({ value, source: "ig" as const });
const col = (name: string, o: Partial<Column> = {}): Column => ({ name, type: "text", line: 1, ...o });
const ref = (table: string, o: Partial<Column> = {}) => ({ references: { table }, ...o });
const table = (name: string, columns: Column[], o: Partial<Table> = {}): Table =>
  ({ name, kind: "table", source: "sql", file: "db/V1.sql", range: { startLine: 1, endLine: 2 }, columns, ...o });
const book = ["created_at", "deleted_at"].map((n) => col(n));

const S = "postgres";
const G = `${S}#shop_*`, LEFT = `${S}#(leftovers)`, TR = `${S}#(translations)`;
const id = (n: string) => `${S}#${n}`;

function link(from: string, to: string, o: Partial<MapArrow> = {}): MapArrow {
  return { id: `${id(from)}->${id(to)}`, parent: G, from: id(from), to: id(to), kind: "uses", count: 1, kinds: ["uses"], fromColumn: "x", toColumn: "id", ...o };
}

function dbView(): View {
  const tables: Record<string, Table> = {};
  const parent: Record<string, string> = {};
  const add = (t: Table, p = G) => ((tables[id(t.name)] = t), (parent[id(t.name)] = p));
  add(table("shop_orders", [col("id", { pk: true }), col("user_id", ref("shop_users")), ...book], { translations: ["label"] }));
  add(table("shop_users", [col("id", { pk: true }), col("name")]));
  add(table("shop_items", [col("id", { pk: true }), col("order_id", ref("shop_orders"))]));
  add(table("shop_tags", [col("id", { pk: true })]));
  add(table("shop_item_tags", [col("item_id", { pk: true, ...ref("shop_items") }), col("tag_id", { pk: true, ...ref("shop_tags") })]));
  add(table("shop_orders_translations", [col("id", { pk: true }), col("order_id", ref("shop_orders")), col("label")]), TR);
  add(table("shop_orders_backup", [col("id", { pk: true }), col("order_id", ref("shop_orders"))]), LEFT);
  add(table("shop_orders_old_temp", [col("id", { pk: true })]), LEFT);
  const boxes: Box[] = [
    { id: S, level: "container", madeOf: [], files: 0, kind: f("store"), title: f("PostgreSQL") },
    ...[G, LEFT, TR].map((g) => ({ id: g, parent: S, level: "component" as const, madeOf: [], files: 1, kind: f("group" as const), title: f(g) })),
    ...Object.entries(tables).map(([i, t]) => ({ id: i, parent: parent[i], level: "code" as const, madeOf: [], files: 1, kind: f("table" as const), title: f(t.name) })),
  ];
  const bundled = link("shop_orders_backup", "shop_orders", { parent: S });
  const arrows: MapArrow[] = [
    link("shop_orders", "shop_users", { pinned: true }),
    link("shop_items", "shop_orders"),
    link("shop_item_tags", "shop_items"),
    link("shop_item_tags", "shop_tags"),
    link("shop_items", "shop_tags", { id: `${id("shop_items")}->${id("shop_tags")} via shop_item_tags`, via: "shop_item_tags" }),
    link("shop_orders_translations", "shop_orders", { parent: S }),
    { ...bundled, from: LEFT, to: G, parts: [bundled] },
    link("shop_orders_old_temp", "shop_orders_backup", { parent: LEFT }),
  ];
  const frames = Object.fromEntries(Object.keys(tables).map((t, i) => [t, { x: 40 + (i % 4) * 320, y: 40 + Math.floor(i / 4) * 520, w: 280, h: 480 }]));
  return {
    version: 1, boxes, arrows, marks: [], placed: { frames, inner: { [S]: [1400, 1200] }, regions: [] }, tables,
    tableGroups: { [G]: "domain", [LEFT]: "leftovers", [TR]: "translations" }, notices: [], warnings: [], kinds: [],
  };
}

const ids = (v: View) => v.boxes.map((b) => b.id);
const arrowIds = (v: View) => v.arrows.map((a) => a.id);
const off = (k: keyof Noise): Noise => ({ ...NOISE, [k]: false });
const parentOf = (v: View, i: string) => v.boxes.find((b) => b.id === i)?.parent;

test("everything on: translations, leftovers set aside, hubs pinned, joins folded", () => {
  const v = applyNoise(dbView());
  expect(ids(v)).not.toContain(id("shop_orders_translations"));
  expect(ids(v)).not.toContain(TR);
  expect(v.tableGroups![TR]).toBeUndefined();
  expect(ids(v)).not.toContain(id("shop_item_tags"));
  expect(parentOf(v, id("shop_orders_backup"))).toBe(LEFT);
  expect(arrowIds(v)).not.toContain(id("shop_orders") + "->" + id("shop_users"));
});

test("a view with no tables comes back as it is", () => {
  const v = { ...dbView(), tables: undefined };
  expect(applyNoise(v)).toBe(v);
  expect(foldNoise(v)).toBe(v);
});

test("Fold translations: off, the translations group and its tables are on the map with their arrow", () => {
  const v = applyNoise(dbView(), off("translations"));
  expect(ids(v)).toContain(id("shop_orders_translations"));
  expect(v.tableGroups![TR]).toBe("translations");
  expect(parentOf(v, id("shop_orders_translations"))).toBe(TR);
  const a = v.arrows.find((x) => x.parts?.some((p) => p.from === id("shop_orders_translations")))!;
  expect(a).toMatchObject({ parent: S, from: TR, to: G });
  expect(a.parts![0].from).toBe(id("shop_orders_translations"));
  expect(v.tables![id("shop_orders")].translations).toBeUndefined();
});

test("Fold translations: on, the i18n strip stays and no arrow touches the folded tables", () => {
  const v = applyNoise(dbView());
  expect(v.tables![id("shop_orders")].translations).toEqual(["label"]);
  expect(v.arrows.flatMap((a) => (a.parts?.length ? a.parts : [a])).some((a) => a.from.includes("translations"))).toBe(false);
  expect(v.placed.frames[id("shop_orders_translations")]).toBeUndefined();
});

test("Set leftovers aside: off, they return to their domain, or to the store when it has none", () => {
  const v = applyNoise(dbView(), off("leftovers"));
  expect(ids(v)).not.toContain(LEFT);
  expect(v.tableGroups![LEFT]).toBeUndefined();
  expect(parentOf(v, id("shop_orders_backup"))).toBe(G);
  expect(v.placed.frames[id("shop_orders_backup")]).toBeUndefined();
  expect(v.arrows.find((a) => a.from === id("shop_orders_backup"))).toMatchObject({ parent: G, to: id("shop_orders") });
  expect(v.arrows.find((a) => a.from === id("shop_orders_old_temp"))).toMatchObject({ parent: G });
  const loose = dbView();
  loose.boxes = loose.boxes.filter((b) => b.id !== G);
  loose.boxes.forEach((b) => b.parent === G && (b.parent = S));
  delete loose.tableGroups![G];
  expect(parentOf(applyNoise(loose, off("leftovers")), id("shop_orders_backup"))).toBe(S);
});

test("Pin hubs: on, a pinned arrow is not drawn but the card names its target; off, it is drawn and the card is plain", () => {
  const hub = id("shop_orders") + "->" + id("shop_users");
  expect(arrowIds(applyNoise(dbView()))).not.toContain(hub);
  expect(cardFacts(foldNoise(dbView())).get(id("shop_orders"))!.pinned.get("shop_users")).toBe("shop_users");
  const v = applyNoise(dbView(), off("hubs"));
  expect(arrowIds(v)).toContain(hub);
  expect(v.arrows.find((a) => a.id === hub)!.pinned).toBe(false);
  expect(cardFacts(foldNoise(dbView(), off("hubs"))).size).toBe(0);
});

test("Fold join tables: on, the join table and its two arrows give way to the one between its tables", () => {
  const v = applyNoise(dbView());
  expect(ids(v)).not.toContain(id("shop_item_tags"));
  expect(v.tables![id("shop_item_tags")]).toBeUndefined();
  expect(v.arrows.filter((a) => a.from === id("shop_item_tags") || a.to === id("shop_item_tags"))).toEqual([]);
  expect(v.arrows.find((a) => a.via)).toMatchObject({ from: id("shop_items"), to: id("shop_tags"), via: "shop_item_tags" });
});

test("Fold join tables: off, the join table is a table with its two arrows and the shortcut goes", () => {
  const v = applyNoise(dbView(), off("joins"));
  expect(ids(v)).toContain(id("shop_item_tags"));
  expect(v.arrows.filter((a) => a.from === id("shop_item_tags")).map((a) => a.to).sort()).toEqual([id("shop_items"), id("shop_tags")]);
  expect(v.arrows.some((a) => a.via)).toBe(false);
});

test("Fold bookkeeping: a card lists every column when off", () => {
  const t = (fold: boolean) => {
    const target = document.createElement("div");
    const box = { id: id("shop_orders"), parent: G, level: "code", madeOf: [], files: 1, kind: f("table"), title: f("shop_orders") } as Box;
    mount(MapBox, { target, props: { box, look: look(box.id, [], new Set(), box), detail: "near", onopen: () => {}, table: dbView().tables![box.id], zoom: 2, w: 320, h: 600, fold } });
    flushSync();
    return [...target.querySelectorAll(".cname")].map((c) => c.textContent);
  };
  expect(t(true)).toEqual(["id", "user_id"]);
  expect(t(false)).toEqual(["id", "user_id", "created_at", "deleted_at"]);
});

test("the switches are on by default and kept per browser, only when turned off", () => {
  const kept: Record<string, string> = {};
  vi.stubGlobal("localStorage", {
    getItem: (k: string) => kept[k] ?? null, setItem: (k: string, v: string) => (kept[k] = v), removeItem: (k: string) => delete kept[k],
  });
  expect(loadNoise()).toEqual(NOISE);
  saveNoise({ ...NOISE, joins: false, hubs: false });
  expect(loadNoise()).toEqual({ ...NOISE, joins: false, hubs: false });
  saveNoise(NOISE);
  expect(Object.keys(kept)).toEqual([]);
  vi.unstubAllGlobals();
});

test("More offers the five switches, on, and reports each change", () => {
  const target = document.createElement("div");
  const seen: [string, boolean][] = [];
  mount(Controls, { target, props: {
    crumbs: [], layers: [], on: new Set<string>(), oncrumb: () => {}, ontoggle: () => {}, wheelPref: "auto", onwheel: () => {},
    noise: { ...NOISE }, onnoise: (k: string, on: boolean) => seen.push([k, on]),
  } });
  flushSync();
  const boxes = [...target.querySelectorAll("label")].filter((l) => l.querySelector("input[type=checkbox]") && !l.closest(".layers:not(.more)"));
  expect(boxes.map((l) => l.textContent!.trim())).toEqual(["Fold translations", "Set leftovers aside", "Pin hubs", "Fold join tables", "Fold bookkeeping"]);
  expect(boxes.every((l) => l.querySelector("input")!.checked)).toBe(true);
  const first = boxes[0].querySelector("input")!;
  first.checked = false;
  first.dispatchEvent(new Event("change", { bubbles: true }));
  expect(seen).toEqual([["translations", false]]);
});

const mounted: object[] = [];
afterEach(() => mounted.splice(0).forEach((m) => unmount(m)));

test("on the map, the Fold translations switch takes the translations group off and puts it back", () => {
  const view = structuredClone(shop) as unknown as View;
  const s = dbView();
  const root = view.boxes.find((b) => b.id === "postgres");
  s.boxes.forEach((b) => b.id !== S && view.boxes.push(b));
  if (!root) view.boxes.push(s.boxes[0]);
  view.arrows.push(...s.arrows);
  view.tables = s.tables;
  view.tableGroups = s.tableGroups;
  Object.assign(view.placed.frames, s.placed.frames);
  [G, LEFT, TR].forEach((g, i) => {
    view.placed.frames[g] = { x: 40 + i * 1500, y: 40, w: 1400, h: 1200 };
    view.placed.inner[g] = [1400, 1200];
  });
  view.placed.inner[S] = [4600, 1400];
  const camera = new Camera();
  const target = document.createElement("div");
  document.body.append(target);
  const m = mount(Map, { target, props: { view, camera, size: { w: 1000, h: 800 } } });
  mounted.push(m);
  flushSync();
  camera.to(fitTo(worldFrame(S, view), { w: 1000, h: 800 }), 0);
  m.tick(800);
  flushSync();
  const at = () => [...target.querySelectorAll("[data-box]")].map((e) => e.getAttribute("data-box"));
  const switchFor = (text: string) => [...target.querySelectorAll("label")].find((l) => l.textContent?.trim() === text)!.querySelector("input")!;
  expect(switchFor("Fold translations").checked).toBe(true);
  expect(at()).not.toContain(TR);
  expect(at()).toContain(G);
  const box = switchFor("Fold translations");
  box.checked = false;
  box.dispatchEvent(new Event("change", { bubbles: true }));
  flushSync();
  m.tick(1600);
  flushSync();
  expect(at()).toContain(TR);
  expect(switchFor("Fold translations").checked).toBe(false);
  target.remove();
});
