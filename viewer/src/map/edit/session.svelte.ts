// The state of one editing gesture on the map: the arrow being drawn, the box being drawn (its place in its parent's
// inner units, kept as places are), the new arrow's kind chip, and the one popover (box menu, rename, move picker) or add form that is open. Map.svelte hands it what it knows through `Ctx`.
import type { EditApi } from "../../api";
import type { ArrowKind, ArrowKindView, Edit, Frame, KindView, MapBox, View } from "../types";
import { baseOf, guessArrow } from "./guess";
import type { Act } from "./menu";
import type { Tool } from "./tool.svelte";

/** A box being drawn; `from` when it is the far end of an arrow dropped on empty ground. */
export type Draft = { parent: string; x: number; y: number; w: number; h: number; title: string; drawing: boolean; from?: string };
export type Ui = { t: "menu" | "rename" | "move"; id: string; x: number; y: number };
export type Ctx = {
  edit: () => EditApi | undefined; box: (id: string) => MapBox | undefined; frame: (id: string) => Frame;
  kinds: () => KindView[]; arrowKinds: () => ArrowKindView[] | undefined;
  cam: () => { x: number; y: number; k: number }; stage: () => HTMLElement;
  selected: () => string | null; select: (id: string | null) => void; tool: (t: Tool) => void; used: (shift: boolean) => void;
  run: (p: () => Promise<View>) => boolean;
  arrange: (scope: string[]) => void;
  /** A link let go on empty ground: a draft there, its far end. */
  drop: (e: PointerEvent, from: string) => void;
  /** The draft's frame in world units, as `frame`. */
  draftFrame: () => Frame | undefined;
};
type Pt = { x: number; y: number };
const sides = (f: Frame): Pt[] => [{ x: f.x, y: f.y + f.h / 2 }, { x: f.x + f.w, y: f.y + f.h / 2 }, { x: f.x + f.w / 2, y: f.y }, { x: f.x + f.w / 2, y: f.y + f.h }];
const nearest = (f: Frame, p: Pt) => sides(f).reduce((a, b) => (Math.hypot(b.x - p.x, b.y - p.y) < Math.hypot(a.x - p.x, a.y - p.y) ? b : a));
const centre = (f: Frame): Pt => ({ x: f.x + f.w / 2, y: f.y + f.h / 2 });

export class EditSession {
  link = $state<{ from: string; to: string | null; x: number; y: number; x0: number; y0: number; armed: boolean } | null>(null);
  ui = $state<Ui | null>(null);
  /** The arrow just drawn: its chip changes the guessed kind. */
  made = $state<{ from: string; to: string; kind: ArrowKind } | null>(null);
  adding = $state<{ parent: string; x: number; y: number } | null>(null);
  draft = $state<Draft | null>(null);
  // the box whose kind list a Tab out of a draft asked for (Task 11 opens it)
  kinds = $state<string | null>(null);
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
  /** From the side of one box nearest the other to the side of the other nearest that. */
  private span(a: Frame, b: Frame) {
    const p = nearest(a, centre(b)), q = nearest(b, p);
    return { x1: p.x, y1: p.y, x2: q.x, y2: q.y };
  }
  /** The rubber band on screen: to the pointer, or caught by the nearest side of the box under it or of the draft at its end. */
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
  /** Where the new arrow's chip sits: the middle of its band. */
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
    this.link = this.ui = this.adding = this.draft = this.made = null;
    this.release();
  }
  /** A lost pointer drops the arrow in flight; open forms keep what was typed. */
  dropLink() {
    this.link = null;
    this.release();
  }
  /** A pointer press outside the open popover closes it. */
  outside(e: Event) {
    if ((e.target as Element | null)?.closest?.("[data-popover]")) return;
    this.ui = this.made = null;
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
  /** Press on a box's side dot, or anywhere on a box with the Arrow tool: starts the arrow. */
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
  /** Release on another box draws the arrow; on empty ground, a draft there at its end; on its own box, nothing. */
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
  /** The kind an arrow from one box to another (or to a box of no kind yet) is first drawn with. */
  guess(from: string, to: string | null): ArrowKind {
    const kinds = this.c.kinds(), kind = (id: string | null) => (id && this.c.box(id) ? baseOf(this.c.box(id)!.kind.value, kinds) : "box");
    return guessArrow(kind(from), kind(to), this.c.arrowKinds() ?? []);
  }
  /** Draw an arrow from one box to another with the guessed kind; its chip offers the others. */
  connect(from: string, to: string) {
    if (!this.c.edit() || from === to || !this.c.box(from) || !this.c.box(to)) return;
    const kind = this.guess(from, to);
    if (this.c.run(() => this.c.edit()!.apply({ op: "arrow", from, to, kind, label: null }))) this.made = { from, to, kind };
  }
  /** The chip's pick: the new arrow's kind. */
  rekind(kind: ArrowKind) {
    const m = this.made;
    if (!m || m.kind === kind) return;
    if (this.c.run(() => this.c.edit()!.apply({ op: "arrowset", from: m.from, to: m.to, key: "kind", value: kind }))) this.made = { ...m, kind };
  }
  /** The edit a confirmed draft sends: its box, and the arrow to it when it was dropped at an arrow's end. */
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
    else this.c.run(() => this.c.edit()!.apply({ op: "remove", id }));
  }
  /** Close the rename or move popover and send the edit it makes for its box (read before ui is cleared). */
  settle(make: (id: string) => Edit | null) {
    const u = this.ui;
    this.shut();
    const e = u ? make(u.id) : null;
    if (e) this.c.run(() => this.c.edit()!.apply(e));
  }
  menuAt(e: MouseEvent) {
    const id = ((e.target as Element | null)?.closest?.("[data-box]") as HTMLElement | null)?.dataset.box;
    if (!this.c.edit() || !id || !this.c.box(id)) return;
    e.preventDefault();
    this.c.select(id);
    this.ui = { t: "menu", id, ...this.rel(e) };
  }
  /** The card's ⋯ button: the menu opens under it. */
  menuFrom(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect(), s = this.c.stage().getBoundingClientRect();
    this.ui = { t: "menu", id: this.c.selected()!, x: r.left - s.left, y: r.bottom - s.top };
  }
}
