<!-- The box menu (right-click, or the card's ⋯): Rename, Move to…, Add box inside, Arrow from here, Arrange inside (a box with children), Delete.
     Arrow keys move between items; Esc closes (handled here: the map ignores keys while focus is in a field). -->
<script lang="ts">
  import { onMount } from "svelte";
  import type { Act } from "./menu";
  let { x, y, title, group = false, onpick, onclose }: { x: number; y: number; title: string; group?: boolean; onpick: (a: Act) => void; onclose: () => void } = $props();
  const items: [Act, string, string][] = $derived([
    ["rename", "Rename", "F2"], ["move", "Move to…", "M"], ["add", "Add box inside", "B"], ["arrow", "Arrow from here", "A"],
    ...(group ? [["arrange", "Arrange inside", ""] as [Act, string, string]] : []), ["delete", "Delete", "⌫"],
  ]);
  let menu: HTMLDivElement;
  const buttons = () => [...menu.querySelectorAll<HTMLButtonElement>('[role="menuitem"]')];
  onMount(() => buttons()[0]?.focus());
  function keydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      onclose();
    } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const b = buttons(), i = b.indexOf(document.activeElement as HTMLButtonElement);
      b[(i + (e.key === "ArrowDown" ? 1 : b.length - 1)) % b.length].focus();
    }
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div role="menu" data-popover aria-label="{title} menu" tabindex="-1" bind:this={menu} style:left="{x}px" style:top="{y}px" onkeydown={keydown}>
  {#each items as [a, label, hint] (a)}
    <button type="button" role="menuitem" class:bad={a === "delete"} onclick={() => onpick(a)}><span>{label}</span><kbd>{hint}</kbd></button>
  {/each}
</div>

<style>
  div { position: absolute; z-index: 6; width: 13.75rem; box-sizing: border-box; padding: var(--space-1); background: var(--map-surface); border: 1px solid var(--map-line);
    border-radius: var(--radius-xl); box-shadow: 0 0.5rem 1.5rem var(--map-shadow); font-size: var(--text-md); color: var(--map-fg); }
  button { display: flex; justify-content: space-between; width: 100%; font: inherit; color: inherit; background: none; border: 0; border-radius: var(--radius-md); padding: var(--space-1-5) var(--space-2-5); cursor: pointer; text-align: left; }
  button:hover, button:focus-visible { background: var(--map-sunken); }
  button:focus-visible { outline: 2px solid var(--map-trace); outline-offset: -2px; }
  .bad { color: var(--map-bad-text); }
  kbd { font: inherit; font-size: var(--text-sm); color: var(--map-faint); }
</style>
