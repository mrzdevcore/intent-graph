import type { Place } from "./places";

/** Places by key; null where there was none. */
export type Change = Record<string, Place | null>;

const same = (a: Place | undefined, b: Place | undefined) =>
  a === b || (!!a && !!b && a.x === b.x && a.y === b.y && a.w === b.w && a.h === b.h);

/** What differs from `a` to `b`, both ways; null when nothing does. */
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

/** One count for place changes and ig/ edits, so undo knows which came last. */
export class Clock {
  private n = 0;
  next(): number {
    return ++this.n;
  }
}

/** The ig/ edits made here, by stamp; the edits themselves are undone on the server. */
export class EditLog {
  private done: number[] = [];
  private undone: number[] = [];
  constructor(private clock: Clock) {}
  push(): void {
    this.done.push(this.clock.next());
    this.undone = [];
  }
  undo(): void {
    const s = this.done.pop();
    if (s !== undefined) this.undone.push(s);
  }
  redo(): void {
    const s = this.undone.pop();
    if (s !== undefined) this.done.push(s);
  }
  get last(): number | undefined {
    return this.done.at(-1);
  }
  get next(): number | undefined {
    return this.undone.at(-1);
  }
}

type Step = { stamp: number; before: Change; after: Change };

/** Place changes live only in the browser: their undo is kept here. */
export class PlaceHistory {
  private done: Step[] = [];
  private undone: Step[] = [];
  constructor(private clock: Clock) {}
  push(before: Change, after: Change): void {
    this.done.push({ stamp: this.clock.next(), before, after });
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

/** What ⌘Z undoes: the later of the last place change and the last edit; an edit (the server's
 *  journal may hold older ones) when there is no place change. */
export function undoNext(edits: EditLog, places: PlaceHistory): "edit" | "place" {
  const e = edits.last, p = places.last;
  return p !== undefined && (e === undefined || p > e) ? "place" : "edit";
}

/** What ⌘⇧Z redoes: the one undone last, which is the earlier of the two. */
export function redoNext(edits: EditLog, places: PlaceHistory): "edit" | "place" {
  const e = edits.next, p = places.next;
  return p !== undefined && (e === undefined || p < e) ? "place" : "edit";
}
