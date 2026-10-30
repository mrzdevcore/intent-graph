<!-- The editing toolbar (top left, under the breadcrumb): tools, undo, redo, and the toast of a refused edit. -->
<script lang="ts">
  import type { Tool } from "./tool.svelte";

  let { tool, onpick, onundo, onredo, note }: {
    tool: Tool; onpick: (t: Tool) => void; onundo: () => void; onredo: () => void; note: string;
  } = $props();
  const tools: { id: Tool; label: string; key: string }[] = [
    { id: "select", label: "Select", key: "V" },
    { id: "box", label: "+ Box", key: "B" },
    { id: "arrow", label: "→ Arrow", key: "A" },
  ];
</script>

<div class="wrap">
<div class="bar" role="toolbar" aria-label="Edit the map">
  {#each tools as t (t.id)}
    <button type="button" aria-pressed={tool === t.id} onclick={() => onpick(t.id)}>{t.label} <kbd>{t.key}</kbd></button>
  {/each}
  <span class="sep" aria-hidden="true"></span>
  <button type="button" aria-label="Undo" title="Undo (⌘Z)" onclick={onundo}>↶</button>
  <button type="button" aria-label="Redo" title="Redo (⌘⇧Z)" onclick={onredo}>↷</button>
</div>
<div class="toast" role="status" aria-live="polite">{note}</div>
</div>

<style>
  .wrap { display: flex; flex-direction: column; align-items: flex-start; gap: var(--space-1-5); }
  .bar { display: flex; gap: var(--space-1-5); align-items: center; }
  button { font: inherit; font-size: var(--text-md); border: 1px solid var(--map-line); background: var(--map-surface); color: var(--map-fg); border-radius: var(--radius-lg); padding: 0.25rem var(--space-2-5); min-height: 2rem; cursor: pointer; }
  button[aria-pressed="true"] { border-color: var(--map-trace); background: var(--map-sunken); }
  button:focus-visible { outline: 2px solid var(--map-trace); outline-offset: 2px; }
  kbd { font: inherit; font-size: var(--text-sm); color: var(--map-faint); }
  .sep { width: 1px; height: 1.375rem; background: var(--map-line); }
  .toast { max-width: 28rem; background: var(--map-bad-bg); color: var(--map-bad-text); border-radius: var(--radius-lg); font-size: var(--text-md); }
  .toast:not(:empty) { padding: var(--space-1-5) var(--space-2-5); }
</style>
