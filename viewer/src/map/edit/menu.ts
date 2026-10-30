// Which boxes can hold a box that is being moved. Rust's reader stays the final judge.
import type { MapBox } from "../types";
/** What the box menu offers. */
export type Act = "rename" | "move" | "add" | "arrow" | "delete";
import { NO_CODE, SYSTEM_KINDS } from "./add";

/** A system-level kind goes only under the system root; any other kind under a system-level box that holds
 *  code or under a container. Never the box itself, its descendants or its current parent. */
export function parentsFor(boxes: MapBox[], id: string, root: string): MapBox[] {
  const byId = new Map(boxes.map((b) => [b.id, b]));
  const me = byId.get(id);
  if (!me) return [];
  const under = (x: string) => {
    for (let p: string | undefined = x, n = 0; p && n < 64; p = byId.get(p)?.parent, n++) if (p === id) return true;
    return false;
  };
  const top = SYSTEM_KINDS.includes(me.kind.value);
  return boxes
    .filter((b) => {
      if (under(b.id) || b.id === me.parent || b.level === "code") return false;
      if (top) return b.id === root;
      const k = b.kind.value;
      return b.id !== root && !NO_CODE.includes(k) && k !== "parts";
    })
    .sort((a, b) => a.title.value.localeCompare(b.title.value));
}
