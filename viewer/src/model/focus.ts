// Focus mode: the neighbourhood of one node, `depth` edges away in either direction.

import { topMember, type Index } from "./graph";

export function focusIds(ix: Index, focusId: string, depth: number): Record<string, boolean> {
  const ids: Record<string, boolean> = { [focusId]: true };
  let frontier = [focusId];
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
  const n = ix.byId[focusId];
  if (n && n.module)
    ix.modules[focusId].members.forEach((m) => {
      if (topMember(ix, m) === m) ids[m.id] = true;
    });
  return ids;
}
