<!-- Over the map: the breadcrumb (top left), the layers toggles, zoom −, Fit, + and, once boxes
     were moved by hand, Reset layout (top right). -->
<script lang="ts">
  let { crumbs, layers, on, oncrumb, ontoggle, onzoom, onfit, onreset }: {
    crumbs: { id: string; title: string }[]; layers: string[]; on: Set<string>;
    oncrumb: (id: string) => void; ontoggle: (layer: string) => void; onzoom: (f: number) => void; onfit: () => void;
    onreset?: () => void;
  } = $props();
</script>

<nav class="crumbs" aria-label="Where you are">
  {#each crumbs as c, i (c.id)}
    {#if i}<span aria-hidden="true">›</span>{/if}
    <button type="button" aria-current={i === crumbs.length - 1 ? "location" : undefined} onclick={() => oncrumb(c.id)}>{c.title}</button>
  {/each}
</nav>

<div class="tools">
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
  .crumbs { position: absolute; left: 16px; top: 12px; right: 260px; display: flex; flex-wrap: wrap; gap: 4px; align-items: center; color: #8a8378; font-size: 13px; }
  .tools { position: absolute; right: 16px; top: 12px; display: flex; gap: 8px; align-items: start; }
  button, summary { font: inherit; font-size: 13px; border: 1px solid #d9d2c3; background: #fffdf8; color: #23211e; border-radius: 8px; padding: 3px 10px; cursor: pointer; }
  .crumbs button { border-color: transparent; background: none; padding: 2px 6px; }
  .crumbs button[aria-current] { font-weight: 600; }
  .layers { background: #fffdf8; border-radius: 8px; }
  .layers[open] { border: 1px solid #d9d2c3; padding-bottom: 6px; }
  .layers[open] summary { border: 0; }
  .layers summary { list-style: none; }
  .layers label { display: flex; gap: 6px; align-items: center; padding: 2px 10px; font-size: 13px; white-space: nowrap; }
  .zoom { display: flex; gap: 2px; }
  @media (max-width: 760px) { .crumbs { right: 16px; top: 52px; } }
</style>
