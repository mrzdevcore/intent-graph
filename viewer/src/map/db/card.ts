import type { Column, MapArrow, Table } from "../types";
import type { Anchor, Band } from "../shapes/path";

export type Distance = "far" | "mid" | "near";
export type CardFacts = { hub: number; pinned: Map<string, string> };

export const BOOKKEEPING = ["deleted", "created_at", "updated_at", "deleted_at"];
export const isBookkeeping = (c: Column) => BOOKKEEPING.includes(c.name.toLowerCase());
export const isKey = (c: Column) => !!c.pk || !!c.references;

// a store of tables at fit is under 0.45; a domain fitted to the screen, under 1.4
export const distance = (zoom: number): Distance => (zoom < 0.45 ? "far" : zoom < 1.4 ? "mid" : "near");

// Rust lays a table box out keys-only at these sizes (TABLE and card_rows in crates/ig-app)
export const CARD = { w: 320, head: 56, row: 26, foot: 8 };
const { head: HEAD, row: ROW, foot: FOOT } = CARD, CHIP = 120;

export const keysOnly = (t: Table) => {
  const keys = t.columns.filter(isKey).length;
  return keys + (t.columns.length > keys ? 1 : 0) + (t.translations?.length ? 1 : 0);
};
export const cardSize = (t: Table) => ({ w: CARD.w, h: HEAD + ROW * Math.max(keysOnly(t), 2) + FOOT });

export function cardLayout(t: Table, zoom: number, h: number, fold = true, marked: (c: Column) => boolean = () => false) {
  const folded = fold ? t.columns.filter((c) => isBookkeeping(c) && !isKey(c) && !marked(c)) : [];
  const shown = t.columns.filter((c) => !folded.includes(c));
  const keys = shown.filter(isKey);
  const others = shown.length - keys.length;
  const foot = [others && `+${others} ${others === 1 ? "column" : "columns"}`, folded.length && `+${folded.length} bookkeeping`].filter(Boolean).join(" · ");
  const strip = t.translations?.length ? 1 : 0;
  const near = HEAD + ROW * (shown.length + (folded.length ? 1 : 0) + strip) + FOOT;
  const shrink = Math.min(1, h / near);
  const at = distance(zoom) === "near" && zoom * shrink < 1 ? "mid" : distance(zoom);
  const fit = (need: number, cap: number) => Math.max(1, Math.min(cap / (zoom > 0 ? zoom : 1), h / need));
  const m = at === "far" ? Math.min(3, fit(CHIP, 0.55)) : at === "mid" ? fit(HEAD + ROW * (keys.length + (foot ? 1 : 0) + strip) + FOOT, 0.9) : shrink;
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
