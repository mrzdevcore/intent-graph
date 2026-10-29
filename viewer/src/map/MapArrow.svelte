<!-- An arrow, in the parent's inner units: its curve and head (`part` "line"), or its label pill
     cutting the curve halfway (`part` "label", drawn above the boxes). `zoom` is screen pixels per inner unit: lines, heads and pills keep a steady size on
     screen, so a parent's arrows never turn into bars once you are deep inside one of its boxes.
     At rest a plain arrow is faint; lit (its box or itself under the pointer, or selected) it is
     drawn in full, and dimmed further while another is lit. A rule's or risk's stays in full. -->
<script lang="ts" module>
  let n = 0;
  const REST = 0.35, DIM = 0.12;
</script>
<script lang="ts">
  import type { arrowLook as AL } from "./looks";
  import { steady } from "./camera";

  let { d, mid, arrowLook, part = "line", zoom = 1, lit = false, dim = false, onhover }: {
    d: string; mid: [number, number]; arrowLook: ReturnType<typeof AL>; part?: "line" | "label"; zoom?: number;
    lit?: boolean; dim?: boolean; onhover?: (on: boolean) => void;
  } = $props();

  const id = `ig-head-${n++}`;
  const m = $derived(steady(zoom));
  const w = $derived((arrowLook.label?.length ?? 0) * 6.5 + 16);
  const state = $derived(lit ? "lit" : dim ? "dim" : "rest");
  const alpha = $derived(lit || arrowLook.tone !== "neutral" ? 1 : dim ? DIM : REST);
  const TEXT = { neutral: "#23211e", warn: "#6b4200", bad: "#9b2c1f", down: "#4a4640" };
</script>

<svg style="position:absolute;left:0;top:0;overflow:visible;pointer-events:none;fill:none" width="1" height="1" aria-hidden="true">
  {#if part === "line"}
    <defs>
      <marker {id} orient="auto" markerUnits="userSpaceOnUse" markerWidth="12" markerHeight="12" refX={(arrowLook.head === "filled" ? 4 : 6) * m} refY={4 * m} overflow="visible">
        <g transform="scale({m})">
          {#if arrowLook.head === "filled"}<path d="M0 0 L8 4 L0 8 Z" fill={arrowLook.color} />
          {:else if arrowLook.head === "open"}<path d="M0 0 L6 4 L0 8" fill="none" stroke={arrowLook.color} stroke-width="1.5" />
          {:else}<path d="M0 0 L8 4 L0 8 Z" fill="#faf8f3" stroke={arrowLook.color} stroke-width="1.5" />{/if}
        </g>
      </marker>
    </defs>
    <g opacity={alpha} data-state={state} style="transition: opacity 150ms">
      <path {d} stroke={arrowLook.color} stroke-width={(arrowLook.width + (lit ? 0.75 : 0)) * m}
        stroke-dasharray={arrowLook.dash?.split(" ").map((d) => Number(d) * m).join(" ") ?? undefined}
        stroke-linecap="round" stroke-linejoin="round" marker-end="url(#{id})" />
      {#if onhover}
        <!-- hover only lights the arrow up: the details card holds the same facts for keyboards -->
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <path class="hit" {d} stroke="transparent" stroke-width={12 * m}
          onpointerenter={() => onhover(true)} onpointerleave={() => onhover(false)} />
      {/if}
    </g>
  {:else if arrowLook.label}
    <g transform="translate({mid[0]} {mid[1]}) scale({m})">
      <rect x={-w / 2} y="-10" width={w} height="20" rx="10" fill="#faf8f3" stroke={arrowLook.color} stroke-width="1" />
      <text y="4" text-anchor="middle" font-size="11" font-family="system-ui, sans-serif" fill={TEXT[arrowLook.tone]} style="fill:{TEXT[arrowLook.tone]}">{arrowLook.label}</text>
    </g>
  {/if}
</svg>

<style>
  .hit { pointer-events: stroke; cursor: default; }
</style>
