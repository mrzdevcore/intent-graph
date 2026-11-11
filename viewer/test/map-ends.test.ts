// @vitest-environment happy-dom
import { afterEach, expect, test, vi } from "vitest";
import { flushSync, mount } from "svelte";
import Controls from "../src/map/Controls.svelte";
import { endLabel, endMarks } from "../src/map/db/ends";
import type { End } from "../src/map/types";
import { loadNotation, saveNotation } from "../src/map/db/notation";

const ends: End[] = ["one", "zeroOne", "many0", "many1"];

afterEach(() => vi.unstubAllGlobals());

test("labels", () => {
  expect(ends.map(endLabel)).toEqual(["1", "0..1", "0..n", "1..n"]);
});

test("crow's foot with a ring, pointing right", () => {
  const m = endMarks("many0", [10, 20], [1, 0]);
  expect(m.lines).toBe("M24 20L10 27M24 20L10 20M24 20L10 13");
  expect(m.rings).toEqual([[31, 20]]);
});

test("crow's foot with a bar, pointing down", () => {
  const m = endMarks("many1", [10, 20], [0, 1]);
  expect(m.lines).toBe("M10 34L3 20M10 34L10 20M10 34L17 20M3 38L17 38");
  expect(m.rings).toEqual([]);
});

test("one is two bars", () => {
  expect(endMarks("one", [0, 0], [1, 0]).lines).toBe("M8 7L8 -7M13 7L13 -7");
  expect(endMarks("one", [0, 0], [0, 1]).lines).toBe("M-7 8L7 8M-7 13L7 13");
});

test("zeroOne is a bar and a ring", () => {
  const h = endMarks("zeroOne", [0, 0], [1, 0]);
  expect(h.lines).toBe("M8 7L8 -7");
  expect(h.rings).toEqual([[18, 0]]);
  const v = endMarks("zeroOne", [0, 0], [0, -1]);
  expect(v.lines).toBe("M7 -8L-7 -8");
  expect(v.rings).toEqual([[0, -18]]);
});

test("out is normalised", () => {
  for (const e of ends) expect(endMarks(e, [3, 4], [5, 0])).toEqual(endMarks(e, [3, 4], [1, 0]));
});

test("a zero vector gives numbers, not NaN", () => {
  for (const e of ends) {
    const m = endMarks(e, [3, 4], [0, 0]);
    expect(m.lines).not.toContain("NaN");
    expect(m.rings.flat().every(Number.isFinite)).toBe(true);
  }
});

test("the Ends setting is offered, reports a change and persists per browser", () => {
  const kept: Record<string, string> = {};
  vi.stubGlobal("localStorage", {
    getItem: (k: string) => kept[k] ?? null, setItem: (k: string, v: string) => (kept[k] = v), removeItem: (k: string) => delete kept[k],
  });
  expect(loadNotation()).toBe("crowsfoot");
  const target = document.createElement("div");
  document.body.append(target);
  const seen: string[] = [];
  mount(Controls, { target, props: {
    crumbs: [], layers: [], on: new Set<string>(), oncrumb: () => {}, ontoggle: () => {}, wheelPref: "auto", onwheel: () => {},
    endsPref: loadNotation(), onends: (n: string) => { seen.push(n); saveNotation(n as never); },
  } });
  flushSync();
  const sel = [...target.querySelectorAll("label")].find((l) => l.textContent?.startsWith("Ends"))!.querySelector("select")!;
  expect([...sel.options].map((o) => o.textContent)).toEqual(["Crow's foot", "Labels", "Both"]);
  expect(sel.value).toBe("crowsfoot");
  sel.value = "both";
  sel.dispatchEvent(new Event("change", { bubbles: true }));
  expect(seen).toEqual(["both"]);
  expect(loadNotation()).toBe("both");
  saveNotation("crowsfoot");
  expect(loadNotation()).toBe("crowsfoot");
});
