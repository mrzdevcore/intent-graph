// @vitest-environment happy-dom
import { expect, test } from "vitest";
import { flushSync, mount } from "svelte";
import App from "../src/components/App.svelte";
import type { EditApi, IgApi } from "../src/api";
import type { View } from "../src/map/types";
import shop from "./fixtures/shop-view.json";

const tick = () => new Promise((r) => setTimeout(r, 0));
const clone = () => structuredClone(shop) as unknown as View;
const boxes = (t: HTMLElement) => t.querySelectorAll("[data-box]").length;
const without = (id: string) => {
  const v = clone();
  v.boxes = v.boxes.filter((b) => b.id !== id);
  return v;
};
async function mountApp(api: IgApi) {
  const target = document.createElement("div");
  document.body.append(target);
  mount(App, { target, props: { api } });
  await tick();
  flushSync();
  return target;
}
const nope = () => Promise.reject(new Error("no"));

test("an edit's view wins over an older load still in flight", async () => {
  const loads: ((v: View) => void)[] = [];
  let first = true, rebuild = () => {};
  const B = without("stripe");
  const edit = { undo: async () => B } as unknown as EditApi;
  const api: IgApi = {
    view: () => (first ? ((first = false), Promise.resolve(clone())) : new Promise<View>((r) => loads.push(r))),
    onRebuild: (cb) => ((rebuild = cb), () => {}),
    edit,
  };
  const t = await mountApp(api);
  const full = boxes(t);
  rebuild();
  await tick();
  (t.querySelector('[aria-label="Undo"]') as HTMLElement).click();
  await tick();
  flushSync();
  expect(boxes(t)).toBe(full - 1);
  loads[0](clone());
  await tick();
  flushSync();
  expect(boxes(t)).toBe(full - 1);
});

test("a rejected dropAll re-reads the view and the message still shows", async () => {
  let reads = 0;
  const api: IgApi = {
    view: async () => (reads++, clone()),
    edit: { undo: () => Promise.reject(new Error("partly dropped")), dropAll: nope } as unknown as EditApi,
  };
  const t = await mountApp(api);
  expect(reads).toBe(1);
  (t.querySelector('[aria-label="Undo"]') as HTMLElement).click();
  await tick();
  flushSync();
  expect(reads).toBe(2);
  expect(t.querySelector('[aria-live="polite"]')!.textContent).toBe("partly dropped");
});
