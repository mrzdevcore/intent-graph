import type { Column, View } from "../types";

export type Distance = "far" | "mid" | "near";
export type CardFacts = { hub: number; pinned: Map<string, string> };

export const BOOKKEEPING = ["deleted", "created_at", "updated_at", "deleted_at"];
export const isBookkeeping = (c: Column) => BOOKKEEPING.includes(c.name.toLowerCase());
export const isKey = (c: Column) => !!c.pk || !!c.references;

// a store of tables at fit is under 0.45; a domain fitted to the screen, under 1.4
export const distance = (zoom: number): Distance => (zoom < 0.45 ? "far" : zoom < 1.4 ? "mid" : "near");

/** By table box id: how many tables point at it when it is a hub, and its pinned foreign keys' targets by referenced table name. */
export function cardFacts(view: View): Map<string, CardFacts> {
  const out = new Map<string, CardFacts>();
  const of = (id: string) => out.get(id) ?? out.set(id, { hub: 0, pinned: new Map() }).get(id)!;
  const pointing = new Map<string, Set<string>>();
  for (const a of view.arrows) {
    if (!a.pinned || a.via) continue;
    const to = view.tables?.[a.to];
    if (!to || !view.tables?.[a.from]) continue;
    pointing.set(a.to, (pointing.get(a.to) ?? new Set()).add(a.from));
    of(a.from).pinned.set(to.name.toLowerCase(), to.name);
  }
  for (const [id, from] of pointing) of(id).hub = from.size;
  return out;
}
