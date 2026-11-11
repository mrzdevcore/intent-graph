<script lang="ts">
  import type { Column, View } from "../types";
  import Tree from "./Tree.svelte";
  import { endLabel, endMarks, type Notation } from "./ends";
  import { FOLDS, SIZE, focusLayout, groupOf, type Card, type Folds, type Hops, type Tip } from "./focus";

  let { view, store, table = null, ends = "crowsfoot", folds = FOLDS, onpick, onends, onclose }: {
    view: View; store: string; table?: string | null; ends?: Notation; folds?: Folds;
    onpick: (id: string) => void; onends: (n: Notation) => void; onclose: () => void;
  } = $props();

  let hops = $state<Hops>(1);
  const layout = $derived(table ? focusLayout(view, table, hops, folds) : null);
  const title = (id: string | undefined) => (id ? view.boxes.find((b) => b.id === id)?.title.value ?? id : "");
  const group = $derived(table ? groupOf(view, table) : undefined);
  const crumbs = $derived([title(store), group && group !== store ? title(group) : "", table ? view.tables?.[table]?.name ?? "" : ""].filter(Boolean));
  const u = (n: number) => `${n / 16}rem`;
  const marks = $derived(ends !== "labels");
  const labels = $derived(ends !== "crowsfoot");
  const NOTATIONS: [Notation, string][] = [["crowsfoot", "Crow's foot"], ["labels", "Labels"], ["both", "Both"]];

  function keydown(e: KeyboardEvent) {
    if (e.key !== "Escape" || e.defaultPrevented) return;
    e.preventDefault();
    e.stopImmediatePropagation();
    onclose();
  }
  const fk = (c: Column) => (c.references ? `→ ${c.references.table}${c.references.column ? `.${c.references.column}` : ""}` : undefined);
</script>

<svelte:window onkeydown={keydown} />

{#snippet tip(t: Tip)}
  {#if t.end}
    {#if marks}
      {@const m = endMarks(t.end, [t.x, t.y], t.out)}
      <path class="end" d={m.lines} />
      {#each m.rings as [cx, cy], i (i)}<circle class="ring" {cx} {cy} r="4" />{/each}
    {/if}
    {#if labels}
      <text class="label" x={t.x + t.out[0] * (marks ? 30 : 8)} y={t.y - 6} text-anchor={t.out[0] < 0 ? "end" : "start"}>{endLabel(t.end)}</text>
    {/if}
  {/if}
{/snippet}

{#snippet card(k: Card, centre: boolean)}
  {#snippet body()}
    <span class="head" style:height={u(SIZE.head)}>
      <span class="stereo">«{k.table.kind}»</span>
      <b class="name">{k.table.name}</b>
    </span>
    {#each k.rows as r, i (i)}
      {#if r.t === "col"}
        <span class="row" class:pk={!!r.col.pk} class:fk={!!r.col.references} style:height={u(SIZE.row)} title={fk(r.col)} data-col={r.col.name}>
          <span class="keys">{#if r.col.pk}<i class="b-pk">PK</i>{/if}{#if r.col.references}<i class="b-fk">FK</i>{/if}</span>
          <span class="cname">{r.col.name}</span>
          <span class="type">{r.col.type}{r.col.type && r.col.nullable ? "?" : ""}</span>
        </span>
      {:else if r.t === "fold"}
        <span class="note" style:height={u(SIZE.row)} title={r.names.join(", ")}>+{r.names.length} {r.names.join(" · ")}</span>
      {:else if r.t === "i18n"}
        <span class="note i18n" style:height={u(SIZE.row)}><i class="tag">i18n</i>{r.names.join(", ")}</span>
      {:else}
        <span class="note" style:height={u(SIZE.row)}>+{r.n} {r.n === 1 ? "column" : "columns"}</span>
      {/if}
    {/each}
  {/snippet}
  {#if centre}
    <section class="card centre" data-card={k.id} aria-label={k.table.name} style:left={u(k.x)} style:top={u(k.y)} style:width={u(k.w)} style:height={u(k.h)}>{@render body()}</section>
  {:else}
    <button type="button" class="card side" data-card={k.id} aria-label="Focus {k.table.name}" style:left={u(k.x)} style:top={u(k.y)} style:width={u(k.w)} style:height={u(k.h)}
      onclick={() => onpick(k.id)}>{@render body()}</button>
  {/if}
{/snippet}

<div class="focus" role="dialog" aria-label="Focus on a table">
  <header>
    <button type="button" class="back" onclick={onclose}>Back to the map</button>
    <ol class="crumbs" aria-label="Where">
      <li><button type="button" onclick={onclose}>Map</button></li>
      {#each crumbs as c, i (i)}<li aria-current={i === crumbs.length - 1 ? "page" : undefined}>{c}</li>{/each}
    </ol>
    <span class="spacer"></span>
    <div class="seg" role="group" aria-label="Hops">
      {#each [1, 2] as h (h)}<button type="button" aria-pressed={hops === h} onclick={() => (hops = h as Hops)}>{h} {h === 1 ? "hop" : "hops"}</button>{/each}
    </div>
    <div class="seg" role="group" aria-label="Ends">
      <span class="lead">Ends</span>
      {#each NOTATIONS as [n, label] (n)}<button type="button" aria-pressed={ends === n} onclick={() => onends(n)}>{label}</button>{/each}
    </div>
  </header>
  <div class="body">
    <Tree {view} {store} focus={table} {onpick} />
    <div class="stage">
      {#if layout}
        <div class="board" style:width={u(layout.w)} style:height={u(layout.h)}>
          <svg viewBox="0 0 {layout.w} {layout.h}" width="100%" height="100%" aria-hidden="true">
            {#each layout.lines as l (l.id)}
              <g class="line" class:via={!!l.via} data-line={l.id}>
                <path class="wire" d={l.d} />
                {@render tip(l.a)}
                {@render tip(l.b)}
                {#if l.via}<text class="via-name" x={l.mid[0]} y={l.mid[1] - 6} text-anchor="middle">{l.via}</text>{/if}
              </g>
            {/each}
          </svg>
          {#each layout.cards as k (k.id)}{@render card(k, k.id === table)}{/each}
        </div>
      {:else}
        <p class="empty">Pick a table in the list to see what it references and what references it.</p>
      {/if}
    </div>
  </div>
</div>

<style>
  .focus { position: absolute; inset: 0; z-index: 20; display: flex; flex-direction: column; background: var(--map-ground); color: var(--map-fg); font-size: var(--text-md); }
  header { flex: none; display: flex; align-items: center; gap: var(--space-3); padding: var(--space-2) var(--space-3); border-bottom: 1px solid var(--map-line); background: var(--map-surface); }
  header button { font: inherit; color: inherit; cursor: pointer; border: 1px solid var(--map-line); background: var(--map-surface); border-radius: var(--radius-md); padding: var(--space-1) var(--space-2-5); }
  header button:hover { background: var(--map-sunken); }
  header button:focus-visible { outline: 2px solid var(--map-trace); outline-offset: 1px; }
  .back { font-weight: 600; }
  .crumbs { display: flex; gap: var(--space-1-5); list-style: none; margin: 0; padding: 0; min-width: 0; color: var(--map-muted); white-space: nowrap; overflow: hidden; }
  .crumbs li + li::before { content: "›"; margin-right: var(--space-1-5); }
  .crumbs li[aria-current] { color: var(--map-fg); font-weight: 600; }
  .crumbs button { border: 0; padding: 0; background: none; color: inherit; text-decoration: underline; }
  .spacer { flex: 1; }
  .seg { display: flex; align-items: center; gap: 0; }
  .seg .lead { margin-right: var(--space-1-5); color: var(--map-muted); }
  .seg button { border-radius: 0; margin-left: -1px; }
  .seg button:first-of-type { border-radius: var(--radius-md) 0 0 var(--radius-md); }
  .seg button:last-of-type { border-radius: 0 var(--radius-md) var(--radius-md) 0; }
  .seg button[aria-pressed="true"] { background: var(--map-tint-gold); border-color: var(--map-trace); position: relative; }
  .body { flex: 1; min-height: 0; display: grid; grid-template-columns: 17rem minmax(0, 1fr); }
  .stage { overflow: auto; min-width: 0; }
  .board { position: relative; }
  svg { position: absolute; inset: 0; overflow: visible; }
  .wire { fill: none; stroke: var(--map-fg-2); stroke-width: 1.5; }
  .via .wire { stroke-dasharray: 6 4; }
  .end { fill: none; stroke: var(--map-fg-2); stroke-width: 1.5; }
  .ring { fill: var(--map-surface); stroke: var(--map-fg-2); stroke-width: 1.5; }
  .label, .via-name { font: 11px var(--font-sans); fill: var(--map-muted); }
  .via-name { font-style: italic; }
  .card { position: absolute; box-sizing: border-box; display: flex; flex-direction: column; padding: 0 0 var(--space-2); overflow: hidden; text-align: left;
    font: inherit; color: var(--map-fg); background: var(--map-surface); border: 1.5px solid var(--map-line); border-radius: var(--radius-lg); }
  .side { cursor: pointer; }
  .side:hover { border-color: var(--map-trace); }
  .side:focus-visible { outline: 3px solid var(--map-blue); outline-offset: 2px; }
  .head { flex: none; box-sizing: border-box; padding: 0 var(--space-3); display: flex; flex-direction: column; justify-content: center; width: 100%;
    background: var(--map-sunken); border-bottom: 1px solid var(--map-line); }
  .centre { border-color: var(--map-trace); box-shadow: 0 0.375rem 1.25rem var(--map-shadow); }
  .centre .head { background: var(--map-trace); color: var(--map-surface); }
  .stereo { font-size: var(--text-2xs); font-style: italic; opacity: 0.75; }
  .name { font-size: var(--text-base); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .row { flex: none; box-sizing: border-box; width: 100%; padding: 0 var(--space-3); display: grid; grid-template-columns: 2.75rem minmax(0, 1fr) auto; align-items: center; gap: var(--space-2);
    font: var(--text-sm) var(--font-mono); }
  .row.fk { background: var(--map-sunken); }
  .row.pk { background: var(--map-tint-gold); }
  .keys { display: flex; gap: 0.1875rem; }
  .keys i { font: 700 var(--text-2xs) var(--font-sans); font-style: normal; padding: 0 var(--space-1); border-radius: var(--radius-sm); border: 1px solid transparent; }
  .b-pk { background: var(--map-gold); border-color: var(--map-gold); color: var(--map-surface); }
  .b-fk { border-color: var(--map-fg) !important; color: var(--map-fg); }
  .cname { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .type { color: var(--map-muted); white-space: nowrap; max-width: 8rem; overflow: hidden; text-overflow: ellipsis; }
  .note { flex: none; box-sizing: border-box; width: 100%; padding: 0 var(--space-3); display: flex; align-items: center; gap: var(--space-1-5); white-space: nowrap; overflow: hidden;
    font-size: var(--text-sm); color: var(--map-muted); }
  .i18n { border-top: 1px dashed var(--map-line); font-family: var(--font-mono); }
  .tag { font: 700 var(--text-2xs) var(--font-sans); font-style: normal; padding: 0 var(--space-1-5); border-radius: var(--radius-pill); border: 1px solid var(--map-line); color: var(--map-fg); }
  .empty { margin: var(--space-6); color: var(--map-muted); }
  @media (max-width: 47.5rem) { .body { grid-template-columns: minmax(0, 1fr); grid-template-rows: 14rem minmax(0, 1fr); } header { flex-wrap: wrap; } }
</style>
