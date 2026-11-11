// The state of one editing gesture on the map: the arrow being drawn, the box being drawn (its place in its parent's
// inner units, kept as places are), the new arrow's kind chip, and the one popover (box menu, rename, move picker) or add form that is open. Map.svelte hands it what it knows through `Ctx`.
import type { EditApi } from "../../api";
import type { ArrowKind, ArrowKindView, Edit, Frame, KindView, MapBox, View } from "../types";
import { baseOf, guessArrow } from "./guess";
import { removal, type Act } from "./menu";
import type { Tool } from "./tool.svelte";

export type Draft = { parent: string; x: number; y: number; w: number; h: number; title: string; drawing: boolean; from?: string };
export type Ui = { t: "menu" | "rename" | "move" | "ground"; id: string; x: number; y: number; gx?: number; gy?: number };
export type Ctx = {
  edit: () => EditApi | undefined; view: () => View; box: (id: string) => MapBox | undefined; frame: (id: string) => Frame;
  kinds: () => KindView[]; arrowKinds: () => ArrowKindView[] | undefined;
  cam: () => { x: number; y: number; k: number }; stage: () => HTMLElement;
  selected: () => string | null; select: (id: string | null) => void; tool: (t: Tool) => void; used: (shift: boolean) => void;
  run: (p: () => Promise<View>) => boolean;
  arrange: (scope: string[]) => void;
  drop: (e: PointerEvent, from: string) => void;
  draftFrame: () => Frame | undefined;
  ground: (e: MouseEvent) => { parent: string; x: number; y: number };
  draftAt: (g: { parent: string; x: number; y: number }) => void;
  details: (id: string) => void;
  open: (id: string) => void;
};
type Pt = { x: number; y: number };
const sides = (f: Frame): Pt[] => [{ x: f.x, y: f.y + f.h / 2 }, { x: f.x + f.w, y: f.y + f.h / 2 }, { x: f.x + f.w / 2, y: f.y }, { x: f.x + f.w / 2, y: f.y + f.h }];
const nearest = (f: Frame, p: Pt) => sides(f).reduce((a, b) => (Math.hypot(b.x - p.x, b.y - p.y) < Math.hypot(a.x - p.x, a.y - p.y) ? b : a));
const centre = (f: Frame): Pt => ({ x: f.x + f.w / 2, y: f.y + f.h / 2 });

export class EditSession {
  link = $state<{ from: string; to: string | null; x: number; y: number; x0: number; y0: number; armed: boolean } | null>(null);
  ui = $state<Ui | null>(null);
  made = $state<{ from: string; to: string; kind: ArrowKind; open: boolean } | null>(null);
  adding = $state<{ parent: string; x: number; y: number } | null>(null);
  draft = $state<Draft | null>(null);
  kinds = $state<string[] | null>(null);
  asking = $state<{ ids: string[]; text: string; edit: Edit } | null>(null);
  private c: Ctx;
  private pid = -1;
  constructor(c: Ctx) { this.c = c; }

  rel = (e: { clientX: number; clientY: number }) => {
    const r = this.c.stage().getBoundingClientRect();
    return { x: e.clientX - r.left, y: e.clientY - r.top };
  };
  titleOf = (id: string) => this.c.box(id)?.title.value ?? id;
  /** The box's top-left on screen, and its width. */
  at = (id: string) => {
    const f = this.c.frame(id), m = this.c.cam();
    return { x: f.x * m.k + m.x, y: f.y * m.k + m.y };
  };
  width = (id: string) => this.c.frame(id).w * this.c.cam().k;
  /** The box lit as the arrow's target. */
  get aim() { return this.link?.to ?? null; }
  private screen(f: Frame): Frame {
    const m = this.c.cam();
    return { x: f.x * m.k + m.x, y: f.y * m.k + m.y, w: f.w * m.k, h: f.h * m.k };
  }
  private span(a: Frame, b: Frame) {
    const p = nearest(a, centre(b)), q = nearest(b, p);
    return { x1: p.x, y1: p.y, x2: q.x, y2: q.y };
  }
  get band() {
    const l = this.link;
    if (l) {
      const a = this.screen(this.c.frame(l.from));
      const end = l.to ? nearest(this.screen(this.c.frame(l.to)), l) : { x: l.x, y: l.y }, p = nearest(a, end);
      return { x1: p.x, y1: p.y, x2: end.x, y2: end.y };
    }
    const d = this.draft, f = d?.from && this.c.box(d.from) ? this.c.draftFrame() : undefined;
    return f && d?.from ? this.span(this.screen(this.c.frame(d.from)), this.screen(f)) : undefined;
  }
  get chip() {
    const m = this.made;
    if (!m || !this.c.box(m.from) || !this.c.box(m.to)) return undefined;
    const s = this.span(this.screen(this.c.frame(m.from)), this.screen(this.c.frame(m.to)));
    return { x: (s.x1 + s.x2) / 2, y: (s.y1 + s.y2) / 2 };
  }

  focusBox(id: string) {
    (this.c.stage().querySelector(`[data-box="${CSS.escape(id)}"] > button`) as HTMLElement | null)?.focus({ preventScroll: true });
  }
  /** Close the popover; focus goes back to its box unless the pointer went elsewhere. */
  shut(refocus = true) {
    const u = this.ui;
    this.ui = null;
    if (refocus && u) this.focusBox(u.id);
  }
  /** Drop everything in flight (Esc, a box gone). */
  cancel() {
    this.link = this.ui = this.adding = this.draft = this.made = this.kinds = this.asking = null;
    this.release();
  }
  /** A lost pointer drops the arrow in flight; open forms keep what was typed. */
  dropLink() {
    this.link = null;
    this.release();
  }
  /** A pointer press outside the open popover closes it. */
  outside(e: Event) {
    const t = e.target as Element | null;
    if (t?.closest?.("[data-popover]")) return;
    this.ui = this.made = null;
    if (!t?.closest?.("[data-bar]")) this.kinds = this.asking = null;
  }
  grab(id: number) {
    this.pid = id;
    try { this.c.stage().setPointerCapture?.(id); } catch { /* a synthetic pointer */ }
  }
  release() {
    try { if (this.pid >= 0) this.c.stage().releasePointerCapture?.(this.pid); } catch { /* already gone */ }
    this.pid = -1;
  }

  private boxAt(e: PointerEvent) {
    // captured pointers report the stage as their target: ask the page what is under them
    const hit = document.elementFromPoint?.(e.clientX, e.clientY), s = this.c.stage();
    const el = hit && hit !== s && s.contains(hit) ? hit : (e.target as Element | null);
    return (el?.closest?.("[data-box]:not([data-open])") as HTMLElement | null)?.dataset.box;
  }
  startLink(e: PointerEvent, armed: boolean): boolean {
    const t = e.target as Element | null, from = (t?.closest?.("[data-box]") as HTMLElement | null)?.dataset.box;
    if (!this.c.edit() || !from || !this.c.box(from) || !(armed || t?.closest?.("[data-handle]"))) return false;
    const p = this.rel(e);
    this.made = null;
    this.link = { from, to: null, ...p, x0: p.x, y0: p.y, armed };
    this.grab(e.pointerId);
    return true;
  }
  moveLink(e: PointerEvent): boolean {
    if (!this.link) return false;
    const to = this.boxAt(e);
    this.link = { ...this.link, to: to && to !== this.link.from && this.c.box(to) ? to : null, ...this.rel(e) };
    return true;
  }
  endLink(e: PointerEvent): boolean {
    const l = this.link;
    if (!l) return false;
    this.link = null;
    this.release();
    const p = this.rel(e), moved = Math.hypot(p.x - l.x0, p.y - l.y0) >= 4;
    const to = moved ? this.boxAt(e) : l.from;
    if (to && to !== l.from && this.c.box(to)) this.connect(l.from, to);
    else if (!to && moved) this.c.drop(e, l.from);
    else return true;
    if (l.armed) this.c.used(e.shiftKey);
    return true;
  }
  // a null end is the draft's box, of kind "box"
  guess(from: string, to: string | null): ArrowKind {
    const kinds = this.c.kinds(), kind = (id: string | null) => (id && this.c.box(id) ? baseOf(this.c.box(id)!.kind.value, kinds) : "box");
    return guessArrow(kind(from), kind(to), this.c.arrowKinds() ?? []);
  }
  connect(from: string, to: string) {
    if (!this.c.edit() || from === to || !this.c.box(from) || !this.c.box(to)) return;
    this.send({ op: "arrow", from, to, kind: this.guess(from, to), label: null });
  }
  send(e: Edit, open = true, failed?: () => void): boolean {
    const a = (e.op === "batch" ? e.edits : [e]).find((x) => x.op === "arrow");
    const made = a?.op === "arrow" ? { from: a.from, to: a.to, kind: a.kind, open } : null;
    const sent = this.c.run(() => this.c.edit()!.apply(e).catch((err) => {
      if (made && this.made?.from === made.from && this.made.to === made.to) this.made = null;
      failed?.();
      throw err;
    }));
    if (sent && made) this.made = made;
    return sent;
  }
  chipOpen(open: boolean) {
    if (this.made) this.made = { ...this.made, open };
  }
  rekind(kind: ArrowKind) {
    const m = this.made;
    if (!m) return;
    this.made = { ...m, open: false };
    if (m.kind === kind) return;
    const sent = this.c.run(() => this.c.edit()!.apply({ op: "arrowset", from: m.from, to: m.to, key: "kind", value: kind }).catch((err) => {
      if (this.made?.from === m.from && this.made.to === m.to) this.made = { ...this.made, kind: m.kind };
      throw err;
    }));
    if (sent) this.made = { ...m, kind, open: false };
  }
  confirm(d: Draft, id: string, title: string): Edit {
    const add: Edit = { op: "add", parent: d.parent, kind: "box", id, title, at: [], before: null };
    if (!d.from || !this.c.box(d.from)) return add;
    return { op: "batch", edits: [add, { op: "arrow", from: d.from, to: id, kind: this.guess(d.from, null), label: null }] };
  }

  act(a: Act, id: string, p = this.at(id)) {
    this.ui = null;
    if (a === "rename" || a === "move") this.ui = { t: a, id, ...p };
    else if (a === "add") this.adding = { parent: id, ...p };
    else if (a === "arrow") {
      this.c.select(id);
      this.c.tool("arrow");
    } else if (a === "arrange") this.c.arrange([id]);
    else if (a === "kinds") {
      this.c.select(id);
      this.kinds = [id];
    } else if (a === "details") this.c.details(id);
    else if (a === "code") this.c.open(id);
    else this.remove([id]);
  }
  setKind(ids: string[], value: string) {
    this.kinds = null;
    const edits: Edit[] = ids.filter((id) => { const b = this.c.box(id); return b && (b.custom ?? b.kind.value) !== value; })
      .map((id) => ({ op: "set", id, key: "kind", value }));
    if (edits.length) this.c.run(() => this.c.edit()!.apply(edits.length === 1 ? edits[0] : { op: "batch", edits }));
  }
  // a box with boxes inside asks once in the bar; a second Delete answers yes
  remove(ids: string[]) {
    const v = this.c.view(), r = removal(v.boxes, v.arrows, ids);
    if (!r.edit) return;
    const same = this.asking && this.asking.ids.join() === ids.join();
    if (r.inside > 0 && !same) {
      const n = `${r.inside} ${r.inside === 1 ? "box" : "boxes"} inside`;
      this.asking = { ids, edit: r.edit, text: ids.length === 1 ? `Delete ${this.titleOf(ids[0])} and ${n}?` : `Delete ${ids.length} boxes and ${n}?` };
      return;
    }
    this.asking = null;
    this.c.run(() => this.c.edit()!.apply(r.edit!));
  }
  confirmRemove() {
    const a = this.asking;
    this.asking = null;
    if (a) this.c.run(() => this.c.edit()!.apply(a.edit));
  }
  ground(make: "new" | "arrange") {
    const u = this.ui;
    this.ui = null;
    if (u?.t !== "ground") return;
    if (make === "new") this.c.draftAt({ parent: u.id, x: u.gx!, y: u.gy! });
    else this.c.arrange([u.id]);
  }
  /** Close the rename or move popover and send the edit it makes for its box (read before ui is cleared). */
  settle(make: (id: string) => Edit | null) {
    const u = this.ui;
    this.shut();
    const e = u ? make(u.id) : null;
    if (e) this.c.run(() => this.c.edit()!.apply(e));
  }
  menuAt(e: MouseEvent) {
    if (!this.c.edit()) return;
    const id = ((e.target as Element | null)?.closest?.("[data-box]:not([data-open])") as HTMLElement | null)?.dataset.box;
    e.preventDefault();
    this.kinds = this.asking = null;
    if (id && this.c.box(id)) {
      this.c.select(id);
      this.ui = { t: "menu", id, ...this.rel(e) };
    } else {
      const g = this.c.ground(e);
      this.ui = { t: "ground", id: g.parent, ...this.rel(e), gx: g.x, gy: g.y };
    }
  }
  /** The card's ⋯ button: the menu opens under it. */
  menuFrom(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect(), s = this.c.stage().getBoundingClientRect();
    this.ui = { t: "menu", id: this.c.selected()!, x: r.left - s.left, y: r.bottom - s.top };
  }
}
