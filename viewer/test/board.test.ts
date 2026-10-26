import { describe, expect, it } from "vitest";
import type { Design } from "../src/api";
import { apply, boardDelta, geometryOnly, History, meaning, type Board } from "../src/model/board";
import type { Lane } from "../src/model/architecture";

const board = (p: Partial<Design> = {}): Board => ({
  design: { version: 1, title: "t", scope: { kind: "layer", name: "x" }, elements: [], marks: [], links: [], bridges: [], ...p },
  screens: {},
});
const lanes: Lane[] = [
  { row: 0, name: "api", kind: "layer", top: 0, bottom: 100 },
  { row: 1, name: "Other", kind: "other", top: 120, bottom: 200 },
  { row: 2, name: "External services", kind: "external", top: 220, bottom: 300 },
];
const fn = (id: string, extra = {}) => ({ id, kind: "function" as const, name: id.slice(3), x: 8, y: 8, w: 80, h: 40, ...extra });

describe("apply", () => {
  it("rename keeps the id", () => {
    const b = board({ elements: [fn("fn-a")] });
    const r = apply(b, { t: "rename", id: "fn-a", name: "b" });
    expect(r.board.design.elements[0]).toMatchObject({ id: "fn-a", name: "b" });
    expect(b.design.elements[0].name).toBe("a");
  });
  it("move snaps to 8 and takes the lane's layer", () => {
    const b = board({ elements: [fn("fn-a", { layer: "x", h: 0 })] });
    let e = apply(b, { t: "move", ids: ["fn-a"], dx: 13, dy: 20, lanes }).board.design.elements[0];
    expect([e.x, e.y, e.layer]).toEqual([24, 32, "api"]);
    e = apply(b, { t: "move", ids: ["fn-a"], dx: 0, dy: 150, lanes }).board.design.elements[0];
    expect(e.layer).toBeUndefined();
  });
  it("refuses a non-system in External services", () => {
    const b = board({ elements: [fn("fn-a", { h: 0 }), { id: "sys-db", kind: "system" as const, name: "db", x: 8, y: 8, h: 0 }] });
    const r = apply(b, { t: "move", ids: ["fn-a", "sys-db"], dx: 0, dy: 240, lanes }).board.design.elements;
    expect(r[0].y).toBe(8);
    expect(r[1].y).toBe(248);
  });
  it("delete cascades links, note targets, frames and bridges", () => {
    const b = board({
      elements: [fn("fn-a"), fn("fn-b"), { id: "note-1", kind: "note", name: "n", on: "fn-a" }, fn("fn-c", { frame: "fn-a" })],
      links: [{ from: "fn-a", to: "fn-b", kind: "calls" }, { from: "fn-b", to: "x.ts#f", kind: "calls" }],
      bridges: [{ from: "s/btn", to: "fn-a" }, { from: "s/btn", to: "fn-b" }],
    });
    const d = apply(b, { t: "delete", ids: ["fn-a"] }).board.design;
    expect(d.elements.map((e) => e.id)).toEqual(["fn-b", "note-1", "fn-c"]);
    expect(d.elements[1].on).toBeUndefined();
    expect(d.elements[2].frame).toBeUndefined();
    expect(d.links).toHaveLength(1);
    expect(d.bridges).toEqual([{ from: "s/btn", to: "fn-b" }]);
  });
  it("paste gives fresh ids and rewrites internal link ends", () => {
    const b = board({ elements: [fn("fn-a"), fn("fn-b")] });
    const r = apply(b, { t: "paste", els: [fn("fn-a"), fn("fn-b")], links: [{ from: "fn-a", to: "fn-b", kind: "calls" }, { from: "fn-a", to: "q.ts#z", kind: "calls" }], dx: 16, dy: 16 });
    expect(r.selected).toEqual(["fn-a-2", "fn-b-2"]);
    expect(r.board.design.links.map((l) => `${l.from}>${l.to}`)).toEqual(["fn-a-2>fn-b-2", "fn-a-2>q.ts#z"]);
    expect(r.board.design.elements[2]).toMatchObject({ x: 24, y: 24 });
  });
});

describe("meaning and delta", () => {
  it("meaning ignores geometry", () => {
    const b = board({ elements: [fn("fn-a")] });
    const m = apply(b, { t: "move", ids: ["fn-a"], dx: 40, dy: 0, lanes: [] }).board;
    expect(meaning(m)).toBe(meaning(b));
    expect(geometryOnly(b, m)).toBe(true);
    expect(boardDelta(b, m)).toEqual([]);
  });
  it("boardDelta lines", () => {
    const prev = board({ elements: [fn("fn-a", { name: "a" })] });
    let n = apply(prev, { t: "add", el: { kind: "route", name: "GET /x", method: "GET", path: "/x", file: "server/api/x.get.ts" } }).board;
    n = apply(n, { t: "rename", id: "fn-a", name: "b" }).board;
    n = apply(n, { t: "link", link: { from: "fn-a", to: "server/r.ts#f", kind: "calls" } }).board;
    expect(boardDelta(prev, n)).toEqual([
      "added route route-get-x (GET /x, server/api/x.get.ts)",
      "renamed fn-a: a → b",
      "linked fn-a calls server/r.ts#f",
    ]);
  });
});

describe("History", () => {
  it("undo, redo, truncation", () => {
    const h = new History<number>();
    h.reset(0); [1, 2, 3].forEach((x) => h.push(x));
    expect(h.undo()).toBe(2);
    h.push(9);
    expect(h.redo()).toBeNull();
    expect(h.undo()).toBe(2);
  });
  it("caps at 200 steps", () => {
    const h = new History<number>(); h.reset(0);
    for (let i = 1; i <= 250; i++) h.push(i);
    let n = 0; while (h.undo() !== null) n++;
    expect(n).toBe(200);
  });
  it("push plus 2 amends undo in one step", () => {
    const h = new History<number>(); h.reset(0);
    h.push(1); h.amend(2); h.amend(3);
    expect(h.undo()).toBe(0);
    expect(h.redo()).toBe(3);
  });
});
