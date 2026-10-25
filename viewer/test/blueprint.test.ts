import { expect, test } from "vitest";
import type { ViewData } from "../src/data";
import { blueprint, journeys } from "../src/model/blueprint";
import { index } from "../src/model/graph";
import fixture from "./fixture-data.json";

const D = fixture as ViewData;

test("the journey list equals the lane-0 modules", () => {
  const ix = index(D);
  const lane0 = D.nodes.filter((n) => n.module && n.lane === 0).map((n) => n.id).sort();
  expect(lane0.length).toBeGreaterThan(0);
  expect(journeys(ix).map((n) => n.id)).toEqual(lane0);
});

test("a journey is laid out in its lanes", () => {
  const ix = index(D);
  const els = blueprint(ix, "apps/web/src/main.tsx", true);
  expect(els.filter((e) => e.data.kind === "lane").map((e) => e.data.label)).toEqual(["Front", "UI", "Core"]);
  expect(els.find((e) => e.data.id === "apps/web/src/main.tsx")?.data.kind).toBe("journey");
});
