import type { Column, MapArrow, Table, View } from "../types";
import type { Anchor, Band } from "../shapes/path";

const flat = (a: MapArrow): MapArrow[] => (a.parts?.length ? a.parts.flatMap(flat) : [a]);

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
  for (const a of view.arrows.flatMap(flat)) {
    if (!a.pinned || a.via) continue;
    const to = view.tables?.[a.to];
    if (!to || !view.tables?.[a.from]) continue;
    pointing.set(a.to, (pointing.get(a.to) ?? new Set()).add(a.from));
    of(a.from).pinned.set(to.name.toLowerCase(), to.name);
  }
  for (const [id, from] of pointing) of(id).hub = from.size;
  return out;
}

const HEAD = 56, ROW = 26, FOOT = 8, CHIP = 120;

export function cardLayout(t: Table, zoom: number, h: number, fold = true, marked: (c: Column) => boolean = () => false) {
  const at = distance(zoom);
  const folded = fold ? t.columns.filter((c) => isBookkeeping(c) && !marked(c)) : [];
  const shown = t.columns.filter((c) => !folded.includes(c));
  const keys = shown.filter(isKey);
  const others = shown.length - keys.length;
  const foot = [others && `+${others} ${others === 1 ? "column" : "columns"}`, folded.length && `+${folded.length} bookkeeping`].filter(Boolean).join(" · ");
  const fit = (need: number, cap: number) => Math.max(1, Math.min(cap / (zoom > 0 ? zoom : 1), h / need));
  const m = at === "far" ? Math.min(3, fit(CHIP, 0.55)) : at === "mid" ? fit(HEAD + ROW * (keys.length + (foot ? 1 : 0)) + FOOT, 0.9) : 1;
  return { at, m, folded, shown, keys, foot, rows: at === "far" ? [] : at === "mid" ? keys : shown };
}

export function rowAt(t: Table, column: string | undefined, zoom: number, h: number, fold = true, marked?: (c: Column) => boolean): Band | undefined {
  if (!column) return undefined;
  const l = cardLayout(t, zoom, h, fold, marked), name = column.toLowerCase();
  const i = l.rows.findIndex((c) => c.name.toLowerCase() === name);
  if (i < 0 || l.m * (HEAD + ROW * (i + 1)) > h) return undefined;
  return { y: l.m * (HEAD + ROW * (i + 0.5)), h: l.m * ROW };
}

export function anchorsOf(
  arrows: MapArrow[], frames: Record<string, { h: number }>, tables: Record<string, Table> | undefined, zoom: number, fold = true,
  marked?: (id: string, c: Column) => boolean,
): Record<string, Anchor> {
  const out: Record<string, Anchor> = {};
  if (!tables) return out;
  for (const a of arrows) {
    if (a.parts?.length || !frames[a.from] || !frames[a.to]) continue;
    const [ft, tt] = [tables[a.from], tables[a.to]];
    if (!ft || !tt || ft.kind === "view") continue;
    const of = (id: string) => (marked ? (c: Column) => marked(id, c) : undefined);
    const from = rowAt(ft, a.fromColumn, zoom, frames[a.from].h, fold, of(a.from)), to = rowAt(tt, a.toColumn, zoom, frames[a.to].h, fold, of(a.to));
    if (from || to) out[a.id] = { from, to };
  }
  return out;
}
