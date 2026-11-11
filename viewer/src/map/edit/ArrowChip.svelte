<script lang="ts">
  import type { ArrowKind } from "../types";

  let { kind, kinds, x, y, open, ontoggle, onpick }: {
    kind: ArrowKind; kinds: ArrowKind[]; x: number; y: number; open: boolean; ontoggle: (open: boolean) => void; onpick: (k: ArrowKind) => void;
  } = $props();
  let list = $state<HTMLDivElement>();
  $effect(() => {
    if (open) list?.querySelector<HTMLButtonElement>('[aria-checked="true"]')?.focus({ preventScroll: true });
  });
  function keydown(e: KeyboardEvent) {
    if (!open) return;
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      ontoggle(false);
    } else if (e.key === "Enter") {
      e.preventDefault();
      e.stopPropagation();
      const b = (e.target as Element).closest<HTMLButtonElement>('[role="menuitemradio"]');
      if (b) b.click(); else ontoggle(false);
    } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const b = [...list!.querySelectorAll<HTMLButtonElement>('[role="menuitemradio"]')], i = b.indexOf(document.activeElement as HTMLButtonElement);
      b[(i + (e.key === "ArrowDown" ? 1 : b.length - 1)) % b.length].focus();
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="chip" data-popover data-chip style:left="{x}px" style:top="{y}px" onkeydown={keydown}>
  <button type="button" class="kind" aria-haspopup="menu" aria-expanded={open} aria-label="Arrow kind: {kind}" onclick={() => ontoggle(!open)}>{kind}</button>
  {#if open}
    <div role="menu" aria-label="Arrow kinds" bind:this={list}>
      {#each kinds as k (k)}
        <button type="button" role="menuitemradio" aria-checked={k === kind} onclick={() => onpick(k)}>{k}</button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .chip { position: absolute; z-index: 5; translate: -50% -50%; font-size: var(--text-sm); color: var(--map-fg); }
  .kind { font: inherit; padding: var(--space-0-5) var(--space-2); border-radius: var(--radius-pill); border: 1px solid var(--map-trace); background: var(--map-surface);
    color: inherit; cursor: pointer; box-shadow: 0 0.125rem 0.5rem var(--map-shadow); }
  [role="menu"] { position: absolute; left: 50%; top: calc(100% + var(--space-1)); translate: -50% 0; display: flex; flex-direction: column; min-width: 8rem; padding: var(--space-1);
    background: var(--map-surface); border: 1px solid var(--map-line); border-radius: var(--radius-xl); box-shadow: 0 0.5rem 1.5rem var(--map-shadow); }
  [role="menuitemradio"] { font: inherit; text-align: left; color: inherit; background: none; border: 0; border-radius: var(--radius-md); padding: var(--space-1) var(--space-2); cursor: pointer; }
  [role="menuitemradio"]:hover, [role="menuitemradio"]:focus-visible { background: var(--map-sunken); }
  [aria-checked="true"] { background: var(--map-tint-gold); }
  button:focus-visible { outline: 2px solid var(--map-trace); outline-offset: 1px; }
</style>
