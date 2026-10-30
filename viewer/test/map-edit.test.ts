// @vitest-environment happy-dom
import { expect, test } from "vitest";
import { flushSync, mount } from "svelte";
import Map from "../src/map/Map.svelte";
import type { EditApi } from "../src/api";
import type { View } from "../src/map/types";
import shop from "./fixtures/shop-view.json";
import { prefill, slug } from "../src/map/edit/add";

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

test("contenteditable: shortcuts ignored", () => {
  const { target } = setup(fake().api);
  const div = document.createElement("div");
  div.contentEditable = "true";
  target.append(div);
  expect(div.isContentEditable).toBe(true);
  press("b", {}, div);
  expect(pressed(target)).toContain("Select");
});

// --- add a box
const form = (t: HTMLElement) => t.querySelector('form[aria-label^="New box"]') as HTMLFormElement | null;
const input = (f: HTMLElement, label: string) =>
  [...f.querySelectorAll("label")].find((l) => l.textContent!.startsWith(label))!.querySelector("input,select") as HTMLInputElement;
const type = (el: HTMLInputElement, v: string) => { el.value = v; el.dispatchEvent(new Event("input", { bubbles: true })); flushSync(); };
function clickBox(t: HTMLElement, id: string | null) {
  press("b");
  const el = id ? t.querySelector(`[data-box="${id}"] > button`)! : t.querySelector(".stage")!;
  el.dispatchEvent(new MouseEvent("click", { bubbles: true, clientX: 40, clientY: 50 }));
  flushSync();
}
const adds = () => {
  const got: unknown[] = [];
  const f = fake({ apply: async (e) => (got.push(e), fresh()) });
  return { ...f, got };
};

test("Box tool click in a container: container kinds, Code lives at prefilled from its glob", () => {
  const { target } = setup(adds().api);
  clickBox(target, "api");
  const f = form(target)!;
  expect([...input(f, "Kind").querySelectorAll("option")].map((o) => o.value)).toEqual(["usecase", "domain", "port", "folder", "component", "entry"]);
  expect(document.activeElement).toBe(input(f, "Name"));
  type(input(f, "Name"), "Billing");
  expect(input(f, "Code lives at").value).toBe("apps/api/billing/**");
});

test("Box tool click on the ground: system kinds; external hides the code path", () => {
  const { target } = setup(adds().api);
  clickBox(target, null);
  const f = form(target)!;
  expect([...input(f, "Kind").querySelectorAll("option")].map((o) => o.value)).toEqual(["app", "api", "library", "worker", "store", "queue", "external", "person"]);
  ([...input(f, "Kind").querySelectorAll("option")].find((o) => o.value === "external") as HTMLOptionElement).selected = true; input(f, "Kind").dispatchEvent(new Event("change", { bubbles: true })); flushSync();
  expect([...f.querySelectorAll("label")].some((l) => l.textContent!.startsWith("Code lives"))).toBe(false);
});

test("Enter adds: apply gets the Add, the id de-clashed", async () => {
  const { api, got } = adds();
  const { target } = setup(api);
  clickBox(target, "api");
  type(input(form(target)!, "Name"), "Api");
  form(target)!.requestSubmit();
  await tick();
  expect(got).toEqual([{ op: "add", parent: "api", kind: "usecase", id: "api-2", title: "Api", at: ["apps/api/api/**"], before: null }]);
  expect(form(target)).toBeNull();
});

test("Esc closes the form and sends nothing", async () => {
  const { api, got } = adds();
  const { target } = setup(api);
  clickBox(target, "api");
  type(input(form(target)!, "Name"), "Billing");
  press("Escape", {}, input(form(target)!, "Name"));
  await tick();
  expect(form(target)).toBeNull();
  expect(got).toEqual([]);
});

test("prefill and slug edge cases", () => {
  expect(prefill(["apps/api/**", "libs/x/**"], "Billing")).toBe("apps/api/billing/**");
  expect(prefill(["apps/api/main.ts"], "Billing")).toBe("apps/api/billing/**");
  expect(prefill([], "Billing")).toBe("billing/**");
  expect(slug("Kelvin")).toBe("elvin"); // non-ASCII is a dash, like Rust
});
