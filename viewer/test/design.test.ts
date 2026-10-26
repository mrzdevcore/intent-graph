import { describe, expect, it } from "vitest";
import type { Design } from "../src/api";
import { defaultLinkKind, graphId, isElementId, newId, proposeFile } from "../src/model/design";

const empty = (ids: string[] = []): Design => ({
  version: 1, title: "t", scope: { kind: "layer", name: "x" },
  elements: ids.map((id) => ({ id, kind: "module", name: "m" })), marks: [], links: [], bridges: [],
});

describe("design", () => {
  it("newId vectors (shared with Rust new_id_vectors)", () => {
    expect(newId(empty(), "function", "Export patients")).toBe("fn-export-patients");
    expect(newId(empty(["fn-export-patients"]), "function", "Export patients")).toBe("fn-export-patients-2");
    expect(newId(empty(), "route", "GET /api/patients/export")).toBe("route-get-api-patients-export");
    expect(newId(empty(), "system", "PostgreSQL")).toBe("sys-postgresql");
    expect(newId(empty(["note-1"]), "note", "anything")).toBe("note-2");
    expect(newId(empty(), "module", "  !!  ")).toBe("mod-1");
  });
  it("isElementId", () => {
    expect(isElementId("fn-a")).toBe(true);
    expect(["Fn-X", "1a", "", "a_b"].some(isElementId)).toBe(false);
  });
  it("graphId matches the Rust mapping", () => {
    expect(graphId({ id: "r", kind: "route", name: "x", method: "GET", path: "/x", file: "server/api/x.get.ts" })).toBe("server/api/x.get.ts#GET /x");
    expect(graphId({ id: "r", kind: "route", name: "x", path: "/x", file: "a.ts" })).toBeNull();
    expect(graphId({ id: "f", kind: "function", name: "f", file: "server/r.ts" })).toBe("server/r.ts#f");
    expect(graphId({ id: "f", kind: "function", name: "f", file: "" })).toBeNull();
    expect(graphId({ id: "m", kind: "module", name: "m", file: "a.ts" })).toBe("a.ts");
    expect(graphId({ id: "s", kind: "system", name: "PostgreSQL" })).toBe("system:PostgreSQL");
    expect(graphId({ id: "n", kind: "note", name: "n" })).toBeNull();
  });
  it("proposeFile", () => {
    const n = (file: string, layer: string | null) => ({ file, layer, module: true }) as any;
    const nodes = [n("server/api/a.ts", "api"), n("server/api/b.ts", "api"), n("server/x/c.js", "api"), n("web/d.vue", "ui")];
    expect(proposeFile(nodes, "api", "function", "Export patients")).toBe("server/api/export-patients.ts");
    expect(proposeFile(nodes, "db", "function", "x")).toBeNull();
  });
  it("defaultLinkKind", () => {
    expect(defaultLinkKind("page", "route")).toBe("http");
    expect(defaultLinkKind("function", "system")).toBe("uses");
    expect(defaultLinkKind("module", "module")).toBe("imports");
    expect(defaultLinkKind("function", "function")).toBe("calls");
  });
});
