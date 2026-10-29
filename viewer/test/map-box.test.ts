// @vitest-environment happy-dom
import { expect, test } from "vitest";
import { mount, flushSync } from "svelte";
import MapBox from "../src/map/MapBox.svelte";
import MapArrow from "../src/map/MapArrow.svelte";
import Region from "../src/map/Region.svelte";
import { logoPath } from "../src/map/logo";
import { look } from "../src/map/looks";
import { siNextdotjs, siTrpc, siPrisma, siStripe } from "simple-icons";
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

test("an arrow's line has a head; its label is a part of its own, steady on screen", () => {
  const al = { color: "#333", width: 1.5, dash: "6 4", head: "filled" as const, label: "pays", tone: "neutral" as const };
  const go = (part: "line" | "label", zoom = 1) => {
    const t = document.createElement("div");
    mount(MapArrow, { target: t, props: { d: "M0,0 C50,0 50,40 100,40", mid: [50, 20], arrowLook: al, part, zoom } });
    flushSync();
    return t;
  };
  expect(go("line").querySelector("g[data-state] > path")).not.toBeNull();
  expect(go("line").querySelector("marker path")).not.toBeNull();
  expect(go("line").textContent).not.toContain("pays");
  expect(go("label").textContent).toContain("pays");
  // 10 screen pixels per unit: the line is drawn 1.6 pixels per unit wide on screen, not 10
  const line = go("line", 10).querySelector("g[data-state] > path")!;
  expect(Number(line.getAttribute("stroke-width")) * 10).toBeCloseTo(1.5 * 1.6);
  expect(line.getAttribute("stroke-dasharray")).toBe("0.96 0.64");
  expect(go("label", 10).querySelector("g")!.getAttribute("transform")).toContain("scale(0.16)");
});

test("a function zoomed into shows its numbered lines, steady on screen, and what was cut", () => {
  const t = document.createElement("div");
  const b = box({ level: "code", kind: f("function"), title: f("total") });
  const source = { file: "a.ts", line: 3, text: "function total() {\n  return 1;\n}", cut: 7, complexity: 1 };
  mount(MapBox, { target: t, props: { box: b, look: look(b.id, [], new Set(), b), detail: "source", zoom: 10, source, onopen: () => {} } });
  flushSync();
  const pre = t.querySelector("pre")!;
  expect([...pre.querySelectorAll(".ln")].map((e) => e.textContent)).toEqual(["3", "4", "5", "…"]);
  expect(pre.textContent).toContain("7 more lines");
  expect((pre.closest(".body") as HTMLElement).style.transform).toBe("scale(0.16)");
});

test("a component's part box tallies its parts; zoomed into, it lists them above its code", () => {
  const go = (detail: "near" | "source") => {
    const t = document.createElement("div");
    const b = box({ level: "code", kind: f("parts"), title: f("Inputs") });
    const parts = [
      { name: "orderId", kind: "prop", line: 4, detail: "string" },
      { name: "compact", kind: "prop", line: 5 },
      { name: "saved", kind: "emit", line: 6 },
    ];
    const source = { file: "a.vue", line: 1, text: "<template>", complexity: 0 };
    mount(MapBox, { target: t, props: { box: b, look: look(b.id, [], new Set(), b), detail, zoom: 4, parts, source, onopen: () => {} } });
    flushSync();
    return t;
  };
  expect(go("near").textContent).toContain("2 props · 1 emit");
  expect(go("near").querySelector(".parts")).toBeNull();
  const t = go("source");
  expect([...t.querySelectorAll(".parts b")].map((e) => e.textContent)).toEqual(["orderId", "compact", "saved"]);
  expect(t.querySelector(".parts")!.textContent).toContain("string");
  expect(t.querySelector("pre")!.textContent).toContain("<template>");
});

test("a component box wears its framework's logo", () => {
  const t = render(box({ level: "code", kind: f("component"), title: f("OrderList"), logo: f("vue") }));
  expect(t.querySelector("svg path")!.getAttribute("d")).toBe(logoPath("vue"));
  expect(logoPath("react")).not.toBeNull();
});

test("a region shows its label", () => {
  const t = document.createElement("div");
  mount(Region, { target: t, props: { region: { parent: "p", kind: "group", label: "billing", frame: { x: 0, y: 0, w: 10, h: 10 } } } });
  flushSync();
  expect(t.textContent).toContain("billing");
});

test("bundled brands return the simple-icons path", () => {
  for (const [slug, si] of Object.entries({ nextdotjs: siNextdotjs, trpc: siTrpc, prisma: siPrisma, stripe: siStripe })) expect(logoPath(slug)).toBe(si.path);
});

test("rings: domain gold, application green, else blue-grey", () => {
  const bg = (label: string) => {
    const t = document.createElement("div");
    mount(Region, { target: t, props: { region: { parent: "p", kind: "ring", label, frame: { x: 0, y: 0, w: 10, h: 10 } } } });
    flushSync();
    return t.querySelector(".region")!.getAttribute("data-role");
  };
  expect([bg("Domain"), bg("Application"), bg("Infrastructure")]).toEqual(["domain", "application", "infra"]);
});
