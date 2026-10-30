// @vitest-environment happy-dom
import { expect, test } from "vitest";
import { flushSync, mount } from "svelte";
import App from "../src/components/App.svelte";
import type { IgApi } from "../src/api";
import type { View } from "../src/map/types";
import shop from "./fixtures/shop-view.json";

const tick = () => new Promise((r) => setTimeout(r, 0));
const boxes = (t: HTMLElement) => t.querySelectorAll("[data-box]").length;
const world = (t: HTMLElement) => (t.querySelector(".world") as HTMLElement).style.transform;
const fake = () => {
  let rebuild = () => {};
  let next: () => Promise<View> = async () => structuredClone(shop) as unknown as View;
  const api: IgApi = { view: () => next(), onRebuild: (cb) => ((rebuild = cb), () => {}) };
  return { api, fire: (n: () => Promise<View>) => ((next = n), rebuild()) };
};
const mountApp = async (api: IgApi) => {
  const target = document.createElement("div");
  document.body.append(target);
  mount(App, { target, props: { api } });
  await tick();
  flushSync();
  return target;
};
const without = (id: string) => {
  const v = structuredClone(shop) as unknown as View;
  v.boxes = v.boxes.filter((b) => b.id !== id);
  return v;
};

test("a rebuild replaces the view: the map updates, Map stays mounted (camera unchanged)", async () => {
  const { api, fire } = fake();
  const t = await mountApp(api);
  const n = boxes(t), cam = world(t);
  expect(n).toBeGreaterThan(0);
  fire(async () => without("stripe"));
  await tick();
  flushSync();
  expect(boxes(t)).toBe(n - 1);
  expect(world(t)).toBe(cam);
});

test("the newer reload wins; a failed reload keeps the map and hints the error", async () => {
  const { api, fire } = fake();
  const t = await mountApp(api);
  const n = boxes(t);
  let slow!: (v: View) => void;
  fire(() => new Promise<View>((r) => (slow = r)));
  fire(async () => without("stripe"));
  await tick();
  slow(structuredClone(shop) as unknown as View);
  await tick();
  flushSync();
  expect(boxes(t)).toBe(n - 1);
  fire(async () => { throw new Error("bad ig/"); });
  await tick();
  flushSync();
  expect(boxes(t)).toBe(n - 1);
  expect(t.querySelector("header .spacer")!.textContent).toContain("bad ig/");
});

test("the first load failing shows the map error", async () => {
  const t = await mountApp({ view: async () => { throw new Error("nope"); } });
  expect(t.textContent).toContain("Could not read the map: nope");
});
