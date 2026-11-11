<script lang="ts">
  import type { ArrowKind } from "../types";
  import ArrowChip from "./ArrowChip.svelte";
  import type { Editing } from "./arrows";

  let { x, y, how, title, kind, kinds, picking, note = false, context = false, parts = [],
    onkinds, onpick, onnote, oncontext, onflip, ondetails, ondelete, onkeep, onpart }: {
    x: number; y: number; how: Editing; title: string; kind: ArrowKind; kinds: ArrowKind[]; picking: boolean; note?: boolean; context?: boolean;
    parts?: { title: string; kind: ArrowKind }[];
    onkinds: (open: boolean) => void; onpick: (k: ArrowKind) => void; onnote: () => void; oncontext: () => void; onflip: () => void;
    ondetails: () => void; ondelete: () => void; onkeep: () => void; onpart: (i: number) => void;
  } = $props();
  const can = $derived(how === "edit" || how === "drift");
</script>

<div class="bar" data-bar role="toolbar" aria-label="Arrow {title}" style:left="{x}px" style:top="{y}px">
  {#if how === "bundle"}
    <span class="title">{parts.length === 1 ? "Drawn here for" : `${parts.length} arrows`}</span>
    {#each parts as p, i (i)}
      <button type="button" data-part={i} onclick={() => onpart(i)}><span class="k">{p.kind}</span>{p.title}</button>
    {/each}
  {:else}
    {#if how === "drift"}<button type="button" class="keep" onclick={onkeep} title="Write this arrow into ig/">Keep it in ig/</button>{/if}
    {#if can}
      <ArrowChip {kind} {kinds} open={picking} ontoggle={onkinds} {onpick} />
      <button type="button" class:set={note} onclick={onnote} title="A short line on the arrow">Note</button>
      <button type="button" onclick={oncontext} title="Context for whoever builds it" aria-label={context ? "Context (set)" : "Context"}>Context{#if context}<i class="dot" data-dot aria-hidden="true"></i>{/if}</button>
      <button type="button" onclick={onflip} title="Turn it the other way">Flip</button>
    {:else}
      <span class="title">{title}</span>
    {/if}
    <button type="button" onclick={ondetails} title="Edit details">Details</button>
    {#if how === "edit"}<button type="button" class="bad" onclick={ondelete} title="Delete — ⌫">Delete</button>{/if}
  {/if}
</div>

<style>
  .bar { position: absolute; z-index: 5; translate: -50% calc(-100% - var(--space-3)); display: flex; align-items: center; gap: var(--space-0-5); padding: var(--space-1);
    background: var(--map-surface); border: 1px solid var(--map-line); border-radius: var(--radius-xl); box-shadow: 0 0.375rem 1.25rem var(--map-shadow);
    font-size: var(--text-md); color: var(--map-fg); white-space: nowrap; }
  button { display: inline-flex; align-items: center; gap: var(--space-1); font: inherit; color: var(--map-fg-2); background: none; border: 1px solid transparent;
    border-radius: var(--radius-lg); padding: var(--space-1) var(--space-2); min-height: 1.75rem; cursor: pointer; }
  button:hover { background: var(--map-sunken); }
  button:focus-visible { outline: 2px solid var(--map-trace); outline-offset: 1px; }
  .title { padding: 0 var(--space-2); color: var(--map-muted); }
  .keep { color: var(--map-fg); border-color: var(--map-trace); }
  .set { color: var(--map-fg); }
  .bad { color: var(--map-bad-text); }
  .k { font-size: var(--text-2xs); color: var(--map-faint); }
  .dot { width: 0.375rem; height: 0.375rem; border-radius: 50%; background: var(--map-trace); }
</style>
