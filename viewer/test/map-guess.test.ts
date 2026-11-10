import { expect, test } from "vitest";
import { guessArrow, baseOf } from "../src/map/edit/guess";
import { arrowKindNames, ARROW_KINDS } from "../src/map/edit/arrows";
import type { ArrowKindView, KindView } from "../src/map/types";

const rules: ArrowKindView[] = [
  { name: "reads", from: [], to: ["store", "queue"] },
  { name: "writes", from: [], to: ["store"] },
  { name: "sends", from: [], to: ["queue"] },
  { name: "implements", from: ["adapter"], to: ["port"] },
  { name: "triggers", from: [], to: ["job", "entry"] },
];

test.each([
  ["service", "store", "writes"],
  ["service", "queue", "sends"],
  ["service", "job", "triggers"],
  ["service", "entry", "triggers"],
  ["adapter", "port", "implements"],
  ["service", "port", "calls"],
  ["service", "box", "uses"],
  ["service", "service", "calls"],
])("%s to %s is %s", (from, to, kind) => {
  expect(guessArrow(from, to, rules)).toBe(kind);
  expect(guessArrow(from, to, [])).toBe(kind === "uses" ? "uses" : kind);
});

test("a team kind is judged by its base", () => {
  const kinds = [{ name: "pg", base: "store", custom: true }] as KindView[];
  expect(guessArrow(baseOf("service", kinds), baseOf("pg", kinds), rules)).toBe("writes");
});

test("the arrow kinds come from the view, with a fallback", () => {
  expect(arrowKindNames(rules)).toEqual(["reads", "writes", "sends", "implements", "triggers"]);
  expect(arrowKindNames(undefined)).toBe(ARROW_KINDS);
});
