<script lang="ts">
  import type { Landscape, Skipped } from "./types";

  let { landscape }: { landscape: Landscape } = $props();
  let open = $state(false);
  const sections = $derived<[string, Skipped[]][]>([["To decide", landscape.unsure], ["Projects", landscape.projects], ["Left out", landscape.leftOut]]);
  const side = $derived(landscape.unsure.length + landscape.leftOut.length);
</script>

<section class="folders" aria-label="Folders">
  <button type="button" class="line" aria-expanded={open} onclick={() => (open = !open)}>
    <span>{landscape.projects.length} projects{#if side}, {side} other {side === 1 ? "folder" : "folders"}{/if}</span>
    <span aria-hidden="true">{open ? "⌄" : "⌃"}</span>
  </button>
  {#if open}
    <div class="list">
      {#each sections as [name, list] (name)}
        {#if list.length}
          <h3>{name} · {list.length}</h3>
          <ul>{#each list as f (f.dir)}<li><code>{f.dir}</code><span>{f.why}</span></li>{/each}</ul>
        {/if}
      {/each}
    </div>
  {/if}
</section>

<style>
  .folders { position: absolute; z-index: 2; right: var(--space-4); bottom: var(--space-4); width: 24rem; max-width: calc(100% - 2rem); max-height: 70%; display: flex; flex-direction: column-reverse; background: var(--map-surface); color: var(--map-fg); border: 1px solid var(--map-line); border-radius: var(--radius-xl); box-shadow: 0 6px 20px var(--map-shadow); font-size: var(--text-md); overflow: hidden; }
  .line { display: flex; justify-content: space-between; gap: var(--space-2); padding: var(--space-2) var(--space-3); font: inherit; font-weight: 600; color: inherit; background: none; border: 0; cursor: pointer; text-align: left; }
  .line:focus-visible { outline: 2px solid var(--map-trace); outline-offset: -2px; }
  .list { overflow: auto; padding: var(--space-1) var(--space-3) var(--space-2); border-bottom: 1px solid var(--map-line-soft); }
  h3 { margin: var(--space-2) 0 var(--space-1); font-size: var(--text-xs); text-transform: uppercase; letter-spacing: 0.06em; color: var(--map-muted); }
  ul { margin: 0; padding: 0; list-style: none; display: flex; flex-direction: column; gap: var(--space-1); }
  li { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); gap: var(--space-2); }
  code { background: none; color: inherit; overflow-wrap: anywhere; }
  li span { color: var(--map-muted); font-size: var(--text-sm); }
</style>
