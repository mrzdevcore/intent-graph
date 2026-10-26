// @vitest-environment happy-dom
import { flushSync, mount, unmount } from "svelte";
import { afterEach, expect, test } from "vitest";
import type { Design, DesignApi } from "../src/api";
import type { ViewData } from "../src/data";
import ArchBoard from "../src/board/ArchBoard.svelte";
import type { Pkg, RawEngine } from "../src/board/host";
import { index } from "../src/model/graph";
import fixture from "./fixture-data.json";

const OUT = (o: object = {}) =>
  JSON.stringify({ redraw: false, changed: false, meaning: false, selection: [], edit: null, placed: null, cursor: "default", error: null, ...o });

const design = (scope: Design["scope"]): Design =>
  ({ version: 1, title: "F", scope, elements: [{ id: "fn-a", kind: "function", name: "A", x: 0, y: 0 }], marks: [], links: [], bridges: [] });

/** A fake engine whose board's scope steps back on undo; every call is recorded. */
function fakeEngine() {
  const calls: [string, unknown[]][] = [];
  const boards = [design({ kind: "dir", id: "." }), design({ kind: "layer", name: "core" })];
  let at = 0;
  let freed = false;
  const over: Record<string, (...a: never[]) => unknown> = {
    load: (b: string) => void (boards[at = 0] = JSON.parse(b)),
    ops: (o: string) => {
      const op = JSON.parse(o)[0];
      if (op.op === "scope") boards[(at = 1)] = { ...boards[0], scope: op.scope };
      return OUT({ changed: true, meaning: true });
    },
    input: (ev: string) => {
      const e = JSON.parse(ev);
      if (e.t === "key" && e.key === "z") at = e.mods & 1 ? 1 : 0;
      return OUT({ changed: e.t === "key" && e.key === "z", meaning: true });
    },
    board: () => {
      if (freed) throw new Error("the engine was freed");
      return JSON.stringify(boards[at]);
    },
    free: () => void (freed = true),
    tool: () => "select", highlighted: () => "[]", frame: () => false, camera: () => '{"x":0,"y":0,"zoom":1}',
    labels: () => "[]", a11y: () => "[]", undo: () => OUT(), redo: () => OUT(), external: () => OUT(), hit: () => undefined,
  };
  const raw = new Proxy({}, {
    get: (_, k: string) => (...args: never[]) => {
      calls.push([k, args]);
      return over[k]?.(...args);
    },
  }) as RawEngine;
  const pkg: Pkg = { BoardEngine: function () { return raw; } as unknown as Pkg["BoardEngine"], lastPanic: () => undefined };
  const of = (m: string) => calls.filter((c) => c[0] === m).map((c) => c[1]);
  return { pkg, of };
}

function fakeApi() {
  const writes: Design[] = [];
  const api: DesignApi = {
    features: async () => [{ slug: "f", title: "F", design: true, plan: null, done: false, flat: false }],
    create: async () => { throw new Error("unused"); },
    read: async () => design({ kind: "dir", id: "." }),
    write: async (_s, d) => void writes.push(d),
    check: () => new Promise(() => {}),
    keepUnsaved: async () => {},
    unsaved: async () => null,
    discardUnsaved: async () => {},
    planStatus: async () => ({ exists: false, blocking: 0 }),
    onChanged: () => () => {},
  };
  return { api, writes, setWrite: (f: typeof api.write) => (api.write = f) };
}

const settle = async () => {
  for (let i = 0; i < 10; i++) await new Promise((r) => setTimeout(r, 0));
  flushSync();
};

let mounted: ReturnType<typeof mount> | null = null;
afterEach(() => {
  if (mounted) unmount(mounted);
  mounted = null;
  document.body.innerHTML = "";
});

function show(f: ReturnType<typeof fakeEngine>, api: DesignApi, beforeClose?: (cb: () => Promise<void>) => () => void) {
  const target = document.createElement("div");
  document.body.append(target);
  const ix = index(fixture as ViewData);
  mounted = mount(ArchBoard, { target, props: { ix, api, graphScope: ix.root, beforeClose, importer: () => Promise.resolve(f.pkg) } });
  return target;
}

const lanes = (ctx: unknown) => (JSON.parse(ctx as string).boxes as { id: string }[]).map((b) => b.id).sort();

test("the context follows the scope through the picker, undo and redo", async () => {
  const f = fakeEngine();
  const el = show(f, fakeApi().api);
  await settle();
  const ctxs = () => f.of("setContext").map((c) => c[0]);
  const select = el.querySelector("label.scope select") as HTMLSelectElement;
  const layer = [...select.options].find((o) => o.textContent === "Layer core")!;
  select.value = layer.value;
  select.dispatchEvent(new Event("change", { bubbles: true }));
  await settle();
  const core = lanes(ctxs().at(-1));
  const before = ctxs().length;
  (el.querySelector("button[aria-label=Undo]") as HTMLButtonElement).click();
  await settle();
  expect(ctxs().length).toBe(before + 1);
  expect(lanes(ctxs().at(-1))).not.toEqual(core);
  (el.querySelector("button[aria-label=Redo]") as HTMLButtonElement).click();
  await settle();
  expect(lanes(ctxs().at(-1))).toEqual(core);
});

test("closing the view writes the pending edit before the engine is freed", async () => {
  const f = fakeEngine();
  const a = fakeApi();
  show(f, a.api);
  await settle();
  // a change: the 300 ms autosave has not run yet when the view closes
  (document.querySelector("button[aria-label=Undo]") as HTMLButtonElement).click();
  await settle();
  expect(a.writes).toHaveLength(0);
  unmount(mounted!);
  mounted = null;
  await settle();
  expect(a.writes).toEqual([design({ kind: "dir", id: "." })]);
  expect(f.of("free")).toHaveLength(1);
});

test("a quit right after closing the view waits for the close's write", async () => {
  const f = fakeEngine();
  const a = fakeApi();
  const flushers = new Set<() => Promise<void>>();
  show(f, a.api, (cb) => (flushers.add(cb), () => void flushers.delete(cb)));
  await settle();
  (document.querySelector("button[aria-label=Undo]") as HTMLButtonElement).click();
  await settle();
  let release!: () => void;
  a.setWrite(() => new Promise<void>((r) => (release = r)));
  unmount(mounted!);
  mounted = null;
  // the quit: every registered flush
  let quit = false;
  const quitting = Promise.all([...flushers].map((cb) => cb())).then(() => (quit = true));
  await settle();
  expect(quit).toBe(false);
  release();
  await quitting;
  expect(quit).toBe(true);
  expect(flushers.size).toBe(0); // the close's own registration is gone once written
});
