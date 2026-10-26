// @vitest-environment happy-dom
/// <reference types="node" />
import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { flushSync, mount, unmount, type Component } from "svelte";
import { afterEach, expect, test } from "vitest";
import Board from "../src/board/Board.svelte";
import EditOverlay from "../src/board/EditOverlay.svelte";
import Labels from "../src/board/Labels.svelte";
import type { Label } from "../src/board/engine";
import { loadEngine, type Importer, type Pkg, type RawEngine } from "../src/board/host";

const OUT = (o: object = {}) =>
  JSON.stringify({ redraw: false, changed: false, meaning: false, selection: [], edit: null, placed: null, cursor: "default", error: null, ...o });
const DEFAULTS: Record<string, (...a: never[]) => unknown> = {
  input: () => OUT(), ops: () => OUT({ changed: true }), undo: () => OUT(), redo: () => OUT(), external: () => OUT(),
  highlighted: () => "[]", board: () => "", frame: () => false, camera: () => '{"x":0,"y":0,"zoom":1}',
  labels: () => "[]", a11y: () => "[]",
};

/** A fake glue: every call is recorded; `over` replaces a method. */
function fake(over: Partial<Record<keyof RawEngine, (...a: never[]) => unknown>> = {}) {
  const calls: [string, unknown[]][] = [];
  const raw = new Proxy({}, {
    get: (_, k: string) => (...args: never[]) => {
      calls.push([k, args]);
      return (over[k as keyof RawEngine] ?? DEFAULTS[k])?.(...args);
    },
  }) as RawEngine;
  const pkg: Pkg = { BoardEngine: function () { return raw; } as unknown as Pkg["BoardEngine"], lastPanic: () => undefined };
  const of = (m: string) => calls.filter((c) => c[0] === m).map((c) => c[1]);
  return { pkg, calls, of };
}

const source = () => ({ board: "{}", context: "{}" });
const settle = async () => {
  for (let i = 0; i < 5; i++) await new Promise((r) => setTimeout(r, 0));
  flushSync();
};
let mounted: ReturnType<typeof mount>[] = [];
function show<P extends Record<string, any>>(C: Component<P>, props: P) {
  const target = document.createElement("div");
  document.body.append(target);
  mounted.push(mount(C, { target, props }));
  flushSync();
  return target;
}
afterEach(() => {
  mounted.forEach((m) => unmount(m));
  mounted = [];
  document.body.innerHTML = "";
  localStorage.clear();
});

const key = (el: Element, k: string, extra: KeyboardEventInit = {}) =>
  el.dispatchEvent(new KeyboardEvent("keydown", { key: k, bubbles: true, cancelable: true, ...extra }));

test("load failure shows the rebuild message and Retry re-imports", async () => {
  const f = fake();
  let n = 0;
  const importer: Importer = () => (++n === 1 ? Promise.reject(new Error("pkg/ig_board_bg.wasm: 404")) : Promise.resolve(f.pkg));
  const el = show(Board, { source, importer });
  await settle();
  expect(el.querySelector("[role=alert]")!.textContent).toContain(
    "The board engine failed to load: pkg/ig_board_bg.wasm: 404. Rebuild the app with `make app`.");
  expect(el.querySelector("canvas")).toBeNull();
  (el.querySelector("[role=alert] button") as HTMLButtonElement).click();
  await settle();
  expect(n).toBe(2);
  expect(el.querySelector("[role=alert]")).toBeNull();
  expect(el.querySelector("canvas")).not.toBeNull();
  expect(f.of("load")).toEqual([["{}", "{}"]]);
});

test("no webgl2 shows the WebGL2 message and keeps the app usable", async () => {
  const f = fake({ attach: () => { throw new Error('getContext("webgl2") returned null'); } });
  const el = show(Board, { source, importer: () => Promise.resolve(f.pkg) });
  await settle();
  expect(el.querySelector("[role=alert]")!.textContent).toContain(
    "This board needs WebGL2, which this system doesn't offer (getContext(\"webgl2\") returned null). Graph, Code and chat still work.");
  expect(el.querySelector("[role=alert] button")).toBeNull(); // nothing to retry
  expect(f.of("free").length).toBe(1);
  // the dev flag forces the same path; nothing throws to the app
  localStorage.setItem("ig.dev.noWebgl2", "1");
  const r = await loadEngine(document.createElement("canvas"), () => Promise.resolve(fake().pkg));
  expect(r).toEqual({ error: expect.stringContaining("turned off by ig.dev.noWebgl2"), retry: false });
});

test("a panic reports the load message and Retry reloads the last saved board", async () => {
  const f = fake({ input: () => { throw new WebAssembly.RuntimeError("unreachable"); } });
  f.pkg.lastPanic = () => "panicked at scene.rs:1";
  let saved = "v1";
  const el = show(Board, { source: () => ({ board: saved, context: "{}" }), importer: () => Promise.resolve(f.pkg) });
  await settle();
  saved = "v2";
  key(el.querySelector("canvas")!, "v");
  await settle();
  expect(el.querySelector("[role=alert]")!.textContent).toContain("The board engine failed to load: panicked at scene.rs:1.");
  (el.querySelector("[role=alert] button") as HTMLButtonElement).click();
  await settle();
  expect(f.of("load").map((a) => a[0])).toEqual(["v1", "v2"]);
});

test("labels layer uses the camera transform", () => {
  const labels: Label[] = [{ id: "fn-a", text: "Export", r: [8, 16, 80, 40], faded: false, title: null }];
  const el = show(Labels, { camera: { x: 10, y: 20, zoom: 2 }, labels });
  const layer = el.querySelector(".labels") as HTMLElement;
  expect(layer.style.transform).toBe("translate(-20px, -40px) scale(2)");
  const l = layer.querySelector(".label") as HTMLElement;
  expect([l.textContent, l.style.left, l.style.top, l.style.width]).toEqual(["Export", "8px", "16px", "80px"]);
  expect(layer.classList.contains("tiny")).toBe(false);
  const small = show(Labels, { camera: { x: 0, y: 0, zoom: 0.4 }, labels });
  expect(small.querySelector(".labels")!.classList.contains("tiny")).toBe(true); // under 6 px on screen
});

/** A board whose Enter asks to rename fn-a. */
async function editing() {
  const f = fake({ input: (ev: string) => (JSON.parse(ev).key === "Enter" ? OUT({ edit: { id: "fn-a", rect: [8, 8, 80, 40], text: "A" } }) : OUT()) });
  const el = show(Board, { source, importer: () => Promise.resolve(f.pkg) });
  await settle();
  key(el.querySelector("canvas")!, "Enter");
  flushSync();
  return { f, el, input: el.querySelector("input.edit") as HTMLInputElement };
}

test("edit overlay commits one Update op on Enter and none on Esc", async () => {
  const a = await editing();
  expect(a.input.style.left).toBe("8px");
  a.input.value = "Export patients";
  key(a.input, "Enter");
  flushSync();
  expect(a.f.of("ops")).toEqual([['[{"op":"update","id":"fn-a","set":{"name":"Export patients"}}]', '"user"']]);
  expect(a.el.querySelector("input.edit")).toBeNull();
  const b = await editing();
  b.input.value = "Nope";
  key(b.input, "Escape");
  flushSync();
  expect(b.f.of("ops")).toEqual([]);
  expect(b.el.querySelector("input.edit")).toBeNull();
});

test("composition blocks Enter until compositionend", () => {
  const names: string[] = [];
  const el = show(EditOverlay, { rect: [0, 0, 80, 40], text: "", oncommit: (n: string) => names.push(n), oncancel: () => {} });
  const input = el.querySelector("input")!;
  input.dispatchEvent(new CompositionEvent("compositionstart", { bubbles: true }));
  input.value = "にほん";
  key(input, "Enter");
  key(input, "Escape");
  expect(names).toEqual([]);
  input.dispatchEvent(new CompositionEvent("compositionend", { bubbles: true }));
  key(input, "Enter", { keyCode: 229 } as KeyboardEventInit); // WebKit's composition-ending Enter
  expect(names).toEqual([]);
  key(input, "Enter");
  expect(names).toEqual(["にほん"]);
});

test("board keys are ignored while the overlay is focused", async () => {
  const { f, input } = await editing();
  const before = f.of("input").length;
  for (const k of ["Delete", "Backspace", "r", "z"]) key(input, k, { metaKey: k === "z" });
  expect(f.of("input").length).toBe(before);
});

test("a11y list mirrors a11y() and arrows call input(Key)", async () => {
  // Up and Down move the engine selection (the engine's own arrows nudge); Enter and Delete
  // go to the engine as keys
  let sel = "";
  const f = fake({
    a11y: () => JSON.stringify([
      { id: "fn-a", kind: "function", text: "Export", selected: sel === "fn-a", highlighted: true, problem: null },
      { id: "sys-pg", kind: "system", text: "PostgreSQL", selected: sel === "sys-pg", highlighted: false, problem: "1 rule problem" },
    ]),
    select: (ids: string) => void (sel = JSON.parse(ids)[0]),
  });
  const el = show(Board, { source, importer: () => Promise.resolve(f.pkg), message: "Claude added 1 element" });
  await settle();
  const tree = el.querySelector("[role=tree]")!;
  expect([...tree.querySelectorAll("[role=treeitem]")].map((i) => i.textContent)).toEqual(
    ["Function Export, changed by Claude", "System PostgreSQL, 1 rule problem"]);
  key(tree, "ArrowDown");
  flushSync();
  key(tree, "ArrowDown");
  flushSync();
  expect(f.of("select")).toEqual([['["fn-a"]'], ['["sys-pg"]']]);
  expect(tree.querySelectorAll("[aria-selected=true]")[0].textContent).toContain("PostgreSQL");
  key(tree, "Enter");
  key(tree, "Delete");
  expect(f.of("input").map((a) => JSON.parse(a[0] as string).key)).toEqual(["Enter", "Delete"]);
  expect(el.querySelector("[aria-live=polite]")!.textContent).toBe("Claude added 1 element");
});

// cwd is viewer/ (happy-dom swaps URL, so no import.meta.url here); `make test` builds the page first
const page = resolve("dist/index.html");
test.skipIf(!existsSync(page))("ig view build has no board code", () => {
  const html = readFileSync(page, "utf8");
  expect(html).not.toContain("__wbindgen");
  expect(html).not.toContain("ig-board");
  expect(html).not.toContain("ig_board");
});
