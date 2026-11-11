<script lang="ts">
  import { onMount, type Snippet } from "svelte";

  let { what, label, focus, onclose, head, children }: {
    what: "box" | "arrow"; label: string; focus?: string; onclose: () => void; head: Snippet; children: Snippet;
  } = $props();
  let panel: HTMLElement;
  onMount(() => {
    const f = focus ? panel.querySelector<HTMLElement>(focus) : null;
    (f ?? panel).focus({ preventScroll: true });
  });
  function keydown(e: KeyboardEvent) {
    if (e.key !== "Escape") return;
    e.preventDefault();
    e.stopPropagation();
    (document.activeElement as HTMLElement | null)?.blur();
    onclose();
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<section class="panel" data-panel={what} aria-label={label} tabindex="-1" bind:this={panel} onkeydown={keydown}>
  <div class="head">
    {@render head()}
    <button type="button" class="x" aria-label="Close" onclick={onclose}>×</button>
  </div>
  {@render children()}
</section>

<style>
  .panel { position: absolute; top: 3.5rem; right: var(--space-4); width: 21.25rem; max-height: calc(100% - 7.5rem); overflow: auto; box-sizing: border-box; z-index: 4;
    color: var(--map-fg); background: var(--map-surface); border: 1px solid var(--map-line); border-radius: var(--radius-xl); padding: var(--space-3) var(--space-3-5);
    font-size: var(--text-md); box-shadow: 0 6px 20px var(--map-shadow); overflow-wrap: anywhere; }
  .panel:focus { outline: none; }
  .head { display: flex; align-items: start; gap: var(--space-2); }
  .head > :global(.field) { flex: 1; margin-top: 0; }
  .x { font: inherit; border: 0; background: none; cursor: pointer; font-size: var(--text-3xl); line-height: 1; color: var(--map-muted); }
  .panel :global(button:focus-visible) { outline: 2px solid var(--map-trace); outline-offset: 1px; }
  .panel :global(.lab) { font-size: var(--text-xs); text-transform: uppercase; letter-spacing: 0.06em; color: var(--map-muted); }
  .panel :global(.group) { display: flex; flex-direction: column; gap: var(--space-0-5); margin: var(--space-2) 0; }
  .panel :global(h3) { margin: var(--space-3) 0 var(--space-1); font-size: var(--text-xs); text-transform: uppercase; letter-spacing: 0.06em; color: var(--map-muted); }
  .panel :global(.chips) { display: flex; flex-wrap: wrap; gap: var(--space-1); }
  .panel :global(.chips button) { font: inherit; font-size: var(--text-sm); cursor: pointer; padding: var(--space-0-5) var(--space-2); border-radius: var(--radius-pill);
    border: 1px solid var(--map-line); background: none; color: var(--map-fg-2); }
  .panel :global(.chips button[aria-pressed="true"]) { background: var(--map-tint-gold); border-color: var(--map-trace); color: var(--map-fg); }
  .panel :global(.chips button:disabled) { cursor: default; color: var(--map-faint); }
  @media (max-width: 47.5rem) { .panel { left: var(--space-4); right: var(--space-4); width: auto; top: auto; bottom: 3.5rem; max-height: 50%; } }
</style>
