import { expect, test } from "vitest";
import type { ViewData } from "../src/data";
import { index } from "../src/model/graph";
import { fileTree, revealKeys, treeRows } from "../src/model/tree";
import fixture from "./fixture-data.json";

test("folders, then files, then members", () => {
  const ix = index(fixture as ViewData);
  const expanded: Record<string, boolean> = {};
  Object.keys(ix.modules).forEach((f) => [...revealKeys(ix, f), f].forEach((k) => (expanded[k] = true)));
  const rows = treeRows(ix, fileTree(ix), { filter: "", expanded, onScreen: null });
  const at = rows.findIndex((r) => r.key === "packages/core/src");
  const block = rows.slice(at + 1, at + 6).map((r) => [r.depth - rows[at].depth, r.name, r.dir]);
  expect(block).toEqual([
    [1, "util", true],
    [2, "strings.ts", false],
    [3, "upper", false],
    [3, "slugify2", false],
    [1, "export.ts", false],
  ]);
  const math = rows.findIndex((r) => r.key === "packages/core/src/math.ts");
  expect(rows.slice(math + 1, math + 4).map((r) => r.name)).toEqual(["add", "double", "overloaded"]);
});

test("the filter keeps matching members and their folders", () => {
  const ix = index(fixture as ViewData);
  const rows = treeRows(ix, fileTree(ix), { filter: "slugify", expanded: {}, onScreen: null });
  expect(rows.map((r) => r.name)).toEqual(["packages", "core/src", "util", "strings.ts", "slugify2"]);
});
