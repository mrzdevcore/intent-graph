<!-- Drawing an arrow (EditArrow artboard): the rubber band while dragging, then the kind chips and optional
     label. Enter draws, Esc cancels. Text only. -->
<script lang="ts">
  import { onMount } from "svelte";
  import type { ArrowKind } from "../types";
  import { ARROW_KINDS, last } from "./arrows";

  let { line, pick, onpick, onclose }: {
    line?: { x1: number; y1: number; x2: number; y2: number };
    pick?: { from: string; to: string; x: number; y: number };
    onpick: (kind: ArrowKind, label: string | null) => void; onclose: () => void;
  } = $props();
  let kind = $state<ArrowKind>(last.kind);
  let label = $state("");
  let field: HTMLInputElement | undefined = $state();
  onMount(() => field?.focus());
  function submit(e: Event) {
    e.preventDefault();
    last.kind = kind;
    onpick(kind, label.trim() || null);
  }
  function keydown(e: KeyboardEvent) {
    if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); onclose(); }
  }
</script>

{#if line}
  <svg aria-hidden="true"><line x1={line.x1} y1={line.y1} x2={line.x2} y2={line.y2} /><circle cx={line.x2} cy={line.y2} r="4" /></svg>
{/if}
{#if pick}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <form data-popover aria-label="Arrow {pick.from} to {pick.to}" style:left="{pick.x}px" style:top="{pick.y}px" onsubmit={submit} onkeydown={keydown}>
    <b>{pick.from} → {pick.to}</b>
    <div class="chips" role="radiogroup" aria-label="Kind">
      {#each ARROW_KINDS as k (k)}
        <button type="button" role="radio" aria-checked={kind === k} class:on={kind === k} onclick={() => (kind = k)}>{k}</button>
      {/each}
    </div>
    <label>Label (optional)
      <input bind:this={field} bind:value={label} autocomplete="off" spellcheck="false" />
    </label>
    <div class="row">
      <button type="button" onclick={onclose}>Cancel <kbd>Esc</kbd></button>
      <button type="submit" class="go">Draw <kbd>↵</kbd></button>
    </div>
  </form>
{/if}

<style>
  svg { position: absolute; inset: 0; width: 100%; height: 100%; pointer-events: none; z-index: 4; overflow: visible; }
  line { stroke: var(--map-blue); stroke-width: 2; stroke-dasharray: 6 4; }
  circle { fill: var(--map-blue); }
  form { position: absolute; z-index: 6; width: 18.75rem; max-width: calc(100% - 1rem); box-sizing: border-box; display: flex; flex-direction: column; gap: var(--space-2-5);
    padding: var(--space-3); background: var(--map-surface); border: 1px solid var(--map-line); border-radius: var(--radius-xl); box-shadow: 0 0.5rem 1.5rem var(--map-shadow); color: var(--map-fg); font-size: var(--text-md); }
  .chips { display: flex; flex-wrap: wrap; gap: var(--space-1-5); }
  .chips button { font: inherit; font-size: var(--text-sm); padding: var(--space-0-5) var(--space-2-5); border-radius: var(--radius-pill); border: 1px solid var(--map-line); background: transparent; color: var(--map-fg); cursor: pointer; }
  .chips .on { border-color: var(--map-blue); background: var(--map-tint-blue); }
  label { display: flex; flex-direction: column; gap: var(--space-1); font-size: var(--text-sm); color: var(--map-muted); }
  input { font: inherit; font-size: var(--text-md); color: var(--map-fg); background: var(--map-sunken); border: 1px solid var(--map-line); border-radius: var(--radius-md); padding: var(--space-1-5) var(--space-2); }
  .row { display: flex; justify-content: flex-end; gap: var(--space-1-5); }
  .row button { font: inherit; font-size: var(--text-md); border: 1px solid var(--map-line); background: var(--map-surface); color: var(--map-fg); border-radius: var(--radius-lg); padding: var(--space-1) var(--space-2-5); min-height: 2rem; cursor: pointer; }
  .go { border-color: var(--map-gold); background: var(--map-tint-gold); }
  kbd { font: inherit; font-size: var(--text-sm); color: var(--map-faint); }
  input:focus-visible, button:focus-visible { outline: 2px solid var(--map-trace); outline-offset: 2px; }
</style>
