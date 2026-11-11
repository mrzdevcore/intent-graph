import type { ArrowKind, MapArrow, MapBox, Table, View } from "../types";
import type { Folds } from "./focus";

export type Noise = Folds;
export const NOISE: Noise = { translations: true, joins: true, bookkeeping: true, leftovers: true, hubs: true };

const NOUNS: [ArrowKind, string][] = [
  ["calls", "call"], ["sends", "message"], ["reads", "read"], ["writes", "write"], ["triggers", "trigger"], ["implements", "implementation"], ["uses", "use"],
];

const flat = (a: MapArrow): MapArrow[] => (a.parts?.length ? a.parts.flatMap(flat) : [a]);

function rebuild(view: View, s: Noise, dropHubs: boolean): View {
  const tables = view.tables;
  if (!tables) return view;
  const boxes = new Map(view.boxes.map((b) => [b.id, b]));
  const groups = view.tableGroups ?? {};
  const parent = new Map(view.boxes.map((b) => [b.id, b.parent]));
  const storeOf = (id: string): string | undefined => {
    for (let at: string | undefined = id, n = 0; at && n < 64; at = parent.get(at), n++) if (boxes.get(at)?.kind.value === "store") return at;
    return undefined;
  };
  const kindOf = (id: string) => groups[parent.get(id) ?? ""];

  const gone = new Set<string>();
  const moved = new Map<string, string>();
  for (const id of Object.keys(tables)) {
    if (s.translations && kindOf(id) === "translations") gone.add(id);
    if (!s.leftovers && kindOf(id) === "leftovers") {
      const store = storeOf(id)!;
      const prefix = tables[id].name.toLowerCase().split("_")[0];
      const to = [`${store}#${prefix}_*`, `${store}#(other)`].find((g) => groups[g] === "domain" || groups[g] === "other") ?? store;
      moved.set(id, to);
    }
  }
  const joinNames = new Map<string, string>();
  const byName = new Map<string, string>();
  for (const id of Object.keys(tables)) byName.set(`${storeOf(id)}\0${tables[id].name.toLowerCase()}`, id);
  const leaves = view.arrows.flatMap(flat);
  for (const a of leaves) {
    const j = a.via && tables[a.from] ? byName.get(`${storeOf(a.from)}\0${a.via.toLowerCase()}`) : undefined;
    if (j) joinNames.set(j, a.via!);
  }
  if (s.joins) for (const j of joinNames.keys()) gone.add(j);

  const where = (id: string) => moved.get(id) ?? parent.get(id);
  const emptied = new Set(Object.keys(groups).filter((g) => {
    const kids = view.boxes.filter((b) => b.parent === g);
    return kids.length > 0 && kids.every((b) => gone.has(b.id) || moved.has(b.id));
  }));
  const dropBox = (b: MapBox) => gone.has(b.id) || emptied.has(b.id);

  const dbLeaf = (a: MapArrow) => !!tables[a.from] && !!tables[a.to];
  const out: MapArrow[] = [];
  const pairs = new Map<string, MapArrow[]>();
  const place = (a: MapArrow) => {
    const pf = where(a.from)!, pt = where(a.to)!;
    const p = pf === pt ? pf : storeOf(a.from)!;
    const lift = (id: string, at: string) => (at === p ? id : at);
    const [from, to] = [lift(a.from, pf), lift(a.to, pt)];
    if (from === a.from && to === a.to) return out.push({ ...a, parent: p, parts: undefined, pinned: s.hubs ? a.pinned : false });
    const lifted: MapArrow = { ...a, parent: p, from, to, parts: [a], fromColumn: undefined, toColumn: undefined, ends: undefined, via: undefined, pinned: false };
    const key = `${p}\0${from}\0${to}\0${a.origin ?? ""}`;
    (pairs.get(key) ?? pairs.set(key, []).get(key)!).push(lifted);
  };

  for (const a of view.arrows) {
    const own = flat(a);
    if (!own.some(dbLeaf)) {
      out.push(a);
      continue;
    }
    for (const l of own) {
      if (!dbLeaf(l)) continue;
      if (gone.has(l.from) || gone.has(l.to)) continue;
      if (l.via && !s.joins) continue;
      if (l.pinned && !l.via && dropHubs) continue;
      place(s.hubs || !l.pinned ? l : { ...l, pinned: false });
    }
  }
  for (const list of pairs.values()) {
    if (list.length === 1) {
      out.push(list[0]);
      continue;
    }
    const kinds = [...new Set(list.flatMap((a) => a.kinds))].sort();
    const label = NOUNS.flatMap(([k, noun]) => {
      const n = list.filter((a) => a.kinds.includes(k)).length;
      return n ? [`${n} ${noun}${n > 1 ? "s" : ""}`] : [];
    }).join(" · ");
    out.push({
      ...list[0], kind: kinds.length > 1 ? "bundle" : kinds[0], label, kinds, count: list.reduce((n, a) => n + Math.max(a.count, 1), 0),
      parts: list.flatMap((a) => a.parts ?? []), inLoop: list.some((a) => a.inLoop), shape: undefined,
    });
  }
  out.sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));

  const frames = { ...view.placed.frames };
  for (const b of view.boxes) if (dropBox(b) || moved.has(b.id)) delete frames[b.id];
  const boxesOut = view.boxes.filter((b) => !dropBox(b)).map((b) => (moved.has(b.id) ? { ...b, parent: moved.get(b.id) } : b));
  const tablesOut: Record<string, Table> = {};
  for (const [id, t] of Object.entries(tables)) {
    if (gone.has(id)) continue;
    tablesOut[id] = !s.translations && t.translations ? { ...t, translations: undefined } : t;
  }
  const groupsOut = Object.fromEntries(Object.entries(groups).filter(([g]) => !emptied.has(g)));
  return {
    ...view, boxes: boxesOut, arrows: out, tables: tablesOut, tableGroups: groupsOut,
    placed: { ...view.placed, frames, regions: view.placed.regions.filter((r) => !emptied.has(r.parent) && !gone.has(r.parent)) },
  };
}

const hasTables = (view: View) => !!view.tables && Object.keys(view.tables).length > 0;

export const foldNoise = (view: View, s: Noise = NOISE): View => (hasTables(view) ? rebuild(view, s, false) : view);
export const applyNoise = (view: View, s: Noise = NOISE): View => (hasTables(view) ? rebuild(view, s, s.hubs) : view);

const KEY = "ig-noise";
const SWITCHES = Object.keys(NOISE) as (keyof Noise)[];

export function loadNoise(): Noise {
  try {
    const off = (globalThis.window?.localStorage?.getItem(KEY) ?? "").split(",");
    return Object.fromEntries(SWITCHES.map((k) => [k, !off.includes(k)])) as Noise;
  } catch {
    return { ...NOISE };
  }
}

export function saveNoise(n: Noise) {
  try {
    const off = SWITCHES.filter((k) => !n[k]);
    if (off.length) globalThis.window?.localStorage?.setItem(KEY, off.join(","));
    else globalThis.window?.localStorage?.removeItem(KEY);
  } catch { /* private mode: the setting lasts the page */ }
}
