// @vitest-environment happy-dom
import { expect, test, vi } from "vitest";
import { flushSync, mount } from "svelte";
import Pads from "../src/map/Pads.svelte";
import Map from "../src/map/Map.svelte";
import { Camera } from "../src/map/glide";
import { CARD, fillsBeside, fitBeside, inBox, padsOf, type Pad } from "../src/map/pads";
import { fitInner, fitTo, showsInside, worldFrame } from "../src/map/camera";
import type { Frame, MapArrow, MapBox, Mark, View } from "../src/map/types";

const box = (id: string, parent?: string): MapBox => ({
  id, parent, level: "container", madeOf: [], files: 0, kind: { value: "service", source: "ig" }, title: { value: id.toUpperCase(), source: "ig" },
});
const arrow = (from: string, to: string, parts?: MapArrow[], parent = "system"): MapArrow => ({ id: `${from}->${to}`, parent, from, to, kind: "calls", count: 0, kinds: ["calls"], parts });
const make = (arrows: MapArrow[], marks: Mark[] = [], extra: MapBox[] = []): View => ({
  version: 1,
  boxes: [box("system"), box("host", "system"), box("traefik", "host"), box("traefik/r1", "traefik"), box("traefik/r2", "traefik"), box("public", "system"), box("a", "public"), box("b", "public"), ...extra],
  arrows, marks, placed: { frames: {}, inner: { traefik: [100, 100] }, regions: [] }, notices: [], warnings: [], kinds: [],
});

const F = { w: 200, h: 200 };
const frames: Record<string, Frame> = { "traefik/r1": { x: 0, y: 0, w: 40, h: 20 }, "traefik/r2": { x: 0, y: 60, w: 40, h: 20 } };
const cf = (id: string) => frames[id];
const on = new Set(["state"]);
const out = make([arrow("host", "public", [arrow("traefik/r1", "a"), arrow("traefik/r2", "b")])]);

test("each part crossing the edge is a pad, wired from the child holding its end", () => {
  // traefik holds routes r1, r2; the arrow host->public has parts r1->a, r2->b
  expect(padsOf(out, "traefik", F, cf, on).map((p) => [p.side, p.other, p.child])).toEqual([["out", "a", "traefik/r1"], ["out", "b", "traefik/r2"]]);
});

test("a part's end deep in the box is wired from the direct child holding it", () => {
  const v = make([arrow("host", "public", [arrow("traefik/r1", "a")])]);
  expect(padsOf(v, "host", F, cf, on).map((p) => p.child)).toEqual(["traefik"]);
});

test("a pad sits level with its child", () => {
  const t = fitInner(F, 100, 100);
  const pads = padsOf(out, "traefik", F, cf, on);
  expect(pads[0].y).toBeCloseTo(t.y + t.s * 10);
  expect(pads[1].y).toBeCloseTo(t.y + t.s * 70);
  expect(inBox(out, "traefik", F)(frames["traefik/r2"]).y).toBeCloseTo(t.y + t.s * 60);
});

test("the frames given are the ones used: a moved child takes its pad along", () => {
  const moved = (id: string) => (id === "traefik/r1" ? { ...frames[id], y: 40 } : undefined);
  const t = fitInner(F, 100, 100);
  expect(padsOf(make([arrow("host", "public", [arrow("traefik/r1", "a")])]), "traefik", F, moved, on)[0].y).toBeCloseTo(t.y + t.s * 50);
});

test("incoming parts are on the other side", () => {
  const v = make([arrow("public", "host", [arrow("a", "traefik/r1")])]);
  expect(padsOf(v, "traefik", F, cf, on).map((p) => [p.side, p.other, p.title, p.child])).toEqual([["in", "a", "A", "traefik/r1"]]);
});

test("an arrow with no parts is one pad, with no wire inside", () => {
  const v = make([arrow("host", "public")]);
  const pads = padsOf(v, "host", F, cf, on);
  expect(pads.map((p) => [p.side, p.other, p.title])).toEqual([["out", "public", "PUBLIC"]]);
  expect(pads[0].child).toBeUndefined();
  expect(pads[0].y).toBeGreaterThanOrEqual(0);
  expect(pads[0].y).toBeLessThanOrEqual(F.h);
});

test("an arrow inside the box, or elsewhere, has no pad", () => {
  const v = make([arrow("traefik/r1", "traefik/r2", undefined, "traefik"), arrow("a", "b", undefined, "public")]);
  expect(padsOf(v, "traefik", F, cf, on)).toEqual([]);
});

test("a target marked down gives a down pad, unless its layer is off", () => {
  const marks: Mark[] = [{ layer: "state", target: "a", state: "down" }, { layer: "state", target: "b", state: "ok" }];
  const v = make([arrow("host", "public", [arrow("traefik/r1", "a"), arrow("traefik/r2", "b")])], marks);
  expect(padsOf(v, "traefik", F, cf, on).map((p) => p.down)).toEqual([true, false]);
  expect(padsOf(v, "traefik", F, cf, new Set()).map((p) => p.down)).toEqual([false, false]);
});

test("a box with no connections has no pads", () => {
  expect(padsOf(make([]), "traefik", F, cf, on)).toEqual([]);
});

test("pads too close for their height spread out, in the order of their children", () => {
  const close = (id: string) => ({ x: 0, y: id === "traefik/r1" ? 41 : 40, w: 40, h: 20 });
  const pads = padsOf(out, "traefik", { w: 100, h: 200 }, close, on);
  expect(pads.map((p) => p.other)).toEqual(["b", "a"]);
  expect(pads[1].y - pads[0].y).toBeGreaterThan(20);
});

test("forty pads stay apart and inside the box", () => {
  const extra = Array.from({ length: 40 }, (_, i) => box(`t${i}`, "public"));
  const crowded = make(extra.map((b) => arrow("host", "public", [arrow("x", b.id)])), [], [...extra, box("x", "host")]);
  const ys = padsOf(crowded, "x", { w: 100, h: 200 }, cf, on).map((p) => p.y);
  expect(new Set(ys).size).toBe(40);
  expect(Math.min(...ys)).toBeGreaterThanOrEqual(0);
  expect(Math.max(...ys)).toBeLessThanOrEqual(200);
});

function drawn(n: number, onopen = (_: string) => {}) {
  const extra = Array.from({ length: n }, (_, i) => box(`t${i}`, "public"));
  const v = make(extra.map((b) => arrow("host", "public", [arrow("x", b.id)])), extra.map((b) => ({ layer: "state", target: b.id, state: "down" })), [...extra, box("x", "host")]);
  const target = document.createElement("div");
  mount(Pads, { target, props: { view: v, id: "x", frame: { w: 100, h: 200 }, childFrame: cf, on, scale: 1, onopen } });
  flushSync();
  return target;
}

test("a pad is a button that opens the far box, and a stopped one says so", () => {
  const onopen = vi.fn(), t = drawn(1, onopen), b = t.querySelector("button")!;
  expect(b.textContent).toContain("stopped");
  expect(b.getAttribute("aria-label")).toBe("Open T0, stopped");
  b.click();
  expect(onopen).toHaveBeenCalledWith("t0");
});

test("above 12 pads the labels stay and the chips go", () => {
  expect(drawn(12).querySelectorAll(".chip").length).toBe(12);
  const t = drawn(13);
  expect(t.querySelectorAll(".chip").length).toBe(0);
  expect(t.querySelectorAll(".name").length).toBe(13);
});

// a frame wider than its inside, as a group of one is; at 16px a rem: card 356, labels 192
const wide = { x: 100, y: 100, w: 264, h: 152 };
const V = { w: 1440, h: 900 };
const pad = (side: Pad["side"]): Pad => ({ side, other: "o", title: "O", down: false, y: 0 });
const onScreen = (f: Frame, c: { k: number; x: number; y: number }) => ({ l: f.x * c.k + c.x, r: (f.x + f.w) * c.k + c.x, t: f.y * c.k + c.y, b: (f.y + f.h) * c.k + c.y });

test("with a card at 1440x900 the frame and its labels stay left of the card, the rest on screen", () => {
  const s = onScreen(wide, fitBeside(wide, V, [pad("out"), pad("in")], CARD, 16));
  expect(s.r + 192).toBeLessThanOrEqual(1440 - 356);
  expect(s.l - 192).toBeGreaterThanOrEqual(0);
  expect(s.t).toBeGreaterThanOrEqual(0);
  expect(s.b).toBeLessThanOrEqual(900);
});

test("room for labels is kept only on a side with pads", () => {
  const s = onScreen(wide, fitBeside(wide, V, [pad("out")], CARD, 16));
  expect(s.l).toBeCloseTo(16);
  expect(s.r).toBeCloseTo(1440 - 356 - 16 - 192);
});

test("with no card the width is free, as at the phone width where the card moves below", () => {
  const s = onScreen(wide, fitBeside(wide, V, [pad("out")], 0, 16));
  expect(s.r).toBeCloseTo(1440 - 16 - 192);
});

test("a narrow window still opens the box, or its pads would not draw at all", () => {
  const c = fitBeside(wide, { w: 1024, h: 768 }, [pad("out"), pad("in")], CARD, 16);
  expect(showsInside(wide.w * c.k)).toBe(true);
});

test("a frame of no size gives a finite camera, as fitTo does", () => {
  const c = fitBeside({ x: 0, y: 0, w: 0, h: 0 }, V, [pad("out")], CARD, 16);
  for (const n of [c.k, c.x, c.y]) expect(Number.isFinite(n)).toBe(true);
});

test("a box fitted beside the card counts as filling it, one that is too small does not", () => {
  // 1440x900, card 356: the middle of the space left of the card is x 542
  expect(fillsBeside({ x: 16, y: 203, w: 860, h: 495 }, V, CARD, 16)).toBe(true);
  // both labels and the wide card: the 560 floor still passes
  expect(fillsBeside({ x: 200, y: 289, w: 560, h: 322 }, V, 31, 16)).toBe(true);
  // too small to be open: under 560
  expect(fillsBeside({ x: 300, y: 300, w: 500, h: 300 }, V, CARD, 16)).toBe(false);
  // open but off to one side of x 542
  expect(fillsBeside({ x: 600, y: 203, w: 800, h: 495 }, V, CARD, 16)).toBe(false);
});

const trailOf = (t: HTMLElement) => [...t.querySelectorAll(".crumbs button")].map((c) => c.textContent!.trim());

function opened() {
  const v = make([arrow("host", "public", [arrow("traefik", "a")]), arrow("leaf", "public")], [], [box("alone", "system"), box("leaf", "system")]);
  v.placed.frames = {
    host: { x: 0, y: 0, w: 264, h: 152 }, traefik: { x: 10, y: 10, w: 100, h: 60 }, public: { x: 400, y: 0, w: 264, h: 152 },
    a: { x: 10, y: 10, w: 100, h: 60 }, b: { x: 130, y: 10, w: 100, h: 60 }, alone: { x: 0, y: 300, w: 264, h: 152 }, leaf: { x: 400, y: 300, w: 264, h: 152 },
  };
  v.placed.inner = { host: [120, 80], public: [240, 80], system: [700, 500] };
  const camera = new Camera(), target = document.createElement("div");
  document.body.append(target);
  const m = mount(Map, { target, props: { view: v, camera, size: V } });
  flushSync();
  const slot = (id: string) => target.querySelector(`[data-box="${id}"]`) as HTMLElement;
  const open = (id: string) => {
    (slot(id).querySelector("button") as HTMLButtonElement).click();
    flushSync();
    m.tick(performance.now() + 5000);
    flushSync();
  };
  return { v, camera, target, m, slot, open };
}

test("opening a box with pads frames the whole box beside the card", () => {
  const { v, camera, open } = opened();
  open("host");
  const s = onScreen(worldFrame("host", v), camera.cam);
  expect(s.r + 192).toBeLessThanOrEqual(1440 - 356);
  expect(s.t).toBeGreaterThanOrEqual(0);
  expect(s.b).toBeLessThanOrEqual(900);
});

test("opening a box with no pads keeps today's fit", () => {
  const { v, camera, open } = opened();
  open("alone");
  expect(camera.cam).toEqual(fitTo(worldFrame("alone", v), V));
});

test("a press on a leaf box's pad that moves a little does not drag the box, and still clicks", () => {
  const { camera, target, slot, open } = opened();
  open("leaf");
  const left = slot("leaf").style.left, cam = camera.cam;
  const p = target.querySelector('[data-pad="public"]') as HTMLButtonElement;
  p.dispatchEvent(new PointerEvent("pointerdown", { button: 0, clientX: 500, clientY: 400, bubbles: true }));
  window.dispatchEvent(new PointerEvent("pointermove", { clientX: 520, clientY: 400 }));
  window.dispatchEvent(new PointerEvent("pointerup", {}));
  flushSync();
  expect(slot("leaf").style.left).toBe(left);
  expect(camera.cam).toEqual(cam);
  p.click();
  flushSync();
  expect(target.querySelector(".details")?.textContent).toContain("PUBLIC");
});

test("the breadcrumb names the box opened beside the card, though it fills under 60% of the view", () => {
  const { v, camera, target, open } = opened();
  open("host");
  const s = onScreen(worldFrame("host", v), camera.cam);
  // at 1440 wide: left 16, right 876, so 860 of 1440
  expect([Math.round(s.l), Math.round(s.r)]).toEqual([16, 876]);
  expect((s.r - s.l) / 1440).toBeLessThan(0.6);
  expect(trailOf(target)).toEqual(["SYSTEM", "HOST"]);
  open("traefik");
  expect(trailOf(target)).toEqual(["SYSTEM", "HOST", "TRAEFIK"]);
});

test("closing the card leaves the breadcrumb as it was", () => {
  const { target, open } = opened();
  open("host");
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
  flushSync();
  expect(target.querySelector(".details")).toBeNull();
  expect(trailOf(target)).toEqual(["SYSTEM", "HOST"]);
});

test("zooming by hand to a box with no pads keeps the 60% rule", () => {
  const { v, camera, target, m } = opened();
  // alone is 264 wide and sits where host would: 860 of 1440 on screen, holding the view's centre
  const f = worldFrame("alone", v), at = (w: number) => {
    const k = w / f.w;
    camera.set({ k, x: 720 - (f.x + f.w / 2) * k, y: 450 - (f.y + f.h / 2) * k });
    m.tick(performance.now());
    flushSync();
    return trailOf(target);
  };
  expect(at(860)).toEqual(["SYSTEM"]);
  expect(at(865)).toEqual(["SYSTEM", "ALONE"]);
});
