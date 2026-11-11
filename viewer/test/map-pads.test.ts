// @vitest-environment happy-dom
import { expect, test, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import Pads from "../src/map/Pads.svelte";
import Map from "../src/map/Map.svelte";
import { Camera } from "../src/map/glide";
import { CARD, fillsBeside, fitBeside, inBox, padPoint, padsOf, wiresOf, type Pad } from "../src/map/pads";
import { cuts } from "../src/map/shapes/inside";
import type { P } from "../src/map/shapes/path";
import { HEAD, OPEN, fitInner, fitTo, showsInside, worldFrame } from "../src/map/camera";
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
  expect(padsOf(out, "traefik", F, cf, on).map((p) => [p.way, p.edge, p.other, p.child])).toEqual([["out", "r", "a", "traefik/r1"], ["out", "r", "b", "traefik/r2"]]);
});

test("a part's end deep in the box is wired from the direct child holding it", () => {
  const v = make([arrow("host", "public", [arrow("traefik/r1", "a")])]);
  expect(padsOf(v, "host", F, cf, on).map((p) => p.child)).toEqual(["traefik"]);
});

test("with no frames to face, a pad sits on the right going out, level with its child", () => {
  const t = fitInner(F, 100, 100);
  const pads = padsOf(out, "traefik", F, cf, on);
  expect(pads[0].at).toBeCloseTo(t.y + t.s * 10);
  expect(pads[1].at).toBeCloseTo(t.y + t.s * 70);
  expect(inBox(out, "traefik", F)(frames["traefik/r2"]).y).toBeCloseTo(t.y + t.s * 60);
});

test("the frames given are the ones used: a moved child takes its pad along", () => {
  const moved = (id: string) => (id === "traefik/r1" ? { ...frames[id], y: 40 } : undefined);
  const t = fitInner(F, 100, 100);
  expect(padsOf(make([arrow("host", "public", [arrow("traefik/r1", "a")])]), "traefik", F, moved, on)[0].at).toBeCloseTo(t.y + t.s * 50);
});

test("with no frames to face, incoming parts are on the left", () => {
  const v = make([arrow("public", "host", [arrow("a", "traefik/r1")])]);
  expect(padsOf(v, "traefik", F, cf, on).map((p) => [p.way, p.edge, p.other, p.title, p.child])).toEqual([["in", "l", "a", "A", "traefik/r1"]]);
});

test("an arrow with no parts is one pad, with no wire inside", () => {
  const v = make([arrow("host", "public")]);
  const pads = padsOf(v, "host", F, cf, on);
  expect(pads.map((p) => [p.way, p.other, p.title])).toEqual([["out", "public", "PUBLIC"]]);
  expect(pads[0].child).toBeUndefined();
  expect(pads[0].at).toBeGreaterThanOrEqual(0);
  expect(pads[0].at).toBeLessThanOrEqual(F.h);
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
  expect(pads[1].at - pads[0].at).toBeGreaterThan(20);
});

test("crowded pads fold: at most 12 a side, each a pad's height from the next, inside the box", () => {
  const extra = Array.from({ length: 40 }, (_, i) => box(`t${i}`, "public"));
  const crowded = make(extra.map((b) => arrow("host", "public", [arrow("x", b.id)])), [], [...extra, box("x", "host")]);
  // tall, as drawn at 264 wide, and flat: what fits folds too
  for (const frame of [{ w: 100, h: 200 }, { w: 264, h: 152 }, { w: 560, h: 120 }]) {
    const pads = padsOf(crowded, "x", frame, cf, on), shown = pads.filter((p) => !p.more), fold = pads.filter((p) => p.more);
    expect(fold.length).toBe(1);
    expect(shown.length).toBeLessThanOrEqual(12);
    expect(shown.length + fold[0].more!).toBe(40);
    expect(fold[0].title).toBe(`+${fold[0].more} more`);
    expect(pads.at(-1)).toBe(fold[0]);
    const ys = pads.map((p) => p.at);
    ys.slice(1).forEach((y, i) => expect(y - ys[i]).toBeGreaterThanOrEqual((frame.w * 24) / OPEN));
    expect(Math.min(...ys)).toBeGreaterThanOrEqual(0);
    expect(Math.max(...ys)).toBeLessThanOrEqual(frame.h);
  }
  expect(padsOf(crowded, "x", { w: 100, h: 200 }, cf, on).filter((p) => !p.more).length).toBe(12);
});

test("both ways to one far box are one two-way pad, and 12 fold nothing", () => {
  const extra = Array.from({ length: 12 }, (_, i) => box(`t${i}`, "public"));
  const v = make([
    ...extra.map((b) => arrow("host", "public", [arrow("x", b.id)])),
    ...extra.map((b) => arrow("public", "host", [arrow(b.id, "x")])),
  ], [], [...extra, box("x", "host")]);
  const pads = padsOf(v, "x", { w: 100, h: 200 }, cf, on);
  expect(pads.length).toBe(12);
  expect(pads.some((p) => p.more)).toBe(false);
  expect(pads.every((p) => p.way === "both" && p.ends.length === 2)).toBe(true);
});

test("two parts with the same side, far box and child are one pad", () => {
  const v = make([arrow("host", "public", [arrow("traefik/r1", "a"), arrow("traefik/r2", "a")]), arrow("system", "public", [arrow("traefik/r1", "a")])]);
  expect(padsOf(v, "traefik", F, cf, on).map((p) => [p.way, p.edge, p.other, p.child])).toEqual([["out", "r", "a", "traefik/r1"], ["out", "r", "a", "traefik/r2"]]);
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

test("the fold links to no box: a click selects this one, its card listing them all, and it has no wire", () => {
  const onopen = vi.fn(), t = drawn(14, onopen);
  expect(t.querySelectorAll("line").length).toBe(12);
  const b = t.querySelector('[data-pad=""]') as HTMLButtonElement;
  expect(b.textContent).toContain("+2 more");
  expect(b.getAttribute("aria-label")).toBe("2 more connections, listed on this box's card");
  b.click();
  expect(onopen).toHaveBeenCalledWith("x");
});

test("at the width limit rounding never closes the box", () => {
  // 275 * (560 / 275) is just under 560
  expect(showsInside(275 * (OPEN / 275))).toBe(false);
  const c = fitBeside({ x: 0, y: 0, w: 275, h: 150 }, { w: 400, h: 300 }, [pad("out"), pad("in")], CARD, 16);
  expect(showsInside(275 * c.k)).toBe(true);
});

// a frame wider than its inside, as a group of one is; at 16px a rem: card 356, labels 192
const wide = { x: 100, y: 100, w: 264, h: 152 };
const V = { w: 1440, h: 900 };
const pad = (way: "in" | "out"): Pad => ({ way, edge: way === "out" ? "r" : "l", other: "o", title: "O", down: false, at: 0, arrows: [], ends: [] });
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

const site = (() => {
  const kids = ["c0", "c1", "c2", "c3", "c4", "c5"];
  const v: View = {
    version: 1,
    boxes: [box("system"), box("node3", "system"), ...kids.map((k) => box(k, "node3")), ...["up", "down", "west", "east"].map((b) => box(b, "system"))],
    arrows: [
      arrow("node3", "up", [arrow("c0", "up"), arrow("c4", "up")]),
      arrow("up", "node3", [arrow("up", "c0")]),
      arrow("node3", "east", [arrow("c2", "east"), arrow("c5", "east")]),
      arrow("east", "node3", [arrow("east", "c5")]),
      arrow("west", "node3", [arrow("west", "c0"), arrow("west", "c3")]),
      arrow("node3", "down", [arrow("c3", "down"), arrow("c4", "down"), arrow("c5", "down")]),
      arrow("down", "node3", [arrow("down", "c4")]),
    ],
    marks: [], notices: [], warnings: [], kinds: [],
    placed: {
      frames: {
        node3: { x: 700, y: 500, w: 600, h: 400 }, up: { x: 900, y: 0, w: 200, h: 100 }, down: { x: 900, y: 1300, w: 200, h: 100 },
        west: { x: 0, y: 600, w: 300, h: 200 }, east: { x: 1700, y: 600, w: 300, h: 200 },
        ...Object.fromEntries(kids.map((k, i) => [k, { x: 20 + (i % 3) * 200, y: 20 + Math.floor(i / 3) * 200, w: 160, h: 100 }])),
      },
      inner: { system: [2000, 1400], node3: [600, 340] }, regions: [],
    },
  };
  const frame = { w: 600, h: 400 }, child = (id: string) => v.placed.frames[id];
  const pads = padsOf(v, "node3", frame, child, on);
  const place = inBox(v, "node3", frame);
  const kidFrames = Object.fromEntries(kids.map((k) => [k, place(child(k))]));
  return { v, frame, child, pads, kids: kidFrames };
})();

test("a pad sits on the edge facing its far box, one per far box and child, both ways one two-way pad", () => {
  const got = site.pads.map((p) => [p.edge, p.other, p.child, p.way]);
  expect(got).toEqual([
    ["r", "east", "c2", "out"], ["r", "east", "c5", "both"],
    ["l", "west", "c0", "in"], ["l", "west", "c3", "in"],
    ["t", "up", "c0", "both"], ["t", "up", "c4", "out"],
    ["b", "down", "c3", "out"], ["b", "down", "c4", "both"], ["b", "down", "c5", "out"],
  ]);
  expect(site.pads.reduce((n, p) => n + p.ends.length, 0)).toBe(12);
  expect(site.pads.find((p) => p.other === "up" && p.child === "c0")!.ends).toEqual([
    { from: "c0", to: "up", key: "from" }, { from: "up", to: "c0", key: "to" },
  ]);
});

test("along an edge pads follow their children's places, ties by the far box's", () => {
  for (const e of ["t", "b", "l", "r"] as const) {
    const mine = site.pads.filter((p) => p.edge === e), along = (f: Frame) => (e === "t" || e === "b" ? f.x + f.w / 2 : f.y + f.h / 2);
    mine.slice(1).forEach((p, i) => {
      expect(p.at).toBeGreaterThan(mine[i].at);
      expect(along(site.kids[p.child!])).toBeGreaterThanOrEqual(along(site.kids[mine[i].child!]));
    });
  }
  const v: View = { ...site.v, boxes: [...site.v.boxes, box("up2", "system")], arrows: [...site.v.arrows, arrow("node3", "up2", [arrow("c0", "up2")])] };
  v.placed = { ...v.placed, frames: { ...v.placed.frames, up2: { x: 600, y: 0, w: 200, h: 100 } } };
  expect(padsOf(v, "node3", site.frame, site.child, on).filter((p) => p.edge === "t").map((p) => [p.other, p.child])).toEqual([["up2", "c0"], ["up", "c0"], ["up", "c4"]]);
});

const segs = (pts: P[]) => pts.slice(1).map((q, i) => [pts[i], q] as [P, P]);
function crossing([a, b]: [P, P], [c, d]: [P, P]) {
  const o = (p: P, q: P, r: P) => Math.sign((q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0]));
  const [d1, d2, d3, d4] = [o(c, d, a), o(c, d, b), o(a, b, c), o(a, b, d)];
  if (d1 * d2 < 0 && d3 * d4 < 0) return true;
  if (d1 === 0 && d2 === 0) {
    const k = a[0] === b[0] && c[0] === d[0] ? 1 : 0;
    const [lo, hi] = [Math.max(Math.min(a[k], b[k]), Math.min(c[k], d[k])), Math.min(Math.max(a[k], b[k]), Math.max(c[k], d[k]))];
    return hi - lo > 1e-6;
  }
  return false;
}

for (const shape of ["circuit", "elbow", "curved", "straight"] as const)
  test(`${shape}: each pad is wired to its child, through no other child, and no two inside wires cross`, () => {
    const ws = wiresOf(site.pads, site.frame, site.kids, shape);
    site.pads.forEach((p, i) => {
      const w = ws[i]!, f = site.kids[p.child!], end = w.pts[w.pts.length - 1];
      expect(w.pts[0]).toEqual(padPoint(p, site.frame));
      expect(end[0] >= f.x - 1e-6 && end[0] <= f.x + f.w + 1e-6 && end[1] >= f.y - 1e-6 && end[1] <= f.y + f.h + 1e-6).toBe(true);
      for (const [id, k] of Object.entries(site.kids)) if (id !== p.child) for (const [a, b] of segs(w.pts)) expect(cuts(a, b, k), `${p.other}/${p.child} through ${id}`).toBe(false);
    });
    ws.forEach((a, i) => ws.forEach((b, j) => {
      if (j <= i) return;
      for (const s of segs(a!.pts)) for (const t of segs(b!.pts)) expect(crossing(s, t), `${site.pads[i].child}/${site.pads[i].other} x ${site.pads[j].child}/${site.pads[j].other}`).toBe(false);
    }));
  });

test("a wire into a lower part goes round the part above it", () => {
  const i = site.pads.findIndex((p) => p.edge === "t" && p.child === "c4");
  const w = wiresOf(site.pads, site.frame, site.kids, "circuit")[i]!;
  expect(w.pts.length).toBeGreaterThan(2);
  for (const [a, b] of segs(w.pts)) expect(cuts(a, b, site.kids.c1)).toBe(false);
});

test("an inside wire is in the arrow's own shape, else the level's", () => {
  const v: View = { ...site.v, arrows: site.v.arrows.map((a) => ({ ...a, parts: a.parts?.map((p) => (p.from === "west" && p.to === "c0" ? { ...p, shape: "straight" as const } : p)) })) };
  const pads = padsOf(v, "node3", site.frame, site.child, on), ws = wiresOf(pads, site.frame, site.kids, "circuit");
  const i = pads.findIndex((p) => p.other === "west" && p.child === "c0");
  expect(ws[i]!.pts.length).toBe(2);
  expect(ws[i]!.d).not.toContain("C");
});

function siteDrawn(props: Record<string, unknown> = {}) {
  const target = document.createElement("div");
  document.body.append(target);
  mount(Pads, { target, props: { view: site.v, id: "node3", frame: site.frame, childFrame: site.child, on, scale: 1, onopen: () => {}, ...props } });
  flushSync();
  return target;
}

test("hovering a pad or its inside wire lights both, and names its arrows outside; the outside arrow hovered lights them too", () => {
  const onhot = vi.fn(), t = siteDrawn({ onhot });
  const pad = t.querySelector('[data-pad="up"][data-way="both"]') as HTMLElement;
  pad.dispatchEvent(new PointerEvent("pointerenter"));
  flushSync();
  expect(onhot).toHaveBeenLastCalledWith(["node3->up", "up->node3"]);
  expect(pad.classList.contains("lit")).toBe(true);
  const lit = t.querySelectorAll("g.lit");
  expect(lit.length).toBe(1);
  expect(lit[0].getAttribute("data-wire")).toBe("up");
  pad.dispatchEvent(new PointerEvent("pointerleave"));
  flushSync();
  expect(onhot).toHaveBeenLastCalledWith(null);
  expect(t.querySelectorAll("g.lit, .pad.lit").length).toBe(0);
  const wire = t.querySelectorAll('[data-wire="east"] .hit')[0];
  wire.dispatchEvent(new PointerEvent("pointerenter"));
  flushSync();
  expect(onhot).toHaveBeenLastCalledWith(["node3->east"]);
  expect(t.querySelectorAll(".pad.lit").length).toBe(1);

  const out = siteDrawn({ hot: "node3->down" });
  expect([...out.querySelectorAll(".pad.lit")].map((b) => b.getAttribute("data-pad"))).toEqual(["down", "down", "down"]);
  expect(out.querySelectorAll("g.lit").length).toBe(3);
});

const stubView = () => {
  const v = make([arrow("host", "public")]);
  v.placed.inner = { host: [100, 100] };
  return v;
};
const traefikAt = (() => {
  const f = inBox(stubView(), "host", F)({ x: 50, y: 50, w: 40, h: 20 });
  return { clientX: f.x + f.w / 2, clientY: f.y + f.h / 2 };
})();

function stubbed(onassign?: (e: unknown) => void) {
  const v = stubView();
  const target = document.createElement("div");
  document.body.append(target);
  mount(Pads, { target, props: { view: v, id: "host", frame: F, childFrame: (id: string) => (id === "traefik" ? { x: 50, y: 50, w: 40, h: 20 } : undefined), on, scale: 1, onopen: () => {}, onassign } });
  flushSync();
  const root = target.querySelector(".pads") as HTMLElement;
  root.getBoundingClientRect = () => ({ left: 0, top: 0, width: 200, height: 200, right: 200, bottom: 200, x: 0, y: 0, toJSON: () => ({}) });
  return target;
}

test("a pad with no part shows which part?, and its stub dragged onto a part writes the arrow from it", () => {
  const onassign = vi.fn(), t = stubbed(onassign);
  expect(t.querySelector(".ask")?.textContent).toBe("?");
  const end = t.querySelector("[data-stub]")!;
  end.dispatchEvent(new PointerEvent("pointerdown", { button: 0, clientX: 176, clientY: 100, bubbles: true }));
  window.dispatchEvent(new PointerEvent("pointermove", traefikAt));
  flushSync();
  expect(t.querySelector("rect.pull")).not.toBeNull();
  window.dispatchEvent(new PointerEvent("pointerup", traefikAt));
  flushSync();
  expect(onassign).toHaveBeenCalledWith({ op: "arrowset", from: "host", to: "public", key: "from", value: "traefik" });
});

test("dropped on no part, the stub writes nothing; read-only, it only shows the ?", () => {
  const onassign = vi.fn(), t = stubbed(onassign);
  t.querySelector("[data-stub]")!.dispatchEvent(new PointerEvent("pointerdown", { button: 0, clientX: 176, clientY: 100, bubbles: true }));
  window.dispatchEvent(new PointerEvent("pointerup", { clientX: 10, clientY: 190 }));
  expect(onassign).not.toHaveBeenCalled();
  const ro = stubbed();
  expect(ro.querySelector(".ask")).not.toBeNull();
  expect(ro.querySelector("[data-stub]")).toBeNull();
});

test("on the map, a pad hovered lights its outside arrow gold, and the outside arrow hovered lights the pad", () => {
  const { target, open } = opened();
  open("host");
  const line = () => target.querySelector('[data-arrow="host->public"] g[data-state] path:not(.hit)') as SVGPathElement;
  const pad = target.querySelector('[data-box="host"] [data-pad="a"]') as HTMLElement;
  expect(line().style.stroke).not.toBe("var(--map-trace)");
  pad.dispatchEvent(new PointerEvent("pointerenter"));
  flushSync();
  expect(line().style.stroke).toBe("var(--map-trace)");
  pad.dispatchEvent(new PointerEvent("pointerleave"));
  flushSync();
  expect(line().style.stroke).not.toBe("var(--map-trace)");
  target.querySelector('[data-arrow="host->public"] .hit')!.dispatchEvent(new PointerEvent("pointerenter"));
  flushSync();
  expect(pad.classList.contains("lit")).toBe(true);
  expect(line().style.stroke).toBe("var(--map-trace)");
});

for (const shape of ["circuit", "elbow", "curved", "straight"] as const)
  test(`${shape}: no inside wire runs across the open box's title, but a top pad's own way in`, () => {
    const head = Math.min(HEAD, site.frame.h / 4);
    const ws = wiresOf(site.pads, site.frame, site.kids, shape);
    site.pads.forEach((p, i) => segs(ws[i]!.pts).forEach(([a, b], k) => {
      if (Math.min(a[1], b[1]) >= head - 1e-6) return;
      expect(p.edge === "t" && k === 0 && a[0] === p.at && b[0] === p.at, `${p.other}/${p.child} segment ${k}`).toBe(true);
    }));
  });

test("a top pad far from its part runs under the title, not across it", () => {
  const frame = { w: 400, h: 300 }, kids: Record<string, Frame> = { a: { x: 60, y: 40, w: 80, h: 80 }, b: { x: 300, y: 40, w: 80, h: 80 } };
  const pads: Pad[] = [{ way: "out", edge: "t", other: "far", title: "", child: "b", down: false, at: 10, arrows: [], ends: [] }];
  for (const shape of ["circuit", "elbow", "curved", "straight"] as const) {
    const w = wiresOf(pads, frame, kids, shape)[0]!;
    segs(w.pts).forEach(([a, b], k) => expect(Math.min(a[1], b[1]) >= 30 - 1e-6 || (k === 0 && a[0] === 10 && b[0] === 10), `${shape} segment ${k}`).toBe(true));
    for (const [a, b] of segs(w.pts)) expect(cuts(a, b, kids.a)).toBe(false);
  }
});

const packedPad = (edge: Pad["edge"], at: number, child: string): Pad => ({ way: "out", edge, other: `${edge}${at}`, title: "", child, down: false, at, arrows: [], ends: [] });

test("children packed closer than the clearance are wired between them or left a stub, and no wire cuts one", () => {
  const frame = { w: 400, h: 244 }, kids: Record<string, Frame> = {};
  for (let i = 0; i < 6; i++) kids[`k${i}`] = { x: 2 + (i % 3) * 132, y: 34 + Math.floor(i / 3) * 104, w: 128, h: 100 };
  const pads = [packedPad("t", 66, "k4"), packedPad("l", 190, "k5"), packedPad("b", 330, "k1"), packedPad("r", 84, "k3")];
  for (const shape of ["circuit", "elbow", "curved", "straight"] as const) {
    const ws = wiresOf(pads, frame, kids, shape);
    expect(ws[0], shape).not.toBeNull();
    pads.forEach((p, i) => {
      for (const [id, k] of Object.entries(kids)) if (id !== p.child) for (const [a, b] of segs(ws[i]?.pts ?? [])) expect(cuts(a, b, k), `${shape} ${p.child} through ${id}`).toBe(false);
    });
  }
});

test("a part walled in by touching children gets no inside wire, never a line through them", () => {
  const frame = { w: 400, h: 300 }, kids: Record<string, Frame> = {};
  for (let i = 0; i < 9; i++) kids[`k${i}`] = { x: 50 + (i % 3) * 100, y: 50 + Math.floor(i / 3) * 80, w: 100, h: 80 };
  const ws = wiresOf([packedPad("t", 200, "k4"), packedPad("l", 170, "k4")], frame, kids, "circuit");
  expect(ws).toEqual([null, null]);
});

test("a two-way pad takes the first shape its parts set", () => {
  const v: View = { ...site.v, arrows: site.v.arrows.map((a) => ({ ...a, parts: a.parts?.map((p) => (p.from === "up" && p.to === "c0" ? { ...p, shape: "straight" as const } : p)) })) };
  expect(padsOf(v, "node3", site.frame, site.child, on).find((p) => p.other === "up" && p.child === "c0")!.shape).toBe("straight");
});

test("pads closed while one is hovered let go of its outside arrows", () => {
  const onhot = vi.fn(), target = document.createElement("div");
  document.body.append(target);
  const c = mount(Pads, { target, props: { view: site.v, id: "node3", frame: site.frame, childFrame: site.child, on, scale: 1, onopen: () => {}, onhot } });
  flushSync();
  target.querySelector('[data-pad="east"]')!.dispatchEvent(new PointerEvent("pointerenter"));
  flushSync();
  expect(onhot).toHaveBeenLastCalledWith(["node3->east"]);
  unmount(c);
  expect(onhot).toHaveBeenLastCalledWith(null);
});
