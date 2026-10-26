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
