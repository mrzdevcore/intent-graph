// @vitest-environment happy-dom
import { afterEach, expect, test, vi } from "vitest";
import { flushSync, mount as mountIt, unmount } from "svelte";
import Map from "../src/map/Map.svelte";
import type { EditApi } from "../src/api";
import type { Edit, View } from "../src/map/types";
import shop from "./fixtures/shop-view.json";
import { prefill, slug } from "../src/map/edit/add";
import { proposed } from "../src/map/edit/proposals";
import { arrowLook, look } from "../src/map/looks";
import MapHarness from "./MapHarness.svelte";
import MovePicker from "../src/map/edit/MovePicker.svelte";
import { parentsFor } from "../src/map/edit/menu";
import { Camera } from "../src/map/glide";
import { EditSession, type Ctx } from "../src/map/edit/session.svelte";
import { fitTo, worldFrame } from "../src/map/camera";
import { remember } from "../src/map/edit/clipboard";

const tick = () => new Promise((r) => setTimeout(r, 0));
// a map left mounted still hears the window's keys
const mounted: ReturnType<typeof mountIt>[] = [];
const mount = ((c: never, o: never) => {
  const m = mountIt(c, o);
  mounted.push(m);
  return m;
}) as typeof mountIt;
afterEach(() => {
  for (const m of mounted.splice(0)) unmount(m);
  document.body.innerHTML = "";
  localStorage.clear();
  remember(null);
});
const fresh = () => structuredClone(shop) as unknown as View;
const size = { w: 1000, h: 800 };

function setup(edit?: EditApi, view: View = fresh()) {
  const target = document.createElement("div");
  document.body.append(target);
  const seen: View[] = [];
  mount(Map, { target, props: { view, size, edit, onview: (v: View) => seen.push(v) } });
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
const pressed = (t: HTMLElement) => t.querySelector('[role="toolbar"] [aria-pressed="true"]')!.getAttribute("aria-label");

test("no edit api: no toolbar", () => {
  const { target } = setup();
  expect(target.querySelector('[role="toolbar"]')).toBeNull();
});

test("B, A, V switch tools; Esc returns to select", () => {
  const { target } = setup(fake().api);
  expect(pressed(target)).toContain("Select");
  press("b");
  expect(pressed(target)).toBe("Box");
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
  const el = id ? t.querySelector(`[data-box="${id}"] > button`)! : t.querySelector(".stage")!;
  el.dispatchEvent(new PointerEvent("pointerover", { bubbles: true, clientX: 40, clientY: 50 }));
  press("k");
}
const adds = () => {
  const got: unknown[] = [];
  const f = fake({ apply: async (e) => (got.push(e), fresh()) });
  return { ...f, got };
};

const pick = (f: HTMLElement, kind: string) => { f.querySelector<HTMLButtonElement>(`button[data-kind="${kind}"]`)!.click(); flushSync(); };

test("K over a container: its kinds, then Title focused and the code prefilled from its glob", async () => {
  const { target } = setup(adds().api);
  clickBox(target, "api");
  const f = form(target)!;
  expect(document.activeElement).toBe(f.querySelector('button[data-kind="module"]'));
  pick(f, "usecase");
  await tick();
  expect(document.activeElement).toBe(input(f, "Title"));
  type(input(f, "Title"), "Billing");
  expect(input(f, "Code").value).toBe("apps/api/billing/**");
});

test("Enter adds: apply gets the Add, the id de-clashed", async () => {
  const { api, got } = adds();
  const { target } = setup(api);
  clickBox(target, "api");
  pick(form(target)!, "usecase");
  type(input(form(target)!, "Title"), "Api");
  form(target)!.requestSubmit();
  await tick();
  expect(got).toEqual([{ op: "add", parent: "api", kind: "usecase", id: "api-2", title: "Api", at: ["apps/api/api/**"], before: null }]);
  expect(form(target)).toBeNull();
});

test("Esc closes the form and sends nothing", async () => {
  const { api, got } = adds();
  const { target } = setup(api);
  clickBox(target, "api");
  pick(form(target)!, "usecase");
  type(input(form(target)!, "Title"), "Billing");
  press("Escape", {}, input(form(target)!, "Title"));
  await tick();
  expect(form(target)).toBeNull();
  expect(got).toEqual([]);
});

test("prefill and slug edge cases", () => {
  expect(prefill(["apps/api/**", "libs/x/**"], "Billing")).toBe("apps/api/billing/**");
  expect(prefill(["apps/api/main.ts"], "Billing")).toBe("apps/api/billing/**");
  expect(prefill(["apps/api/src/**/*.ts"], "Billing")).toBe("apps/api/src/billing/**");
  expect(prefill(["apps/api/src/*/x.ts"], "Billing")).toBe("apps/api/src/billing/**");
  expect(prefill([], "Billing")).toBe("billing/**");
  expect(slug("Kelvin")).toBe("elvin"); // non-ASCII is a dash, like Rust
});

// --- draw an arrow; the box menu
const recorder = () => {
  const got: any[] = [];
  const f = fake({ apply: async (e) => (got.push(e), fresh()) });
  return { ...f, got };
};
const ptr = (el: EventTarget, type: string, init: PointerEventInit = {}) => {
  el.dispatchEvent(new PointerEvent(type, { bubbles: true, button: 0, clientX: 10, clientY: 10, ...init }));
  flushSync();
};
const boxEl = (t: HTMLElement, id: string) => t.querySelector(`[data-box="${id}"] > button`)!;
const menuOf = (t: HTMLElement) => t.querySelector('[role="menu"]') as HTMLElement | null;
const item = (t: HTMLElement, name: string) => [...menuOf(t)!.querySelectorAll<HTMLElement>('[role="menuitem"]')].find((b) => b.textContent!.startsWith(name))!;

const far = { clientX: 300, clientY: 300 };
const chipOf = (t: HTMLElement) => t.querySelector<HTMLElement>("[data-chip] button.kind");
const kindItems = (t: HTMLElement) => [...t.querySelectorAll<HTMLElement>('[data-chip] [role="menuitemradio"]')];

test("Arrow tool: drag A to B lights B in gold and sends the guessed kind at once; the chip changes it", async () => {
  const { api, got } = recorder();
  const { target } = setup(api);
  press("a");
  ptr(boxEl(target, "web"), "pointerdown");
  ptr(boxEl(target, "postgres"), "pointermove", far);
  expect(target.querySelector('[data-box="postgres"]')!.classList.contains("target")).toBe(true);
  expect(target.querySelector("[data-band]")).not.toBeNull();
  ptr(boxEl(target, "postgres"), "pointerup", far);
  await tick();
  flushSync();
  expect(got).toEqual([{ op: "arrow", from: "web", to: "postgres", kind: "writes", label: null }]);
  expect(target.querySelector("form")).toBeNull();
  expect(pressed(target)).toBe("Select");
  expect(document.activeElement!.textContent).toBe("writes");
  expect(document.activeElement!.getAttribute("aria-checked")).toBe("true");
  kindItems(target).find((b) => b.textContent === "reads")!.click();
  await tick();
  flushSync();
  expect(got[1]).toEqual({ op: "arrowset", from: "web", to: "postgres", key: "kind", value: "reads" });
  expect(chipOf(target)!.textContent).toBe("reads");
  expect(kindItems(target)).toEqual([]);
  chipOf(target)!.click();
  flushSync();
  expect(kindItems(target).find((b) => b.getAttribute("aria-checked") === "true")!.textContent).toBe("reads");
});

test("the new arrow's kind list closes on Esc, Enter or a click elsewhere and keeps the guess", async () => {
  const { api, got } = recorder();
  const { target } = setup(api);
  const make = async () => {
    press("a");
    ptr(boxEl(target, "web"), "pointerdown");
    ptr(boxEl(target, "api"), "pointerup", far);
    await tick();
    flushSync();
    expect(kindItems(target).length).toBeGreaterThan(0);
  };
  await make();
  press("Escape", {}, document.activeElement!);
  expect(kindItems(target)).toEqual([]);
  expect(chipOf(target)!.textContent).toBe("calls");
  await make();
  press("Enter", {}, document.activeElement!);
  expect(kindItems(target)).toEqual([]);
  await make();
  ptr(stageOf(target), "pointerdown", { clientX: 900, clientY: 700 });
  ptr(window, "pointerup", { clientX: 900, clientY: 700 });
  expect(kindItems(target)).toEqual([]);
  await tick();
  expect(got.map((e) => e.op)).toEqual(["arrow", "arrow", "arrow"]);
});

test("a failed arrow leaves no chip", async () => {
  const { target } = setup(fake({ apply: async () => { throw new Error("no"); } }).api);
  press("a");
  ptr(boxEl(target, "web"), "pointerdown");
  ptr(boxEl(target, "api"), "pointerup", far);
  await tick();
  flushSync();
  expect(chipOf(target)).toBeNull();
});

test("under the arrow tool a corner grip of the selected box still resizes", () => {
  const target = harness(fake().api);
  const slot = (id: string) => target.querySelector(`[data-box="${id}"]`) as HTMLElement;
  const h = px(slot("postgres"), "height");
  select(target, "postgres");
  press("a");
  drawBox(slot("postgres").querySelector('[data-resize="se"]')!, { x: 500, y: 300 }, { x: 500, y: 500 });
  expect(target.querySelector("[data-band]")).toBeNull();
  expect(px(slot("postgres"), "height")).not.toBe(h);
});

test("side dots show on a hovered box under any tool; a drag from one connects without the arrow tool", async () => {
  const { api, got } = recorder();
  const { target } = setup(api);
  const dots = (id: string) => target.querySelectorAll(`[data-box="${id}"] [data-handle]`).length;
  expect(dots("web")).toBe(0);
  boxEl(target, "web").dispatchEvent(new PointerEvent("pointerover", { bubbles: true }));
  flushSync();
  expect(dots("web")).toBe(4);
  press("b");
  expect(dots("web")).toBe(4);
  ptr(target.querySelector('[data-box="web"] [data-handle]')!, "pointerdown");
  ptr(boxEl(target, "api"), "pointermove", far);
  ptr(boxEl(target, "api"), "pointerup", far);
  await tick();
  expect(got).toEqual([{ op: "arrow", from: "web", to: "api", kind: "calls", label: null }]);
  expect(draftOf(target)).toBeNull();
  expect(pressed(target)).toBe("Box");
});

test("Arrow released on its own box cancels; on empty ground a draft there, confirmed as one batch of add and arrow", async () => {
  const { api, got } = recorder();
  const { target } = setup(api);
  press("a");
  ptr(boxEl(target, "web"), "pointerdown");
  ptr(boxEl(target, "web"), "pointerup");
  ptr(boxEl(target, "web"), "pointerdown");
  ptr(boxEl(target, "web"), "pointerup", far);
  expect(draftOf(target)).toBeNull();
  press("a");
  ptr(boxEl(target, "web"), "pointerdown");
  ptr(stageOf(target), "pointermove", far);
  ptr(stageOf(target), "pointerup", far);
  expect(draftOf(target)).not.toBeNull();
  expect(target.querySelector("[data-band]")).not.toBeNull();
  expect(document.activeElement).toBe(nameField(target));
  type(nameField(target), "Ledger");
  press("Enter", {}, nameField(target));
  await tick();
  expect(got).toEqual([{ op: "batch", edits: [
    { op: "add", parent: "system", kind: "box", id: "ledger", title: "Ledger", at: [], before: null },
    { op: "arrow", from: "web", to: "ledger", kind: "uses", label: null },
  ] }]);
  expect(target.querySelector("[data-band]")).toBeNull();
});

test("Esc on the empty draft at an arrow's end leaves nothing", async () => {
  const { api, got } = recorder();
  const { target } = setup(api);
  press("a");
  ptr(boxEl(target, "web"), "pointerdown");
  ptr(stageOf(target), "pointerup", far);
  press("Escape", {}, nameField(target));
  await tick();
  expect(draftOf(target)).toBeNull();
  expect(target.querySelector("[data-band]")).toBeNull();
  expect(got).toEqual([]);
});

test("right-click opens the menu, Esc closes it", () => {
  const { target } = setup(recorder().api);
  boxEl(target, "web").dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true }));
  flushSync();
  expect([...menuOf(target)!.querySelectorAll('[role="menuitem"]')].map((b) => b.firstElementChild!.textContent))
    .toEqual(["Rename", "Kind", "Edit details", "Arrow from here", "Add a box inside", "Move to a group…", "Arrange inside", "Open the code", "Copy", "Copy with what's inside", "Duplicate", "Delete"]);
  expect([...menuOf(target)!.querySelectorAll("kbd")].filter((k) => !k.textContent)).toHaveLength(0);
  press("ArrowDown", {}, document.activeElement!);
  press("Escape", {}, document.activeElement!);
  expect(menuOf(target)).toBeNull();
  for (const id of ["stripe", "api"]) {
    boxEl(target, id).dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true }));
    flushSync();
    expect(menuOf(target)!.textContent, id).not.toContain("Arrange inside");
  }
});

test("Arrange inside lays out the box's inside and says how many boxes moved", () => {
  const { target } = setup(recorder().api);
  boxEl(target, "web").dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true }));
  flushSync();
  ([...menuOf(target)!.querySelectorAll('[role="menuitem"]')].find((b) => b.textContent?.startsWith("Arrange inside")) as HTMLButtonElement).click();
  flushSync();
  expect(menuOf(target)).toBeNull();
  expect(target.querySelector(".arranged")?.textContent).toMatch(/Arranged \d+ box/);
});

test("F2, typing and Enter sets the title", async () => {
  const { api, got } = recorder();
  const { target } = setup(api);
  (boxEl(target, "web") as HTMLElement).click();
  flushSync();
  press("F2");
  const i = target.querySelector('input[aria-label="Rename box"]') as HTMLInputElement;
  type(i, "Storefront");
  press("Enter", {}, i);
  await tick();
  expect(got).toEqual([{ op: "set", id: "web", key: "title", value: "Storefront" }]);
});

test("Move to… on a box nothing can hold says so; the picker filters and picks", async () => {
  const { target } = setup(recorder().api);
  boxEl(target, "api").dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true }));
  flushSync();
  item(target, "Move to").click();
  flushSync();
  expect(target.querySelector('[role="dialog"]')!.textContent).toContain("No box can hold it.");
  const picks: string[] = [];
  const t = document.createElement("div");
  document.body.append(t);
  mount(MovePicker, { target: t, props: { title: "x", x: 0, y: 0, onclose() {}, onpick: (id: string) => picks.push(id),
    options: [{ id: "web", title: "Web", kind: "app" }, { id: "api", title: "Api", kind: "app" }] } });
  flushSync();
  type(t.querySelector("input")!, "ap");
  expect([...t.querySelectorAll("li")].length).toBe(1);
  press("Enter", {}, t.querySelector("input")!);
  expect(picks).toEqual(["api"]);
});

test("parentsFor: a container kind goes under apps and containers, not its parent, itself, its descendants or external", () => {
  const v = fresh();
  const ids = (id: string) => parentsFor(v.boxes, v.kinds, id, "system").map((b) => b.id);
  expect(ids("api/domain-order")).toEqual(expect.arrayContaining(["web", "api-client"]));
  expect(ids("api/domain-order")).not.toContain("api");
  expect(ids("api/domain-order")).not.toContain("stripe");
  expect(ids("api/domain-order")).not.toContain("api/domain-order");
  expect(ids("api")).toEqual([]); // system-level kind: only the root, which is its parent
  expect(ids("web")).not.toContain("web/pages");
});

test("parentsFor: a run block goes into a top-level group, a group back to the root", () => {
  const v = fresh();
  const group = (id: string, parent: string) => ({ ...structuredClone(v.boxes.find((b) => b.id === "api/domain-order")!), id, parent,
    kind: { value: "group" }, title: { value: id } }) as (typeof v.boxes)[number];
  v.boxes.push(group("g", "system"), group("api/g", "api"));
  const ids = (id: string) => parentsFor(v.boxes, v.kinds, id, "system").map((b) => b.id);
  expect(ids("api")).toEqual(["g"]);
  expect(ids("api/g")).toContain("system");
  expect(ids("api/g")).toContain("g");
  expect(ids("api/domain-order")).toContain("api/g");
  expect(ids("api/domain-order")).not.toContain("g");
});

test("Delete of a box with boxes inside asks once in the bar, then sends one batch; a refusal shows", async () => {
  const got: any[] = [];
  const { api } = fake({ apply: async (e) => (got.push(e), Promise.reject(new Error("`api` is in use"))) });
  const { target } = setup(api);
  boxEl(target, "api").dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true }));
  flushSync();
  item(target, "Delete").click();
  flushSync();
  await tick();
  expect(got).toEqual([]);
  const bar = target.querySelector("[data-bar]")!;
  expect(bar.textContent).toContain("Delete Api and 9 boxes inside?");
  expect(document.activeElement!.textContent).toBe("Delete");
  (document.activeElement as HTMLElement).click();
  await tick();
  flushSync();
  expect(got).toHaveLength(1);
  const edits = got[0].edits as any[];
  expect(got[0].op).toBe("batch");
  expect(edits.filter((e) => e.op === "unarrow")).toContainEqual({ op: "unarrow", from: "web", to: "api", kind: "calls" });
  expect(edits.filter((e) => e.op === "unarrow")).toContainEqual({ op: "unarrow", from: "api/root", to: "api/routers", kind: "uses" });
  expect(edits.findIndex((e) => e.op === "remove")).toBe(edits.filter((e) => e.op === "unarrow").length);
  expect(edits.filter((e) => e.op === "remove")).toHaveLength(10);
  expect(edits.at(-1)).toEqual({ op: "remove", id: "api" });
  expect(target.querySelector('[aria-live="polite"]')!.textContent).toBe("`api` is in use");
});

test("Keep or Esc answers no to the delete question", async () => {
  const { api, got } = recorder();
  const { target } = setup(api);
  select(target, "web");
  press("Delete");
  expect(target.querySelector("[data-bar]")!.textContent).toContain("Delete Web and 1 box inside?");
  press("Escape");
  expect(target.querySelector("[data-bar]")!.textContent).not.toContain("inside?");
  press("Delete");
  [...target.querySelectorAll<HTMLElement>("[data-bar] button")].find((b) => b.textContent === "Keep")!.click();
  flushSync();
  press("Delete");
  press("Delete");
  await tick();
  expect(got).toEqual([{ op: "batch", edits: [{ op: "unarrow", from: "web", to: "api", kind: "calls" }, { op: "remove", id: "web/pages" }, { op: "remove", id: "web" }] }]);
});

// --- fix round 1
const select = (t: HTMLElement, id: string) => { (boxEl(t, id) as HTMLElement).click(); flushSync(); };
const rightClick = (t: HTMLElement, id: string) => {
  boxEl(t, id).dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true }));
  flushSync();
};

test("right-click on an unselected box leaves focus on a menu item", () => {
  const { target } = setup(recorder().api);
  rightClick(target, "web");
  expect(document.activeElement!.getAttribute("role")).toBe("menuitem");
});

test("closing the menu with Esc returns focus to the box", () => {
  const { target } = setup(recorder().api);
  rightClick(target, "web");
  press("Escape", {}, document.activeElement!);
  expect(document.activeElement).toBe(boxEl(target, "web"));
});

test("a press outside an open popover closes it", () => {
  const { target } = setup(recorder().api);
  rightClick(target, "web");
  ptr(document.body, "pointerdown");
  expect(menuOf(target)).toBeNull();
});

test("Map-level Move: pick a parent and Move is sent", async () => {
  const { api, got } = recorder();
  const v = fresh();
  v.boxes.find((b) => b.id === "api/application-refund")!.parent = "system"; // a container-kind box on the ground
  const { target } = setup(api, v);
  rightClick(target, "api/application-refund");
  item(target, "Move to").click();
  flushSync();
  const web = [...target.querySelectorAll<HTMLElement>('[role="dialog"] li button')].find((b) => b.textContent!.startsWith("Web"))!;
  web.click();
  await tick();
  expect(got).toEqual([{ op: "move", id: "api/application-refund", parent: "web", before: null }]);
});

test("Backspace and Delete with a box selected remove it with its arrows, one batch each", async () => {
  const { api, got } = recorder();
  const { target } = setup(api);
  select(target, "stripe");
  press("Backspace");
  await tick();
  press("Delete");
  await tick();
  const gone = { op: "batch", edits: [{ op: "unarrow", from: "api", to: "stripe", kind: "calls" }, { op: "remove", id: "stripe" }] };
  expect(got).toEqual([gone, gone]);
});

test("Backspace inside the rename input sends nothing", async () => {
  const { api, got } = recorder();
  const { target } = setup(api);
  select(target, "web");
  press("F2");
  press("Backspace", {}, target.querySelector('input[aria-label="Rename box"]')!);
  await tick();
  expect(got).toEqual([]);
});

test("Backspace with the arrow's chip focused sends nothing more; so does a held key", async () => {
  const { api, got } = recorder();
  const { target } = setup(api);
  select(target, "web");
  press("a");
  ptr(boxEl(target, "web"), "pointerdown");
  ptr(boxEl(target, "api"), "pointermove", far);
  ptr(boxEl(target, "api"), "pointerup", far);
  await tick();
  flushSync();
  const chip = chipOf(target)!;
  chip.focus();
  press("Backspace", {}, chip);
  press("Escape", {}, chip);
  press("Delete", { repeat: true });
  await tick();
  expect(got).toEqual([{ op: "arrow", from: "web", to: "api", kind: "calls", label: null }]);
});

test("pointercancel drops an arrow in flight", () => {
  const { target } = setup(recorder().api);
  press("a");
  ptr(boxEl(target, "web"), "pointerdown");
  ptr(boxEl(target, "api"), "pointermove");
  window.dispatchEvent(new Event("pointercancel"));
  flushSync();
  ptr(boxEl(target, "api"), "pointerup");
  expect(target.querySelector('form[aria-label^="Arrow"]')).toBeNull();
  expect(target.querySelector(".target")).toBeNull();
});

test("without edit, F2, M and Backspace do nothing", () => {
  const { target } = setup();
  select(target, "web");
  for (const k of ["F2", "m", "Backspace"]) press(k);
  expect(target.querySelector("[data-popover]")).toBeNull();
  expect(target.querySelector('input[aria-label="Rename box"]')).toBeNull();
});

const withProposals = (ps: NonNullable<View["proposals"]>) => ({ ...fresh(), proposals: ps });
const two = [
  { n: 3, by: "claude" as const, summary: "add usecase Refund", ids: ["web"], marks: ["web"] },
  { n: 4, by: "reading" as const, summary: "api: tech", ids: ["api"], marks: ["api"] },
  { n: 5, by: "claude" as const, summary: "arrow web uses api", ids: ["api->postgres"], marks: ["api->postgres"] },
];
const btn = (t: HTMLElement, name: string) => [...t.querySelectorAll<HTMLElement>("button")].find((b) => b.textContent === name)!;

test("no proposals: no bar", () => {
  expect(setup(fake().api).target.querySelector('[aria-label="Proposed edits"]')).toBeNull();
});

test("two authors: counts per author; Keep on a row calls keep(n), Drop all calls dropAll", async () => {
  const calls: unknown[] = [];
  const { api } = fake({ keep: async (n) => (calls.push(["keep", n]), fresh()), dropAll: async () => (calls.push("dropAll"), fresh()) });
  const { target } = setup(api, withProposals(two));
  expect(target.querySelector('[aria-label="Proposed edits"]')!.textContent).toContain("Claude proposed 2 edits · AI reading 1");
  btn(target, "Review").click();
  flushSync();
  btn(target.querySelector("section.list")!, "Keep").click();
  await tick();
  btn(target, "Drop all").click();
  await tick();
  expect(calls).toEqual([["keep", 3], "dropAll"]);
});

test("read-only: the bar shows but nothing acts", () => {
  const { target } = setup(undefined, withProposals(two));
  btn(target, "Review").click();
  flushSync();
  expect(target.textContent).toContain("Claude proposed 2 edits");
  for (const n of ["Keep all", "Drop all", "Keep", "Drop"]) expect(btn(target, n)).toBeUndefined();
});

test("a proposed box has the tag chip; hovering a row lights it; its card offers Keep", async () => {
  const calls: unknown[] = [];
  const { target } = setup(fake({ keep: async (n) => (calls.push(n), fresh()) }).api, withProposals(two));
  const web = fresh().boxes.find((b) => b.id === "web")!;
  expect(look("web", [], new Set(), web, "claude")).toMatchObject({ edge: "var(--map-trace)", dashed: true, chips: [{ label: "Claude" }] });
  expect(look("web", [], new Set(), web, "reading").chips.at(-1)!.label).toBe("reading");
  expect(arrowLook(fresh().arrows[0], [], new Set(), "claude")).toMatchObject({ color: "var(--map-trace)", dash: "6 4", label: "Claude" });
  btn(target, "Review").click();
  flushSync();
  const row = target.querySelector('section.list [role="group"]')!;
  row.dispatchEvent(new PointerEvent("pointerenter"));
  flushSync();
  expect(target.querySelector('[data-box="web"].traced')).not.toBeNull();
  row.dispatchEvent(new PointerEvent("pointerleave"));
  flushSync();
  expect(target.querySelector(".traced")).toBeNull();
  select(target, "web");
  btn(target.querySelector("section.details")!, "Keep").click();
  await tick();
  expect(calls).toEqual([3]);
});

const row = (t: HTMLElement, i = 0) => t.querySelectorAll('section.list [role="group"]')[i];
const fire = (el: Element, type: string) => { el.dispatchEvent(new (type.startsWith("pointer") ? PointerEvent : FocusEvent)(type, { bubbles: true })); flushSync(); };
const review = (t: HTMLElement) => { btn(t, "Review").click(); flushSync(); };

test("pointerenter, focusin, pointerleave, focusout leaves nothing lit", () => {
  const { target } = setup(fake().api, withProposals(two));
  review(target);
  const r = row(target)!;
  for (const e of ["pointerenter", "focusin", "pointerleave", "focusout"]) fire(r, e);
  expect(target.querySelector(".traced, .faded")).toBeNull();
});

test("Keep on the hovered row clears the light", async () => {
  const left = withProposals(two.slice(1));
  const target = document.createElement("div");
  document.body.append(target);
  mount(MapHarness, { target, props: { start: withProposals(two), edit: fake({ keep: async () => left }).api, size } });
  flushSync();
  review(target);
  fire(row(target)!, "pointerenter");
  expect(target.querySelector(".traced")).not.toBeNull();
  btn(row(target) as HTMLElement, "Keep").click();
  await tick();
  flushSync();
  expect(target.querySelectorAll('section.list [role="group"]').length).toBe(2);
  expect(target.querySelector(".traced")).toBeNull();
});

test("closing the list while a row is hovered clears the light", () => {
  const { target } = setup(fake().api, withProposals(two));
  review(target);
  fire(row(target)!, "pointerenter");
  btn(target, "Review").click();
  flushSync();
  expect(target.querySelector(".traced")).toBeNull();
});

test("a hover while the search lit a path gives the search path back", () => {
  const { target } = setup(fake().api, withProposals(two));
  const input = target.querySelector('input[aria-label="Search the map"]') as HTMLInputElement;
  input.focus();
  input.value = "web to postgres";
  input.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  flushSync();
  const lit = () => [...target.querySelectorAll(".traced")].map((e) => (e as HTMLElement).dataset.box).sort();
  const before = lit();
  expect(before).toContain("postgres");
  review(target);
  fire(row(target, 1)!, "pointerenter");
  expect(lit()).toEqual(["api"]);
  fire(row(target, 1)!, "pointerleave");
  expect(lit()).toEqual(before);
});

test("a proposed arrow is dashed in the map", () => {
  const { target } = setup(fake().api, withProposals(two));
  const path = target.querySelector('[data-arrow="api->postgres"] path[stroke-dasharray], [data-arrow="api->postgres"] path[style*="dash"]');
  expect(path).not.toBeNull();
});

// --- final review
const harness = (api: EditApi, start: View = fresh()) => {
  const target = document.createElement("div");
  document.body.append(target);
  mount(MapHarness, { target, props: { start, edit: api, size } });
  flushSync();
  return target;
};

test("a new view for the selected box keeps focus in the chat and the paths expanded", async () => {
  const start = fresh();
  start.boxes.find((b) => b.id === "web")!.madeOf = Array.from({ length: 8 }, (_, i) => `apps/web/p${i}.ts`);
  const next = structuredClone(start);
  const target = harness(fake({ undo: async () => next }).api, start);
  select(target, "web");
  btn(target.querySelector("section.details")!, "Show all 8").click();
  flushSync();
  const area = document.createElement("textarea");
  document.body.append(area);
  area.focus();
  (target.querySelector('[aria-label="Undo"]') as HTMLElement).click();
  await tick();
  flushSync();
  expect(document.activeElement).toBe(area);
  expect(target.querySelector("section.details")!.textContent).toContain("Show fewer");
});

test("a second undo while the first is pending is ignored", async () => {
  let release!: () => void;
  let n = 0;
  const v = fresh();
  const { api } = fake({ undo: () => (n++, new Promise<View>((r) => (release = () => r(v)))) });
  setup(api);
  press("z", { metaKey: true });
  press("z", { metaKey: true });
  expect(n).toBe(1);
  release();
  await tick();
  press("z", { metaKey: true });
  expect(n).toBe(2);
});

test("a popover on a box the new view lacks is cancelled", async () => {
  const gone = fresh();
  gone.boxes = gone.boxes.filter((b) => b.id !== "web" && b.parent !== "web");
  gone.arrows = gone.arrows.filter((a) => !a.from.startsWith("web") && !a.to.startsWith("web"));
  const target = harness(fake({ undo: async () => gone }).api);
  select(target, "web");
  press("F2");
  expect(target.querySelector('input[aria-label="Rename box"]')).not.toBeNull();
  (target.querySelector('[aria-label="Undo"]') as HTMLElement).click();
  await tick();
  flushSync();
  expect(target.querySelector('input[aria-label="Rename box"]')).toBeNull();
});

test("a move proposal marks only the moved box, hover still lights both", () => {
  const p = { n: 1, by: "claude" as const, summary: "move web → api", ids: ["web", "api"], marks: ["web"] };
  expect([...proposed([p]).keys()]).toEqual(["web"]);
});

// --- draw a box
const stageOf = (t: HTMLElement) => t.querySelector(".stage") as HTMLElement;
const draftOf = (t: HTMLElement) => t.querySelector("[data-draft]") as HTMLElement | null;
const nameField = (t: HTMLElement) => t.querySelector('input[aria-label="Name the box"]') as HTMLInputElement;
const px = (el: HTMLElement, k: "left" | "top" | "width" | "height") => parseFloat(el.style[k]);
const scaleOf = (t: HTMLElement) => parseFloat((t.querySelector(".world") as HTMLElement).style.transform.match(/scale\(([^)]+)\)/)![1]);
function drawBox(from: Element, a: { x: number; y: number }, b = a, init: PointerEventInit = {}) {
  ptr(from, "pointerdown", { clientX: a.x, clientY: a.y, ...init });
  if (b !== a) ptr(window, "pointermove", { clientX: b.x, clientY: b.y, ...init });
  ptr(window, "pointerup", { clientX: b.x, clientY: b.y, ...init });
}
const GRID = 24;
const withBox = (id: string, title: string, parent = "system") => {
  const v = fresh();
  v.boxes.push({ ...structuredClone(v.boxes.find((b) => b.id === "postgres")!), id, parent, kind: { value: "box" } as never, title: { value: title } as never, madeOf: [] });
  v.placed.frames[id] = { x: 96, y: 600, w: 248, h: 152 };
  return v;
};

test("Box tool: a drag draws a draft at that size, its name in edit mode; a click places the default", () => {
  const { target } = setup(recorder().api);
  press("b");
  const k = scaleOf(target);
  drawBox(stageOf(target), { x: 100, y: 100 }, { x: 100 + 16 * GRID * k, y: 100 + 4 * GRID * k });
  const d = draftOf(target)!;
  expect(Math.abs(px(d, "width") - 16 * GRID)).toBeLessThanOrEqual(GRID);
  expect(Math.abs(px(d, "height") - 4 * GRID)).toBeLessThanOrEqual(GRID);
  expect(px(d, "left") % GRID).toBe(0);
  expect(d.textContent).toContain("no kind yet");
  expect(document.activeElement).toBe(nameField(target));
  press("Escape", {}, nameField(target));
  expect(draftOf(target)).toBeNull();
  press("b");
  drawBox(stageOf(target), { x: 300, y: 300 });
  expect([px(draftOf(target)!, "width"), px(draftOf(target)!, "height")]).toEqual([10 * GRID, 6 * GRID]);
});

test("double-click on empty ground in select makes a draft there; on a box it does not", () => {
  const { target } = setup(recorder().api);
  boxEl(target, "postgres").dispatchEvent(new MouseEvent("dblclick", { bubbles: true, clientX: 300, clientY: 300 }));
  flushSync();
  expect(draftOf(target)).toBeNull();
  stageOf(target).dispatchEvent(new MouseEvent("dblclick", { bubbles: true, clientX: 300, clientY: 300 }));
  flushSync();
  expect(document.activeElement).toBe(nameField(target));
  expect(px(draftOf(target)!, "width")).toBe(10 * GRID);
});

test("Enter sends add with kind box and the title, inside the open box under it", async () => {
  const { api, got } = recorder();
  const target = document.createElement("div");
  document.body.append(target);
  const v = fresh(), camera = new Camera();
  const m = mount(Map, { target, props: { view: v, size, edit: api, camera } });
  flushSync();
  camera.to(fitTo(worldFrame("api", v), size), 0);
  m.tick(800);
  flushSync();
  const open = target.querySelector('[data-box="api"][data-open]') as HTMLElement;
  press("b");
  drawBox(open.querySelector("button")!, { x: 400, y: 400 });
  expect(open.querySelector("[data-draft]")).not.toBeNull();
  type(nameField(target), "Billing");
  press("Enter", {}, nameField(target));
  await tick();
  expect(got).toEqual([{ op: "add", parent: open.dataset.box, kind: "box", id: "billing", title: "Billing", at: [], before: null }]);
  expect(draftOf(target)).toBeNull();
});

test("the confirmed box lands where and as big as it was drawn", async () => {
  vi.stubGlobal("localStorage", { getItem: () => null, setItem: () => {}, removeItem: () => {} });
  const target = harness(fake({ apply: async () => withBox("billing", "Billing") }).api);
  press("b");
  const k = scaleOf(target);
  drawBox(stageOf(target), { x: 40, y: 600 }, { x: 40 + 12 * GRID * k, y: 600 + 5 * GRID * k });
  const d = draftOf(target)!, drawn = (["left", "top", "width", "height"] as const).map((p) => px(d, p));
  type(nameField(target), "Billing");
  press("Enter", {}, nameField(target));
  await tick();
  flushSync();
  const slot = target.querySelector('[data-box="billing"]') as HTMLElement;
  expect((["left", "top", "width", "height"] as const).map((p) => px(slot, p))).toEqual(drawn);
  vi.unstubAllGlobals();
});

test("Tab confirms and opens the new box for its kind", async () => {
  const got: unknown[] = [];
  const target = harness(fake({ apply: async (e) => (got.push(e), withBox("billing", "Billing")) }).api);
  stageOf(target).dispatchEvent(new MouseEvent("dblclick", { bubbles: true, clientX: 40, clientY: 600 }));
  flushSync();
  type(nameField(target), "Billing");
  press("Tab", {}, nameField(target));
  await tick();
  flushSync();
  expect(got).toHaveLength(1);
  expect(target.querySelector("section.details")!.textContent).toContain("Billing");
});

test("Esc or a click away on an empty draft sends nothing; a click away with a name confirms", async () => {
  const { api, got } = recorder();
  const { target } = setup(api);
  const dbl = () => {
    stageOf(target).dispatchEvent(new MouseEvent("dblclick", { bubbles: true, clientX: 300, clientY: 300 }));
    flushSync();
  };
  dbl();
  press("Escape", {}, nameField(target));
  dbl();
  ptr(stageOf(target), "pointerdown", { clientX: 900, clientY: 700 });
  ptr(window, "pointerup", { clientX: 900, clientY: 700 });
  await tick();
  expect(draftOf(target)).toBeNull();
  expect(got).toEqual([]);
  dbl();
  type(nameField(target), "Ledger");
  ptr(stageOf(target), "pointerdown", { clientX: 900, clientY: 700 });
  ptr(window, "pointerup", { clientX: 900, clientY: 700 });
  await tick();
  expect(got).toMatchObject([{ op: "add", kind: "box", title: "Ledger", id: "ledger" }]);
});

test("box goes back to select after one shape; Shift keeps it for that one, a double-click locks it", () => {
  const { target } = setup(recorder().api);
  const tool = (name: string) => target.querySelector(`[role="toolbar"] button[aria-label="${name}"]`) as HTMLElement;
  const once = (init: PointerEventInit = {}) => {
    drawBox(stageOf(target), { x: 300, y: 300 }, { x: 300, y: 300 }, init);
    press("Escape", {}, nameField(target));
  };
  press("b");
  once();
  expect(pressed(target)).toBe("Select");
  press("b");
  once({ shiftKey: true });
  expect(pressed(target)).toBe("Box");
  expect(tool("Box").hasAttribute("data-locked")).toBe(false);
  once();
  expect(pressed(target)).toBe("Select");
  tool("Select").click();
  flushSync();
  tool("Box").click();
  tool("Box").click();
  tool("Box").dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
  flushSync();
  expect(tool("Box").hasAttribute("data-locked")).toBe(true);
  once();
  expect(pressed(target)).toBe("Box");
  press("f");
  expect(pressed(target)).toBe("Frame");
  expect(tool("Frame").hasAttribute("data-locked")).toBe(false);
});

test("a corner handle resizes the box, nudges its neighbour, and one undo puts both back", () => {
  vi.stubGlobal("localStorage", { getItem: () => null, setItem: () => {}, removeItem: () => {} });
  const target = document.createElement("div");
  document.body.append(target);
  const m = mount(MapHarness, { target, props: { start: fresh(), edit: fake().api, size } });
  flushSync();
  const glide = () => (m.tick(performance.now() + 10_000), flushSync());
  const slot = (id: string) => target.querySelector(`[data-box="${id}"]`) as HTMLElement;
  const k = scaleOf(target);
  const [h, top] = [px(slot("postgres"), "height"), px(slot("stripe"), "top")];
  select(target, "postgres");
  drawBox(slot("postgres").querySelector('[data-resize="se"]')!, { x: 500, y: 300 }, { x: 500, y: 300 + 150 * k });
  glide();
  expect(px(slot("postgres"), "height")).toBe(Math.round((h + 150) / GRID) * GRID);
  expect(px(slot("stripe"), "top")).toBeGreaterThan(top);
  press("z", { metaKey: true });
  glide();
  expect(px(slot("postgres"), "height")).toBe(h);
  expect(px(slot("stripe"), "top")).toBe(top);
  vi.unstubAllGlobals();
});

test("Shift keeps the arrow tool for one more arrow without locking it", async () => {
  const { target } = setup(recorder().api);
  press("a");
  ptr(boxEl(target, "web"), "pointerdown");
  ptr(boxEl(target, "api"), "pointerup", { ...far, shiftKey: true });
  await tick();
  expect(pressed(target)).toBe("Arrow");
  ptr(boxEl(target, "web"), "pointerdown");
  ptr(boxEl(target, "postgres"), "pointerup", far);
  expect(pressed(target)).toBe("Select");
});

test("Esc during a corner resize cancels it and commits nothing", () => {
  const target = harness(fake().api);
  const slot = (id: string) => target.querySelector(`[data-box="${id}"]`) as HTMLElement;
  const h = px(slot("postgres"), "height");
  select(target, "postgres");
  ptr(slot("postgres").querySelector('[data-resize="se"]')!, "pointerdown", { clientX: 500, clientY: 300 });
  ptr(window, "pointermove", { clientX: 500, clientY: 500 });
  expect(px(slot("postgres"), "height")).not.toBe(h);
  press("Escape");
  expect(px(slot("postgres"), "height")).toBe(h);
  ptr(window, "pointerup", { clientX: 500, clientY: 500 });
  expect(px(slot("postgres"), "height")).toBe(h);
});

test("confirming a draft while another edit is saving keeps it open with its name", async () => {
  let done!: (v: View) => void;
  const got: unknown[] = [];
  const api = fake({ apply: (e) => (got.push(e), got.length === 1 ? new Promise<View>((r) => (done = r)) : Promise.resolve(fresh())) }).api;
  const { target } = setup(api);
  select(target, "stripe");
  press("Delete");
  stageOf(target).dispatchEvent(new MouseEvent("dblclick", { bubbles: true, clientX: 300, clientY: 300 }));
  flushSync();
  type(nameField(target), "Ledger");
  press("Enter", {}, nameField(target));
  expect(nameField(target).value).toBe("Ledger");
  done(fresh());
  await tick();
  press("Enter", {}, nameField(target));
  await tick();
  expect(got).toHaveLength(2);
  expect(draftOf(target)).toBeNull();
});

test("the window losing focus leaves a draft open", () => {
  const { target } = setup(recorder().api);
  stageOf(target).dispatchEvent(new MouseEvent("dblclick", { bubbles: true, clientX: 300, clientY: 300 }));
  flushSync();
  type(nameField(target), "Ledger");
  nameField(target).dispatchEvent(new FocusEvent("blur"));
  window.dispatchEvent(new Event("blur"));
  flushSync();
  expect(nameField(target).value).toBe("Ledger");
});

test("double-clicking an arrow or its label makes no draft", () => {
  const { target } = setup(recorder().api);
  select(target, "web");
  const label = target.querySelector("[data-label]");
  for (const el of [target.querySelector("[data-arrow]"), label]) {
    expect(el).not.toBeNull();
    el!.dispatchEvent(new MouseEvent("dblclick", { bubbles: true, clientX: 300, clientY: 300 }));
    flushSync();
  }
  expect(draftOf(target)).toBeNull();
});

// --- several boxes
const barOf = (t: HTMLElement) => t.querySelector<HTMLElement>("[data-bar]");
const barButton = (t: HTMLElement, name: string) => [...barOf(t)!.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent!.startsWith(name))!;
const shiftClick = (t: HTMLElement, id: string) => {
  boxEl(t, id).dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, shiftKey: true }));
  flushSync();
};
const picked = (t: HTMLElement) => [...t.querySelectorAll<HTMLElement>(".slot.picked")].map((e) => e.dataset.box).sort();
const marquee = (t: HTMLElement, a: { x: number; y: number }, b: { x: number; y: number }, init: PointerEventInit = {}) => {
  ptr(stageOf(t), "pointerdown", { clientX: a.x, clientY: a.y, ...init });
  ptr(window, "pointermove", { clientX: b.x, clientY: b.y, ...init });
  const box = t.querySelector<HTMLElement>("[data-marquee]");
  ptr(window, "pointerup", { clientX: b.x, clientY: b.y, ...init });
  return box;
};

test("a select-drag on the ground draws a dashed marquee and selects what it encloses; a click on the ground clears", () => {
  const { target } = setup(recorder().api);
  const m = marquee(target, { x: 120, y: 220 }, { x: 880, y: 560 });
  expect(m).not.toBeNull();
  expect(target.querySelector("[data-marquee]")).toBeNull();
  expect(picked(target)).toEqual(["api", "api-client", "postgres", "stripe", "web"]);
  expect(barOf(target)!.textContent).toContain("5 boxes");
  ptr(stageOf(target), "pointerdown", { clientX: 950, clientY: 750 });
  ptr(window, "pointerup", { clientX: 950, clientY: 750 });
  expect(picked(target)).toEqual([]);
  expect(barOf(target)).toBeNull();
});

test("Shift-drag adds to the selection; Esc clears it", () => {
  const { target } = setup(recorder().api);
  marquee(target, { x: 140, y: 240 }, { x: 360, y: 380 });
  expect(target.querySelector("section.details")!.textContent).toContain("Web");
  marquee(target, { x: 640, y: 240 }, { x: 850, y: 380 }, { shiftKey: true });
  expect(picked(target)).toEqual(["postgres", "web"]);
  press("Escape");
  expect(picked(target)).toEqual([]);
});

test("read-only: a drag on the ground still pans", () => {
  const { target } = setup();
  const before = (target.querySelector(".world") as HTMLElement).style.transform;
  expect(marquee(target, { x: 120, y: 220 }, { x: 400, y: 400 })).toBeNull();
  expect((target.querySelector(".world") as HTMLElement).style.transform).not.toBe(before);
});

test("Shift-click adds and removes; back to one box it is the plain selection", () => {
  const { target } = setup(recorder().api);
  select(target, "web");
  shiftClick(target, "postgres");
  expect(picked(target)).toEqual(["postgres", "web"]);
  expect(target.querySelector("section.details")).toBeNull();
  expect(barOf(target)!.textContent).toContain("2 boxes");
  shiftClick(target, "stripe");
  expect(barOf(target)!.textContent).toContain("3 boxes");
  shiftClick(target, "postgres");
  shiftClick(target, "stripe");
  expect(picked(target)).toEqual([]);
  expect(target.querySelector("section.details")!.textContent).toContain("Web");
  expect(barOf(target)!.textContent).not.toContain("boxes");
});

test("two boxes selected and A connects the first to the second", async () => {
  const { api, got } = recorder();
  const { target } = setup(api);
  select(target, "web");
  shiftClick(target, "postgres");
  press("a");
  await tick();
  expect(got).toEqual([{ op: "arrow", from: "web", to: "postgres", kind: "writes", label: null }]);
  expect(pressed(target)).toBe("Select");
});

test("the bar of several boxes sets their kind in one batch", async () => {
  const { api, got } = recorder();
  const { target } = setup(api);
  select(target, "postgres");
  shiftClick(target, "stripe");
  expect(barButton(target, "Kind").textContent).toContain("Kind");
  barButton(target, "Kind").click();
  flushSync();
  target.querySelector<HTMLButtonElement>('[data-kinds] [data-kind="queue"]')!.click();
  await tick();
  expect(got).toEqual([{ op: "batch", edits: [
    { op: "set", id: "postgres", key: "kind", value: "queue" }, { op: "set", id: "stripe", key: "kind", value: "queue" },
  ] }]);
});

const grouped = (ids: string[]) => {
  const v = fresh();
  v.boxes.push({ ...structuredClone(v.boxes.find((b) => b.id === "postgres")!), id: "group", kind: { value: "group" } as never, title: { value: "Group" } as never, madeOf: [] });
  for (const id of ids) v.boxes.find((b) => b.id === id)!.parent = "group";
  v.placed.frames.group = { x: 900, y: 0, w: 400, h: 400 };
  v.placed.inner.group = [400, 400];
  return v;
};

test("Group puts the boxes in a new group in their parent, one batch, its title then in edit mode", async () => {
  const got: any[] = [];
  const target = harness(fake({ apply: async (e) => (got.push(e), grouped(["postgres", "stripe"])) }).api);
  select(target, "postgres");
  shiftClick(target, "stripe");
  barButton(target, "Group").click();
  await tick();
  flushSync();
  expect(got).toEqual([{ op: "batch", edits: [
    { op: "add", parent: "system", kind: "group", id: "group", title: "Group", at: [], before: null },
    { op: "move", id: "postgres", parent: "group", before: null }, { op: "move", id: "stripe", parent: "group", before: null },
  ] }]);
  const rename = target.querySelector<HTMLInputElement>('input[aria-label="Rename box"]')!;
  expect(rename.value).toBe("Group");
  expect(document.activeElement).toBe(rename);
  type(rename, "Data");
  press("Enter", {}, rename);
  await tick();
  expect(got[1]).toEqual({ op: "set", id: "group", key: "title", value: "Data" });
});

test("Group is disabled for boxes in different places", () => {
  const target = document.createElement("div");
  document.body.append(target);
  const v = fresh(), camera = new Camera();
  const m = mount(Map, { target, props: { view: v, size, edit: recorder().api, camera } });
  flushSync();
  camera.to(fitTo(worldFrame("api", v), size), 0);
  m.tick(800);
  flushSync();
  select(target, "api/routers");
  shiftClick(target, "postgres");
  expect(barOf(target)!.textContent).toContain("2 boxes");
  const g = barButton(target, "Group");
  expect(g.disabled).toBe(true);
  expect(g.title).toBe("These boxes are in different places");
});

test("Delete on several boxes is one batch with their arrows and what is inside, asked first", async () => {
  const { api, got } = recorder();
  const { target } = setup(api);
  select(target, "web");
  shiftClick(target, "stripe");
  barButton(target, "Delete").click();
  flushSync();
  expect(barOf(target)!.textContent).toContain("Delete 2 boxes and 1 box inside?");
  barButton(target, "Delete").click();
  await tick();
  expect(got).toEqual([{ op: "batch", edits: [
    { op: "unarrow", from: "api", to: "stripe", kind: "calls" }, { op: "unarrow", from: "web", to: "api", kind: "calls" },
    { op: "remove", id: "web/pages" }, { op: "remove", id: "web" }, { op: "remove", id: "stripe" },
  ] }]);
});

test("Arrange these lays out the selection, moves its boxes and says so", () => {
  const v = fresh();
  v.placed.frames.stripe = { ...v.placed.frames.stripe, x: v.placed.frames.web.x, y: v.placed.frames.stripe.y + 200 };
  const target = document.createElement("div");
  document.body.append(target);
  const m = mount(MapHarness, { target, props: { start: v, edit: recorder().api, size } });
  flushSync();
  const slot = (id: string) => target.querySelector(`[data-box="${id}"]`) as HTMLElement;
  const at = () => ["web", "postgres", "stripe"].map((id) => [px(slot(id), "left"), px(slot(id), "top")]);
  const before = at();
  shiftClick(target, "postgres");
  shiftClick(target, "stripe");
  shiftClick(target, "web");
  barButton(target, "Arrange these").click();
  m.tick(performance.now() + 10_000);
  flushSync();
  expect(at()).not.toEqual(before);
  expect(target.querySelector(".arranged")?.textContent).toMatch(/Arranged [1-9]\d* box/);
});

test("Row lines the boxes up at one height, Column at one left edge", () => {
  const target = document.createElement("div");
  document.body.append(target);
  const m = mount(MapHarness, { target, props: { start: fresh(), edit: recorder().api, size } });
  flushSync();
  const glide = () => (m.tick(performance.now() + 10_000), flushSync());
  const slot = (id: string) => target.querySelector(`[data-box="${id}"]`) as HTMLElement;
  select(target, "postgres");
  shiftClick(target, "stripe");
  barButton(target, "Row").click();
  glide();
  expect(px(slot("stripe"), "top")).toBe(px(slot("postgres"), "top"));
  expect(px(slot("stripe"), "left")).toBeGreaterThan(px(slot("postgres"), "left"));
  barButton(target, "Column").click();
  glide();
  expect(px(slot("stripe"), "left")).toBe(px(slot("postgres"), "left"));
});

test("right-click on the ground: Paste waits, New box here makes a draft there, Arrange this group", () => {
  const { target } = setup(recorder().api);
  stageOf(target).dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 950, clientY: 750 }));
  flushSync();
  const items = [...menuOf(target)!.querySelectorAll<HTMLButtonElement>('[role="menuitem"]')];
  expect(items.map((b) => b.firstElementChild!.textContent)).toEqual(["Paste", "New box here", "Arrange this group"]);
  expect(items[0].disabled).toBe(true);
  expect(document.activeElement).toBe(items[1]);
  items[1].click();
  flushSync();
  expect(menuOf(target)).toBeNull();
  expect(document.activeElement).toBe(nameField(target));
  press("Escape", {}, nameField(target));
  stageOf(target).dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 950, clientY: 750 }));
  flushSync();
  item(target, "Arrange this group").click();
  flushSync();
  expect(target.querySelector(".arranged")).not.toBeNull();
});

test("Frame tool: a drag draws a group around the boxes it encloses, one batch", async () => {
  const { api, got } = recorder();
  const { target } = setup(api);
  press("f");
  drawBox(stageOf(target), { x: 640, y: 250 }, { x: 850, y: 550 });
  await tick();
  expect(got).toEqual([{ op: "batch", edits: [
    { op: "add", parent: "system", kind: "group", id: "group", title: "Group", at: [], before: null },
    { op: "move", id: "postgres", parent: "group", before: null }, { op: "move", id: "stripe", parent: "group", before: null },
  ] }]);
  expect(pressed(target)).toBe("Select");
});

test("E with a box selected opens its panel and focuses it; read-only, the card stays", async () => {
  const { target } = setup(recorder().api);
  select(target, "web");
  (document.activeElement as HTMLElement | null)?.blur();
  press("e");
  await tick();
  expect(target.querySelector("section.details")).toBeNull();
  expect(document.activeElement).toBe(target.querySelector('[data-panel="box"]'));
  press("Escape", {}, target.querySelector('[data-panel="box"]')!);
  target.querySelector<HTMLElement>('section.details [aria-label="Edit details"]')!.click();
  flushSync();
  expect(target.querySelector('[data-panel="box"]')).not.toBeNull();
  const ro = setup().target;
  select(ro, "web");
  expect(ro.querySelector("section.details")).not.toBeNull();
  expect(ro.querySelector('[aria-label="Edit details"]')).toBeNull();
});

// --- an arrow: select, bar, reconnect, label and note
const hitOf = (t: HTMLElement, id: string) => t.querySelector<SVGElement>(`[data-arrow="${id}"] [data-hit]`)!;
const pickArrow = (t: HTMLElement, id: string) => {
  ptr(hitOf(t, id), "pointerdown");
  ptr(window, "pointerup");
};
const arrowBar = (t: HTMLElement) => t.querySelector<HTMLElement>('[data-bar][aria-label^="Arrow"]');
const arrowButton = (t: HTMLElement, name: string) => [...arrowBar(t)!.querySelectorAll<HTMLButtonElement>(":scope > button")].find((b) => b.textContent!.startsWith(name));
const typeIn = (el: HTMLInputElement, v: string) => {
  el.value = v;
  el.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
};
const withArrow = (id: string, over: Record<string, unknown>) => {
  const v = fresh();
  Object.assign(v.arrows.find((a) => a.id === id)!, over);
  return v;
};

test("a click on an arrow picks it: gold, a handle on each end, and its bar; Esc lets go", () => {
  const { target } = setup(recorder().api);
  expect(arrowBar(target)).toBeNull();
  pickArrow(target, "web->api");
  expect(target.querySelector('[data-arrow="web->api"] [data-state]')!.getAttribute("data-state")).toBe("picked");
  expect([...target.querySelectorAll('[data-label="web->api"] [data-end]')].map((e) => e.getAttribute("data-end"))).toEqual(["from", "to"]);
  const bar = arrowBar(target)!;
  expect(bar.getAttribute("aria-label")).toBe("Arrow Web → Api");
  expect(bar.querySelector("[data-chip] button.kind")!.textContent).toBe("calls");
  expect([...bar.querySelectorAll(":scope > button")].map((b) => b.textContent)).toEqual(["Note", "Context", "Flip", "Details", "Delete"]);
  press("Escape");
  expect(arrowBar(target)).toBeNull();
  expect(target.querySelector('[data-state="picked"]')).toBeNull();
});

test("the arrow bar's kind sends arrowset kind; flip and delete send theirs; Backspace deletes too", async () => {
  const { api, got } = recorder();
  const { target } = setup(api);
  pickArrow(target, "web->api");
  arrowBar(target)!.querySelector<HTMLElement>("[data-chip] button.kind")!.click();
  flushSync();
  kindItems(target).find((b) => b.textContent === "sends")!.click();
  await tick();
  arrowButton(target, "Flip")!.click();
  await tick();
  pickArrow(target, "api->postgres");
  arrowButton(target, "Delete")!.click();
  await tick();
  pickArrow(target, "api->stripe");
  press("Backspace");
  await tick();
  expect(got).toEqual([
    { op: "arrowset", from: "web", to: "api", key: "kind", value: "sends" },
    { op: "arrowset", from: "web", to: "api", key: "flip", value: null },
    { op: "unarrow", from: "api", to: "postgres", kind: "reads" },
    { op: "unarrow", from: "api", to: "stripe", kind: "calls" },
  ]);
  expect(arrowBar(target)).toBeNull();
});

test("Note opens a field on the arrow, one line of at most 80, and Enter sends arrowset note", async () => {
  const { api, got } = recorder();
  const { target } = setup(api);
  pickArrow(target, "web->api");
  arrowButton(target, "Note")!.click();
  flushSync();
  const field = target.querySelector<HTMLInputElement>('[data-arrow-text="note"]')!;
  expect(field.maxLength).toBe(80);
  expect(document.activeElement).toBe(field);
  typeIn(field, "  retries twice ");
  press("Enter", {}, field);
  await tick();
  expect(got).toEqual([{ op: "arrowset", from: "web", to: "api", key: "note", value: "retries twice" }]);
});

test("a double-click on an arrow edits its label in place; Esc leaves it", async () => {
  const { api, got } = recorder();
  const { target } = setup(api);
  const dbl = () => {
    pickArrow(target, "web->api");
    hitOf(target, "web->api").dispatchEvent(new MouseEvent("dblclick", { bubbles: true, cancelable: true }));
    flushSync();
    return target.querySelector<HTMLInputElement>('[data-arrow-text="label"]')!;
  };
  let field = dbl();
  expect(field.value).toBe("POST /checkout");
  press("Escape", {}, field);
  expect(target.querySelector("[data-arrow-text]")).toBeNull();
  field = dbl();
  typeIn(field, "POST /orders");
  press("Enter", {}, field);
  await tick();
  expect(got).toEqual([{ op: "arrowset", from: "web", to: "api", key: "label", value: "POST /orders" }]);
  expect(draftOf(target)).toBeNull();
});

test("dragging an end handle to another box sends arrowset to or from", async () => {
  const { api, got } = recorder();
  const { target } = setup(api);
  const drag = (end: "from" | "to", onto: string) => {
    pickArrow(target, "api->postgres");
    ptr(target.querySelector(`[data-end="${end}"]`)!, "pointerdown");
    ptr(boxEl(target, onto), "pointermove", far);
    expect(target.querySelector(`[data-box="${onto}"]`)!.classList.contains("target")).toBe(true);
    ptr(boxEl(target, onto), "pointerup", far);
  };
  drag("to", "stripe");
  await tick();
  drag("from", "web");
  await tick();
  expect(got).toEqual([
    { op: "arrowset", from: "api", to: "postgres", key: "to", value: "stripe" },
    { op: "arrowset", from: "api", to: "postgres", key: "from", value: "web" },
  ]);
});

test("a bundle is read-only: its bar lists its parts, and picking one selects it there", async () => {
  const { api, got } = recorder();
  const v = withArrow("api->stripe", {
    parts: [
      { id: "api/infra-stripe->stripe", parent: "system", from: "api/infra-stripe", to: "stripe", kind: "calls", count: 1, kinds: ["calls"] },
      { id: "api/application-refund->stripe", parent: "system", from: "api/application-refund", to: "stripe", kind: "sends", count: 1, kinds: ["sends"] },
    ],
  });
  const { target } = setup({ ...api, apply: async (e) => (got.push(e), v) }, v);
  pickArrow(target, "api->stripe");
  expect(target.querySelector("[data-end]")).toBeNull();
  const bar = arrowBar(target)!;
  expect(bar.querySelector("[data-chip]")).toBeNull();
  expect([...bar.querySelectorAll("[data-part]")].map((b) => b.textContent)).toEqual(["callsStripe → Stripe", "sendsRefund → Stripe"]);
  bar.querySelector<HTMLElement>('[data-part="1"]')!.click();
  flushSync();
  expect(arrowBar(target)!.querySelector("[data-chip] button.kind")!.textContent).toBe("sends");
  arrowButton(target, "Note")!.click();
  flushSync();
  const field = target.querySelector<HTMLInputElement>('[data-arrow-text="note"]')!;
  typeIn(field, "refunds go out as events");
  press("Enter", {}, field);
  await tick();
  expect(got).toEqual([{ op: "arrowset", from: "api/application-refund", to: "stripe", key: "note", value: "refunds go out as events" }]);
});

test("a drift arrow offers Keep it in ig/ first; a change to it is written with it, in one batch", async () => {
  const { api, got } = recorder();
  const v = withArrow("web->api", { origin: "drift" });
  const { target } = setup({ ...api, apply: async (e) => (got.push(e), v) }, v);
  pickArrow(target, "web->api");
  const buttons = [...arrowBar(target)!.querySelectorAll(":scope > button")].map((b) => b.textContent);
  expect(buttons[0]).toBe("Keep it in ig/");
  expect(buttons).not.toContain("Delete");
  arrowButton(target, "Keep it in ig/")!.click();
  await tick();
  arrowButton(target, "Flip")!.click();
  await tick();
  const written = { op: "arrow", from: "web", to: "api", kind: "calls", label: "POST /checkout" };
  expect(got).toEqual([
    { op: "batch", edits: [written] },
    { op: "batch", edits: [written, { op: "arrowset", from: "web", to: "api", key: "flip", value: null }] },
  ]);
});

test("the note shows under the label, cut to the arrow's length; context never shows; Context has a dot when set", () => {
  const long = "a note far too long to fit along the arrow ".repeat(8);
  const v = withArrow("web->api", { note: "retries twice", context: "Idempotent writes only." });
  Object.assign(v.arrows.find((a) => a.id === "api->postgres")!, { note: long });
  const { target } = setup(recorder().api, v);
  expect(target.querySelector('[data-label="web->api"] [data-note]')!.textContent).toBe("retries twice");
  const cut = target.querySelector('[data-label="api->postgres"] [data-note]')!.textContent!;
  expect(cut.endsWith("…")).toBe(true);
  expect(cut.length).toBeLessThan(long.length);
  expect(target.textContent).not.toContain("Idempotent");
  pickArrow(target, "web->api");
  expect(arrowButton(target, "Context")!.querySelector("[data-dot]")).not.toBeNull();
  expect(arrowButton(target, "Note")!.classList.contains("set")).toBe(true);
  pickArrow(target, "api->postgres");
  expect(arrowButton(target, "Context")!.querySelector("[data-dot]")).toBeNull();
});

test("right-click on an arrow gives its bar as a menu", async () => {
  const { api, got } = recorder();
  const { target } = setup(api);
  hitOf(target, "api->postgres").dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 200, clientY: 200 }));
  flushSync();
  const items = [...menuOf(target)!.querySelectorAll<HTMLElement>('[role="menuitem"]')];
  expect(items.map((b) => b.textContent)).toEqual(["Kind", "Label", "Note", "Context", "Flip", "Edit details", "Delete⌫"]);
  expect(document.activeElement).toBe(items[0]);
  items.find((b) => b.dataset.item === "delete")!.click();
  await tick();
  expect(got).toEqual([{ op: "unarrow", from: "api", to: "postgres", kind: "reads" }]);
});

test("typed note text is kept when the press lands on another arrow or on an end handle", async () => {
  const { api, got } = recorder();
  const { target } = setup(api);
  const typeNote = (id: string, v: string) => {
    pickArrow(target, id);
    arrowButton(target, "Note")!.click();
    flushSync();
    typeIn(target.querySelector<HTMLInputElement>('[data-arrow-text="note"]')!, v);
  };
  typeNote("web->api", "first");
  ptr(hitOf(target, "api->postgres"), "pointerdown");
  ptr(window, "pointerup");
  await tick();
  typeNote("api->postgres", "second");
  ptr(target.querySelector('[data-end="to"]')!, "pointerdown");
  ptr(window, "pointerup");
  await tick();
  expect(got).toEqual([
    { op: "arrowset", from: "web", to: "api", key: "note", value: "first" },
    { op: "arrowset", from: "api", to: "postgres", key: "note", value: "second" },
  ]);
});

test("panning keeps the picked arrow; a click on the ground lets it go", () => {
  const { target } = setup(recorder().api);
  pickArrow(target, "web->api");
  const pan = (init: PointerEventInit) => {
    ptr(stageOf(target), "pointerdown", { clientX: 900, clientY: 700, ...init });
    ptr(window, "pointermove", { clientX: 800, clientY: 600, ...init });
    ptr(window, "pointerup", { clientX: 800, clientY: 600, ...init });
  };
  pan({ button: 1 });
  expect(arrowBar(target)).not.toBeNull();
  press("h");
  pan({});
  expect(arrowBar(target)).not.toBeNull();
  press("v");
  ptr(stageOf(target), "pointerdown", { clientX: 900, clientY: 700 });
  ptr(window, "pointerup", { clientX: 900, clientY: 700 });
  expect(arrowBar(target)).toBeNull();
});

test("an end dropped on its own box, the other end, the ground or a box not in ig/ sends nothing", async () => {
  const { api, got } = recorder();
  const v = fresh();
  v.boxes.find((b) => b.id === "stripe")!.origin = "drift";
  const { target } = setup({ ...api, apply: async (e) => (got.push(e), v) }, v);
  for (const onto of ["postgres", "api", null, "stripe"]) {
    pickArrow(target, "api->postgres");
    ptr(target.querySelector('[data-end="to"]')!, "pointerdown");
    const at = onto ? boxEl(target, onto) : stageOf(target);
    ptr(at, "pointermove", { clientX: 950, clientY: 750 });
    if (onto === "stripe") expect(target.querySelector('[data-box="stripe"]')!.classList.contains("target")).toBe(false);
    ptr(at, "pointerup", { clientX: 950, clientY: 750 });
  }
  await tick();
  expect(got).toEqual([]);
});

test("the arrow menu's Kind opens the bar's kind list and Label edits the label", () => {
  const { target } = setup(recorder().api);
  const menu = (item: string) => {
    hitOf(target, "web->api").dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 200, clientY: 200 }));
    flushSync();
    menuOf(target)!.querySelector<HTMLElement>(`[data-item="${item}"]`)!.click();
    flushSync();
  };
  menu("kind");
  expect(target.querySelector("[data-item]")).toBeNull();
  expect(kindItems(target).length).toBeGreaterThan(0);
  menu("label");
  expect(target.querySelector<HTMLInputElement>('[data-arrow-text="label"]')!.value).toBe("POST /checkout");
});

test("a labelled arrow shows its label at rest", () => {
  const { target } = setup(recorder().api);
  const g = target.querySelector('[data-label="web->api"] g[data-state]')!;
  expect(g.getAttribute("data-state")).toBe("rest");
  expect(g.textContent).toContain("POST /checkout");
});

test("Context and Details open the panel seam at their field; letting go of the arrow closes it", () => {
  const view = fresh();
  const s = new EditSession({ view: () => view, box: (id: string) => view.boxes.find((b) => b.id === id), select: () => {}, edit: () => undefined } as unknown as Ctx);
  s.pickArrow({ id: "web->api" });
  s.arrowAct("context");
  expect(s.panel).toEqual({ arrow: { id: "web->api" }, focus: "context" });
  s.arrowAct("details");
  expect(s.panel).toEqual({ arrow: { id: "web->api" }, focus: null });
  s.pickArrow(null);
  expect(s.panel).toBeNull();
  s.pickArrow({ id: "web->api" });
  s.openPanel("context");
  s.cancel();
  expect(s.panel).toBeNull();
});

// --- the details panel
const panelField = (t: HTMLElement, name: string) => t.querySelector<HTMLInputElement>(`[data-panel] [data-field="${name}"]`)!;
const fill = (el: HTMLInputElement, v: string) => {
  el.focus();
  typeIn(el, v);
  el.blur();
  flushSync();
};
const openPanel = (t: HTMLElement, id: string) => {
  select(t, id);
  press("e");
};

test("the box panel: name, description, tech, context and made of each send one set on blur or Enter, nothing when unchanged", async () => {
  const { api, got } = recorder();
  const { target } = setup(api);
  openPanel(target, "web");
  const panel = target.querySelector<HTMLElement>('[data-panel="box"]')!;
  expect(panelField(target, "title").value).toBe("Web");
  expect(panelField(target, "title").classList.contains("hand")).toBe(true);
  expect(panel.textContent).toContain("One line, shown on hover and in lists.");
  expect(panel.textContent).toContain("Context for whoever builds it");
  fill(panelField(target, "title"), "Web");
  fill(panelField(target, "title"), "  Storefront ");
  await tick();
  fill(panelField(target, "description"), "The shop people see");
  await tick();
  const tech = panelField(target, "tech");
  tech.focus();
  typeIn(tech, "SvelteKit");
  press("Enter", {}, tech);
  await tick();
  tech.blur();
  fill(panelField(target, "context"), "Checkout runs here.\n\nKeep it fast.\n");
  await tick();
  fill(panelField(target, "at"), " apps/web/**\n\napps/shared/ui/** \n");
  await tick();
  fill(panelField(target, "at"), "apps/web/**\napps/shared/ui/**");
  fill(panelField(target, "description"), "");
  await tick();
  expect(got).toEqual([
    { op: "set", id: "web", key: "title", value: "Storefront" },
    { op: "set", id: "web", key: "description", value: "The shop people see" },
    { op: "set", id: "web", key: "tech", value: "SvelteKit" },
    { op: "set", id: "web", key: "context", value: "Checkout runs here.\n\nKeep it fast." },
    { op: "set", id: "web", key: "at", value: "apps/web/**\napps/shared/ui/**" },
    { op: "set", id: "web", key: "description", value: null },
  ]);
});

test("Shift+Enter is a new line in context; Enter saves it", async () => {
  const { api, got } = recorder();
  const { target } = setup(api);
  openPanel(target, "web");
  const ctx = panelField(target, "context");
  ctx.focus();
  typeIn(ctx, "line one");
  const shift = new KeyboardEvent("keydown", { key: "Enter", shiftKey: true, bubbles: true, cancelable: true });
  ctx.dispatchEvent(shift);
  flushSync();
  await tick();
  expect(shift.defaultPrevented).toBe(false);
  expect(got).toEqual([]);
  typeIn(ctx, "line one\nline two");
  const enter = new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true });
  ctx.dispatchEvent(enter);
  flushSync();
  await tick();
  expect(enter.defaultPrevented).toBe(true);
  expect(got).toEqual([{ op: "set", id: "web", key: "context", value: "line one\nline two" }]);
});

test("the box panel's kind dropdown and the kind's own properties: chips for one-of", async () => {
  const { api, got } = recorder();
  const view = fresh();
  Object.assign(view.boxes.find((b) => b.id === "web")!, { kind: { value: "model", source: "ig" }, props: { hosted: "local" } });
  const { target } = setup(api, view);
  openPanel(target, "web");
  const panel = target.querySelector<HTMLElement>('[data-panel="box"]')!;
  const chips = () => [...panel.querySelectorAll<HTMLButtonElement>('[data-prop="hosted"]')];
  expect(chips().map((c) => [c.textContent, c.getAttribute("aria-pressed")])).toEqual([["local", "true"], ["hosted", "false"]]);
  chips()[1].click();
  await tick();
  chips()[0].click();
  await tick();
  fill(panelField(target, "prop-provider"), "ollama");
  await tick();
  panel.querySelector<HTMLElement>("[data-kind-button]")!.click();
  flushSync();
  panel.querySelector<HTMLElement>('[data-kinds] [data-kind="service"]')!.click();
  await tick();
  expect(got).toEqual([
    { op: "set", id: "web", key: "prop:hosted", value: "hosted" },
    { op: "set", id: "web", key: "prop:hosted", value: null },
    { op: "set", id: "web", key: "prop:provider", value: "ollama" },
    { op: "set", id: "web", key: "kind", value: "service" },
  ]);
});

test("a kind's own property named like a set key is set as prop:<name>; the set chip of a required one sends nothing", async () => {
  const { api, got } = recorder();
  const view = fresh();
  view.kinds.find((k) => k.name === "entry")!.props.find((p) => p.name === "kind")!.required = true;
  Object.assign(view.boxes.find((b) => b.id === "web")!, { kind: { value: "entry", source: "ig" }, props: { kind: "route" } });
  const { target } = setup(api, view);
  openPanel(target, "web");
  const chip = (v: string) => [...target.querySelectorAll<HTMLButtonElement>('[data-panel="box"] [data-prop="kind"]')].find((b) => b.textContent === v)!;
  expect(chip("route").getAttribute("aria-pressed")).toBe("true");
  chip("route").click();
  await tick();
  expect(got).toEqual([]);
  chip("command").click();
  await tick();
  expect(got).toEqual([{ op: "set", id: "web", key: "prop:kind", value: "command" }]);
});

test("the box panel lists its arrows and what is inside; an arrow opens its panel, + Add a box a draft inside", () => {
  const target = document.createElement("div");
  document.body.append(target);
  const m = mount(Map, { target, props: { view: fresh(), size, edit: recorder().api, camera: new Camera() } });
  flushSync();
  openPanel(target, "api");
  const panel = target.querySelector<HTMLElement>('[data-panel="box"]')!;
  const rows = [...panel.querySelectorAll<HTMLElement>("[data-arrow-row]")];
  expect(rows.map((r) => r.textContent!.replace(/\s+/g, " ").trim())).toEqual(expect.arrayContaining(["reads→ Postgres", "calls← Web"]));
  expect([...panel.querySelectorAll("[data-inside]")].map((b) => b.getAttribute("data-inside"))).toContain("api/routers");
  panel.querySelector<HTMLElement>("[data-add-inside]")!.click();
  m.tick(performance.now() + 10_000);
  flushSync();
  expect(target.querySelector('[data-box="api"][data-open] [data-draft]')).not.toBeNull();
  expect(target.querySelector('[data-panel="box"]')).not.toBeNull();
  rows.find((r) => r.getAttribute("data-arrow-row") === "api->postgres")!.click();
  flushSync();
  expect(target.querySelector('[data-panel="box"]')).toBeNull();
  expect(target.querySelector('[data-panel="arrow"]')!.getAttribute("aria-label")).toBe("Edit arrow Api → Postgres");
});

test("the arrow panel: kind chips, label, the note of at most 80 and context send arrowset; the preview shows it as written", async () => {
  const { api, got } = recorder();
  const { target } = setup(api, withArrow("web->api", {
    note: "retries twice", context: 'Idempotent: send "key".\nSee #12.', written: "ig/system.kdl:7", seen: ["apps/web/src/cart.ts:12", "apps/web/src/pay.ts:3"],
  }));
  pickArrow(target, "web->api");
  arrowButton(target, "Details")!.click();
  flushSync();
  const panel = target.querySelector<HTMLElement>('[data-panel="arrow"]')!;
  expect(panel.querySelector(".head b")!.textContent).toBe("Web → Api");
  expect(panel.querySelector("[data-kdl]")!.textContent).toBe(
    'calls "api" label="POST /checkout" note="retries twice" context=#"""\n    Idempotent: send "key".\n    See #12.\n    """#');
  expect(panel.querySelector("[data-written]")!.textContent).toBe("In ig/system.kdl");
  expect([...panel.querySelectorAll("[data-seen] li")].map((l) => l.textContent)).toEqual(["apps/web/src/cart.ts:12", "apps/web/src/pay.ts:3"]);
  expect(panel.querySelector('[data-arrow-kind="calls"]')!.getAttribute("aria-pressed")).toBe("true");
  panel.querySelector<HTMLElement>('[data-arrow-kind="sends"]')!.click();
  await tick();
  expect(panelField(target, "note").maxLength).toBe(80);
  fill(panelField(target, "note"), "retries twice");
  fill(panelField(target, "label"), "POST /pay");
  await tick();
  fill(panelField(target, "note"), "");
  await tick();
  fill(panelField(target, "context"), "Ask the team first.");
  await tick();
  expect(got).toEqual([
    { op: "arrowset", from: "web", to: "api", key: "kind", value: "sends" },
    { op: "arrowset", from: "web", to: "api", key: "label", value: "POST /pay" },
    { op: "arrowset", from: "web", to: "api", key: "note", value: null },
    { op: "arrowset", from: "web", to: "api", key: "context", value: "Ask the team first." },
  ]);
});

test("Context on the arrow bar opens the panel at the context field", () => {
  const { target } = setup(recorder().api);
  pickArrow(target, "api->postgres");
  arrowButton(target, "Context")!.click();
  flushSync();
  expect(document.activeElement).toBe(panelField(target, "context"));
  expect(target.querySelector("[data-kdl]")!.textContent).toBe('reads "postgres"');
  expect(target.querySelector("[data-written]")!.textContent).toBe("In ig/");
  expect(target.querySelector("[data-seen]")).toBeNull();
});

test("Esc closes the panel, keeps what was typed and gives the focus back to the map", async () => {
  const { api, got } = recorder();
  const { target } = setup(api);
  openPanel(target, "web");
  const d = panelField(target, "description");
  d.focus();
  typeIn(d, "The shop");
  press("Escape", {}, d);
  await tick();
  expect(target.querySelector("[data-panel]")).toBeNull();
  expect(document.activeElement).toBe(boxEl(target, "web"));
  expect(target.querySelector("section.details")).not.toBeNull();
  pickArrow(target, "web->api");
  arrowButton(target, "Details")!.click();
  flushSync();
  press("Escape", {}, target.querySelector('[data-panel="arrow"]')!);
  await tick();
  expect(target.querySelector("[data-panel]")).toBeNull();
  expect(arrowBar(target)).not.toBeNull();
  expect(document.activeElement).toBe(boxEl(target, "web"));
  expect(got).toEqual([{ op: "set", id: "web", key: "description", value: "The shop" }]);
});

test("a failed save shows the toast and the field keeps what was typed; a second blur tries again", async () => {
  let n = 0;
  const { api } = fake({ apply: async () => { n++; throw new Error("not a glob"); } });
  const { target } = setup(api);
  openPanel(target, "web");
  fill(panelField(target, "at"), "../outside");
  await tick();
  flushSync();
  expect(target.querySelector('[aria-live="polite"]')!.textContent).toBe("not a glob");
  expect(panelField(target, "at").value).toBe("../outside");
  panelField(target, "at").focus();
  panelField(target, "at").blur();
  await tick();
  expect(n).toBe(2);
});

// --- copy and paste
const pasteText = (text: string) => {
  const dt = new DataTransfer();
  dt.setData("text/plain", text);
  const e = new ClipboardEvent("paste", { clipboardData: dt, bubbles: true, cancelable: true });
  window.dispatchEvent(e);
  flushSync();
  return e;
};
const copied = () => {
  const texts: string[] = [];
  vi.spyOn(navigator.clipboard, "writeText").mockImplementation(async (t: string) => void texts.push(t));
  return texts;
};
const withCopyOf = (of: string, id: string) => {
  const v = fresh();
  v.boxes.push({ ...structuredClone(v.boxes.find((b) => b.id === of)!), id, title: { value: "copy", source: "ig" } });
  v.placed.frames[id] = { ...v.placed.frames[of], y: v.placed.frames[of].y + 400 };
  return v;
};

test("Cmd+C copies the selected box as ig/ text; Cmd+V off the map pastes it beside, one batch, and selects it", async () => {
  const texts = copied(), got: Edit[] = [];
  const target = harness(fake({ apply: async (e) => (got.push(e), withCopyOf("postgres", "postgres-2")) }).api);
  select(target, "postgres");
  (document.activeElement as HTMLElement | null)?.blur();
  press("c", { metaKey: true });
  expect(texts).toEqual(['store "postgres" title="Postgres"\n']);
  const e = pasteText(texts[0]);
  await tick();
  flushSync();
  expect(e.defaultPrevented).toBe(true);
  expect(got).toHaveLength(1);
  expect(got[0]).toEqual({ op: "batch", edits: [{ op: "add", parent: "system", kind: "store", id: "postgres-2", title: "Postgres copy", at: [], before: null }] });
  expect(target.querySelector("section.details")!.textContent).toContain("copy");
  vi.restoreAllMocks();
});

test("Cmd+Shift+C copies with what's inside; pasting at the pointer goes into the box under it", async () => {
  const texts = copied();
  const { api, got } = recorder();
  const target = harness(api);
  select(target, "web");
  (document.activeElement as HTMLElement | null)?.blur();
  press("c", { metaKey: true, shiftKey: true });
  expect(texts[0]).toContain('entry "web/pages" title="Pages"');
  boxEl(target, "postgres").dispatchEvent(new PointerEvent("pointerover", { bubbles: true, clientX: 900, clientY: 200 }));
  pasteText(texts[0]);
  await tick();
  expect(got).toHaveLength(1);
  expect((got[0] as Extract<Edit, { op: "batch" }>).edits.filter((x) => x.op === "add").map((x) => [(x as { parent: string }).parent, (x as { id: string }).id]))
    .toEqual([["postgres", "web-2"], ["web-2", "web-2/pages"]]);
  vi.restoreAllMocks();
});

test("pasting ig/ text copied from a file works the same; text that is not boxes is left to the page", async () => {
  const { api, got } = recorder();
  harness(api);
  expect(pasteText("just words").defaultPrevented).toBe(false);
  pasteText('queue "events" title="Order events" tech="NATS"\nstore "db" title="Database" {\n    reads "events"\n}\n');
  await tick();
  expect(got).toEqual([{ op: "batch", edits: [
    { op: "add", parent: "system", kind: "queue", id: "events", title: "Order events", at: [], before: null, props: { tech: "NATS" } },
    { op: "add", parent: "system", kind: "store", id: "db", title: "Database", at: [], before: null },
    { op: "arrow", from: "db", to: "events", kind: "reads", label: null },
  ] }]);
});

test("Cmd+D duplicates the box alone, Cmd+Shift+D with what's inside; one Cmd+Z undoes a paste", async () => {
  const { api, got, calls } = recorder();
  const target = harness(api);
  select(target, "web");
  (document.activeElement as HTMLElement | null)?.blur();
  press("d", { metaKey: true });
  await tick();
  press("d", { metaKey: true, shiftKey: true });
  await tick();
  const ids = (e: Edit) => (e as Extract<Edit, { op: "batch" }>).edits.flatMap((x) => (x.op === "add" ? [x.id] : []));
  expect(got.map(ids)).toEqual([["web-2"], ["web-2", "web-2/pages"]]);
  expect(got[0].edits[0].title).toBe("Web copy");
  press("z", { metaKey: true });
  await tick();
  expect(calls).toEqual(["undo"]);
});

test("the box menu copies, duplicates and, once something is copied, pastes; the ground menu's Paste goes live", async () => {
  const texts = copied();
  const { api, got } = recorder();
  const { target } = setup(api);
  rightClick(target, "postgres");
  const names = () => [...menuOf(target)!.querySelectorAll('[role="menuitem"]')].map((b) => b.firstElementChild!.textContent);
  expect(names()).toEqual(expect.arrayContaining(["Copy", "Copy with what's inside", "Duplicate"]));
  expect(names()).not.toContain("Paste");
  item(target, "Copy").click();
  flushSync();
  expect(texts).toHaveLength(1);
  rightClick(target, "web");
  expect(names()).toContain("Paste");
  press("Escape", {}, menuOf(target)!);
  stageOf(target).dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 950, clientY: 750 }));
  flushSync();
  const paste = item(target, "Paste") as HTMLButtonElement;
  expect(paste.disabled).toBe(false);
  paste.click();
  await tick();
  expect(got).toHaveLength(1);
  expect(got[0].edits[0]).toMatchObject({ op: "add", parent: "system", id: "postgres-2", title: "Postgres copy" });
  vi.restoreAllMocks();
});

test("several boxes pasted are all selected once the view holds them", async () => {
  const texts = copied(), v = withCopyOf("postgres", "postgres-2");
  v.boxes.push({ ...structuredClone(v.boxes.find((b) => b.id === "stripe")!), id: "stripe-2" });
  v.placed.frames["stripe-2"] = { ...v.placed.frames.stripe, y: v.placed.frames.stripe.y + 400 };
  const target = harness(fake({ apply: async () => v }).api);
  select(target, "postgres");
  shiftClick(target, "stripe");
  (document.activeElement as HTMLElement | null)?.blur();
  press("c", { metaKey: true });
  pasteText(texts[0]);
  await tick();
  flushSync();
  expect(picked(target)).toEqual(["postgres-2", "stripe-2"]);
  vi.restoreAllMocks();
});

test("Cmd+V with no paste event from the webview pastes the copy in the page; with one, only once", async () => {
  const { api, got } = recorder();
  const target = harness(api);
  select(target, "postgres");
  (document.activeElement as HTMLElement | null)?.blur();
  press("c", { metaKey: true });
  press("v", { metaKey: true });
  await new Promise((r) => setTimeout(r, 150));
  expect(got).toHaveLength(1);
  await tick();
  press("v", { metaKey: true });
  pasteText("");
  await new Promise((r) => setTimeout(r, 150));
  expect(got).toHaveLength(2);
});
