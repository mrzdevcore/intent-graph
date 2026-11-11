// @vitest-environment happy-dom
import { expect, test } from "vitest";
import { mount, flushSync } from "svelte";
import MapBox from "../src/map/MapBox.svelte";
import { look } from "../src/map/looks";
import { diffAnswer, type TableMark } from "../src/map/diff";
import type { Diff } from "../src/model/diff";
import type { Column, MapBox as Box, Table, View } from "../src/map/types";

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

const render = (rows?: TableMark, table = notes) => {
  const t = document.createElement("div");
  mount(MapBox, { target: t, props: { box, look: look(box.id, [], new Set(), box), detail: "near", onopen: () => {}, table, rows } });
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
