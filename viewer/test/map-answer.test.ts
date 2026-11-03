import { expect, test } from "vitest";
import { answerFrame, answerOf, arrowLit, arrowNumber, boxStates, stepFrame } from "../src/map/answer";
import { placer } from "../src/map/camera";
import { parseShow, type Show } from "../src/model/show";
import type { MapArrow, View } from "../src/map/types";
import shop from "./fixtures/shop-view.json";

const view = shop as unknown as View;
const show = (raw: object): Show => parseShow({ title: "t", ...raw })!;
const place = placer(view);
const frame = (id: string) => place.frame(id);

test("each step names its box by title, an arrow by its two ends, and says when it is gone", () => {
  const a = answerOf(view, show({ nodes: ["api", "web"], steps: [
    { target: "api", text: "one" },
    { target: "web|api", text: "two" },
    { target: "nope", text: "three" },
  ] }));
  const title = (id: string) => view.boxes.find((b) => b.id === id)!.title.value;
  expect(a.steps.map((s) => [s.title, s.text, s.gone])).toEqual([
    [title("api"), "one", false],
    [`${title("web")} → ${title("api")}`, "two", false],
    ["nope", "three", true],
  ]);
});

test("an answer without steps lists its boxes", () => {
  const a = answerOf(view, show({ nodes: ["api", "gone-box"] }));
  expect(a.steps.map((s) => [s.target, s.text, s.gone])).toEqual([["api", "", false], ["gone-box", "", true]]);
  expect([...a.boxes]).toEqual(["api"]);
});

test("a box is in the answer, holds part of it, sits inside it, or is out of it", () => {
  const inner = view.boxes.find((b) => b.parent === "api")!.id;
  const sibling = view.boxes.find((b) => b.parent === "api" && b.id !== inner)!.id;
  const a = answerOf(view, show({ nodes: [inner] }));
  const states = boxStates(view, a);
  expect(states.get(inner)).toEqual({ in: true, holds: 0, under: [] });
  expect(states.get("api")).toEqual({ in: false, holds: 1, under: [1] });
  expect(states.has(sibling)).toBe(false);
  expect(states.has("web")).toBe(false);
  expect(boxStates(view, answerOf(view, show({ nodes: ["api"] }))).get(inner)).toEqual({ in: false, holds: 0, under: [] });
});

test("an arrow lights by its ends or by one of the arrows drawn in it", () => {
  const arrow = (from: string, to: string, parts: MapArrow[] = []) =>
    ({ id: `${from}->${to}`, parent: "system", from, to, kind: "calls", count: 1, kinds: [], parts }) as MapArrow;
  const merged = arrow("web", "api", [arrow("web/x", "api/y")]);
  expect(arrowLit(answerOf(view, show({ nodes: ["web"], edges: [["web/x", "api/y"]] })), merged)).toBe(true);
  expect(arrowLit(answerOf(view, show({ nodes: ["web"], edges: [["web", "api"]] })), merged)).toBe(true);
  expect(arrowLit(answerOf(view, show({ nodes: ["web"] })), merged)).toBe(false);
  expect(arrowNumber(answerOf(view, show({ nodes: ["web"], steps: [{ target: "web", text: "" }, { target: "web|api", text: "" }] })), merged)).toBe(2);
});

test("the answer's frame: the box holding it all, or the top boxes that hold its parts", () => {
  const kids = view.boxes.filter((b) => b.parent === "api").map((b) => b.id);
  expect(answerFrame(answerOf(view, show({ nodes: kids.slice(0, 2) })), view, frame)).toEqual({ id: "api", frame: frame("api") });
  expect(answerFrame(answerOf(view, show({ nodes: [kids[0]] })), view, frame)).toEqual({ id: "api", frame: frame("api") });
  const across = answerFrame(answerOf(view, show({ nodes: [kids[0], "web"] })), view, frame)!;
  const [a, w] = [frame("api"), frame("web")];
  expect(across.id).toBeUndefined();
  expect(across.frame.x).toBe(Math.min(a.x, w.x));
  expect(across.frame.w).toBe(Math.max(a.x + a.w, w.x + w.w) - across.frame.x);
  expect(answerFrame(answerOf(view, show({ nodes: ["nope"] })), view, frame)).toBeNull();
});

test("a step moves the camera only when its target is off screen or too small to read", () => {
  const kid = view.boxes.find((b) => b.parent === "api")!.id;
  const a = answerOf(view, show({ nodes: [kid], steps: [{ target: kid, text: "x" }, { target: "web|api", text: "y" }, { target: "nope", text: "z" }] }));
  const f = frame(kid), screen = { w: 1000, h: 800 };
  const near = { k: 400 / f.w, x: -f.x * (400 / f.w), y: -f.y * (400 / f.w) };
  expect(stepFrame(a, 0, view, frame, near, screen)).toBeNull();
  expect(stepFrame(a, 0, view, frame, { k: 0.001, x: 0, y: 0 }, screen)).toEqual({ id: "api", frame: frame("api") });
  const both = stepFrame(a, 1, view, frame, near, screen)!;
  expect(both.id).toBeUndefined();
  expect(both.frame.w).toBeGreaterThan(0);
  expect(stepFrame(a, 2, view, frame, near, screen)).toBeNull();
});
