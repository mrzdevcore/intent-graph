// @vitest-environment happy-dom
import { expect, test } from "vitest";
import { mount, flushSync } from "svelte";
import MapBox from "../src/map/MapBox.svelte";
import { look } from "../src/map/looks";
import { diffAnswer, type TableMark } from "../src/map/diff";
import type { Diff } from "../src/model/diff";
import type { Column, MapArrow, MapBox as Box, Table, View } from "../src/map/types";
import { cardFacts, distance, type CardFacts } from "../src/map/db/card";

const f = <T,>(value: T) => ({ value, source: "ig" as const });
const col = (name: string, type: string, o: Partial<Column> = {}): Column => ({ name, type, line: 1, ...o });
const notes: Table = {
  name: "batches_notes", kind: "table", source: "sql", file: "db/V1__init.sql", range: { startLine: 1, endLine: 9 },
  columns: [
    col("id_batch_note", "uuid", { pk: true }),
    col("batch", "uuid", { references: { table: "batches" } }),
    col("impacts", "varchar[]", { nullable: true }),
  ],
};
const box: Box = { id: "postgres#batches_notes", parent: "postgres", level: "code", madeOf: [], files: 1, kind: f("table"), title: f("batches_notes") };

type Opts = { zoom?: number; facts?: CardFacts; picked?: boolean; h?: number };
const render = (rows?: TableMark, table = notes, { zoom = 2, facts, picked, h = 600 }: Opts = {}) => {
  const t = document.createElement("div");
  mount(MapBox, { target: t, props: { box, look: look(box.id, [], new Set(), box), detail: "near", onopen: () => {}, table, rows, zoom, w: 320, h, facts, picked } });
  flushSync();
  return t;
};
// a row by its column's name, without its diff sign
const row = (t: HTMLElement, name: string) =>
  [...t.querySelectorAll(".col")].find((r) => r.querySelector(".cname")?.textContent?.replace(/^[+~−]/, "") === name)!;

test("a table box is a card: its name, then a row per column with its keys and type", () => {
  const t = render();
  expect(t.querySelector("button")!.getAttribute("aria-label")).toBe("Open batches_notes");
  expect(t.querySelector(".stereo")!.textContent).toBe("«table»");
  expect([...t.querySelectorAll(".cname")].map((c) => c.textContent)).toEqual(["id_batch_note", "batch", "impacts"]);
  expect(row(t, "id_batch_note").querySelector(".pk")).not.toBeNull();
  expect(row(t, "batch").querySelector(".fk")).not.toBeNull();
  expect(row(t, "batch").getAttribute("title")).toBe("→ batches");
  expect(row(t, "impacts").querySelector(".type")!.textContent).toBe("varchar[]?");
});

test("a view has its own header", () => {
  const t = render(undefined, { ...notes, kind: "view", name: "v_impacts" });
  expect(t.querySelector(".table")!.className).toContain("view");
  expect(t.querySelector(".stereo")!.textContent).toBe("«view»");
});

test("in a diff, columns added and changed are marked and dropped ones struck through", () => {
  const t = render({ added: false, columns: new Map([["impacts", "added"], ["batch", "changed"]]), removed: [col("impact", "varchar", { references: { table: "c" } })] });
  expect(row(t, "impacts").className).toContain("added");
  expect(row(t, "batch").className).toContain("changed");
  const gone = row(t, "impact");
  expect(gone.className).toContain("removed");
  expect(gone.querySelector("s")!.textContent).toBe("impact");
});

test("diffAnswer marks a changed table's columns, a new table, and dropped ones on their store", () => {
  const view = {
    version: 1, boxes: [
      { id: "postgres", parent: "system", level: "container", madeOf: [], files: 0, kind: f("store"), title: f("PostgreSQL") },
      box,
      { ...box, id: "postgres#v_impacts", title: f("v_impacts") },
    ],
    arrows: [], marks: [], placed: { frames: {}, inner: {}, regions: [] }, notices: [], warnings: [], kinds: [],
    tables: { [box.id]: notes, "postgres#v_impacts": { ...notes, name: "v_impacts", kind: "view" } },
  } as unknown as View;
  const d: Diff = {
    base: "a".repeat(40), head: null, source: "code", summary: "",
    boxes: { added: [], removed: [], changed: [] }, arrows: { added: [], removed: [] },
    tables: {
      added: [{ name: "v_impacts", kind: "view" }],
      removed: [{ name: "old_notes", kind: "table" }],
      changed: [{ name: "BATCHES_NOTES", kind: "table", added: [col("impacts", "varchar[]")], removed: [col("impact", "varchar")],
        changed: [{ name: "batch", base: col("batch", "int4"), what: ["type"] }] }],
    },
    intent: { status: "same", added: [], removed: [], moved: [], changed: [], arrowsAdded: [], arrowsRemoved: [] },
    problems: { new: [], fixed: [] },
  };
  const { answer, marks } = diffAnswer(view, d);
  expect(marks.boxes.get("postgres#v_impacts")).toBe("added");
  expect(marks.boxes.get(box.id)).toBe("changed");
  expect(marks.chips.get(box.id)).toBe("3 columns");
  expect([...marks.tables.get(box.id)!.columns]).toEqual([["impacts", "added"], ["batch", "changed"]]);
  expect(marks.removedHere.get("postgres")!.map((b) => b.title)).toEqual(["old_notes"]);
  expect(answer.steps.filter((s) => s.group === "Tables").map((s) => [s.kind, s.title, s.meta])).toEqual([
    ["added", "v_impacts", "new view"],
    ["changed", "BATCHES_NOTES", "+ impacts, ~ batch (type), − impact"],
    ["removed", "1 table dropped", "old_notes"],
  ]);
});

test("dropped tables are marked on the store when the tables sit in its domain groups", () => {
  const store = { id: "postgres", parent: "system", level: "container", madeOf: [], files: 0, kind: f("store"), title: f("PostgreSQL") };
  const group = { ...store, id: "postgres#batches_*", parent: "postgres", level: "component", kind: f("group"), title: f("batches") };
  const view = {
    version: 1, boxes: [store, group, { ...box, parent: group.id }],
    arrows: [], marks: [], placed: { frames: {}, inner: {}, regions: [] }, notices: [], warnings: [], kinds: [],
    tables: { [box.id]: notes }, tableGroups: { [group.id]: "domain" },
  } as unknown as View;
  const d: Diff = {
    base: "a".repeat(40), head: null, source: "code", summary: "",
    boxes: { added: [], removed: [], changed: [] }, arrows: { added: [], removed: [] },
    tables: { added: [], removed: [{ name: "old_notes", kind: "table" }], changed: [] },
    intent: { status: "same", added: [], removed: [], moved: [], changed: [], arrowsAdded: [], arrowsRemoved: [] },
    problems: { new: [], fixed: [] },
  };
  expect(diffAnswer(view, d).marks.removedHere.get("postgres")!.map((b) => b.id)).toEqual(["postgres#old_notes"]);
});

test("two columns of one name, as a bad schema can give, are both drawn", () => {
  const t = render(undefined, { ...notes, columns: [col("old_location", "uuid"), col("old_location", "uuid")] });
  expect(t.querySelectorAll(".col")).toHaveLength(2);
});

const FAR = 0.3, MID = 0.8;
const big: Table = {
  ...notes, name: "batches", translations: ["name", "description"],
  columns: [
    col("id_batch", "uuid", { pk: true }),
    col("company", "uuid", { references: { table: "companies" } }),
    col("cycle", "uuid", { references: { table: "batch_cycles" }, nullable: true }),
    col("label", "text"),
    col("starts_on", "date", { nullable: true }),
    col("deleted", "bool"), col("created_at", "timestamptz"), col("updated_at", "timestamptz"), col("deleted_at", "timestamptz", { nullable: true }),
  ],
};
const names = (t: HTMLElement) => [...t.querySelectorAll(".cname")].map((c) => c.textContent);

test("the distance follows the zoom: chips for a store at fit, keys for a domain, all for one table", () => {
  expect([0.2, 0.44, 0.45, 1, 1.39, 1.4, 3].map(distance)).toEqual(["far", "far", "mid", "mid", "mid", "near", "near"]);
});

test("far, a table is a chip: its name, its column count and its tags, no row per column", () => {
  const facts = { hub: 12, pinned: new Map() };
  const rows: TableMark = { added: false, columns: new Map([["label", "added"], ["cycle", "changed"], ["starts_on", "added"]]), removed: [] };
  const t = render(rows, big, { zoom: FAR, facts });
  expect(t.querySelector(".table")!.className).toContain("far");
  expect(t.querySelector(".name")!.textContent).toBe("batches");
  expect(t.querySelector(".count")!.textContent).toBe("9 columns");
  expect([...t.querySelectorAll(".tag")].map((x) => x.textContent)).toEqual(["i18n", "←12", "+2 ~1"]);
  expect(t.querySelectorAll(".col, .cname, .keys")).toHaveLength(0);
});

test("far, a plain table has no tags", () => {
  const t = render(undefined, notes, { zoom: FAR });
  expect(t.querySelector(".tags")).toBeNull();
});

test("far on a 95-table store, the chips draw no column", () => {
  const t = document.createElement("div");
  for (let i = 0; i < 95; i++)
    mount(MapBox, { target: t, props: { box: { ...box, id: `postgres#t${i}` }, look: look(box.id, [], new Set(), box), detail: "far", onopen: () => {}, table: { ...big, name: `t${i}` }, zoom: FAR, w: 320, h: 300 } });
  flushSync();
  expect(t.querySelectorAll(".chip")).toHaveLength(95);
  expect(t.querySelectorAll(".col")).toHaveLength(0);
});

test("mid, a table shows its keys and counts the rest", () => {
  const t = render(undefined, big, { zoom: MID });
  expect(t.querySelector(".table")!.className).toContain("mid");
  expect(names(t)).toEqual(["id_batch", "company", "cycle"]);
  expect(t.querySelector(".fold")!.textContent).toBe("+2 columns · +4 bookkeeping");
  expect(t.querySelector(".i18n")).toBeNull();
});

test("mid, the keys are scaled up to read, within the near card's frame", () => {
  const t = render(undefined, big, { zoom: 0.5, h: 400 });
  const card = t.querySelector(".card") as HTMLElement;
  expect(card.style.transform).toBe("scale(1.8)");
  expect(render(undefined, big, { zoom: 0.5, h: 252 }).querySelector<HTMLElement>(".card")!.style.transform).toBe("scale(1.5)");
  expect(render(undefined, big, { zoom: 0.5, h: 150 }).querySelector<HTMLElement>(".card")!.style.transform).toBe("");
});

test("near, every column with its type, nullable marked, the bookkeeping folded into one line", () => {
  const t = render(undefined, big);
  expect(t.querySelector(".table")!.className).toContain("near");
  expect(names(t)).toEqual(["id_batch", "company", "cycle", "label", "starts_on"]);
  expect(row(t, "starts_on").querySelector(".type")!.textContent).toBe("date?");
  expect(row(t, "label").querySelector(".type")!.textContent).toBe("text");
  expect(row(t, "id_batch").querySelector(".pk")!.textContent).toBe("PK");
  expect(row(t, "company").querySelector(".fk")!.textContent).toBe("FK");
  const fold = t.querySelector(".fold.bookkeeping")!;
  expect(fold.textContent).toBe("+4 deleted, created_at, updated_at, deleted_at");
});

test("near, the translated columns sit in an i18n strip", () => {
  const t = render(undefined, big);
  expect(t.querySelector(".i18n .names")!.textContent).toBe("name, description");
  expect(render().querySelector(".i18n")).toBeNull();
});

test("a bookkeeping column changed in a diff is not folded", () => {
  const t = render({ added: false, columns: new Map([["updated_at", "changed"]]), removed: [] }, big);
  expect(names(t)).toContain("~updated_at");
  expect(t.querySelector(".fold.bookkeeping")!.textContent).toBe("+3 deleted, created_at, deleted_at");
});

test("near, a foreign key pinned to a hub names its target instead of a wire", () => {
  const facts = { hub: 0, pinned: new Map([["companies", "companies"]]) };
  const t = render(undefined, big, { facts });
  expect(row(t, "company").className).toContain("pinned");
  expect(row(t, "company").querySelector(".type")!.textContent).toBe("→ companies");
  expect(row(t, "cycle").querySelector(".type")!.textContent).toBe("uuid?");
});

test("the header is neutral, the picked table's gold", () => {
  expect(render().querySelector(".table")!.className).not.toContain("picked");
  expect(render(undefined, notes, { picked: true }).querySelector(".table")!.className).toContain("picked");
});

test("cardFacts counts the tables pointing at a hub and its pinned keys", () => {
  const arrow = (from: string, to: string, o: Partial<MapArrow> = {}) => ({ id: `${from}->${to}`, from, to, kind: "uses", count: 1, kinds: ["uses"], ...o }) as MapArrow;
  const tables: Record<string, Table> = { "p#companies": { ...notes, name: "companies" }, "p#a": notes, "p#b": notes, "p#c": notes };
  const view = {
    arrows: [
      arrow("p#a", "p#companies", { pinned: true }), arrow("p#b", "p#companies", { pinned: true }),
      arrow("p#c", "p#companies", { pinned: true, via: "p#a" }), arrow("p#c", "p#a"),
    ],
    tables,
  } as unknown as View;
  const facts = cardFacts(view);
  expect(facts.get("p#companies")!.hub).toBe(2);
  expect([...facts.get("p#a")!.pinned]).toEqual([["companies", "companies"]]);
  expect(facts.get("p#c")).toBeUndefined();
});
