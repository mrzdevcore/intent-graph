// Which boxes can hold a box that is being moved: the add form's rules (role and `inside`). Rust's reader stays the final judge.
import type { KindView, MapBox } from "../types";
/** What the box menu offers. */
export type Act = "rename" | "move" | "add" | "arrow" | "arrange" | "delete";
import { inside, kindOf } from "./add";

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
