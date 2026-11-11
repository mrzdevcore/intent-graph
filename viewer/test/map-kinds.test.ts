// @vitest-environment happy-dom
// The add form speaks the kinds table (`view.kinds`); warnings have their own field.
import { afterEach, expect, test } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import Map from "../src/map/Map.svelte";
import type { EditApi } from "../src/api";
import type { KindView, View } from "../src/map/types";
import shop from "./fixtures/shop-view.json";
import { mostUsed, rankKinds } from "../src/map/edit/kinds";
import MapHarness from "./MapHarness.svelte";

const tick = () => new Promise((r) => setTimeout(r, 0));
const fresh = () => structuredClone(shop) as unknown as View;
const IOT: KindView = { name: "iot-device", base: "service", custom: true, title: "IoT device", means: "A device in the field", icon: "cpu", code: true, role: "run",
  props: [{ name: "firmware", means: "Its firmware version", required: true, oneOf: [] }] };
const withIot = () => { const v = fresh(); v.kinds = [...v.kinds, IOT]; return v; };

function setup(view: View, got: unknown[] = []) {
  const target = document.createElement("div");
  document.body.append(target);
  const api = { apply: async (e: unknown) => (got.push(e), fresh()), keep: async () => fresh(), drop: async () => fresh(), dropAll: async () => fresh(),
    undo: async () => fresh(), redo: async () => fresh() } as unknown as EditApi;
  mounted.push(mount(Map, { target, props: { view, size: { w: 1000, h: 800 }, edit: api } }));
  flushSync();
  return target;
}
// a map left mounted still hears the window's keys
const mounted: ReturnType<typeof mount>[] = [];
afterEach(() => {
  for (const m of mounted.splice(0)) unmount(m);
  document.body.innerHTML = "";
});
function clickBox(t: HTMLElement, id: string | null) {
  const el = id ? t.querySelector(`[data-box="${id}"] > button`)! : t.querySelector(".stage")!;
  el.dispatchEvent(new PointerEvent("pointerover", { bubbles: true, clientX: 40, clientY: 50 }));
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "k", bubbles: true }));
  flushSync();
}
const form = (t: HTMLElement) => t.querySelector('form[aria-label^="New box"]') as HTMLFormElement;
const groups = (f: HTMLElement) => [...f.querySelectorAll<HTMLElement>('[role="group"]')].map((g) => ({
  label: g.getAttribute("aria-label"), kinds: [...g.querySelectorAll<HTMLElement>("button[data-kind]")].map((b) => b.dataset.kind),
}));
const row = (f: HTMLElement, kind: string) => f.querySelector<HTMLButtonElement>(`button[data-kind="${kind}"]`)!;
const field = (f: HTMLElement, label: string) =>
  [...f.querySelectorAll("label")].find((l) => l.textContent!.trim().startsWith(label))?.querySelector("input,select") as HTMLInputElement | undefined;
const type = (el: HTMLInputElement, v: string) => { el.value = v; el.dispatchEvent(new Event("input", { bubbles: true })); flushSync(); };

test("the add form offers the kinds of the view, grouped, with their meaning", () => {
  const t = setup(withIot());
  clickBox(t, null);
  const f = form(t);
  expect(groups(f)).toEqual([
    { label: "Runs", kinds: ["person", "external", "app", "service", "job", "store", "queue", "model"] },
    { label: "Your team's kinds", kinds: ["iot-device"] },
    { label: "Grouping", kinds: ["group"] },
  ]);
  expect(row(f, "iot-device").textContent).toContain("IoT device · a service");
  expect(row(f, "iot-device").textContent).toContain("A device in the field");
  expect(row(f, "iot-device").textContent).toContain("From ig/kinds/iot-device.kdl");
  expect(row(f, "store").textContent).toContain("Where data is kept");
  expect(row(f, "store").querySelector(".ic")).not.toBeNull();
  expect(f.textContent).not.toMatch(/\bapi\b|folder/);
  document.body.innerHTML = "";
  const u = setup(withIot());
  clickBox(u, "api");
  expect(groups(form(u))).toEqual([{ label: "Inside", kinds: ["module", "entry", "usecase", "port", "adapter", "agent", "domain", "group"] }]);
  expect(row(form(u), "port").textContent).toContain("What the core needs from outside");
});

test("choosing model shows its own fields; the add sent carries them", async () => {
  const got: unknown[] = [];
  const t = setup(fresh(), got);
  clickBox(t, null);
  row(form(t), "model").click();
  flushSync();
  const f = form(t);
  expect(f.textContent).toContain("New AI model");
  expect(field(f, "Provider")!.tagName).toBe("INPUT");
  expect(field(f, "Model")!.tagName).toBe("INPUT");
  const hosted = field(f, "Hosted") as unknown as HTMLSelectElement;
  expect(hosted.tagName).toBe("SELECT");
  expect([...hosted.options].map((o) => o.value).filter(Boolean)).toEqual(["local", "hosted"]);
  type(field(f, "Title")!, "Llama");
  type(field(f, "Provider")!, "ollama");
  hosted.value = "local"; hosted.dispatchEvent(new Event("change", { bubbles: true })); flushSync();
  expect(field(f, "Code")!.value).toBe("llama/**");
  f.requestSubmit();
  await tick();
  expect(got).toEqual([{ op: "add", parent: "system", kind: "model", id: "llama", title: "Llama", at: ["llama/**"], before: null,
    props: { provider: "ollama", hosted: "local" } }]);
});

test("a team kind's required property is marked; external has no code field", () => {
  const t = setup(withIot());
  clickBox(t, null);
  row(form(t), "iot-device").click();
  flushSync();
  const fw = field(form(t), "Firmware")!;
  expect(fw.required).toBe(true);
  expect(fw.closest("label")!.textContent).toContain("required");
  document.body.innerHTML = "";
  const u = setup(fresh());
  clickBox(u, null);
  row(form(u), "external").click();
  flushSync();
  expect(field(form(u), "Code")).toBeUndefined();
});

test("warnings render from view.warnings, without the warning: prefix", () => {
  const v = fresh();
  v.warnings = ["`g` holds 13 boxes; more than 12 is hard to read"];
  v.notices = ["warning: not this one"];
  const t = setup(v);
  const w = t.querySelector('[aria-label="Warnings"]')!;
  expect(w.querySelector(".word")!.textContent).toBe("Crowded");
  expect(w.textContent).toContain("g holds 13 boxes; more than 12 is hard to read");
  expect(w.textContent).not.toContain("warning:");
});

test("a required choice adds with its first value untouched; a team kind of external has no code field", async () => {
  const got: unknown[] = [];
  const v = fresh();
  v.kinds = [...v.kinds,
    { ...IOT, name: "sensor", title: "Sensor", props: [{ name: "bus", means: "How it talks", required: true, oneOf: ["i2c", "spi"] }] },
    { ...IOT, name: "partner", base: "external", title: "Partner", code: false, props: [] }];
  const t = setup(v, got);
  clickBox(t, null);
  row(form(t), "sensor").click();
  flushSync();
  const bus = field(form(t), "Bus") as unknown as HTMLSelectElement;
  expect([...bus.options].map((o) => o.value)).toEqual(["i2c", "spi"]);
  type(field(form(t), "Title")!, "Probe");
  form(t).requestSubmit();
  await tick();
  expect(got).toEqual([{ op: "add", parent: "system", kind: "sensor", id: "probe", title: "Probe", at: ["probe/**"], before: null, props: { bus: "i2c" } }]);
  clickBox(t, null);
  row(form(t), "partner").click();
  flushSync();
  expect(field(form(t), "Code")).toBeUndefined();
});

test("a group owns no code: its add form has no Code field", async () => {
  const t = setup(fresh());
  clickBox(t, null);
  row(form(t), "group").click();
  flushSync();
  await tick();
  expect(field(form(t), "Title")).toBeDefined();
  expect(field(form(t), "Code")).toBeUndefined();
});

// --- the kind of a box, set afterwards
const names = (ks: KindView[]) => ks.map((k) => k.name);
const kinds = () => withIot().kinds;

test("rankKinds: before typing, the kind most used beside the box is best; run kinds, design kinds in a run, the team's, grouping", () => {
  const out = rankKinds("", kinds(), { parentKinds: ["store", "service", "store", "box"], inRun: false });
  expect(out.best?.name).toBe("store");
  expect(out.groups.map((g) => g.title)).toEqual(["Runs", "Your team's kinds", "Grouping"]);
  expect(names(out.groups[0].kinds)).toEqual(["person", "external", "app", "service", "job", "queue", "model"]);
  expect(names(out.groups[1].kinds)).toEqual(["iot-device"]);
  const inRun = rankKinds("", kinds(), { parentKinds: [], inRun: true });
  expect(inRun.best).toBeNull();
  expect(inRun.groups.map((g) => g.title)).toEqual(["Runs", "Inside a run", "Your team's kinds", "Grouping"]);
  expect(names(inRun.groups[1].kinds)).toEqual(["module", "entry", "usecase", "port", "adapter", "agent", "domain"]);
  expect(names(inRun.groups.flatMap((g) => g.kinds))).not.toContain("system");
});

test("rankKinds: typing filters by name, title and meaning; the best match is the top hit and is not listed twice", () => {
  const ctx = { parentKinds: ["store"], inRun: true };
  const q = rankKinds("que", kinds(), ctx);
  expect(q.best?.name).toBe("queue");
  expect(names(q.groups.flatMap((g) => g.kinds))).not.toContain("queue");
  expect(rankKinds("iot", kinds(), ctx).best?.name).toBe("iot-device");
  const field = rankKinds("field", kinds(), ctx);
  expect(field.best?.name).toBe("iot-device");
  expect(rankKinds("zzz", kinds(), ctx)).toEqual({ best: null, groups: [] });
  expect(rankKinds("  STO ", kinds(), ctx).best?.name).toBe("store");
});

test("mostUsed: the map's own kinds first, five of them", () => {
  const v = fresh();
  const top = names(mostUsed(v.boxes, v.kinds, false));
  expect(top).toHaveLength(5);
  expect(top[0]).toBe("app");
  expect(top).toEqual(expect.arrayContaining(["store", "external"]));
});

function harnessed(view: View, got: unknown[], reply: (e: unknown) => View = () => fresh()) {
  const target = document.createElement("div");
  document.body.append(target);
  const api = { apply: async (e: unknown) => (got.push(e), reply(e)), keep: async () => fresh(), drop: async () => fresh(), dropAll: async () => fresh(),
    undo: async () => fresh(), redo: async () => fresh() } as unknown as EditApi;
  mounted.push(mount(MapHarness, { target, props: { start: view, edit: api, size: { w: 1000, h: 800 } } }));
  flushSync();
  return target;
}
const select = (t: HTMLElement, id: string) => { (t.querySelector(`[data-box="${id}"] > button`) as HTMLElement).click(); flushSync(); };
const bar = (t: HTMLElement) => t.querySelector<HTMLElement>("[data-bar]");
const kindButton = (t: HTMLElement) => bar(t)!.querySelector<HTMLButtonElement>("[data-kind-button]")!;
const finder = (t: HTMLElement) => t.querySelector<HTMLInputElement>('input[aria-label="Find a kind"]');
const press = (key: string, on: EventTarget = window) => { on.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true })); flushSync(); };
const withBox = (id: string, title: string) => {
  const v = fresh();
  v.boxes.push({ ...structuredClone(v.boxes.find((b) => b.id === "postgres")!), id, kind: { value: "box" } as never, title: { value: title } as never, madeOf: [] });
  v.placed.frames[id] = { x: 96, y: 600, w: 248, h: 152 };
  return v;
};

test("the bar shows the selected box's kind; a box with no kind says Set a kind", () => {
  const t = setup(withBox("billing", "Billing"));
  expect(bar(t)).toBeNull();
  select(t, "postgres");
  expect(kindButton(t).textContent).toContain("Store");
  expect(bar(t)!.textContent).toContain("Details");
  expect(bar(t)!.textContent).not.toContain("Code");
  select(t, "web");
  expect(bar(t)!.textContent).toContain("Code");
  select(t, "billing");
  expect(kindButton(t).textContent).toContain("Set a kind");
});

test("no edit: no bar", () => {
  const target = document.createElement("div");
  document.body.append(target);
  mounted.push(mount(Map, { target, props: { view: fresh(), size: { w: 1000, h: 800 } } }));
  flushSync();
  select(target, "postgres");
  expect(bar(target)).toBeNull();
});

test("the kind list: type to filter, Enter sets the best match; No kind sets box; Esc closes", async () => {
  const got: unknown[] = [];
  const t = setup(withBox("billing", "Billing"), got);
  select(t, "billing");
  kindButton(t).click();
  flushSync();
  expect(document.activeElement).toBe(finder(t));
  const heads = [...t.querySelectorAll("[data-kinds] .h")].map((h) => h.textContent).filter(Boolean);
  expect(heads[0]).toBe("Best match");
  expect(t.querySelector("[data-kinds] .h + button")!.getAttribute("data-kind")).toBe("app");
  type(finder(t)!, "queu");
  expect(t.querySelector("[data-kinds] .h + button")!.getAttribute("data-kind")).toBe("queue");
  press("Enter", finder(t)!);
  await tick();
  expect(got).toEqual([{ op: "set", id: "billing", key: "kind", value: "queue" }]);
  expect(finder(t)).toBeNull();
  select(t, "postgres");
  kindButton(t).click();
  flushSync();
  expect(t.querySelector('[data-kinds] [aria-checked="true"]')!.getAttribute("data-kind")).toBe("store");
  press("ArrowDown", finder(t)!);
  expect(document.activeElement!.getAttribute("role")).toBe("menuitemradio");
  press("ArrowUp", document.activeElement!);
  expect(document.activeElement).toBe(finder(t));
  t.querySelector<HTMLButtonElement>('[data-kinds] [data-kind="box"]')!.click();
  await tick();
  expect(got[1]).toEqual({ op: "set", id: "postgres", key: "kind", value: "box" });
  kindButton(t).click();
  flushSync();
  press("Escape", finder(t)!);
  expect(finder(t)).toBeNull();
  expect(bar(t)).not.toBeNull();
  expect(document.activeElement).toBe(kindButton(t));
});

test("picking the kind a box already has sends nothing", async () => {
  const got: unknown[] = [];
  const t = setup(fresh(), got);
  select(t, "postgres");
  kindButton(t).click();
  flushSync();
  t.querySelector<HTMLButtonElement>('[data-kinds] [data-kind="store"]')!.click();
  await tick();
  expect(got).toEqual([]);
});

const rightClick = (t: HTMLElement, id: string) => {
  t.querySelector(`[data-box="${id}"] > button`)!.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true }));
  flushSync();
};
const menu = (t: HTMLElement) => t.querySelector<HTMLElement>('[role="menu"]')!;

test("right-click Kind opens the five most used and All kinds; one sets it, K opens the whole list", async () => {
  const got: unknown[] = [];
  const t = setup(fresh(), got);
  rightClick(t, "stripe");
  const kind = menu(t).querySelector<HTMLElement>('[data-item="kinds"]')!;
  kind.focus();
  press("ArrowRight", kind);
  const sub = t.querySelector<HTMLElement>("[data-sub]")!;
  expect([...sub.querySelectorAll("[data-kind]")].map((b) => b.getAttribute("data-kind"))).toEqual(mostUsed(fresh().boxes, fresh().kinds, false).map((k) => k.name));
  expect(sub.textContent).toContain("All kinds…");
  expect(document.activeElement).toBe(sub.querySelector("[data-kind]"));
  press("ArrowLeft", document.activeElement!);
  expect(t.querySelector("[data-sub]")).toBeNull();
  expect(document.activeElement).toBe(kind);
  kind.click();
  flushSync();
  t.querySelector<HTMLButtonElement>('[data-sub] [data-kind="store"]')!.click();
  await tick();
  expect(got).toEqual([{ op: "set", id: "stripe", key: "kind", value: "store" }]);
  expect(t.querySelector('[role="menu"][aria-label$="menu"]')).toBeNull();
  rightClick(t, "stripe");
  menu(t).querySelector<HTMLElement>('[data-item="kinds"]')!.click();
  flushSync();
  press("k", t.querySelector("[data-sub] [data-kind]")!);
  expect(t.querySelector('[role="menu"][aria-label$="menu"]')).toBeNull();
  expect(document.activeElement).toBe(finder(t));
});

test("the menu's keys: E edits details, F2 renames, M moves, A draws an arrow", () => {
  const t = setup(fresh());
  rightClick(t, "web");
  press("e", document.activeElement!);
  expect(t.querySelector('[role="menu"]')).toBeNull();
  expect(t.querySelector("section.details")).not.toBeNull();
  rightClick(t, "web");
  press("F2", document.activeElement!);
  expect(t.querySelector('input[aria-label="Rename box"]')).not.toBeNull();
  press("Escape", t.querySelector('input[aria-label="Rename box"]')!);
  rightClick(t, "web");
  press("m", document.activeElement!);
  expect(t.querySelector('[role="dialog"]')).not.toBeNull();
  press("Escape", document.activeElement!);
  rightClick(t, "web");
  press("a", document.activeElement!);
  expect(t.querySelector('[role="toolbar"] [aria-pressed="true"]')!.getAttribute("aria-label")).toContain("Arrow");
});

test("Tab out of a draft opens the new box's kind list", async () => {
  const got: unknown[] = [];
  const t = harnessed(fresh(), got, () => withBox("billing", "Billing"));
  t.querySelector(".stage")!.dispatchEvent(new MouseEvent("dblclick", { bubbles: true, clientX: 40, clientY: 600 }));
  flushSync();
  const name = t.querySelector<HTMLInputElement>('input[aria-label="Name the box"]')!;
  type(name, "Billing");
  press("Tab", name);
  await tick();
  flushSync();
  expect(finder(t)).not.toBeNull();
  expect(document.activeElement).toBe(finder(t));
  type(finder(t)!, "store");
  press("Enter", finder(t)!);
  await tick();
  expect(got[1]).toEqual({ op: "set", id: "billing", key: "kind", value: "store" });
});
