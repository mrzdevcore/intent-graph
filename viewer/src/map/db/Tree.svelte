<script lang="ts">
  import type { View } from "../types";
  import { groupOf, treeOf, type TreeGroup } from "./focus";

  let { view, store, focus = null, onpick }: { view: View; store: string; focus?: string | null; onpick: (id: string) => void } = $props();

  let query = $state("");
  let open = $state(new Set<string>());
  $effect(() => {
    const g = focus ? groupOf(view, focus) : undefined;
    open = new Set(g ? [g, store] : [store]);
  });
  const tree = $derived(treeOf(view, store, query));
  const searching = $derived(!!query.trim());
  const toggle = (id: string) => {
    const s = new Set(open);
    if (!s.delete(id)) s.add(id);
    open = s;
  };
  function keydown(e: KeyboardEvent) {
    if (e.key === "Escape" && query) {
      e.preventDefault();
      e.stopPropagation();
      query = "";
    }
  }
</script>

{#snippet group(g: TreeGroup)}
  {@const shown = searching || open.has(g.id)}
  <li class="group">
    <button type="button" class="domain" aria-expanded={shown} onclick={() => toggle(g.id)}>
      <span class="caret" aria-hidden="true">{shown ? "▾" : "▸"}</span>
      <span class="title">{g.title}</span><span class="count">{g.count}</span>
    </button>
    {#if shown}
      <ul>
        {#each g.tables as t (t.id)}
          <li>
            <button type="button" class="table" class:on={t.id === focus} class:view={t.kind === "view"} aria-current={t.id === focus ? "true" : undefined} data-table={t.id} onclick={() => onpick(t.id)}>
              <span class="name" title={t.name}>{t.name}</span>
              {#each t.matched as c (c)}<span class="matched">· {c}</span>{/each}
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </li>
{/snippet}

<nav class="tree" aria-label="Tables">
  <input type="search" placeholder="Find a table or a column" aria-label="Find a table or a column" bind:value={query} onkeydown={keydown} />
  <p class="sum">{tree.title} · {tree.tables} {tree.tables === 1 ? "table" : "tables"} · {tree.views} {tree.views === 1 ? "view" : "views"}</p>
  <ul class="groups">{#each tree.groups as g (g.id)}{@render group(g)}{/each}</ul>
  {#if tree.aside.length}
    <p class="aside">Set aside: {tree.aside.map((g) => `${g.title} ${g.count}`).join(", ")}</p>
    <ul class="groups">{#each tree.aside as g (g.id)}{@render group(g)}{/each}</ul>
  {/if}
  {#if searching && !tree.groups.length && !tree.aside.length}<p class="none">No table or column matches “{query.trim()}”.</p>{/if}
</nav>

<style>
  .tree { display: flex; flex-direction: column; gap: var(--space-2); min-height: 0; overflow: auto; padding: var(--space-3); box-sizing: border-box;
    background: var(--map-surface); border-right: 1px solid var(--map-line); color: var(--map-fg); font-size: var(--text-md); }
  input { font: inherit; padding: var(--space-1-5) var(--space-2-5); border: 1px solid var(--map-line); border-radius: var(--radius-md); background: var(--map-ground); color: var(--map-fg); }
  input:focus-visible { outline: 2px solid var(--map-trace); outline-offset: 1px; }
  .sum, .aside { margin: 0; font-size: var(--text-sm); color: var(--map-muted); }
  .aside { margin-top: var(--space-2); padding-top: var(--space-2); border-top: 1px solid var(--map-line-soft); }
  ul { list-style: none; margin: 0; padding: 0; }
  .group > ul { padding-left: var(--space-4); }
  button { display: flex; align-items: baseline; gap: var(--space-1-5); width: 100%; font: inherit; color: inherit; text-align: left; background: none; border: 0;
    border-radius: var(--radius-md); padding: var(--space-1) var(--space-1-5); cursor: pointer; }
  button:hover { background: var(--map-sunken); }
  button:focus-visible { outline: 2px solid var(--map-trace); outline-offset: -2px; }
  .domain .title { flex: 1; font-weight: 600; }
  .caret { width: 0.75rem; color: var(--map-muted); }
  .count { font-size: var(--text-sm); color: var(--map-muted); font-variant-numeric: tabular-nums; }
  .table { flex-wrap: wrap; font-family: var(--font-mono); font-size: var(--text-sm); }
  .table .name { max-width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .table.view .name { font-style: italic; }
  .table.on { background: var(--map-trace); color: var(--map-surface); }
  .matched { flex-basis: 100%; padding-left: var(--space-2); color: var(--map-muted); }
  .table.on .matched { color: inherit; opacity: 0.8; }
  .none { margin: 0; color: var(--map-muted); font-size: var(--text-sm); }
</style>
