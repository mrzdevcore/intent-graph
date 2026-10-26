import { expect, test } from "vitest";
import type { Design, Screen } from "../src/api";
import type { ViewData, ViewEdge, ViewNode } from "../src/data";
import { refitter } from "../src/board/host";
import { Bridges, bridgeChain, designIndex, pickerItems, requirement, withDesign, type Board } from "../src/board/bridge";

const node = (id: string, o: Partial<ViewNode> = {}): ViewNode => {
  const file = o.file ?? id.split("#")[0];
  return {
    id, kind: "function", name: id.split("#")[1] ?? id, file, line: 1, endLine: 1, parent: null, status: "unchanged",
    complexity: 1, loc: 1, intents: [], module: !id.includes("#"), layer: null, lane: null, test: false, ...o,
  };
};
const edge = (from: string, to: string, kind = "calls"): ViewEdge => ({
  from, to, kind, confidence: "certain", status: "unchanged", line: 1, violation: false,
});

const D = {
  nodes: [
    node("web/pages/patients.vue", { layer: "web", lane: 0 }),
    node("api/routes/patients.ts", { layer: "api", lane: 1 }),
    node("api/routes/patients.ts#GET /api/patients", { kind: "route", layer: "api", lane: 1 }),
    node("api/use-cases/list.ts#listPatients", { layer: "use-cases", lane: 2 }),
    node("api/repo.ts", { layer: "adapters", lane: 3 }),
    node("api/repo.ts#PatientRepo.findAll", { layer: "adapters", lane: 3 }),
    node("api/export.test.ts#exportsAll", { test: true }),
    node("system:PostgreSQL", { kind: "system", name: "PostgreSQL", file: "", module: false, lane: 4 }),
  ],
  edges: [edge("api/repo.ts", "system:PostgreSQL", "uses")],
  findings: [], lanes: ["Web", "API", "Use cases", "Adapters", "Systems"], layers: [],
} as unknown as ViewData;

const design: Design = {
  version: 1, title: "Export", scope: { kind: "dir", id: "." },
  elements: [
    { id: "export-route", kind: "route", name: "export", method: "GET", path: "/api/patients/export", file: "api/routes/patients.ts", layer: "api" },
    { id: "export-patients", kind: "function", name: "exportPatients", file: "api/use-cases/export.ts", layer: "use-cases" },
    { id: "note-stream", kind: "note", name: "stream CSV, don't buffer", on: "export-route" },
  ],
  marks: [{ node: "api/repo.ts#PatientRepo.findAll", mark: "changed" }],
  links: [
    { from: "export-route", to: "export-patients", kind: "calls" },
    { from: "export-patients", to: "api/repo.ts#PatientRepo.findAll", kind: "calls" },
  ],
  bridges: [
    { from: "patients/export-csv", to: "export-route" },
    { from: "patients/title", to: "api/gone.ts#archive" },
  ],
};
const screen = (id: string, title: string, o: Partial<Screen> = {}): Screen =>
  ({ version: 1, id, title, x: 0, y: 0, w: 400, h: 300, elements: [], navs: [], ...o });
const b: Board = {
  design,
  screens: {
    patients: screen("patients", "Patient list", {
      elements: [
        { id: "export-csv", kind: "button", label: "Export CSV", x: 8, y: 8, w: 120, h: 40 },
        { id: "title", kind: "heading", label: "Patients", x: 8, y: 60, w: 120, h: 40 },
      ],
      navs: [{ from: "export-csv", to: "export-options", label: "" }],
    }),
    "export-options": screen("export-options", "Export options"),
    "export-options-error": screen("export-options-error", "Export options", { variantOf: "export-options", state: "error" }),
    "export-options-loading": screen("export-options-loading", "Export options", { variantOf: "export-options", state: "loading" }),
  },
};

test("withDesign adds planned nodes and edges", () => {
  const W = withDesign(D, design);
  const route = W.nodes.find((n) => n.id === "export-route")!;
  expect(route).toMatchObject({ planned: true, kind: "route", name: "GET /api/patients/export", layer: "api", lane: 1 });
  expect(W.nodes.some((n) => n.id === "note-stream")).toBe(false);
  expect(W.edges.filter((e) => e.planned).map((e) => [e.from, e.to])).toEqual([
    ["export-route", "export-patients"], ["export-patients", "api/repo.ts#PatientRepo.findAll"],
  ]);
  expect(W.nodes.find((n) => n.id === "api/repo.ts#PatientRepo.findAll")!.status).toBe("changed");
  expect(D.nodes.find((n) => n.id === "api/repo.ts#PatientRepo.findAll")!.status).toBe("unchanged");
});

test("bridgeChain follows button to route to use case to repository to system", () => {
  const c = bridgeChain(designIndex(D, design), design, "patients/export-csv", "Patient list")!;
  expect(c.lifelines.map((l) => l.label)).toEqual(["Patient list", "API", "Use cases", "Adapters", "PostgreSQL"]);
  expect(c.steps.map((s) => [s.n, s.label, !!s.planned, !!s.changed])).toEqual([
    [1, "GET /api/patients/export", true, false],
    [2, "exportPatients", true, false],
    [3, "PatientRepo.findAll", true, true],
    [4, "uses", false, false],
  ]);
  expect(c.steps.map((s) => [s.from, s.to])).toEqual([[0, 1], [1, 2], [2, 3], [3, 4]]);
});

test("bridgeChain returns null for an unresolved target", () => {
  const ix = designIndex(D, design);
  expect(bridgeChain(ix, design, "patients/title", "Patient list")).toBeNull();
  expect(bridgeChain(ix, design, "patients/nothing", "Patient list")).toBeNull();
});

test("requirement sentence", () => {
  expect(requirement(b, "patients/export-csv")).toBe(
    "The Export CSV button on Patient list triggers GET /api/patients/export. Its Export options screen has loading and error states. Note: stream CSV, don't buffer.",
  );
  expect(requirement(b, "patients/title")).toBe("The Patients heading on Patient list triggers archive in api/gone.ts.");
});

test("pickerItems filters and lists design first", () => {
  expect(pickerItems(D, b, "patients").map((i) => i.id)).toEqual([
    "export-route", "export-patients",
    "web/pages/patients.vue", "api/routes/patients.ts", "api/routes/patients.ts#GET /api/patients", "api/use-cases/list.ts#listPatients",
  ]);
  expect(pickerItems(D, b, "")[0]).toEqual({ id: "export-route", label: "GET /api/patients/export", kind: "route" });
  expect(pickerItems(D, b, "export").some((i) => i.id === "api/export.test.ts#exportsAll")).toBe(false);
  expect(pickerItems(D, b, "").length).toBeLessThanOrEqual(20);
  expect(pickerItems(D, b, "patient")[0].id).toBe("api/repo.ts#PatientRepo.findAll");
});

test("an element whose code exists is its real node: the chain goes on into real calls", () => {
  const d: Design = { ...design, elements: design.elements.map((e) => e.id === "export-route"
    ? { ...e, method: "GET", path: "/api/patients", file: "api/routes/patients.ts" } : e) };
  const real = { ...D, edges: [...D.edges, edge("api/routes/patients.ts#GET /api/patients", "api/use-cases/list.ts#listPatients")] } as ViewData;
  expect(withDesign(real, d).nodes.some((n) => n.id === "export-route")).toBe(false);
  const listed = pickerItems(real, { design: d, screens: {} }, "patients").filter((i) => i.id === "api/routes/patients.ts#GET /api/patients");
  expect(listed).toEqual([{ id: "api/routes/patients.ts#GET /api/patients", label: "GET /api/patients", kind: "route" }]);
  expect(pickerItems(real, { design: d, screens: {} }, "patients").some((i) => i.id === "export-route")).toBe(false);
  const c = bridgeChain(designIndex(real, d), d, "patients/export-csv", "Patient list")!;
  expect(c.steps.map((s) => [s.label, !!s.planned])).toEqual([
    ["GET /api/patients", false], ["exportPatients", true], ["PatientRepo.findAll", true], ["uses", false], ["listPatients", false],
  ]);
});

test("a chain is not computed again for an edit that does not change its meaning", () => {
  const br = new Bridges();
  const c = br.chain(D, design, "patients/export-csv", "Patient list");
  const moved: Design = { ...design, elements: design.elements.map((e) => ({ ...e, x: 99, y: 99 })) };
  expect(br.chain(D, moved, "patients/export-csv", "Patient list")).toBe(c);
  expect(br.chain(D, moved, "patients/title", "Patient list")).toBeNull();
  expect(br.builds).toBe(1);
  const renamed: Design = { ...moved, elements: moved.elements.map((e) => (e.id === "export-patients" ? { ...e, name: "exportAll" } : e)) };
  expect(br.chain(D, renamed, "patients/export-csv", "Patient list")!.steps[1].label).toBe("exportAll");
  expect(br.builds).toBe(2);
});

test("a wanted fit waits for the size to change", () => {
  const r = refitter();
  expect(r.sized(800, 600)).toBe(false);
  r.want();
  expect(r.sized(800, 600)).toBe(false);
  expect(r.sized(400, 600)).toBe(true);
  expect(r.sized(800, 600)).toBe(false);
});
