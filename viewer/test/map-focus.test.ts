// @vitest-environment happy-dom
import { afterEach, expect, test } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import Map from "../src/map/Map.svelte";
import Focus from "../src/map/db/Focus.svelte";
import Tree from "../src/map/db/Tree.svelte";
import { Camera } from "../src/map/glide";
import { fitTo, worldFrame } from "../src/map/camera";
import { anchor, focusLayout, relations, treeOf, type Card } from "../src/map/db/focus";
import type { Column, End, MapArrow, MapBox, Table, View } from "../src/map/types";
import shop from "./fixtures/shop-view.json";

const f = <T,>(value: T) => ({ value, source: "ig" as const });
const col = (name: string, o: Partial<Column> = {}): Column => ({ name, type: o.references ? "uuid" : "text", line: 1, ...o });
const ref = (table: string, o: Partial<Column> = {}) => ({ references: { table }, ...o });
const table = (name: string, columns: Column[], o: Partial<Table> = {}): Table =>
  ({ name, kind: "table", source: "sql", file: "db/V1.sql", range: { startLine: 1, endLine: 2 }, columns, ...o });
const book = ["deleted", "created_at", "updated_at", "deleted_at"].map((n) => col(n));

const PARENTS = ["plants_production_lines", "parameters_batch_cycles", "parameters_products", "parameters_recipes", "plants_sites", "users"];
const CHILDREN = ["batches_alarms", "batches_events", "batches_lots", "batches_notes", "batches_reports", "batches_samples", "batches_steps", "manual_entries"];

function schema(store: string, loose = false) {
  const tables: Record<string, Table> = {};
  const parent: Record<string, string> = {};
  const arrows: MapArrow[] = [];
  const g = (name: string) => (loose ? store : `${store}#${name.split("_")[0]}_*`);
  const add = (t: Table, group = g(t.name)) => {
    tables[`${store}#${t.name}`] = t;
    parent[`${store}#${t.name}`] = group;
  };
  const link = (from: string, to: string, fromColumn: string, ends?: { from: End; to: End }, via?: string): MapArrow => ({
    id: `${store}#${from}->${store}#${to}${via ? ` via ${via}` : ""}`, parent: store, from: `${store}#${from}`, to: `${store}#${to}`, kind: "uses", count: 1, kinds: ["uses"],
    fromColumn, toColumn: "id", ends, via,
  });
  add(table("batches", [
    col("id", { pk: true }), col("code"),
    ...PARENTS.map((p) => col(`${p}_id`, ref(p, { nullable: p === "parameters_batch_cycles" }))),
    col("started_at"), col("ended_at"), col("status"), col("quantity"), col("comment"), col("parent_batch", ref("batches", { nullable: true })),
    ...book,
  ], { translations: ["label"] }));
  for (const p of PARENTS) add(table(p, [col("id", { pk: true }), col("name"), col("code"), ...(p === "plants_sites" ? [col("company_id", ref("companies"))] : [])]));
  add(table("companies", [col("id", { pk: true }), col("name")]));
  for (const c of CHILDREN)
    add(table(c, [col("id", { pk: true }), col("batch_id", ref("batches", { nullable: c === "manual_entries" })), col("value"), ...(c === "batches_notes" ? [col("impacts")] : []),
      ...(c === "batches_events" ? [col("previous_batch_id", ref("batches", { nullable: true }))] : [])]));
  add(table("batches_steps_values", [col("id", { pk: true }), col("step_id", ref("batches_steps")), col("value")]));
  add(table("batches_translations", [col("id", { pk: true }), col("batch_id", ref("batches")), col("locale"), col("label")]), loose ? store : `${store}#(translations)`);
  add(table("users_roles", [col("user_id", { pk: true, ...ref("users") }), col("role_id", { pk: true, ...ref("roles") })]));
  add(table("roles", [col("id", { pk: true }), col("name")]));

  for (const p of PARENTS) arrows.push(link("batches", p, `${p}_id`, { from: "many0", to: p === "parameters_batch_cycles" ? "zeroOne" : "one" }));
  const kids = CHILDREN.map((c) => link(c, "batches", "batch_id", { from: "many0", to: c === "manual_entries" ? "zeroOne" : "one" }));
  arrows.push({ id: "bundle", parent: store, from: `${store}#x`, to: `${store}#y`, kind: "uses", count: kids.length, kinds: ["uses"], parts: kids });
  arrows.push(link("batches_steps_values", "batches_steps", "step_id", { from: "many0", to: "one" }));
  arrows.push(link("plants_sites", "companies", "company_id", { from: "many0", to: "one" }));
  arrows.push(link("batches_translations", "batches", "batch_id", { from: "many0", to: "one" }));
  arrows.push(link("users_roles", "users", "user_id", { from: "many0", to: "one" }), link("users_roles", "roles", "role_id", { from: "many0", to: "one" }));
  arrows.push(link("users", "roles", "id", { from: "many0", to: "many0" }, "users_roles"));

  const groups = [...new Set(Object.values(parent))].filter((p) => p !== store);
  const boxes: MapBox[] = [
    ...groups.map((id) => ({ id, parent: store, level: "component" as const, madeOf: [], files: 1, kind: f("group" as const), title: f(id.split("#")[1].replace(/[()_*]/g, "")) })),
    ...Object.entries(tables).map(([id, t]) => ({ id, parent: parent[id], level: "code" as const, madeOf: [], files: 1, kind: f("table" as const), title: f(t.name) })),
  ];
  const tableGroups = Object.fromEntries(groups.map((id) => [id, id.endsWith("(translations)") ? "translations" as const : "domain" as const]));
  return { tables, arrows, boxes, tableGroups };
}

function dbView(): View {
  const s = schema("pg");
  return {
    version: 1, boxes: [{ id: "pg", level: "container", madeOf: [], files: 0, kind: f("store"), title: f("PostgreSQL") }, ...s.boxes],
    arrows: s.arrows, marks: [], placed: { frames: {}, inner: {}, regions: [] }, tables: s.tables, tableGroups: s.tableGroups,
    notices: [], warnings: [], kinds: [],
  };
}
const id = (name: string) => `pg#${name}`;
const byCol = (cards: Card[], c: number) => cards.filter((k) => k.col === c).map((k) => k.table.name);

test("relations take the arrows written inside bundles, between tables only", () => {
  const rels = relations(dbView());
  expect(rels.some((r) => r.from === id("manual_entries"))).toBe(true);
  expect(rels.some((r) => r.id === "bundle")).toBe(false);
});

test("batches in the middle: 6 parents on the right, 8 children on the left, translations and the join table folded", () => {
  const l = focusLayout(dbView(), id("batches"))!;
  expect(byCol(l.cards, 0)).toEqual(["batches"]);
  expect(byCol(l.cards, 1)).toEqual(PARENTS);
  expect(byCol(l.cards, -1)).toEqual(CHILDREN);
  expect(l.cards.some((k) => k.table.name === "batches_translations")).toBe(false);
  const centre = l.cards.find((k) => k.col === 0)!;
  expect(centre.rows.filter((r) => r.t === "col")).toHaveLength(14);
  expect(centre.rows).toContainEqual({ t: "fold", names: ["deleted", "created_at", "updated_at", "deleted_at"] });
  expect(centre.rows).toContainEqual({ t: "i18n", names: ["label"] });
  const site = l.cards.find((k) => k.table.name === "plants_sites")!;
  expect(site.rows.map((r) => (r.t === "col" ? r.col.name : r.t))).toEqual(["id", "company_id", "more"]);
});

test("each line joins the foreign-key row to the primary-key row, with the ends Rust read", () => {
  const l = focusLayout(dbView(), id("batches"))!;
  const centre = l.cards.find((k) => k.col === 0)!;
  const line = (from: string, to: string) => l.lines.find((x) => x.from === id(from) && x.to === id(to))!;
  for (const p of PARENTS) {
    const x = line("batches", p), card = l.cards.find((k) => k.id === id(p))!;
    expect(x.a).toMatchObject({ x: centre.x + centre.w, y: anchor(centre, `${p}_id`), out: [1, 0], end: "many0" });
    expect(x.b).toMatchObject({ x: card.x, y: anchor(card, "id"), out: [-1, 0], end: p === "parameters_batch_cycles" ? "zeroOne" : "one" });
  }
  const pkRow = anchor(centre, "id");
  expect(pkRow).toBe(centre.y + 44 + 13);
  for (const c of CHILDREN) {
    const x = line(c, "batches");
    expect(x.b).toMatchObject({ x: centre.x, out: [-1, 0], end: c === "manual_entries" ? "zeroOne" : "one" });
    expect(Math.abs(x.b.y - pkRow)).toBeLessThanOrEqual(10);
    expect(x.a.end).toBe("many0");
  }
  const arriving = l.lines.filter((x) => x.to === id("batches") && x.from !== x.to).map((x) => x.b.y);
  expect(arriving).toHaveLength(9);
  expect(new Set(arriving).size).toBe(9);
  const self = l.lines.find((x) => x.from === x.to)!;
  expect(self.a).toMatchObject({ y: anchor(centre, "parent_batch"), end: "many0" });
  expect(self.b).toMatchObject({ y: pkRow, end: "zeroOne" });
});

test("a child with two foreign keys to the centre draws a line from each row", () => {
  const l = focusLayout(dbView(), id("batches"))!;
  const events = l.cards.find((k) => k.id === id("batches_events"))!;
  const two = l.lines.filter((x) => x.from === id("batches_events"));
  expect(two.map((x) => x.a.y)).toEqual([anchor(events, "batch_id"), anchor(events, "previous_batch_id")]);
  expect(two.map((x) => x.b.end)).toEqual(["one", "zeroOne"]);
});

test("2 hops: the neighbours' neighbours in the outer columns", () => {
  const l = focusLayout(dbView(), id("batches"), 2)!;
  expect(byCol(l.cards, -2)).toEqual(["batches_steps_values"]);
  expect(byCol(l.cards, 2)).toEqual(["companies", "roles"]);
  const far = l.lines.find((x) => x.from === id("batches_steps_values"))!;
  const steps = l.cards.find((k) => k.id === id("batches_steps"))!;
  expect(far.b).toMatchObject({ x: steps.x, y: anchor(steps, "id") });
  const via = l.lines.find((x) => x.via === "users_roles")!;
  expect(via.a.end).toBe("many0");
  expect(via.b.end).toBe("many0");
  expect(focusLayout(dbView(), id("batches"), 1)!.cards.some((k) => k.col === 2)).toBe(false);
});

test("the join table comes back as a card when it is not folded", () => {
  const l = focusLayout(dbView(), id("users"), 1, { translations: true, joins: false, bookkeeping: true, leftovers: true, hubs: true })!;
  expect(byCol(l.cards, -1)).toContain("users_roles");
  expect(l.lines.some((x) => x.via)).toBe(false);
  const folded = focusLayout(dbView(), id("users"))!;
  expect(byCol(folded.cards, -1)).not.toContain("users_roles");
  expect(byCol(folded.cards, 1)).toEqual(["roles"]);
});

test("the tree: domains with counts, set-aside groups apart, search by a column name", () => {
  const t = treeOf(dbView(), "pg");
  expect(t.title).toBe("PostgreSQL");
  expect(t.tables).toBe(19);
  expect(t.groups.find((g) => g.title === "batches")!.count).toBe(9);
  expect(t.aside.map((g) => [g.title, g.count])).toEqual([["translations", 1]]);
  const s = treeOf(dbView(), "pg", "impacts");
  expect(s.groups.flatMap((g) => g.tables)).toEqual([{ id: id("batches_notes"), name: "batches_notes", kind: "table", matched: ["impacts"] }]);
});

function tree(focus: string | null = id("batches")) {
  const picked: string[] = [];
  const target = document.createElement("div");
  mount(Tree, { target, props: { view: dbView(), store: "pg", focus, onpick: (x: string) => picked.push(x) } });
  flushSync();
  return { target, picked };
}

test("the tree opens the focused table's domain only, and picking a table focuses it", () => {
  const { target, picked } = tree();
  expect(target.textContent).toContain("PostgreSQL · 19 tables · 0 views");
  expect(target.textContent).toContain("Set aside: translations 1");
  const open = [...target.querySelectorAll(".domain[aria-expanded=true]")].map((b) => b.querySelector(".title")!.textContent);
  expect(open).toEqual(["batches"]);
  expect(target.querySelector(`[data-table="${id("batches")}"]`)!.getAttribute("aria-current")).toBe("true");
  expect(target.querySelector(`[data-table="${id("manual_entries")}"]`)).toBeNull();
  expect(target.querySelector(`[data-table="${id("users")}"]`)).toBeNull();
  (target.querySelector(`[data-table="${id("batches_notes")}"]`) as HTMLButtonElement).click();
  expect(picked).toEqual([id("batches_notes")]);
});

test("the tree's search shows the column that matched under its table", () => {
  const { target } = tree(null);
  const input = target.querySelector("input")!;
  input.value = "impact";
  input.dispatchEvent(new Event("input"));
  flushSync();
  const hits = [...target.querySelectorAll("[data-table]")];
  expect(hits.map((h) => h.querySelector(".name")!.textContent)).toEqual(["batches_notes"]);
  expect(hits[0].querySelector(".matched")!.textContent).toBe("· impacts");
  input.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  flushSync();
  expect(input.value).toBe("");
});

const mounted: Record<string, unknown>[] = [];
afterEach(() => {
  for (const m of mounted.splice(0)) unmount(m);
  document.body.replaceChildren();
});

function focus(ends: "crowsfoot" | "labels" | "both" = "crowsfoot") {
  const calls: string[] = [];
  const target = document.createElement("div");
  document.body.append(target);
  const m = mount(Focus, { target, props: { view: dbView(), store: "pg", table: id("batches"), ends,
    onpick: (x: string) => calls.push(`pick ${x}`), onends: (n: string) => calls.push(`ends ${n}`), onclose: () => calls.push("close") } });
  mounted.push(m);
  flushSync();
  return { target, calls };
}

test("ends drawn as the notation says: crow's foot, labels or both", () => {
  const crow = focus("crowsfoot").target;
  expect(crow.querySelectorAll(".end").length).toBeGreaterThan(0);
  expect(crow.querySelectorAll(".ring").length).toBeGreaterThan(0);
  expect(crow.querySelectorAll("text.label").length).toBe(0);
  const labels = focus("labels").target;
  expect(labels.querySelectorAll(".end").length).toBe(0);
  const texts = [...labels.querySelectorAll(`[data-line="${CSS.escape("pg#manual_entries.batch_id->pg#batches")}"] text.label`)].map((t) => t.textContent);
  expect(texts).toEqual(["0..n", "0..1"]);
  const both = focus("both").target;
  expect(both.querySelectorAll(".end").length).toBeGreaterThan(0);
  expect(both.querySelectorAll("text.label").length).toBeGreaterThan(0);
});

test("the focus view: a neighbour click picks it, 2 hops adds the outer columns, the Ends buttons and Esc", () => {
  const { target, calls } = focus();
  expect(target.querySelector("section.card")!.getAttribute("aria-label")).toBe("batches");
  expect(target.querySelector("[role=dialog]")!.getAttribute("aria-modal")).toBe("true");
  expect(document.activeElement).toBe(target.querySelector(".tree input"));
  expect(target.querySelector(".crumbs")!.textContent).toContain("batches");
  expect(target.querySelectorAll("button.card")).toHaveLength(14);
  (target.querySelector(`button[data-card="${id("plants_sites")}"]`) as HTMLButtonElement).click();
  flushSync();
  expect(calls).toEqual([`pick ${id("plants_sites")}`]);
  [...target.querySelectorAll(".seg button")].find((b) => b.textContent === "2 hops")!.dispatchEvent(new MouseEvent("click", { bubbles: true }));
  flushSync();
  expect(target.querySelector(`[data-card="${id("companies")}"]`)).not.toBeNull();
  [...target.querySelectorAll(".seg button")].find((b) => b.textContent === "Labels")!.dispatchEvent(new MouseEvent("click", { bubbles: true }));
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
  expect(calls.slice(1)).toEqual(["ends labels", "close"]);
  target.remove();
});

function mapWithTables() {
  const view = structuredClone(shop) as unknown as View;
  const s = schema("postgres", true);
  const names = Object.keys(s.tables);
  view.boxes.push(...s.boxes);
  view.arrows.push(...s.arrows);
  view.tables = s.tables;
  view.tableGroups = s.tableGroups;
  names.forEach((n, i) => (view.placed.frames[n] = { x: 40 + (i % 6) * 320, y: 40 + Math.floor(i / 6) * 520, w: 280, h: 480 }));
  view.placed.inner.postgres = [1960, 2200];
  const camera = new Camera();
  const target = document.createElement("div");
  document.body.append(target);
  const m = mount(Map, { target, props: { view, camera, size: { w: 1000, h: 800 } } });
  mounted.push(m);
  flushSync();
  return { view, camera, target, m };
}

test("on the map: the store's Tables… opens the tree, Enter on a selected table focuses it, Esc returns", () => {
  const { view, camera, target, m } = mapWithTables();
  (target.querySelector(`[data-box="postgres"] button`) as HTMLButtonElement).click();
  flushSync();
  const tables = [...target.querySelectorAll(".details button")].find((b) => b.textContent === "Tables…") as HTMLButtonElement;
  tables.click();
  flushSync();
  expect(target.querySelector(".focus .tree")).not.toBeNull();
  expect(target.querySelector(".focus .empty")).not.toBeNull();
  (target.querySelector(`.focus [data-table="postgres#batches"]`) as HTMLButtonElement).click();
  flushSync();
  expect(target.querySelector(".focus section.card")!.getAttribute("aria-label")).toBe("batches");
  (target.querySelector(`.focus button[data-card="postgres#plants_sites"]`) as HTMLButtonElement).click();
  flushSync();
  expect(target.querySelector(".focus section.card")!.getAttribute("aria-label")).toBe("plants_sites");
  expect(target.querySelector(`.focus [data-table="postgres#plants_sites"]`)!.getAttribute("aria-current")).toBe("true");
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
  flushSync();
  expect(target.querySelector(".focus")).toBeNull();

  camera.to(fitTo(worldFrame("postgres#batches", view), { w: 1000, h: 800 }), 0);
  m.tick(800);
  flushSync();
  const card = target.querySelector(`[data-box="postgres#batches"]`)!;
  (card.querySelector("button.table") as HTMLButtonElement).click();
  flushSync();
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter" }));
  flushSync();
  expect(target.querySelector(".focus section.card")!.getAttribute("aria-label")).toBe("batches");
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
  flushSync();
  (target.querySelector(`[data-box="postgres#batches"] button.focus`) as HTMLButtonElement).click();
  flushSync();
  expect(target.querySelector(".focus section.card")!.getAttribute("aria-label")).toBe("batches");
  target.remove();
});
