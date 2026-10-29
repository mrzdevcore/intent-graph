import { expect, test } from "vitest";
import { arrowLook, insideCounts, look } from "../src/map/looks";
import type { MapArrow, MapBox, Mark, View } from "../src/map/types";
import shop from "./fixtures/shop-view.json";

const view = shop as unknown as View;
const box = view.boxes.find((b) => b.id === "api/application-checkout") as MapBox;
const bad: Mark = { layer: "health", target: box.id, state: "bad", badge: "failing" };

test("health beats kind; off, the kind shows", () => {
  const l = look(box.id, [bad], new Set(["health"]), box);
  expect(l).toMatchObject({ edge: "var(--map-bad)", width: 2.5 });
  expect(l.glow).not.toBeNull();
  expect(look(box.id, [bad], new Set(), box).edge).toBe("var(--map-green)");
});

test("an Ok mark is a dot, no chips", () => {
  const l = look(box.id, [{ ...bad, state: "ok" }], new Set(["health"]), box);
  expect(l).toMatchObject({ dot: true, chips: [], edge: "var(--map-green)" });
});

test("insideCounts reaches every ancestor", () => {
  const code = view.boxes.find((b) => b.parent === box.id)!;
  const c = insideCounts(view, [{ layer: "health", target: code.id, state: "bad" }], new Set(["health"]));
  expect([c.get(box.id), c.get("api"), c.get("system")]).toEqual([1, 1, 1]);
  expect(c.get(code.id)).toBeUndefined();
});

const arrow = (kind: MapArrow["kind"]): MapArrow => ({ id: "a->b", parent: "system", from: "a", to: "b", kind, count: 1, kinds: [kind] });

test("arrowLook: event, and a risk keeps the dash", () => {
  expect(arrowLook(arrow("event"), [], new Set())).toMatchObject({ color: "var(--map-purple)", dash: "2 5", head: "open" });
  const warn: Mark = { layer: "risks", target: "a->b", state: "warn", badge: "risk" };
  expect(arrowLook(arrow("network"), [warn], new Set(["risks"]))).toMatchObject({ color: "var(--map-warn)", dash: "6 4" });
});

test("insideCounts stops at a parent loop", () => {
  const v = structuredClone(view);
  v.boxes.push({ ...box, id: "zzz", parent: "zzz" }, { ...box, id: "x", parent: "y" }, { ...box, id: "y", parent: "x" });
  const c = insideCounts(v, [{ layer: "rules", target: "zzz", state: "bad" }, { layer: "rules", target: "x", state: "bad" }], new Set(["rules"]));
  expect(c.get("zzz")).toBeUndefined(); // its own mark, under no other box
  expect([c.get("x"), c.get("y")]).toEqual([undefined, 1]);
});
