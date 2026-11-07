import type { ArrowRef, BoxRef, Diff } from "../model/diff";
import { key, line, parents, ROOT, type Answer, type DiffKind, type Step } from "./answer";
import type { MapArrow, View } from "./types";

export type Marks = {
  boxes: Map<string, "changed" | "added">;
  files: Map<string, number>;
  removedHere: Map<string, BoxRef[]>;
  addedArrows: Set<string>;
  ghostIds: Set<string>;
};

const plural = (n: number, one: string, many = one + "es") => `${n} ${n === 1 ? one : many}`;
export const files = (n: number) => `${n} ${n === 1 ? "file" : "files"}`;

function names(titles: string[], max = 5): string {
  const shown = titles.slice(0, max).join(", ");
  return titles.length > max ? `${shown} and ${titles.length - max} more` : shown;
}

function bySize<T>(groups: Map<string, T[]>): [string, T[]][] {
  return [...groups].sort((a, b) => b[1].length - a[1].length);
}

export function diffAnswer(view: View, d: Diff): { answer: Answer; marks: Marks; view: View } {
  const byId = new Map(view.boxes.map((b) => [b.id, b]));
  const on = (id: string) => byId.has(id) && id !== ROOT;
  const up = parents(view);
  const named = new Map<string, BoxRef>([...d.boxes.changed, ...d.boxes.added, ...d.boxes.removed].map((b) => [b.id, b]));
  const upOf = (id: string) => (byId.has(id) ? up.get(id) : named.get(id)?.parent ?? undefined);
  const ancestors = (id: string) => {
    const out: string[] = [];
    for (let p = upOf(id); p && p !== ROOT && !out.includes(p); p = upOf(p)) out.push(p);
    return out;
  };
  const nearest = (id: string) => ancestors(id).find(on) ?? ROOT;
  const title = (id: string) => byId.get(id)?.title.value ?? named.get(id)?.title ?? id;

  const marks: Marks = { boxes: new Map(), files: new Map(), removedHere: new Map(), addedArrows: new Set(), ghostIds: new Set() };
  for (const c of d.boxes.changed) if (on(c.id)) { marks.boxes.set(c.id, "changed"); marks.files.set(c.id, c.filesChanged); }
  for (const a of d.boxes.added) if (on(a.id)) marks.boxes.set(a.id, "added");
  for (const r of d.boxes.removed) {
    const at = nearest(r.id);
    marks.removedHere.set(at, [...(marks.removedHere.get(at) ?? []), r]);
  }

  const steps: Step[] = [];
  const step = (group: string, kind: DiffKind, target: string, title: string, boxes: string[], meta?: string) =>
    steps.push({ target, text: "", title, boxes: [...new Set(boxes)], gone: false, group, kind, ...(meta ? { meta } : {}) });

  const i = d.intent;
  if (i.status === "changed") {
    const ends = (as: ArrowRef[]) => as.flatMap((a) => [a.from, a.to]).filter(on);
    if (i.added.length) step("ig/", "added", "ig/added", `${plural(i.added.length, "box")} declared`, i.added.map((b) => b.id).filter(on), names(i.added.map((b) => b.title)));
    if (i.removed.length) step("ig/", "removed", "ig/removed", `${plural(i.removed.length, "box")} dropped`, i.removed.map((b) => nearest(b.id)).filter(on), names(i.removed.map((b) => b.title)));
    if (i.moved.length) step("ig/", "changed", "ig/moved", `${plural(i.moved.length, "box")} moved`, i.moved.map((m) => m.id).filter(on), names(i.moved.map((m) => title(m.id))));
    if (i.changed.length) step("ig/", "changed", "ig/changed", `${plural(i.changed.length, "box")} reworded`, i.changed.map((c) => c.id).filter(on), names([...new Set(i.changed.flatMap((c) => c.fields))]));
    if (i.arrowsAdded.length || i.arrowsRemoved.length) step("ig/", "changed", "ig/arrows", "Arrows", ends([...i.arrowsAdded, ...i.arrowsRemoved]), `${i.arrowsAdded.length} declared, ${i.arrowsRemoved.length} dropped`);
  } else if (i.status !== "same") {
    const kind: DiffKind = i.status === "added" ? "added" : i.status === "removed" ? "removed" : "changed";
    step("ig/", kind, "ig/", i.note ?? `ig/ ${i.status}`, []);
  }

  for (const p of d.problems.new) step("New problems", "problem", `problem/${steps.length}`, p.text, p.boxes.filter(on));

  const touched = new Set([...d.boxes.changed, ...d.boxes.added, ...d.boxes.removed].map((b) => b.id));
  const owner = (id: string) => ancestors(id).reverse().find((a) => touched.has(a));
  const folded = new Map<string, { title: string; sign: string }[]>();
  for (const [sign, bs] of [["+", d.boxes.added], ["−", d.boxes.removed]] as const) for (const b of bs) {
    const o = owner(b.id);
    if (o) folded.set(o, [...(folded.get(o) ?? []), { title: b.title, sign }]);
  }
  const extra = (id: string) => (folded.get(id) ?? []).map((f) => `${f.sign} ${f.title}`).join(", ");
  const meta = (...parts: string[]) => parts.filter(Boolean).join(" · ") || undefined;
  const own = <T extends BoxRef>(bs: T[]) => bs.filter((b) => b.id !== ROOT && !owner(b.id));
  for (const c of own(d.boxes.changed).sort((a, b) => b.filesChanged - a.filesChanged))
    step("Boxes", "changed", c.id, c.title, [on(c.id) ? c.id : nearest(c.id)], meta(files(c.filesChanged), extra(c.id)));
  for (const a of own(d.boxes.added)) step("Boxes", "added", a.id, a.title, [on(a.id) ? a.id : nearest(a.id)], meta(extra(a.id)));
  for (const r of own(d.boxes.removed)) step("Boxes", "removed", r.id, r.title, [nearest(r.id)].filter(on), meta(extra(r.id)));

  const arrowSteps = (as: ArrowRef[], kind: DiffKind) => {
    const groups = new Map<string, ArrowRef[]>();
    for (const a of as) groups.set(a.from, [...(groups.get(a.from) ?? []), a]);
    for (const [from, g] of bySize(groups)) {
      const one = g.length === 1;
      step("Arrows", kind, `arrows/${kind}/${from}`, one ? `${g[0].fromTitle} → ${g[0].toTitle}` : `${g[0].fromTitle} → ${g.length} boxes`,
        [from, ...g.map((a) => a.to)].filter(on), one ? undefined : names(g.map((a) => a.toTitle)));
    }
  };
  arrowSteps(d.arrows.added, "added");
  arrowSteps(d.arrows.removed, "removed");

  const drawnAs = (from: string, to: string) =>
    view.arrows.find((a) => (a.from === from && a.to === to) || a.parts?.some((p) => p.from === from && p.to === to));
  for (const a of d.arrows.added) {
    const v = drawnAs(a.from, a.to);
    if (v) marks.addedArrows.add(key(v.from, v.to));
  }

  const drawn = new Set(view.arrows.map((a) => key(a.from, a.to)));
  const lifted = new Map<string, MapArrow>();
  for (const a of d.arrows.removed) {
    if (!on(a.from) || !on(a.to)) continue;
    const lf = [a.from, ...line(up, a.from).slice(1)].reverse(), lt = [a.to, ...line(up, a.to).slice(1)].reverse();
    let k = 0;
    while (k < lf.length && k < lt.length && lf[k] === lt[k]) k++;
    const from = lf[k], to = lt[k];
    if (!from || !to || from === to || drawn.has(key(from, to))) continue;
    const kind = a.kind as MapArrow["kind"];
    const parent = k ? lf[k - 1] : ROOT;
    const written: MapArrow = { id: `${a.from}->${a.to}`, parent, from: a.from, to: a.to, kind, count: 1, kinds: [kind] };
    const g = lifted.get(key(from, to));
    if (g) g.parts = [...(g.parts ?? []), written];
    else lifted.set(key(from, to), { ...written, id: `ghost:${from}|${to}`, from, to, ghost: true, parts: [written] });
  }
  const ghosts = [...lifted.values()];
  for (const g of ghosts) marks.ghostIds.add(g.id);

  const numbers = new Map<string, number>();
  steps.forEach((s, n) => s.boxes.length === 1 && !numbers.has(s.boxes[0]) && numbers.set(s.boxes[0], n + 1));
  const answer: Answer = {
    title: "What this change does",
    steps,
    numbers,
    boxes: new Set([...marks.boxes.keys(), ...[...marks.removedHere.keys()].filter(on), ...steps.flatMap((s) => s.boxes)]),
    edges: new Set([...marks.addedArrows, ...ghosts.map((g) => key(g.from, g.to))]),
  };
  return { answer, marks, view: ghosts.length ? { ...view, arrows: [...view.arrows, ...ghosts] } : view };
}

const short = (rev: string) => (/^[0-9a-f]{40}$/.test(rev) ? rev.slice(0, 7) : rev);

export function diffLines(d: Diff): { sub: string; note: string } {
  const quiet = [
    d.intent.status === "same" ? "ig/ unchanged" : "",
    d.problems.new.length ? "" : "no new problems",
  ].filter(Boolean);
  return { sub: `${short(d.base)} → ${d.head ? short(d.head) : "working tree"} · ${d.summary}`, note: quiet.join(" · ") };
}
