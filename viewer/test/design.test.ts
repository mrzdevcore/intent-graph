import { describe, expect, it } from "vitest";
import { isElementId, proposeFile } from "../src/model/design";

describe("design", () => {
  it("isElementId", () => {
    expect(isElementId("fn-a")).toBe(true);
    expect(["Fn-X", "1a", "", "a_b"].some(isElementId)).toBe(false);
  });
  it("proposeFile", () => {
    const n = (file: string, layer: string | null) => ({ file, layer, module: true }) as any;
    const nodes = [n("server/api/a.ts", "api"), n("server/api/b.ts", "api"), n("server/x/c.js", "api"), n("web/d.vue", "ui")];
    expect(proposeFile(nodes, "api", "function", "Export patients")).toBe("server/api/export-patients.ts");
    expect(proposeFile(nodes, "db", "function", "x")).toBeNull();
  });
});
