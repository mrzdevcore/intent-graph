// @vitest-environment happy-dom
// The add form speaks the kinds table (`view.kinds`); warnings have their own field.
import { expect, test } from "vitest";
import { flushSync, mount } from "svelte";
import Map from "../src/map/Map.svelte";
import type { EditApi } from "../src/api";
import type { KindView, View } from "../src/map/types";
import shop from "./fixtures/shop-view.json";

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
  mount(Map, { target, props: { view, size: { w: 1000, h: 800 }, edit: api } });
  flushSync();
  return target;
}
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
