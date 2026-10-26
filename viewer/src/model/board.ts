// The board: a design plus its screens, edited by pure actions. No DOM, no Svelte.

import type { Design, MarkKind } from "../api";
import type { DesignScope as Scope } from "../api";
import { rowAt, type Lane } from "./architecture";
import { bridgeKey, linkKey, newId, type Bridge, type Element, type Link } from "./design";

/** Task 10 defines the real screen; the board only needs ids, elements and navs. */
export interface Screen {
  id: string;
  elements?: { id: string; [k: string]: unknown }[];
  navs?: { from: string; to: string; [k: string]: unknown }[];
  [k: string]: unknown;
}
export interface Board {
  design: Design;
  screens: Record<string, Screen>;
}

export const GRID = 8;
export const snap = (v: number): number => Math.round(v / GRID) * GRID;

type Patch = Partial<Pick<Element, "file" | "method" | "path" | "icon" | "layer" | "frame" | "on" | "question" | "answer">>;
export type ArchAction =
  | { t: "add"; el: Omit<Element, "id"> } | { t: "move"; ids: string[]; dx: number; dy: number; lanes: Lane[] }
  | { t: "resize"; id: string; w: number; h: number } | { t: "rename"; id: string; name: string }
  | { t: "set"; id: string; patch: Patch }
  | { t: "delete"; ids: string[] } | { t: "link"; link: Link } | { t: "unlink"; key: string }
  | { t: "mark"; node: string; mark: MarkKind | null } | { t: "paste"; els: Element[]; links: Link[]; dx: number; dy: number }
  | { t: "scope"; scope: Scope } | { t: "bridge"; bridge: Bridge } | { t: "unbridge"; key: string };
export type Action = ArchAction; // ponytail: ScreenAction joins this union in Task 10

const GEOM = new Set(["x", "y", "w", "h"]);
const stripGeom = (_: string, v: unknown) => (GEOM.has(_) ? undefined : v);

/** A fingerprint of everything but geometry; collections sorted so order never matters. */
export function meaning(b: Board): string {
  const d = b.design;
  const by = <T>(xs: T[], k: (x: T) => string) => [...xs].sort((a, c) => (k(a) < k(c) ? -1 : 1));
  return JSON.stringify(
    {
      ...d,
      elements: by(d.elements, (e) => e.id),
      marks: by(d.marks, (m) => m.node),
      links: by(d.links, linkKey),
      bridges: by(d.bridges, bridgeKey),
      screens: Object.keys(b.screens).sort().map((k) => b.screens[k]),
    },
    stripGeom,
  );
}

/** Position-only change: the outbox has nothing to tell the agent. */
export const geometryOnly = (prev: Board, next: Board): boolean => meaning(prev) === meaning(next) && JSON.stringify(prev) !== JSON.stringify(next);

function describe(e: Element): string {
  const what = e.kind === "route" ? `${e.method} ${e.path}` : e.name;
  return e.file ? `${what}, ${e.file}` : what;
}

const FIELDS = ["file", "method", "path", "icon", "layer", "frame", "on", "question", "answer"] as const;

/** Human lines for what changed between two boards; geometry-only gives []. */
export function boardDelta(prev: Board, next: Board): string[] {
  const out: string[] = [];
  const a = prev.design, b = next.design;
  const was = new Map(a.elements.map((e) => [e.id, e]));
  const now = new Map(b.elements.map((e) => [e.id, e]));
  b.elements.filter((e) => !was.has(e.id)).forEach((e) => out.push(`added ${e.kind} ${e.id} (${describe(e)})`));
  b.elements.forEach((e) => {
    const o = was.get(e.id);
    if (!o) return;
    if (o.name !== e.name) out.push(`renamed ${e.id}: ${o.name} → ${e.name}`);
    FIELDS.filter((f) => o[f] !== e[f]).forEach((f) => out.push(`set ${e.id} ${f}: ${o[f] ?? "none"} → ${e[f] ?? "none"}`));
  });
  a.elements.filter((e) => !now.has(e.id)).forEach((e) => out.push(`removed ${e.id}`));
  const lk = (d: Design) => new Map(d.links.map((l) => [linkKey(l), l]));
  const [la, lb] = [lk(a), lk(b)];
  lb.forEach((l, k) => !la.has(k) && out.push(`linked ${l.from} ${l.kind} ${l.to}`));
  la.forEach((l, k) => !lb.has(k) && out.push(`unlinked ${l.from} ${l.kind} ${l.to}`));
  const mk = (d: Design) => new Map(d.marks.map((m) => [m.node, m.mark]));
  const [ma, mb] = [mk(a), mk(b)];
  mb.forEach((m, n) => ma.get(n) !== m && out.push(`marked ${n} ${m}`));
  ma.forEach((_, n) => !mb.has(n) && out.push(`unmarked ${n}`));
  const bk = (d: Design) => new Set(d.bridges.map(bridgeKey));
  const [ba, bb] = [bk(a), bk(b)];
  b.bridges.forEach((x) => !ba.has(bridgeKey(x)) && out.push(`bridged ${x.from} to ${x.to}`));
  a.bridges.forEach((x) => !bb.has(bridgeKey(x)) && out.push(`unbridged ${x.from} to ${x.to}`));
  if (JSON.stringify(a.scope) !== JSON.stringify(b.scope)) out.push(`scope: ${JSON.stringify(b.scope)}`);
  if (a.title !== b.title) out.push(`title: ${b.title}`);
  const sa = JSON.stringify(prev.screens, stripGeom), sb = JSON.stringify(next.screens, stripGeom);
  if (sa !== sb) {
    Object.keys(next.screens).forEach((k) => !prev.screens[k] && out.push(`added screen ${k}`));
    Object.keys(prev.screens).forEach((k) => !next.screens[k] && out.push(`removed screen ${k}`));
    Object.keys(next.screens).forEach((k) => prev.screens[k] && JSON.stringify(prev.screens[k], stripGeom) !== JSON.stringify(next.screens[k], stripGeom) && out.push(`changed screen ${k}`));
  }
  return out;
}

const hasEl = (d: Design, id: string) => d.elements.some((e) => e.id === id);

export function apply(board: Board, a: Action): { board: Board; selected: string[]; touched: string[] } {
  const b = structuredClone(board);
  const d = b.design;
  const get = (id: string) => d.elements.find((e) => e.id === id);
  const done = (selected: string[], touched = selected) => ({ board: b, selected, touched });
  switch (a.t) {
    case "add": {
      const el: Element = { ...structuredClone(a.el), id: newId(d, a.el.kind, a.el.name) };
      if (el.x !== undefined) el.x = snap(el.x);
      if (el.y !== undefined) el.y = snap(el.y);
      d.elements.push(el);
      return done([el.id]);
    }
    case "move": {
      const moved: string[] = [];
      for (const id of a.ids) {
        const e = get(id);
        if (!e || e.x === undefined || e.y === undefined) continue;
        const y = snap(e.y + a.dy);
        const mid = y + (e.h ?? 0) / 2;
        const lane = a.lanes.find((l) => mid >= l.top && mid <= l.bottom);
        if (lane?.kind === "external" && e.kind !== "system") continue; // only systems live in External services
        e.x = snap(e.x + a.dx);
        e.y = y;
        if (e.kind === "module" || e.kind === "function" || e.kind === "route") {
          const layer = rowAt(a.lanes, mid);
          if (layer) e.layer = layer; else delete e.layer;
        }
        moved.push(id);
      }
      return done(a.ids, moved);
    }
    case "resize": {
      const e = get(a.id);
      if (e) { e.w = Math.max(GRID, snap(a.w)); e.h = Math.max(GRID, snap(a.h)); }
      return done([a.id], e ? [a.id] : []);
    }
    case "rename": {
      const e = get(a.id);
      if (e) e.name = a.name; // the id stays: links and bridges point at it
      return done([a.id], e ? [a.id] : []);
    }
    case "set": {
      const e = get(a.id);
      if (e) for (const [k, v] of Object.entries(a.patch)) v === undefined ? delete (e as unknown as Record<string, unknown>)[k] : Object.assign(e, { [k]: v });
      return done([a.id], e ? [a.id] : []);
    }
    case "delete": {
      const gone = new Set(a.ids.filter((id) => hasEl(d, id)));
      d.elements = d.elements.filter((e) => !gone.has(e.id));
      d.elements.forEach((e) => {
        if (e.on && gone.has(e.on)) delete e.on;
        if (e.frame && gone.has(e.frame)) delete e.frame;
      });
      d.links = d.links.filter((l) => !gone.has(l.from) && !gone.has(l.to));
      d.bridges = d.bridges.filter((x) => !gone.has(x.to) && !gone.has(x.from.split("/")[1]));
      return done([], [...gone]);
    }
    case "link":
      if (!d.links.some((l) => linkKey(l) === linkKey(a.link))) d.links.push({ ...a.link });
      return done([], [a.link.from, a.link.to].filter((x) => hasEl(d, x)));
    case "unlink": {
      const l = d.links.find((x) => linkKey(x) === a.key);
      d.links = d.links.filter((x) => linkKey(x) !== a.key);
      return done([], l ? [l.from, l.to].filter((x) => hasEl(d, x)) : []);
    }
    case "mark":
      d.marks = d.marks.filter((m) => m.node !== a.node);
      if (a.mark) d.marks.push({ node: a.node, mark: a.mark });
      return done([], [a.node]);
    case "paste": {
      const ids = new Map<string, string>();
      const fresh: Element[] = a.els.map((src) => {
        const e: Element = structuredClone(src);
        e.id = newId(d, e.kind, e.name);
        ids.set(src.id, e.id);
        d.elements.push(e); // pushed now so the next id sees it
        if (e.x !== undefined) e.x = snap(e.x + a.dx);
        if (e.y !== undefined) e.y = snap(e.y + a.dy);
        return e;
      });
      // a frame or note target outside the paste keeps pointing at the board's own element, else lapses
      fresh.forEach((e) => {
        if (e.frame) { const m = ids.get(e.frame) ?? (hasEl(d, e.frame) ? e.frame : undefined); if (m) e.frame = m; else delete e.frame; }
        if (e.on && ids.has(e.on)) e.on = ids.get(e.on)!;
      });
      const end = (s: string) => ids.get(s) ?? s;
      for (const l of a.links) {
        const n: Link = { ...l, from: end(l.from), to: end(l.to) };
        const ok = (s: string) => !/^[a-z][a-z0-9-]*$/.test(s) || hasEl(d, s);
        if (ok(n.from) && ok(n.to) && !d.links.some((x) => linkKey(x) === linkKey(n))) d.links.push(n);
      }
      return done(fresh.map((e) => e.id));
    }
    case "scope":
      d.scope = structuredClone(a.scope);
      return done([], []);
    case "bridge":
      if (!d.bridges.some((x) => bridgeKey(x) === bridgeKey(a.bridge))) d.bridges.push({ ...a.bridge });
      return done([], [a.bridge.to].filter((x) => hasEl(d, x)));
    case "unbridge":
      d.bridges = d.bridges.filter((x) => bridgeKey(x) !== a.key);
      return done([], []);
  }
}

/** Undo history of whole-board snapshots. `limit` is the number of undo steps kept. */
// ponytail: whole-board snapshots in History; switch to patches past ~2k elements
export class History<T> {
  private s: T[] = [];
  private i = -1;
  constructor(private limit = 200) {}
  push(x: T): void {
    this.s.length = this.i + 1;
    this.s.push(x);
    if (this.s.length > this.limit + 1) this.s.shift();
    this.i = this.s.length - 1;
  }
  /** Replaces the current state: later changes of the same gesture or agent turn share one undo step. */
  amend(x: T): void {
    if (this.i < 0) this.push(x); else this.s[this.i] = x;
  }
  undo(): T | null { return this.i > 0 ? this.s[--this.i] : null; }
  redo(): T | null { return this.i < this.s.length - 1 ? this.s[++this.i] : null; }
  reset(x: T): void { this.s = [x]; this.i = 0; }
}
