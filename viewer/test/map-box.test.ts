// @vitest-environment happy-dom
import { expect, test } from "vitest";
import { mount, flushSync } from "svelte";
import MapBox from "../src/map/MapBox.svelte";
import DetailsCard from "../src/map/DetailsCard.svelte";
import MapArrow from "../src/map/MapArrow.svelte";
import Region from "../src/map/Region.svelte";
import { logo } from "../src/map/logo";
import { look } from "../src/map/looks";
import { siMongodb, siNextdotjs, siTrpc, siPrisma, siStripe, siVuedotjs } from "simple-icons";
import { shapeOf } from "../src/map/shapes";
import type { KindView, MapBox as Box, View } from "../src/map/types";

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
  expect(t.querySelector("svg.logo path")!.getAttribute("d")).toBe(siStripe.path);
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

test("unknown logo slug is null", () => expect(logo("not-a-logo")).toBeNull());

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
  expect(t.querySelector("svg path")!.getAttribute("d")).toBe(siVuedotjs.path);
  expect(logo("react")).not.toBeNull();
});

test("a region shows its label", () => {
  const t = document.createElement("div");
  mount(Region, { target: t, props: { region: { parent: "p", kind: "group", label: "billing", frame: { x: 0, y: 0, w: 10, h: 10 } } } });
  flushSync();
  expect(t.textContent).toContain("billing");
});

test("bundled brands return the simple-icons path", () => {
  for (const [slug, si] of Object.entries({ nextdotjs: siNextdotjs, trpc: siTrpc, prisma: siPrisma, stripe: siStripe })) expect(logo(slug)?.path).toBe(si.path);
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

const kinds = (extra: Partial<KindView>[] = []): KindView[] => [
  ...["service", "app", "store", "entry"].map((n) => ({ name: n, base: n as Box["kind"]["value"], custom: false, title: n, means: "", icon: "server", code: true, role: "run" as const, props: [] })),
  ...extra.map((e) => ({ name: "x", base: "service" as const, custom: true, title: "", means: "", icon: "cpu", code: true, role: "run" as const, props: [], ...e })),
];
const shaped = (b: Box, detail: "far" | "mid" | "near" = "near") => {
  const t = document.createElement("div");
  mount(MapBox, { target: t, props: { box: b, look: look(b.id, [], new Set(), b), detail, onopen: () => {}, kinds: kinds([{ name: "iot-device" }]) } });
  flushSync();
  return t.querySelector("button")!;
};

test("an app whose tech names a CLI, or whose entries are commands with no route, is a terminal", () => {
  expect(shaped(box({ level: "container", kind: f("app"), tech: f("Typer CLI") })).dataset.shape).toBe("cli");
  expect(shaped(box({ level: "container", kind: f("app"), tech: f("Svelte") })).dataset.shape).toBe("app");
  const app = box({ id: "a", level: "container", kind: f("app") });
  const entry = (kind: string): Box => box({ id: `a/${kind}`, parent: "a", kind: f("entry"), props: { kind } });
  expect(shapeOf(app, [entry("command")])).toBe("cli");
  expect(shapeOf(app, [entry("command"), entry("route")])).toBe("app");
});

test("a box with tech MongoDB 7 wears the MongoDB logo; a dark brand draws in the map's foreground", () => {
  const b = shaped(box({ level: "container", kind: f("store"), tech: f("MongoDB 7") }));
  expect(b.dataset.logo).toBe("mongodb");
  expect(b.querySelector("svg.logo path")!.getAttribute("d")).toBe(siMongodb.path);
  const k = shaped(box({ level: "container", kind: f("queue"), tech: f("Apache Kafka") }));
  expect(k.dataset.logo).toBe("apachekafka");
  expect(k.querySelector("svg.logo path")!.getAttribute("fill")).toBe("var(--map-fg)");
  // no logo: the kind's own icon, not an empty tile
  expect(shaped(box({ level: "container", kind: f("store"), tech: f("Homegrown") })).querySelector(".kind-icon")).not.toBeNull();
});

test("a team kind with base service gets the service shape and its own icon", () => {
  const b = shaped(box({ level: "container", kind: f("service"), custom: "iot-device" }));
  expect(b.dataset.shape).toBe("service");
  expect(b.querySelector(".kind-icon")!.getAttribute("data-icon")).toBe("cpu");
});

test("a module and an adapter show their kind's icon from the table", () => {
  const icon = (b: Box, k: Partial<KindView>) => {
    const t = document.createElement("div");
    mount(MapBox, { target: t, props: { box: b, look: look(b.id, [], new Set(), b), detail: "near", onopen: () => {}, kinds: kinds([{ custom: false, role: "design", ...k }]) } });
    flushSync();
    return t.querySelector(".kind-icon")?.getAttribute("data-icon");
  };
  expect(icon(box({ level: "container", kind: f("module") }), { name: "module", base: "module", icon: "package" })).toBe("package");
  expect(icon(box({ id: "a/b", parent: "a", kind: f("adapter") }), { name: "adapter", base: "adapter", icon: "cable" })).toBe("cable");
});

test("near: status first, then at most 2 badges and +n; the kind word is not on the face", () => {
  const b = shaped(box({ level: "container", kind: f("service"), origin: "build", badges: f(["REST", "gRPC", "GraphQL", "WS"]) }));
  expect([...b.querySelectorAll(".chips > span")].map((c) => c.textContent)).toEqual(["to build", "REST", "gRPC", "+2"]);
  expect(shaped(box({ level: "container", kind: f("service"), tech: f("FastAPI") }), "mid").textContent).not.toContain("service");
});

test("a dark brand on a CLI's terminal draws in the terminal's foreground, not the map's", () => {
  const b = shaped(box({ level: "container", kind: f("app"), tech: f("Rust CLI") }));
  expect(b.dataset.logo).toBe("rust");
  expect(b.querySelector("svg.logo path")!.getAttribute("fill")).toBe("var(--kind-cli-fg)");
});

test("an external is a dashed cloud; too small a frame makes it a dashed rounded box", () => {
  expect(shaped(box({ level: "container", kind: f("external") })).querySelector('[data-outline="cloud"]')).not.toBeNull();
  const t = document.createElement("div");
  const b = box({ level: "container", kind: f("external") });
  mount(MapBox, { target: t, props: { box: b, look: look(b.id, [], new Set(), b), detail: "near", onopen: () => {}, w: 160, h: 90 } });
  flushSync();
  expect(t.querySelector('[data-outline="cloud"]')).toBeNull();
  expect(t.querySelector("button")!.dataset.shape).toBe("external");
});

test("a group's name sits on its dotted edge, which breaks under it", () => {
  const b = shaped(box({ level: "container", kind: f("group"), title: f("Ops") }));
  expect(b.querySelector(".edge-label")!.textContent).toBe("Ops");
  const d = b.querySelector("svg.shape path")!.getAttribute("d")!;
  // the frame's path starts after the label and ends before it: an open path, no Z
  expect(d.endsWith("Z")).toBe(false);
  expect(b.querySelector(".title")).toBeNull();
});

const card = (b: Box) => {
  const t = document.createElement("div");
  const view = { version: 1, boxes: [b], arrows: [], marks: [], placed: { frames: {}, inner: {}, regions: [] }, notices: [], warnings: [], kinds: [] } as unknown as View;
  mount(DetailsCard, { target: t, props: { view, box: b, onclose: () => {}, onopen: () => {} } });
  flushSync();
  return t.textContent ?? "";
};

test("the card says what to do with a box not in ig/, and nothing for a group that holds them", () => {
  expect(card(box({ origin: "unmanaged" }))).toContain("Not in ig/: add a box named like it, or link one with runs=");
  expect(card(box())).not.toContain("Not in ig/");
});

test("a box of kind box is dashed, edged and filled like a draft, with its band", () => {
  const t = render(box({ kind: f("box"), title: f("Thing") }));
  const card = t.querySelector<HTMLElement>(".card")!;
  expect(card.style.borderStyle).toBe("dashed");
  expect(card.style.borderColor).toBe("var(--map-edge)");
  expect(card.style.background).toContain("var(--map-surface)");
  expect(t.querySelector(".band")!.textContent).toBe("no kind yet");
});
