<!-- Bottom left, two small islands: zoom out, the zoom readout (fits the map on click), zoom in, Fit; and, when the map can be edited, undo and redo. -->
<script lang="ts">
  import { Minus, Plus, Redo2, Scan, Undo2 } from "lucide-static";

  let { k, onzoom, onfit, onundo, onredo }: {
    k: number; onzoom: (f: number) => void; onfit: () => void; onundo?: () => void; onredo?: () => void;
  } = $props();
  const mask = (svg: string) => `url("data:image/svg+xml,${encodeURIComponent(svg)}")`;
  const pct = $derived(!Number.isFinite(k) ? "–" : k >= 10 ? `${Math.round(k).toLocaleString()}×` : `${Math.round(k * 100)}%`);
</script>

<div class="corner">
  <div class="isle">
    <button type="button" aria-label="Zoom out" onclick={() => onzoom(1 / 1.6)}><i style:mask-image={mask(Minus)} aria-hidden="true"></i></button>
    <button type="button" class="pct" aria-label="Fit the map" title="Fit the map (⇧1)" onclick={onfit}>{pct}</button>
    <button type="button" aria-label="Zoom in" onclick={() => onzoom(1.6)}><i style:mask-image={mask(Plus)} aria-hidden="true"></i></button>
    <button type="button" class="fit" title="Fit the map (⇧1)" onclick={onfit}><i style:mask-image={mask(Scan)} aria-hidden="true"></i>Fit</button>
  </div>
  {#if onundo && onredo}
    <div class="isle">
      <button type="button" aria-label="Undo" title="Undo (⌘Z)" onclick={onundo}><i style:mask-image={mask(Undo2)} aria-hidden="true"></i></button>
      <button type="button" aria-label="Redo" title="Redo (⌘⇧Z)" onclick={onredo}><i style:mask-image={mask(Redo2)} aria-hidden="true"></i></button>
    </div>
  {/if}
</div>

<style>
  .corner { position: absolute; z-index: 3; left: var(--space-4); bottom: var(--space-4); display: flex; gap: var(--space-2); }
  .isle { display: flex; align-items: center; padding: var(--space-0-5); background: var(--map-surface); border: 1px solid var(--map-line); border-radius: var(--radius-lg);
    box-shadow: 0 0.25rem 0.75rem var(--map-shadow); }
  button { min-width: 2rem; height: 2rem; display: inline-flex; align-items: center; justify-content: center; padding: 0 var(--space-1-5); border: 0; border-radius: var(--radius-md);
    background: none; color: var(--map-fg-2); font: inherit; font-size: var(--text-sm); font-variant-numeric: tabular-nums; cursor: pointer; }
  button:hover { background: var(--map-sunken); }
  button:focus-visible { outline: 2px solid var(--map-trace); outline-offset: 1px; }
  .pct { min-width: 3.25rem; }
  .fit { gap: var(--space-1); margin-left: var(--space-0-5); border-left: 1px solid var(--map-line); border-radius: 0 var(--radius-md) var(--radius-md) 0; }
  i { width: 1rem; height: 1rem; background: currentColor; mask-repeat: no-repeat; mask-position: center; mask-size: contain; }
</style>
