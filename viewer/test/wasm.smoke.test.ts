/// <reference types="node" />
// The real engine (viewer/src/board/pkg, built by `make board-wasm`; skipped without it), run
// without a canvas: load, ops, then board() is ig_design's canonical text.
import { existsSync, readFileSync } from "node:fs";
import { expect, test } from "vitest";
import { wrap, type Pkg } from "../src/board/host";

const dir = new URL("../src/board/pkg/", import.meta.url);
const built = existsSync(new URL("ig_board_bg.wasm", dir));

test.skipIf(!built)("load, ops (add, link), then board() round-trips to canonical JSON", async () => {
  const glue = (await import(/* @vite-ignore */ new URL("ig_board.js", dir).href)) as Pkg & {
    default(i: { module_or_path: BufferSource }): Promise<unknown>;
  };
  await glue.default({ module_or_path: readFileSync(new URL("ig_board_bg.wasm", dir)) });
  const e = wrap(new glue.BoardEngine(), glue, (r) => { throw new Error(r); });
  const empty = '{"version":1,"title":"Smoke","scope":{"kind":"dir","id":"src"}}';
  e.load(empty, JSON.stringify({ lanes: [], boxes: [], edges: [] }));
  const add = (id: string, kind: string, x: number) => ({ op: "add", id, kind, name: id, file: null, method: null, path: null,
    layer: null, on: null, x, y: 8, w: null, h: null });
  const o = e.ops(JSON.stringify([add("system-pg", "system", 200), add("function-a", "function", 8),
    { op: "link", from: "function-a", to: "system-pg", kind: "uses" }]), "user");
  expect([o.error, o.changed, o.meaning]).toEqual([null, true, true]);
  const text = e.board();
  expect(text).toBe(readFileSync(new URL("fixtures/smoke-design.json", import.meta.url), "utf8"));
  e.load(text, JSON.stringify({ lanes: [], boxes: [], edges: [] }));
  expect(e.board()).toBe(text);
  expect(e.ops("[{\"op\":\"remove\"}]", "user").error).toMatch(/missing field/);
  e.saved(text);
  expect(e.undo().changed).toBe(false); // a fresh load has no history
  e.free();
});

test.skipIf(!built)("the board over a real context: place in a row, file proposed, link, check reason, scope switch keeps undo", async () => {
  const { buildContext } = await import("../src/board/context");
  const { BoardStore } = await import("../src/board/store");
  const { index } = await import("../src/model/graph");
  const fixture = (await import("./fixture-data.json")).default as unknown as import("../src/data").ViewData;
  const glue = (await import(/* @vite-ignore */ new URL("ig_board.js", dir).href)) as Pkg & {
    default(i: { module_or_path: BufferSource }): Promise<unknown>;
  };
  await glue.default({ module_or_path: readFileSync(new URL("ig_board_bg.wasm", dir)) });
  const e = wrap(new glue.BoardEngine(), glue, (r) => { throw new Error(r); });
  const ix = index(fixture);
  const whole = buildContext(ix, { kind: "dir", id: "." }, 1200);
  const lane = (JSON.parse(whole).lanes as { name: string; kind: string; top: number; bottom: number }[]).find((l) => l.kind === "layer")!;
  const design = { version: 1, title: "F", scope: { kind: "dir", id: "." }, elements: [], marks: [], links: [], bridges: [] };
  const writes: unknown[] = [];
  const api = { read: async () => design, write: async (_: string, d: unknown) => void writes.push(d),
    check: () => new Promise(() => {}), features: async () => [], create: async () => { throw 0; }, onChanged: () => () => {} };
  const store = new BoardStore(api as never, e, "f", { now: () => 0, after: () => () => {} },
    { context: () => whole, nodes: () => fixture.nodes });
  await store.load();
  e.resize(1200, 800, 1);
  e.frame(); // no canvas: draws nothing, but the view is set
  // place a function in the first layer row (camera at the origin: screen = world)
  e.input({ t: "tool", tool: "function" });
  const y = (lane.top + lane.bottom) / 2;
  const o = e.input({ t: "down", x: 1300, y, button: 0, mods: 0 });
  e.input({ t: "up", x: 1300, y, mods: 0 });
  expect(o.placed?.layer).toBe(lane.name);
  store.onOut(o);
  const id = o.placed!.id;
  const el = () => JSON.parse(e.board()).elements.find((x: { id: string }) => x.id === id);
  expect(el().file).toMatch(new RegExp(`/function\\.[a-z]+$`));
  expect(e.hit(1300, y)).toBe(id);
  // placing and the proposed file are one undo step
  const placed = e.board();
  e.undo();
  expect(JSON.parse(e.board()).elements).toEqual([]);
  e.redo();
  expect(e.board()).toBe(placed);
  // a link to a system and its check reason
  const sys = { op: "add", id: "sys-pg", kind: "system", name: "PostgreSQL", file: null, method: null, path: null, layer: null, on: null, x: 1600, y: 8, w: null, h: null };
  store.onOut(e.ops(JSON.stringify([sys, { op: "link", from: id, to: "sys-pg", kind: "uses" }]), "user"));
  const key = `${id}|uses|sys-pg`;
  e.setCheck(JSON.stringify([[key, "error", "core must not use PostgreSQL"]]));
  expect(e.a11y().find((i) => i.id === key)?.problem).toBe("core must not use PostgreSQL");
  // another scope: the board and its history stay
  const before = e.board();
  e.setContext(buildContext(ix, { kind: "layer", name: lane.name }, 1200));
  expect(e.board()).toBe(before);
  expect(e.undo().changed).toBe(true);
  expect(JSON.parse(e.board()).links).toEqual([]);
  // Claude's turn comes in from the file: highlighted, touched, and reverted in one step
  const disk = JSON.parse(e.board());
  disk.elements.push({ id: "fn-audit", kind: "function", name: "audit", layer: lane.name });
  const a = e.external(JSON.stringify(disk), "turn-1");
  expect(a.touched).toEqual(["fn-audit"]);
  expect(e.highlighted()).toEqual(["fn-audit"]);
  expect(e.revert("turn-1").changed).toBe(true);
  expect(JSON.parse(e.board()).elements.map((x: { id: string }) => x.id)).not.toContain("fn-audit");
  e.free();
});

test.skipIf(!built)("screens across the boundary: load, a piece, rename, a state, a merge from disk, saved", async () => {
  const glue = (await import(/* @vite-ignore */ new URL("ig_board.js", dir).href)) as Pkg & {
    default(i: { module_or_path: BufferSource }): Promise<unknown>;
  };
  await glue.default({ module_or_path: readFileSync(new URL("ig_board_bg.wasm", dir)) });
  const e = wrap(new glue.BoardEngine(), glue, (r) => { throw new Error(r); });
  const design = { version: 1, title: "S", scope: { kind: "dir", id: "." } };
  const login = { version: 1, id: "login", title: "Login", x: 0, y: 0, w: 400, h: 300, elements: [], navs: [] };
  e.load(JSON.stringify({ design, screens: [login] }), JSON.stringify({ lanes: [], boxes: [], edges: [] }));
  expect(e.surface("screens")).toBe("screens");
  e.input({ t: "key", key: "b", mods: 0 });
  expect(e.tool()).toBe("piece:button");
  const o = e.input({ t: "down", x: 100, y: 100, button: 0, mods: 0 });
  expect([o.error, o.changed, o.edit?.id]).toEqual([null, true, "login/btn-button"]);
  expect(e.rename("login/btn-button", "Sign in").changed).toBe(true);
  expect(JSON.parse(e.screens().login).elements[0].label).toBe("Sign in");
  expect(e.variant("login", "error").changed).toBe(true);
  expect(Object.keys(e.screens())).toEqual(["login", "login-error"]);
  // Claude retitles login on disk (from the merge base: no screens saved yet, so the base has login only)
  e.saved(JSON.stringify({ screens: { login: JSON.parse(e.screens().login) } }));
  const disk = { ...JSON.parse(e.screens().login), title: "Sign in page" };
  const m = e.external(JSON.stringify({ screens: { login: disk } }), "t1");
  expect(m.touched).toEqual(["login"]);
  expect(JSON.parse(e.screens().login).title).toBe("Sign in page");
  expect(e.highlighted()).toEqual(["login"]);
  expect(e.external(JSON.stringify({ screens: { login: { ...disk, version: 2 } } }), null).error).toMatch(/version 2/);
  e.free();
});
