import type { Show } from "../model/show";
import { detailFor, type Cam } from "./camera";
import type { Frame, MapArrow, View } from "./types";

const ROOT = "system";

export type Step = { target: string; text: string; title: string; boxes: string[]; gone: boolean };
export type Answer = { title: string; steps: Step[]; boxes: Set<string>; edges: Set<string>; numbers: Map<string, number> };
export type Aim = { id?: string; frame: Frame };

const key = (from: string, to: string) => JSON.stringify([from, to]);

function parents(view: View) {
  return new Map(view.boxes.map((b) => [b.id, b.parent]));
}

function line(up: Map<string, string | undefined>, id: string): string[] {
  const out: string[] = [], seen = new Set<string>();
  for (let p: string | undefined = id; p && p !== ROOT && !seen.has(p); p = up.get(p)) {
    seen.add(p);
    out.push(p);
  }
  return out;
}

export function answerOf(view: View, show: Show): Answer {
  const title = new Map(view.boxes.map((b) => [b.id, b.title.value]));
  const on = (id: string) => title.has(id) && id !== ROOT;
  const raw = show.steps.length ? show.steps : show.nodes.map((target) => ({ target, text: "" }));
  const numbers = new Map<string, number>();
  const steps = raw.map((s, i) => {
    const ends = s.target.split("|");
    const pair = ends.length === 2;
    const boxes = ends.filter(on);
    if (pair && boxes.length === 2) numbers.set(key(ends[0], ends[1]), i + 1);
    else if (!pair && boxes.length && !numbers.has(boxes[0])) numbers.set(boxes[0], i + 1);
    return {
      target: s.target, text: s.text, boxes,
      title: pair ? `${title.get(ends[0]) ?? ends[0]} → ${title.get(ends[1]) ?? ends[1]}` : title.get(s.target) ?? s.target,
      gone: boxes.length !== ends.length,
    };
  });
  return {
    title: show.title, steps, numbers,
    boxes: new Set(show.nodes.filter(on)),
    edges: new Set(show.edges.map(([f, t]) => key(f, t))),
  };
}

export type BoxState = { in: boolean; holds: number; under: number[] };

export function boxStates(view: View, a: Answer): Map<string, BoxState> {
  const up = parents(view), held = new Map<string, Set<string>>(), under = new Map<string, Set<number>>();
  for (const n of a.boxes) {
    const l = line(up, n);
    for (let i = 1; i < l.length; i++) (held.get(l[i]) ?? held.set(l[i], new Set()).get(l[i])!).add(l[i - 1]);
  }
  for (const [i, s] of a.steps.entries()) for (const b of s.boxes) {
    for (const p of line(up, b).slice(1)) (under.get(p) ?? under.set(p, new Set()).get(p)!).add(i + 1);
  }
  const out = new Map<string, BoxState>();
  for (const b of view.boxes) {
    const own = a.boxes.has(b.id), holds = held.get(b.id)?.size ?? 0;
    if (own || holds || line(up, b.id).some((p) => a.boxes.has(p))) out.set(b.id, { in: own, holds: own ? 0 : holds, under: own ? [] : [...(under.get(b.id) ?? [])] });
  }
  return out;
}

export function holders(view: View, id: string): string[] {
  return line(parents(view), id).slice(1);
}

export function arrowNumber(a: Answer, arrow: MapArrow): number | undefined {
  return a.numbers.get(key(arrow.from, arrow.to)) ?? arrow.parts?.map((p) => a.numbers.get(key(p.from, p.to))).find(Boolean);
}

export function arrowLit(a: Answer, arrow: MapArrow): boolean {
  return a.edges.has(key(arrow.from, arrow.to)) || !!arrow.parts?.some((p) => a.edges.has(key(p.from, p.to)));
}

export function union(frames: Frame[]): Frame {
  const x = Math.min(...frames.map((f) => f.x)), y = Math.min(...frames.map((f) => f.y));
  return { x, y, w: Math.max(...frames.map((f) => f.x + f.w)) - x, h: Math.max(...frames.map((f) => f.y + f.h)) - y };
}

export function answerFrame(a: Answer, view: View, frame: (id: string) => Frame): Aim | null {
  if (!a.boxes.size) return null;
  const up = parents(view);
  const lines = [...a.boxes].map((n) => line(up, n).reverse());
  // one box is read among its siblings, in the box holding it
  let common = lines.length === 1 ? lines[0].length - 1 : lines[0].length;
  if (lines.length > 1) for (const l of lines) {
    let i = 0;
    while (i < common && i < l.length && l[i] === lines[0][i]) i++;
    common = i;
  }
  if (common) {
    const id = lines[0][common - 1];
    return { id, frame: frame(id) };
  }
  const tops = [...new Set(lines.map((l) => l[0]))];
  return tops.length === 1 ? { id: tops[0], frame: frame(tops[0]) } : { frame: union(tops.map(frame)) };
}

export function stepFrame(a: Answer, i: number, view: View, frame: (id: string) => Frame, cam: Cam, screen: { w: number; h: number }): Aim | null {
  const s = a.steps[i];
  if (!s?.boxes.length) return null;
  const f = s.boxes.length > 1 ? union(s.boxes.map(frame)) : frame(s.boxes[0]);
  const x = f.x * cam.k + cam.x, y = f.y * cam.k + cam.y, w = f.w * cam.k, h = f.h * cam.k;
  if (x >= 0 && y >= 0 && x + w <= screen.w && y + h <= screen.h && detailFor(w) !== "far") return null;
  if (s.boxes.length > 1) return { frame: f };
  const parent = parents(view).get(s.boxes[0]);
  return parent && parent !== ROOT ? { id: parent, frame: frame(parent) } : { id: s.boxes[0], frame: f };
}
