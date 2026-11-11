<!-- Over the map: the breadcrumb (top left), the search (`children`), the layers toggles and More top right. Zoom sits in the bottom-left corner. -->
<script lang="ts">
  import type { Snippet } from "svelte";
  import type { ArrowShape } from "./types";
  import { SHAPES, type MapShape } from "./shapes/auto";
  import type { Notation } from "./db/ends";
  import type { Noise } from "./db/noise";

  const SWITCHES: [keyof Noise, string][] = [
    ["translations", "Fold translations"], ["leftovers", "Set leftovers aside"], ["hubs", "Pin hubs"], ["joins", "Fold join tables"], ["bookkeeping", "Fold bookkeeping"],
  ];
  let { crumbs, layers, on, oncrumb, ontoggle, wheelPref, onwheel, arrowPref = "auto", arrowHere = "circuit", onarrows, endsPref = "crowsfoot", onends, noise, onnoise, onarrange, low = false, children }: {
    crumbs: { id: string; title: string }[]; layers: string[]; on: Set<string>;
    oncrumb: (id: string) => void; ontoggle: (layer: string) => void;
    wheelPref: "auto" | "zoom" | "pan"; onwheel: (p: "auto" | "zoom" | "pan") => void;
    arrowPref?: MapShape; arrowHere?: ArrowShape; onarrows?: (p: MapShape) => void;
    endsPref?: Notation; onends?: (n: Notation) => void; noise?: Noise; onnoise?: (k: keyof Noise, on: boolean) => void; onarrange?: () => void; low?: boolean; children?: Snippet;
  } = $props();
</script>

<div class="head" class:low>
<nav class="crumbs" aria-label="Where you are">
  {#each crumbs as c, i (c.id)}
    {#if i}<span aria-hidden="true">›</span>{/if}
    <button type="button" aria-current={i === crumbs.length - 1 ? "location" : undefined} onclick={() => oncrumb(c.id)}>{c.title}</button>
  {/each}
</nav>
</div>

<div class="tools">
  {@render children?.()}
  <details class="layers">
    <summary>Layers</summary>
    {#each layers as l (l)}
      <label><input type="checkbox" checked={on.has(l)} onchange={() => ontoggle(l)} /> {l}</label>
    {/each}
  </details>
  <details class="layers more">
    <summary>More</summary>
    <label>Scroll wheel
      <select value={wheelPref} onchange={(e) => onwheel(e.currentTarget.value as "auto" | "zoom" | "pan")}>
        <option value="auto">Automatic</option><option value="zoom">Zoom</option><option value="pan">Pan</option>
      </select>
    </label>
    {#if onarrows}
      <label>Arrows
        <select value={arrowPref} onchange={(e) => onarrows(e.currentTarget.value as MapShape)}>
          <option value="auto">Auto</option>{#each SHAPES as s (s)}<option value={s}>{s[0].toUpperCase() + s.slice(1)}</option>{/each}
        </select>
        {#if arrowPref === "auto"}<span class="here" data-here>· {arrowHere} here</span>{/if}
      </label>
    {/if}
    <label>Ends
      <select value={endsPref} onchange={(e) => onends?.(e.currentTarget.value as Notation)}>
        <option value="crowsfoot">Crow's foot</option><option value="labels">Labels</option><option value="both">Both</option>
      </select>
    </label>
    {#if noise && onnoise}
      {#each SWITCHES as [k, text] (k)}
        <label><input type="checkbox" checked={noise[k]} onchange={(e) => onnoise(k, e.currentTarget.checked)} /> {text}</label>
      {/each}
    {/if}
    {#if onarrange}<button type="button" class="item" onclick={onarrange}>Arrange the map <kbd>⌥⇧F</kbd></button>{/if}
  </details>
</div>

<style>
  .head { position: absolute; z-index: 3; left: var(--space-4); top: var(--space-3); right: 36rem; display: flex; flex-direction: column; align-items: flex-start; gap: var(--space-1-5); pointer-events: none; }
  .head > :global(*) { pointer-events: auto; }
  .crumbs { display: flex; flex-wrap: wrap; gap: var(--space-1); align-items: center; color: var(--map-faint); font-size: var(--text-md); }
  .tools { position: absolute; z-index: 3; right: var(--space-4); top: var(--space-3); display: flex; gap: var(--space-2); align-items: start; }
  button, summary { font: inherit; font-size: var(--text-md); border: 1px solid var(--map-line); background: var(--map-surface); color: var(--map-fg); border-radius: var(--radius-lg); padding: 0.1875rem var(--space-2-5); cursor: pointer; }
  .crumbs button { border-color: transparent; background: none; padding: var(--space-0-5) var(--space-1-5); }
  .crumbs button[aria-current] { font-weight: 600; }
  .layers { background: var(--map-surface); border-radius: var(--radius-lg); }
  .layers[open] { border: 1px solid var(--map-line); padding-bottom: var(--space-1-5); }
  .layers[open] summary { border: 0; }
  .layers summary { list-style: none; }
  .layers label { display: flex; gap: var(--space-1-5); align-items: center; padding: var(--space-0-5) var(--space-2-5); font-size: var(--text-md); white-space: nowrap; }
  .layers .item { display: flex; justify-content: space-between; gap: var(--space-3); width: 100%; border: 0; background: none; text-align: left; white-space: nowrap; }
  .layers .item:hover { background: var(--map-sunken); }
  .here { color: var(--map-faint); }
  .layers kbd { font: inherit; font-size: var(--text-sm); color: var(--map-faint); }
  @media (max-width: 47.5rem) { .head { right: var(--space-4); top: 3.25rem; } .head.low { top: 6.25rem; } }
</style>
