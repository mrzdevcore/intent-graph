<!-- An arrow, in the parent's inner units: its wire and head (`part` "line"), or its label pill
     cutting the wire halfway (`part` "label", drawn above the boxes). `zoom` is screen pixels per inner unit: lines, heads and pills keep a steady size on
     screen, so a parent's arrows never turn into bars once you are deep inside one of its boxes.
     At rest a plain arrow is faint; lit (its box or itself under the pointer, or selected) it is
     drawn in full, and dimmed further while another is lit. A rule's or risk's stays in full. -->
<script lang="ts" module>
  let n = 0;
  const REST = 0.35, DIM = 0.12;
</script>
<script lang="ts">
  import type { ArrowLook } from "./looks";
  import { steady } from "./camera";

  let { d, mid, arrowLook, part = "line", zoom = 1, lit = false, dim = false, picked = false, note, len = Infinity, ends, onhover }: {
    d: string; mid: [number, number]; arrowLook: ArrowLook; part?: "line" | "label"; zoom?: number;
    lit?: boolean; dim?: boolean; picked?: boolean; note?: string; len?: number; ends?: [[number, number], [number, number]]; onhover?: (on: boolean) => void;
  } = $props();

  const id = `ig-head-${n++}`;
  const m = $derived(steady(zoom));
  // Kalam runs wide: about 0.6 em a character at 12
  const w = $derived((arrowLook.label?.length ?? 0) * 7.4 + 18);
  const state = $derived(lit ? "lit" : dim ? "dim" : "rest");
  const alpha = $derived(lit || arrowLook.strong || arrowLook.tone !== "neutral" ? 1 : dim ? DIM : REST);
  const NOTE_CH = 5.2;
  const line = $derived(cut(note ?? "", Math.floor((len / m - 8) / NOTE_CH)));
  function cut(t: string, n: number) {
    return t.length <= n ? t : n < 2 ? "" : `${t.slice(0, n - 1).trimEnd()}…`;
  }
  const color = $derived(picked ? "var(--map-trace)" : arrowLook.color);
  const TEXT = { neutral: "var(--map-fg)", warn: "var(--map-warn-text)", bad: "var(--map-bad-text)", down: "var(--map-fg-2)" };
</script>

<svg style="position:absolute;left:0;top:0;overflow:visible;pointer-events:none;fill:none" width="1" height="1" aria-hidden="true">
  {#if part === "line"}
    <defs>
      <marker {id} orient="auto" markerUnits="userSpaceOnUse" markerWidth="12" markerHeight="12" refX={(arrowLook.head === "filled" ? 4 : 6) * m} refY={4 * m} overflow="visible">
        <g transform="scale({m})">
          {#if arrowLook.head === "filled"}<path d="M0 0 L8 4 L0 8 Z" style:fill={color} />
          {:else if arrowLook.head === "open"}<path d="M0 0 L6 4 L0 8" fill="none" style:stroke={color} stroke-width="1.5" />
          {:else}<path d="M0 0 L8 4 L0 8 Z" style:fill="var(--map-ground)" style:stroke={color} stroke-width="1.5" />{/if}
        </g>
      </marker>
    </defs>
    <g opacity={picked ? 1 : alpha} data-state={picked ? "picked" : state} style="transition: opacity 150ms">
      <path {d} style:stroke={color} stroke-width={(arrowLook.width + (lit || picked ? 0.75 : 0)) * m}
        stroke-dasharray={arrowLook.dash?.split(" ").map((d) => Number(d) * m).join(" ") ?? undefined}
        stroke-linecap="round" stroke-linejoin="round" marker-end="url(#{id})" />
      {#if onhover}
        <!-- hover only lights the arrow up: the details card holds the same facts for keyboards -->
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <path class="hit" data-hit {d} stroke="transparent" stroke-width={16 * m}
          onpointerenter={() => onhover(true)} onpointerleave={() => onhover(false)} />
      {/if}
    </g>
  {:else}
    {#if arrowLook.label || line}
      <g transform="translate({mid[0]} {mid[1]}) scale({m})" opacity={picked ? 1 : alpha} data-state={picked ? "picked" : state} style="transition: opacity 150ms">
        {#if arrowLook.label}
          <rect x={-w / 2} y="-10" width={w} height="20" rx="10" style:fill="var(--map-ground)" style:stroke={color} stroke-width="1" />
          <text y="4" text-anchor="middle" font-size="12" style:font-family="var(--font-hand)" style:fill={TEXT[arrowLook.tone]}>{arrowLook.label}</text>
        {/if}
        {#if line}
          <text class="note" data-note y={arrowLook.label ? 22 : 3.5} text-anchor="middle" font-size="10">{line}</text>
        {/if}
      </g>
    {/if}
    {#if picked && ends}
      {#each ends as [x, y], i (i)}
        <circle class="end" data-end={i ? "to" : "from"} cx={x} cy={y} r={5 * m} stroke-width={1.5 * m} />
      {/each}
    {/if}
  {/if}
</svg>

<style>
  .hit { pointer-events: stroke; cursor: pointer; }
  .end { pointer-events: all; cursor: crosshair; fill: var(--map-ground); stroke: var(--map-trace); }
  .note { font-family: var(--font-hand); fill: var(--map-muted); stroke: var(--map-ground); stroke-width: 3px; paint-order: stroke; stroke-linejoin: round; }
</style>
