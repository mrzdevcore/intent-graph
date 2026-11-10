// @vitest-environment happy-dom
import { expect, test } from "vitest";
import { flushSync, mount } from "svelte";
import Map from "../src/map/Map.svelte";
import type { EditApi } from "../src/api";
import type { View } from "../src/map/types";
import shop from "./fixtures/shop-view.json";
import { prefill, slug } from "../src/map/edit/add";
import { proposed } from "../src/map/edit/proposals";
import { arrowLook, look } from "../src/map/looks";
import MapHarness from "./MapHarness.svelte";
import MovePicker from "../src/map/edit/MovePicker.svelte";
import { parentsFor } from "../src/map/edit/menu";

const tick = () => new Promise((r) => setTimeout(r, 0));
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

const pick = (f: HTMLElement, kind: string) => { f.querySelector<HTMLButtonElement>(`button[data-kind="${kind}"]`)!.click(); flushSync(); };

test("Box tool click in a container: its kinds, then Title focused and the code prefilled from its glob", async () => {
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

test("Arrow tool: drag A to B lights B, Enter draws; the last kind is preselected next time", async () => {
  const { api, got } = recorder();
  const { target } = setup(api);
  press("a");
  ptr(boxEl(target, "web"), "pointerdown");
  ptr(boxEl(target, "api"), "pointermove");
  expect(target.querySelector('[data-box="api"]')!.classList.contains("target")).toBe(true);
  ptr(boxEl(target, "api"), "pointerup");
  const f = target.querySelector('form[aria-label^="Arrow"]') as HTMLFormElement;
  expect(f).not.toBeNull();
  expect(f.querySelector('[aria-checked="true"]')!.textContent).toBe("calls");
  (f.querySelectorAll<HTMLElement>('[role="radio"]')[2]).click();
  flushSync();
  f.requestSubmit();
  await tick();
  expect(got).toEqual([{ op: "arrow", from: "web", to: "api", kind: "reads", label: null }]);
  press("a");
  ptr(boxEl(target, "web"), "pointerdown");
  ptr(boxEl(target, "api"), "pointermove");
  ptr(boxEl(target, "api"), "pointerup");
  expect(target.querySelector('form[aria-label^="Arrow"] [aria-checked="true"]')!.textContent).toBe("reads");
});

test("Arrow released on the same box or empty ground cancels", () => {
  const { target } = setup(recorder().api);
  press("a");
  ptr(boxEl(target, "web"), "pointerdown");
  ptr(boxEl(target, "web"), "pointerup");
  ptr(boxEl(target, "web"), "pointerdown");
  ptr(target.querySelector(".stage")!, "pointermove");
  ptr(target.querySelector(".stage")!, "pointerup");
  expect(target.querySelector('form[aria-label^="Arrow"]')).toBeNull();
});

test("right-click opens the menu, Esc closes it", () => {
  const { target } = setup(recorder().api);
  boxEl(target, "web").dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true }));
  flushSync();
  expect([...menuOf(target)!.querySelectorAll('[role="menuitem"]')].map((b) => b.firstElementChild!.textContent))
    .toEqual(["Rename", "Move to…", "Add box inside", "Arrow from here", "Arrange inside", "Delete"]);
  press("ArrowDown", {}, document.activeElement!);
  press("Escape", {}, document.activeElement!);
  expect(menuOf(target)).toBeNull();
  boxEl(target, "stripe").dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true }));
  flushSync();
  expect(menuOf(target)!.textContent).not.toContain("Arrange inside");
});

test("Arrange inside lays out the box's inside and says how many boxes moved", () => {
  const { target } = setup(recorder().api);
  boxEl(target, "api").dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true }));
  flushSync();
  ([...menuOf(target)!.querySelectorAll('[role="menuitem"]')].find((b) => b.textContent?.startsWith("Arrange inside")) as HTMLButtonElement).click();
  flushSync();
  expect(menuOf(target)).toBeNull();
  expect(target.querySelector(".arranged")?.textContent).toMatch(/Arranged \d+ boxes/);
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

test("Delete sends Remove and shows the refusal", async () => {
  const got: any[] = [];
  const { api } = fake({ apply: async (e) => (got.push(e), Promise.reject(new Error("`api` still holds 9 boxes"))) });
  const { target } = setup(api);
  boxEl(target, "api").dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true }));
  flushSync();
  item(target, "Delete").click();
  await tick();
  flushSync();
  expect(got).toEqual([{ op: "remove", id: "api" }]);
  expect(target.querySelector('[aria-live="polite"]')!.textContent).toBe("`api` still holds 9 boxes");
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

test("Backspace and Delete with a box selected send Remove", async () => {
  const { api, got } = recorder();
  const { target } = setup(api);
  select(target, "web");
  press("Backspace");
  await tick();
  press("Delete");
  await tick();
  expect(got).toEqual([{ op: "remove", id: "web" }, { op: "remove", id: "web" }]);
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

test("Backspace with an arrow chip focused sends nothing; so does a held key", async () => {
  const { api, got } = recorder();
  const { target } = setup(api);
  select(target, "web");
  press("a");
  ptr(boxEl(target, "web"), "pointerdown");
  ptr(boxEl(target, "api"), "pointermove");
  ptr(boxEl(target, "api"), "pointerup");
  const chip = target.querySelector<HTMLElement>('form[aria-label^="Arrow"] [role="radio"]')!;
  chip.focus();
  press("Backspace", {}, chip);
  press("Backspace");
  expect(got).toEqual([]);
  press("Escape", {}, chip);
  press("Delete", { repeat: true });
  await tick();
  expect(got).toEqual([]);
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
