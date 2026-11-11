<script lang="ts">
  import { onMount } from "svelte";
  import type { Editing } from "./arrows";
  import { menuKeys } from "./menu";

  type A = "keep" | "kind" | "label" | "note" | "context" | "flip" | "details" | "delete";
  let { x, y, title, how, parts = [], onpick, onpart, onclose }: {
    x: number; y: number; title: string; how: Editing; parts?: string[]; onpick: (a: A) => void; onpart: (i: number) => void; onclose: () => void;
  } = $props();
  const can = $derived(how === "edit" || how === "drift");
  const items: [A, string, string][] = $derived([
    ...(how === "drift" ? [["keep", "Keep it in ig/", ""] as [A, string, string]] : []),
    ...(can ? ([["kind", "Kind", ""], ["label", "Label", ""], ["note", "Note", ""], ["context", "Context", ""], ["flip", "Flip", ""]] as [A, string, string][]) : []),
    ...(how === "bundle" ? [] : [["details", "Edit details", ""] as [A, string, string]]),
    ...(how === "edit" ? [["delete", "Delete", "⌫"] as [A, string, string]] : []),
  ]);
  const KEYS: Record<string, A> = { delete: "delete", backspace: "delete" };
  let menu: HTMLDivElement;
  const all = () => [...menu.querySelectorAll<HTMLElement>("button")];
  onMount(() => all()[0]?.focus());
  function keydown(e: KeyboardEvent) {
    const a = KEYS[e.key.toLowerCase()];
    if (a && items.some(([x]) => x === a)) {
      e.preventDefault();
      onpick(a);
    } else menuKeys(e, all(), onclose);
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div role="menu" data-popover aria-label="{title} menu" tabindex="-1" bind:this={menu} style:left="{x}px" style:top="{y}px" onkeydown={keydown}>
  {#each parts as p, i (i)}
    <button type="button" role="menuitem" data-part={i} onclick={() => onpart(i)}><span>{p}</span></button>
  {/each}
  {#each items as [a, label, hint] (a)}
    <button type="button" role="menuitem" data-item={a} class:bad={a === "delete"} onclick={() => onpick(a)}><span>{label}</span>{#if hint}<kbd>{hint}</kbd>{/if}</button>
  {/each}
</div>

<style>
  [role="menu"] { position: absolute; z-index: 6; width: 13.75rem; box-sizing: border-box; padding: var(--space-1); background: var(--map-surface); border: 1px solid var(--map-line);
    border-radius: var(--radius-xl); box-shadow: 0 0.5rem 1.5rem var(--map-shadow); font-size: var(--text-md); color: var(--map-fg); }
  button { display: flex; align-items: center; gap: var(--space-2); width: 100%; font: inherit; color: inherit; background: none; border: 0; border-radius: var(--radius-md); padding: var(--space-1-5) var(--space-2-5); cursor: pointer; text-align: left; }
  button > span { flex: 1; }
  button:hover, button:focus-visible { background: var(--map-sunken); }
  button:focus-visible { outline: 2px solid var(--map-trace); outline-offset: -2px; }
  .bad { color: var(--map-bad-text); }
  kbd { font: inherit; font-size: var(--text-sm); color: var(--map-faint); }
</style>
