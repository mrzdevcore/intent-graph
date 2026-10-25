import { expect, test } from "vitest";
import type { ViewData } from "../src/data";
import { changedIds, exact } from "../src/model/changed";
import { focusIds } from "../src/model/focus";
import { index } from "../src/model/graph";
import fixture from "./fixture-data.json";

const D = fixture as ViewData;

test("changed nodes and their neighbours, members boxed in their modules", () => {
  const ix = index(D);
  const ids = changedIds(ix);
  expect(ids["packages/core/src/export.ts#exportRows"]).toBe(true);
  const els = exact(ix, ids);
  const member = els.find((e) => e.data.id === "packages/core/src/export.ts#exportRows");
  expect(member?.data).toMatchObject({ parent: "packages/core/src/export.ts", planned: true, status: "added" });
  expect(els.find((e) => e.data.id === "packages/core/src/export.ts")?.data.kind).toBe("openmodule");
});

test("focus reaches one edge away at depth 1, more at depth 2", () => {
  const ix = index(D);
  const one = Object.keys(focusIds(ix, "packages/core/src/math.ts#add", 1));
  const two = Object.keys(focusIds(ix, "packages/core/src/math.ts#add", 2));
  expect(one).toContain("packages/core/src/math.ts#double");
  expect(two.length).toBeGreaterThan(one.length);
});

test("focus accepts several start ids", () => {
  const ix = index(D);
  const ids = Object.keys(focusIds(ix, ["packages/core/src/math.ts#add", "packages/core/src/export.ts#exportRows"], 0));
  expect(ids.sort()).toEqual(["packages/core/src/export.ts#exportRows", "packages/core/src/math.ts#add"]);
});
