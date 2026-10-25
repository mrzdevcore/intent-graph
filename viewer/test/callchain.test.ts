import { expect, test } from "vitest";
import type { ViewData, ViewEdge, ViewNode } from "../src/data";
import { index } from "../src/model/graph";
import { callChain, chainToMermaid, chainToPlantUml } from "../src/model/callchain";

const node = (id: string, o: Partial<ViewNode> = {}): ViewNode => {
  const file = o.file ?? id.split("#")[0];
  return {
    id, kind: "function", name: id.split("#")[1] ?? id, file, line: 1, endLine: 1, parent: null, status: "unchanged",
    complexity: 1, loc: 1, intents: [], module: !id.includes("#"), layer: null, lane: null, test: false, ...o,
  };
};
const edge = (from: string, to: string, kind = "calls", line = 1): ViewEdge => ({
  from, to, kind, confidence: "certain", status: "unchanged", line, violation: false,
});
const build = (nodes: ViewNode[], edges: ViewEdge[], lanes: string[] = []) =>
  index({ nodes, edges, findings: [], lanes, layers: [] } as unknown as ViewData);

const ix = build(
  [
    node("web/pages/page.vue", { lane: 0 }),
    node("web/pages/page.vue#title", { lane: 0 }),
    node("web/composables/catalog.ts", { lane: 0 }),
    node("web/composables/catalog.ts#useCatalog", { lane: 0 }),
    node("api/routes.ts", { lane: 1 }),
    node("api/routes.ts#GET /api/catalog", { kind: "route", lane: 1 }),
    node("api/use-cases/list.ts", { lane: 2 }),
    node("api/use-cases/list.ts#list", { lane: 2 }),
    node("api/repo.ts", { lane: 3 }),
    node("api/repo.ts#find", { lane: 3 }),
    node("system:PostgreSQL", { kind: "system", name: "PostgreSQL", file: "", module: false, lane: 4 }),
  ],
  [
    edge("web/pages/page.vue", "web/pages/page.vue#title"),
    edge("web/pages/page.vue#title", "web/composables/catalog.ts#useCatalog"),
    edge("web/composables/catalog.ts#useCatalog", "api/routes.ts#GET /api/catalog", "http"),
    edge("api/routes.ts#GET /api/catalog", "api/use-cases/list.ts#list"),
    edge("api/use-cases/list.ts#list", "api/repo.ts#find"),
    edge("api/repo.ts", "system:PostgreSQL", "uses"),
  ],
  ["Front", "API", "Use cases", "Adapters", "Systems"],
);

test("folds same-file calls and follows modules down to the database", () => {
  const c = callChain(ix, "web/pages/page.vue");
  expect(c.lifelines.map((l) => l.label)).toEqual(["User", "Front", "API", "Use cases", "Adapters", "PostgreSQL"]);
  expect(c.steps.map((s) => s.label)).toEqual(["useCatalog", "GET /api/catalog", "list", "find", "uses"]);
  expect(c.steps[0].target).toBe("web/composables/catalog.ts#useCatalog");
  expect(c.steps.some((s) => s.label === "title")).toBe(false);
});

test("terminates on cycles", () => {
  const cyc = build(
    [node("x/a.ts#a"), node("x/b.ts#b"), node("x/a.ts"), node("x/b.ts")],
    [edge("x/a.ts#a", "x/b.ts#b"), edge("x/b.ts#b", "x/a.ts#a"), edge("x/a.ts#a", "x/a.ts#a")],
  );
  expect(callChain(cyc, "x/a.ts#a").steps.length).toBeLessThanOrEqual(2);
});

test("caps fan-out and reports the rest", () => {
  const ids = Array.from({ length: 80 }, (_, i) => `f${i}.ts#fn`);
  const wide = build(
    [node("p.vue"), ...ids.map((id) => node(id))],
    ids.map((id, i) => edge("p.vue", id, "calls", i)),
  );
  const c = callChain(wide, "p.vue", { max: 60 });
  expect(c.steps.length).toBe(60);
  expect(c.truncated).toBe(20);
});

test("mermaid uses lifeline labels and step numbers", () => {
  const m = chainToMermaid(callChain(ix, "web/pages/page.vue"));
  expect(m.startsWith("sequenceDiagram")).toBe(true);
  expect(m).toContain("participant");
  expect(m).toContain("autonumber");
  expect(chainToPlantUml(callChain(ix, "web/pages/page.vue")).startsWith("@startuml")).toBe(true);
});
