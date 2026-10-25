import { expect, test } from "vitest";
import type { ViewData, ViewEdge, ViewNode } from "../src/data";
import { architecture, initialScope } from "../src/model/architecture";
import { index } from "../src/model/graph";
import fixture from "./fixture-data.json";

const D = fixture as ViewData;

test("at the root scope the groups are the layers, with counts", () => {
  const ix = index(D);
  expect(initialScope(ix)).toBe(ix.root);
  const boxes = architecture(ix, ix.root).filter((e) => e.group === "nodes" && !String(e.data.kind).startsWith("lane"));
  const count = (l: string) => new Set(D.nodes.filter((n) => n.layer === l && n.kind !== "system").map((n) => n.file)).size;
  expect(boxes.map((b) => [b.data.kind, b.data.label]).sort()).toEqual(D.layers.map((l) => ["layer", `${l} · ${count(l)}`]).sort());
  const edges = architecture(ix, ix.root).filter((e) => e.group === "edges");
  expect(edges.some((e) => e.data.violation && e.data.source === "g:/L:core" && e.data.target === "g:/L:ui")).toBe(true);
});

// The fixture plus a PostgreSQL system used by core's repo.
function withSystem(): ViewData {
  const sys: ViewNode = { id: "system:PostgreSQL", kind: "system", name: "PostgreSQL", file: "", line: 1, endLine: 1, parent: null,
    status: "unchanged", complexity: 0, loc: 0, intents: [], module: false, layer: null, lane: null, test: false,
    category: "database", packages: ["pg"] };
  const edge: ViewEdge = { from: "packages/core/src/repo.ts", to: sys.id, kind: "uses", confidence: "resolved", status: "unchanged", line: 1, violation: false };
  return { ...D, nodes: [...D.nodes, sys], edges: [...D.edges, edge] };
}

test("rows: the layers in declaration order, then External services holding the systems", () => {
  const ix = index(withSystem());
  const els = architecture(ix, ix.root);
  const lanes = els.filter((e) => e.data.kind === "lane");
  expect(lanes.map((e) => e.data.label)).toEqual([...D.layers, "External services"]);
  const y = (id: string) => els.find((e) => e.data.id === id)!.position!.y;
  D.layers.forEach((l, i) => expect(y("g:/L:" + l)).toBe(lanes[i].position!.y));
  expect(y("system:PostgreSQL")).toBe(lanes[lanes.length - 1].position!.y);
  expect(els.find((e) => e.data.id === "system:PostgreSQL")!.data.category).toBe("database");
});

test("inside one layer its children fill its row, context and systems keep theirs, and a narrow row wraps", () => {
  const ix = index(withSystem());
  const core = ix.groups["g:/L:core"];
  const wide = architecture(ix, core);
  const narrow = architecture(ix, core, 100);
  const lane = (els: typeof wide, label: string) => els.find((e) => e.data.kind === "lane" && e.data.label === label)!;
  const kids = (els: typeof wide) => els.filter((e) => e.group === "nodes" && !e.data.outer && e.data.kind !== "lane" && e.data.kind !== "lanelabel");
  expect(kids(wide).length).toBeGreaterThan(1);
  expect(new Set(kids(wide).map((e) => e.position!.y))).toEqual(new Set([lane(wide, "core").position!.y]));
  expect(new Set(kids(narrow).map((e) => e.position!.y)).size).toBe(kids(narrow).length);
  expect(lane(narrow, "core").data.h!).toBeGreaterThan(lane(wide, "core").data.h!);
  expect(lane(wide, "External services")).toBeTruthy();
  const outer = wide.filter((e) => e.data.outer && e.data.kind === "layer");
  outer.forEach((e) => expect(e.position!.y).toBe(lane(wide, e.data.id.slice("g:/L:".length)).position!.y));
});
