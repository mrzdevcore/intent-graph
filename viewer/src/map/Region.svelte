<!-- A ring (onion), band (layered) or group (bounded context), placed by its frame in the parent's inner units. -->
<script lang="ts">
  import type { Region } from "./types";

  let { region }: { region: Region } = $props();
  const f = $derived(region.frame);
  const domain = $derived(/domain/i.test(region.label));
</script>

<div class="region {region.kind}" class:domain aria-hidden="true"
  style:left="{f.x}px" style:top="{f.y}px" style:width="{f.w}px" style:height="{f.h}px">
  <span class="label">{region.label}</span>
</div>

<style>
  .region { position: absolute; box-sizing: border-box; pointer-events: none; }
  .label { position: absolute; font: 600 10px system-ui, sans-serif; letter-spacing: 0.08em; text-transform: uppercase; }
  .ring { background: #eef2f8; border: 1.5px solid #a9bad3; border-radius: 72px 64px 76px 66px; color: #4b5f80; }
  .ring.domain { background: #fcf3d9; border-color: #d4ab4a; border-radius: 40px 36px 44px 38px; color: #7a5a0f; }
  .ring .label { left: 40px; top: 16px; }
  .band { background: #f2eee5; border-radius: 12px; color: #5c5850; }
  .band .label { left: 12px; top: 8px; }
  .group { background: rgba(255, 255, 255, 0.6); border: 1.5px dashed #8a8378; border-radius: 16px; }
  .group .label { left: 16px; top: -12px; height: 24px; display: flex; align-items: center; padding: 0 10px; background: #faf8f3; border: 1px solid #8a8378; border-radius: 999px; font: 600 12px system-ui, sans-serif; letter-spacing: 0; text-transform: none; color: #23211e; }
</style>
