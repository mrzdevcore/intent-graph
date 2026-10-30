// Proposals: the journal's edits nobody kept yet (Claude's, the AI reading's), as the map shows them.
import type { Proposal, View } from "../types";
import type { Trace } from "../search";

const ROOT = "system";

/** Edited box and arrow id (not a move's destination) -> who proposed it (the first proposal wins). */
export function proposed(ps: Proposal[] | undefined): Map<string, Proposal["by"]> {
  const out = new Map<string, Proposal["by"]>();
  for (const p of ps ?? []) for (const id of p.marks) if (!out.has(id)) out.set(id, p.by);
  return out;
}

/** "Claude proposed 1 edit · AI reading 2": only the authors that have entries. */
export function barText(ps: Proposal[]): string {
  const n = (by: Proposal["by"]) => ps.filter((p) => p.by === by).length;
  const c = n("claude"), r = n("reading");
  return [c && `Claude proposed ${c} ${c === 1 ? "edit" : "edits"}`, r && `AI reading ${r}`].filter(Boolean).join(" · ");
}

/** A proposal's ids that exist on the map, as the path search lights; null when none does.
 *  Only the first box's level lights (a trace has one parent). */
export function lit(view: View, ids: string[]): Trace | null {
  const boxes = view.boxes.filter((b) => ids.includes(b.id));
  const arrows = view.arrows.filter((a) => ids.includes(a.id));
  const parent = boxes[0] ? boxes[0].parent ?? ROOT : arrows[0]?.parent;
  return parent ? { parent, boxes: boxes.map((b) => b.id), arrows, reversed: false } : null;
}
