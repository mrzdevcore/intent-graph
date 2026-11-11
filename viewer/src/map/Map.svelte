<!-- The walkable map. One world <div> under the camera; each box whose screen width reaches
     showsInside mounts its inside (children, regions, wires in its inner units), recursively.
     Only mounted levels are in the DOM. Dragging a box moves it inside its parent (kept per map
     in localStorage; others make room, the parent grows); dragging elsewhere, or an open box, pans, or draws a marquee when editing. Map text is repo/AI-derived: text only, never {@html}. -->
<script lang="ts">
  import { onDestroy, tick as flushed, untrack } from "svelte";
  import type { ArrowShape, Edit, Frame, MapArrow as Arrow, MapBox as Box, Region as R, View } from "./types";
  import { HEAD, detailFor, fitInner, fitTo, placer, showsInside, type Cam } from "./camera";
  import { arrowLook, diffLook, insideCounts, linksOf, look } from "./looks";
  import { Camera, PLACE_MS, tween, wheelFactor } from "./glide";
  import { GRID, edgePan, gridLevel, momentum, softK, wheelIntent, type WheelPref } from "./nav";
  import { wires, type Mode, type Wire } from "./wires";
  import { lineOf, mapShape, type MapShape } from "./shapes/auto";
  import MapBox from "./MapBox.svelte";
  import { shapeOf } from "./shapes";
  import MapArrow from "./MapArrow.svelte";
  import Region from "./Region.svelte";
  import Pads from "./Pads.svelte";
  import { ANSWER_CARD, CARD, CARD_WIDE, fillsBeside, fitBeside, padsOf, type Outside } from "./pads";
  import { answerFrame, answerOf, arrowLit, arrowNumber, boxStates, holders, stepFrame, type Aim } from "./answer";
  import AnswerCard from "./AnswerCard.svelte";
  import type { Show } from "../model/show";
  import type { Diff } from "../model/diff";
  import { diffAnswer, diffLines, files } from "./diff";
  import { key as arrowKey } from "./answer";
  import DetailsCard from "./DetailsCard.svelte";
  import Controls from "./Controls.svelte";
  import type { Notation } from "./db/ends";
  import { loadNotation, saveNotation } from "./db/notation";
  import { applyNoise, foldNoise, loadNoise, saveNoise, type Noise } from "./db/noise";
  import Search from "./Search.svelte";
  import type { Trace } from "./search";
  import type { EditApi } from "../api";
  import Island from "./edit/Island.svelte";
  import Corner from "./edit/Corner.svelte";
  import Draft from "./edit/Draft.svelte";
  import { ToolState, type Tool } from "./edit/tool.svelte";
  import AddBox from "./edit/AddBox.svelte";
  import { freeId, inside, kindOf } from "./edit/add";
  import BoxBar from "./edit/BoxBar.svelte";
  import ArrowBar from "./edit/ArrowBar.svelte";
  import BoxPanel from "./edit/BoxPanel.svelte";
  import ArrowPanel from "./edit/ArrowPanel.svelte";
  import ArrowMenu from "./edit/ArrowMenu.svelte";
  import ArrowText from "./edit/ArrowText.svelte";
  import { arrowKindNames, specKind } from "./edit/arrows";
  import { arrowBar as barOf, arrowHit, partFrame } from "./edit/arrowPick";
  import { kindName } from "./edit/kinds";
  import { sharedParent, written } from "./edit/menu";
  import EditLayer from "./edit/EditLayer.svelte";
  import { EditSession, type Draft as DraftBox } from "./edit/session.svelte";
  import type { Act, Clipped } from "./edit/menu";
  import { copy, fromKdl, paste, remember, remembered, toKdl, type Clip, type Copy } from "./edit/clipboard";
  import Proposals from "./edit/Proposals.svelte";
  import Warnings from "./Warnings.svelte";
  import Folders from "./Folders.svelte";
  import { proposed } from "./edit/proposals";
  import { Places, inside as innerKey, isInside, type Place } from "./place/places";
  import { arrange } from "./place/arrange";
  import { rekey, reparented, settle as settlePlaces, shownView } from "./place/rules";
  import { Clock, EditLog, PlaceHistory, apply, changes, redoNext, renamesOf, undoNext, type Rename } from "./place/history";
  import { cardFacts } from "./db/card";
  import Focus from "./db/Focus.svelte";
  import { hasTables as storeHasTables, storeOf } from "./db/focus";

  let { view: given, camera = new Camera(), size, edit, onview, show, diff }: {
    view: View; camera?: Camera; size?: { w: number; h: number };
    /** Only where the host can edit (the desktop app); `onview` takes the view an edit returns. */
    edit?: EditApi; onview?: (v: View) => void;
    show?: Show | null;
    diff?: Diff | null;
  } = $props();
  let noise = $state<Noise>(untrack(() => loadNoise()));
  const folded = $derived(foldNoise(given, noise));
  const view = $derived(applyNoise(given, noise));
  const homes = $derived(new Map(given.boxes.map((b) => [b.id, b.parent])));
  const shifted = (v: View) => v.boxes.filter((b) => homes.get(b.id) !== b.parent).map((b) => b.id);
  const aside = () => {
    const drawn = new Set(view.boxes.map((b) => b.id)), moved = new Set(shifted(view));
    const bare = (k: string) => (isInside(k) ? k.slice(0, -1) : k);
    return Object.fromEntries(Object.entries(store.snapshot()).filter(([k]) => homes.has(bare(k)) && (!drawn.has(bare(k)) || moved.has(bare(k)))));
  };
  const unshifted = (places: Record<string, Place>, v: View) => {
    const out = { ...places };
    for (const id of shifted(v)) delete out[id];
    return out;
  };
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
  const clock = new Clock(), edits = new EditLog(clock), history = new PlaceHistory(clock);
  // the ig/ edits a run applies: only those are on the server's undo
  let applied: Edit[] = [];
  let renaming: Rename[] = [];
  const api = $derived.by((): EditApi | undefined => {
    const e = edit;
    return e && {
      apply: (x) => (applied.push(x), e.apply(x)), keep: (n) => e.keep(n), drop: (n) => e.drop(n), dropAll: () => e.dropAll(), undo: () => e.undo(), redo: () => e.redo(),
    };
  });
  const run = (call: () => Promise<View>, as: "do" | "undo" | "redo" = "do"): boolean => {
    if (busy) return (toast("Still saving the last change; try again"), false);
    busy = true;
    applied = [];
    const moved = as === "undo" ? edits.undo() : as === "redo" ? edits.redo() : null;
    const sent = call(), made = applied;
    const stamp = made.length ? edits.begin() : 0;
    sent.then((v) => {
      if (moved) renaming = moved.renames;
      else if (made.length) edits.push((renaming = renamesOf(made)), stamp || undefined);
      onview?.(v);
    }, (e) => (moved?.failed(), toast(e))).finally(() => (busy = false));
    return true;
  };
  const typing = (t: EventTarget | null) => {
    const el = t as HTMLElement | null;
    return !!el && (/^(INPUT|TEXTAREA|SELECT)$/.test(el.tagName) || el.isContentEditable || !!el.closest?.("[contenteditable]"));
  };

  const ROOT = "system";
  // each box's place in its parent's inner units, kept per map: once drawn, a box stays where it is
  const title = $derived(view.boxes.find((b) => b.id === ROOT)?.title.value ?? "");
  let store = untrack(() => new Places(title, view.placed.frames));
  let truth = $state.raw<Record<string, Place>>(untrack(() => settlePlaces(unshifted(store.snapshot(), view), view, {}, null)));
  let lastView = untrack(() => view);
  // the places of boxes since removed: an undone delete puts them back
  const gone: Record<string, Place> = {};
  let anim = $state.raw<{ from: Record<string, Place>; start: number; t: number } | null>(null);
  const showing = $derived(anim ? tween(anim.from, truth, anim.t) : truth);
  let placeRaf = 0;
  function stepPlaces(now: number) {
    if (anim) anim = now - anim.start >= PLACE_MS ? null : { ...anim, t: now - anim.start };
  }
  function placeFrame(now: number) {
    placeRaf = 0;
    stepPlaces(now);
    if (anim) placeRaf = requestAnimationFrame(placeFrame);
  }
  function glidePlaces(from: Record<string, Place>, instant: string[] = []) {
    const f = { ...from };
    for (const id of instant) if (truth[id]) f[id] = truth[id];
    anim = { from: f, start: performance.now(), t: 0 };
    if (!placeRaf) placeRaf = requestAnimationFrame(placeFrame);
  }
  function placeAll(next: Record<string, Place>, instant: string[] = [], record: "push" | "merge" | false = "push") {
    const c = changes(truth, next);
    if (!c) return;
    if (record) history[record](c.before, c.after);
    const from = showing;
    truth = next;
    store.replace({ ...aside(), ...unshifted(next, view) });
    glidePlaces(from, instant);
  }
  // a confirmed draft: its box lands where and as big as it was drawn once the view holds it
  // `also`: the places of what lands with it (a new group's inside and the boxes moved into it)
  // `pick`: the boxes to select once they land (a paste)
  let landing: { id: string; place?: Place; kinds?: boolean; rename?: boolean; also?: Record<string, Place>; pick?: string[] } | null = null;
  // a new view keeps what was drawn; settle finds room for what is new before it is drawn
  $effect.pre(() => {
    const v = view, t = title;
    untrack(() => {
      if (v === lastView) return;
      const renames = renaming;
      renaming = [];
      if (t !== store.system) {
        store = new Places(t, v.placed.frames);
        truth = settlePlaces(unshifted(store.snapshot(), v), v, {}, null);
        anim = null;
        lastView = v;
        return;
      }
      const ids = new Set(v.boxes.map((b) => b.id));
      const prev = rekey({ ...gone, ...truth }, renames, (id) => ids.has(id));
      for (const id of reparented(lastView, v)) delete prev[id];
      const back: Record<string, Place> = {};
      for (const b of v.boxes) if (!truth[b.id] && gone[b.id] && prev[b.id]) back[b.id] = prev[b.id];
      lastView = v;
      const made = landing && ids.has(landing.id) ? landing : null;
      landing = null;
      const carried: Record<string, Place> = {};
      for (const [k, p] of Object.entries(made?.also ?? {})) if (isInside(k)) prev[k] = p; else if (ids.has(k)) carried[k] = p;
      const own = made?.place ? { [made.id]: made.place } : {};
      const next = settlePlaces(prev, v, { ...back, ...carried, ...own }, made?.place ? made.id : null);
      for (const [id, p] of Object.entries(truth)) if (!(id in next)) gone[id] = p;
      placeAll(next, [...Object.keys(own), ...Object.keys(carried)], false);
      if (made?.pick) choose(made.pick.filter((id) => ids.has(id)));
      if (made?.kinds) {
        selected = made.id;
        sess.kinds = [made.id];
      }
      if (made?.rename) {
        many = [];
        selected = made.id;
        sess.ui = { t: "rename", id: made.id, ...sess.at(made.id) };
      }
    });
  });
  let lastStep: { ids: string; at: number } | null = null;
  // a parent's inner origin: places are kept from it, its children are drawn from 0
  const origin = (p: string) => (p !== ROOT && truth[innerKey(p)]) || { x: 0, y: 0 };
  type At = { x: number; y: number; w?: number; h?: number };
  function putBoxes(at: Record<string, At>, step = false): boolean {
    const ids = Object.keys(at).filter((id) => truth[id] && id !== ROOT);
    if (!ids.length) return false;
    const key = ids.join(" "), now = performance.now(), merge = step && lastStep?.ids === key && now - lastStep.at < 1000;
    lastStep = step ? { ids: key, at: now } : null;
    const frames = Object.fromEntries(ids.map((id) => {
      const o = origin(byId.get(id)?.parent ?? ROOT), f = truth[id], a = at[id];
      return [id, { x: a.x + o.x, y: a.y + o.y, w: a.w ?? f.w, h: a.h ?? f.h }];
    }));
    placeAll(settlePlaces(truth, view, frames, ids[0]), ids, merge ? "merge" : "push");
    return true;
  }
  function undo() {
    if (undoNext(edits, history) === "place") {
      const c = history.undo();
      if (c) placeAll(apply(truth, c), [], false);
    } else if (edit) run(() => edit.undo(), "undo");
  }
  function redo() {
    const next = redoNext(edits, history);
    if (next === "place") {
      const c = history.redo();
      if (c) placeAll(apply(truth, c), [], false);
    } else if (next === "edit" && edit) run(() => edit.redo(), "redo");
  }
  let tidied = $state<{ say: string; stamp?: number } | null>(null);
  let tidyTimer: ReturnType<typeof setTimeout> | undefined;
  onDestroy(() => clearTimeout(tidyTimer));
  function arrangeIn(scope: string[]) {
    const laid = arrange(scope, view, truth, GRID, vw > 0 && vh > 0 ? { w: vw, h: vh } : undefined);
    // a selection laid out as a block may leave its parent: the parent grows
    const next = scope.length > 1 ? settlePlaces(laid, view, Object.fromEntries(scope.filter((id) => laid[id]).map((id) => [id, laid[id]])), null) : laid;
    const c = changes(truth, next);
    placeAll(next);
    const moved = c ? Object.keys(c.after).filter((k) => !isInside(k)) : [];
    // only boxes at the level on screen move visibly; deeper moves are told as groups
    const level = new Set(scope.length > 1 ? scope : (kids.get(scope[0]) ?? []).map((b) => b.id));
    const seen = moved.filter((id) => level.has(id)).length, groups = new Set(moved.filter((id) => !level.has(id)).map((id) => byId.get(id)?.parent ?? ROOT)).size;
    tidied = {
      say: seen ? `Arranged ${seen} ${seen === 1 ? "box" : "boxes"}` : groups ? `Arranged inside ${groups} ${groups === 1 ? "group" : "groups"}` : "Already reads well",
      stamp: c ? history.last : undefined,
    };
    clearTimeout(tidyTimer);
    tidyTimer = setTimeout(() => (tidied = null), 6000);
  }
  // what is open on screen: the deepest box of the breadcrumb that holds boxes
  const arrangeMap = () => arrangeIn([[...trail].reverse().find((id) => arrangeable.has(id)) ?? ROOT]);
  function undoArrange() {
    if (tidied?.stamp !== undefined && history.last === tidied.stamp && undoNext(edits, history) === "place") undo();
    tidied = null;
  }
  const shown = $derived(shownView(view, showing));
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
  // a box with a pattern keeps the inside its rings were drawn around
  const arrangeable = $derived(new Set([...kids.keys()].filter((id) => (byId.get(id)?.pattern?.value.kind ?? "none") === "none")));
  // a component's part boxes (inputs, state, uses, renders): its card sums them up
  const anatomies = $derived(group(view.boxes.filter((b) => b.kind.value === "parts" && !!b.parent), (b: Box) => b.parent!));
  // world frames, once per layout (linear in the boxes): a drag in flight leaves them be
  const place = $derived(placer(shown));
  const world = $derived(new Map(framed.map((b) => [b.id, place.frame(b.id)])));
  const area = (f: Frame) => f.w * f.h;
  const regions = $derived(group([...view.placed.regions].sort((a, b) => area(b.frame) - area(a.frame)), (r: R) => r.parent));
  /** Each parent's wires both ways, from its children's frames as drawn. */
  const kindById = $derived(Object.fromEntries(view.boxes.map((b) => [b.id, b.kind.value])));
  const wired = (as: Arrow[], frames: Record<string, Frame>, p: string) => {
    const map = shapeAt.get(p) ?? "circuit";
    return { routed: wires(as, frames, "routed", kindById, (a) => lineOf(a, map)), ribbon: wires(as, frames, "ribbon") };
  };
  const siblings = (p: string) => Object.fromEntries((kids.get(p) ?? []).map((b) => [b.id, shown.placed.frames[b.id]]));
  const byParent = $derived(group([...view.arrows, ...ghosts], (a: Arrow) => a.parent));
  const shapeAt = $derived.by(() => {
    const out = new Map<string, ArrowShape>();
    for (const [p, bs] of kids) {
      const at = new Set(bs.map((b) => b.id)), n = (byParent.get(p) ?? []).filter((a) => !a.ghost && a.from !== a.to && at.has(a.from) && at.has(a.to)).length;
      out.set(p, mapShape(arrowPref, bs.length, n));
    }
    return out;
  });
  const wiring = $derived(new Map([...byParent].map(([p, as]) => [p, wired(as, siblings(p), p)])));
  const arrows = $derived(group([...view.arrows, ...ghosts].filter((a) => wiring.get(a.parent)?.routed.wires.has(a.id)), (a: Arrow) => a.parent));
  // the box being dragged and its shift so far: only it and its parent's arrows follow the pointer
  // `with`: the other selected boxes of its parent, moved along
  let live = $state<{ id: string; x: number; y: number; w?: number; h?: number; with?: Record<string, { x: number; y: number }> } | null>(null);
  const drawn = (id: string) => {
    const f = shown.placed.frames[id], o = live?.with?.[id];
    return live?.id === id ? { x: live.x, y: live.y, w: live.w ?? f.w, h: live.h ?? f.h } : o ? { ...f, ...o } : f;
  };
  const liveWiring = $derived.by(() => {
    if (!live) return null;
    const p = byId.get(live.id)?.parent ?? ROOT, frames = siblings(p);
    for (const id of [live.id, ...Object.keys(live.with ?? {})]) frames[id] = drawn(id);
    return { p, ...wired(arrows.get(p) ?? [], frames, p) };
  });
  const wiringOf = (p: string) => (liveWiring?.p === p ? liveWiring : wiring.get(p));
  const outsideOf = (id: string): Outside => ({ frame: drawn(id), wire: (a) => wiringOf(a.parent)?.routed.wires.get(a.id) });
  const pathOf = (a: Arrow, mode: Mode) => wiringOf(a.parent)![mode].wires.get(a.id)!;
  const lineAt = (a: Arrow) => lineOf(a, shapeAt.get(a.parent) ?? "circuit");
  const FADE = 240;
  let fading = $state.raw<{ a: Arrow; w: Wire }[]>([]);
  let fresh = $state.raw(new Set<string>());
  let fadeTimer: ReturnType<typeof setTimeout> | undefined;
  let lastLines: { lines: Map<string, string>; wiring: typeof wiring } | null = null;
  onDestroy(() => clearTimeout(fadeTimer));
  $effect(() => {
    const now = new Map([...arrows.values()].flat().map((a) => [a.id, lineAt(a)])), w = wiring;
    untrack(() => {
      const was = lastLines;
      lastLines = { lines: now, wiring: w };
      if (!was) return;
      const out: { a: Arrow; w: Wire }[] = [];
      for (const as of arrows.values())
        for (const a of as) {
          const old = was.lines.get(a.id), line = old && old !== now.get(a.id) ? was.wiring.get(a.parent)?.routed.wires.get(a.id) : undefined;
          if (line) out.push({ a, w: line });
        }
      if (!out.length) return;
      fading = out;
      fresh = new Set(out.map((f) => f.a.id));
      clearTimeout(fadeTimer);
      fadeTimer = setTimeout(() => ((fading = []), (fresh = new Set())), FADE);
    });
  });
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
  const cards = $derived(cardFacts(folded));
  const reading = $derived(view.boxes.some((b) => b.needsReading));

  let cw = $state(0), ch = $state(0);
  const vw = $derived(size?.w ?? cw), vh = $derived(size?.h ?? ch);
  let cam = $state<Cam>(untrack(() => camera.cam));
  // detail and wires are worked out from this camera; it trails `cam` while a wheel or drag gesture runs
  let lod = $state<Cam>(untrack(() => camera.cam));
  let selected = $state<string | null>(null);
  // several boxes selected, in the order they were picked; `selected` is then null
  let many = $state<string[]>([]);
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
  let hotPad = $state<string[] | null>(null);
  const touches = (a: Arrow) => a.id === hotArrow || [hot, selected].some((id) => id && linksOf([a], id).length > 0);
  let pointer = { x: 0, y: 0 }, onMap = false;
  function over(e: PointerEvent) {
    const el = (e.target as Element | null)?.closest?.("[data-box]") as HTMLElement | null;
    hot = el?.dataset.box ?? null;
    pointer = { x: e.clientX, y: e.clientY };
    onMap = true;
  }
  let stage: HTMLDivElement;
  let root = $state<HTMLDivElement>();

  const home = () => fitTo(place.frame(ROOT), { w: vw, h: vh });
  const frameOf = (id: string) => world.get(id) ?? place.frame(id);
  const opened = (id: string) => kids.has(id) && !!view.placed.inner[id] && showsInside((world.get(id)?.w ?? 0) * lod.k);

  // rAF-driven camera: runs only while a glide is in flight.
  let raf = 0;
  function frame(now: number) {
    raf = 0;
    cam = lod = camera.tick(now);
    if (camera.moving) raf = requestAnimationFrame(frame);
  }
  function glideTo(c: Cam, ms?: number) {
    away = !!current || showingDiff;
    coast = null;
    camera.to(c, performance.now(), ms);
    if (!raf) raf = requestAnimationFrame(frame);
  }
  function jump(c: Cam) {
    away = !!current || showingDiff;
    camera.set(c);
    cam = lod = c;
  }
  const SETTLE = 120;
  let settleTimer: ReturnType<typeof setTimeout> | undefined;
  let anchorPt = { x: 0, y: 0 };
  function gesture(c: Cam) {
    away = !!current || showingDiff;
    camera.set(c);
    cam = c;
    clearTimeout(settleTimer);
    settleTimer = setTimeout(settle, SETTLE);
  }
  function settle() {
    lod = cam;
    const [lo, hi] = bounds;
    if (cam.k < lo || cam.k > hi) zoomAt(anchorPt.x, anchorPt.y, clampTo(cam.k, lo, hi) / cam.k, true);
  }
  const clampTo = (k: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, k));
  const bounds = $derived.by((): [number, number] => {
    const lo = home().k / 4;
    let w = Infinity;
    for (const f of world.values()) if (f.w > 0) w = Math.min(w, f.w);
    return [lo, Math.min(1e9, Number.isFinite(w) && vw > 0 ? Math.max(lo, vw / w) : 1e9)];
  });
  let coast: { x: number; y: number } | null = null;
  let coastRaf = 0;
  function stopCoast() {
    coast = null;
    cancelAnimationFrame(coastRaf);
  }
  function startCoast(v: { x: number; y: number }) {
    stopCoast();
    coast = v;
    let last = performance.now();
    const step = (now: number) => {
      const dt = Math.min(64, now - last);
      last = now;
      if (!coast) return;
      const c = camera.cam;
      gesture({ k: c.k, x: c.x + coast.x * dt, y: c.y + coast.y * dt });
      coast = momentum(coast, dt);
      if (coast.x || coast.y) coastRaf = requestAnimationFrame(step);
      else coast = null;
    };
    coastRaf = requestAnimationFrame(step);
  }
  onDestroy(() => {
    clearTimeout(settleTimer);
    stopCoast();
  });
  /** Advance the camera to `now` (tests; the rAF loop does this in the page). */
  export function tick(now: number) {
    cam = lod = camera.tick(now);
    stepPlaces(now);
  }
  onDestroy(() => {
    cancelAnimationFrame(raf);
    cancelAnimationFrame(placeRaf);
    cancelAnimationFrame(edgeRaf);
  });

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
    const to = { w: vw, h: vh }, pads = padsOf(view, id, frameOf(id), drawn, on, frameOf, outsideOf(id));
    if (pads.length) return fitBeside(frameOf(id), to, pads, selected ? cardRem() : 0, rem());
    const inner = shown.placed.inner[id];
    if (!kids.has(id) || !inner) return fitTo(frameOf(id), to);
    const t = place.inside(id);
    return fitTo({ x: t.x, y: t.y, w: inner[0] * t.s, h: inner[1] * t.s }, to);
  }
  const fly = (id: string) => glideTo(id === ROOT ? home() : target(id));
  function aimAt(aim: Aim | null) {
    if (!aim) return;
    const pads = aim.id ? padsOf(view, aim.id, aim.frame, drawn, on, frameOf, outsideOf(aim.id)) : [];
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
    if (sess.arrow) sess.pickArrow(null);
    many = [];
    selected = id;
    fly(id);
  }
  let focusing = $state<{ store: string; table: string | null } | null>(null);
  function openFocus(id: string) {
    const store = storeOf(given, id);
    if (store && given.tables?.[id]) focusing = { store, table: id };
  }
  const hasTables = (id: string) => storeHasTables(view, id);
  function openTables(id: string) {
    if (hasTables(id)) focusing = { store: id, table: null };
  }
  function closeFocus() {
    const id = focusing?.table;
    focusing = null;
    if (id && byId.has(id)) sess.focusBox(id);
  }
  function close() {
    const id = selected;
    selected = null;
    if (id) sess.focusBox(id);
  }
  const tools: Record<string, Tool> = { v: "select", b: "box", a: "arrow", f: "frame" };
  let space = $state(false);
  const KEY_MS = 150, PAN_STEP = 96;
  function navKey(e: KeyboardEvent): boolean {
    const mod = e.metaKey || e.ctrlKey;
    if (mod && (e.key === "+" || e.key === "=" || e.key === "-" || e.key === "_" || e.key === "0")) {
      e.preventDefault();
      const c = camera.cam;
      if (e.key === "0") {
        const k = clampK(1), r = k / c.k;
        glideTo({ k, x: vw / 2 - (vw / 2 - c.x) * r, y: vh / 2 - (vh / 2 - c.y) * r }, KEY_MS);
      } else zoomAt(vw / 2, vh / 2, e.key === "-" || e.key === "_" ? 1 / 1.25 : 1.25, true);
      return true;
    }
    if (e.shiftKey && !mod && !e.altKey && e.code === "Digit1") {
      e.preventDefault();
      glideTo(home(), KEY_MS);
      return true;
    }
    if (e.shiftKey && !mod && !e.altKey && e.code === "Digit2") {
      e.preventDefault();
      if (selected) glideTo(fitTo(frameOf(selected), { w: vw, h: vh }), KEY_MS);
      return true;
    }
    const dir = { ArrowLeft: [1, 0], ArrowRight: [-1, 0], ArrowUp: [0, 1], ArrowDown: [0, -1] }[e.key];
    if (dir && !mod && !e.altKey) {
      if ((e.target as Element | null)?.closest?.("button, summary, a, select, [contenteditable], [role=menu], [role=listbox], [role=slider], [role=tablist]")) return false;
      e.preventDefault();
      const at: Record<string, At> = {};
      for (const id of chosen()) {
        const f = shown.placed.frames[id];
        if (f) at[id] = { x: f.x - dir[0] * GRID, y: f.y - dir[1] * GRID };
      }
      if (putBoxes(at, true)) return true;
      const c = camera.cam;
      glideTo({ k: c.k, x: c.x + dir[0] * PAN_STEP, y: c.y + dir[1] * PAN_STEP }, KEY_MS);
      return true;
    }
    return false;
  }
  function keyup(e: KeyboardEvent) {
    if (e.key !== " ") return;
    // a held Space pans: the focused box's button must not take the keyup as a click
    if (space && (e.target as Element | null)?.matches?.("[data-box] > button")) e.preventDefault();
    space = false;
  }
  const shortcuts: Record<string, Act> = { f2: "rename", m: "move", e: "details", o: "code", delete: "delete", backspace: "delete" };
  function key(e: KeyboardEvent) {
    if (focusing || typing(e.target)) return;
    if ((current || showingDiff) && !sess.ui && (e.key === "ArrowRight" || e.key === "ArrowLeft")) {
      goStep(step + (e.key === "ArrowRight" ? 1 : -1));
      return;
    }
    const onBox = !!(e.target as Element | null)?.matches?.("[data-box] > button");
    if (e.key === " " && (onBox || !(e.target as Element | null)?.closest?.("button, summary, a, [data-popover]"))) {
      e.preventDefault();
      space = true;
      return;
    }
    if (!sess.ui && !(e.target as Element | null)?.closest?.("[data-popover]")) {
      if (!e.metaKey && !e.ctrlKey && !e.altKey && !e.repeat && e.key.toLowerCase() === "h") tool.pick("hand");
      if (navKey(e)) return;
    }
    if (e.key === "Enter" && !sess.ui && selected && view.tables?.[selected] && !e.metaKey && !e.ctrlKey && !e.altKey) {
      const t = e.target as Element | null;
      if (!t?.closest?.("button, a, summary, select, [data-popover]") || t.matches?.("[data-box] > button")) {
        e.preventDefault();
        openFocus(selected);
        return;
      }
    }
    // a popover owns the keys while it is open or has focus
    const owned = e.repeat || sess.ui || !!(e.target as Element | null)?.closest?.("[data-popover]");
    if (!owned && (e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "z") {
      e.preventDefault();
      if (e.shiftKey) redo(); else undo();
      return;
    }
    if (!owned && e.altKey && e.shiftKey && !e.metaKey && !e.ctrlKey && e.code === "KeyF") {
      e.preventDefault();
      arrangeMap();
      return;
    }
    if (edit && e.key !== "Escape") {
      if (owned) return;
      const k = e.key.toLowerCase(), plain = !e.metaKey && !e.ctrlKey && !e.altKey;
      if (plain && sess.arrow && (k === "delete" || k === "backspace")) {
        e.preventDefault();
        sess.unarrow();
        return;
      }
      if (plain && many.length > 1 && (k === "delete" || k === "backspace" || (k === "a" && many.length === 2))) {
        e.preventDefault();
        if (k === "a") sess.connect(many[0], many[1]);
        else sess.remove(many);
        return;
      }
      const mod = (e.metaKey || e.ctrlKey) && !e.altKey;
      if (mod && (k === "c" || k === "d") && chosen().length && !(k === "c" && globalThis.getSelection?.()?.toString())) {
        e.preventDefault();
        if (k === "c") copyOut(chosen(), e.shiftKey);
        else duplicate(chosen(), e.shiftKey);
        return;
      }
      if (mod && k === "v" && clip) {
        clearTimeout(pasteTimer);
        pasteTimer = setTimeout(() => clip && canPaste(document.activeElement) && pasteHere(clip.copy), 100);
      }
      if (plain && selected && shortcuts[k] && (k !== "o" || hasCode(selected))) {
        e.preventDefault();
        sess.act(shortcuts[k], selected);
        return;
      }
      if (!e.metaKey && !e.ctrlKey && !e.altKey && tools[k]) tool.pick(tools[k]);
      if (!e.metaKey && !e.ctrlKey && !e.altKey && k === "k") kindFirst(pointer.x, pointer.y, hot && byId.has(hot) ? hot : anchor);
    }
    if (e.key !== "Escape") return;
    if (sess.asking) return void (sess.asking = null);
    tool.pick("select");
    if (drag?.draw || drag?.resize || drag?.mark || drag?.frame) {
      sess.release();
      live = null;
      drag = null;
    }
    const layer = !!(sess.arrow || sess.panel || sess.ui || sess.adding || sess.draft || sess.kinds || sess.made || sess.link || sess.end || sess.writing || sess.arrowKinds || marquee);
    sess.cancel();
    marquee = null;
    if (layer) return;
    if (many.length) many = [];
    else if (selected) close();
    else if (traced) traced = null;
    else clearAnswer();
  }
  // deep enough for the smallest code box of a deep tree
  const clampK = (k: number) => Math.min(1e9, Math.max(1e-3, k));
  function zoomAt(px: number, py: number, f: number, smooth = false) {
    const c = camera.cam, [lo, hi] = bounds, k = clampK(smooth ? clampTo(c.k * f, lo, hi) : softK(c.k * f, lo, hi)), r = k / c.k;
    const next = { k, x: px - (px - c.x) * r, y: py - (py - c.y) * r };
    anchorPt = { x: px, y: py };
    if (smooth) glideTo(next, KEY_MS); else gesture(next);
  }
  let wheelPref = $state<WheelPref>(untrack(() => loadPref()));
  function loadPref(): WheelPref {
    try {
      const v = globalThis.window?.localStorage?.getItem("ig-wheel");
      return v === "zoom" || v === "pan" ? v : "auto";
    } catch {
      return "auto";
    }
  }
  let arrowPref = $state<MapShape>(untrack(() => loadArrows()));
  function loadArrows(): MapShape {
    try {
      const v = globalThis.window?.localStorage?.getItem("ig-arrows");
      return v === "circuit" || v === "elbow" || v === "curved" || v === "straight" ? v : "auto";
    } catch {
      return "auto";
    }
  }
  function setArrows(p: MapShape) {
    arrowPref = p;
    try {
      if (p === "auto") globalThis.window?.localStorage?.removeItem("ig-arrows");
      else globalThis.window?.localStorage?.setItem("ig-arrows", p);
    } catch { /* private mode: the setting lasts the page */ }
  }
  function setPref(p: WheelPref) {
    wheelPref = p;
    try {
      if (p === "auto") globalThis.window?.localStorage?.removeItem("ig-wheel");
      else globalThis.window?.localStorage?.setItem("ig-wheel", p);
    } catch { /* private mode: the setting lasts the page */ }
  }

  let endsPref = $state<Notation>(untrack(() => loadNotation()));
  function setEnds(n: Notation) {
    endsPref = n;
    saveNotation(n);
  }
  function setNoise(k: keyof Noise, on: boolean) {
    noise = { ...noise, [k]: on };
    saveNoise(noise);
  }

  // Wheel needs a non-passive listener to stop the page scrolling.
  $effect(() => {
    const wheel = (e: WheelEvent) => {
      e.preventDefault();
      stopCoast();
      const r = stage.getBoundingClientRect();
      if (wheelIntent(e, wheelPref) === "zoom") return zoomAt(e.clientX - r.left, e.clientY - r.top, wheelFactor(e));
      const u = e.deltaMode === 1 ? 16 : e.deltaMode === 2 ? 400 : 1, c = camera.cam;
      const swap = e.shiftKey && e.deltaX === 0;
      gesture({ k: c.k, x: c.x - (swap ? e.deltaY : e.deltaX) * u, y: c.y - (swap ? 0 : e.deltaY) * u });
    };
    stage.addEventListener("wheel", wheel, { passive: false });
    return () => stage.removeEventListener("wheel", wheel);
  });

  // Drag pans, or moves the closed box under the pointer (`box`: its id, where it was at the start
  // and screen pixels per unit of its parent); a drag's trailing click must not open a box.
  let drag: {
    x: number; y: number; from: Cam; moved: boolean; pointer: number; pan?: boolean; pt?: { x: number; y: number }; track?: { x: number; y: number; t: number }[];
    box?: { id: string; at: { x: number; y: number }; k: number; with: Record<string, { x: number; y: number }> }; resize?: { id: string; corner: string; f: Frame; k: number }; draw?: { parent: string; x: number; y: number };
    mark?: { sx: number; sy: number; add: boolean }; frame?: { parent: string; x: number; y: number; sx: number; sy: number }; letGo?: boolean;
  } | null = null;
  let marquee = $state<{ x0: number; y0: number; x1: number; y1: number; frame: boolean } | null>(null);
  let swallow = false;
  const BOX = { w: 10 * GRID, h: 6 * GRID }, MIN = { w: 4 * GRID, h: 2 * GRID };
  const snap = (v: number) => Math.round(v / GRID) * GRID;
  function groundAt(e: { clientX: number; clientY: number; target: EventTarget | null }, parent?: string) {
    const el = (e.target as Element | null)?.closest?.("[data-box][data-open]") as HTMLElement | null;
    const p = parent ?? (el?.dataset.box && byId.has(el.dataset.box) ? el.dataset.box : anchor);
    const r = stage.getBoundingClientRect(), c = camera.cam, t = p === ROOT ? { x: 0, y: 0, s: 1 } : place.inside(p), o = origin(p);
    return { parent: p, x: ((e.clientX - r.left - c.x) / c.k - t.x) / t.s + o.x, y: ((e.clientY - r.top - c.y) / c.k - t.y) / t.s + o.y };
  }
  const draftAt = (g: { parent: string; x: number; y: number }, from?: string) => {
    sess.draft = { parent: g.parent, x: snap(g.x - BOX.w / 2), y: snap(g.y - BOX.h / 2), ...BOX, title: "", drawing: false, ...(from ? { from } : {}) };
  };
  function spanned(d: { parent: string; x: number; y: number }, e: PointerEvent) {
    const g = groundAt(e, d.parent), x = snap(Math.min(d.x, g.x)), y = snap(Math.min(d.y, g.y));
    return { parent: d.parent, x, y, w: Math.max(MIN.w, snap(Math.max(d.x, g.x)) - x), h: Math.max(MIN.h, snap(Math.max(d.y, g.y)) - y), title: "" };
  }
  function down(e: PointerEvent) {
    swallow = false;
    stopCoast();
    if (e.button === 1) e.preventDefault();
    // picking or grabbing an arrow unmounts the field before its own blur would keep the text
    (document.activeElement as HTMLElement | null)?.closest?.<HTMLElement>("[data-arrow-text], [data-panel] [data-field]")?.blur();
    // a pad is only ever clicked: it is a button on the box's edge, not a grip on the box
    if ((e.button !== 0 && e.button !== 1) || (e.target as Element | null)?.closest?.("[data-pad], [data-draft]")) return;
    const panning = e.button === 1 || space || tool.current === "hand";
    const grip = !panning && edit ? (e.target as Element | null)?.closest?.("[data-resize]") as HTMLElement | null : null;
    const gripped = grip && (grip.closest("[data-box]") as HTMLElement | null)?.dataset.box;
    if (gripped && shown.placed.frames[gripped]) {
      sess.grab(e.pointerId);
      drag = { x: e.clientX, y: e.clientY, from: camera.cam, moved: false, pointer: e.pointerId,
        resize: { id: gripped, corner: grip!.dataset.resize!, f: shown.placed.frames[gripped], k: (unit.get(byId.get(gripped)?.parent ?? ROOT) ?? 1) * camera.cam.k } };
      return;
    }
    if (e.button === 0 && !space && sess.startEnd(e)) return;
    const hit = e.button === 0 && !space && edit && tool.current === "select" ? arrowHit(e) : undefined;
    if (hit) return void sess.pickArrow({ id: hit });
    if (e.button === 0 && !space && sess.startLink(e, tool.current === "arrow")) return;
    drag = { x: e.clientX, y: e.clientY, from: camera.cam, moved: false, pointer: e.pointerId, letGo: !panning && !!sess.arrow };
    if (!panning && edit && tool.current === "box") {
      drag.draw = groundAt(e);
      return;
    }
    if (!panning && edit && tool.current === "frame") {
      const p = sess.rel(e);
      drag.frame = { ...groundAt(e), sx: p.x, sy: p.y };
      return;
    }
    const el = (e.target as Element | null)?.closest?.("[data-box]") as HTMLElement | null;
    const id = el && !el.hasAttribute("data-open") ? el.dataset.box : undefined;
    if (!id && !panning && edit && tool.current === "select") {
      const p = sess.rel(e);
      drag.mark = { sx: p.x, sy: p.y, add: e.shiftKey };
      return;
    }
    if (panning || !id) {
      drag.pan = true;
      drag.track = [];
      return;
    }
    const b = id ? byId.get(id) : undefined;
    const f = b && shown.placed.frames[b.id];
    if (b && f) {
      const along = many.includes(b.id) ? many.filter((m) => m !== b.id && byId.get(m)?.parent === b.parent && shown.placed.frames[m]) : [];
      const at = (m: string) => ({ x: shown.placed.frames[m].x, y: shown.placed.frames[m].y });
      drag.box = { id: b.id, at: at(b.id), k: (unit.get(b.parent ?? ROOT) ?? 1) * camera.cam.k, with: Object.fromEntries(along.map((m) => [m, at(m)])) };
    }
  }
  function placeBox() {
    const b = drag?.box;
    if (!drag || !b || !drag.pt || b.k <= 0) return;
    const dx = drag.pt.x - drag.x - (camera.cam.x - drag.from.x), dy = drag.pt.y - drag.y - (camera.cam.y - drag.from.y);
    const by = (p: { x: number; y: number }) => ({ x: p.x + dx / b.k, y: p.y + dy / b.k });
    live = { id: b.id, ...by(b.at), with: Object.fromEntries(Object.entries(b.with).map(([m, p]) => [m, by(p)])) };
  }
  const EDGE_SPEED = 0.5;
  let edgeRaf = 0;
  function edgeLoop() {
    if (edgeRaf) return;
    let last = performance.now();
    const step = (now: number) => {
      edgeRaf = 0;
      const pt = drag?.pt, r = stage?.getBoundingClientRect();
      if (!drag?.box || !pt || !r) return;
      const v = edgePan({ x: pt.x - r.left, y: pt.y - r.top }, { w: vw, h: vh }, 3 * rem()), dt = Math.min(64, now - last);
      last = now;
      if (v.x || v.y) {
        const c = camera.cam;
        gesture({ k: c.k, x: c.x + v.x * EDGE_SPEED * dt, y: c.y + v.y * EDGE_SPEED * dt });
        placeBox();
      }
      edgeRaf = requestAnimationFrame(step);
    };
    edgeRaf = requestAnimationFrame(step);
  }
  function resized(r: { corner: string; f: Frame; k: number }, dx: number, dy: number) {
    const { f, corner } = r;
    const w = Math.max(MIN.w, snap(f.w + (corner.includes("e") ? dx : -dx) / r.k)), h = Math.max(MIN.h, snap(f.h + (corner.includes("s") ? dy : -dy) / r.k));
    return { x: corner.includes("w") ? f.x + f.w - w : f.x, y: corner.includes("n") ? f.y + f.h - h : f.y, w, h };
  }
  function move(e: PointerEvent) {
    pointer = { x: e.clientX, y: e.clientY };
    if (sess.moveLink(e) || sess.moveEnd(e) || !drag) return;
    const dx = e.clientX - drag.x, dy = e.clientY - drag.y;
    if (!drag.moved && Math.hypot(dx, dy) < 4) return;
    if (!drag.moved) sess.grab(drag.pointer);
    drag.moved = true;
    const b = drag.box;
    if (drag.resize && drag.resize.k > 0) live = { id: drag.resize.id, ...resized(drag.resize, dx, dy) };
    else if (drag.draw) sess.draft = { ...spanned(drag.draw, e), drawing: true };
    else if (drag.mark || drag.frame) {
      const o = (drag.mark ?? drag.frame)!, p = sess.rel(e);
      marquee = { x0: o.sx, y0: o.sy, x1: p.x, y1: p.y, frame: !!drag.frame };
    }
    else if (b && b.k > 0) {
      drag.pt = { x: e.clientX, y: e.clientY };
      placeBox();
      edgeLoop();
    } else if (drag.pan) {
      const now = performance.now();
      drag.track = [...(drag.track ?? []).filter((p) => now - p.t < 80), { x: e.clientX, y: e.clientY, t: now }];
      gesture({ k: drag.from.k, x: drag.from.x + dx, y: drag.from.y + dy });
    }
  }
  function up(e: PointerEvent) {
    if (sess.endLink(e) || sess.endEnd(e)) {
      swallow = true;
      return;
    }
    sess.release();
    if (drag?.letGo && !drag.moved) sess.pickArrow(null);
    swallow = !!drag?.moved;
    const d = drag?.draw;
    if (d) {
      if (drag!.moved) sess.draft = { ...spanned(d, e), drawing: false };
      else draftAt(d);
      tool.used(e.shiftKey);
      swallow = true;
      drag = null;
      return;
    }
    if (drag?.mark || drag?.frame) {
      const { mark, frame: fr, moved } = drag, m = marquee;
      marquee = null;
      drag = null;
      swallow = true;
      if (fr) {
        if (moved) {
          const r = spanned(fr, e), within = (p: Place) => p.x >= r.x && p.y >= r.y && p.x + p.w <= r.x + r.w && p.y + p.h <= r.y + r.h;
          makeGroup(fr.parent, (kids.get(fr.parent) ?? []).filter((b) => b.level !== "code" && truth[b.id] && within(truth[b.id])).map((b) => b.id), r);
        }
        tool.used(e.shiftKey);
      } else if (moved && m) pickIn(m, mark!.add);
      else if (!mark!.add) choose([]);
      return;
    }
    const tr = drag?.pan ? drag.track : undefined;
    if (tr && tr.length > 1) {
      const a = tr[0], b = tr[tr.length - 1], dt = b.t - a.t;
      if (dt > 0 && performance.now() - b.t < 50) startCoast(momentum({ x: (b.x - a.x) / dt, y: (b.y - a.y) / dt }, 0));
    }
    const held = live;
    live = null;
    if (held) putBoxes({ ...held.with, [held.id]: held });
    cancelAnimationFrame(edgeRaf);
    edgeRaf = 0;
    drag = null;
  }
  // a cancelled pointer, or the window losing focus mid-gesture, leaves nothing half done
  function abort() {
    space = false;
    stopCoast();
    cancelAnimationFrame(edgeRaf);
    edgeRaf = 0;
    sess.dropLink();
    live = null;
    drag = null;
  }
  const chosen = () => (many.length ? many : selected ? [selected] : []);
  function choose(ids: string[]) {
    if (ids.length && sess.arrow) sess.pickArrow(null);
    if (ids.length > 1) [many, selected] = [ids, null];
    else [many, selected] = [[], ids[0] ?? null];
  }
  function pickIn(m: { x0: number; y0: number; x1: number; y1: number }, add: boolean) {
    const l = Math.min(m.x0, m.x1), r = Math.max(m.x0, m.x1), t = Math.min(m.y0, m.y1), b = Math.max(m.y0, m.y1), c = cam;
    const hit = [...stage.querySelectorAll<HTMLElement>("[data-box]")].map((el) => el.dataset.box!).filter((id) => {
      if (!byId.has(id) || id === ROOT) return false;
      const f = frameOf(id), x = f.x * c.k + c.x, y = f.y * c.k + c.y;
      return x >= l && y >= t && x + f.w * c.k <= r && y + f.h * c.k <= b;
    });
    const set = new Set(hit), top = hit.filter((id) => ![...trailOf(id)].some((p) => set.has(p)));
    const was = add ? chosen() : [];
    choose([...was, ...top.filter((id) => !was.includes(id))]);
  }
  function* trailOf(id: string) {
    for (let p = byId.get(id)?.parent, n = 0; p && n < 64; p = byId.get(p)?.parent, n++) yield p;
  }
  function toggle(id: string) {
    const was = chosen();
    choose(was.includes(id) ? was.filter((x) => x !== id) : [...was, id]);
  }
  const hasCode = (id: string) => { const b = byId.get(id); return !!b && b.level !== "code" && b.madeOf.length > 0; };
  function details(id: string) {
    choose([id]);
    if (edit && written(byId.get(id))) sess.panel = { box: id, focus: null };
    else void flushed().then(() => root?.querySelector<HTMLElement>("section.details")?.focus({ preventScroll: true }));
  }
  function kindContext(id: string) {
    const parent = byId.get(id)?.parent ?? ROOT;
    return { parentKinds: (kids.get(parent) ?? []).filter((b) => b.id !== id).map(kindName).filter((n) => n !== "box"), inRun: inside(parent, byId, view.kinds) };
  }
  function addInside(id: string) {
    const ps = view.boxes.filter((b) => b.parent === id && truth[b.id]).map((b) => truth[b.id]), o = origin(id);
    const x = ps.length ? Math.min(...ps.map((p) => p.x)) : o.x + GRID, y = ps.length ? Math.max(...ps.map((p) => p.y + p.h)) + GRID : o.y + GRID;
    openBox(id);
    sess.panel = { box: id, focus: null };
    draftAt({ parent: id, x: x + BOX.w / 2, y: y + BOX.h / 2 });
  }
  function closePanel() {
    const p = sess.panel;
    sess.panel = null;
    const to = p?.box ?? (p?.arrow && sess.picked?.from);
    // after the read-only card mounts, which takes the focus for itself
    if (to) void flushed().then(() => sess.focusBox(to));
  }
  function makeGroup(parent: string, ids: string[], rect?: Place) {
    const ps = ids.map((i) => truth[i]).filter(Boolean);
    if (!api || (!rect && !ps.length)) return;
    const id = freeId("group", new Set(view.boxes.map((b) => b.id)));
    let frame: Place, room: Place;
    if (rect) {
      const head = Math.min(HEAD, rect.h / 4);
      [frame, room] = [rect, { x: rect.x, y: rect.y + head, w: rect.w, h: rect.h - head }];
    } else {
      const l = Math.min(...ps.map((p) => p.x)) - GRID, t = Math.min(...ps.map((p) => p.y)) - GRID;
      const r = Math.max(...ps.map((p) => p.x + p.w)) + GRID, b = Math.max(...ps.map((p) => p.y + p.h)) + GRID;
      [frame, room] = [{ x: l, y: t - HEAD, w: r - l, h: b - t + HEAD }, { x: l, y: t, w: r - l, h: b - t }];
    }
    const add: Edit = { op: "add", parent, kind: "group", id, title: "Group", at: [], before: null };
    const change: Edit = ids.length ? { op: "batch", edits: [add, ...ids.map((i): Edit => ({ op: "move", id: i, parent: id, before: null }))] } : add;
    const was = landing;
    landing = { id, place: frame, rename: true, also: { [innerKey(id)]: room, ...Object.fromEntries(ids.filter((i) => truth[i]).map((i) => [i, truth[i]])) } };
    const sent = run(() => api!.apply(change).catch((err) => {
      if (landing?.id === id) landing = null;
      throw err;
    }));
    if (!sent) landing = was;
  }
  let clip = $state.raw<Clip | null>(remembered());
  function copyOut(ids: string[], inside: boolean) {
    const c = copy(ids, view, inside, truth);
    if (!c.boxes.length) return;
    const text = toKdl(c);
    clip = remember({ copy: c, text });
    try {
      globalThis.navigator?.clipboard?.writeText(text)?.catch(() => {});
    } catch { /* no clipboard: the copy stays in the page */ }
  }
  function pasteIn(c: Copy, parent: string, at: { x: number; y: number } | null) {
    if (!api || !c.boxes.length) return;
    const r = paste(c, parent, at, view, ROOT), first = r.ids[0];
    if (!first) return;
    const was = landing, { [first]: place, ...also } = r.places;
    landing = { id: first, place, also, pick: r.ids };
    const sent = run(() => api!.apply(r.batch).catch((err) => {
      if (landing?.id === first) landing = null;
      throw err;
    }));
    if (!sent) landing = was;
  }
  const cameFrom = (c: Copy) => (c.parent && byId.has(c.parent) ? c.parent : ROOT);
  // a closed box under the point is no target: the copy lands beside it, in the open box or group whose inside is there
  function pasteAtScreen(c: Copy, x: number, y: number, parent?: string) {
    const hit = document.elementFromPoint?.(x, y), g = groundAt({ clientX: x, clientY: y, target: hit && stage.contains(hit) ? hit : stage }, parent);
    pasteIn(c, g.parent, g);
  }
  function pasteHere(c: Copy) {
    if (!onMap) return pasteIn(c, cameFrom(c), null);
    pasteAtScreen(c, pointer.x, pointer.y);
  }
  const canPaste = (t: EventTarget | null) => !!edit && !typing(t) && !sess.ui && !(t as Element | null)?.closest?.("[data-popover]");
  // a webview with no Paste in its menu sends no paste event for Cmd+V: the copy in the page goes in after a moment
  let pasteTimer: ReturnType<typeof setTimeout> | undefined;
  onDestroy(() => clearTimeout(pasteTimer));
  function pasted(e: ClipboardEvent) {
    clearTimeout(pasteTimer);
    if (!canPaste(e.target)) return;
    const text = e.clipboardData?.getData("text/plain") ?? "";
    const c = clip && (!text || text === clip.text) ? clip.copy : text ? fromKdl(text, view.kinds) : null;
    if (!c) return;
    e.preventDefault();
    pasteHere(c);
  }
  function duplicate(ids: string[], inside: boolean) {
    const c = copy(ids, view, inside, truth);
    pasteIn(c, cameFrom(c), null);
  }
  function clipAct(a: Clipped, id: string, p: { x: number; y: number }) {
    const ids = chosen().includes(id) ? chosen() : [id];
    if (a === "copy" || a === "copyAll") copyOut(ids, a === "copyAll");
    else if (a === "duplicate" || a === "duplicateAll") duplicate(ids, a === "duplicateAll");
    else if (clip) {
      const r = stage.getBoundingClientRect();
      const open = !!stage.querySelector(`[data-box="${CSS.escape(id)}"][data-open]`);
      pasteAtScreen(clip.copy, r.left + p.x, r.top + p.y, open ? id : byId.get(id)?.parent ?? ROOT);
    }
  }
  function align(dir: "row" | "column") {
    const ids = many.filter((id) => truth[id]);
    if (ids.length < 2) return;
    const row = dir === "row", x0 = Math.min(...ids.map((id) => truth[id].x)), y0 = Math.min(...ids.map((id) => truth[id].y)), at: Record<string, Place> = {};
    let c = row ? x0 : y0;
    for (const id of [...ids].sort((a, b) => (row ? truth[a].x - truth[b].x : truth[a].y - truth[b].y))) {
      const p = truth[id];
      at[id] = row ? { ...p, x: c, y: y0 } : { ...p, x: x0, y: c };
      c += (row ? p.w : p.h) + 2 * GRID;
    }
    placeAll(settlePlaces(truth, view, at, null));
  }
  const sess = new EditSession({
    edit: () => api, view: () => view, box: (id) => byId.get(id), frame: frameOf, cam: () => cam, stage: () => stage,
    kinds: () => view.kinds, arrowKinds: () => view.arrowKinds,
    selected: () => selected, select: (id) => choose(id ? [id] : []), db: (id) => (view.tables?.[id] ? "focus" : hasTables(id) ? "tables" : undefined),
    openDb: (a, id) => (a === "focus" ? openFocus(id) : openTables(id)), tool: (t) => tool.pick(t), used: (shift) => tool.used(shift), run, arrange: arrangeIn,
    drop: (e, from) => {
      const hit = document.elementFromPoint?.(e.clientX, e.clientY);
      draftAt(groundAt({ clientX: e.clientX, clientY: e.clientY, target: hit && stage.contains(hit) ? hit : e.target }), from);
    },
    draftFrame: (): Frame | undefined => {
      const d: DraftBox | null = sess.draft;
      if (!d) return undefined;
      const t = d.parent === ROOT ? { x: 0, y: 0, s: 1 } : place.inside(d.parent), o = origin(d.parent);
      return { x: (d.x - o.x) * t.s + t.x, y: (d.y - o.y) * t.s + t.y, w: d.w * t.s, h: d.h * t.s };
    },
    ground: (e) => groundAt(e), draftAt: (g) => void draftAt(g), details, open: openBox,
    clip: (a, id, p) => clipAct(a, id, p), pasteAt: (g) => clip && pasteIn(clip.copy, g.parent, g),
  });
  const panelBox = $derived(edit && sess.panel?.box && sess.panel.box === selected ? byId.get(selected) : undefined);
  // the panel follows the selection to another box, and closes with it
  $effect(() => {
    const p = untrack(() => sess.panel), id = selected;
    if (!p?.box || p.box === id) return;
    sess.panel = id && written(byId.get(id)) ? { box: id, focus: null } : null;
  });
  const addParent = $derived(sess.adding ? byId.get(sess.adding.parent) : undefined);
  let landed: Show | null | undefined;
  $effect(() => {
    const s = show;
    if (!s || s === landed || diff || !(vw > 0 && vh > 0)) return;
    untrack(() => {
      landed = s;
      if (sess.ui || sess.adding || sess.draft || tool.current !== "select" || live) waiting = s;
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
    const shiftOn = edit && e.shiftKey && tool.current === "select" && ((e.target as Element | null)?.closest?.("[data-box]:not([data-open])") as HTMLElement | null)?.dataset.box;
    if (swallow) swallow = false;
    else if (shiftOn && byId.has(shiftOn)) toggle(shiftOn);
    else if (!(edit && tool.current === "box")) return;
    e.stopPropagation();
    e.preventDefault();
  }
  function dblclick(e: MouseEvent) {
    const t = e.target as Element | null, b = t?.closest?.("[data-box]"), hit = edit && tool.current === "select" ? arrowHit(e) : undefined;
    if (hit) {
      e.preventDefault();
      sess.pickArrow({ id: hit });
      if (sess.editing === "edit" || sess.editing === "drift") sess.writing = "label";
      return;
    }
    if (!edit || tool.current !== "select" || space || t?.closest?.("[data-draft], [data-pad], [data-arrow], [data-label], [data-popover]") || (b && !b.hasAttribute("data-open"))) return;
    e.preventDefault();
    draftAt(groundAt(e));
  }
  function pressAway(e: PointerEvent) {
    sess.outside(e);
    const t = e.target as Element | null;
    if (sess.draft && !sess.draft.drawing && t && root?.contains(t) && !t.closest?.("[data-draft]")) finishDraft(null);
  }
  function finishDraft(then: "kinds" | null) {
    const d = sess.draft, title = d?.title.trim();
    if (!d || d.drawing) return;
    if (!title || !api) return void (sess.draft = null);
    const id = freeId(title, new Set(view.boxes.map((b) => b.id)));
    const was = landing, change = sess.confirm(d, id, title);
    landing = { id, place: { x: d.x, y: d.y, w: d.w, h: d.h }, kinds: then === "kinds" };
    const sent = sess.send(change, then !== "kinds", () => {
      if (landing?.id === id) landing = null;
      sess.draft ??= d;
    });
    if (!sent) return void (landing = was);
    sess.draft = null;
  }
  function kindFirst(x: number, y: number, parent: string) {
    if (!edit) return;
    const r = stage.getBoundingClientRect();
    sess.ui = null;
    sess.adding = { parent, x: x - r.left, y: y - r.top };
  }
  function add(a: { kind: string; name: string; at: string[]; props: Record<string, string> }) {
    const parent = sess.adding!.parent;
    sess.adding = null;
    const props = Object.keys(a.props).length ? { props: a.props } : {};
    run(() => api!.apply({ op: "add", parent, kind: a.kind, id: freeId(a.name, new Set(view.boxes.map((b) => b.id))), title: a.name, at: a.at, before: null, ...props }));
  }

  /** The deepest box holding the view's centre and filling >= 60% of the view's width or height;
   *  a box with pads may do so in the space opening it fits it to, beside the card. */
  const trail = $derived.by(() => {
    const cam = lod;
    const out = [ROOT], card = cardRem(), r = rem();
    // ponytail: padsOf walks every arrow, but only for the box or two a frame's test lets through; memo per view if a huge map shows it
    const hasPads = (id: string) => padsOf(view, id, frameOf(id), () => undefined, on, frameOf).length > 0;
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
    const cam = lod;
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
    if (many.some((id) => !byId.has(id))) choose(many.filter((id) => byId.has(id)));
    if (traced && !traced.boxes.every((id) => byId.has(id))) traced = null;
    // an open popover or form on a box that is gone has nothing to act on
    const gone = (id: string) => id !== ROOT && !byId.has(id);
    const u = sess.ui;
    if ((u && u.t !== "arrow" && gone(u.id)) || (sess.adding && gone(sess.adding.parent)) || (sess.draft && gone(sess.draft.parent))) sess.cancel();
  });
  const dots = $derived.by(() => {
    const { step, fade } = gridLevel(cam.k, GRID), sp = GRID * step * cam.k;
    const at = (sp: number, opacity: number) => ({ sp, opacity, x: (((cam.x - sp / 2) % sp) + sp) % sp, y: (((cam.y - sp / 2) % sp) + sp) % sp });
    return sp > 0 && Number.isFinite(sp) ? [at(sp, 1), ...(fade > 0 ? [at(sp / 4, fade)] : [])] : [];
  });
  const crumbs = $derived(trail.map((id) => ({ id, title: byId.get(id)?.title.value ?? id })));
  const shapeHere = $derived(shapeAt.get([...trail].reverse().find((id) => shapeAt.has(id) && (id === ROOT || !!view.placed.inner[id])) ?? ROOT) ?? "circuit");
  const picked = $derived(many.length > 1 ? many : selected && byId.get(selected)?.level !== "code" && byId.has(selected) ? [selected] : []);
  const barTop = 3 * rem();
  const bar = $derived.by(() => {
    if (!edit || !picked.length || live || sess.link || sess.draft || marquee) return null;
    const c = cam, boxes = picked.map((id) => byId.get(id)!), names = new Set(boxes.map(kindName));
    let x = Infinity, y = Infinity;
    for (const id of picked) {
      const f = frameOf(id);
      [x, y] = [Math.min(x, f.x * c.k + c.x), Math.min(y, f.y * c.k + c.y)];
    }
    const current = names.size === 1 ? [...names][0] : null, shared = sharedParent(view.boxes, picked, ROOT), parent = shared ?? boxes[0].parent ?? ROOT;
    const parentKinds = (kids.get(parent) ?? []).filter((b) => !picked.includes(b.id)).map(kindName).filter((n) => n !== "box");
    return {
      x: Math.max(8, x), y: Math.max(barTop, y), current, kind: current && boxes[0] ? kindOf(boxes[0], view.kinds) ?? null : null, shared,
      context: { parentKinds, inRun: inside(parent, byId, view.kinds) }, ask: sess.asking && sess.asking.ids.join() === picked.join() ? sess.asking.text : null,
      picking: !!sess.kinds && sess.kinds.join() === picked.join(),
    };
  });

  const arrowBar = $derived.by(() => {
    if (!edit || live || sess.link || sess.end || sess.draft || marquee) return null;
    return barOf(sess, view.arrows, {
      cam, frame: frameOf, inside: (p) => (p === ROOT ? { x: 0, y: 0, s: 1 } : place.inside(p)),
      wire: (a) => wiringOf(a.parent)?.[(unit.get(a.parent) ?? 1) * lod.k < RIBBON ? "ribbon" : "routed"].wires.get(a.id),
    }, barTop);
  });
  function pickPart(bundle: Arrow, i: number) {
    const p = bundle.parts?.[i];
    if (!p) return;
    sess.pickArrow({ id: bundle.id, part: { from: p.from, to: p.to } });
    glideTo(fitTo(partFrame(p, frameOf), { w: vw, h: vh }, 0.2));
  }
  $effect(() => {
    void view;
    untrack(() => {
      if (sess.arrow && !sess.picked) sess.pickArrow(null);
    });
  });

  /** A box's inner space inside its own frame, below its header (as place.inside, locally). */
  function local(id: string) {
    const f = shown.placed.frames[id], [iw, ih] = shown.placed.inner[id] ?? [0, 0];
    return { ...fitInner(f, iw, ih), iw, ih };
  }
</script>

{#snippet level(pid: string)}
  {@const zoom = (unit.get(pid) ?? 1) * lod.k}
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
  {#if mode === "routed"}
    {#each fading.filter((f) => f.a.parent === pid) as f (f.a.id)}
      <div class="arrow fade-out" aria-hidden="true"><MapArrow d={f.w.d} mid={f.w.mid} arrowLook={arrowLook(f.a, view.marks, on)} {zoom} /></div>
    {/each}
  {/if}
  {#each all as a (`${a.id} ${lineAt(a)}`)}
    {@const c = pathOf(a, mode)}
    {@const asked = ans && arrowLit(ans, a)}
    {@const drawnLook = arrowLook(a, view.marks, on, by.get(a.id))}
    {@const changeLook = !marks ? null : a.ghost ? "ghost" : marks.addedArrows.has(arrowKey(a.from, a.to)) ? "added" : null}
    {@const gold = !!hotPad?.includes(a.id) || (hotArrow === a.id && (opened(a.from) || opened(a.to)))}
    <div class="arrow" class:fresh={mode === "routed" && fresh.has(a.id)} data-arrow={a.id} data-line={mode === "routed" ? lineAt(a) : "ribbon"}>
      <MapArrow d={c.d} mid={c.mid} arrowLook={changeLook ? diffLook(drawnLook, changeLook) : asked || gold ? { ...drawnLook, color: "var(--map-trace)" } : drawnLook} {zoom}
        lit={gold || lit.includes(a)} dim={!gold && lit.length > 0 && !lit.includes(a)} picked={sess.arrow?.id === a.id}
        onhover={(h) => (hotArrow = h ? a.id : hotArrow === a.id ? null : hotArrow)} />
      {#if ans && arrowNumber(ans, a)}
        <span class="number on-arrow" class:now={arrowNumber(ans, a) === step + 1} style:left="{c.mid[0]}px" style:top="{c.mid[1]}px" style:scale={1 / zoom}>{arrowNumber(ans, a)}</span>
      {/if}
    </div>
  {/each}
  {#each (kids.get(pid) ?? []).filter((b) => !vw || seen(b.id)) as b (b.id)}
    {@const f = drawn(b.id)}
    {@const sw = world.get(b.id)!.w * lod.k}
    {@const open = opened(b.id)}
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
      class:picked={many.includes(b.id)} class:answer={inAnswer && (!marks || nowOn.has(b.id))} class:answer-now={inAnswer && nowOn.has(b.id)} class:diff-changed={change === "changed"} class:diff-added={change === "added"} data-box={b.id} data-open={open ? "" : undefined} style:left="{f.x}px" style:top="{f.y}px" style:width="{f.w}px" style:height="{f.h}px">
      <MapBox box={b} look={look(b.id, view.marks, on, b, by.get(b.id))} detail={open ? "open" : reads ? "source" : detailFor(sw)}
        zoom={sw / f.w} source={reads && (showsInside(sw) || !list) ? source : undefined} parts={list} {anatomy}
        inside={counts.get(b.id) ?? 0} more={kids.get(b.id)?.length ?? 0} onopen={() => openBox(b.id)}
        kinds={view.kinds} w={f.w} h={f.h} {shape} table={view.tables?.[b.id]} rows={marks?.tables.get(b.id)} facts={cards.get(b.id)} fold={noise.bookkeeping} picked={many.includes(b.id)}
        onfocus={view.tables?.[b.id] ? () => openFocus(b.id) : undefined} />
      {#if edit && (selected === b.id || hot === b.id) && !open}
        {#each ["l", "r", "t", "b"] as h (h)}<span class="handle {h}" data-handle aria-hidden="true" style:scale={sw ? f.w / sw : 1}></span>{/each}
      {/if}
      {#if edit && selected === b.id && !open}
        {#each ["nw", "ne", "sw", "se"] as c (c)}<span class="grip {c}" data-resize={c} aria-hidden="true" style:scale={sw ? f.w / sw : 1}></span>{/each}
      {/if}
      {#if open}
        {@const t = local(b.id)}
        <div class="inside" style:width="{t.iw}px" style:height="{t.ih}px"
          style:transform="translate({t.x}px, {t.y}px) scale({t.s})">{@render level(b.id)}</div>
      {/if}
      {#if showsInside(sw)}
        <Pads {view} id={b.id} frame={f} childFrame={drawn} {on} scale={f.w / sw} onopen={openBox} worldOf={frameOf} outside={outsideOf(b.id)} shape={shapeAt.get(b.id) ?? "circuit"}
          hot={hotArrow} onhot={(ids) => (hotPad = ids)} onassign={edit ? (e) => sess.send(e) : undefined} />
      {/if}
      {#if inAnswer}
        <span class="numbers" style:scale={f.w / sw}>
          {#each (ans?.numbers.has(b.id) ? [ans.numbers.get(b.id)!] : st!.under).filter((n) => !marks || n === step + 1) as n (n)}<span class="number" class:now={n === step + 1}>{n}</span>{/each}
        </span>
        {#if !st!.in && !marks}<span class="holds" style:scale={f.w / sw}>{st!.holds} of {kids.get(b.id)?.length ?? st!.holds}</span>{/if}
      {/if}
      {#if change}<span class="change {change}" style:scale={f.w / sw}>{change === "added" ? "new" : marks!.chips.get(b.id) ?? files(marks!.files.get(b.id) ?? 0)}</span>{/if}
      {#if goneHere}<span class="gone-here" style:scale={f.w / sw}>−{goneHere} removed here</span>{/if}
    </div>
  {/each}
  {#if sess.draft?.parent === pid}
    {@const d = sess.draft}
    {@const o = origin(pid)}
    <Draft x={d.x - o.x} y={d.y - o.y} w={d.w} h={d.h} editing={!d.drawing} title={d.title}
      ontitle={(v) => sess.draft && (sess.draft.title = v)} onfinish={finishDraft} />
  {/if}
  <!-- the arrows' labels, above the boxes -->
  {#if mode === "routed"}
    {#each fading.filter((f) => f.a.parent === pid) as f (f.a.id)}
      <div class="arrow fade-out" aria-hidden="true"><MapArrow part="label" d={f.w.d} mid={f.w.mid} arrowLook={arrowLook(f.a, view.marks, on)} {zoom} note={f.a.note} len={f.w.len} /></div>
    {/each}
  {/if}
  {#each mode === "ribbon" ? all.filter((a) => lit.includes(a) || sess.arrow?.id === a.id) : all as a (`${a.id} ${lineAt(a)}`)}
    {@const c = pathOf(a, mode)}
    {@const here = sess.arrow?.id === a.id}
    <div class="arrow" class:fresh={mode === "routed" && fresh.has(a.id)} data-label={a.id}>
      <MapArrow part="label" d={c.d} mid={c.mid} arrowLook={arrowLook(a, view.marks, on)} {zoom} note={a.note} len={c.len} picked={here}
        lit={lit.includes(a)} dim={lit.length > 0 && !lit.includes(a)}
        ends={here && !sess.arrow?.part && (sess.editing === "edit" || sess.editing === "drift") ? c.ends : undefined} />
    </div>
  {/each}
{/snippet}

<svelte:window onpointermove={move} onpointerup={up} onpointercancel={abort} onblur={abort} onpointerdowncapture={pressAway} onkeydown={key} onkeyup={keyup} onpaste={pasted} />

<!-- the dot grid sits on the map's own grid and moves and scales with it; a dot keeps its size on screen -->
<div class="map" bind:this={root} bind:clientWidth={cw} bind:clientHeight={ch} style:--fade="{FADE}ms">
  {#each dots as d, i (i)}
    <div class="dots" aria-hidden="true" style:opacity={d.opacity} style:background-size="{d.sp}px {d.sp}px" style:background-position="{d.x}px {d.y}px"></div>
  {/each}
  <div class="stage" class:hand={tool.current === "hand" || space} class:draw={!!edit && (tool.current === "box" || tool.current === "frame") && !space} bind:this={stage} role="presentation"
    onpointerdown={down} onclickcapture={clickCapture} ondblclick={dblclick} oncontextmenu={(e) => { const hit = edit ? arrowHit(e) : undefined; if (hit) sess.arrowMenuAt(e, hit); else sess.menuAt(e); }}
    onpointerover={over} onpointerleave={() => ((hot = hotArrow = null), (onMap = false))}>
    <div class="world" style:transform="translate({base.x}px, {base.y}px) scale({base.k})">{@render level(anchor)}</div>
  </div>
  {#if marquee}
    <div class="marquee" class:frame={marquee.frame} data-marquee aria-hidden="true" style:left="{Math.min(marquee.x0, marquee.x1)}px" style:top="{Math.min(marquee.y0, marquee.y1)}px"
      style:width="{Math.abs(marquee.x1 - marquee.x0)}px" style:height="{Math.abs(marquee.y1 - marquee.y0)}px">{#if marquee.frame}<span>Group</span>{/if}</div>
  {/if}
  <Controls {crumbs} {layers} {on} oncrumb={fly} low={!!edit}
    ontoggle={(l) => { const s = new Set(off); if (!s.delete(l)) s.add(l); off = s; }}
    {wheelPref} onwheel={setPref} {arrowPref} arrowHere={shapeHere} onarrows={setArrows} {endsPref} onends={setEnds} {noise} onnoise={setNoise} onarrange={arrangeMap}>
    <Search view={given} drawn={view} onopen={(id) => { traced = null; byId.has(id) ? openBox(id) : openFocus(id); }} ontrace={lightPath} />
  </Controls>
  {#if edit}
    <Island tool={tool.current} locked={tool.locked} onpick={(t, lock) => tool.pick(t, lock)} onarrange={arrangeMap}
      onkind={(e) => { const r = (e.currentTarget as HTMLElement).getBoundingClientRect(); kindFirst(r.left, r.bottom + 8, anchor); }} />
  {/if}
  <Corner k={cam.k} onzoom={(f) => zoomAt(vw / 2, vh / 2, f, true)} onfit={() => glideTo(home())}
    onundo={edit ? undo : undefined} onredo={edit ? redo : undefined} />
  <div class="notes" class:low={!!edit}>
    {#if edit}<div class="toast" role="status" aria-live="polite">{note}</div>{/if}
    {#if tidied}
      <div class="arranged" role="status">
        {tidied.say}
        {#if tidied.stamp !== undefined}<button type="button" onclick={undoArrange}>Undo</button>{/if}
      </div>
    {/if}
  </div>
  {#if edit && sess.adding}
    <AddBox parentTitle={addParent?.title.value ?? view.boxes.find((b) => b.id === ROOT)?.title.value ?? ROOT} within={inside(sess.adding.parent, byId, view.kinds)}
      kinds={view.kinds} globs={addParent?.madeOf ?? []} x={sess.adding.x} y={sess.adding.y} onadd={add} onclose={() => sess.cancel()} />
  {/if}
  <Proposals {view} edit={api} {run} onlight={glow} />
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
  {#if bar}
    {@const one = picked[0]}
    <BoxBar x={bar.x} y={bar.y} count={picked.length} kind={bar.kind} current={bar.current} code={picked.length === 1 && hasCode(one)} apart={!bar.shared}
      asking={bar.ask} picking={bar.picking} kinds={view.kinds} context={bar.context}
      onkinds={(o) => (sess.kinds = o ? [...picked] : null)} onpick={(k) => sess.setKind([...picked], k)}
      ondetails={() => details(one)} onarrow={() => sess.act("arrow", one)} oncode={() => openBox(one)} onmore={(e) => sess.menuFrom(e)}
      ongroup={() => bar.shared && makeGroup(bar.shared, [...picked])} onalign={align} onarrange={() => arrangeIn([...picked])}
      ondelete={() => sess.remove([...picked])} onconfirm={() => sess.confirmRemove()} onkeep={() => (sess.asking = null)} />
  {/if}
  {#if arrowBar}
    {@const ab = arrowBar}
    {#if sess.writing}
      <ArrowText what={sess.writing} value={sess.text(ab.a, sess.writing) ?? ""} x={ab.x} y={ab.y} max={sess.writing === "note" ? 80 : undefined}
        onsubmit={(v) => sess.arrowSet(sess.writing!, v)} onclose={() => (sess.writing = null)} />
    {:else}
      <ArrowBar x={ab.x} y={ab.y} how={ab.how} title={ab.title} kind={specKind(ab.a)} kinds={arrowKindNames(view.arrowKinds)} picking={sess.arrowKinds}
        note={!!ab.a.note} context={!!ab.a.context} shape={ab.a.shape ?? null} parts={ab.parts} onshape={(v, undo) => sess.arrowSet("shape", v, undo) || undo()}
        onkinds={(o) => (sess.arrowKinds = o)} onpick={(k) => sess.arrowSet("kind", k)} onnote={() => (sess.writing = "note")} oncontext={() => sess.openPanel("context")}
        onflip={() => sess.flip()} ondetails={() => sess.openPanel(null)} ondelete={() => sess.unarrow()} onkeep={() => sess.keepArrow()} onpart={(i) => pickPart(ab.bundle, i)} />
    {/if}
  {/if}
  {#if edit && sess.ui?.t === "arrow" && arrowBar}
    {@const ab = arrowBar}
    <ArrowMenu x={sess.ui.x} y={sess.ui.y} title={ab.title} how={ab.how} parts={ab.parts.map((p) => `${p.kind} · ${p.title}`)}
      onpick={(a) => sess.arrowAct(a)} onpart={(i) => { sess.ui = null; pickPart(ab.bundle, i); }} onclose={() => (sess.ui = null)} />
  {/if}
  {#if edit && sess.panel?.arrow && sess.picked}
    {@const a = sess.picked}
    {#key sess.panel.arrow.id + (sess.panel.arrow.part ? `:${a.from}->${a.to}` : "")}
      <ArrowPanel {a} how={sess.editing} kinds={arrowKindNames(view.arrowKinds)} s={sess} titleOf={sess.titleOf} focus={sess.panel.focus} onclose={closePanel} />
    {/key}
  {/if}
  {#if edit}<EditLayer s={sess} {arrangeable} boxes={view.boxes} kinds={view.kinds} arrowKinds={view.arrowKinds} root={ROOT} paste={!!clip} />{/if}
  {#if view.landscape}
    <Folders landscape={view.landscape} />
  {/if}
  {#if reading}
    <p class="reading" class:lifted={!!view.proposals?.length}>{#if view.landscape}Dotted projects are not read yet.{:else}Some boxes are ig's guess.{/if} Run <code>ig analyse</code> to let an AI agent read them.</p>
  {/if}
  {#if focusing}
    <Focus view={given} folds={noise} store={focusing.store} table={focusing.table} ends={endsPref} onends={setEnds} onclose={closeFocus}
      onpick={(id) => focusing && (focusing = { ...focusing, table: id })} />
  {/if}
  {#if panelBox}
    {#key panelBox.id}
      <BoxPanel {view} box={panelBox} s={sess} context={kindContext(panelBox.id)} onclose={closePanel} onbox={(id) => details(id)} onadd={addInside}
        onarrow={(p) => { sess.pickArrow(p); sess.openPanel(null); }} />
    {/key}
  {:else if selected && byId.get(selected)}
    <DetailsCard {view} box={byId.get(selected)!} onclose={close} onopen={(id) => { traced = null; openBox(id); }} onmenu={edit ? (e) => sess.menuFrom(e) : undefined}
      onedit={edit && written(byId.get(selected)) ? () => details(selected!) : undefined} edit={api} {run}
      ondb={view.tables?.[selected] ? { label: "Focus", run: () => openFocus(selected!) } : hasTables(selected) ? { label: "Tables…", run: () => openTables(selected!) } : undefined} />
  {/if}
</div>

<style>
  .map { position: absolute; inset: 0; overflow: hidden; container: map / inline-size;
    background: var(--map-ground); color: var(--map-fg); }
  .dots { position: absolute; inset: 0; pointer-events: none; background-image: radial-gradient(var(--map-dots) 1.2px, transparent 1.4px); }
  .stage { position: absolute; inset: 0; touch-action: none; }
  .stage.hand { cursor: grab; }
  .stage.hand:active { cursor: grabbing; }
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
  .slot.target { outline: 3px solid var(--map-trace); outline-offset: 3px; border-radius: 1rem; }
  .slot.picked { outline: 2px solid var(--map-trace); outline-offset: 4px; border-radius: 1rem; }
  .marquee { position: absolute; z-index: 3; pointer-events: none; box-sizing: border-box; border: 1px dashed var(--map-trace); background: color-mix(in srgb, var(--map-trace) 8%, transparent); }
  .marquee.frame { border-width: 1.5px; border-radius: var(--radius-lg); background: none; }
  .marquee span { position: absolute; left: var(--space-2); top: var(--space-1); font-size: var(--text-sm); color: var(--map-trace); }
  .handle { position: absolute; width: 0.625rem; height: 0.625rem; border-radius: 50%; background: var(--map-blue); border: 2px solid var(--map-ground); translate: -50% -50%; cursor: crosshair; }
  .grip { position: absolute; z-index: 1; width: 0.625rem; height: 0.625rem; box-sizing: border-box; border-radius: 2px; background: var(--map-surface); border: 1.5px solid var(--map-trace); translate: -50% -50%; }
  .grip.nw { left: 0; top: 0; cursor: nwse-resize; } .grip.se { left: 100%; top: 100%; cursor: nwse-resize; }
  .grip.ne { left: 100%; top: 0; cursor: nesw-resize; } .grip.sw { left: 0; top: 100%; cursor: nesw-resize; }
  .stage.draw { cursor: crosshair; }
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
  .fresh > :global(svg) { animation: line-in var(--fade) ease both; }
  .fade-out > :global(svg) { animation: line-out var(--fade) ease both; }
  @keyframes line-in { from { opacity: 0; } }
  @keyframes line-out { to { opacity: 0; } }
  @media (prefers-reduced-motion: reduce) { .fresh > :global(svg), .fade-out > :global(svg) { animation: none; } .fade-out { display: none; } }
  .sleeves { position: absolute; left: 0; top: 0; overflow: visible; pointer-events: none; fill: none; stroke: var(--map-line-soft); stroke-linecap: round; }
  .inside { position: absolute; left: 0; top: 0; transform-origin: 0 0; pointer-events: none; }
  .inside > .slot { pointer-events: auto; }
  /* app.css's global .row (list rows) must not restyle the boxes' title row */
  .map :global(.row) { padding: 0; background: none; cursor: inherit; }
  .reading { position: absolute; left: 50%; bottom: var(--space-4); transform: translateX(-50%); margin: 0; padding: var(--space-1-5) var(--space-3-5);
    border: 1px dotted var(--map-gold); border-radius: var(--radius-pill); background: var(--map-surface); font-size: var(--text-md); color: var(--map-fg-2); max-width: calc(100% - 2rem); }
  .notes { position: absolute; z-index: 4; left: 50%; top: var(--space-3); transform: translateX(-50%); display: flex; flex-direction: column; align-items: center; gap: var(--space-1-5);
    max-width: calc(100% - 2rem); pointer-events: none; }
  .notes.low { top: 4.25rem; }
  /* the island drops below the search when the two would meet */
  @container map (max-width: 80rem) { .notes.low { top: 6.5rem; } }
  @media (max-width: 47.5rem) { .notes.low { top: 8.75rem; } }
  .notes > * { pointer-events: auto; }
  .toast { max-width: 28rem; background: var(--map-bad-bg); color: var(--map-bad-text); border-radius: var(--radius-lg); font-size: var(--text-md); }
  .toast:not(:empty) { padding: var(--space-1-5) var(--space-2-5); }
  .arranged { display: flex; align-items: center; gap: var(--space-3); padding: var(--space-1-5) var(--space-2) var(--space-1-5) var(--space-3);
    background: var(--map-surface); border: 1px solid var(--map-line); border-radius: var(--radius-lg); box-shadow: 0 0.375rem 1.25rem var(--map-shadow); font-size: var(--text-md); color: var(--map-fg); }
  .arranged button { font: inherit; color: var(--map-fg); background: var(--map-sunken); border: 1px solid var(--map-line); border-radius: var(--radius-md); padding: var(--space-0-5) var(--space-2-5); cursor: pointer; }
  .arranged button:focus-visible { outline: 2px solid var(--map-trace); outline-offset: 2px; }
  .reading.lifted { bottom: 4.5rem; }
  .reading code { background: var(--map-sunken); }
</style>
