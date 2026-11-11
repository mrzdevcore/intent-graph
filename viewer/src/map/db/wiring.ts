import type { MapArrow, Table, View } from "../types";
import type { Wire } from "../wires";
import type { TableMark } from "../diff";
import type { Anchor } from "../shapes/path";
import { isInside, type Place } from "../place/places";
import { anchorsOf } from "./card";

type Tables = Record<string, Table> | undefined;
type Homes = Map<string, string | undefined>;

export const leaves = (a: MapArrow): MapArrow[] => (a.parts?.length ? a.parts.flatMap(leaves) : [a]);
export const isRead = (a: MapArrow, tables: Tables) => a.kind === "reads" && tables?.[a.from]?.kind === "view";
export const readsOnly = (a: MapArrow, tables: Tables) => leaves(a).every((l) => isRead(l, tables));
export const isTableLink = (a: MapArrow, tables: Tables) => !a.parts?.length && !!tables?.[a.from] && !!tables?.[a.to];

export const readShown = (a: MapArrow, tables: Tables, picked: (id: string) => boolean) => {
  const reads = leaves(a).filter((l) => isRead(l, tables));
  return !reads.length || reads.some((l) => picked(l.from));
};

export const tipsOf = (a: MapArrow, w: Wire, tables: Tables) =>
  isTableLink(a, tables) && !readsOnly(a, tables) && a.ends
    ? [{ end: a.ends.from, at: w.ends[0], out: w.out[0] }, { end: a.ends.to, at: w.ends[1], out: w.out[1] }]
    : [];

export const anchorsAt = (
  as: MapArrow[], frames: Record<string, { h: number }>, tables: Tables, zoom: number, fold: boolean, marks: Map<string, TableMark> | null,
): Record<string, Anchor> =>
  anchorsOf(as, frames, tables, zoom, fold, marks ? (id, c) => !!marks.get(id)?.columns.get(c.name.toLowerCase()) : undefined);

const shifted = (v: View, homes: Homes) => v.boxes.filter((b) => homes.get(b.id) !== b.parent).map((b) => b.id);
const bare = (k: string) => (isInside(k) ? k.slice(0, -1) : k);

// the kept places of boxes a noise switch hides or moves: they come back when it goes off
export function aside(kept: Record<string, Place>, v: View, homes: Homes): Record<string, Place> {
  const drawn = new Set(v.boxes.map((b) => b.id)), moved = new Set(shifted(v, homes));
  return Object.fromEntries(Object.entries(kept).filter(([k]) => homes.has(bare(k)) && (!drawn.has(bare(k)) || moved.has(bare(k)))));
}

export function unshifted(places: Record<string, Place>, v: View, homes: Homes): Record<string, Place> {
  const out = { ...places };
  for (const id of shifted(v, homes)) delete out[id];
  return out;
}

export function backHome(prev: Record<string, Place>, moved: string[], v: View, homes: Homes, kept: (id: string) => Place | undefined) {
  const parent = new Map(v.boxes.map((b) => [b.id, b.parent]));
  for (const id of moved) {
    const p = homes.has(id) && homes.get(id) === parent.get(id) ? kept(id) : undefined;
    if (p) prev[id] = p;
    else delete prev[id];
  }
}
