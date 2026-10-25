// Plan notes in the viewer: the move a drop makes, the pins and ghosts drawn on the graph, and the
// client for `ig serve`'s notes API (every POST carries the run's token).

import type { Note, NoteInput } from "../data";
import { rowAt, type Lane } from "./architecture";

/** The move note for dropping box `id` (now in layer `from`) at height `y`; null unless it lands in another layer's row. */
export function dropNote(lanes: Lane[], id: string, y: number, from: string | null | undefined): NoteInput | null {
  const layer = rowAt(lanes, y);
  return layer && layer !== from ? { kind: "move", target: id, layer } : null;
}

/** Open note ids by the node they are on (both ends of an edge). */
export function pins(notes: Note[]): Record<string, string[]> {
  const out: Record<string, string[]> = {};
  notes.filter((n) => !n.done && n.kind !== "add").forEach((n) => {
    [n.target, n.to].forEach((id) => id && (out[id] = out[id] || []).push(n.id));
  });
  return out;
}

/** Boxes with an open move note: drawn ghosted until the agent applies it. */
export function ghosts(notes: Note[]): Set<string> {
  return new Set(notes.filter((n) => !n.done && n.kind === "move").map((n) => n.target));
}

/** A note as one line, like its line in the notes file. */
export function noteLine(n: NoteInput): string {
  const t = n.to ? n.target + " → " + n.to : n.target;
  switch (n.kind) {
    case "comment": return "comment on " + t + ": " + n.text;
    case "move": return "move " + t + " to layer " + n.layer;
    case "link": return "link " + t + " (" + n.text + ")";
    case "remove": return "remove " + t;
    case "add": return "add " + n.text + " " + n.target + " in " + n.layer;
  }
}

export function client(token: string, f: typeof fetch = fetch.bind(globalThis)) {
  const call = async (path: string, body?: unknown) => {
    const r = await f(path, body === undefined ? undefined
      : { method: "POST", headers: { "Content-Type": "application/json", "X-IG-Token": token }, body: JSON.stringify(body) });
    const v = await r.json();
    if (!r.ok) throw new Error(v.error || "ig serve: " + r.status);
    return v;
  };
  return {
    notes: async (): Promise<Note[]> => (await call("/api/notes")).notes,
    add: (n: NoteInput): Promise<Note> => call("/api/notes", n),
  };
}
export type Client = ReturnType<typeof client>;
