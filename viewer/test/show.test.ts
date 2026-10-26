import { expect, test } from "vitest";
import type { ViewData } from "../src/data";
import { index } from "../src/model/graph";
import { applyShow, parseShow, showFromTool, showReducer, type Show, type ShowState } from "../src/model/show";
import fixture from "./fixture-data.json";

const ix = index(fixture as ViewData);
const APP = "apps/web/src/main.tsx#App";
const ADD = "packages/core/src/math.ts#add";

const show = (o: Partial<Show> = {}): Show => ({
  seq: 1, title: "How App adds", nodes: [APP, ADD], edges: [[APP, ADD]],
  steps: [{ target: APP, text: "App renders" }, { target: APP + "|" + ADD, text: "App calls add" }, { target: ADD, text: "add adds" }],
  mode: "focus", ...o,
});

test("applyShow highlights nodes and edges and orders steps", () => {
  const a = applyShow(ix, show());
  expect(a.mode).toBe("focus");
  expect([...a.nodes]).toEqual([APP, ADD]);
  expect([...a.edges]).toEqual([APP + "|" + ADD]);
  expect(a.steps.map((s) => [s.n, s.text])).toEqual([[1, "App renders"], [2, "App calls add"], [3, "add adds"]]);
  expect(a.missing).toEqual([]);
  expect(applyShow(ix, show({ mode: "architecture" })).mode).toBe("architecture");
  expect(applyShow(ix, show({ mode: "chain" })).mode).toBe("focus");
});

test("missing ids are dropped and listed", () => {
  const gone = "packages/core/src/gone.ts#x";
  const a = applyShow(ix, show({ nodes: [APP, gone], edges: [[APP, gone]], steps: [{ target: gone, text: "?" }, { target: APP, text: "a" }] }));
  expect([...a.nodes]).toEqual([APP]);
  expect(a.edges.size).toBe(0);
  expect(a.steps).toEqual([{ n: 2, target: APP, text: "a" }]);
  expect(a.missing).toEqual([gone]);
});

const view = (mode: "changed" | "architecture", sel: string[]) => ({ mode, selection: sel, scope: null, focus: [] });

test("show then clear keeps the mode; show then back restores the previous mode and selection", () => {
  const empty: ShowState = { current: null, previous: null };
  const shown = showReducer(empty, { t: "show", s: show(), view: view("changed", [ADD]) });
  expect(shown.state.current?.title).toBe("How App adds");
  expect(shown.restore).toBeNull();
  const cleared = showReducer(shown.state, { t: "clear" });
  expect(cleared.state).toEqual(empty);
  expect(cleared.restore).toBeNull();
  const back = showReducer(shown.state, { t: "back" });
  expect(back.state).toEqual(empty);
  expect(back.restore).toEqual(view("changed", [ADD]));
});

test("a second show keeps the first previous view", () => {
  const one = showReducer({ current: null, previous: null }, { t: "show", s: show(), view: view("changed", []) });
  const two = showReducer(one.state, { t: "show", s: show({ seq: 2, title: "second" }), view: view("architecture", [APP]) });
  expect(two.state.current?.title).toBe("second");
  expect(showReducer(two.state, { t: "back" }).restore).toEqual(view("changed", []));
});

test("the chip parses ig_show tool input", () => {
  const rawInput = { title: "t", nodes: [APP], edges: [APP + "|calls|" + ADD, [APP, ADD]], steps: [{ target: APP, text: "x" }], mode: "chain" };
  const s = showFromTool({ title: "mcp__plugin_ig_ig__ig_show", rawInput });
  expect(s).toMatchObject({ title: "t", nodes: [APP], edges: [[APP, ADD], [APP, ADD]], mode: "chain" });
  expect(showFromTool({ title: "ig_show", rawInput: { title: "t", nodes: [APP] } })).toMatchObject({ edges: [], steps: [], mode: "focus" });
  expect(showFromTool({ title: "ig_focus", rawInput })).toBeNull();
  expect(showFromTool({ title: "ig_show", rawInput: { title: "t", nodes: [] } })).toBeNull();
  expect(showFromTool({ title: "ig_show", rawInput: "nope" })).toBeNull();
});

test("a host show without edges or steps gets []", () => {
  expect(parseShow({ seq: 4, title: "t", nodes: [APP], mode: "architecture" })).toMatchObject({ edges: [], steps: [], mode: "architecture" });
  expect(parseShow({ title: "t" })).toBeNull();
});
