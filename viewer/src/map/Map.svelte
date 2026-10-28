<!-- The walkable map. One world <div> under the camera; each box whose screen width reaches
     showsInside mounts its inside (children, regions, routes in its inner units), recursively.
     Only mounted levels are in the DOM. Map text is repo/AI-derived: text only, never {@html}. -->
<script lang="ts">
  import { onDestroy, untrack } from "svelte";
  import type { Frame, MapArrow as Arrow, MapBox as Box, Region as R, View } from "./types";
  import { detailFor, fitTo, innerTransform, showsInside, worldFrame, type Cam } from "./camera";
  import { arrowLook, insideCounts, look } from "./looks";
  import { Camera, wheelFactor } from "./glide";
  import MapBox from "./MapBox.svelte";
  import MapArrow from "./MapArrow.svelte";
  import Region from "./Region.svelte";
  import DetailsCard from "./DetailsCard.svelte";
  import Controls from "./Controls.svelte";

  let { view, camera = new Camera(), size }: {
    view: View; camera?: Camera; size?: { w: number; h: number };
  } = $props();

  const ROOT = "system";
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
  // ponytail: world frames computed once per view (O(n·depth) lookups); fine for thousands of boxes.
  const world = $derived(new Map(framed.map((b) => [b.id, worldFrame(b.id, view)])));
  const area = (f: Frame) => f.w * f.h;
  const regions = $derived(group([...view.placed.regions].sort((a, b) => area(b.frame) - area(a.frame)), (r: R) => r.parent));
  const arrows = $derived(group(view.arrows.filter((a) => view.placed.routes[a.id]), (a: Arrow) => a.parent));
  // each arrow-holding parent's inner unit, in world units: arrow labels show once it is legible
  const unit = $derived(new Map([...arrows.keys()].map((p) => [p, p === ROOT ? 1 : innerTransform(p, view).s])));
  const layers = $derived([...new Set(["rules", "risks", ...view.marks.map((m) => m.layer)])]);
  let off = $state(new Set<string>());
  const on = $derived(new Set(layers.filter((l) => !off.has(l))));
  const counts = $derived(insideCounts(view, view.marks, on));
  const reading = $derived(view.boxes.some((b) => b.needsReading));

  let cw = $state(0), ch = $state(0);
  const vw = $derived(size?.w ?? cw), vh = $derived(size?.h ?? ch);
  let cam = $state<Cam>(untrack(() => camera.cam));
  let selected = $state<string | null>(null);
  let stage: HTMLDivElement;

  const home = () => fitTo(worldFrame(ROOT, view), { w: vw, h: vh });
  const frameOf = (id: string) => (id === ROOT ? worldFrame(ROOT, view) : world.get(id) ?? worldFrame(id, view));

  // rAF-driven camera: runs only while a glide is in flight.
  let raf = 0;
  function frame(now: number) {
    raf = 0;
    cam = camera.tick(now);
    if (camera.moving) raf = requestAnimationFrame(frame);
  }
  function glideTo(c: Cam) {
    camera.to(c, performance.now());
    if (!raf) raf = requestAnimationFrame(frame);
  }
  function jump(c: Cam) {
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

  /** A box with an inside is framed by its inside (so the inside fills the screen), others by their frame. */
  function target(id: string): Frame {
    const inner = view.placed.inner[id];
    if (!kids.has(id) || !inner) return frameOf(id);
    const t = innerTransform(id, view);
    return { x: t.x, y: t.y, w: inner[0] * t.s, h: inner[1] * t.s };
  }
  const fly = (id: string) => glideTo(id === ROOT ? home() : fitTo(target(id), { w: vw, h: vh }));
  function openBox(id: string) {
    selected = id;
    fly(id);
  }
  function close() {
    const id = selected;
    selected = null;
    if (id) (stage.querySelector(`[data-box="${CSS.escape(id)}"] > button`) as HTMLElement | null)?.focus({ preventScroll: true });
  }
  const key = (e: KeyboardEvent) => e.key === "Escape" && selected && close();
  const clampK = (k: number) => Math.min(1e5, Math.max(1e-3, k));
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

  // Drag pans; a drag's trailing click must not open a box.
  let drag: { x: number; y: number; from: Cam; moved: boolean } | null = null;
  let swallow = false;
  function down(e: PointerEvent) {
    swallow = false;
    if (e.button === 0) drag = { x: e.clientX, y: e.clientY, from: camera.cam, moved: false };
  }
  function move(e: PointerEvent) {
    if (!drag) return;
    const dx = e.clientX - drag.x, dy = e.clientY - drag.y;
    if (!drag.moved && Math.hypot(dx, dy) < 4) return;
    drag.moved = true;
    jump({ k: drag.from.k, x: drag.from.x + dx, y: drag.from.y + dy });
  }
  function up() {
    swallow = !!drag?.moved;
    drag = null;
  }
  function clickCapture(e: MouseEvent) {
    if (!swallow) return;
    swallow = false;
    e.stopPropagation();
    e.preventDefault();
  }

  /** The deepest box holding the view's centre and filling >= 60% of the view's width or height. */
  const trail = $derived.by(() => {
    const out = [ROOT];
    for (let p = ROOT; ; ) {
      if (out.indexOf(p) !== out.length - 1) return out.slice(0, -1); // a loop: stop before it
      const next = (kids.get(p) ?? []).find((b) => {
        const f = world.get(b.id)!, x = f.x * cam.k + cam.x, y = f.y * cam.k + cam.y, w = f.w * cam.k, h = f.h * cam.k;
        return x <= vw / 2 && vw / 2 <= x + w && y <= vh / 2 && vh / 2 <= y + h && Math.max(w / vw, h / vh) >= 0.6;
      });
      if (!next) return out;
      out.push((p = next.id));
    }
  });
  const crumbs = $derived(trail.map((id) => ({ id, title: byId.get(id)?.title.value ?? id })));

  /** A box's inner space inside its own frame: uniform scale, centred (as innerTransform, locally). */
  function local(id: string) {
    const f = view.placed.frames[id], [iw, ih] = view.placed.inner[id] ?? [0, 0];
    const s0 = Math.min(f.w / iw, f.h / ih), s = Number.isFinite(s0) && s0 > 0 ? s0 : 1;
    return { x: (f.w - iw * s) / 2, y: (f.h - ih * s) / 2, s, iw, ih };
  }
</script>

{#snippet level(pid: string)}
  {#each regions.get(pid) ?? [] as r, i (i)}<Region region={r} />{/each}
  {#each arrows.get(pid) ?? [] as a (a.id)}
    <div class="arrow" data-arrow={a.id}>
      <MapArrow points={view.placed.routes[a.id]} arrowLook={arrowLook(a, view.marks, on)}
        detail={(unit.get(pid) ?? 1) * cam.k >= 0.6 ? "near" : "far"} />
    </div>
  {/each}
  {#each kids.get(pid) ?? [] as b (b.id)}
    {@const f = view.placed.frames[b.id]}
    {@const sw = world.get(b.id)!.w * cam.k}
    {@const open = showsInside(sw) && kids.has(b.id) && !!view.placed.inner[b.id]}
    <div class="slot" data-box={b.id} style:left="{f.x}px" style:top="{f.y}px" style:width="{f.w}px" style:height="{f.h}px">
      <MapBox box={b} look={look(b.id, view.marks, on, b)} detail={open ? "far" : detailFor(sw)}
        inside={counts.get(b.id) ?? 0} onopen={() => openBox(b.id)} />
      {#if open}
        {@const t = local(b.id)}
        <div class="inside" style:width="{t.iw}px" style:height="{t.ih}px"
          style:transform="translate({t.x}px, {t.y}px) scale({t.s})">{@render level(b.id)}</div>
      {/if}
    </div>
  {/each}
{/snippet}

<svelte:window onpointermove={move} onpointerup={up} onkeydown={key} />

<div class="map" bind:clientWidth={cw} bind:clientHeight={ch}>
  <div class="stage" bind:this={stage} role="presentation" onpointerdown={down} onclickcapture={clickCapture}>
    <div class="world" style:transform="translate({cam.x}px, {cam.y}px) scale({cam.k})">{@render level(ROOT)}</div>
  </div>
  <Controls {crumbs} {layers} {on} oncrumb={fly}
    ontoggle={(l) => { const s = new Set(off); if (!s.delete(l)) s.add(l); off = s; }}
    onzoom={(f) => zoomAt(vw / 2, vh / 2, f, true)} onfit={() => glideTo(home())} />
  {#if reading}
    <p class="reading">Some boxes are ig's guess. Run <code>ig view --read</code> to let an AI agent read them.</p>
  {/if}
  {#if selected && byId.get(selected)}
    <DetailsCard {view} box={byId.get(selected)!} onclose={close} />
  {/if}
</div>

<style>
  .map { position: absolute; inset: 0; overflow: hidden; background: #faf8f3; color: #23211e; }
  .stage { position: absolute; inset: 0; cursor: grab; touch-action: none; }
  .stage:active { cursor: grabbing; }
  .world { position: absolute; left: 0; top: 0; transform-origin: 0 0; }
  .slot { position: absolute; }
  .arrow { display: contents; }
  .inside { position: absolute; left: 0; top: 0; transform-origin: 0 0; pointer-events: none; animation: ig-fade 400ms ease-out; }
  .inside > .slot { pointer-events: auto; }
  @keyframes ig-fade { from { opacity: 0; } }
  /* app.css's global .row (list rows) must not restyle the boxes' title row */
  .map :global(.row) { padding: 0; background: none; cursor: inherit; }
  .reading { position: absolute; left: 50%; bottom: 16px; transform: translateX(-50%); margin: 0; padding: 6px 14px;
    border: 1px dotted #b8860b; border-radius: 999px; background: #fffdf8; font-size: 13px; color: #4a4640; max-width: calc(100% - 32px); }
  .reading code { background: #f2eee5; }
</style>
