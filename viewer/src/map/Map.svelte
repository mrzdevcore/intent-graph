<!-- The walkable map. One world <div> under the camera; each box whose screen width reaches
     showsInside mounts its inside (children, regions, wires in its inner units), recursively.
     Only mounted levels are in the DOM. Dragging a box moves it inside its parent (kept per map
     in localStorage); dragging elsewhere, or an open box, pans. Map text is repo/AI-derived: text only, never {@html}. -->
<script lang="ts">
  import { onDestroy, untrack } from "svelte";
  import type { Frame, MapArrow as Arrow, MapBox as Box, Region as R, View } from "./types";
  import { HEAD, detailFor, fitInner, fitTo, placer, showsInside, type Cam } from "./camera";
  import { arrowLook, diffLook, insideCounts, linksOf, look } from "./looks";
  import { Camera, wheelFactor } from "./glide";
  import { wires, type Mode } from "./wires";
  import MapBox from "./MapBox.svelte";
  import { shapeOf } from "./shapes";
  import MapArrow from "./MapArrow.svelte";
  import Region from "./Region.svelte";
  import Pads from "./Pads.svelte";
  import { ANSWER_CARD, CARD, CARD_WIDE, fillsBeside, fitBeside, padsOf } from "./pads";
  import { answerFrame, answerOf, arrowLit, arrowNumber, boxStates, holders, stepFrame, type Aim } from "./answer";
  import AnswerCard from "./AnswerCard.svelte";
  import type { Show } from "../model/show";
  import type { Diff } from "../model/diff";
  import { diffAnswer, diffLines, files } from "./diff";
  import { key as arrowKey } from "./answer";
  import DetailsCard from "./DetailsCard.svelte";
  import Controls from "./Controls.svelte";
  import Search from "./Search.svelte";
  import type { Trace } from "./search";
  import type { EditApi } from "../api";
  import Toolbar from "./edit/Toolbar.svelte";
  import { ToolState, type Tool } from "./edit/tool.svelte";
  import AddBox from "./edit/AddBox.svelte";
  import { freeId, inside } from "./edit/add";
  import EditLayer from "./edit/EditLayer.svelte";
  import { EditSession } from "./edit/session.svelte";
  import type { Act } from "./edit/menu";
  import Proposals from "./edit/Proposals.svelte";
  import Warnings from "./Warnings.svelte";
  import Folders from "./Folders.svelte";
  import { proposed } from "./edit/proposals";

  let { view: given, camera = new Camera(), size, edit, onview, show, diff }: {
    view: View; camera?: Camera; size?: { w: number; h: number };
    /** Only where the host can edit (the desktop app); `onview` takes the view an edit returns. */
    edit?: EditApi; onview?: (v: View) => void;
    show?: Show | null;
    diff?: Diff | null;
  } = $props();
  const view = $derived(given);
  const diffed = $derived(diff ? diffAnswer(view, diff) : null);
  let cleared = $state(false);
  const showingDiff = $derived(!!diffed && !cleared);
  const ghosts = $derived(showingDiff ? diffed!.view.arrows.filter((a) => a.ghost) : []);
  const marks = $derived(showingDiff ? diffed!.marks : null);
  const lines = $derived(diff ? diffLines(diff) : null);

  const tool = new ToolState();
  let note = $state("");
  let noteTimer: ReturnType<typeof setTimeout> | undefined;
  onDestroy(() => clearTimeout(noteTimer));
  function toast(e: unknown) {
    note = String((e as Error)?.message ?? e);
    clearTimeout(noteTimer);
    noteTimer = setTimeout(() => (note = ""), 6000);
  }
  // one edit call at a time: the journal is read-modify-write, so a call made while one is in flight is dropped
  let busy = false;
  const run = (call: () => Promise<View>) => {
    if (busy) return;
    busy = true;
    call().then((v) => onview?.(v), toast).finally(() => (busy = false));
  };
  const typing = (t: EventTarget | null) => {
    const el = t as HTMLElement | null;
    return !!el && (/^(INPUT|TEXTAREA|SELECT)$/.test(el.tagName) || el.isContentEditable);
  };

  const ROOT = "system";
  // the boxes moved by hand: their shift from where ig placed them, in their parent's inner units
  type Moves = Record<string, { x: number; y: number }>;
  const store = $derived(`ig-moves:${view.boxes.find((b) => b.id === ROOT)?.title.value ?? ""}`);
  function load(key: string): Moves {
    try {
      const m = JSON.parse(globalThis.window?.localStorage?.getItem(key) ?? "{}");
      return m && typeof m === "object" ? m : {};
    } catch {
      return {};
    }
  }
  let moves = $state<Moves>(untrack(() => load(store)));
  function keep(m: Moves) {
    moves = m;
    try {
      if (Object.keys(m).length) globalThis.window?.localStorage?.setItem(store, JSON.stringify(m));
      else globalThis.window?.localStorage?.removeItem(store);
    } catch { /* private mode: moves last the page */ }
  }
  // the view as drawn: ig's frames with the hand moves applied
  const shown = $derived.by((): View => {
    const frames = { ...view.placed.frames };
    for (const [id, d] of Object.entries(moves)) {
      const f = frames[id];
      if (f && Number.isFinite(d?.x) && Number.isFinite(d?.y)) frames[id] = { ...f, x: f.x + d.x, y: f.y + d.y };
    }
    return { ...view, placed: { ...view.placed, frames } };
  });
  const group = <T,>(xs: T[], key: (x: T) => string) => {
    const m = new Map<string, T[]>();
    for (const x of xs) m.set(key(x), [...(m.get(key(x)) ?? []), x]);
    return m;
  };
  // a bad map can never loop the page: the first box of an id only, never its own parent
  const framed = $derived.by(() => {
    const seen = new Set<string>();
    return view.boxes.filter((b) => view.placed.frames[b.id] && b.parent !== b.id && !seen.has(b.id) && !!seen.add(b.id));
  });
  const byId = $derived(new Map(view.boxes.map((b) => [b.id, b])));
  const kids = $derived(group(framed, (b: Box) => b.parent ?? ROOT));
  // a component's part boxes (inputs, state, uses, renders): its card sums them up
  const anatomies = $derived(group(view.boxes.filter((b) => b.kind.value === "parts" && !!b.parent), (b: Box) => b.parent!));
  // world frames, once per layout (linear in the boxes): a drag in flight leaves them be
  const place = $derived(placer(shown));
  const world = $derived(new Map(framed.map((b) => [b.id, place.frame(b.id)])));
  const area = (f: Frame) => f.w * f.h;
  const regions = $derived(group([...view.placed.regions].sort((a, b) => area(b.frame) - area(a.frame)), (r: R) => r.parent));
  /** Each parent's wires both ways, from its children's frames as drawn. */
  const wired = (as: Arrow[], frames: Record<string, Frame>) => ({ routed: wires(as, frames, "routed"), ribbon: wires(as, frames, "ribbon") });
  const siblings = (p: string) => Object.fromEntries((kids.get(p) ?? []).map((b) => [b.id, shown.placed.frames[b.id]]));
  const wiring = $derived(new Map([...group([...view.arrows, ...ghosts], (a: Arrow) => a.parent)].map(([p, as]) => [p, wired(as, siblings(p))])));
  const arrows = $derived(group([...view.arrows, ...ghosts].filter((a) => wiring.get(a.parent)?.routed.wires.has(a.id)), (a: Arrow) => a.parent));
  // the box being dragged and its shift so far: only it and its parent's arrows follow the pointer
  let live = $state<{ id: string; x: number; y: number } | null>(null);
  const shifted = (id: string, d: { x: number; y: number }) => {
    const f = view.placed.frames[id];
    return { ...f, x: f.x + d.x, y: f.y + d.y };
  };
  const drawn = (id: string) => (live?.id === id ? shifted(id, live) : shown.placed.frames[id]);
  const liveWiring = $derived.by(() => {
    if (!live) return null;
    const p = byId.get(live.id)?.parent ?? ROOT, frames = siblings(p);
    frames[live.id] = shifted(live.id, live);
    return { p, ...wired(arrows.get(p) ?? [], frames) };
  });
  const wiringOf = (p: string) => (liveWiring?.p === p ? liveWiring : wiring.get(p));
  const pathOf = (a: Arrow, mode: Mode) => wiringOf(a.parent)![mode].wires.get(a.id)!;
  // under this many screen pixels per unit, a level's wires gather into ribbons
  const RIBBON = 0.5;
  // each parent's inner unit, in world units
  const unit = $derived(new Map([...kids.keys()].map((p) => [p, p === ROOT ? 1 : place.inside(p).s])));
  // past this many screen pixels per unit, a level is far behind the box you are in: its arrows
  // and regions are no longer drawn
  const DEEP = 12;
  const layers = $derived([...new Set(["rules", "risks", ...view.marks.map((m) => m.layer)])]);
  let off = $state(new Set<string>());
  const on = $derived(new Set(layers.filter((l) => !off.has(l))));
  const counts = $derived(insideCounts(view, view.marks, on));
  const reading = $derived(view.boxes.some((b) => b.needsReading));

  let cw = $state(0), ch = $state(0);
  const vw = $derived(size?.w ?? cw), vh = $derived(size?.h ?? ch);
  let cam = $state<Cam>(untrack(() => camera.cam));
  let selected = $state<string | null>(null);
  // the path the search lit: its boxes and arrows drawn in full, the rest of that level faded
  let traced = $state<Trace | null>(null);
  function lightPath(t: Trace | null) {
    traced = t;
    if (t) {
      selected = null;
      fly(t.parent);
    }
  }
  // a hovered proposal lights through the same path; the search's own path comes back when it ends
  let held: { t: Trace | null } | undefined;
  function glow(t: Trace | null) {
    if (t) {
      held ??= { t: traced };
      traced = t;
    } else if (held) {
      traced = held.t;
      held = undefined;
    }
  }
  let current = $state.raw<Show | null>(null);
  let waiting = $state.raw<Show | null>(null);
  let step = $state(0);
  let away = $state(false);
  const answer = $derived(showingDiff ? diffed!.answer : current ? answerOf(view, current) : null);
  const states = $derived(answer ? boxStates(view, answer) : null);
  const nowOn = $derived(new Set((answer?.steps[step]?.boxes ?? []).flatMap((id) => [id, ...holders(view, id)])));
  const by = $derived(proposed(view.proposals));
  // under the pointer: the deepest box, and an arrow
  let hot = $state<string | null>(null);
  let hotArrow = $state<string | null>(null);
  const touches = (a: Arrow) => a.id === hotArrow || [hot, selected].some((id) => id && linksOf([a], id).length > 0);
  function over(e: PointerEvent) {
    const el = (e.target as Element | null)?.closest?.("[data-box]") as HTMLElement | null;
    hot = el?.dataset.box ?? null;
  }
  let stage: HTMLDivElement;

  const home = () => fitTo(place.frame(ROOT), { w: vw, h: vh });
  const frameOf = (id: string) => world.get(id) ?? place.frame(id);

  // rAF-driven camera: runs only while a glide is in flight.
  let raf = 0;
  function frame(now: number) {
    raf = 0;
    cam = camera.tick(now);
    if (camera.moving) raf = requestAnimationFrame(frame);
  }
  function glideTo(c: Cam) {
    away = !!current || showingDiff;
    camera.to(c, performance.now());
    if (!raf) raf = requestAnimationFrame(frame);
  }
  function jump(c: Cam) {
    away = !!current || showingDiff;
    camera.set(c);
    cam = c;
  }
  /** Advance the camera to `now` (tests; the rAF loop does this in the page). */
  export function tick(now: number) {
    cam = camera.tick(now);
  }
  onDestroy(() => cancelAnimationFrame(raf));

  let fitted = false;
  $effect(() => {
    if (!fitted && vw > 0 && vh > 0) {
      fitted = true;
      untrack(() => jump(home()));
    }
  });

  /** The card's room in rem, open or not: it moves below at the phone width. */
  const cardRem = () => globalThis.matchMedia?.("(max-width: 47.5rem)").matches ? 0 : view.sources?.[selected ?? ""] ? CARD_WIDE : CARD;
  const rem = () => parseFloat(getComputedStyle(document.documentElement).fontSize) || 16;
  /** Where opening a box puts the camera. A box with pads: its whole frame, beside the details card
   *  (below it at the phone width, so across the full width) with room for the labels. Otherwise a
   *  box with an inside is framed by its inside (so the inside fills the screen), others by their frame. */
  function target(id: string): Cam {
    const to = { w: vw, h: vh }, pads = padsOf(view, id, frameOf(id), drawn, on);
    if (pads.length) return fitBeside(frameOf(id), to, pads, selected ? cardRem() : 0, rem());
    const inner = view.placed.inner[id];
    if (!kids.has(id) || !inner) return fitTo(frameOf(id), to);
    const t = place.inside(id);
    return fitTo({ x: t.x, y: t.y, w: inner[0] * t.s, h: inner[1] * t.s }, to);
  }
  const fly = (id: string) => glideTo(id === ROOT ? home() : target(id));
  function aimAt(aim: Aim | null) {
    if (!aim) return;
    const pads = aim.id ? padsOf(view, aim.id, aim.frame, drawn, on) : [];
    const beside = globalThis.matchMedia?.("(max-width: 47.5rem)").matches ? 0 : ANSWER_CARD;
    glideTo(fitBeside(aim.frame, { w: vw, h: vh }, pads, 0, rem(), beside));
    away = false;
  }
  const backToAnswer = () => answer && aimAt(answerFrame(answer, view, frameOf));
  function land(s: Show) {
    current = s;
    waiting = null;
    step = 0;
    selected = null;
    traced = null;
    backToAnswer();
  }
  function goStep(i: number) {
    if (!answer || i < 0 || i >= answer.steps.length) return;
    step = i;
    aimAt(stepFrame(answer, i, view, frameOf, cam, { w: vw, h: vh }));
  }
  function clearAnswer() {
    current = null;
    cleared = true;
    away = false;
  }
  function openBox(id: string) {
    selected = id;
    fly(id);
  }
  function close() {
    const id = selected;
    selected = null;
    if (id) sess.focusBox(id);
  }
  const tools: Record<string, Tool> = { v: "select", b: "box", a: "arrow" };
  const shortcuts: Record<string, Act> = { f2: "rename", m: "move", delete: "delete", backspace: "delete" };
  function key(e: KeyboardEvent) {
    if (typing(e.target)) return;
    if ((current || showingDiff) && !sess.ui && (e.key === "ArrowRight" || e.key === "ArrowLeft")) {
      goStep(step + (e.key === "ArrowRight" ? 1 : -1));
      return;
    }
    if (edit && e.key !== "Escape") {
      // a popover owns the keys while it is open or has focus
      if (e.repeat || sess.ui || (e.target as Element | null)?.closest?.("[data-popover]")) return;
      const k = e.key.toLowerCase();
      if ((e.metaKey || e.ctrlKey) && k === "z") {
        e.preventDefault();
        run(() => e.shiftKey ? edit.redo() : edit.undo());
        return;
      }
      if (!e.metaKey && !e.ctrlKey && !e.altKey && selected && shortcuts[k]) {
        e.preventDefault();
        sess.act(shortcuts[k], selected);
        return;
      }
      if (!e.metaKey && !e.ctrlKey && !e.altKey && tools[k]) tool.current = tools[k];
    }
    if (e.key !== "Escape") return;
    tool.current = "select";
    sess.cancel();
    if (selected) close();
    else if (traced) traced = null;
    else clearAnswer();
  }
  // deep enough for the smallest code box of a deep tree
  const clampK = (k: number) => Math.min(1e9, Math.max(1e-3, k));
  function zoomAt(px: number, py: number, f: number, smooth = false) {
    const c = camera.cam, k = clampK(c.k * f), r = k / c.k;
    const next = { k, x: px - (px - c.x) * r, y: py - (py - c.y) * r };
    if (smooth) glideTo(next); else jump(next);
  }

  // Wheel needs a non-passive listener to stop the page scrolling.
  $effect(() => {
    const wheel = (e: WheelEvent) => {
      e.preventDefault();
      const r = stage.getBoundingClientRect();
      zoomAt(e.clientX - r.left, e.clientY - r.top, wheelFactor(e));
    };
    stage.addEventListener("wheel", wheel, { passive: false });
    return () => stage.removeEventListener("wheel", wheel);
  });

  // Drag pans, or moves the closed box under the pointer (`box`: its id, its shift at the start
  // and screen pixels per unit of its parent); a drag's trailing click must not open a box.
  let drag: { x: number; y: number; from: Cam; moved: boolean; pointer: number; box?: { id: string; at: { x: number; y: number }; k: number } } | null = null;
  let swallow = false;
  function down(e: PointerEvent) {
    swallow = false;
    // a pad is only ever clicked: it is a button on the box's edge, not a grip on the box
    if (e.button !== 0 || (e.target as Element | null)?.closest?.("[data-pad]")) return;
    if (sess.startLink(e, tool.current === "arrow")) return;
    drag = { x: e.clientX, y: e.clientY, from: camera.cam, moved: false, pointer: e.pointerId };
    const el = (e.target as Element | null)?.closest?.("[data-box]") as HTMLElement | null;
    const id = el && !el.hasAttribute("data-open") ? el.dataset.box : undefined;
    const b = id ? byId.get(id) : undefined;
    if (b) drag.box = { id: b.id, at: moves[b.id] ?? { x: 0, y: 0 }, k: (unit.get(b.parent ?? ROOT) ?? 1) * camera.cam.k };
  }
  /** Shift `d` of box `id`, kept inside its parent's frame (the system has none: anywhere). */
  function within(id: string, d: { x: number; y: number }) {
    const f = view.placed.frames[id], p = byId.get(id)?.parent;
    const pf = p && p !== ROOT ? shown.placed.frames[p] : undefined;
    const inner = p ? view.placed.inner[p] : undefined;
    if (!f || !pf || !inner) return d;
    const t = fitInner(pf, inner[0], inner[1]), head = Math.min(HEAD, pf.h / 4);
    const clamp = (v: number, lo: number, hi: number) => Math.min(Math.max(v, lo), Math.max(lo, hi));
    return {
      x: clamp(f.x + d.x, -t.x / t.s, (pf.w - t.x) / t.s - f.w) - f.x,
      y: clamp(f.y + d.y, (head - t.y) / t.s, (pf.h - t.y) / t.s - f.h) - f.y,
    };
  }
  function move(e: PointerEvent) {
    if (sess.moveLink(e) || !drag) return;
    const dx = e.clientX - drag.x, dy = e.clientY - drag.y;
    if (!drag.moved && Math.hypot(dx, dy) < 4) return;
    if (!drag.moved) sess.grab(drag.pointer);
    drag.moved = true;
    const b = drag.box;
    if (b && b.k > 0) live = { id: b.id, ...within(b.id, { x: b.at.x + dx / b.k, y: b.at.y + dy / b.k }) };
    else jump({ k: drag.from.k, x: drag.from.x + dx, y: drag.from.y + dy });
  }
  function up() {
    if (sess.endLink()) {
      swallow = true;
      return;
    }
    sess.release();
    swallow = !!drag?.moved;
    if (live) keep({ ...moves, [live.id]: { x: live.x, y: live.y } });
    live = null;
    drag = null;
  }
  // a cancelled pointer, or the window losing focus mid-gesture, leaves nothing half done
  function abort() {
    sess.dropLink();
    live = null;
    drag = null;
  }
  const sess = new EditSession({
    edit: () => edit, box: (id) => byId.get(id), frame: frameOf, cam: () => cam, stage: () => stage,
    selected: () => selected, select: (id) => (selected = id), tool: (t) => (tool.current = t), run,
  });
  const addParent = $derived(sess.adding ? byId.get(sess.adding.parent) : undefined);
  let landed: Show | null | undefined;
  $effect(() => {
    const s = show;
    if (!s || s === landed || diff || !(vw > 0 && vh > 0)) return;
    untrack(() => {
      landed = s;
      if (sess.ui || sess.adding || tool.current !== "select" || live) waiting = s;
      else land(s);
    });
  });
  let diffLanded = false;
  $effect(() => {
    if (!diffed || diffLanded || !(vw > 0 && vh > 0)) return;
    untrack(() => {
      diffLanded = true;
      step = 0;
      backToAnswer();
      away = false;
    });
  });
  function clickCapture(e: MouseEvent) {
    if (swallow) {
      swallow = false;
    } else if (edit && tool.current === "box") {
      const id = ((e.target as Element | null)?.closest?.("[data-box]") as HTMLElement | null)?.dataset.box;
      const r = stage.getBoundingClientRect();
      sess.adding = { parent: id && byId.has(id) ? id : ROOT, x: e.clientX - r.left, y: e.clientY - r.top };
    } else return;
    e.stopPropagation();
    e.preventDefault();
  }
  function add(a: { kind: string; name: string; at: string[]; props: Record<string, string> }) {
    const parent = sess.adding!.parent;
    sess.adding = null;
    tool.current = "select";
    const props = Object.keys(a.props).length ? { props: a.props } : {};
    run(() => edit!.apply({ op: "add", parent, kind: a.kind, id: freeId(a.name, new Set(view.boxes.map((b) => b.id))), title: a.name, at: a.at, before: null, ...props }));
  }

  /** The deepest box holding the view's centre and filling >= 60% of the view's width or height;
   *  a box with pads may do so in the space opening it fits it to, beside the card. */
  const trail = $derived.by(() => {
    const out = [ROOT], card = cardRem(), r = rem();
    // ponytail: padsOf walks every arrow, but only for the box or two a frame's test lets through; memo per view if a huge map shows it
    const hasPads = (id: string) => padsOf(view, id, frameOf(id), () => undefined, on).length > 0;
    for (let p = ROOT; ; ) {
      if (out.indexOf(p) !== out.length - 1) return out.slice(0, -1); // a loop: stop before it
      const next = (kids.get(p) ?? []).find((b) => {
        const f = world.get(b.id)!, x = f.x * cam.k + cam.x, y = f.y * cam.k + cam.y, w = f.w * cam.k, h = f.h * cam.k;
        return (x <= vw / 2 && vw / 2 <= x + w && y <= vh / 2 && vh / 2 <= y + h && Math.max(w / vw, h / vh) >= 0.6)
          || (fillsBeside({ x, y, w, h }, { w: vw, h: vh }, card, r) && hasPads(b.id));
      });
      if (!next) return out;
      out.push((p = next.id));
    }
  });
  /** The deepest open box of the trail covering the whole view: only its inside is drawn, placed
   *  straight on the screen, so neither the boxes around it nor a huge scale cost anything. */
  const anchor = $derived.by(() => {
    let a = ROOT;
    for (const id of trail.slice(1)) {
      const f = world.get(id)!, x = f.x * cam.k + cam.x, y = f.y * cam.k + cam.y;
      const covers = x <= 0 && y <= 0 && x + f.w * cam.k >= vw && y + f.h * cam.k >= vh;
      if (!covers || !kids.has(id) || !view.placed.inner[id]) break;
      a = id;
    }
    return a;
  });
  const base = $derived.by(() => {
    const t = anchor === ROOT ? { x: 0, y: 0, s: 1 } : place.inside(anchor);
    return { x: t.x * cam.k + cam.x, y: t.y * cam.k + cam.y, k: t.s * cam.k };
  });
  /** Whether box `id` is on screen, or near it (half a view around). */
  function seen(id: string) {
    const f = world.get(id)!, x = f.x * cam.k + cam.x, y = f.y * cam.k + cam.y;
    return x < 1.5 * vw && y < 1.5 * vh && x + f.w * cam.k > -0.5 * vw && y + f.h * cam.k > -0.5 * vh;
  }
  /** `id` is `top` or inside it. */
  function under(id: string, top: string) {
    for (let p: string | undefined = id, n = 0; p && n < 64; p = byId.get(p)?.parent, n++) if (p === top) return true;
    return false;
  }
  // the card follows you: walking into another box closes the card of a box outside it (not
  // while a glide is on its way to the selected box)
  $effect(() => {
    const at = trail[trail.length - 1];
    untrack(() => {
      if (selected && !camera.moving && !trail.includes(selected) && !under(selected, at)) selected = null;
    });
  });
  // a reload can drop the selected box or the lit path's boxes
  $effect(() => {
    if (selected && !byId.has(selected)) selected = null;
    if (traced && !traced.boxes.every((id) => byId.has(id))) traced = null;
    // an open popover or form on a box that is gone has nothing to act on
    const gone = (id: string) => id !== ROOT && !byId.has(id);
    const u = sess.ui;
    if ((u && (gone(u.t === "arrow" ? u.from : u.id) || (u.t === "arrow" && gone(u.to)))) || (sess.adding && gone(sess.adding.parent))) sess.cancel();
  });
  const crumbs = $derived(trail.map((id) => ({ id, title: byId.get(id)?.title.value ?? id })));

  /** A box's inner space inside its own frame, below its header (as place.inside, locally). */
  function local(id: string) {
    const f = shown.placed.frames[id], [iw, ih] = view.placed.inner[id] ?? [0, 0];
    return { ...fitInner(f, iw, ih), iw, ih };
  }
</script>

{#snippet level(pid: string)}
  {@const zoom = (unit.get(pid) ?? 1) * cam.k}
  {@const near = zoom < DEEP}
  {@const all = near ? arrows.get(pid) ?? [] : []}
  {@const path = traced?.parent === pid ? traced : null}
  {@const ans = path ? null : answer}
  {@const lit = path ? all.filter((a) => path.arrows.includes(a)) : all.filter((a) => touches(a) || (ans && arrowLit(ans, a) && (!marks || (nowOn.has(a.from) && nowOn.has(a.to)))))}
  {@const answered = ans && (kids.get(pid) ?? []).some((b) => states?.has(b.id))}
  {@const mode = zoom < RIBBON ? "ribbon" : "routed"}
  {#if near}{#each regions.get(pid) ?? [] as r, i (i)}<Region region={r} {zoom} />{/each}{/if}
  {#if all.length && mode === "ribbon"}
    <svg class="sleeves" width="1" height="1" aria-hidden="true">
      {#each wiringOf(pid)?.ribbon.sleeves ?? [] as s, i (i)}<path d={s.d} stroke-width={s.w} />{/each}
    </svg>
  {/if}
  {#each all as a (a.id)}
    {@const c = pathOf(a, mode)}
    {@const asked = ans && arrowLit(ans, a)}
    {@const drawnLook = arrowLook(a, view.marks, on, by.get(a.id))}
    {@const changeLook = !marks ? null : a.ghost ? "ghost" : marks.addedArrows.has(arrowKey(a.from, a.to)) ? "added" : null}
    <div class="arrow" data-arrow={a.id}>
      <MapArrow d={c.d} mid={c.mid} arrowLook={changeLook ? diffLook(drawnLook, changeLook) : asked ? { ...drawnLook, color: "var(--map-trace)" } : drawnLook} {zoom}
        lit={lit.includes(a)} dim={lit.length > 0 && !lit.includes(a)}
        onhover={(h) => (hotArrow = h ? a.id : hotArrow === a.id ? null : hotArrow)} />
      {#if ans && arrowNumber(ans, a)}
        <span class="number on-arrow" class:now={arrowNumber(ans, a) === step + 1} style:left="{c.mid[0]}px" style:top="{c.mid[1]}px" style:scale={1 / zoom}>{arrowNumber(ans, a)}</span>
      {/if}
    </div>
  {/each}
  {#each (kids.get(pid) ?? []).filter((b) => !vw || seen(b.id)) as b (b.id)}
    {@const f = drawn(b.id)}
    {@const sw = world.get(b.id)!.w * cam.k}
    {@const open = showsInside(sw) && kids.has(b.id) && !!view.placed.inner[b.id]}
    {@const source = view.sources?.[b.id]}
    {@const list = view.parts?.[b.id]}
    <!-- a part box shows its list once it is near; code waits for more room -->
    {@const reads = !open && (!!source || !!list) && (showsInside(sw) || (!!list && detailFor(sw) === "near"))}
    {@const anatomy = anatomies.get(b.id)?.map((k) => ({ title: k.title.value, parts: view.parts?.[k.id] ?? [] }))}
    {@const shape = shapeOf(b, kids.get(b.id))}
    {@const st = ans ? states?.get(b.id) : undefined}
    {@const inAnswer = !!st && (st.in || (!open && st.holds > 0))}
    {@const change = marks?.boxes.get(b.id)}
    {@const goneHere = marks?.removedHere.get(b.id)?.length ?? 0}
    <div class="slot" data-shape={shape} class:target={sess.aim === b.id} class:traced={path?.boxes.includes(b.id)} class:faded={path ? !path.boxes.includes(b.id) : answered && !st}
      class:answer={inAnswer && (!marks || nowOn.has(b.id))} class:answer-now={inAnswer && nowOn.has(b.id)} class:diff-changed={change === "changed"} class:diff-added={change === "added"} data-box={b.id} data-open={open ? "" : undefined} style:left="{f.x}px" style:top="{f.y}px" style:width="{f.w}px" style:height="{f.h}px">
      <MapBox box={b} look={look(b.id, view.marks, on, b, by.get(b.id))} detail={open ? "open" : reads ? "source" : detailFor(sw)}
        zoom={sw / f.w} source={reads && (showsInside(sw) || !list) ? source : undefined} parts={list} {anatomy}
        inside={counts.get(b.id) ?? 0} more={kids.get(b.id)?.length ?? 0} onopen={() => openBox(b.id)}
        kinds={view.kinds} w={f.w} h={f.h} {shape} />
      {#if edit && selected === b.id && !open}
        {#each ["l", "r", "t", "b"] as h (h)}<span class="handle {h}" data-handle aria-hidden="true" style:scale={sw ? f.w / sw : 1}></span>{/each}
      {/if}
      {#if open}
        {@const t = local(b.id)}
        <div class="inside" style:width="{t.iw}px" style:height="{t.ih}px"
          style:transform="translate({t.x}px, {t.y}px) scale({t.s})">{@render level(b.id)}</div>
      {/if}
      {#if showsInside(sw)}<Pads {view} id={b.id} frame={f} childFrame={drawn} {on} scale={f.w / sw} onopen={openBox} />{/if}
      {#if inAnswer}
        <span class="numbers" style:scale={f.w / sw}>
          {#each (ans?.numbers.has(b.id) ? [ans.numbers.get(b.id)!] : st!.under).filter((n) => !marks || n === step + 1) as n (n)}<span class="number" class:now={n === step + 1}>{n}</span>{/each}
        </span>
        {#if !st!.in && !marks}<span class="holds" style:scale={f.w / sw}>{st!.holds} of {kids.get(b.id)?.length ?? st!.holds}</span>{/if}
      {/if}
      {#if change}<span class="change {change}" style:scale={f.w / sw}>{change === "added" ? "new" : files(marks!.files.get(b.id) ?? 0)}</span>{/if}
      {#if goneHere}<span class="gone-here" style:scale={f.w / sw}>−{goneHere} removed here</span>{/if}
    </div>
  {/each}
  <!-- the focused arrows' labels, above the boxes -->
  {#each lit as a (a.id)}
    {@const c = pathOf(a, mode)}
    <div class="arrow" data-label={a.id}>
      <MapArrow part="label" d={c.d} mid={c.mid} arrowLook={arrowLook(a, view.marks, on)} {zoom} />
    </div>
  {/each}
{/snippet}

<svelte:window onpointermove={move} onpointerup={up} onpointercancel={abort} onblur={abort} onpointerdowncapture={(e) => sess.outside(e)} onkeydown={key} />

<!-- the dot grid pans with the world; it keeps its size at any zoom -->
<div class="map" bind:clientWidth={cw} bind:clientHeight={ch} style:background-position="{base.x}px {base.y}px">
  <div class="stage" bind:this={stage} role="presentation" onpointerdown={down} onclickcapture={clickCapture} oncontextmenu={(e) => sess.menuAt(e)}
    onpointerover={over} onpointerleave={() => (hot = hotArrow = null)}>
    <div class="world" style:transform="translate({base.x}px, {base.y}px) scale({base.k})">{@render level(anchor)}</div>
  </div>
  <Controls {crumbs} {layers} {on} oncrumb={fly}
    ontoggle={(l) => { const s = new Set(off); if (!s.delete(l)) s.add(l); off = s; }}
    onzoom={(f) => zoomAt(vw / 2, vh / 2, f, true)} onfit={() => glideTo(home())}
    onreset={Object.keys(moves).length ? () => keep({}) : undefined}>
    {#snippet bar()}
      {#if edit}<Toolbar tool={tool.current} onpick={(t) => (tool.current = t)} onundo={() => run(() => edit.undo())} onredo={() => run(() => edit.redo())} {note} />{/if}
    {/snippet}
    <Search {view} onopen={(id) => { traced = null; openBox(id); }} ontrace={lightPath} />
  </Controls>
  {#if edit && sess.adding}
    <AddBox parentTitle={addParent?.title.value ?? view.boxes.find((b) => b.id === ROOT)?.title.value ?? ROOT} within={inside(sess.adding.parent, byId, view.kinds)}
      kinds={view.kinds} globs={addParent?.madeOf ?? []} x={sess.adding.x} y={sess.adding.y} onadd={add} onclose={() => sess.cancel()} />
  {/if}
  <Proposals {view} {edit} {run} onlight={glow} />
  <Warnings warnings={view.warnings} />
  {#if answer}
    <AnswerCard {answer} {step} {away} onstep={goStep} onback={backToAnswer} onclear={clearAnswer}
      change={showingDiff} sub={showingDiff ? lines?.sub : undefined} note={showingDiff ? lines?.note : undefined} />
  {/if}
  {#if showingDiff}
    <ul class="legend" aria-label="Legend">
      <li><span class="sw changed"></span>changed</li>
      <li><span class="sw added"></span>added</li>
      <li><span class="sw removed"></span>removed, counted on its parent</li>
      <li><svg viewBox="0 0 26 8" aria-hidden="true"><path d="M0 4 H26" class="w-added" /></svg>new arrow</li>
      <li><svg viewBox="0 0 26 8" aria-hidden="true"><path d="M0 4 H26" class="w-ghost" /></svg>removed arrow, both ends still here</li>
    </ul>
  {/if}
  {#if waiting}
    <div class="waiting" role="status">
      <span class="words"><span class="muted">New answer</span><b>{waiting.title}</b></span>
      <button type="button" class="go" onclick={() => land(waiting!)}>Show</button>
      <button type="button" onclick={() => (waiting = null)}>Later</button>
    </div>
  {/if}
  {#if edit}<EditLayer s={sess} boxes={view.boxes} kinds={view.kinds} root={ROOT} />{/if}
  {#if view.landscape}
    <Folders landscape={view.landscape} />
  {/if}
  {#if reading}
    <p class="reading" class:lifted={!!view.proposals?.length}>{#if view.landscape}Dotted projects are not read yet.{:else}Some boxes are ig's guess.{/if} Run <code>ig analyse</code> to let an AI agent read them.</p>
  {/if}
  {#if selected && byId.get(selected)}
    <DetailsCard {view} box={byId.get(selected)!} onclose={close} onopen={(id) => { traced = null; openBox(id); }} onmenu={edit ? (e) => sess.menuFrom(e) : undefined} {edit} {run} />
  {/if}
</div>

<style>
  .map { position: absolute; inset: 0; overflow: hidden;
    background: var(--map-ground) radial-gradient(var(--map-dots) 1.2px, transparent 1.4px) 0 0 / 18px 18px; color: var(--map-fg); }
  .stage { position: absolute; inset: 0; cursor: grab; touch-action: none; }
  .stage:active { cursor: grabbing; }
  .world { position: absolute; left: 0; top: 0; transform-origin: 0 0; }
  .slot { position: absolute; }
  .slot:not([data-open]) { cursor: move; }
  .slot.traced { outline: 3px solid var(--map-trace); outline-offset: 4px; border-radius: 16px; }
  .slot.answer { outline: 2px solid var(--map-trace); outline-offset: 4px; border-radius: 16px; }
  .slot.answer-now { outline-width: 3px; box-shadow: 0 0 0 10px color-mix(in srgb, var(--map-trace) 22%, transparent); }
  .numbers { position: absolute; left: 0; top: 0; display: flex; gap: 0.1875rem; transform-origin: 0 0; translate: -0.6875rem -0.6875rem; pointer-events: none; z-index: 3; }
  .number { flex: none; width: 1.375rem; height: 1.375rem; border-radius: 50%; box-shadow: 0 0 0 2px var(--map-ground); background: var(--map-trace); color: var(--color-on-accent); font: 700 var(--text-sm)/1.375rem var(--font-mono); text-align: center; }
  .number.now { outline: 2px solid var(--map-trace); outline-offset: 2px; }
  .number.on-arrow { position: absolute; translate: -50% -50%; pointer-events: none; z-index: 3; }
  .holds { position: absolute; left: 0.5rem; bottom: 0.5rem; transform-origin: 0 100%; padding: 0.125rem 0.5rem; border-radius: var(--radius-pill); border: 1px solid var(--map-trace); background: var(--map-surface); color: var(--map-fg); font: 600 var(--text-xs)/1.2 var(--font-sans); pointer-events: none; white-space: nowrap; }
  .waiting { position: absolute; z-index: 3; left: var(--space-4); top: 5.75rem; display: flex; align-items: center; gap: var(--space-2); padding: var(--space-2) var(--space-2-5) var(--space-2) var(--space-3); background: var(--map-surface); border: 1px solid var(--map-trace); border-radius: var(--radius-xl); box-shadow: 0 6px 20px var(--map-shadow); font-size: var(--text-md); }
  .waiting .words { display: flex; flex-direction: column; min-width: 0; }
  .waiting .muted { font-size: var(--text-sm); color: var(--map-muted); }
  .waiting button { font: inherit; color: var(--map-fg); background: var(--map-surface); border: 1px solid var(--map-line); border-radius: var(--radius-md); padding: var(--space-1) var(--space-2-5); cursor: pointer; }
  .waiting button.go { background: var(--map-trace); border-color: var(--map-trace); color: var(--color-on-accent); font-weight: 600; }
  .slot.target { outline: 3px solid var(--map-green); outline-offset: 3px; border-radius: 16px; }
  .handle { position: absolute; width: 0.625rem; height: 0.625rem; border-radius: 50%; background: var(--map-blue); border: 2px solid var(--map-ground); translate: -50% -50%; cursor: crosshair; }
  .handle.l { left: 0; top: 50%; } .handle.r { left: 100%; top: 50%; } .handle.t { left: 50%; top: 0; } .handle.b { left: 50%; top: 100%; }
  .slot.faded { opacity: 0.35; transition: opacity 150ms; }
  .slot.diff-changed { outline: 0.125rem solid var(--color-changed); outline-offset: 0.125rem; border-radius: 1rem; }
  .slot.diff-added { outline: 0.125rem solid var(--color-added); outline-offset: 0.125rem; border-radius: 1rem; }
  .slot.answer-now.diff-changed, .slot.answer-now.diff-added { outline: 0.1875rem solid var(--map-trace); outline-offset: 0.3125rem; }
  .change, .gone-here { position: absolute; pointer-events: none; z-index: 3; white-space: nowrap; font: 600 var(--text-xs)/1.4 var(--font-sans); padding: 0 var(--space-1-5); border-radius: var(--radius-pill); }
  .change { right: 0; top: 0; transform-origin: 100% 0; translate: 0 -55%; }
  .change.changed { color: var(--color-changed); background: color-mix(in srgb, var(--color-changed) 15%, var(--map-surface)); }
  .change.added { color: var(--color-added); background: color-mix(in srgb, var(--color-added) 15%, var(--map-surface)); }
  .gone-here { left: 50%; bottom: 0; transform-origin: 0 100%; translate: -50% 50%; color: var(--color-removed); background: var(--map-ground); border: 1px dashed var(--color-removed); }
  .legend { position: absolute; z-index: 2; right: var(--space-4); bottom: var(--space-4); margin: 0; padding: var(--space-1-5) var(--space-3); list-style: none; display: flex; flex-wrap: wrap; gap: var(--space-1) var(--space-4);
    background: var(--map-surface); border: 1px solid var(--map-line); border-radius: var(--radius-md); font-size: var(--text-sm); color: var(--map-muted); max-width: calc(100% - 2rem); box-sizing: border-box; }
  .legend li { display: flex; align-items: center; gap: var(--space-1-5); }
  .legend .sw { width: 1.125rem; height: 0.75rem; border-radius: var(--radius-sm); box-sizing: border-box; background: var(--map-surface); }
  .legend svg { width: 1.625rem; height: 0.5rem; }
  .legend .sw.changed { border: 2px solid var(--color-changed); }
  .legend .sw.added { border: 2px solid var(--color-added); }
  .legend .sw.removed { border: 1px dashed var(--color-removed); }
  .legend .w-added { stroke: var(--color-added); stroke-width: 2; }
  .legend .w-ghost { stroke: var(--color-removed); stroke-width: 1.5; stroke-dasharray: 5 4; }
  .arrow { display: contents; }
  .sleeves { position: absolute; left: 0; top: 0; overflow: visible; pointer-events: none; fill: none; stroke: var(--map-line-soft); stroke-linecap: round; }
  .inside { position: absolute; left: 0; top: 0; transform-origin: 0 0; pointer-events: none; }
  .inside > .slot { pointer-events: auto; }
  /* app.css's global .row (list rows) must not restyle the boxes' title row */
  .map :global(.row) { padding: 0; background: none; cursor: inherit; }
  .reading { position: absolute; left: 50%; bottom: var(--space-4); transform: translateX(-50%); margin: 0; padding: var(--space-1-5) var(--space-3-5);
    border: 1px dotted var(--map-gold); border-radius: var(--radius-pill); background: var(--map-surface); font-size: var(--text-md); color: var(--map-fg-2); max-width: calc(100% - 2rem); }
  .reading.lifted { bottom: 4.5rem; }
  .reading code { background: var(--map-sunken); }
</style>
