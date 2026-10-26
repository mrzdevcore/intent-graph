import { expect, test } from "vitest";
import type { ViewData } from "../src/data";
import { buildContext } from "../src/board/context";
import { index } from "../src/model/graph";
import fixture from "./fixture-data.json";

const ix = index(fixture as ViewData);
type Ctx = { lanes: { name: string; top: number; bottom: number }[]; boxes: { id: string; layer: string | null; y: number; h: number }[]; edges: string[][] };
const ctx = (s: Parameters<typeof buildContext>[1]): Ctx => JSON.parse(buildContext(ix, s, 1200));

test("the whole system: a lane per layer, each layer box inside its lane", () => {
  const c = ctx({ kind: "dir", id: "." });
  expect(c.lanes.map((l) => l.name)).toEqual(fixture.layers.filter((l) => c.boxes.some((b) => b.layer === l)));
  for (const b of c.boxes) {
    const lane = c.lanes.find((l) => l.name === b.layer)!;
    expect(b.y).toBeGreaterThanOrEqual(lane.top);
    expect(b.y + b.h).toBeLessThanOrEqual(lane.bottom);
  }
  expect(c.edges.length).toBeGreaterThan(0);
});

test("a layer scope shows that layer's insides; around shows modules near a node, edges between them", () => {
  const layer = ctx({ kind: "layer", name: "core" });
  expect(layer.boxes.length).toBeGreaterThan(0);
  expect(layer.boxes.some((b) => b.id === "g:/L:core")).toBe(false);
  const id = fixture.nodes.find((n) => n.module && n.layer === "core")!.id;
  const near = ctx({ kind: "around", id, depth: 1 });
  expect(near.boxes.some((b) => b.id === id)).toBe(true);
  const ids = new Set(near.boxes.map((b) => b.id));
  expect(near.edges.every(([a, b]) => ids.has(a) && ids.has(b))).toBe(true);
});
