<script lang="ts">
  import { tick } from "svelte";
  import type { KindView } from "../types";
  import { iconMask } from "./add";
  import KindPicker from "./KindPicker.svelte";

  let { x, y, count, kind, current, code = false, apart = false, asking = null, picking, kinds, context,
    onkinds, onpick, ondetails, onarrow, oncode, onmore, ongroup, onalign, onarrange, ondelete, onconfirm, onkeep }: {
    x: number; y: number; count: number; kind: KindView | null; current: string | null; code?: boolean; apart?: boolean; asking?: string | null;
    picking: boolean; kinds: KindView[]; context: { parentKinds: string[]; inRun: boolean };
    onkinds: (open: boolean) => void; onpick: (name: string) => void; ondetails: () => void; onarrow: () => void; oncode: () => void; onmore: (e: MouseEvent) => void;
    ongroup: () => void; onalign: (dir: "row" | "column") => void; onarrange: () => void; ondelete: () => void; onconfirm: () => void; onkeep: () => void;
  } = $props();
  const label = $derived(kind ? kind.title : current && current !== "box" ? current : "Set a kind");
  let bar: HTMLDivElement;
  $effect(() => {
    if (asking) void tick().then(() => bar?.querySelector<HTMLElement>("[data-confirm]")?.focus({ preventScroll: true }));
  });
  function closeKinds() {
    onkinds(false);
    bar?.querySelector<HTMLElement>("[data-kind-button]")?.focus({ preventScroll: true });
  }
</script>

<div class="bar" data-bar role="toolbar" aria-label={count > 1 ? `${count} boxes` : "Box"} bind:this={bar} style:left="{x}px" style:top="{y}px">
  {#if asking}
    <span class="ask" role="alert">{asking}</span>
    <button type="button" class="bad" data-confirm onclick={onconfirm}>Delete</button>
    <button type="button" onclick={onkeep}>Keep</button>
  {:else}
    {#if count > 1}<span class="count">{count} boxes</span>{/if}
    <span class="kind-at">
      <button type="button" class="kind" data-kind-button aria-haspopup="menu" aria-expanded={picking} class:none={!kind && count === 1 && current === "box"} onclick={() => onkinds(!picking)}>
        {#if kind}<i style:mask-image={iconMask(kind.icon)} aria-hidden="true"></i>{/if}<span>{count > 1 && !kind ? "Kind" : label}</span><b aria-hidden="true">▾</b>
      </button>
      {#if picking}<KindPicker {kinds} current={current} {context} {onpick} onclose={closeKinds} />{/if}
    </span>
    {#if count === 1}
      <button type="button" onclick={ondetails} title="Edit details — E">Details<kbd>E</kbd></button>
      <button type="button" onclick={onarrow} title="Arrow from here — A">Arrow<kbd>A</kbd></button>
      {#if code}<button type="button" onclick={oncode} title="Open the code — O">Code<kbd>O</kbd></button>{/if}
      <button type="button" aria-label="More" aria-haspopup="menu" onclick={onmore}>⋯</button>
    {:else}
      <button type="button" disabled={apart} title={apart ? "These boxes are in different places" : "Put them in a group"} onclick={ongroup}>Group</button>
      <button type="button" onclick={() => onalign("row")} title="Line them up in a row">Row</button>
      <button type="button" onclick={() => onalign("column")} title="Line them up in a column">Column</button>
      <button type="button" onclick={onarrange}>Arrange these</button>
      <button type="button" class="bad" onclick={ondelete} title="Delete — ⌫">Delete</button>
    {/if}
  {/if}
</div>

<style>
  .bar { position: absolute; z-index: 5; translate: 0 calc(-100% - var(--space-2)); display: flex; align-items: center; gap: var(--space-0-5); padding: var(--space-1);
    background: var(--map-surface); border: 1px solid var(--map-line); border-radius: var(--radius-xl); box-shadow: 0 0.375rem 1.25rem var(--map-shadow);
    font-size: var(--text-md); color: var(--map-fg); white-space: nowrap; }
  button { display: inline-flex; align-items: center; gap: var(--space-1); font: inherit; color: var(--map-fg-2); background: none; border: 1px solid transparent;
    border-radius: var(--radius-lg); padding: var(--space-1) var(--space-2); min-height: 1.75rem; cursor: pointer; }
  button:hover:not(:disabled) { background: var(--map-sunken); }
  button:disabled { color: var(--map-faint); cursor: default; }
  button:focus-visible { outline: 2px solid var(--map-trace); outline-offset: 1px; }
  .kind-at { position: relative; }
  .kind { color: var(--map-fg); border-color: var(--map-line); }
  .kind.none { border-style: dashed; color: var(--map-muted); }
  .kind b { font-weight: 400; color: var(--map-faint); }
  .count { padding: 0 var(--space-2); font-weight: 600; }
  .ask { padding: 0 var(--space-2); }
  .bad { color: var(--map-bad-text); }
  i { width: 0.875rem; height: 0.875rem; background: currentColor; mask-repeat: no-repeat; mask-position: center; mask-size: contain; }
  kbd { font: inherit; font-size: var(--text-2xs); color: var(--map-faint); }
</style>
