<!-- A ring (onion), band (layered) or group (bounded context), placed by its frame in the parent's inner units. -->
<script lang="ts">
  import type { Region } from "./types";
  import { steady } from "./camera";

  // zoom: screen pixels per unit; the label keeps a steady size on screen
  let { region, zoom = 1 }: { region: Region; zoom?: number } = $props();
  const m = $derived(steady(zoom));
  const f = $derived(region.frame);
  const role = $derived(/domain/i.test(region.label) ? "domain" : /application/i.test(region.label) ? "application" : "infra");
</script>

<div class="region {region.kind}" data-role={role} aria-hidden="true"
  style:left="{f.x}px" style:top="{f.y}px" style:width="{f.w}px" style:height="{f.h}px">
  <span class="label" style:transform="scale({m})">{region.label}</span>
</div>

<style>
  .region { position: absolute; box-sizing: border-box; pointer-events: none; }
  .label { position: absolute; transform-origin: 0 0; font: 600 10px var(--font-sans); letter-spacing: 0.08em; text-transform: uppercase; }
  .ring { background: var(--map-ring-bg); border: 1.5px solid var(--map-ring-line); border-radius: 72px 64px 76px 66px; color: var(--map-ring-text); }
  .ring[data-role="domain"] { background: var(--map-domain-bg); border-color: var(--map-domain-line); border-radius: 40px 36px 44px 38px; color: var(--map-domain-text); }
  .ring[data-role="application"] { background: var(--map-app-bg); border-color: var(--map-app-line); border-radius: 56px 50px 60px 52px; color: var(--map-app-text); }
  .ring .label { left: 40px; top: 16px; }
  .band { background: var(--map-sunken); border-radius: 12px; color: var(--map-muted); }
  .band .label { left: 12px; top: 8px; }
  .group { background: var(--map-group-bg); border: 1.5px dashed var(--map-faint); border-radius: 16px; }
  .group .label { left: 16px; top: -12px; height: 24px; display: flex; align-items: center; padding: 0 10px; background: var(--map-ground); border: 1px solid var(--map-faint); border-radius: 999px; font: 600 12px var(--font-sans); letter-spacing: 0; text-transform: none; color: var(--map-fg); }
</style>
