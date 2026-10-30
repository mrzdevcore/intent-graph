// @vitest-environment happy-dom
import { expect, test } from "vitest";
import { flushSync, mount } from "svelte";
import Map from "../src/map/Map.svelte";
import type { EditApi } from "../src/api";
import type { View } from "../src/map/types";
import shop from "./fixtures/shop-view.json";

const tick = () => new Promise((r) => setTimeout(r, 0));
const fresh = () => structuredClone(shop) as unknown as View;
const size = { w: 1000, h: 800 };

function setup(edit?: EditApi) {
  const target = document.createElement("div");
  document.body.append(target);
  const seen: View[] = [];
  mount(Map, { target, props: { view: fresh(), size, edit, onview: (v: View) => seen.push(v) } });
  flushSync();
  return { target, seen };
}
const fake = (over: Partial<EditApi> = {}) => {
  const calls: string[] = [];
  const v = fresh();
  const api = { apply: async () => v, keep: async () => v, drop: async () => v, dropAll: async () => v,
    undo: async () => (calls.push("undo"), v), redo: async () => (calls.push("redo"), v), ...over } as EditApi;
  return { api, calls, v };
};
const press = (key: string, init: KeyboardEventInit = {}, on: EventTarget = window) => {
  on.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, ...init }));
  flushSync();
};
const pressed = (t: HTMLElement) => t.querySelector('[role="toolbar"] [aria-pressed="true"]')!.textContent;

test("no edit api: no toolbar", () => {
  const { target } = setup();
  expect(target.querySelector('[role="toolbar"]')).toBeNull();
});

test("B, A, V switch tools; Esc returns to select", () => {
  const { target } = setup(fake().api);
  expect(pressed(target)).toContain("Select");
  press("b");
  expect(pressed(target)).toContain("+ Box");
  press("a");
  expect(pressed(target)).toContain("Arrow");
  press("Escape");
  expect(pressed(target)).toContain("Select");
  press("b");
  press("v");
  expect(pressed(target)).toContain("Select");
});

test("Cmd+Z undoes and the returned view goes to onview; Cmd+Shift+Z redoes", async () => {
  const { api, calls, v } = fake();
  const { seen } = setup(api);
  press("z", { metaKey: true });
  await tick();
  press("z", { metaKey: true, shiftKey: true });
  await tick();
  expect(calls).toEqual(["undo", "redo"]);
  expect(seen).toEqual([v, v]);
});

test("shortcuts are ignored while an input has focus", async () => {
  const { api, calls } = fake();
  const { target } = setup(api);
  const input = document.createElement("input");
  const area = document.createElement("textarea");
  target.append(input, area);
  for (const el of [input, area]) {
    press("b", {}, el);
    press("z", { metaKey: true }, el);
  }
  press("a");
  press("Escape", {}, input);
  expect(pressed(target)).toContain("Arrow");
  press("v");
  await tick();
  expect(pressed(target)).toContain("Select");
  expect(calls).toEqual([]);
});

test("a rejected call shows its message in the live region", async () => {
  const { api } = fake({ undo: () => Promise.reject(new Error("changed by hand")) });
  const { target, seen } = setup(api);
  (target.querySelector('[aria-label="Undo"]') as HTMLElement).click();
  await tick();
  flushSync();
  expect(target.querySelector('[aria-live="polite"]')!.textContent).toBe("changed by hand");
  expect(seen).toEqual([]);
});

test("contenteditable: happy-dom's isContentEditable", () => {
  const { target } = setup(fake().api);
  const div = document.createElement("div");
  div.contentEditable = "true";
  target.append(div);
  // ponytail: happy-dom may not implement isContentEditable; then the guard is untested here
  if (!div.isContentEditable) return console.warn("happy-dom: no isContentEditable");
  press("b", {}, div);
  expect(pressed(target)).toContain("Select");
});
