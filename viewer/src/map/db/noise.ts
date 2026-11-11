import type { ArrowKind, MapArrow, MapBox, Table, View } from "../types";
import { HEAD } from "../camera";
import { settle, shownView } from "../place/rules";
import { cardSize, type CardFacts } from "./card";
import { leaves } from "./wiring";

export type Noise = { translations: boolean; joins: boolean; bookkeeping: boolean; leftovers: boolean; hubs: boolean };
export const NOISE: Noise = { translations: true, joins: true, bookkeeping: true, leftovers: true, hubs: true };

const NOUNS: [ArrowKind, string][] = [
  ["calls", "call"], ["sends", "message"], ["reads", "read"], ["writes", "write"], ["triggers", "trigger"], ["implements", "implementation"], ["uses", "use"],
];

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
  const all = view.arrows.flatMap(leaves);
  for (const a of all) {
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
    const own = leaves(a);
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

  const frames = { ...view.placed.frames }, inner = { ...view.placed.inner };
  for (const b of view.boxes) if (dropBox(b) || moved.has(b.id)) delete frames[b.id];
  const boxesOut = view.boxes.filter((b) => !dropBox(b)).map((b) => (moved.has(b.id) ? { ...b, parent: moved.get(b.id) } : b));
  const tablesOut: Record<string, Table> = {};
  for (const [id, t] of Object.entries(tables)) {
    if (gone.has(id)) continue;
    tablesOut[id] = !s.translations && t.translations ? { ...t, translations: undefined } : t;
  }
  const groupsOut = Object.fromEntries(Object.entries(groups).filter(([g]) => !emptied.has(g)));
  const next: View = {
    ...view, boxes: boxesOut, arrows: out, tables: tablesOut, tableGroups: groupsOut,
    placed: { ...view.placed, frames, inner, regions: view.placed.regions.filter((r) => !emptied.has(r.parent) && !gone.has(r.parent)) },
  };
  const bare = boxesOut.filter((b) => !frames[b.id] && (tables[b.id] || groupsOut[b.id]));
  const empty = boxesOut.filter((b) => groupsOut[b.id] && frames[b.id] && !inner[b.id] && boxesOut.some((k) => k.parent === b.id && tables[k.id] && !frames[k.id]));
  if (!bare.length) return next;
  size(next, bare, empty, tables);
  const prev = settle({}, view, {}, null);
  for (const id of [...gone, ...emptied, ...moved.keys()]) delete prev[id];
  return shownView(next, settle(prev, next, {}, null));
}

const GUTTER = 96;

function size(view: View, bare: MapBox[], empty: MapBox[], tables: Record<string, Table>) {
  const { frames, inner } = view.placed;
  const card = (id: string) => ({ x: 0, y: 0, ...cardSize(tables[id]) });
  const ratios = Object.keys(view.tableGroups ?? {}).flatMap((g) => (frames[g] && inner[g] ? [frames[g].w / inner[g][0]] : [])).sort((a, b) => a - b);
  const scale = ratios[Math.floor(ratios.length / 2)] ?? 0.4;
  for (const g of [...bare.filter((b) => view.tableGroups?.[b.id]), ...empty]) {
    const kids = bare.filter((b) => b.parent === g.id && tables[b.id]);
    const cols = Math.max(1, Math.ceil(Math.sqrt(kids.length)));
    const at = (i: number) => ({ c: i % cols, r: Math.floor(i / cols) });
    const wide = Math.max(...kids.map((k) => card(k.id).w), 0);
    const rowH = Array.from({ length: Math.ceil(kids.length / cols) }, (_, r) => Math.max(...kids.filter((_, i) => at(i).r === r).map((k) => card(k.id).h)));
    kids.forEach((k, i) => {
      const { c, r } = at(i), c0 = card(k.id);
      frames[k.id] = { ...c0, x: GUTTER + c * (wide + GUTTER), y: GUTTER + rowH.slice(0, r).reduce((n, h) => n + h + GUTTER, 0) };
    });
    const w = GUTTER * 2 + cols * wide + (cols - 1) * GUTTER, h = GUTTER * 2 + rowH.reduce((n, x) => n + x, 0) + (rowH.length - 1) * GUTTER;
    inner[g.id] = [w, h];
    frames[g.id] ??= { x: 0, y: 0, w: w * scale, h: h * scale + HEAD };
  }
  for (const b of bare) if (!frames[b.id] && tables[b.id]) frames[b.id] = card(b.id);
}

const anyTables = (view: View) => !!view.tables && Object.keys(view.tables).length > 0;

export const applyNoise = (view: View, s: Noise = NOISE): View => (anyTables(view) ? rebuild(view, s, s.hubs) : view);

// by table box id: how many drawn tables point at it when it is a hub, and its pinned keys' targets by name
export function cardFacts(given: View, hubs: boolean, drawn: Record<string, Table> | undefined): Map<string, CardFacts> {
  const out = new Map<string, CardFacts>();
  if (!hubs || !drawn) return out;
  const of = (id: string) => out.get(id) ?? out.set(id, { hub: 0, pinned: new Map() }).get(id)!;
  const pointing = new Map<string, Set<string>>();
  for (const a of given.arrows.flatMap(leaves)) {
    if (!a.pinned || a.via || !drawn[a.from] || !drawn[a.to]) continue;
    const to = drawn[a.to];
    pointing.set(a.to, (pointing.get(a.to) ?? new Set()).add(a.from));
    of(a.from).pinned.set(to.name.toLowerCase(), to.name);
  }
  for (const [id, from] of pointing) of(id).hub = from.size;
  return out;
}

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
