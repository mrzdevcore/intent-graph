<!-- An open box's connections, in the box's own units: a pad on its edge per connection (right going
     out, left coming in) with the far box's title beside it, and a wire from the child holding its
     end. `scale` keeps pads and text one size on screen. Titles are repo-derived: text only. -->
<script lang="ts">
  import type { Frame, View } from "./types";
  import { inBox, padsOf } from "./pads";

  let { view, id, frame, childFrame, on, scale, onopen }: {
    view: View; id: string; frame: { w: number; h: number }; childFrame: (id: string) => Frame | undefined;
    on: Set<string>; scale: number; onopen: (id: string) => void;
  } = $props();

  // past this many, chips would crowd the labels: the grey says it alone
  const CHIPS = 12;
  const STUB = 24;
  const pads = $derived(padsOf(view, id, frame, childFrame, on));
  const place = $derived(inBox(view, id, frame));
  /** A wire's start: the child's side facing the pad, or a short stub in from the edge. */
  function start(side: "in" | "out", child: string | undefined, y: number) {
    const f = child ? childFrame(child) : undefined;
    if (!f) return { x: side === "out" ? frame.w - STUB * scale : STUB * scale, y, wired: false };
    const c = place(f);
    return { x: side === "out" ? c.x + c.w : c.x, y: c.y + c.h / 2, wired: true };
  }
</script>

{#if pads.length}
  <div class="pads">
    <svg width="1" height="1" aria-hidden="true">
      {#each pads as p, i (i)}
        {@const s = start(p.side, p.child, p.y)}
        <g class:down={p.down} stroke-width={1.5 * scale}>
          <line x1={s.x} y1={s.y} x2={p.side === "out" ? frame.w : 0} y2={p.y} stroke-linecap="round" stroke-dasharray={p.down ? `0 ${4 * scale}` : undefined} />
          {#if s.wired}<circle cx={s.x} cy={s.y} r={3 * scale} />{/if}
        </g>
      {/each}
    </svg>
    {#each pads as p, i (i)}
      <button class="pad {p.side}" class:down={p.down} type="button" data-pad={p.other} title={p.title}
        aria-label="Open {p.title}{p.down ? ', stopped' : ''}" style:top="{p.y}px" style:scale={scale} onclick={() => onopen(p.other)}>
        <span class="dot"></span>
        <span class="tag"><span class="name">{p.title}</span>{#if p.down && pads.length <= CHIPS}<span class="chip">stopped</span>{/if}</span>
      </button>
    {/each}
  </div>
{/if}

<style>
  .pads { position: absolute; inset: 0; z-index: 1; pointer-events: none; }
  svg { position: absolute; left: 0; top: 0; overflow: visible; fill: var(--map-surface); stroke: var(--map-trace); }
  g.down { stroke: var(--map-faint); }
  .pad { --dot: var(--space-3); position: absolute; display: flex; align-items: center; gap: var(--space-1); translate: 0 -50%;
    padding: 0; border: 0; background: none; font: inherit; cursor: pointer; pointer-events: auto; }
  .pad.out { left: 100%; transform-origin: left center; }
  .pad.in { right: 100%; transform-origin: right center; flex-direction: row-reverse; }
  /* the dot straddles the edge */
  .dot { flex: none; box-sizing: border-box; width: var(--dot); height: var(--dot); border: 2px solid var(--map-trace); border-radius: 50%; background: var(--map-surface); }
  .pad.out .dot { margin-left: calc(var(--dot) / -2); }
  .pad.in .dot { margin-right: calc(var(--dot) / -2); }
  .tag { display: flex; align-items: center; gap: var(--space-1); padding: var(--space-0-5) var(--space-1-5); border: 1px solid var(--map-trace); border-radius: var(--radius-md);
    background: var(--map-surface); color: var(--map-fg); font-family: var(--font-sans); font-size: var(--text-xs); line-height: 1.2; white-space: nowrap; }
  .name { max-width: 7rem; overflow: hidden; text-overflow: ellipsis; }
  .pad:hover .tag { background: var(--map-sunken); }
  .pad:focus-visible { outline: none; }
  .pad:focus-visible .tag { outline: 3px solid var(--map-blue); outline-offset: 2px; }
  .down .dot, .down .tag { border-color: var(--map-faint); }
  .down .dot { background: var(--map-down-bg); }
  .down .tag { border-style: dotted; color: var(--map-muted); }
  .chip { padding: 0 var(--space-1); border-radius: var(--radius-pill); background: var(--map-down-bg); color: var(--map-fg-2); font-size: var(--text-2xs); }
</style>
