<script lang="ts">
  import { onMount } from "svelte";
  import type { KindView } from "../types";
  import { iconMask } from "./add";
  import { rankKinds } from "./kinds";

  let { kinds, current, context, onpick, onclose }: {
    kinds: KindView[]; current: string | null; context: { parentKinds: string[]; inRun: boolean };
    onpick: (name: string) => void; onclose: () => void;
  } = $props();
  let q = $state("");
  const ranked = $derived(rankKinds(q, kinds, context));
  let field: HTMLInputElement, list: HTMLDivElement;
  onMount(() => field.focus({ preventScroll: true }));
  const rows = () => [...list.querySelectorAll<HTMLButtonElement>('[role="menuitemradio"]')];
  function keydown(e: KeyboardEvent) {
    const inField = e.target === field;
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      onclose();
    } else if (e.key === "Enter") {
      e.preventDefault();
      e.stopPropagation();
      if (inField) {
        if (ranked.best) onpick(ranked.best.name);
      }
      else (e.target as Element).closest<HTMLButtonElement>('[role="menuitemradio"]')?.click();
    } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const r = rows(), i = inField ? -1 : r.indexOf(document.activeElement as HTMLButtonElement), down = e.key === "ArrowDown";
      if (!down && i <= 0) field.focus();
      else r[down ? Math.min(r.length - 1, i + 1) : i - 1]?.focus();
    }
  }
</script>

{#snippet row(k: KindView)}
  <button type="button" role="menuitemradio" data-kind={k.name} aria-checked={k.name === current} onclick={() => onpick(k.name)}>
    <span class="ic" aria-hidden="true"><i style:mask-image={iconMask(k.icon)}></i></span>
    <span class="n">{k.title}</span>
    <span class="m">{k.means}</span>
  </button>
{/snippet}

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="picker" data-popover data-kinds onkeydown={keydown}>
  <input bind:this={field} bind:value={q} aria-label="Find a kind" placeholder="Find a kind" autocomplete="off" spellcheck="false" />
  <div role="menu" aria-label="Kinds" bind:this={list}>
    {#if ranked.best}
      <span class="h">Best match</span>
      {@render row(ranked.best)}
    {/if}
    {#each ranked.groups as g (g.title)}
      <span class="h">{g.title}</span>
      {#each g.kinds as k (k.name)}{@render row(k)}{/each}
    {/each}
    <span class="h"></span>
    <button type="button" role="menuitemradio" data-kind="box" aria-checked={current === "box"} onclick={() => onpick("box")}>
      <span class="ic none" aria-hidden="true"></span><span class="n">No kind</span><span class="m">Back to a plain box</span>
    </button>
  </div>
</div>

<style>
  .picker { position: absolute; z-index: 6; left: 0; top: calc(100% + var(--space-1)); width: 22rem; max-width: calc(100vw - 2rem); box-sizing: border-box; display: flex; flex-direction: column;
    gap: var(--space-1); padding: var(--space-1-5); background: var(--map-surface); border: 1px solid var(--map-line); border-radius: var(--radius-xl);
    box-shadow: 0 0.5rem 1.5rem var(--map-shadow); color: var(--map-fg); font-size: var(--text-md); }
  input { font: inherit; color: inherit; background: var(--map-sunken); border: 1px solid var(--map-line); border-radius: var(--radius-md); padding: var(--space-1-5) var(--space-2); }
  input:focus-visible { outline: 2px solid var(--map-trace); outline-offset: 1px; }
  [role="menu"] { display: flex; flex-direction: column; max-height: 22rem; overflow-y: auto; }
  .h { font-size: var(--text-xs); letter-spacing: 0.05em; text-transform: uppercase; color: var(--map-muted); padding: var(--space-2) var(--space-2) var(--space-0-5); }
  .h:empty { padding: 0; margin: var(--space-1) 0; border-top: 1px solid var(--map-line-soft); }
  button { display: flex; align-items: center; gap: var(--space-2); font: inherit; color: inherit; text-align: left; background: none; border: 0; border-radius: var(--radius-md);
    padding: var(--space-1) var(--space-2); cursor: pointer; }
  button:hover, button:focus-visible { background: var(--map-sunken); }
  button:focus-visible { outline: 2px solid var(--map-trace); outline-offset: -2px; }
  [aria-checked="true"] { background: var(--map-tint-gold); }
  .ic { flex: none; width: 1.375rem; height: 1.375rem; border-radius: var(--radius-sm); background: var(--map-sunken); display: inline-flex; align-items: center; justify-content: center; color: var(--map-fg-2); }
  .ic.none { border: 1px dashed var(--map-edge); background: none; box-sizing: border-box; }
  .ic i { width: 0.875rem; height: 0.875rem; background: currentColor; mask-repeat: no-repeat; mask-position: center; mask-size: contain; }
  .n { flex: none; font-weight: 600; }
  .m { flex: 1; min-width: 0; text-align: right; font-size: var(--text-sm); color: var(--map-muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
</style>
