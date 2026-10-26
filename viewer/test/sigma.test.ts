import { expect, test } from "vitest";
import type { ViewData } from "../src/data";
import { architecture } from "../src/model/architecture";
import { changedIds, exact } from "../src/model/changed";
import { index, type El } from "../src/model/graph";
import { background, edgeReducer, groupAt, nodeReducer, palette, toGraphology, type ViewState } from "../src/model/sigma";
import fixture from "./fixture-data.json";

const ix = index(fixture as ViewData);
const state = (o: Partial<ViewState> = {}): ViewState => ({ selected: null, hovered: null, show: null, tests: true, links: true, focus: null, ...o });

/** Changed mode's elements, laid out on a line so every leaf has a position. */
function changed(): El[] {
  const els = exact(ix, changedIds(ix));
  els.forEach((e, i) => e.group === "nodes" && (e.position = { x: i * 50, y: (i % 3) * 40 }));
  return els;
}

test("toGraphology keeps leaves and drops compounds", () => {
  const els = changed();
  const parents = new Set(els.map((e) => e.data.parent).filter(Boolean));
  const leaves = els.filter((e) => e.group === "nodes" && !parents.has(e.data.id));
  const ends = new Set(els.flatMap((e) => (e.group === "edges" ? [e.data.source, e.data.target] : [])));
  const g = toGraphology(els);
  expect(parents.size).toBeGreaterThan(0);
  // A compound stays only as an invisible anchor at its box's centre when an edge ends on it.
  const anchors = [...parents].filter((p) => ends.has(p));
  expect(g.order).toBe(leaves.length + anchors.length);
  expect(g.size).toBe(els.filter((e) => e.group === "edges").length);
  parents.forEach((p) => expect(g.hasNode(p) ? g.getNodeAttribute(p, "anchor") : false).toBe(ends.has(p!)));
  anchors.forEach((p) => expect(nodeReducer(state())(p!, g.getNodeAttributes(p!))).toMatchObject({ color: "rgba(0,0,0,0)", label: null }));
  const leaf = leaves[0];
  expect(g.getNodeAttributes(leaf.data.id)).toMatchObject({ x: leaf.position!.x, y: leaf.position!.y, kind: leaf.data.kind, status: leaf.data.status });
  const arch = architecture(ix, ix.root);
  const ga = toGraphology(arch);
  expect(ga.hasNode("g:/L:core")).toBe(true); // layer boxes are leaves, drawn as boxes on the background
  expect(ga.filterNodes((_, a) => String(a.kind).startsWith("lane") && a.kind !== "laneadd")).toEqual([]);
  const v = ga.findEdge((_, a) => !!a.violation);
  expect(v && ga.getEdgeAttributes(v)).toMatchObject({ violation: true, from: "g:/L:core", to: "g:/L:ui" });
});

test("background boxes enclose their children with padding, nested", () => {
  const els: El[] = [
    { group: "nodes", data: { id: "A", label: "a", kind: "openmodule", status: "changed" } },
    { group: "nodes", data: { id: "B", label: "b", kind: "openmodule", status: "unchanged", parent: "A" } },
    { group: "nodes", data: { id: "x", label: "x", kind: "member", parent: "B" }, position: { x: 0, y: 0 } },
    { group: "nodes", data: { id: "y", label: "y", kind: "member", parent: "B" }, position: { x: 100, y: 50 } },
    { group: "nodes", data: { id: "z", label: "z", kind: "member", parent: "A" }, position: { x: 300, y: 0 } },
  ];
  const bg = background(els);
  const box = (id: string) => bg.groups.find((g) => g.id === id)!;
  const r = 7; // a member's radius
  expect(box("B")).toMatchObject({ x0: -r - 24, y0: -r - 24, x1: 100 + r + 24, y1: 50 + r + 24, label: "b", status: "unchanged" });
  expect(box("A")).toMatchObject({ x0: box("B").x0 - 24, y0: box("B").y0 - 24, x1: 300 + r + 24, y1: box("B").y1 + 24, status: "changed" });
  const arch = background(architecture(ix, ix.root));
  expect(arch.lanes.map((l) => l.label)).toEqual((fixture as ViewData).layers);
  arch.lanes.forEach((l) => expect(l.y1).toBeGreaterThan(l.y0));
  expect(arch.groups.map((g) => g.id).sort()).toEqual((fixture as ViewData).layers.map((l) => "g:/L:" + l).sort());
});

test("groupAt returns the innermost group", () => {
  const bg = {
    lanes: [],
    groups: [
      { id: "outer", label: "", x0: 0, y0: 0, x1: 100, y1: 100, status: "unchanged" as const },
      { id: "inner", label: "", x0: 10, y0: 10, x1: 50, y1: 50, status: "unchanged" as const },
    ],
  };
  expect(groupAt(bg, 20, 20)).toBe("inner");
  expect(groupAt(bg, 80, 80)).toBe("outer");
  expect(groupAt(bg, 200, 20)).toBe(null);
});

test("nodeReducer: shown gold, the rest dimmed, hidden tests, the selection ring", () => {
  const base = { x: 0, y: 0, size: 7, label: "f", color: palette.unchanged, kind: "member", status: "unchanged" as const, planned: false, test: false };
  const shown = nodeReducer(state({ show: { nodes: new Set(["a"]), edges: new Set() } }));
  expect(shown("a", base)).toMatchObject({ ring: palette.brand, forceLabel: true });
  expect(shown("b", base).color).not.toBe(base.color);
  expect(shown("b", base).color).toMatch(/^rgba\(.*0\.15\)$/);
  expect(nodeReducer(state({ tests: false }))("t", { ...base, test: true }).hidden).toBe(true);
  expect(nodeReducer(state({ tests: true }))("t", { ...base, test: true }).hidden).toBeFalsy();
  const sel = nodeReducer(state({ selected: "a" }));
  expect(sel("a", base)).toMatchObject({ ring: palette.accent, highlighted: true });
  expect(sel("b", base).ring).toBeUndefined();
  expect(nodeReducer(state({ focus: new Set(["a"]) }))("b", base).color).toMatch(/0\.12\)$/);
});

test("edgeReducer: planned dashed, violations red, cross links hidden when links is off", () => {
  const e = { from: "a", to: "b", size: 1, color: palette.muted, kind: "calls", status: "unchanged" as const, planned: false, violation: false };
  const on = edgeReducer(state());
  expect(on("e", { ...e, planned: true }).dash).toBeGreaterThan(0);
  expect(on("e", e).dash).toBe(0);
  expect(on("e", { ...e, violation: true }).color).toBe(palette.error);
  const off = edgeReducer(state({ links: false }));
  expect(off("e", e).hidden).toBe(true);
  expect(edgeReducer(state({ links: false, selected: "a" }))("e", e).hidden).toBeFalsy();
  const shown = edgeReducer(state({ show: { nodes: new Set(), edges: new Set(["a|b"]) } }));
  expect(shown("e", e).color).toBe(palette.brand);
  expect(shown("f", { ...e, to: "c" }).color).toMatch(/0\.15\)$/);
});
