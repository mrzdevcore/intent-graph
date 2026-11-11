import type { Column, End, MapArrow, MapBox, Table, TableGroup, View } from "../types";
import { isBookkeeping, isKey } from "./card";

export type Hops = 1 | 2;
export type Folds = { translations: boolean; joins: boolean; bookkeeping: boolean };
export const FOLDS: Folds = { translations: true, joins: true, bookkeeping: true };

export type Rel = { id: string; from: string; to: string; fromColumn?: string; toColumn?: string; ends?: { from: End; to: End }; via?: string };
export type Row =
  | { t: "col"; col: Column }
  | { t: "fold"; names: string[] }
  | { t: "i18n"; names: string[] }
  | { t: "more"; n: number };
export type Card = { id: string; table: Table; col: number; x: number; y: number; w: number; h: number; rows: Row[] };
export type Tip = { x: number; y: number; out: [number, number]; end?: End };
export type Line = { id: string; from: string; to: string; a: Tip; b: Tip; d: string; via?: string; mid: [number, number] };
export type Focus = { cards: Card[]; lines: Line[]; w: number; h: number };

export const SIZE = { centre: 300, side: 240, head: 44, row: 26, pad: 8, gapX: 150, gapY: 24, margin: 32, loop: 56 };
const STATES = ["sql", "prisma", "django", "sqlalchemy", "drizzle", "typeorm"];

const flat = (a: MapArrow): MapArrow[] => (a.parts?.length ? a.parts.flatMap(flat) : [a]);

export function relations(view: View): Rel[] {
  const seen = new Map<string, Rel>();
  for (const a of view.arrows.flatMap(flat)) {
    if (seen.has(a.id) || a.from === a.to || !view.tables?.[a.from] || !view.tables?.[a.to]) continue;
    seen.set(a.id, { id: a.id, from: a.from, to: a.to, fromColumn: a.fromColumn, toColumn: a.toColumn, ends: a.ends, via: a.via });
  }
  return [...seen.values()];
}

const groupKind = (view: View, id: string): TableGroup | undefined => {
  const p = view.boxes.find((b) => b.id === id)?.parent;
  return p ? view.tableGroups?.[p] : undefined;
};

function hidden(view: View, rels: Rel[], folds: Folds, centre: string): Set<string> {
  const out = new Set<string>();
  if (folds.translations) for (const id of Object.keys(view.tables ?? {})) if (groupKind(view, id) === "translations") out.add(id);
  if (folds.joins) {
    const byName = new Map(Object.entries(view.tables ?? {}).map(([id, t]) => [t.name.toLowerCase(), id]));
    for (const r of rels) if (r.via && byName.has(r.via.toLowerCase())) out.add(byName.get(r.via.toLowerCase())!);
  }
  out.delete(centre);
  return out;
}

export function visible(view: View, centre: string, folds: Folds = FOLDS): Rel[] {
  const rels = relations(view), off = hidden(view, rels, folds, centre);
  return rels.filter((r) => (folds.joins ? true : !r.via) && !off.has(r.from) && !off.has(r.to));
}

function rowsOf(t: Table, centre: boolean, need: Set<string>, folds: Folds): Row[] {
  if (centre) {
    const folded = folds.bookkeeping ? t.columns.filter((c) => isBookkeeping(c) && !isKey(c)) : [];
    const rows: Row[] = t.columns.filter((c) => !folded.includes(c)).map((col) => ({ t: "col", col }));
    if (folded.length) rows.push({ t: "fold", names: folded.map((c) => c.name) });
    if (folds.translations && t.translations?.length) rows.push({ t: "i18n", names: t.translations });
    return rows;
  }
  const shown = t.columns.filter((c) => isKey(c) || need.has(c.name.toLowerCase()));
  const rows: Row[] = shown.map((col) => ({ t: "col", col }));
  if (t.columns.length > shown.length) rows.push({ t: "more", n: t.columns.length - shown.length });
  return rows;
}

export function anchor(card: Card, column?: string): number {
  const name = column?.toLowerCase();
  const i = name ? card.rows.findIndex((r) => r.t === "col" && r.col.name.toLowerCase() === name) : -1;
  const pk = card.rows.findIndex((r) => r.t === "col" && !!r.col.pk);
  const at = i >= 0 ? i : column === undefined ? -1 : pk;
  return at >= 0 ? card.y + SIZE.head + at * SIZE.row + SIZE.row / 2 : card.y + SIZE.head / 2;
}

const pkOf = (t: Table) => t.columns.find((c) => c.pk)?.name;

export function focusLayout(view: View, centre: string, hops: Hops = 1, folds: Folds = FOLDS): Focus | null {
  const me = view.tables?.[centre];
  if (!me) return null;
  const rels = visible(view, centre, folds);
  const col = new Map<string, number>([[centre, 0]]);
  const put = (id: string, c: number) => !col.has(id) && col.set(id, c);
  const toLeft = (r: Rel, id: string) => !r.via && r.to === id;
  const around = (id: string, side: -1 | 1) =>
    rels.filter((r) => (r.from === id || r.to === id) && (side === -1 ? toLeft(r, id) : !toLeft(r, id))).map((r) => (r.from === id ? r.to : r.from));
  const centreCols = me.columns.map((c) => c.name.toLowerCase());
  const rank = (id: string) => {
    const r = rels.find((r) => r.from === centre && r.to === id);
    const i = r?.fromColumn ? centreCols.indexOf(r.fromColumn.toLowerCase()) : -1;
    return (r?.via ? 1e6 : 0) + (i < 0 ? 1e5 : i);
  };
  const name = (id: string) => view.tables![id].name;
  const right = [...new Set(around(centre, 1))].sort((a, b) => rank(a) - rank(b) || name(a).localeCompare(name(b)));
  const left = [...new Set(around(centre, -1))].filter((id) => !right.includes(id)).sort((a, b) => name(a).localeCompare(name(b)));
  for (const id of right) put(id, 1);
  for (const id of left) put(id, -1);
  if (hops === 2) {
    for (const id of left) for (const o of around(id, -1)) put(o, -2);
    for (const id of right) for (const o of around(id, 1)) put(o, 2);
  }

  const need = new Map<string, Set<string>>();
  const needs = (id: string, c?: string) => c && (need.get(id) ?? need.set(id, new Set()).get(id)!).add(c.toLowerCase());
  for (const r of rels) {
    needs(r.from, r.fromColumn);
    needs(r.to, r.toColumn ?? pkOf(view.tables![r.to]));
  }

  const columns = [...new Set(col.values())].sort((a, b) => a - b);
  const byCol = new Map(columns.map((c) => [c, [...col].filter(([, k]) => k === c).map(([id]) => id)]));
  const width = (c: number) => (c === 0 ? SIZE.centre : SIZE.side);
  const xs = new Map<number, number>();
  let x = SIZE.margin;
  for (const c of columns) {
    xs.set(c, x);
    x += width(c) + SIZE.gapX;
  }
  const cards: Card[] = [];
  for (const c of columns) {
    let y = 0;
    for (const id of byCol.get(c)!) {
      const rows = rowsOf(view.tables![id], c === 0, need.get(id) ?? new Set(), folds);
      const h = SIZE.head + rows.length * SIZE.row + SIZE.pad;
      cards.push({ id, table: view.tables![id], col: c, x: xs.get(c)!, y, w: width(c), h, rows });
      y += h + SIZE.gapY;
    }
  }
  const colH = (c: number) => cards.filter((k) => k.col === c).reduce((s, k) => s + k.h + SIZE.gapY, -SIZE.gapY);
  const H = Math.max(...columns.map(colH));
  for (const k of cards) k.y += SIZE.margin + (H - colH(k.col)) / 2;

  const at = new Map(cards.map((k) => [k.id, k]));
  const lines: Line[] = [];
  for (const r of rels) {
    const a = at.get(r.from), b = at.get(r.to);
    if (!a || !b || Math.abs(a.col - b.col) !== 1) continue;
    const ay = anchor(a, r.fromColumn), by = anchor(b, r.toColumn ?? pkOf(b.table));
    const aRight = a.col < b.col;
    const ax = aRight ? a.x + a.w : a.x, bx = aRight ? b.x : b.x + b.w;
    const mx = (ax + bx) / 2;
    lines.push({
      id: r.id, from: r.from, to: r.to, via: r.via, mid: [mx, (ay + by) / 2],
      a: { x: ax, y: ay, out: [aRight ? 1 : -1, 0], end: r.ends?.from },
      b: { x: bx, y: by, out: [aRight ? -1 : 1, 0], end: r.ends?.to },
      d: `M${ax} ${ay}C${mx} ${ay} ${mx} ${by} ${bx} ${by}`,
    });
  }
  const self = at.get(centre)!;
  const known = STATES.includes(me.source);
  for (const c of me.columns) {
    if (c.references?.table.toLowerCase() !== me.name.toLowerCase()) continue;
    const ay = anchor(self, c.name), by = anchor(self, c.references.column ?? pkOf(me));
    const ex = self.x + self.w, lx = ex + SIZE.loop;
    lines.push({
      id: `${centre}:self:${c.name}`, from: centre, to: centre, mid: [lx, (ay + by) / 2],
      a: { x: ex, y: ay, out: [1, 0], end: known ? (c.unique || c.pk ? "zeroOne" : "many0") : undefined },
      b: { x: ex, y: by, out: [1, 0], end: known ? (c.nullable ? "zeroOne" : "one") : undefined },
      d: `M${ex} ${ay}C${lx} ${ay} ${lx} ${by} ${ex} ${by}`,
    });
  }
  const loops = lines.some((l) => l.from === l.to) ? SIZE.loop : 0;
  return { cards, lines, w: x - SIZE.gapX + SIZE.margin + (columns.at(-1) === 0 ? loops : 0), h: H + 2 * SIZE.margin };
}

export function storeOf(view: View, id: string): string | undefined {
  const byId = new Map(view.boxes.map((b) => [b.id, b]));
  for (let b: MapBox | undefined = byId.get(id), n = 0; b && n < 64; b = b.parent ? byId.get(b.parent) : undefined, n++)
    if (b.kind.value === "store") return b.id;
  return undefined;
}

export type TreeTable = { id: string; name: string; kind: Table["kind"]; matched: string[] };
export type TreeGroup = { id: string; title: string; kind: TableGroup | "store"; count: number; tables: TreeTable[] };
export type Tree = { title: string; tables: number; views: number; groups: TreeGroup[]; aside: TreeGroup[] };

const ORDER: Record<string, number> = { domain: 0, other: 1, views: 2, store: 3, translations: 4, leftovers: 5 };

export function treeOf(view: View, store: string, query = ""): Tree {
  const q = query.trim().toLowerCase();
  const boxes = view.boxes, tables = view.tables ?? {};
  const groups: TreeGroup[] = [];
  const of = (parent: string) => boxes.filter((b) => b.parent === parent && tables[b.id]).map((b) => ({ id: b.id, t: tables[b.id] }));
  const group = (id: string, title: string, kind: TreeGroup["kind"]) => {
    const all = of(id);
    const list = all.flatMap(({ id, t }): TreeTable[] => {
      if (!q || t.name.toLowerCase().includes(q)) return [{ id, name: t.name, kind: t.kind, matched: [] }];
      const matched = t.columns.filter((c) => c.name.toLowerCase().includes(q)).map((c) => c.name);
      return matched.length ? [{ id, name: t.name, kind: t.kind, matched }] : [];
    }).sort((a, b) => a.name.localeCompare(b.name));
    if (all.length && (!q || list.length)) groups.push({ id, title, kind, count: all.length, tables: list });
  };
  for (const b of boxes) if (b.parent === store && view.tableGroups?.[b.id]) group(b.id, b.title.value, view.tableGroups[b.id]);
  group(store, "tables", "store");
  groups.sort((a, b) => ORDER[a.kind] - ORDER[b.kind] || a.title.localeCompare(b.title));
  const aside = (g: TreeGroup) => g.kind === "translations" || g.kind === "leftovers";
  const counted = Object.entries(tables).filter(([id]) => storeOf(view, id) === store && !["translations", "leftovers"].includes(groupKind(view, id) ?? ""));
  return {
    title: boxes.find((b) => b.id === store)?.title.value ?? store,
    tables: counted.filter(([, t]) => t.kind === "table").length,
    views: counted.filter(([, t]) => t.kind === "view").length,
    groups: groups.filter((g) => !aside(g)),
    aside: groups.filter(aside),
  };
}

export const groupOf = (view: View, id: string) => view.boxes.find((b) => b.id === id)?.parent;
