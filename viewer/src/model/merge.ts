// Three-way merge of the board on disk (theirs) into the board on screen (ours). The user wins.

import type { Design } from "../api";
import { bridgeKey, isElementId, linkKey } from "./design";
import type { Board } from "./board";
import type { Lane } from "./architecture";

type M = Map<string, unknown>;
const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);

/** Every keyed thing on a board: design element id, link key, mark node, bridge key, screen id, "<screen>/<element>". */
function keyed(b: Board): { design: M; screens: M } {
  const design: M = new Map();
  b.design.elements.forEach((e) => design.set(`e:${e.id}`, e));
  b.design.links.forEach((l) => design.set(`l:${linkKey(l)}`, l));
  b.design.marks.forEach((m) => design.set(`m:${m.node}`, m));
  b.design.bridges.forEach((x) => design.set(`b:${bridgeKey(x)}`, x));
  const screens: M = new Map();
  for (const [id, s] of Object.entries(b.screens)) {
    const { elements, navs, ...rest } = s;
    screens.set(id, rest);
    (elements ?? []).forEach((e) => screens.set(`${id}/${e.id}`, e));
    (navs ?? []).forEach((n) => screens.set(`${id}>${n.from}>${n.to}`, n));
  }
  return { design, screens };
}

/** Per key: only theirs changed -> theirs; ours changed (or both) -> ours; a removal only counts if ours left it alone. */
function pick(base: M, ours: M, theirs: M, changed: string[], removed: string[]): M {
  const out: M = new Map();
  for (const k of new Set([...base.keys(), ...ours.keys(), ...theirs.keys()])) {
    const [b, o, t] = [base.get(k), ours.get(k), theirs.get(k)];
    const useTheirs = !same(t, b) && same(o, b);
    const v = useTheirs ? t : o;
    if (useTheirs && !same(t, o)) (t === undefined ? removed : changed).push(k);
    if (v !== undefined) out.set(k, v);
  }
  return out;
}

const bare = (k: string) => k.slice(2);

export function mergeBoard(base: Board, ours: Board, theirs: Board): { board: Board; changed: string[]; removed: string[] } {
  const [kb, ko, kt] = [keyed(base), keyed(ours), keyed(theirs)];
  const changed: string[] = [], removed: string[] = [];
  const dm = pick(kb.design, ko.design, kt.design, changed, removed);
  const sm = pick(kb.screens, ko.screens, kt.screens, changed, removed);

  const scalar = <T>(f: (d: Design) => T): T => (same(f(ours.design), f(base.design)) ? f(theirs.design) : f(ours.design));
  const design: Design = structuredClone({ ...ours.design, title: scalar((d) => d.title), scope: scalar((d) => d.scope), version: ours.design.version });
  const pickAll = <T>(p: string) => [...dm].filter(([k]) => k.startsWith(p)).map(([, v]) => v as T);
  design.elements = pickAll(`e:`);
  design.links = pickAll(`l:`);
  design.marks = pickAll(`m:`);
  design.bridges = pickAll(`b:`);

  const screens: Board["screens"] = {};
  for (const [k, v] of sm) if (!/[/>]/.test(k)) screens[k] = { ...(v as object), id: k, elements: [], navs: [] };
  for (const [k, v] of sm) {
    if (k.includes("/")) { const [s] = k.split("/"); screens[s]?.elements!.push(v as { id: string }); }
    else if (k.includes(">")) { const [s] = k.split(">"); screens[s]?.navs!.push(v as { from: string; to: string }); }
  }
  // the screen elements and navs are only listed when the source screen listed them
  for (const [id, s] of Object.entries(screens)) {
    const src = ours.screens[id] ?? theirs.screens[id] ?? base.screens[id];
    if (!src.elements) delete s.elements;
    if (!src.navs) delete s.navs;
  }

  // dangling ends are dropped: an element-id end must name an element; a bridge's screen end a screen element
  const ids = new Set(design.elements.map((e) => e.id));
  const live = (s: string) => !isElementId(s) || ids.has(s);
  design.links = design.links.filter((l) => live(l.from) && live(l.to));
  design.bridges = design.bridges.filter((x) => {
    const [s, e] = x.from.split("/");
    const scr = screens[s];
    return live(x.to) && (!scr || !scr.elements || scr.elements.some((el) => el.id === e));
  });
  for (const s of Object.values(screens)) s.navs = s.navs?.filter((n) => !(isElementId(n.from) && !screens[n.from]) && !(isElementId(n.to) && !screens[n.to]));

  const alive = (k: string) => {
    if (k.startsWith("e:")) return ids.has(bare(k));
    if (k.startsWith("l:")) return design.links.some((l) => linkKey(l) === bare(k));
    if (k.startsWith("b:")) return design.bridges.some((x) => bridgeKey(x) === bare(k));
    return true;
  };
  const report = (ks: string[]) => [...new Set(ks)].map((k) => (/^[elmb]:/.test(k) ? bare(k) : k));
  const ch = changed.filter(alive);
  const rm = [...removed, ...changed.filter((k) => !alive(k))];
  return { board: { design, screens }, changed: report(ch), removed: report(rm) };
}

/** Puts unplaced elements (no x/y) in their lane: layer elements in their layer's row, the rest below. */
export function placeNew(b: Board, ids: string[], lanes: Lane[]): Board {
  const out = structuredClone(b);
  const bottom = Math.max(0, ...lanes.map((l) => l.bottom));
  const nextX = new Map<string, number>();
  for (const id of ids) {
    const e = out.design.elements.find((x) => x.id === id);
    if (!e || (e.x !== undefined && e.y !== undefined)) continue;
    const lane = e.kind === "system" ? lanes.find((l) => l.kind === "external") : lanes.find((l) => l.kind === "layer" && l.name === e.layer);
    const key = lane ? `${lane.row}` : "tail";
    const x = nextX.get(key) ?? 80;
    nextX.set(key, x + 160);
    e.x = x;
    e.y = Math.round((lane ? lane.top + 60 : bottom + 60) / 8) * 8;
  }
  return out;
}
