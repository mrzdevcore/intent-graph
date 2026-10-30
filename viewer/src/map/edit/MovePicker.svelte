<!-- Move to…: a searchable list of the boxes that can hold the chosen one. Enter picks the first match, Esc closes. -->
<script lang="ts">
  import { onMount } from "svelte";
  let { title, options, x, y, onpick, onclose }: {
    title: string; options: { id: string; title: string; kind: string }[]; x: number; y: number; onpick: (id: string) => void; onclose: () => void;
  } = $props();
  let q = $state("");
  const shown = $derived(options.filter((o) => (o.title + " " + o.id).toLowerCase().includes(q.trim().toLowerCase())));
  let field: HTMLInputElement, box: HTMLDivElement;
  onMount(() => field.focus());
  function keydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      onclose();
    } else if (e.key === "Enter" && e.target === field) {
      e.preventDefault();
      if (shown[0]) onpick(shown[0].id);
    } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const b = [field, ...box.querySelectorAll<HTMLElement>("li button")], i = b.indexOf(document.activeElement as HTMLElement);
      b[Math.min(b.length - 1, Math.max(0, i + (e.key === "ArrowDown" ? 1 : -1)))].focus();
    }
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div role="dialog" tabindex="-1" aria-label="Move {title} to" bind:this={box} style:left="{x}px" style:top="{y}px" onkeydown={keydown}>
  <b>Move {title} to</b>
  <input bind:this={field} bind:value={q} placeholder="Search boxes" aria-label="Search boxes" autocomplete="off" spellcheck="false" />
  {#if shown.length}
    <ul>{#each shown as o (o.id)}<li><button type="button" onclick={() => onpick(o.id)}><span>{o.title}</span><small>{o.kind}</small></button></li>{/each}</ul>
  {:else}<small>No box can hold it.</small>{/if}
</div>

<style>
  div { position: absolute; z-index: 6; width: 17.5rem; max-width: calc(100% - 1rem); box-sizing: border-box; display: flex; flex-direction: column; gap: var(--space-2);
    padding: var(--space-3); background: var(--map-surface); border: 1px solid var(--map-line); border-radius: var(--radius-xl); box-shadow: 0 0.5rem 1.5rem var(--map-shadow); color: var(--map-fg); font-size: var(--text-md); }
  input { font: inherit; color: var(--map-fg); background: var(--map-sunken); border: 1px solid var(--map-line); border-radius: var(--radius-md); padding: var(--space-1-5) var(--space-2); }
  ul { list-style: none; margin: 0; padding: 0; max-height: 12rem; overflow: auto; }
  li button { display: flex; justify-content: space-between; width: 100%; font: inherit; color: inherit; background: none; border: 0; border-radius: var(--radius-md); padding: var(--space-1) var(--space-2); cursor: pointer; text-align: left; }
  li button:hover { background: var(--map-sunken); }
  small { font-size: var(--text-sm); color: var(--map-faint); }
  input:focus-visible, li button:focus-visible { outline: 2px solid var(--map-trace); outline-offset: 2px; }
</style>
