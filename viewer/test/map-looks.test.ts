import { expect, test } from "vitest";
import { arrowLook, insideCounts, look } from "../src/map/looks";
import type { MapArrow, MapBox, Mark, View } from "../src/map/types";
import shop from "./fixtures/shop-view.json";

const view = shop as unknown as View;
const box = view.boxes.find((b) => b.id === "api/application-checkout") as MapBox;
const bad: Mark = { layer: "health", target: box.id, state: "bad", badge: "failing" };

test("health beats kind; off, the kind shows", () => {
  const l = look(box.id, [bad], new Set(["health"]), box);
  expect(l).toMatchObject({ edge: "#c0392b", width: 2.5 });
  expect(l.glow).not.toBeNull();
  expect(look(box.id, [bad], new Set(), box).edge).toBe("#2f8f5b");
});

test("an Ok mark is a dot, no chips", () => {
  const l = look(box.id, [{ ...bad, state: "ok" }], new Set(["health"]), box);
  expect(l).toMatchObject({ dot: true, chips: [], edge: "#2f8f5b" });
});

test("insideCounts reaches every ancestor", () => {
  const code = view.boxes.find((b) => b.parent === box.id)!;
  const c = insideCounts(view, [{ layer: "health", target: code.id, state: "bad" }], new Set(["health"]));
  expect([c.get(box.id), c.get("api"), c.get("system")]).toEqual([1, 1, 1]);
  expect(c.get(code.id)).toBeUndefined();
});

const arrow = (kind: MapArrow["kind"]): MapArrow => ({ id: "a->b", parent: "system", from: "a", to: "b", kind, count: 1, kinds: [kind] });

test("arrowLook: event, and a risk keeps the dash", () => {
  expect(arrowLook(arrow("event"), [], new Set())).toMatchObject({ color: "#7b4fc9", dash: "2 5", head: "open" });
  const warn: Mark = { layer: "risks", target: "a->b", state: "warn", badge: "risk" };
  expect(arrowLook(arrow("network"), [warn], new Set(["risks"]))).toMatchObject({ color: "#d08a00", dash: "6 4" });
});
