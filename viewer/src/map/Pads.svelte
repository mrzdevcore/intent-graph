<!-- An open box's connections, in the box's own units: a pad on the edge facing each far box, its title
     beside it, and a wire inside to the child holding its end. `scale` keeps pads and text one size on
     screen. Titles are repo-derived: text only. The fold of what does not fit has no wire and opens this
     box, whose card lists every link. -->
<script lang="ts">
  import type { ArrowShape, Edit, Frame, View } from "./types";
  import { assign, inBox, padPoint, padsOf, wiresOf, type Pad } from "./pads";
  import { written } from "./edit/menu";
  import type { P } from "./shapes/path";

  let { view, id, frame, childFrame, on, scale, onopen, shape = "circuit", hot = null, onhot, onassign, worldOf }: {
    view: View; id: string; frame: { w: number; h: number }; childFrame: (id: string) => Frame | undefined;
    on: Set<string>; scale: number; onopen: (id: string) => void; shape?: ArrowShape; hot?: string | null;
    onhot?: (arrows: string[] | null) => void; onassign?: (e: Edit) => void; worldOf?: (id: string) => Frame | undefined;
  } = $props();

  // past this many, chips would crowd the labels: the grey says it alone
  const CHIPS = 12;
  const STUB = 24;
  const INWARD = { l: [1, 0], r: [-1, 0], t: [0, 1], b: [0, -1] } as const;
  const pads = $derived(padsOf(view, id, frame, childFrame, on, worldOf));
  const place = $derived(inBox(view, id, frame));
  const kids = $derived.by(() => {
    const out: Record<string, Frame> = {};
    for (const b of view.boxes) {
      const f = b.parent === id ? childFrame(b.id) : undefined;
      if (f) out[b.id] = place(f);
    }
    return out;
  });
  const wires = $derived(wiresOf(pads, frame, kids, shape));
  const asks = $derived(Object.keys(kids).length > 0);
  const keyOf = (p: Pad) => `${p.other}\u0000${p.child ?? ""}`;
  let near = $state<string | null>(null);
  const lit = (i: number) => near === keyOf(pads[i]) || (!!hot && pads[i].arrows.includes(hot));
  function enter(i: number) {
    near = keyOf(pads[i]);
    onhot?.(pads[i].arrows);
  }
  function leave(i: number) {
    if (near !== keyOf(pads[i])) return;
    near = null;
    onhot?.(null);
  }
  $effect(() => {
    if (near && !pads.some((p) => keyOf(p) === near)) {
      near = null;
      onhot?.(null);
    }
  });
  $effect(() => () => {
    if (near) onhot?.(null);
  });
  const stub = (p: Pad): P => {
    const [x, y] = padPoint(p, frame), [dx, dy] = INWARD[p.edge];
    return [x + dx * STUB * scale, y + dy * STUB * scale];
  };

  let root = $state<HTMLDivElement>();
  let drag = $state<{ i: number; to: P; over: string | null } | null>(null);
  function local(e: PointerEvent): P {
    const r = root!.getBoundingClientRect();
    return r.width && r.height ? [((e.clientX - r.left) * frame.w) / r.width, ((e.clientY - r.top) * frame.h) / r.height] : [0, 0];
  }
  function childAt([x, y]: P) {
    const hit = Object.entries(kids).find(([, f]) => x >= f.x && x <= f.x + f.w && y >= f.y && y <= f.y + f.h)?.[0] ?? null;
    return hit && written(view.boxes.find((b) => b.id === hit)) ? hit : null;
  }
  function grab(e: PointerEvent, i: number) {
    if (e.button !== 0) return;
    e.stopPropagation();
    e.preventDefault();
    drag = { i, to: stub(pads[i]), over: null };
  }
  function move(e: PointerEvent) {
    if (!drag) return;
    const to = local(e);
    drag = { ...drag, to, over: childAt(to) };
  }
  function up(e: PointerEvent) {
    if (!drag) return;
    const p = pads[drag.i], child = childAt(local(e));
    drag = null;
    if (p && child) onassign?.(assign(p, child));
  }
</script>

<svelte:window onpointermove={move} onpointerup={up} onpointercancel={() => (drag = null)} />

{#if pads.length}
  <div class="pads" bind:this={root}>
    <svg width="1" height="1" aria-hidden="true">
      {#each pads as p, i (i)}
        {#if !p.more}
          {@const w = wires[i]}
          <g class:down={p.down} class:lit={lit(i)} stroke-width={(lit(i) ? 2.25 : 1.5) * scale} data-wire={w ? p.other : undefined}>
            {#if w}
              {@const end = w.pts[w.pts.length - 1]}
              <path d={w.d} stroke-linecap="round" stroke-linejoin="round" stroke-dasharray={p.down ? `0 ${4 * scale}` : undefined} />
              <circle cx={end[0]} cy={end[1]} r={3 * scale} />
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <path class="hit" d={w.d} stroke="transparent" stroke-width={12 * scale} onpointerenter={() => enter(i)} onpointerleave={() => leave(i)} />
            {:else}
              {@const [x, y] = padPoint(p, frame)}
              {@const s = stub(p)}
              <line x1={s[0]} y1={s[1]} x2={x} y2={y} stroke-linecap="round" stroke-dasharray={p.down ? `0 ${4 * scale}` : undefined} />
              {#if onassign && asks && !p.child}
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <circle class="grab" data-pad={p.other} data-stub={i} cx={s[0]} cy={s[1]} r={5 * scale} onpointerdown={(e) => grab(e, i)} />
              {/if}
            {/if}
          </g>
        {/if}
      {/each}
      {#if drag}
        {@const s = stub(pads[drag.i])}
        <line class="pull" x1={s[0]} y1={s[1]} x2={drag.to[0]} y2={drag.to[1]} stroke-width={1.5 * scale} stroke-dasharray="{4 * scale} {4 * scale}" />
        {#if drag.over}
          {@const f = kids[drag.over]}
          <rect class="pull" x={f.x} y={f.y} width={f.w} height={f.h} stroke-width={2 * scale} />
        {/if}
      {/if}
    </svg>
    {#each pads as p, i (i)}
      {@const [x, y] = padPoint(p, frame)}
      <button class="pad {p.edge}" class:down={p.down} class:fold={p.more} class:lit={lit(i)} type="button" data-pad={p.other} data-way={p.way} title={p.title}
        aria-label={p.more ? `${p.more} more connections, listed on this box's card` : `Open ${p.title}${p.way === "both" ? ", both ways" : ""}${p.down ? ", stopped" : ""}`}
        style:left="{x}px" style:top="{y}px" style:scale={scale} onclick={() => onopen(p.more ? id : p.other)}
        onpointerenter={() => p.more || enter(i)} onpointerleave={() => leave(i)}>
        <span class="dot"></span>
        <span class="tag">
          {#if p.way === "both"}<span class="way" aria-hidden="true">⇄</span>{/if}
          <span class="name">{p.title}</span>
          {#if !p.more && !p.child && asks}<span class="ask" title="Which part? Drag the end of its line onto a box">?</span>{/if}
          {#if p.down && pads.length <= CHIPS}<span class="chip">stopped</span>{/if}
        </span>
      </button>
    {/each}
  </div>
{/if}

<style>
  .pads { position: absolute; inset: 0; z-index: 1; pointer-events: none; }
  svg { position: absolute; left: 0; top: 0; overflow: visible; fill: none; stroke: var(--map-fg-2); }
  circle { fill: var(--map-surface); }
  g.down { stroke: var(--map-faint); }
  g.lit { stroke: var(--map-trace); }
  .hit { pointer-events: stroke; }
  .grab { pointer-events: all; cursor: crosshair; stroke: var(--map-trace); }
  .pull { stroke: var(--map-trace); }
  .pad { --dot: var(--space-3); position: absolute; display: flex; align-items: center; gap: var(--space-1);
    padding: 0; border: 0; background: none; font: inherit; cursor: pointer; pointer-events: auto; }
  .pad.r { translate: 0 -50%; transform-origin: left center; }
  .pad.l { translate: -100% -50%; transform-origin: right center; flex-direction: row-reverse; }
  .pad.t { translate: -50% -100%; transform-origin: center bottom; flex-direction: column-reverse; }
  .pad.b { translate: -50% 0; transform-origin: center top; flex-direction: column; }
  /* the dot straddles the edge */
  .dot { flex: none; box-sizing: border-box; width: var(--dot); height: var(--dot); border: 2px solid var(--map-fg-2); border-radius: 50%; background: var(--map-surface); }
  .pad.r .dot { margin-left: calc(var(--dot) / -2); }
  .pad.l .dot { margin-right: calc(var(--dot) / -2); }
  .pad.t .dot { margin-bottom: calc(var(--dot) / -2); }
  .pad.b .dot { margin-top: calc(var(--dot) / -2); }
  .tag { display: flex; align-items: center; gap: var(--space-1); padding: var(--space-0-5) var(--space-1-5); border: 1px solid var(--map-line); border-radius: var(--radius-md);
    background: var(--map-surface); color: var(--map-fg); font-family: var(--font-sans); font-size: var(--text-xs); line-height: 1.2; white-space: nowrap; }
  .name { max-width: 7rem; overflow: hidden; text-overflow: ellipsis; }
  .way { color: var(--map-fg-2); }
  .ask { padding: 0 var(--space-1); border-radius: var(--radius-pill); background: var(--map-tint-gold); color: var(--map-fg); font-weight: 600; }
  .pad:hover .tag { background: var(--map-sunken); }
  .pad.lit .dot, .pad.lit .tag { border-color: var(--map-trace); }
  .pad.lit .way { color: var(--map-trace); }
  .pad:focus-visible { outline: none; }
  .pad:focus-visible .tag { outline: 3px solid var(--map-blue); outline-offset: 2px; }
  .down .dot, .down .tag { border-color: var(--map-faint); }
  .down .dot { background: var(--map-down-bg); }
  .down .tag { border-style: dotted; color: var(--map-muted); }
  .fold .tag { border-style: dashed; color: var(--map-muted); }
  .chip { padding: 0 var(--space-1); border-radius: var(--radius-pill); background: var(--map-down-bg); color: var(--map-fg-2); font-size: var(--text-2xs); }
</style>
