// The calm Architecture view on large repos: one test per rule.
import { expect, test } from "vitest";
import type { ViewData } from "../src/data";
import { architecture, assignRows, placeRows } from "../src/model/architecture";
import { index, lane, type El } from "../src/model/graph";
import { boxSize, curveOf, edgeReducer, edgeTip, frame, LAYERS, MIN_SCALE, nodeReducer, palette, toGraphology, type ViewState } from "../src/model/sigma";
import fixture from "./fixture-data.json";

const ix = index(fixture as ViewData);
const state = (o: Partial<ViewState> = {}): ViewState => ({ selected: null, hovered: null, show: null, tests: true, links: true, focus: null, ...o });
const box = (id: string, y: number, x = 0): El => ({ group: "nodes", data: { id, label: id + " · 10", kind: "layer", count: 10 }, position: { x, y } });
const sys = (id: string, x: number): El => ({ group: "nodes", data: { id, label: id, kind: "system", category: "service" }, position: { x, y: 660 } });
const edge = (id: string, source: string, target: string, count = 3): El => ({ group: "edges", data: { id, source, target, count, kind: "imports" } });

/** Three layer rows (a, b, c) and External services with three systems; each box uses some systems. */
function withSystems(): El[] {
  return [
    ...lane(0, "a", 60, 600, 120), ...lane(1, "b", 210, 600, 120), ...lane(2, "c", 360, 600, 120), ...lane(3, "External services", 660, 600, 420),
    box("A", 60), box("B", 210), box("C", 360), box("C2", 360, 200), sys("s1", 0), sys("s2", 100), sys("s3", 200),
    edge("e1", "A", "B"), edge("e2", "A", "C", 7), edge("e3", "B", "A"),
    edge("x1", "A", "s1", 2), edge("x2", "A", "s2", 5), edge("x3", "C", "s1"), edge("x4", "C2", "s3"), edge("x5", "s3", "B"),
  ];
}

test("rule 1: boxes draw above edge labels; counts only for the hovered or selected box's edges", () => {
  expect(LAYERS.over).toBe("edgeLabels"); // the boxes' canvas sits after sigma's edge labels
  const g = toGraphology(architecture(ix, ix.root));
  const id = g.findEdge((_, a) => a.from === "g:/L:app" && a.to === "g:/L:core")!;
  const a = g.getEdgeAttributes(id);
  expect(edgeReducer(state())(id, a).label).toBeUndefined();
  expect(edgeReducer(state())(id, a).forceLabel).toBe(false);
  expect(edgeReducer(state({ selected: "g:/L:app" }))(id, a)).toMatchObject({ label: "4", forceLabel: true });
  expect(edgeReducer(state({ hovered: "g:/L:core" }))(id, a)).toMatchObject({ label: "4", forceLabel: true });
  expect(edgeReducer(state({ selected: "g:/L:ui" }))(id, a).label).toBeUndefined();
  // ig_show step badges still always draw.
  expect(edgeReducer(state())(id, { ...a, step: "①" })).toMatchObject({ label: "①", forceLabel: true });
  // The tooltip names both ends without their counts.
  expect(edgeTip(a, (n) => g.getNodeAttribute(n, "label"))).toBe("app → core · 4 links");
});

test("rule 2: skip-row edges curve aside by row distance; adjacent rows stay straight; pairs keep their offset", () => {
  expect(curveOf(1)).toBe(0);
  expect(curveOf(2)).toBeGreaterThan(0);
  expect(curveOf(3)).toBeGreaterThan(curveOf(2));
  expect(curveOf(50)).toBeLessThanOrEqual(0.5);
  const g = toGraphology(architecture(ix, ix.root));
  const at = (s: string, t: string) => g.getEdgeAttributes(g.outEdges(s, t)[0]);
  expect(at("g:/L:app", "g:/L:core").curve).toBe(curveOf(2)); // app (row 2) → core (row 0)
  expect(at("g:/L:app", "g:/L:ui").curve).toBe(0);
  expect(at("g:/L:core", "g:/L:ui").offset).toBeGreaterThan(0);
  expect(at("g:/L:ui", "g:/L:core").offset).toBeGreaterThan(0);
  const r = edgeReducer(state());
  expect(r("x", at("g:/L:app", "g:/L:core")).type).toBe("curve");
  expect(r("x", at("g:/L:app", "g:/L:ui")).type).toBeUndefined();
});

test("rule 3: one edge per row to External services until a box or a system is selected", () => {
  const g = toGraphology(withSystems());
  const bands = g.filterEdges((_, a) => !!a.band);
  // Rows a and c send links out; row b only receives one (s3 → B): three band edges, one per row and direction.
  expect(bands.length).toBe(3);
  const out = (row: string) => g.getEdgeAttributes(bands.find((e) => g.getEdgeAttribute(e, "from") === "band:" + row && g.getEdgeAttribute(e, "to") === "band:ext")!);
  expect(out("a")).toMatchObject({ count: 7, systems: 2 });
  expect(out("c")).toMatchObject({ count: 6, systems: 2 }); // C → s1 and C2 → s3, one band for the row
  expect(out("a").curve).toBe(curveOf(3)); // row a skips b and c on its way to the band
  expect(out("c").curve).toBe(0); // row c sits right above it
  expect(edgeTip(out("a"), (n) => g.getNodeAttribute(n, "label"))).toBe("a → External services · 7 links to 2 systems");
  const sysEdges = g.filterEdges((_, a) => !!a.sys);
  expect(sysEdges.length).toBe(5);
  const r = edgeReducer(state());
  sysEdges.forEach((e) => expect(r(e, g.getEdgeAttributes(e)).hidden).toBe(true));
  bands.forEach((e) => expect(r(e, g.getEdgeAttributes(e)).hidden).toBeFalsy());
  // Selecting box A: its own system edges, and row a's band gives way to them.
  const selA = edgeReducer(state({ selected: "A" }));
  const x = (id: string) => g.getEdgeAttributes(g.edges().find((e) => e === id)!);
  expect(selA("x1", x("x1")).hidden).toBeFalsy();
  expect(selA("x3", x("x3")).hidden).toBe(true);
  expect(selA("b", out("a")).hidden).toBe(true);
  expect(selA("b", out("c")).hidden).toBeFalsy();
  // Selecting a system: its edges, and every band through it gives way.
  const selS = edgeReducer(state({ selected: "s1" }));
  expect(selS("x1", x("x1")).hidden).toBeFalsy();
  expect(selS("x3", x("x3")).hidden).toBeFalsy();
  expect(selS("b", out("a")).hidden).toBe(true);
  // ig_show reveals a highlighted system edge, in gold.
  expect(edgeReducer(state({ show: { nodes: new Set(), edges: new Set(["A|s2"]) } }))("x2", x("x2"))).toMatchObject({ hidden: false, color: palette.brand });
  // The band ends are invisible anchors, never boxes or labels.
  expect(nodeReducer(state())("band:ext", g.getNodeAttributes("band:ext"))).toMatchObject({ label: null, color: "rgba(0,0,0,0)" });
});

test("rule 3: systems sit in a compact grid under the band", () => {
  const nodes: El[] = Array.from({ length: 26 }, (_, i) => ({ group: "nodes", data: { id: "system:s" + i, label: "Service " + i, kind: "system" } }));
  const of = assignRows(ix, nodes);
  placeRows(nodes, of, 1200);
  const xs = new Set(nodes.map((e) => e.position!.x));
  const ys = new Set(nodes.map((e) => e.position!.y));
  expect(xs.size).toBeLessThanOrEqual(8); // ~square: 26 systems on at most 8 columns
  expect(ys.size).toBeGreaterThanOrEqual(4);
  const gap = [...ys].sort((a, b) => a - b);
  expect(gap[1] - gap[0]).toBeLessThanOrEqual(100);
});

test("rule 4: boxes fit their label with padding; fitting keeps labels at 11 px or more", () => {
  const { w, h } = boxSize({ id: "x", label: "micro-front · 1014", kind: "layer", count: 1014 });
  expect(w).toBeGreaterThanOrEqual("micro-front · 1014".length * 15 * 0.6 + 32); // 15 px semibold, 16 px each side
  expect(h).toBeGreaterThanOrEqual(40);
  expect(MIN_SCALE * 13).toBeGreaterThanOrEqual(11);
  // A tall picture: the ratio stops at the minimum scale and the top stays in view.
  const tall = frame({ x: 0, y: 0 }, { x: 400, y: 4000 }, 1000, 800, 1, MIN_SCALE);
  expect(1 / tall.ratio).toBeCloseTo(MIN_SCALE);
  expect(tall.y - (800 / 2) * tall.ratio).toBeLessThanOrEqual(0); // the view starts at the top
  // A small one is fitted (never closer than 1.4x), centred.
  const small = frame({ x: 0, y: 0 }, { x: 400, y: 300 }, 1000, 800, 1, MIN_SCALE);
  expect(small).toMatchObject({ x: 200, y: 150 });
  expect(1 / small.ratio).toBeLessThanOrEqual(1.4 + 1e-9);
  // Without a minimum (flat graphs), everything is fitted as before.
  expect(frame({ x: 0, y: 0 }, { x: 400, y: 4000 }, 1000, 800, 1).ratio).toBeCloseTo(4000 / 740);
});

test("rule 5: weight → capped width, quiet unselected edges, gold selection on top", () => {
  const els: El[] = [box("A", 60), box("B", 210), edge("e", "A", "B", 5000)];
  const g = toGraphology(els);
  const a = g.getEdgeAttributes("e");
  expect(a.size).toBeLessThanOrEqual(3);
  expect(toGraphology([box("A", 60), box("B", 210), edge("e", "A", "B", 1)]).getEdgeAttribute("e", "size")).toBe(1);
  const quiet = edgeReducer(state())("e", a);
  expect(Number(/,([\d.]+)\)$/.exec(quiet.color)![1])).toBeLessThanOrEqual(0.35);
  const sel = edgeReducer(state({ selected: "A" }))("e", a);
  expect(Number(/,([\d.]+)\)$/.exec(sel.color)?.[1] ?? 1)).toBeGreaterThanOrEqual(0.85);
  expect(sel.zIndex).toBe(2);
  const shown = edgeReducer(state({ show: { nodes: new Set(), edges: new Set(["A|B"]) } }))("e", a);
  expect(shown).toMatchObject({ color: palette.brand, zIndex: 3 });
});
