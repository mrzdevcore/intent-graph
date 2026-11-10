<!-- A table or view box: a header with its name, then a row per column — PK and FK badges, the
     name, the type on the right. The sizes are the layout's (TABLE in crates/ig-app/src/map/layout.rs):
     a 56px header and 26px rows. In a diff, `rows` marks the columns added and changed, and the
     base's columns the head dropped are drawn struck through below the rest. -->
<script lang="ts">
  import type { Look } from "./looks";
  import type { Column, Table } from "./types";
  import type { TableMark } from "./diff";

  let { table, look, rows, onopen }: { table: Table; look: Look; rows?: TableMark; onopen: () => void } = $props();

  const view = $derived(table.kind === "view");
  const neutral = $derived(look.edge === "var(--map-edge)");
  const glow = $derived(look.glow ? `0 0 0 5px color-mix(in srgb, ${look.glow} 25%, transparent)` : "none");
  const mark = (c: Column) => rows?.columns.get(c.name.toLowerCase());
  const fk = (c: Column) => (c.references ? `${c.references.table}${c.references.column ? `.${c.references.column}` : ""}` : "");
  const hint = (c: Column) => [fk(c) && `→ ${fk(c)}`, c.nullable ? "null" : "", c.default ? `default ${c.default}` : "", c.check ? `check ${c.check}` : ""]
    .filter(Boolean).join(" · ");
  const SIGN = { added: "+", changed: "~", removed: "−" } as const;
</script>

<button class="table" class:view type="button" aria-label="Open {table.name}" onclick={onopen}
  style:border-color={neutral ? null : look.edge} style:border-style={look.dashed ? "dashed" : null} style:box-shadow={glow} style:opacity={look.fade}>
  <span class="head">
    <span class="stereo">«{table.kind}»{#if rows?.added} <b class="new">new</b>{/if}</span>
    <b class="name">{table.name}</b>
  </span>
  <span class="rows">
    {#each table.columns as c, i (i)}
      {@const m = mark(c)}
      <span class="col {m ?? ''}" title={hint(c) || undefined}>
        <span class="keys">{#if c.pk}<i class="pk">PK</i>{/if}{#if c.references}<i class="fk">FK</i>{/if}</span>
        <span class="cname">{#if m}<span class="sign">{SIGN[m]}</span>{/if}{c.name}</span>
        <span class="type">{c.type}{c.type && c.nullable ? "?" : ""}</span>
      </span>
    {/each}
    {#each rows?.removed ?? [] as c, i (i)}
      <span class="col removed" title="dropped by this change">
        <span class="keys">{#if c.pk}<i class="pk">PK</i>{/if}{#if c.references}<i class="fk">FK</i>{/if}</span>
        <span class="cname"><span class="sign">{SIGN.removed}</span><s>{c.name}</s></span>
        <span class="type"><s>{c.type}</s></span>
      </span>
    {/each}
    {#if !table.columns.length && !rows?.removed.length}<span class="none">no columns read</span>{/if}
  </span>
</button>

<style>
  .table { font: inherit; text-align: left; cursor: pointer; width: 100%; min-height: 100%; box-sizing: border-box; padding: 0 0 8px; display: flex;
    flex-direction: column; border: 1.5px solid var(--map-line); border-radius: 8px; background: var(--map-surface); color: var(--map-fg); overflow: hidden; }
  .table:focus-visible { outline: 3px solid var(--map-blue); outline-offset: 3px; }
  .head { flex: none; height: 56px; box-sizing: border-box; padding: 8px 12px; display: flex; flex-direction: column; justify-content: center;
    background: var(--map-table-head); color: #fff; }
  .view .head { background: var(--map-view-head); }
  .stereo { font: italic 10px/12px var(--font-sans); opacity: 0.8; }
  .new { font: 700 9px/12px var(--font-sans); text-transform: uppercase; background: var(--map-green); border-radius: 999px; padding: 1px 6px; margin-left: 4px; font-style: normal; }
  .name { font: 600 15px/20px var(--font-sans); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .rows { display: flex; flex-direction: column; }
  .col { height: 26px; box-sizing: border-box; padding: 0 12px; display: grid; grid-template-columns: 46px minmax(0, 1fr) auto; align-items: center; gap: 8px;
    font: 12.5px/26px var(--font-mono); }
  .keys { display: flex; gap: 3px; }
  .keys i { font: 700 9px/14px var(--font-sans); font-style: normal; padding: 0 4px; border-radius: 3px; color: #fff; }
  .pk { background: var(--map-gold); }
  .fk { background: var(--map-blue); }
  .cname { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .type { color: var(--map-muted); white-space: nowrap; max-width: 120px; overflow: hidden; text-overflow: ellipsis; }
  .sign { font-weight: 700; margin-right: 6px; }
  .col.added { background: var(--map-tint-green); }
  .col.added .sign { color: var(--map-green); }
  .col.changed { background: var(--map-warn-bg); color: var(--map-warn-text); }
  .col.removed { background: var(--map-bad-bg); color: var(--map-bad-text); }
  .col.removed .type { color: inherit; }
  .none { padding: 4px 12px; font-size: 12px; color: var(--map-faint); }
</style>
