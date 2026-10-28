// @vitest-environment happy-dom
import { expect, test } from "vitest";
import { flushSync, mount } from "svelte";
import Map from "../src/map/Map.svelte";
import { Camera } from "../src/map/glide";
import { fitTo, innerTransform, worldFrame } from "../src/map/camera";
import type { View } from "../src/map/types";
import shop from "./fixtures/shop-view.json";

const EVIL = "<img src=x onerror=alert(1)>";
const size = { w: 1000, h: 800 };

function setup() {
  const view = structuredClone(shop) as unknown as View;
  view.boxes.find((b) => b.id === "stripe")!.title.value = EVIL;
  const camera = new Camera();
  const target = document.createElement("div");
  document.body.append(target);
  const m = mount(Map, { target, props: { view, camera, size } });
  flushSync();
  return { view, camera, target, m };
}
const box = (t: HTMLElement, id: string) => t.querySelector(`[data-box="${CSS.escape(id)}"]`);

test("at Fit the containers render and api's inside is not mounted", () => {
  const { view, target } = setup();
  const fit = fitTo(worldFrame("system", view), size);
  expect((target.querySelector(".world") as HTMLElement).style.transform).toContain(`scale(${fit.k})`);
  for (const id of ["api", "api-client", "postgres", "stripe", "web"]) expect(box(target, id)).not.toBeNull();
  expect(box(target, "api/application-checkout")).toBeNull();
  expect(target.textContent).toContain("Some boxes are ig's guess");
});

test("gliding into api mounts its components and the failing rules arrow", () => {
  const { view, camera, target, m } = setup();
  camera.to(fitTo(worldFrame("api", view), size), 0);
  m.tick(800);
  flushSync();
  expect(box(target, "api/application-checkout")).not.toBeNull();
  const line = target.querySelector(`[data-arrow="${CSS.escape("api/domain-order->api/infra-prisma")}"] polyline`)!;
  expect(line.getAttribute("stroke")).toBe("#c0392b");
  // still no code level: checkout's own inside is too small on screen
  expect(box(target, "apps/api/src/application/checkout/checkout.ts#checkout")).toBeNull();
});

test("clicking a box opens the details card; titles are text", () => {
  const { target } = setup();
  (box(target, "stripe")!.querySelector("button") as HTMLButtonElement).click();
  flushSync();
  const card = target.querySelector(".details")!;
  expect(card.textContent).toContain(EVIL);
  expect(card.querySelector("img")).toBeNull();
  expect(target.querySelector("img")).toBeNull();
});

test("the details card lists connections and needs attention", () => {
  const { view, camera, target, m } = setup();
  camera.to(fitTo(worldFrame("api", view), size), 0);
  m.tick(800);
  flushSync();
  (box(target, "api/domain-order")!.querySelector("button") as HTMLButtonElement).click();
  flushSync();
  const card = target.querySelector(".details")!.textContent!;
  expect(card).toContain("breaks: domain → infrastructure");
  expect(card).toContain("apps/api/src/domain/order/**");
});

test("a health layer turns api amber and its value shows in the card; the layer can be switched off", () => {
  const view = structuredClone(shop) as unknown as View;
  view.marks.push({ layer: "health", target: "api", state: "warn", value: "p95 820ms" });
  const target = document.createElement("div");
  document.body.append(target);
  mount(Map, { target, props: { view, camera: new Camera(), size } });
  flushSync();
  const btn = () => box(target, "api")!.querySelector("button") as HTMLButtonElement;
  expect(btn().style.border).toContain("#d08a00");
  btn().click();
  flushSync();
  expect(target.querySelector(".details")!.textContent).toContain("p95 820ms");
  const health = [...target.querySelectorAll(".layers label")].find((l) => l.textContent!.includes("health"))!;
  (health.querySelector("input") as HTMLInputElement).click();
  flushSync();
  expect(btn().style.border).not.toContain("#d08a00");
});

test("clicking a box with an inside frames the inside: it fills >= 80% of the view", () => {
  const { view, camera, target, m } = setup();
  (box(target, "api")!.querySelector("button") as HTMLButtonElement).click();
  m.tick(performance.now() + 1000);
  flushSync();
  const t = innerTransform("api", view), [iw, ih] = view.placed.inner["api"], k = camera.cam.k;
  expect(Math.max((iw * t.s * k) / size.w, (ih * t.s * k) / size.h)).toBeGreaterThanOrEqual(0.8 - 1e-9);
  expect(box(target, "api/application-checkout")).not.toBeNull();
});

test("Esc closes the details card and focus returns to the box", () => {
  const { target } = setup();
  const btn = box(target, "stripe")!.querySelector("button") as HTMLButtonElement;
  btn.click();
  flushSync();
  expect(document.activeElement).toBe(target.querySelector(".details"));
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
  flushSync();
  expect(target.querySelector(".details")).toBeNull();
  expect(document.activeElement).toBe(btn);
});

test("a bad map (a parent loop, a duplicate id) neither hangs nor throws", () => {
  const view = structuredClone(shop) as unknown as View;
  const api = view.boxes.find((b) => b.id === "api")!;
  view.boxes.push({ ...api, id: "zzz", parent: "zzz" }, { ...api, id: "x", parent: "y" }, { ...api, id: "y", parent: "x" });
  view.boxes.push({ ...api, parent: "api" });
  for (const id of ["zzz", "x", "y"]) view.placed.frames[id] = view.placed.frames.api;
  view.marks.push({ layer: "rules", target: "zzz", state: "bad" }, { layer: "rules", target: "x", state: "bad" });
  const camera = new Camera();
  const target = document.createElement("div");
  document.body.append(target);
  const m = mount(Map, { target, props: { view, camera, size } });
  flushSync();
  expect(target.querySelector(`[data-box="api"]`)).not.toBeNull();
  // into api, where the duplicate `api` (its own child) would open forever
  camera.to(fitTo(worldFrame("api", view), size), 0);
  m.tick(800);
  flushSync();
  expect(target.querySelectorAll(`[data-box="api"]`).length).toBe(1);
  expect(target.querySelector(`[data-box="api/application-checkout"]`)).not.toBeNull();
});
