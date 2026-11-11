import type { KindView, MapBox } from "../types";

export type Ranked = { best: KindView | null; groups: { title: string; kinds: KindView[] }[] };

const sectionsOf = (kinds: KindView[], inRun: boolean) => [
  { title: "Runs", kinds: kinds.filter((k) => !k.custom && k.role === "run") },
  ...(inRun ? [{ title: "Inside a run", kinds: kinds.filter((k) => !k.custom && k.role === "design") }] : []),
  { title: "Your team's kinds", kinds: kinds.filter((k) => k.custom && k.role !== "root") },
  { title: "Grouping", kinds: kinds.filter((k) => !k.custom && k.role === "group") },
];

function score(k: KindView, q: string): number {
  const name = k.name.toLowerCase(), title = k.title.toLowerCase();
  if (name.startsWith(q) || title.startsWith(q)) return 0;
  if (title.split(/[^a-z0-9]+/).some((w) => w.startsWith(q))) return 1;
  if (name.includes(q) || title.includes(q)) return 2;
  if (k.means.toLowerCase().includes(q)) return 3;
  return -1;
}

export function rankKinds(q: string, kinds: KindView[], context: { parentKinds: string[]; inRun: boolean }): Ranked {
  const sections = sectionsOf(kinds, context.inRun), order = sections.flatMap((s) => s.kinds), text = q.trim().toLowerCase();
  let best: KindView | null = null;
  let keep = (_: KindView) => true;
  if (text) {
    const scores = new Map(order.map((k) => [k, score(k, text)]));
    keep = (k) => scores.get(k)! >= 0;
    for (const k of order) if (keep(k) && (!best || scores.get(k)! < scores.get(best)!)) best = k;
  } else {
    const used = new Map<string, number>();
    for (const n of context.parentKinds) used.set(n, (used.get(n) ?? 0) + 1);
    for (const k of order) if ((used.get(k.name) ?? 0) > (best ? used.get(best.name)! : 0)) best = k;
  }
  const groups = sections.map((s) => ({ title: s.title, kinds: s.kinds.filter((k) => k !== best && keep(k)) })).filter((s) => s.kinds.length);
  return { best, groups };
}

export const kindName = (b: MapBox) => b.custom ?? b.kind.value;

export function mostUsed(boxes: MapBox[], kinds: KindView[], inRun: boolean, n = 5): KindView[] {
  const order = sectionsOf(kinds, inRun).flatMap((s) => s.kinds), used = new Map<string, number>();
  for (const b of boxes) if (b.level !== "code") used.set(kindName(b), (used.get(kindName(b)) ?? 0) + 1);
  return order.map((k, i) => ({ k, i, n: used.get(k.name) ?? 0 })).sort((a, b) => b.n - a.n || a.i - b.i).slice(0, n).map((x) => x.k);
}
export const NO_KIND = "no kind yet";
