// @vitest-environment happy-dom
import { expect, test } from "vitest";
import { flushSync, mount } from "svelte";
import Map from "../src/map/Map.svelte";
import { Camera } from "../src/map/glide";
import type { View } from "../src/map/types";
import type { ArrowRef, BoxRef, ChangedBox, Diff } from "../src/model/diff";
import { parseShow } from "../src/model/show";
import shop from "./fixtures/shop-view.json";

const size = { w: 1000, h: 800 };
const box = (id: string, parent: string, title = id): BoxRef => ({ id, title, parent });
const changed = (id: string, parent: string, n: number): ChangedBox => ({ ...box(id, parent), files: [], filesChanged: n, direct: true });
const arrow = (from: string, to: string): ArrowRef => ({ from, to, kind: "calls", fromTitle: from, toTitle: to });

function diff(over: Partial<Diff["boxes"] & { arrowsRemoved: ArrowRef[] }> = {}): Diff {
  return {
    base: "159c1b6" + "0".repeat(33), head: "fc0a6a6" + "0".repeat(33), source: "code", summary: "2 boxes changed",
    boxes: { added: over.added ?? [], removed: over.removed ?? [], changed: over.changed ?? [] },
    arrows: { added: [], removed: over.arrowsRemoved ?? [] },
    intent: { status: "same", added: [], removed: [], moved: [], changed: [], arrowsAdded: [], arrowsRemoved: [] },
    problems: { new: [], fixed: [] },
  };
}

function setup(d: Diff, props: object = {}) {
  const view = structuredClone(shop) as unknown as View;
  const target = document.createElement("div");
  document.body.append(target);
  const m = mount(Map, { target, props: { view, camera: new Camera(), size, diff: d, ...props } });
  flushSync();
  m.tick(performance.now() + 5000);
  flushSync();
  return target;
}
const slot = (t: HTMLElement, id: string) => t.querySelector(`[data-box="${CSS.escape(id)}"]`) as HTMLElement;
const card = (t: HTMLElement) => t.querySelector('[aria-label="What this change does"]') as HTMLElement | null;
const press = (key: string) => {
  window.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true }));
  flushSync();
};
const SOME = diff({ changed: [changed("api/domain-order", "api", 3), changed("api/infra-prisma", "api", 1)], added: [box("api/routers", "api", "Routers")] });

test("a diff marks changed and added boxes and fades the rest", () => {
  const t = setup(SOME);
  expect(slot(t, "api/domain-order").classList.contains("diff-changed")).toBe(true);
  expect(slot(t, "api/domain-order").textContent).toContain("3 files");
  expect(slot(t, "api/routers").classList.contains("diff-added")).toBe(true);
  expect(slot(t, "api/routers").textContent).toContain("new");
  expect(slot(t, "api/domain-payment").classList.contains("faded")).toBe(true);
  expect(slot(t, "api/domain-order").classList.contains("faded")).toBe(false);
});

test("the card walks the steps with the arrow keys", () => {
  const t = setup(SOME);
  const c = card(t)!;
  expect(c.textContent).toContain("159c1b6 → fc0a6a6");
  expect(c.textContent).toContain("Boxes");
  const current = () => c.querySelector('[aria-current="step"]')!.textContent;
  expect(current()).toContain("api/domain-order");
  press("ArrowRight");
  expect(current()).toContain("api/infra-prisma");
  press("Escape");
  expect(card(t)).toBeNull();
});

test("removed boxes show on their parent", () => {
  const t = setup(diff({ removed: [box("api/plan", "api"), box("api/review", "api")] }));
  expect(slot(t, "api").textContent).toContain("−2 removed here");
});

test("a ghost arrow is drawn dashed", () => {
  const t = setup(diff({ arrowsRemoved: [arrow("api/root", "api/domain-order")] }));
  const ghost = t.querySelector('[data-arrow^="ghost:"]');
  expect(ghost).not.toBeNull();
  expect(ghost!.querySelector("path[stroke-dasharray]")).not.toBeNull();
});

test("no change shows the plain map", () => {
  const t = setup(diff());
  expect(t.querySelector(".faded")).toBeNull();
  expect(card(t)!.textContent).toContain("No change to the architecture");
});

test("a diff wins over a show", () => {
  const t = setup(SOME, { show: parseShow({ title: "Where it is used", nodes: ["api/root"] }) });
  expect(card(t)).not.toBeNull();
  expect(t.querySelector('[aria-label="The answer"]')).toBeNull();
});
