<!-- The proposals bar (top centre) and its list: one row per edit Claude or the AI reading made that nobody
     kept. Hovering a row lights its boxes and arrows on the map. Without `edit` (ig view) it only shows. -->
<script lang="ts">
  import { untrack } from "svelte";
  import type { EditApi } from "../../api";
  import type { View } from "../types";
  import type { Trace } from "../search";
  import { tag } from "../looks";
  import { barText, lit } from "./proposals";

  let { view, edit, run, onlight }: {
    view: View; edit?: EditApi; run: (p: Promise<View>) => void; onlight: (t: Trace | null) => void;
  } = $props();
  const list = $derived(view.proposals ?? []);
  let open = $state(false);
  // the row under the pointer or focus; it lights while the list is open and still holds it
  let hover = $state<number | null>(null);
  $effect(() => {
    if (!list.length) open = false;
    const p = open ? list.find((x) => x.n === hover) : undefined;
    const t = p ? lit(view, p.ids) : null;
    untrack(() => onlight(t));
  });
</script>

{#if list.length}
  <div class="bar" role="group" aria-label="Proposed edits">
    <span><b>{barText(list)}</b></span>
    <button type="button" aria-expanded={open} onclick={() => (open = !open)}>Review</button>
    {#if edit}
      <button type="button" onclick={() => run(edit.dropAll())}>Drop all</button>
      <button type="button" class="go" onclick={() => run(edit.keep())}>Keep all</button>
    {/if}
  </div>
  {#if open}
    <section class="list" data-popover aria-label="Edits not kept yet">
      <b>Edits not kept yet</b>
      {#each list as p (p.n)}
        <div class="item" role="group" aria-label={p.summary} onpointerenter={() => (hover = p.n)} onpointerleave={() => (hover = null)}
          onfocusin={() => (hover = p.n)} onfocusout={() => (hover = null)}>
          <span class="text"><span class="tag">{tag(p.by)}</span> {p.summary}</span>
          {#if edit}
            <button type="button" onclick={() => run(edit.drop(p.n))}>Drop</button>
            <button type="button" class="go" onclick={() => run(edit.keep(p.n))}>Keep</button>
          {/if}
        </div>
      {/each}
      <span class="hint">Hover an edit to light it on the map.{edit ? " Dropping undoes it; it refuses if you changed the same thing since." : ""}</span>
    </section>
  {/if}
{/if}

<style>
  .bar { position: absolute; left: 50%; top: var(--space-3); transform: translateX(-50%); display: flex; align-items: center; gap: var(--space-2-5);
    padding: var(--space-1-5) var(--space-2) var(--space-1-5) var(--space-3-5); border-radius: var(--radius-xl); background: var(--map-surface); border: 1px solid var(--map-trace); color: var(--map-fg); font-size: var(--text-md); }
  .list { position: absolute; right: var(--space-4); top: 4.375rem; width: 22.5rem; max-height: calc(100% - 8rem); overflow: auto; display: flex; flex-direction: column; gap: var(--space-2);
    background: var(--map-surface); border: 1px solid var(--map-line); border-radius: var(--radius-xl); padding: var(--space-3) var(--space-3-5); font-size: var(--text-md); color: var(--map-fg); box-shadow: 0 6px 20px var(--map-shadow); }
  .item { display: flex; align-items: center; gap: var(--space-1-5); }
  .text { flex: 1; overflow-wrap: anywhere; }
  .tag { font-size: var(--text-xs); padding: var(--space-0-5) 0.4375rem; border-radius: var(--radius-pill); background: var(--map-sunken); color: var(--map-trace); font-weight: 600; }
  .hint { font-size: var(--text-sm); color: var(--map-muted); }
  button { font: inherit; font-size: var(--text-md); border: 1px solid var(--map-line); background: var(--map-surface); color: var(--map-fg); border-radius: var(--radius-lg); padding: 0.25rem var(--space-2-5); min-height: 2rem; cursor: pointer; }
  button.go { border-color: var(--map-trace); background: var(--map-trace); color: var(--map-ground); font-weight: 600; }
  button:focus-visible { outline: 2px solid var(--map-trace); outline-offset: 2px; }
</style>
