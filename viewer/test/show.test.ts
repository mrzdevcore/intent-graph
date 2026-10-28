import { expect, test } from "vitest";
import { parseShow, showFromTool } from "../src/model/show";

const APP = "apps/web/src/main.tsx#App";
const ADD = "packages/core/src/math.ts#add";

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

test("a tool input over ig_show's caps is refused, like the tool refuses it", () => {
  const ok = { title: "t", nodes: [APP] };
  const step = (text: string) => ({ target: APP, text });
  expect(parseShow({ ...ok, nodes: Array(60).fill(APP), steps: Array(40).fill(step("x".repeat(500))), title: "é".repeat(200) })).not.toBeNull();
  for (const bad of [{ nodes: Array(61).fill(APP) }, { steps: Array(41).fill(step("x")) }, { title: "x".repeat(201) },
    { steps: [step("x".repeat(501))] }]) expect(parseShow({ ...ok, ...bad })).toBeNull();
});
