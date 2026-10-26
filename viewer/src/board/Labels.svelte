<!-- The board's text: one layer carrying the camera as a single CSS transform, the labels in
     world px inside it (the engine culls them to the viewport), hidden below 6 px on screen.
     Screen names (and their state chips) sit in a second, unscaled layer placed from the camera:
     12 px at any zoom, cut with an ellipsis at the frame's width. -->
<script lang="ts">
  import type { Camera, Label } from "./engine";

  /** `hand`: the screens' handwriting (Kalam, bundled: ./fonts, OFL 1.1). */
  let { camera, labels, hand = false }: { camera: Camera; labels: Label[]; hand?: boolean } = $props();
  const FONT = 12;
  const names = $derived(labels.filter((l) => l.name));
  const rest = $derived(labels.filter((l) => !l.name));
</script>

<div class="labels" aria-hidden="true" class:tiny={FONT * camera.zoom < 6} class:hand
  style="transform: translate({-camera.x * camera.zoom}px, {-camera.y * camera.zoom}px) scale({camera.zoom})">
  {#each rest as l (l.id)}
    <div class="label" class:faded={l.faded} class:paper={l.paper} style="left:{l.r[0]}px;top:{l.r[1]}px;width:{l.r[2]}px;height:{l.r[3]}px">{l.text}</div>
  {/each}
</div>
{#if names.length}
  <div class="names" class:hand aria-hidden="true">
    {#each names as l (l.id)}
      <div class="name" style="left:{(l.r[0] - camera.x) * camera.zoom}px;top:{(l.r[1] + l.r[3] - camera.y) * camera.zoom - 18}px;width:{l.r[2] * camera.zoom}px">
        <span class="text">{l.text}</span>{#if l.chip}<span class="chip">{l.chip}</span>{/if}</div>
    {/each}
  </div>
{/if}

<style>
  @font-face { font-family: "Kalam"; src: url("./fonts/Kalam-Regular.woff") format("woff"); font-display: swap; }
  .labels { position: absolute; left: 0; top: 0; transform-origin: 0 0; pointer-events: none; font-size: 12px; }
  .tiny { visibility: hidden; }
  .label { position: absolute; display: flex; align-items: center; justify-content: center; text-align: center;
    padding: 0 6px; box-sizing: border-box; overflow: hidden; color: var(--fg); line-height: 1.2; }
  .faded { color: var(--muted); }
  .hand { font-family: "Kalam", "Bradley Hand", "Chalkboard SE", cursive; font-size: 14px; }
  .paper { color: var(--ink); }
  .names { position: absolute; inset: 0; pointer-events: none; overflow: hidden; }
  .name { position: absolute; display: flex; align-items: center; gap: 6px; height: 16px; font-size: 12px; line-height: 16px;
    color: var(--fg); white-space: nowrap; min-width: 0; }
  .text { overflow: hidden; text-overflow: ellipsis; min-width: 0; }
  .chip { flex: none; padding: 0 5px; border-radius: 3px; font-size: 10px; letter-spacing: 0.04em; font-family: system-ui, sans-serif;
    color: var(--bg); background: var(--warning); }
</style>
