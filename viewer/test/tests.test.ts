import { expect, test } from "vitest";
import { withTests } from "../src/model/tests";
import type { ViewData } from "../src/data";

const n = (id: string, file: string, t?: boolean) => ({ id, file, test: t }) as never;
const e = (from: string, to: string) => ({ from, to }) as never;
const D = {
  nodes: [n("a.ts", "a.ts"), n("a.ts#f", "a.ts"), n("a.spec.ts", "a.spec.ts", true), n("a.spec.ts#t", "a.spec.ts", true)],
  edges: [e("a.ts#f", "a.ts"), e("a.spec.ts#t", "a.ts#f"), e("a.ts", "a.spec.ts")],
  findings: [{ pinned: "a.spec.ts#t" }],
} as unknown as ViewData;

test("tests off drops test nodes and their edges; on restores them", () => {
  const off = withTests(D, false);
  expect(off.nodes.map((x) => x.id)).toEqual(["a.ts", "a.ts#f"]);
  expect(off.edges).toEqual([e("a.ts#f", "a.ts")]);
  expect(off.findings).toBe(D.findings);
  expect(withTests(D, true)).toBe(D);
});
