import type { Edit } from "../types";
import type { Place } from "./places";

export type Change = Record<string, Place | null>;
export type Rename = [from: string, to: string];

const same = (a: Place | undefined, b: Place | undefined) =>
  a === b || (!!a && !!b && a.x === b.x && a.y === b.y && a.w === b.w && a.h === b.h);

export function changes(a: Record<string, Place>, b: Record<string, Place>): { before: Change; after: Change } | null {
  const before: Change = {}, after: Change = {};
  let any = false;
  for (const k of new Set([...Object.keys(a), ...Object.keys(b)])) {
    if (same(a[k], b[k])) continue;
    before[k] = a[k] ?? null;
    after[k] = b[k] ?? null;
    any = true;
  }
  return any ? { before, after } : null;
}

export function apply(places: Record<string, Place>, c: Change): Record<string, Place> {
  const out = { ...places };
  for (const [k, p] of Object.entries(c)) {
    if (p) out[k] = p;
    else delete out[k];
  }
  return out;
}

export const renamesOf = (edits: Edit[]): Rename[] =>
  edits.flatMap((e): Rename[] => (e.op === "rename" ? [[e.id, e.to]] : e.op === "batch" ? renamesOf(e.edits) : []));

export class Clock {
  private n = 0;
  next(): number {
    return ++this.n;
  }
  get now(): number {
    return this.n;
  }
}

type Done = { stamp: number; renames: Rename[] };

// only edits applied here are stamped: keep and drop are not on the server's undo
export class EditLog {
  private done: Done[] = [];
  private undone: Done[] = [];
  constructor(private clock: Clock) {}
  push(renames: Rename[] = []): void {
    this.done.push({ stamp: this.clock.next(), renames });
    this.undone = [];
  }
  undo(): Rename[] {
    const d = this.done.pop();
    if (!d) return [];
    this.undone.push(d);
    return d.renames.map(([a, b]): Rename => [b, a]).reverse();
  }
  redo(): Rename[] {
    const d = this.undone.pop();
    if (!d) return [];
    this.done.push(d);
    return d.renames;
  }
  get last(): number | undefined {
    return this.done.at(-1)?.stamp;
  }
  get next(): number | undefined {
    return this.undone.at(-1)?.stamp;
  }
}

type Step = { stamp: number; before: Change; after: Change };

export class PlaceHistory {
  private done: Step[] = [];
  private undone: Step[] = [];
  constructor(private clock: Clock) {}
  push(before: Change, after: Change): void {
    this.done.push({ stamp: this.clock.next(), before, after });
    this.undone = [];
  }
  merge(before: Change, after: Change): void {
    const top = this.done.at(-1);
    if (!top || top.stamp !== this.clock.now) return this.push(before, after);
    top.before = { ...before, ...top.before };
    top.after = { ...top.after, ...after };
    this.undone = [];
  }
  undo(): Change | null {
    const s = this.done.pop();
    if (!s) return null;
    this.undone.push(s);
    return s.before;
  }
  redo(): Change | null {
    const s = this.undone.pop();
    if (!s) return null;
    this.done.push(s);
    return s.after;
  }
  get last(): number | undefined {
    return this.done.at(-1)?.stamp;
  }
  get next(): number | undefined {
    return this.undone.at(-1)?.stamp;
  }
}

/** An edit when there is no place change: the server's journal may hold edits from before this page. */
export function undoNext(edits: EditLog, places: PlaceHistory): "edit" | "place" {
  const e = edits.last, p = places.last;
  return p !== undefined && (e === undefined || p > e) ? "place" : "edit";
}

// the one undone last has the earlier stamp
export function redoNext(edits: EditLog, places: PlaceHistory): "edit" | "place" {
  const e = edits.next, p = places.next;
  return p !== undefined && (e === undefined || p < e) ? "place" : "edit";
}
