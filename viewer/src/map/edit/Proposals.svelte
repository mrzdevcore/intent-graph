<!-- The proposals bar (bottom centre, clear of the tools and search) and its list, opening upward: one row per edit Claude or the AI reading made that nobody
     kept. Hovering a row lights its boxes and arrows on the map. Without `edit` (ig view) it only shows, and says what to run. -->
<script lang="ts">
  import { untrack } from "svelte";
  import type { EditApi } from "../../api";
  import type { View } from "../types";
  import type { Trace } from "../search";
  import { tag } from "../looks";
  import { barText, lit } from "./proposals";

  let { view, edit, run, onlight }: {
    view: View; edit?: EditApi; run: (p: () => Promise<View>) => void; onlight: (t: Trace | null) => void;
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
      <button type="button" onclick={() => run(() => edit.dropAll())}>Drop all</button>
      <button type="button" class="go" onclick={() => run(() => edit.keep())}>Keep all</button>
    {:else}
      <span class="how">Already in ig/ · keep or drop in <code>ig app</code></span>
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
            <button type="button" onclick={() => run(() => edit.drop(p.n))}>Drop</button>
            <button type="button" onclick={() => run(() => edit.keep(p.n))}>Keep</button>
          {/if}
        </div>
      {/each}
      {#if edit}
        <span class="hint">Hover an edit to light it on the map. Dropping undoes it; it refuses if you changed the same thing since.</span>
      {:else}
        <span class="hint">Hover an edit to light it on the map. These edits are already written to ig/: see them with <code>git diff ig/</code>,
          and run <code>ig app</code> to keep or drop them one by one.</span>
      {/if}
    </section>
  {/if}
{/if}

<style>
  .bar { position: absolute; left: 50%; bottom: var(--space-4); transform: translateX(-50%); display: flex; align-items: center; gap: var(--space-2-5); max-width: calc(100% - 2rem);
    padding: var(--space-1-5) var(--space-2) var(--space-1-5) var(--space-3-5); border-radius: var(--radius-xl); background: var(--map-surface); border: 1px solid var(--map-trace); color: var(--map-fg); font-size: var(--text-md); box-shadow: 0 6px 20px var(--map-shadow); white-space: nowrap; }
  .list { position: absolute; left: 50%; bottom: 4.5rem; transform: translateX(-50%); width: min(28rem, calc(100% - 2rem)); max-height: calc(100% - 10rem); overflow: auto; display: flex; flex-direction: column; gap: var(--space-0-5);
    background: var(--map-surface); border: 1px solid var(--map-line); border-radius: var(--radius-xl); padding: var(--space-2-5) var(--space-2); font-size: var(--text-md); color: var(--map-fg); box-shadow: 0 6px 20px var(--map-shadow); }
  .list > b { padding: 0 var(--space-1-5) var(--space-1); }
  .item { display: flex; align-items: center; gap: var(--space-1-5); padding: var(--space-1) var(--space-1-5); border-radius: var(--radius-lg); min-height: 2rem; }
  .item:hover, .item:focus-within { background: var(--map-sunken); }
  /* a row's keep and drop show on the row in hand: one gold button on screen, Keep all */
  .item button { opacity: 0; }
  .item:hover button, .item:focus-within button { opacity: 1; }
  .text { flex: 1; overflow-wrap: anywhere; }
  .tag { font-size: var(--text-xs); padding: var(--space-0-5) 0.4375rem; border-radius: var(--radius-pill); background: var(--map-sunken); color: var(--map-trace); font-weight: 600; }
  .how { color: var(--map-muted); font-size: var(--text-sm); }
  code { font-family: var(--font-mono); font-size: var(--text-sm); color: var(--map-fg); background: var(--map-sunken); padding: 0 var(--space-1); border-radius: var(--radius-sm); }
  .hint { font-size: var(--text-sm); color: var(--map-muted); padding: var(--space-1) var(--space-1-5) 0; }
  button { font: inherit; font-size: var(--text-md); border: 1px solid var(--map-line); background: var(--map-surface); color: var(--map-fg); border-radius: var(--radius-lg); padding: 0.25rem var(--space-2-5); min-height: 2rem; cursor: pointer; }
  button.go { border-color: var(--map-trace); background: var(--map-trace); color: var(--map-ground); font-weight: 600; }
  button:focus-visible { outline: 2px solid var(--map-trace); outline-offset: 2px; }
</style>
