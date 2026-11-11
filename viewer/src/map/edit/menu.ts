// Which boxes can hold a box that is being moved: the add form's rules (role and `inside`). Rust's reader stays the final judge.
import type { Edit, KindView, MapArrow, MapBox } from "../types";
import { inside, kindOf } from "./add";

export type Clipped = "copy" | "copyAll" | "paste" | "duplicate" | "duplicateAll";
export type Act = "rename" | "kinds" | "details" | "arrow" | "add" | "move" | "arrange" | "code" | "delete" | Clipped;

/** A run block goes to the root or a group outside any running block; a design block inside a running block (a run block, a
 *  module, a box or group within one); a module either; a group anywhere. Never the box itself, its descendants, its current
 *  parent, a code-level box, or a box that is not code and not a group (a person, an external system). */
export function parentsFor(boxes: MapBox[], kinds: KindView[], id: string, root: string): MapBox[] {
  const byId = new Map(boxes.map((b) => [b.id, b]));
  const me = byId.get(id);
  if (!me) return [];
  const under = (x: string) => {
    for (let p: string | undefined = x, n = 0; p && n < 64; p = byId.get(p)?.parent, n++) if (p === id) return true;
    return false;
  };
  const role = kindOf(me, kinds)?.role;
  const module = me.kind.value === "module";
  return boxes
    .filter((b) => {
      if (under(b.id) || b.id === me.parent || b.level === "code" || b.kind.value === "parts") return false;
      const k = kindOf(b, kinds);
      const group = k?.role === "group";
      if (k?.code === false && !group) return false;
      if (role === "group") return true;
      const within = inside(b.id, byId, kinds);
      const runs = b.id === root || (group && !within);
      return ((role === "run" || module) && runs) || (role === "design" && within);
    })
    .sort((a, b) => a.title.value.localeCompare(b.title.value));
}

export const written = (b: MapBox | undefined) => !!b && b.level !== "code" && b.kind.value !== "parts" && b.origin !== "drift" && b.origin !== "unmanaged";

export function removal(boxes: MapBox[], arrows: MapArrow[], ids: string[]): { edit: Edit | null; inside: number } {
  const byId = new Map(boxes.map((b) => [b.id, b])), kids = new Map<string, string[]>();
  for (const b of boxes) if (b.parent && b.parent !== b.id) kids.set(b.parent, [...(kids.get(b.parent) ?? []), b.id]);
  const all = new Set<string>();
  const walk = (id: string, d: number) => {
    if (all.has(id) || d > 64) return;
    all.add(id);
    for (const k of kids.get(id) ?? []) walk(k, d + 1);
  };
  const top = ids.filter((id) => written(byId.get(id)));
  for (const id of top) walk(id, 0);
  const level = (id: string) => {
    let n = 0;
    for (let p = byId.get(id)?.parent; p && n < 64; p = byId.get(p)?.parent) n++;
    return n;
  };
  const flat = (a: MapArrow): MapArrow[] => (a.parts?.length ? a.parts.flatMap(flat) : [a]);
  const seen = new Set<string>(), edits: Edit[] = [];
  for (const a of arrows.flatMap(flat)) {
    const key = `${a.from}>${a.to}:${a.kind}`;
    if (a.kind === "bundle" || a.origin === "drift" || a.origin === "unmanaged" || seen.has(key)) continue;
    if (!(all.has(a.from) || all.has(a.to)) || !written(byId.get(a.from)) || !written(byId.get(a.to))) continue;
    seen.add(key);
    edits.push({ op: "unarrow", from: a.from, to: a.to, kind: a.kind });
  }
  const gone = [...all].filter((id) => written(byId.get(id))).map((id) => ({ id, d: level(id) })).sort((a, b) => b.d - a.d).map((x) => x.id);
  for (const id of gone) edits.push({ op: "remove", id });
  const picked = new Set(top);
  return { edit: edits.length === 0 ? null : edits.length === 1 ? edits[0] : { op: "batch", edits }, inside: gone.filter((id) => !picked.has(id)).length };
}

export function sharedParent(boxes: MapBox[], ids: string[], root: string): string | null {
  const byId = new Map(boxes.map((b) => [b.id, b])), ps = new Set(ids.map((id) => byId.get(id)?.parent ?? root));
  return ps.size === 1 ? [...ps][0] : null;
}

export function menuKeys(e: KeyboardEvent, items: HTMLElement[], onclose: () => void) {
  if (e.key === "Escape") {
    e.preventDefault();
    e.stopPropagation();
    onclose();
  } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
    e.preventDefault();
    const b = items.filter((x) => !(x as HTMLButtonElement).disabled), i = b.indexOf(document.activeElement as HTMLElement);
    b[(i + (e.key === "ArrowDown" ? 1 : b.length - 1)) % b.length]?.focus();
  }
}
