// @vitest-environment happy-dom
import { expect, test, vi } from "vitest";
import { flushSync, mount } from "svelte";
import Map from "../src/map/Map.svelte";
import { Camera } from "../src/map/glide";
import { fitTo, innerTransform, worldFrame } from "../src/map/camera";
import type { View } from "../src/map/types";
import shop from "./fixtures/shop-view.json";

const EVIL = "<img src=x onerror=alert(1)>";
const size = { w: 1000, h: 800 };

function setup(change = (_: View) => {}) {
  const view = structuredClone(shop) as unknown as View;
  change(view);
  view.boxes.find((b) => b.id === "stripe")!.title.value = EVIL;
  const camera = new Camera();
  const target = document.createElement("div");
  document.body.append(target);
  const m = mount(Map, { target, props: { view, camera, size } });
  flushSync();
  return { view, camera, target, m };
}
const box = (t: HTMLElement, id: string) => t.querySelector(`[data-box="${CSS.escape(id)}"]`);

test("at Fit the containers render and api's inside is not mounted", () => {
  const { view, target } = setup();
  const fit = fitTo(worldFrame("system", view), size);
  expect((target.querySelector(".world") as HTMLElement).style.transform).toContain(`scale(${fit.k})`);
  for (const id of ["api", "api-client", "postgres", "stripe", "web"]) expect(box(target, id)).not.toBeNull();
  expect(box(target, "api/application-checkout")).toBeNull();
  expect(target.textContent).toContain("Some boxes are ig's guess");
});

test("gliding into api mounts its components and the failing rules arrow", () => {
  const { view, camera, target, m } = setup();
  camera.to(fitTo(worldFrame("api", view), size), 0);
  m.tick(800);
  flushSync();
  expect(box(target, "api/application-checkout")).not.toBeNull();
  const line = target.querySelector(`[data-arrow="${CSS.escape("api/domain-order->api/infra-prisma")}"] g[data-state] > path`) as SVGPathElement;
  expect(line.style.stroke).toBe("var(--map-bad)");
  // still no code level: checkout's own inside is too small on screen
  expect(box(target, "apps/api/src/application/checkout/checkout.ts#checkout")).toBeNull();
});

test("clicking a box opens the details card; titles are text", () => {
  const { target } = setup();
  (box(target, "stripe")!.querySelector("button") as HTMLButtonElement).click();
  flushSync();
  const card = target.querySelector(".details")!;
  expect(card.textContent).toContain(EVIL);
  expect(card.querySelector("img")).toBeNull();
  expect(target.querySelector("img")).toBeNull();
});

test("the details card lists connections and needs attention", () => {
  const { view, camera, target, m } = setup();
  camera.to(fitTo(worldFrame("api", view), size), 0);
  m.tick(800);
  flushSync();
  (box(target, "api/domain-order")!.querySelector("button") as HTMLButtonElement).click();
  flushSync();
  const card = target.querySelector(".details")!.textContent!;
  expect(card).toContain("breaks: domain → infrastructure");
  expect(card).toContain("apps/api/src/domain/order/**");
  expect(card).toContain("Uses ·");
  // a neighbour is a button that opens it
  const link = target.querySelector(".details .links button") as HTMLButtonElement;
  const name = link.querySelector(".name")!.textContent;
  link.click();
  flushSync();
  expect(target.querySelector(".details b")!.textContent).toBe(name);
});

test("a health layer turns api amber and its value shows in the card; the layer can be switched off", () => {
  const view = structuredClone(shop) as unknown as View;
  view.marks.push({ layer: "health", target: "api", state: "warn", value: "p95 820ms" });
  const target = document.createElement("div");
  document.body.append(target);
  mount(Map, { target, props: { view, camera: new Camera(), size } });
  flushSync();
  const btn = () => box(target, "api")!.querySelector("button") as HTMLButtonElement;
  expect(btn().style.borderColor).toBe("var(--map-warn)");
  btn().click();
  flushSync();
  expect(target.querySelector(".details")!.textContent).toContain("p95 820ms");
  const health = [...target.querySelectorAll(".layers label")].find((l) => l.textContent!.includes("health"))!;
  (health.querySelector("input") as HTMLInputElement).click();
  flushSync();
  expect(btn().style.borderColor).not.toBe("var(--map-warn)");
});

test("clicking a box with an inside and no pads frames the inside: it fills >= 80% of the view", () => {
  const { view, camera, target, m } = setup((v) => { v.arrows = v.arrows.filter((a) => a.parent !== "system"); });
  (box(target, "api")!.querySelector("button") as HTMLButtonElement).click();
  m.tick(performance.now() + 1000);
  flushSync();
  const t = innerTransform("api", view), [iw, ih] = view.placed.inner["api"], k = camera.cam.k;
  expect(Math.max((iw * t.s * k) / size.w, (ih * t.s * k) / size.h)).toBeGreaterThanOrEqual(0.8 - 1e-9);
  expect(box(target, "api/application-checkout")).not.toBeNull();
});

test("Esc closes the details card and focus returns to the box", () => {
  const { target } = setup();
  const btn = box(target, "stripe")!.querySelector("button") as HTMLButtonElement;
  btn.click();
  flushSync();
  expect(document.activeElement).toBe(target.querySelector(".details"));
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
  flushSync();
  expect(target.querySelector(".details")).toBeNull();
  expect(document.activeElement).toBe(btn);
});

test("a bad map (a parent loop, a duplicate id) neither hangs nor throws", () => {
  const view = structuredClone(shop) as unknown as View;
  const api = view.boxes.find((b) => b.id === "api")!;
  view.boxes.push({ ...api, id: "zzz", parent: "zzz" }, { ...api, id: "x", parent: "y" }, { ...api, id: "y", parent: "x" });
  view.boxes.push({ ...api, parent: "api" });
  for (const id of ["zzz", "x", "y"]) view.placed.frames[id] = view.placed.frames.api;
  view.marks.push({ layer: "rules", target: "zzz", state: "bad" }, { layer: "rules", target: "x", state: "bad" });
  const camera = new Camera();
  const target = document.createElement("div");
  document.body.append(target);
  const m = mount(Map, { target, props: { view, camera, size } });
  flushSync();
  expect(target.querySelector(`[data-box="api"]`)).not.toBeNull();
  // into api, where the duplicate `api` (its own child) would open forever
  camera.to(fitTo(worldFrame("api", view), size), 0);
  m.tick(800);
  flushSync();
  expect(target.querySelectorAll(`[data-box="api"]`).length).toBe(1);
  expect(target.querySelector(`[data-box="api/application-checkout"]`)).not.toBeNull();
});

test("arrows rest faint; the box under the pointer lights its own, with labels, and dims the others", () => {
  const { view, camera, target, m } = setup();
  camera.to(fitTo(worldFrame("api", view), size), 0);
  m.tick(800);
  flushSync();
  const hub = "api/application-checkout", id = "api/application-checkout->api/infra-prisma";
  const states = () => [...target.querySelectorAll("[data-arrow] g[data-state]")].map((g) => g.getAttribute("data-state"));
  const labels = (state: string) => [...target.querySelectorAll(`[data-label] g[data-state="${state}"]`)].map((g) => g.closest("[data-label]")!.getAttribute("data-label")!);
  expect(new Set(states())).toEqual(new Set(["rest"]));
  expect(labels("rest").filter((a) => a.startsWith("api/"))).toEqual([]);
  box(target, hub)!.querySelector("button")!.dispatchEvent(new PointerEvent("pointerover", { bubbles: true }));
  flushSync();
  expect(labels("lit")).toContain(id);
  expect(labels("lit").every((a) => a.split("->").includes(hub))).toBe(true);
  expect(labels("rest").filter((a) => a.startsWith("api/"))).toEqual([]);
  const mine = view.arrows.filter((a) => a.parent === "api" && (a.from === hub || a.to === hub)).length;
  expect(states().filter((s) => s === "lit").length).toBe(mine);
  expect(states().filter((s) => s === "dim").length).toBe(view.arrows.filter((a) => a.parent === "api").length - mine);
});

test("an open box's inside sits below its header", () => {
  const { view } = setup();
  const f = worldFrame("api", view), t = innerTransform("api", view);
  expect(t.y - f.y).toBeGreaterThan(0);
  const [, ih] = view.placed.inner["api"];
  expect(t.y + ih * t.s).toBeLessThanOrEqual(f.y + f.h + 1e-6);
});

test("zoomed into a function, its box shows its code; the details card does too", () => {
  const { view, camera, target, m } = setup();
  const id = "apps/api/src/application/checkout/checkout.ts#checkout";
  expect(box(target, id)?.querySelector("pre")).toBeFalsy();
  camera.to(fitTo(worldFrame(id, view), size), 0);
  m.tick(800);
  flushSync();
  const pre = box(target, id)!.querySelector("pre")!;
  expect(pre.textContent).toContain("return pay.charge(total(items));");
  expect(pre.querySelector(".ln")!.textContent).toBe("6");
  // coloured by the theme's code tokens, as text
  const kw = [...pre.querySelectorAll("span")].find((e) => e.textContent === "return")!;
  expect(kw.getAttribute("style")).toContain("--code-token-");
  box(target, id)!.querySelector("button")!.click();
  flushSync();
  const card = target.querySelector(".details")!;
  expect(card.textContent).toContain("apps/api/src/application/checkout/checkout.ts:6");
  expect(card.querySelector("pre")!.textContent).toContain("orderRepo.save(item)");
});

test("deep inside, only the covering box's inside is drawn, at a modest scale", () => {
  const { view, camera, target, m } = setup();
  const all = target.querySelectorAll("[data-box]").length;
  const id = "apps/api/src/application/checkout/checkout.ts#checkout";
  camera.to(fitTo(worldFrame(id, view), size), 0);
  m.tick(800);
  flushSync();
  expect(box(target, id)).not.toBeNull();
  expect(box(target, "stripe")).toBeNull(); // off screen, not drawn
  expect(box(target, "api")).toBeNull(); // covers the view: only its inside is drawn
  const k = Number((target.querySelector(".world") as HTMLElement).style.transform.match(/scale\(([\d.e+-]+)\)/)![1]);
  expect(k).toBeLessThan(camera.cam.k);
  expect(target.querySelectorAll("[data-box]").length).toBeLessThan(all + 10);
});

test("a React component is a box of its own; the details card lists a part box's parts", () => {
  const { view, target, m, camera } = setup();
  const id = "apps/web/pages/checkout.tsx#CheckoutPage";
  expect(view.boxes.find((b) => b.id === id)!.kind.value).toBe("component");
  view.parts = { [id]: [{ name: "open", kind: "useState", line: 4, detail: "false" }] };
  camera.to(fitTo(worldFrame(view.boxes.find((b) => b.id === id)!.parent!, view), size), 0);
  m.tick(800);
  flushSync();
  box(target, id)!.querySelector("button")!.click();
  flushSync();
  expect(target.querySelector(".details")!.textContent).toContain("useState open false");
});

test("dragging a closed box moves it and its arrows, keeps it, and Reset layout undoes it", () => {
  // Node's own localStorage (unset without --localstorage-file) hides happy-dom's
  const kept: Record<string, string> = {};
  vi.stubGlobal("localStorage", {
    getItem: (k: string) => kept[k] ?? null, setItem: (k: string, v: string) => (kept[k] = v), removeItem: (k: string) => delete kept[k],
  });
  const { view, camera, target, m } = setup();
  camera.to(fitTo(worldFrame("api", view), size), 0);
  m.tick(800);
  flushSync();
  const id = "api/domain-order";
  const slot = () => box(target, id) as HTMLElement;
  const line = () => target.querySelector(`[data-arrow="${CSS.escape("api/domain-order->api/infra-prisma")}"] g[data-state] > path`)!.getAttribute("d");
  const [left, before] = [slot().style.left, line()];
  const k = innerTransform("api", view).s * camera.cam.k;
  const btn = slot().querySelector("button")!;
  btn.dispatchEvent(new PointerEvent("pointerdown", { button: 0, clientX: 500, clientY: 400, bubbles: true }));
  window.dispatchEvent(new PointerEvent("pointermove", { clientX: 500 - 20 * k, clientY: 400 }));
  window.dispatchEvent(new PointerEvent("pointerup", {}));
  flushSync();
  expect(parseFloat(slot().style.left)).toBeCloseTo(parseFloat(left) - 20);
  expect(line()).not.toBe(before);
  // the camera did not pan, and the trailing click opened nothing
  expect(camera.cam).toEqual(fitTo(worldFrame("api", view), size));
  btn.click();
  flushSync();
  expect(target.querySelector(".details")).toBeNull();
  const places = JSON.parse(kept["ig-places:" + view.boxes.find((b) => b.id === "system")!.title.value]);
  expect(places[id].x).toBeCloseTo(view.placed.frames[id].x - 20);
  (([...target.querySelectorAll(".tools button")].find((b) => b.textContent === "Reset layout")) as HTMLButtonElement).click();
  m.tick(performance.now() + 1000);
  flushSync();
  expect(slot().style.left).toBe(left);
  expect(kept).toEqual({});
  vi.unstubAllGlobals();
});

/** A system of one container, `groups` boxes in a grid inside it, each holding `each` code boxes. */
function big(groups: number, each: number): View {
  const view: View = { version: 1, boxes: [], arrows: [], marks: [], notices: [], warnings: [], kinds: [], placed: { frames: {}, inner: {}, regions: [] } };
  const f = (title: string) => ({ value: title, source: "ig" as const });
  const add = (id: string, parent: string | undefined, level: View["boxes"][0]["level"]) =>
    view.boxes.push({ id, parent, level, madeOf: [], files: 1, kind: f("folder") as never, title: f(id) });
  add("system", undefined, "system");
  add("c", "system", "container");
  view.placed.inner.system = [1000, 800];
  view.placed.frames.c = { x: 0, y: 0, w: 1000, h: 800 };
  view.placed.inner.c = [1000, 800];
  for (let g = 0; g < groups; g++) {
    const id = `c/g${g}`;
    add(id, "c", "component");
    view.placed.frames[id] = { x: (g % 4) * 250, y: Math.floor(g / 4) * 200, w: 200, h: 150 };
    view.placed.inner[id] = [400, 300];
    if (g) view.arrows.push({ id: `c/g0->${id}`, parent: "c", from: "c/g0", to: id, kind: "uses", count: 1, kinds: ["uses"] });
    for (let i = 0; i < each; i++) {
      add(`${id}/b${i}`, id, "code");
      view.placed.frames[`${id}/b${i}`] = { x: (i % 20) * 20, y: Math.floor(i / 20) * 20, w: 15, h: 15 };
    }
  }
  return view;
}

test("dragging a box in a map of thousands stays well inside a frame", () => {
  vi.stubGlobal("localStorage", { getItem: () => null, setItem: () => {}, removeItem: () => {} });
  const view = big(12, 400);
  const camera = new Camera(), target = document.createElement("div");
  document.body.append(target);
  const m = mount(Map, { target, props: { view, camera, size } });
  flushSync();
  camera.to(fitTo(worldFrame("c", view), size), 0);
  m.tick(800);
  flushSync();
  const left = (box(target, "c/g5") as HTMLElement).style.left;
  // the best of three runs: one slow run on a busy machine is not the code
  const runs = [0, 1, 2].map(() => {
    const btn = box(target, "c/g5")!.querySelector("button")!;
    btn.dispatchEvent(new PointerEvent("pointerdown", { button: 0, clientX: 500, clientY: 400, bubbles: true }));
    window.dispatchEvent(new PointerEvent("pointermove", { clientX: 510, clientY: 400 }));
    flushSync();
    const t0 = performance.now();
    for (let i = 0; i < 30; i++) {
      window.dispatchEvent(new PointerEvent("pointermove", { clientX: 510 + i, clientY: 400 + i }));
      flushSync();
    }
    const each = (performance.now() - t0) / 30;
    const t1 = performance.now();
    window.dispatchEvent(new PointerEvent("pointerup", {}));
    flushSync();
    // letting go lays the whole map out again, once
    return { each, drop: performance.now() - t1 };
  });
  vi.unstubAllGlobals();
  expect((box(target, "c/g5") as HTMLElement).style.left).not.toBe(left);
  expect(Math.min(...runs.map((r) => r.each))).toBeLessThan(8);
  expect(Math.min(...runs.map((r) => r.drop))).toBeLessThan(100);
});

test("the search opens a box it finds, and lights the path A to B on the map", () => {
  const { target } = setup();
  const input = target.querySelector('input[aria-label="Search the map"]') as HTMLInputElement;
  const type = (q: string) => {
    input.focus();
    input.value = q;
    input.dispatchEvent(new Event("input", { bubbles: true }));
    flushSync();
  };
  const enter = () => {
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    flushSync();
  };
  type("postg");
  expect(target.querySelector(".search .drop")!.textContent).toContain("Postgres");
  enter();
  expect(target.querySelector(".details b")!.textContent).toBe("Postgres");

  type("web to postgres");
  expect(target.querySelector(".search .drop")!.textContent).toContain("2 steps");
  enter();
  expect(target.querySelector(".details")).toBeNull();
  for (const id of ["web", "api", "postgres"]) expect(box(target, id)!.classList).toContain("traced");
  expect(box(target, "api-client")!.classList).toContain("faded");
  // Esc in the field clears the path
  input.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  flushSync();
  expect(target.querySelector(".slot.traced, .slot.faded")).toBeNull();
});

/** shop with CheckoutPage's part boxes (what it keeps, what it renders), mounted. */
function withAnatomy() {
  const view = structuredClone(shop) as unknown as View;
  const id = "apps/web/pages/checkout.tsx#CheckoutPage";
  const page = view.boxes.find((b) => b.id === id)!;
  for (const [key, title] of [["state", "State"], ["renders", "Renders"]]) {
    view.boxes.push({ ...page, id: `${id}/${key}`, parent: id, kind: { value: "parts", source: "ig" }, title: { value: title, source: "ig" }, logo: undefined });
  }
  view.parts = {
    [`${id}/state`]: [{ name: "open", kind: "useState", line: 4, detail: "false" }],
    [`${id}/renders`]: [{ name: "CheckoutPage", kind: "tag", line: 9 }, { name: "PayButton", kind: "tag", line: 12 }],
  };
  // another component it renders
  const pay = view.boxes.find((b) => b.id === `${id}.pay`)!;
  pay.kind = { value: "component", source: "ig" };
  pay.title = { value: "PayButton", source: "ig" };
  const camera = new Camera();
  const target = document.createElement("div");
  document.body.append(target);
  const m = mount(Map, { target, props: { view, camera, size } });
  flushSync();
  const go = (to: string) => {
    camera.to(fitTo(worldFrame(to, view), size), 0);
    m.tick(800);
    flushSync();
  };
  return { view, target, id, go };
}

test("a component's details card sums up what it keeps and renders; its face does not", () => {
  const { target, id, go } = withAnatomy();
  go("web/pages");
  const face = box(target, id)!.textContent!;
  expect(face).not.toContain("keeps open");
  expect(face).toContain("1 useState");
  (box(target, id)!.querySelector("button") as HTMLButtonElement).click();
  flushSync();
  const card = target.querySelector(".details")!.textContent!;
  expect(card).toContain("keeps open");
  expect(card).toContain("renders CheckoutPage");
});

test("a store is drawn as a cylinder and a group as a frame", () => {
  const view = structuredClone(shop) as unknown as View;
  view.boxes.find((b) => b.id === "api-client")!.kind.value = "group";
  const target = document.createElement("div");
  document.body.append(target);
  mount(Map, { target, props: { view, camera: new Camera(), size } });
  flushSync();
  expect((box(target, "postgres") as HTMLElement).dataset.shape).toBe("store");
  expect((box(target, "api-client") as HTMLElement).dataset.shape).toBe("group");
  expect((box(target, "web") as HTMLElement).dataset.shape).toBe("app");
});

test("the card links a rendered tag to its component, and closes once you walk into another box", () => {
  const { target, go } = withAnatomy();
  const input = target.querySelector('input[aria-label="Search the map"]') as HTMLInputElement;
  input.focus();
  input.value = "Renders";
  input.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  flushSync();
  go("web/pages");
  expect(target.querySelector(".details b")!.textContent).toBe("Renders");
  // its own component is not a link, another one it renders is
  const tags = [...target.querySelectorAll(".details .tag")];
  expect(tags.map((t) => t.textContent)).toEqual(["PayButton"]);
  (tags[0] as HTMLButtonElement).click();
  flushSync();
  expect(target.querySelector(".details b")!.textContent).toBe("PayButton");
  // walking into the api (outside the web) closes it
  go("api");
  expect(target.querySelector(".details")).toBeNull();
});
