import type { Frame, MapArrow } from "../types";
import type { Wire } from "../wires";
import { specKind } from "./arrows";
import type { EditSession } from "./session.svelte";

type Cam = { x: number; y: number; k: number };
export type Geo = { cam: Cam; inside: (parent: string) => { x: number; y: number; s: number }; frame: (id: string) => Frame; wire: (a: MapArrow) => Wire | undefined };

export const arrowHit = (e: Event) => {
  const t = e.target as Element | null;
  return t?.closest?.("[data-hit]") ? (t.closest("[data-arrow]") as HTMLElement | null)?.dataset.arrow : undefined;
};

const centre = (g: Geo, id: string) => {
  const f = g.frame(id), c = g.cam;
  return { x: (f.x + f.w / 2) * c.k + c.x, y: (f.y + f.h / 2) * c.k + c.y };
};
const between = (g: Geo, from: string, to: string) => {
  const [f, t] = [centre(g, from), centre(g, to)];
  return { x: (f.x + t.x) / 2, y: (f.y + t.y) / 2 };
};

export function arrowBar(s: EditSession, arrows: MapArrow[], g: Geo, top: number) {
  const p = s.arrow, drawn = p && arrows.find((x) => x.id === p.id), a = s.picked;
  if (!p || !drawn || !a) return null;
  const w = p.part ? undefined : g.wire(drawn), t = w && g.inside(drawn.parent);
  const at = p.part ? between(g, p.part.from, p.part.to) : w && t ? { x: (t.x + w.mid[0] * t.s) * g.cam.k + g.cam.x, y: (t.y + w.mid[1] * t.s) * g.cam.k + g.cam.y } : between(g, a.from, a.to);
  const how = s.editing, title = `${s.titleOf(a.from)} → ${s.titleOf(a.to)}`;
  const parts = how === "bundle" ? (a.parts ?? []).map((q) => ({ title: `${s.titleOf(q.from)} → ${s.titleOf(q.to)}`, kind: specKind(q) })) : [];
  return { x: at.x, y: Math.max(top, at.y), how, title, a, parts, bundle: drawn };
}

export function partFrame(p: MapArrow, frame: (id: string) => Frame): Frame {
  const [f, t] = [frame(p.from), frame(p.to)], x = Math.min(f.x, t.x), y = Math.min(f.y, t.y);
  return { x, y, w: Math.max(f.x + f.w, t.x + t.w) - x, h: Math.max(f.y + f.h, t.y + t.h) - y };
}
