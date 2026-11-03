// @vitest-environment happy-dom
import { expect, test } from "vitest";
import { flushSync, mount } from "svelte";
import Map from "../src/map/Map.svelte";
import { Camera } from "../src/map/glide";
import MapShowHarness from "./MapShowHarness.svelte";
import type { EditApi } from "../src/api";
import type { View } from "../src/map/types";
import { parseShow } from "../src/model/show";
import shop from "./fixtures/shop-view.json";

const size = { w: 1000, h: 800 };
const KID = "api/application-checkout", OTHER = "api/domain-order";

function setup(raw: object, props: object = {}) {
  const view = structuredClone(shop) as unknown as View;
  const camera = new Camera();
  const target = document.createElement("div");
  document.body.append(target);
  const m = mount(Map, { target, props: { view, camera, size, show: parseShow({ title: "Where it is used", ...raw }), ...props } });
  flushSync();
  m.tick(performance.now() + 5000);
  flushSync();
  return { view, camera, target, m };
}
const slot = (t: HTMLElement, id: string) => t.querySelector(`[data-box="${CSS.escape(id)}"]`) as HTMLElement | null;
const card = (t: HTMLElement) => t.querySelector('[aria-label="The answer"]') as HTMLElement | null;
const press = (key: string) => {
  window.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true }));
  flushSync();
};
const STEPS = [{ target: KID, text: "starts here" }, { target: OTHER, text: "then here" }];

test("an answer flies to the box holding it and lights its boxes, the rest faded", () => {
  const { target } = setup({ nodes: [KID, OTHER], steps: STEPS });
  expect(card(target)!.textContent).toContain("Where it is used");
  expect(card(target)!.textContent).toContain("starts here");
  expect(slot(target, KID)!.classList.contains("answer-now")).toBe(true);
  expect(slot(target, OTHER)!.classList.contains("answer")).toBe(true);
  expect(slot(target, "api/infra-prisma")!.classList.contains("faded")).toBe(true);
  expect(slot(target, KID)!.querySelector(".number")!.textContent).toBe("1");
  expect(slot(target, OTHER)!.querySelector(".number")!.textContent).toBe("2");
});

test("from far away a box holding part of the answer says how much", () => {
  const { target } = setup({ nodes: [KID, "web"] });
  expect(slot(target, "api")!.querySelector(".holds")!.textContent).toMatch(/^1 of \d+$/);
  expect(slot(target, "postgres")!.classList.contains("faded")).toBe(true);
  expect(slot(target, "web")!.classList.contains("faded")).toBe(false);
});

test("the keys and the card walk the steps; Escape clears it all", () => {
  const { target } = setup({ nodes: [KID, OTHER], steps: STEPS });
  press("ArrowRight");
  expect(card(target)!.textContent).toContain("then here");
  expect(card(target)!.textContent).not.toContain("starts here");
  expect(slot(target, OTHER)!.classList.contains("answer-now")).toBe(true);
  press("ArrowRight");
  expect(card(target)!.textContent).toContain("then here");
  press("ArrowLeft");
  expect(card(target)!.textContent).toContain("starts here");
  (card(target)!.querySelectorAll("ol button")[1] as HTMLElement).click();
  flushSync();
  expect(card(target)!.textContent).toContain("then here");
  press("Escape");
  expect(card(target)).toBeNull();
  expect(slot(target, "api")!.classList.contains("faded")).toBe(false);
});

test("panning away offers the way back, and it lands where the answer was", () => {
  const { target, camera, m } = setup({ nodes: [KID, OTHER], steps: STEPS });
  const at = camera.cam;
  expect(card(target)!.textContent).not.toContain("Back to the answer");
  const zoom = target.querySelector('button[aria-label="Zoom out"]') as HTMLElement | null;
  (zoom ?? ([...target.querySelectorAll("button")].find((b) => b.textContent === "−") as HTMLElement)).click();
  flushSync();
  const back = [...card(target)!.querySelectorAll("button")].find((b) => b.textContent === "Back to the answer")!;
  back.click();
  flushSync();
  m.tick(performance.now() + 5000);
  flushSync();
  expect(camera.cam).toEqual(at);
  expect(card(target)!.textContent).not.toContain("Back to the answer");
});

test("a step whose box is gone says so, and still lets the rest be walked", () => {
  const { target } = setup({ nodes: [KID], steps: [{ target: "gone/box", text: "x" }, { target: KID, text: "y" }] });
  expect(card(target)!.textContent).toContain("No longer on the map");
});

test("the card collapses to a line and comes back", () => {
  const { target } = setup({ nodes: [KID], steps: STEPS });
  (card(target)!.querySelector('button[aria-label="Collapse"]') as HTMLElement).click();
  flushSync();
  expect(card(target)!.querySelector("ol")).toBeNull();
  (card(target)!.querySelector('button[aria-label="Expand"]') as HTMLElement).click();
  flushSync();
  expect(card(target)!.querySelector("ol")).not.toBeNull();
});

test("an answer waits while an edit is under way, and comes when asked for", () => {
  const view = structuredClone(shop) as unknown as View;
  const target = document.createElement("div");
  document.body.append(target);
  const edit = {} as EditApi;
  const m = mount(MapShowHarness, { target, props: { view, edit, size } });
  flushSync();
  press("b");
  m.say(parseShow({ title: "Later on", nodes: [KID] })!);
  flushSync();
  expect(card(target)).toBeNull();
  expect(target.querySelector(".waiting")!.textContent).toContain("Later on");
  press("Escape");
  ([...target.querySelectorAll(".waiting button")].find((b) => b.textContent === "Show") as HTMLElement).click();
  flushSync();
  expect(target.querySelector(".waiting")).toBeNull();
  expect(card(target)!.textContent).toContain("Later on");
});

test("the same answer sent again draws again after it was cleared", () => {
  const view = structuredClone(shop) as unknown as View;
  const target = document.createElement("div");
  document.body.append(target);
  const m = mount(MapShowHarness, { target, props: { view, size } });
  flushSync();
  const s = parseShow({ title: "Twice", nodes: [KID] })!;
  m.say(s);
  flushSync();
  press("Escape");
  expect(card(target)).toBeNull();
  m.say({ ...s });
  flushSync();
  expect(card(target)!.textContent).toContain("Twice");
});
