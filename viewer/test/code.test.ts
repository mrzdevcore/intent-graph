import { expect, test } from "vitest";
import { beforeAfter, languageFor, nodeNamed } from "../src/model/code";
import type { DiffLine, ViewNode } from "../src/data";

test("the language is chosen by extension", () => {
  for (const p of ["a.ts", "a.tsx", "a.js", "a.jsx", "a.css", "a.html", "a.vue", "a.json", "A.TS", "dir.x/a.json"])
    expect(languageFor(p), p).toHaveLength(1);
  for (const p of ["README", "a.rs", "a.md", "dir.ts/Makefile"]) expect(languageFor(p), p).toHaveLength(0);
});

test("before and after are rebuilt from the diff rows", () => {
  const d: DiffLine[] = [[" ", 1, 1, "a"], ["-", 2, null, "old"], ["+", null, 2, "new"], [" ", 3, 3, "z"]];
  expect(beforeAfter(d)).toEqual({ before: "a\nold\nz", after: "a\nnew\nz" });
});

test("a word maps to the node of that file with that name", () => {
  const n = (id: string, name: string, line: number, endLine: number, file = "f.ts") => ({ id, name, line, endLine, file, module: false }) as unknown as ViewNode;
  const nodes = [n("a", "go", 1, 3), n("b", "go", 10, 12), n("c", "go", 1, 3, "g.ts"), { ...n("m", "go", 1, 99), module: true } as ViewNode];
  expect(nodeNamed(nodes, "f.ts", "go", 11)?.id).toBe("b");
  expect(nodeNamed(nodes, "f.ts", "go", 50)?.id).toBe("a");
  expect(nodeNamed(nodes, "f.ts", "nope", 1)).toBeUndefined();
});
