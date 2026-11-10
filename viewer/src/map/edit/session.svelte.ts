// The state of one editing gesture on the map: the arrow being drawn, and the one popover (box menu, rename,
// move picker, arrow kind) or add form that is open. Map.svelte hands it what it knows through `Ctx`.
import type { EditApi } from "../../api";
import type { ArrowKind, Edit, Frame, MapBox, View } from "../types";
import type { Act } from "./menu";
import type { Tool } from "./tool.svelte";

export type Ui = { t: "menu" | "rename" | "move"; id: string; x: number; y: number } | { t: "arrow"; from: string; to: string; x: number; y: number };
export type Ctx = {
  edit: () => EditApi | undefined; box: (id: string) => MapBox | undefined; frame: (id: string) => Frame;
  cam: () => { x: number; y: number; k: number }; stage: () => HTMLElement;
  selected: () => string | null; select: (id: string | null) => void; tool: (t: Tool) => void; run: (p: () => Promise<View>) => void;
  arrange: (scope: string[]) => void;
};

export class EditSession {
  link = $state<{ from: string; to: string | null; x: number; y: number } | null>(null);
  ui = $state<Ui | null>(null);
  adding = $state<{ parent: string; x: number; y: number } | null>(null);
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
  get aim() { return this.link?.to ?? (this.ui?.t === "arrow" ? this.ui.to : null); }
  get band() {
    if (!this.link) return undefined;
    const f = this.c.frame(this.link.from), m = this.c.cam();
    return { x1: (f.x + f.w / 2) * m.k + m.x, y1: (f.y + f.h / 2) * m.k + m.y, x2: this.link.x, y2: this.link.y };
  }

  focusBox(id: string) {
    (this.c.stage().querySelector(`[data-box="${CSS.escape(id)}"] > button`) as HTMLElement | null)?.focus({ preventScroll: true });
  }
  /** Close the popover; focus goes back to its box unless the pointer went elsewhere. */
  shut(refocus = true) {
    const u = this.ui;
    this.ui = null;
    if (refocus && u) this.focusBox(u.t === "arrow" ? u.from : u.id);
  }
  /** Drop everything in flight (Esc, a cancelled pointer, the window losing focus). */
  cancel() {
    this.link = this.ui = this.adding = null;
    this.release();
  }
  /** A lost pointer drops the arrow in flight; open forms keep what was typed. */
  dropLink() {
    this.link = null;
    this.release();
  }
  /** A pointer press outside the open popover closes it. */
  outside(e: Event) {
    if (this.ui && !(e.target as Element | null)?.closest?.("[data-popover]")) this.ui = null;
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
    return (el?.closest?.("[data-box]") as HTMLElement | null)?.dataset.box;
  }
  /** Press on a box with the Arrow tool, or on its handle: starts the arrow. */
  startLink(e: PointerEvent, armed: boolean): boolean {
    const from = ((e.target as Element | null)?.closest?.("[data-box]") as HTMLElement | null)?.dataset.box;
    if (!this.c.edit() || !from || !this.c.box(from) || !(armed || (e.target as Element).closest("[data-handle]"))) return false;
    this.link = { from, to: null, ...this.rel(e) };
    this.grab(e.pointerId);
    return true;
  }
  moveLink(e: PointerEvent): boolean {
    if (!this.link) return false;
    const to = this.boxAt(e);
    this.link = { ...this.link, to: to && to !== this.link.from && this.c.box(to) ? to : null, ...this.rel(e) };
    return true;
  }
  /** Release: the popover for the chosen kind when it landed on another box. */
  endLink(): boolean {
    const l = this.link;
    if (!l) return false;
    this.link = null;
    this.release();
    if (l.to) this.ui = { t: "arrow", from: l.from, to: l.to, x: l.x, y: l.y };
    return true;
  }
  draw(kind: ArrowKind, label: string | null) {
    const u = this.ui;
    if (u?.t !== "arrow") return;
    this.shut();
    this.c.tool("select");
    this.c.run(() => this.c.edit()!.apply({ op: "arrow", from: u.from, to: u.to, kind, label }));
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
    const e = u && u.t !== "arrow" ? make(u.id) : null;
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
