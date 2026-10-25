// Focus mode: the neighbourhood of one node, `depth` edges away in either direction.

import { topMember, type Index } from "./graph";

export function focusIds(ix: Index, focus: string | string[], depth: number): Record<string, boolean> {
  const roots = ([] as string[]).concat(focus);
  const ids: Record<string, boolean> = Object.fromEntries(roots.map((r) => [r, true]));
  let frontier = roots;
  for (let d = 0; d < depth; d++) {
    const next: string[] = [];
    ix.D.edges.forEach((e) => {
      frontier.forEach((f) => {
        if (e.from === f && !ids[e.to]) {
          ids[e.to] = true;
          next.push(e.to);
        }
        if (e.to === f && !ids[e.from]) {
          ids[e.from] = true;
          next.push(e.from);
        }
      });
    });
    frontier = next;
  }
  // A module in focus brings its members' neighbours too.
  roots.forEach((r) => {
    const n = ix.byId[r];
    if (n && n.module)
      ix.modules[r].members.forEach((m) => {
        if (topMember(ix, m) === m) ids[m.id] = true;
      });
  });
  return ids;
}
