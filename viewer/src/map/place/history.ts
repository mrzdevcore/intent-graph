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
  private redos: (() => void)[] = [];
  // anything new done on this page: the server's redo may still hold entries undone before it
  dirty = false;
  next(): number {
    return ++this.n;
  }
  get now(): number {
    return this.n;
  }
  onWork(clear: () => void): void {
    this.redos.push(clear);
  }
  work(): void {
    this.dirty = true;
    for (const clear of this.redos) clear();
  }
}

type Done = { stamp: number; renames: Rename[] };
export type Moved = { renames: Rename[]; failed(): void };

// only edits applied here are stamped: keep and drop are not on the server's undo
export class EditLog {
  private done: Done[] = [];
  private undone: Done[] = [];
  constructor(private clock: Clock) {
    clock.onWork(() => (this.undone = []));
  }
  begin(): number {
    this.clock.work();
    return this.clock.next();
  }
  push(renames: Rename[] = [], stamp = this.begin()): void {
    this.done.push({ stamp, renames });
  }
  // an undo with nothing done here undoes an entry from before this page, older than every stamp
  undo(): Moved {
    const d = this.done.pop() ?? { stamp: 0, renames: [] };
    this.undone.push(d);
    return {
      renames: d.renames.map(([a, b]): Rename => [b, a]).reverse(),
      failed: () => {
        this.undone = this.undone.filter((x) => x !== d);
        if (d.stamp) this.done.push(d);
      },
    };
  }
  redo(): Moved {
    const d = this.undone.pop();
    if (d) this.done.push(d);
    return { renames: d?.renames ?? [], failed: () => void (this.done = this.done.filter((x) => x !== d)) };
  }
  get last(): number | undefined {
    return this.done.at(-1)?.stamp;
  }
  get next(): number | undefined {
    return this.undone.at(-1)?.stamp;
  }
  get redoable(): boolean {
    return this.undone.length > 0 || !this.clock.dirty;
  }
}

type Step = { stamp: number; before: Change; after: Change };

export class PlaceHistory {
  private done: Step[] = [];
  private undone: Step[] = [];
  constructor(private clock: Clock) {
    clock.onWork(() => (this.undone = []));
  }
  push(before: Change, after: Change): void {
    this.clock.work();
    this.done.push({ stamp: this.clock.next(), before, after });
  }
  merge(before: Change, after: Change): void {
    const top = this.done.at(-1);
    if (!top || top.stamp !== this.clock.now) return this.push(before, after);
    top.before = { ...before, ...top.before };
    top.after = { ...top.after, ...after };
    this.clock.work();
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

export function undoNext(edits: EditLog, places: PlaceHistory): "edit" | "place" {
  const e = edits.last, p = places.last;
  return p !== undefined && (e === undefined || p > e) ? "place" : "edit";
}

// the one undone last has the earlier stamp; nothing once new work ended the redo line
export function redoNext(edits: EditLog, places: PlaceHistory): "edit" | "place" | null {
  const e = edits.next, p = places.next;
  if (p !== undefined && (e === undefined || p < e)) return "place";
  return edits.redoable ? "edit" : null;
}
