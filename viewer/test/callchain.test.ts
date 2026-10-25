import { expect, test } from "vitest";
import type { ViewData, ViewEdge, ViewNode } from "../src/data";
import { index } from "../src/model/graph";
import { callChain, chainToMermaid, chainToPlantUml, stepRows } from "../src/model/callchain";

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
    node("web/composables/catalog.ts", { lane: 1 }),
    node("web/composables/catalog.ts#useCatalog", { lane: 1 }),
    node("api/routes.ts", { lane: 2 }),
    node("api/routes.ts#GET /api/catalog", { kind: "route", lane: 2 }),
    node("api/use-cases/list.ts", { lane: 3 }),
    node("api/use-cases/list.ts#list", { lane: 3 }),
    node("api/repo.ts", { lane: 4 }),
    node("api/repo.ts#find", { lane: 4 }),
    node("system:PostgreSQL", { kind: "system", name: "PostgreSQL", file: "", module: false, lane: 5 }),
  ],
  [
    edge("web/pages/page.vue", "web/pages/page.vue#title"),
    edge("web/pages/page.vue#title", "web/composables/catalog.ts#useCatalog"),
    edge("web/composables/catalog.ts#useCatalog", "api/routes.ts#GET /api/catalog", "http"),
    edge("api/routes.ts#GET /api/catalog", "api/use-cases/list.ts#list"),
    edge("api/use-cases/list.ts#list", "api/repo.ts#find"),
    edge("api/repo.ts", "system:PostgreSQL", "uses"),
  ],
  ["User", "Front", "API", "Use cases", "Adapters", "Systems"],
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
  expect(callChain(cyc, "x/a.ts#a").steps.length).toBe(2);
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

test("starts at its own lane, with no orphan lifelines", () => {
  const c = callChain(ix, "api/use-cases/list.ts#list");
  expect(c.lifelines[0].label).toBe("Use cases");
  c.lifelines.forEach((_, i) => expect(c.steps.some((s) => s.from === i || s.to === i)).toBe(true));
});

test("a same-file helper is folded into every caller", () => {
  const h = build(
    [node("a.ts#foo"), node("a.ts#bar"), node("a.ts#h"), node("a.ts"), node("b.ts#x"), node("b.ts")],
    [edge("a.ts#foo", "a.ts#h", "calls", 1), edge("a.ts#bar", "a.ts#h", "calls", 1), edge("a.ts#h", "b.ts#x")],
  );
  expect(callChain(h, "a.ts#foo").steps.map((s) => s.label)).toEqual(["x"]);
  expect(callChain(h, "a.ts#bar").steps.map((s) => s.label)).toEqual(["x"]);
  expect(callChain(h, "a.ts").steps.map((s) => s.label)).toEqual(["x"]);
});

test("a dense same-file graph is fast and respects max", () => {
  const ids = Array.from({ length: 12 }, (_, i) => `a.ts#f${i}`);
  const dense = build(
    [node("a.ts"), ...ids.map((_, i) => node("b.ts#x" + i)), ...ids.map((id) => node(id))],
    ids.flatMap((a, i) => [...ids.filter((b) => b !== a).map((b) => edge(a, b)), edge(a, "b.ts#x" + i, "calls", 100 + i)]),
  );
  const t0 = performance.now();
  const c = callChain(dense, "a.ts", { max: 5 });
  expect(performance.now() - t0).toBeLessThan(200);
  expect(c.steps.length).toBe(5);
  expect(callChain(dense, "a.ts#f0").steps.length).toBe(12);
});

test("stepRows formats lane arrows and locations", () => {
  const rows = stepRows(callChain(ix, "web/pages/page.vue"));
  expect(rows[1]).toEqual({ n: 2, text: "Front → API  GET /api/catalog  web/composables/catalog.ts:1", target: "api/routes.ts#GET /api/catalog" });
  expect(stepRows({ lifelines: [], steps: [], truncated: 0 })).toEqual([]);
});

test("follows renders into components", () => {
  const r = build(
    [node("page.vue"), node("C.vue", { kind: "component" }), node("C.vue#setup"), node("x.ts#useX"), node("x.ts")],
    [edge("page.vue", "C.vue", "renders"), edge("C.vue#setup", "x.ts#useX")],
  );
  const c = callChain(r, "page.vue");
  expect(c.steps.map((s) => [s.label, s.kind])).toEqual([["C", "renders"], ["useX", "calls"]]);
});

test("a rendered index component is labelled by its directory", () => {
  const r = build(
    [node("page.vue"), node("components/Card/index.vue", { kind: "component" })],
    [edge("page.vue", "components/Card/index.vue", "renders")],
  );
  expect(callChain(r, "page.vue").steps.map((s) => s.label)).toEqual(["Card"]);
});

test("walks inner members of a called function", () => {
  const r = build(
    [node("page.vue"), node("a.ts#useA"), node("a.ts#useA.load", { parent: "a.ts#useA" }), node("a.ts"), node("svc.ts#Svc.list"), node("svc.ts")],
    [edge("page.vue", "a.ts#useA"), edge("a.ts#useA.load", "svc.ts#Svc.list")],
  );
  expect(callChain(r, "page.vue").steps.map((s) => s.label)).toEqual(["useA", "Svc.list"]);
});

test("module start adds uses once", () => {
  const sys = node("system:DB", { kind: "system", name: "DB", file: "", module: false });
  const r = build(
    [node("m.ts"), node("m.ts#a"), node("m.ts#b"), node("m.ts#c"), sys],
    [edge("m.ts", "system:DB", "uses")],
  );
  expect(callChain(r, "m.ts").steps.filter((s) => s.kind === "uses").length).toBe(1);
});

test("a renders cycle terminates", () => {
  const r = build(
    [node("A.vue", { kind: "component" }), node("B.vue", { kind: "component" })],
    [edge("A.vue", "B.vue", "renders"), edge("B.vue", "A.vue", "renders")],
  );
  expect(callChain(r, "A.vue").steps.map((s) => s.label)).toEqual(["B", "A"]);
});

test("a stepped-into class walks its methods, with no repeated hop", () => {
  const r = build(
    [
      node("page.vue"),
      node("repo.ts"), node("repo.ts#Repo", { kind: "class" }), node("repo.ts#Repo.find", { parent: "repo.ts#Repo" }),
      node("db.ts"), node("db.ts#Db", { kind: "class" }),
      node("system:PostgreSQL", { kind: "system", name: "PostgreSQL", file: "", module: false }),
    ],
    [
      edge("page.vue", "repo.ts#Repo", "calls", 1),
      edge("page.vue", "repo.ts#Repo.find", "calls", 2),
      edge("repo.ts#Repo.find", "db.ts#Db"),
      edge("db.ts", "system:PostgreSQL", "uses"),
    ],
  );
  const c = callChain(r, "page.vue");
  expect(c.steps.map((s) => s.label)).toEqual(["Repo", "Db", "uses", "Repo.find"]);
  const keys = c.steps.map((s) => s.from + ">" + s.target + ":" + s.file);
  expect(new Set(keys).size).toBe(keys.length);
});

test("a renders edge to a test component is skipped", () => {
  const r = build(
    [node("page.vue"), node("C.spec.vue", { kind: "component", test: true }), node("D.vue", { kind: "component" })],
    [edge("page.vue", "C.spec.vue", "renders"), edge("page.vue", "D.vue", "renders", 2)],
  );
  expect(callChain(r, "page.vue").steps.map((s) => s.label)).toEqual(["D"]);
});

test("a hop reached through several same-file helpers is shown once", () => {
  const r = build(
    [node("a.ts"), node("a.ts#foo"), node("a.ts#h1"), node("a.ts#h2"), node("b.ts"), node("b.ts#x")],
    [edge("a.ts#foo", "a.ts#h1"), edge("a.ts#foo", "a.ts#h2"), edge("a.ts#h1", "b.ts#x"), edge("a.ts#h2", "b.ts#x", "calls", 2)],
  );
  expect(callChain(r, "a.ts#foo").steps.map((s) => s.label)).toEqual(["x"]);
});
