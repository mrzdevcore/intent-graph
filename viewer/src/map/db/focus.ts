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
// Rust's TELLS_NULLABLE (crates/ig-app/src/map/tables.rs)
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

type Index = { boxes: Map<string, MapBox>; kids: Map<string, MapBox[]>; store: Map<string, string | undefined>; byName: Map<string, Map<string, string>> };
const indexes = new WeakMap<View, Index>();

function indexOf(view: View): Index {
  const had = indexes.get(view);
  if (had) return had;
  const boxes = new Map(view.boxes.map((b) => [b.id, b]));
  const kids = new Map<string, MapBox[]>();
  for (const b of view.boxes) if (b.parent) (kids.get(b.parent) ?? kids.set(b.parent, []).get(b.parent)!).push(b);
  const store = new Map<string, string | undefined>();
  const up = (id: string): string | undefined => {
    if (store.has(id)) return store.get(id);
    store.set(id, undefined);
    const b = boxes.get(id);
    const s = !b ? undefined : b.kind.value === "store" ? b.id : b.parent ? up(b.parent) : undefined;
    store.set(id, s);
    return s;
  };
  for (const id of boxes.keys()) up(id);
  const byName = new Map<string, Map<string, string>>();
  for (const [id, t] of Object.entries(view.tables ?? {})) {
    const s = up(id) ?? "";
    (byName.get(s) ?? byName.set(s, new Map()).get(s)!).set(t.name.toLowerCase(), id);
  }
  const ix = { boxes, kids, store, byName };
  indexes.set(view, ix);
  return ix;
}

export const storeOf = (view: View, id: string) => indexOf(view).store.get(id);
export const hasTables = (view: View, store: string) => !!store && indexOf(view).byName.has(store);
export const groupOf = (view: View, id: string) => indexOf(view).boxes.get(id)?.parent;
const tableIn = (view: View, store: string | undefined, name: string) => indexOf(view).byName.get(store ?? "")?.get(name.toLowerCase());

const groupKind = (view: View, id: string): TableGroup | undefined => {
  const p = groupOf(view, id);
  return p ? view.tableGroups?.[p] : undefined;
};

function hidden(view: View, rels: Rel[], folds: Folds, centre: string): Set<string> {
  const out = new Set<string>();
  if (folds.translations) for (const id of Object.keys(view.tables ?? {})) if (groupKind(view, id) === "translations") out.add(id);
  if (folds.joins)
    for (const r of rels) {
      const join = r.via ? tableIn(view, storeOf(view, r.from), r.via) : undefined;
      if (join) out.add(join);
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

export function columnEnds(t: Table, c: Column): { from: End; to: End } | undefined {
  if (!STATES.includes(t.source)) return undefined;
  const pk = t.columns.filter((k) => k.pk);
  return {
    from: c.unique || (pk.length === 1 && pk[0].name === c.name) ? "zeroOne" : "many0",
    to: c.nullable ? "zeroOne" : "one",
  };
}

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

  const store = storeOf(view, centre);
  const adjacent = (a: string, b: string) => col.has(a) && col.has(b) && Math.abs(col.get(a)! - col.get(b)!) === 1;
  const specs: Rel[] = rels.filter((r) => r.via && adjacent(r.from, r.to));
  for (const from of col.keys()) {
    const t = view.tables![from];
    for (const c of t.columns) {
      const to = c.references ? tableIn(view, store, c.references.table) : undefined;
      if (!to || to === from || !adjacent(from, to)) continue;
      specs.push({ id: `${from}.${c.name}->${to}`, from, to, fromColumn: c.name, toColumn: c.references!.column ?? pkOf(view.tables![to]), ends: columnEnds(t, c) });
    }
  }
  const need = new Map<string, Set<string>>();
  const needs = (id: string, c?: string) => c && (need.get(id) ?? need.set(id, new Set()).get(id)!).add(c.toLowerCase());
  for (const r of specs) {
    needs(r.from, r.fromColumn);
    needs(r.to, r.toColumn);
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
  const lines: Line[] = specs.map((r) => {
    const a = at.get(r.from)!, b = at.get(r.to)!, aRight = a.col < b.col;
    return {
      id: r.id, from: r.from, to: r.to, via: r.via, mid: [0, 0], d: "",
      a: { x: aRight ? a.x + a.w : a.x, y: anchor(a, r.fromColumn), out: [aRight ? 1 : -1, 0], end: r.ends?.from },
      b: { x: aRight ? b.x : b.x + b.w, y: anchor(b, r.toColumn ?? pkOf(b.table)), out: [aRight ? -1 : 1, 0], end: r.ends?.to },
    };
  });
  spread(lines);
  for (const l of lines) {
    const mx = (l.a.x + l.b.x) / 2;
    l.mid = [mx, (l.a.y + l.b.y) / 2];
    l.d = `M${l.a.x} ${l.a.y}C${mx} ${l.a.y} ${mx} ${l.b.y} ${l.b.x} ${l.b.y}`;
  }
  const self = at.get(centre)!;
  for (const c of me.columns) {
    if (!c.references || tableIn(view, store, c.references.table) !== centre) continue;
    const ay = anchor(self, c.name), by = anchor(self, c.references.column ?? pkOf(me));
    const ex = self.x + self.w, lx = ex + SIZE.loop, ends = columnEnds(me, c);
    lines.push({
      id: `${centre}:self:${c.name}`, from: centre, to: centre, mid: [lx, (ay + by) / 2],
      a: { x: ex, y: ay, out: [1, 0], end: ends?.from },
      b: { x: ex, y: by, out: [1, 0], end: ends?.to },
      d: `M${ex} ${ay}C${lx} ${ay} ${lx} ${by} ${ex} ${by}`,
    });
  }
  const loops = lines.some((l) => l.from === l.to) ? SIZE.loop : 0;
  return { cards, lines, w: x - SIZE.gapX + SIZE.margin + (columns.at(-1) === 0 ? loops : 0), h: H + 2 * SIZE.margin };
}

function spread(lines: Line[]) {
  const at = new Map<string, { tip: Tip; other: Tip }[]>();
  for (const l of lines)
    for (const [tip, other] of [[l.a, l.b], [l.b, l.a]] as const) {
      const k = `${tip.x}:${tip.y}`;
      (at.get(k) ?? at.set(k, []).get(k)!).push({ tip, other });
    }
  const half = SIZE.row / 2 - 3;
  for (const list of at.values()) {
    if (list.length < 2) continue;
    list.sort((p, q) => p.other.y - q.other.y);
    const y = list[0].tip.y;
    list.forEach((p, i) => (p.tip.y = y - half + (2 * half * i) / (list.length - 1)));
  }
}

export type TreeTable = { id: string; name: string; kind: Table["kind"]; matched: string[] };
export type TreeGroup = { id: string; title: string; kind: TableGroup | "store"; count: number; tables: TreeTable[] };
export type Tree = { title: string; tables: number; views: number; groups: TreeGroup[]; aside: TreeGroup[] };

const ORDER: Record<string, number> = { domain: 0, other: 1, views: 2, store: 3, translations: 4, leftovers: 5 };

export function treeOf(view: View, store: string, query = ""): Tree {
  const q = query.trim().toLowerCase();
  const ix = indexOf(view), tables = view.tables ?? {};
  const groups: TreeGroup[] = [];
  const of = (parent: string) => (ix.kids.get(parent) ?? []).filter((b) => tables[b.id]).map((b) => ({ id: b.id, t: tables[b.id] }));
  const group = (id: string, title: string, kind: TreeGroup["kind"]) => {
    const all = of(id);
    const list = all.flatMap(({ id, t }): TreeTable[] => {
      if (!q || t.name.toLowerCase().includes(q)) return [{ id, name: t.name, kind: t.kind, matched: [] }];
      const matched = t.columns.filter((c) => c.name.toLowerCase().includes(q)).map((c) => c.name);
      return matched.length ? [{ id, name: t.name, kind: t.kind, matched }] : [];
    }).sort((a, b) => a.name.localeCompare(b.name));
    if (all.length && (!q || list.length)) groups.push({ id, title, kind, count: all.length, tables: list });
  };
  for (const b of ix.kids.get(store) ?? []) if (view.tableGroups?.[b.id]) group(b.id, b.title.value, view.tableGroups[b.id]);
  group(store, "tables", "store");
  groups.sort((a, b) => ORDER[a.kind] - ORDER[b.kind] || a.title.localeCompare(b.title));
  const aside = (g: TreeGroup) => g.kind === "translations" || g.kind === "leftovers";
  const counted = [...(ix.byName.get(store)?.values() ?? [])].filter((id) => !["translations", "leftovers"].includes(groupKind(view, id) ?? ""));
  return {
    title: ix.boxes.get(store)?.title.value ?? store,
    tables: counted.filter((id) => tables[id].kind === "table").length,
    views: counted.filter((id) => tables[id].kind === "view").length,
    groups: groups.filter((g) => !aside(g)),
    aside: groups.filter(aside),
  };
}
