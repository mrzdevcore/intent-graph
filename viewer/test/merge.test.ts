import { describe, expect, it } from "vitest";
import type { Design } from "../src/api";
import type { Board } from "../src/model/board";
import { mergeBoard, placeNew } from "../src/model/merge";

const el = (id: string, name = id, extra = {}) => ({ id, kind: "function" as const, name, ...extra });
const board = (p: Partial<Design>): Board => ({
  design: { version: 1, title: "t", scope: { kind: "layer", name: "x" }, elements: [], marks: [], links: [], bridges: [], ...p },
  screens: {},
});
const ids = (b: Board) => b.design.elements.map((e) => e.id).sort();

describe("mergeBoard", () => {
  const base = board({ elements: [el("fn-a"), el("fn-b")] });
  it("user and agent edits on different elements both survive", () => {
    const ours = board({ elements: [el("fn-a", "A"), el("fn-b")] });
    const theirs = board({ elements: [el("fn-a"), el("fn-b", "B")] });
    const r = mergeBoard(base, ours, theirs);
    expect(r.board.design.elements.map((e) => e.name).sort()).toEqual(["A", "B"]);
    expect(r.changed).toEqual(["fn-b"]);
  });
  it("the user wins on the same element", () => {
    const r = mergeBoard(base, board({ elements: [el("fn-a", "U"), el("fn-b")] }), board({ elements: [el("fn-a", "T"), el("fn-b")] }));
    expect(r.board.design.elements[0].name).toBe("U");
    expect(r.changed).toEqual([]);
  });
  it("an agent removal of an element the user edited is dropped", () => {
    const r = mergeBoard(base, board({ elements: [el("fn-a", "U"), el("fn-b")] }), board({ elements: [el("fn-b")] }));
    expect(ids(r.board)).toEqual(["fn-a", "fn-b"]);
    const r2 = mergeBoard(base, base, board({ elements: [el("fn-b")] }));
    expect(ids(r2.board)).toEqual(["fn-b"]);
    expect(r2.removed).toEqual(["fn-a"]);
  });
  it("links and bridges to a removed element are dropped", () => {
    const b = board({ elements: [el("fn-a"), el("fn-b")], links: [{ from: "fn-a", to: "fn-b", kind: "calls" }], bridges: [{ from: "s/btn", to: "fn-b" }] });
    const t = board({ elements: [el("fn-a")], links: b.design.links, bridges: b.design.bridges });
    const r = mergeBoard(b, b, t);
    expect(r.board.design.links).toEqual([]);
    expect(r.board.design.bridges).toEqual([]);
  });
  it("agent additions arrive, marks merge by node", () => {
    const t = board({ elements: [el("fn-a"), el("fn-b"), el("fn-c")], marks: [{ node: "x.ts", mark: "changed" }] });
    const r = mergeBoard(base, base, t);
    expect(ids(r.board)).toEqual(["fn-a", "fn-b", "fn-c"]);
    expect(r.board.design.marks).toHaveLength(1);
    expect(r.changed.sort()).toEqual(["fn-c", "x.ts"]);
  });
});

describe("placeNew", () => {
  it('puts an "api" element into the api row', () => {
    const lanes = [{ row: 0, name: "ui", kind: "layer" as const, top: 0, bottom: 100 }, { row: 1, name: "api", kind: "layer" as const, top: 150, bottom: 250 }];
    const b = placeNew(board({ elements: [el("fn-a", "a", { layer: "api" })] }), ["fn-a"], lanes);
    expect(b.design.elements[0].y).toBe(208);
    expect(b.design.elements[0].x).toBe(80);
  });
});
