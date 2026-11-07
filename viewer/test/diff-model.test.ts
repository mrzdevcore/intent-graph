// @vitest-environment happy-dom
import { afterEach, expect, test } from "vitest";
import { parseDiff } from "../src/model/diff";
import { embedded } from "../src/api/embedded";

const sample = {
  base: "159c1b6a0000000000000000000000000000000",
  head: "fc0a6a6a0000000000000000000000000000000",
  source: "spec",
  summary: "1 box changed, 1 box added",
  boxes: {
    added: [{ id: "intent-graph/mcp", title: "Mcp", parent: "intent-graph" }],
    removed: [],
    changed: [{ id: "intent-graph/root", title: "Src", parent: "intent-graph", files: ["src/a.rs"], filesChanged: 28, direct: true }],
  },
  arrows: { added: [{ from: "a", to: "b", kind: "calls", fromTitle: "A", toTitle: "B" }], removed: [] },
  intent: { status: "same", added: [], removed: [], moved: [], changed: [], arrowsAdded: [], arrowsRemoved: [] },
  problems: { new: [], fixed: [] },
};

afterEach(() => document.getElementById("ig-diff")?.remove());

test("parseDiff reads ig diff --json and refuses anything else", () => {
  const d = parseDiff(sample);
  expect(d!.boxes.changed[0].filesChanged).toBe(28);
  expect(d!.arrows.added[0].fromTitle).toBe("A");
  expect(parseDiff(null)).toBeNull();
  expect(parseDiff("x")).toBeNull();
  expect(parseDiff({ summary: "x" })).toBeNull();
  expect(parseDiff({ ...sample, boxes: { added: [] } })).toBeNull();
  expect(parseDiff({ ...sample, problems: undefined })!.problems).toEqual({ new: [], fixed: [] });
  expect(parseDiff({ ...sample, intent: undefined })!.intent.status).toBe("same");
});

test("the embedded host reads #ig-diff, null when the page has none", () => {
  expect(embedded().diff!()).toBeNull();
  const s = document.createElement("script");
  s.id = "ig-diff";
  s.type = "application/json";
  s.textContent = "null";
  document.body.append(s);
  expect(embedded().diff!()).toBeNull();
  s.textContent = "{not json";
  expect(embedded().diff!()).toBeNull();
  s.textContent = JSON.stringify(sample);
  expect(embedded().diff!()!.summary).toBe(sample.summary);
});
