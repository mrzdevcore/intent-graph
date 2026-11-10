export type Place = { x: number; y: number; w: number; h: number };

// a box's inner space sits under its id with a slash: no box id ends with one
export const inside = (id: string) => `${id}/`;
export const isInside = (key: string) => key.endsWith("/");

type Store = Pick<Storage, "getItem" | "setItem" | "removeItem">;

const isPlace = (p: unknown): p is Place =>
  !!p && typeof p === "object" && (["x", "y", "w", "h"] as const).every((k) => Number.isFinite((p as Place)[k]));

function local(): Store | undefined {
  try {
    return globalThis.window?.localStorage;
  } catch {
    return undefined;
  }
}

export class Places {
  readonly key: string;
  private all: Record<string, Place> = {};

  constructor(readonly system: string, frames: Record<string, Place>, private store: Store | undefined = local()) {
    this.key = `ig-places:${system}`;
    const kept = this.read(this.key);
    if (kept) {
      for (const [id, p] of Object.entries(kept)) if (isPlace(p)) this.all[id] = { x: p.x, y: p.y, w: p.w, h: p.h };
      return;
    }
    const old = `ig-moves:${system}`, moves = this.read(old);
    if (!moves) return;
    for (const [id, d] of Object.entries(moves)) {
      const f = frames[id], m = d as { x: number; y: number } | null;
      if (f && Number.isFinite(m?.x) && Number.isFinite(m?.y)) this.all[id] = { ...f, x: f.x + m!.x, y: f.y + m!.y };
    }
    this.save();
    try {
      this.store?.removeItem(old);
    } catch { /* private mode */ }
  }

  get size(): number {
    return Object.keys(this.all).length;
  }
  get(id: string): Place | undefined {
    return this.all[id];
  }
  set(id: string, p: Place): void {
    this.all = { ...this.all, [id]: { ...p } };
    this.save();
  }
  forget(ids: string[]): void {
    const all = { ...this.all };
    for (const id of ids) delete all[id];
    this.all = all;
    this.save();
  }
  replace(all: Record<string, Place>): void {
    this.all = { ...all };
    this.save();
  }
  snapshot(): Record<string, Place> {
    return { ...this.all };
  }

  private read(key: string): Record<string, unknown> | null {
    try {
      const v = JSON.parse(this.store?.getItem(key) ?? "null");
      return v && typeof v === "object" && !Array.isArray(v) ? v : null;
    } catch {
      return null;
    }
  }
  private save() {
    try {
      if (this.size) this.store?.setItem(this.key, JSON.stringify(this.all));
      else this.store?.removeItem(this.key);
    } catch { /* private mode: places last the page */ }
  }
}
