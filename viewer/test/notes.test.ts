import { expect, test, vi } from "vitest";
import type { Note } from "../src/data";
import type { Lane } from "../src/model/architecture";
import { client, dropNote, ghosts, noteLine, pins } from "../src/model/notes";

const lanes: Lane[] = [
  { row: 0, name: "core", kind: "layer", top: 0, bottom: 120 },
  { row: 1, name: "ui", kind: "layer", top: 150, bottom: 270 },
  { row: 2, name: "Other", kind: "other", top: 300, bottom: 420 },
];

test("dropping box X on layer row L gives a move note; same row, no row or Other gives none", () => {
  expect(dropNote(lanes, "packages/ui/src/x.ts", 200, "core")).toEqual({ kind: "move", target: "packages/ui/src/x.ts", layer: "ui" });
  expect(dropNote(lanes, "packages/ui/src/x.ts", 60, "core")).toBeNull();
  expect(dropNote(lanes, "packages/ui/src/x.ts", 135, "core")).toBeNull();
  expect(dropNote(lanes, "packages/ui/src/x.ts", 350, "core")).toBeNull();
});

const note = (id: string, o: Partial<Note>): Note => ({ id, done: false, reply: null, kind: "comment", target: "a.ts", ...o });

test("pins number the open notes on their targets, both ends of an edge; ghosts are the open moves", () => {
  const ns = [
    note("N1", { text: "hm" }),
    note("N2", { kind: "link", target: "a.ts", to: "b.ts", text: "calls" }),
    note("N3", { kind: "move", target: "c.ts", layer: "ui" }),
    note("N4", { kind: "move", target: "d.ts", layer: "ui", done: true }),
  ];
  expect(pins(ns)).toEqual({ "a.ts": ["N1", "N2"], "b.ts": ["N2"], "c.ts": ["N3"] });
  expect(ghosts(ns)).toEqual(new Set(["c.ts"]));
  expect(noteLine(ns[1])).toBe("link a.ts → b.ts (calls)");
  expect(noteLine(note("N5", { kind: "add", target: "cache", text: "module", layer: "core" }))).toBe("add module cache in core");
});

test("the client sends the token on posts and turns errors into exceptions", async () => {
  const fetch = vi.fn(async (_url: string, init?: RequestInit) =>
    init?.method === "POST" && JSON.parse(String(init.body)).target === "bad"
      ? new Response(JSON.stringify({ error: "unknown node bad" }), { status: 400 })
      : new Response(JSON.stringify({ notes: [] }), { status: 200 }));
  const c = client("tok", fetch as unknown as typeof globalThis.fetch);
  await c.add({ kind: "remove", target: "a.ts" });
  const [url, init] = fetch.mock.calls[0];
  expect(url).toBe("/api/notes");
  expect((init!.headers as Record<string, string>)["X-IG-Token"]).toBe("tok");
  await expect(c.add({ kind: "remove", target: "bad" })).rejects.toThrow("unknown node bad");
  expect(await c.notes()).toEqual([]);
});
