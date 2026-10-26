<!-- The board's text: one layer carrying the camera as a single CSS transform, the labels in
     world px inside it (the engine culls them to the viewport). Hidden below 6 px on screen. -->
<script lang="ts">
  import type { Camera, Label } from "./engine";

  let { camera, labels }: { camera: Camera; labels: Label[] } = $props();
  const FONT = 12;
</script>

<div class="labels" aria-hidden="true" class:tiny={FONT * camera.zoom < 6}
  style="transform: translate({-camera.x * camera.zoom}px, {-camera.y * camera.zoom}px) scale({camera.zoom})">
  {#each labels as l (l.id)}
    <div class="label" class:faded={l.faded} style="left:{l.r[0]}px;top:{l.r[1]}px;width:{l.r[2]}px;height:{l.r[3]}px">{l.text}</div>
  {/each}
</div>

<style>
  .labels { position: absolute; left: 0; top: 0; transform-origin: 0 0; pointer-events: none; font-size: 12px; }
  .tiny { visibility: hidden; }
  .label { position: absolute; display: flex; align-items: center; justify-content: center; text-align: center;
    padding: 0 6px; box-sizing: border-box; overflow: hidden; color: var(--fg); line-height: 1.2; }
  .faded { color: var(--muted); }
</style>
