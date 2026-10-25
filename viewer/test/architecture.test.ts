import { expect, test } from "vitest";
import type { ViewData } from "../src/data";
import { architecture, initialScope } from "../src/model/architecture";
import { index } from "../src/model/graph";
import fixture from "./fixture-data.json";

const D = fixture as ViewData;

test("at the root scope the groups are the layers, with counts", () => {
  const ix = index(D);
  expect(initialScope(ix)).toBe(ix.root);
  const boxes = architecture(ix, ix.root).filter((e) => e.group === "nodes");
  const count = (l: string) => new Set(D.nodes.filter((n) => n.layer === l && n.kind !== "system").map((n) => n.file)).size;
  expect(boxes.map((b) => [b.data.kind, b.data.label]).sort()).toEqual(D.layers.map((l) => ["layer", `${l} · ${count(l)}`]).sort());
  const edges = architecture(ix, ix.root).filter((e) => e.group === "edges");
  expect(edges.some((e) => e.data.violation && e.data.source === "g:/L:core" && e.data.target === "g:/L:ui")).toBe(true);
});
