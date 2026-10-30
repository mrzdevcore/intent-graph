<!-- Over the map: the breadcrumb (top left), the search (`children`), the layers toggles, zoom −,
     Fit, + and, once boxes were moved by hand, Reset layout (top right). -->
<script lang="ts">
  import type { Snippet } from "svelte";

  let { crumbs, layers, on, oncrumb, ontoggle, onzoom, onfit, onreset, bar, children }: {
    crumbs: { id: string; title: string }[]; layers: string[]; on: Set<string>;
    oncrumb: (id: string) => void; ontoggle: (layer: string) => void; onzoom: (f: number) => void; onfit: () => void;
    onreset?: () => void; bar?: Snippet; children?: Snippet;
  } = $props();
</script>

<div class="head">
<nav class="crumbs" aria-label="Where you are">
  {#each crumbs as c, i (c.id)}
    {#if i}<span aria-hidden="true">›</span>{/if}
    <button type="button" aria-current={i === crumbs.length - 1 ? "location" : undefined} onclick={() => oncrumb(c.id)}>{c.title}</button>
  {/each}
</nav>
{@render bar?.()}
</div>

<div class="tools">
  {@render children?.()}
  {#if onreset}<button type="button" onclick={onreset}>Reset layout</button>{/if}
  <details class="layers">
    <summary>Layers</summary>
    {#each layers as l (l)}
      <label><input type="checkbox" checked={on.has(l)} onchange={() => ontoggle(l)} /> {l}</label>
    {/each}
  </details>
  <div class="zoom">
    <button type="button" aria-label="Zoom out" onclick={() => onzoom(1 / 1.6)}>−</button>
    <button type="button" onclick={onfit}>Fit</button>
    <button type="button" aria-label="Zoom in" onclick={() => onzoom(1.6)}>+</button>
  </div>
</div>

<style>
  .head { position: absolute; left: var(--space-4); top: var(--space-3); right: 36rem; display: flex; flex-direction: column; align-items: flex-start; gap: var(--space-1-5); pointer-events: none; }
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
  .zoom { display: flex; gap: var(--space-0-5); }
  @media (max-width: 47.5rem) { .head { right: var(--space-4); top: 3.25rem; } }
</style>
