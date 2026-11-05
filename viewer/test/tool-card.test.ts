// @vitest-environment happy-dom
import { expect, test } from "vitest";
import { flushSync, mount } from "svelte";
import ToolCard from "../src/components/chat/ToolCard.svelte";
import type { Show } from "../src/model/show";

const input = { title: "Where Graphify is used", nodes: ["a"], steps: [{ target: "a", text: "x" }, { target: "a", text: "y" }] };
const card = (call: object, onshow?: (s: Show) => void) => {
  const target = document.createElement("div");
  mount(ToolCard, { target, props: { call: { toolCallId: "1", title: "mcp__plugin_ig_ig__ig_show", rawInput: input, ...call }, root: "", onshow } });
  flushSync();
  return target;
};

test("an answer's card names it, counts its steps and draws it again", () => {
  const shown: Show[] = [];
  const t = card({ status: "completed" }, (s) => shown.push(s));
  expect(t.querySelector(".ttitle")!.textContent).toBe("Where Graphify is used");
  expect(t.querySelector(".chip")!.textContent).toBe("2 steps");
  (t.querySelector("button") as HTMLElement).click();
  expect(shown[0].title).toBe("Where Graphify is used");
});

test("a refused answer says so and offers nothing to draw", () => {
  const t = card({ status: "failed" }, () => {});
  expect(t.querySelector(".chip")!.textContent).toBe("refused");
  expect(t.querySelector("button")).toBeNull();
});

test("another tool's card is as it was", () => {
  const t = card({ title: "Read a.ts", status: "completed", rawInput: {} }, () => {});
  expect(t.querySelector(".ttitle")!.textContent).toBe("Read a.ts");
  expect(t.querySelector(".chip")!.textContent).toBe("completed");
  expect(t.querySelector("button")).toBeNull();
});
