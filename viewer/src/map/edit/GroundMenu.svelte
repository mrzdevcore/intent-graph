<script lang="ts">
  import { onMount } from "svelte";
  import { menuKeys } from "./menu";

  let { x, y, title, arrange, onnew, onarrange, onclose }: {
    x: number; y: number; title: string; arrange: boolean; onnew: () => void; onarrange: () => void; onclose: () => void;
  } = $props();
  let menu: HTMLDivElement;
  const items = () => [...menu.querySelectorAll<HTMLElement>('[role="menuitem"]')];
  onMount(() => items().find((b) => !(b as HTMLButtonElement).disabled)?.focus());
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div role="menu" data-popover aria-label="{title} menu" tabindex="-1" bind:this={menu} style:left="{x}px" style:top="{y}px" onkeydown={(e) => menuKeys(e, items(), onclose)}>
  <button type="button" role="menuitem" disabled title="Copy a box first"><span>Paste</span><kbd>⌘V</kbd></button>
  <button type="button" role="menuitem" onclick={onnew}><span>New box here</span></button>
  {#if arrange}<button type="button" role="menuitem" onclick={onarrange}><span>Arrange this group</span><kbd>⌥⇧F</kbd></button>{/if}
</div>

<style>
  div { position: absolute; z-index: 6; width: 13.75rem; box-sizing: border-box; padding: var(--space-1); background: var(--map-surface); border: 1px solid var(--map-line);
    border-radius: var(--radius-xl); box-shadow: 0 0.5rem 1.5rem var(--map-shadow); font-size: var(--text-md); color: var(--map-fg); }
  button { display: flex; justify-content: space-between; width: 100%; font: inherit; color: inherit; background: none; border: 0; border-radius: var(--radius-md); padding: var(--space-1-5) var(--space-2-5); cursor: pointer; text-align: left; }
  button:disabled { color: var(--map-faint); cursor: default; }
  button:not(:disabled):hover, button:focus-visible { background: var(--map-sunken); }
  button:focus-visible { outline: 2px solid var(--map-trace); outline-offset: -2px; }
  kbd { font: inherit; font-size: var(--text-sm); color: var(--map-faint); }
</style>
