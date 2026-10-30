<!-- Rename in place: an input over the box's title. Enter keeps, Esc or leaving the field cancels. -->
<script lang="ts">
  import { onMount } from "svelte";
  let { value, x, y, w, onsubmit, onclose }: { value: string; x: number; y: number; w: number; onsubmit: (v: string) => void; onclose: () => void } = $props();
  let v = $state(""), done = false, field: HTMLInputElement;
  onMount(() => { v = value; field.focus(); field.select(); });
  const finish = (fn: () => void) => { if (!done) { done = true; fn(); } };
  function keydown(e: KeyboardEvent) {
    if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); finish(onclose); }
    else if (e.key === "Enter") { e.preventDefault(); finish(() => onsubmit(v.trim())); }
  }
</script>

<input bind:this={field} bind:value={v} aria-label="Rename box" autocomplete="off" spellcheck="false" style:left="{x}px" style:top="{y}px" style:width="max(10rem, {w}px)"
  onkeydown={keydown} onblur={() => finish(onclose)} />

<style>
  input { position: absolute; z-index: 6; box-sizing: border-box; font: inherit; font-size: var(--text-lg); font-weight: 600; color: var(--map-fg); background: var(--map-surface);
    border: 1px solid var(--map-gold); border-radius: var(--radius-md); padding: var(--space-1) var(--space-2); }
  input:focus-visible { outline: 2px solid var(--map-trace); outline-offset: 2px; }
</style>
