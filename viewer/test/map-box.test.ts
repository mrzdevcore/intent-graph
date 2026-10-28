// @vitest-environment happy-dom
import { expect, test } from "vitest";
import { mount, flushSync } from "svelte";
import MapBox from "../src/map/MapBox.svelte";
import MapArrow from "../src/map/MapArrow.svelte";
import Region from "../src/map/Region.svelte";
import { logoPath } from "../src/map/logo";
import { look } from "../src/map/looks";
import type { MapBox as Box } from "../src/map/types";

const f = <T,>(value: T) => ({ value, source: "ig" as const });
const box = (o: Partial<Box> = {}): Box => ({
  id: "b", level: "component", madeOf: [], files: 1, kind: f("usecase"), title: f("Checkout"),
  description: f("Prices the cart."), badges: f(["REST"]), ...o,
});
const render = (b: Box, detail: "far" | "mid" | "near" = "near") => {
  const t = document.createElement("div");
  mount(MapBox, { target: t, props: { box: b, look: look(b.id, [], new Set(), b), detail, onopen: () => {} } });
  flushSync();
  return t;
};

test("a box is a button that opens; needsReading is dotted, stale is chipped", () => {
  const t = render(box({ needsReading: true, stale: true }));
  const btn = t.querySelector("button")!;
  expect(btn.getAttribute("aria-label")).toBe("Open Checkout");
  expect(btn.className).toContain("reading");
  expect(t.textContent).toContain("stale");
});

test("a container with logo stripe renders the simple-icons path", () => {
  const t = render(box({ level: "container", kind: f("external"), logo: f("stripe") }));
  expect(t.querySelector("svg path")!.getAttribute("d")).toBe(logoPath("stripe"));
});

test("far detail: no description, no chips", () => {
  const t = render(box(), "far");
  expect(t.textContent).not.toContain("Prices the cart.");
  expect(t.textContent).not.toContain("REST");
  expect(render(box()).textContent).toContain("REST");
});

test("a person is a figure without a box border", () => {
  const t = render(box({ kind: f("person"), title: f("Shopper") }));
  expect(t.querySelector(".figure svg")).not.toBeNull();
  expect(t.querySelector(".card")).toBeNull();
});

test("unknown logo slug is null", () => expect(logoPath("not-a-logo")).toBeNull());

test("an arrow draws a head and, unless far, a label", () => {
  const al = { color: "#333", width: 1.5, dash: null, head: "filled" as const, label: "pays", tone: "neutral" as const };
  const go = (detail: "far" | "near") => {
    const t = document.createElement("div");
    mount(MapArrow, { target: t, props: { points: [[0, 0], [100, 0], [100, 40]], arrowLook: al, detail } });
    flushSync();
    return t;
  };
  expect(go("near").querySelector("polyline")).not.toBeNull();
  expect(go("near").textContent).toContain("pays");
  expect(go("far").textContent).not.toContain("pays");
});

test("a region shows its label", () => {
  const t = document.createElement("div");
  mount(Region, { target: t, props: { region: { parent: "p", kind: "group", label: "billing", frame: { x: 0, y: 0, w: 10, h: 10 } } } });
  flushSync();
  expect(t.textContent).toContain("billing");
});
