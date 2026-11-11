<!-- Sizes are the layout's (TABLE in crates/ig-app/src/map/layout.rs), sized for the near card; far
     and mid are scaled up to read inside that frame. -->
<script lang="ts">
  import type { Look } from "./looks";
  import type { Column, Table } from "./types";
  import type { TableMark } from "./diff";
  import { cardLayout, type CardFacts } from "./db/card";

  let { table, look, rows, onopen, onfocus, zoom = 1, h = 128, facts, picked = false, fold = true }: {
    table: Table; look: Look; rows?: TableMark; onopen: () => void; onfocus?: () => void; zoom?: number; h?: number; facts?: CardFacts; picked?: boolean; fold?: boolean;
  } = $props();

  const SIGN = { added: "+", changed: "~", removed: "−" } as const;

  const view = $derived(table.kind === "view");
  const neutral = $derived(look.edge === "var(--map-edge)");
  const glow = $derived(look.glow ? `0 0 0 5px color-mix(in srgb, ${look.glow} 25%, transparent)` : "none");
  const mark = (c: Column) => rows?.columns.get(c.name.toLowerCase());
  const fk = (c: Column) => (c.references ? `${c.references.table}${c.references.column ? `.${c.references.column}` : ""}` : "");
  const hint = (c: Column) => [fk(c) && `→ ${fk(c)}`, c.nullable ? "null" : "", c.default ? `default ${c.default}` : "", c.check ? `check ${c.check}` : ""]
    .filter(Boolean).join(" · ");
  const pinnedTo = (c: Column) => (c.references ? facts?.pinned.get(c.references.table.toLowerCase()) : undefined);

  const card = $derived(cardLayout(table, zoom, h, fold, (c) => !!mark(c)));
  const { at, m, folded, shown, keys, foot } = $derived(card);
  const tags = $derived.by(() => {
    const t: { text: string; tone: string; title: string }[] = [];
    if (table.translations?.length) t.push({ text: "i18n", tone: "i18n", title: `translated: ${table.translations.join(", ")}` });
    if (facts?.hub) t.push({ text: `←${facts.hub}`, tone: "hub", title: `${facts.hub} tables point at it` });
    if (rows) {
      const n = (k: "added" | "changed") => [...rows.columns.values()].filter((v) => v === k).length;
      const d = [rows.added ? "new" : "", n("added") ? `+${n("added")}` : "", n("changed") ? `~${n("changed")}` : "", rows.removed.length ? `−${rows.removed.length}` : ""];
      const text = d.filter(Boolean).join(" ");
      if (text) t.push({ text, tone: "diff", title: "changed in this diff" });
    }
    return t;
  });
</script>

{#snippet row(c: Column)}
  {@const k = mark(c)}
  {@const to = pinnedTo(c)}
  <span class="col {k ?? ''}" class:pinned={!!to} title={hint(c) || undefined}>
    <span class="keys">{#if c.pk}<i class="pk">PK</i>{/if}{#if c.references}<i class="fk">FK</i>{/if}</span>
    <span class="cname">{#if k}<span class="sign">{SIGN[k]}</span>{/if}{c.name}</span>
    {#if to}<span class="type to">→ {to}</span>{:else}<span class="type">{c.type}{c.type && c.nullable ? "?" : ""}</span>{/if}
  </span>
{/snippet}

<button class="table {at}" class:view class:picked type="button" aria-label="Open {table.name}" onclick={onopen}
  style:border-color={neutral ? null : look.edge} style:border-style={look.dashed ? "dashed" : null} style:box-shadow={glow} style:opacity={look.fade}>
  <span class="card" style:width={m === 1 ? null : `${100 / m}%`} style:height={m === 1 ? null : `${100 / m}%`} style:transform={m === 1 ? null : `scale(${m})`}>
    {#if at === "far"}
      <span class="chip">
        <span class="stereo">«{table.kind}»</span>
        <b class="name">{table.name}</b>
        <span class="count">{table.columns.length} {table.columns.length === 1 ? "column" : "columns"}</span>
        {#if tags.length}<span class="tags">{#each tags as t (t.tone)}<i class="tag {t.tone}" title={t.title}>{t.text}</i>{/each}</span>{/if}
      </span>
    {:else}
      <span class="head">
        <span class="stereo">«{table.kind}»{#if rows?.added} <b class="new">new</b>{/if}</span>
        <b class="name">{table.name}</b>
      </span>
      <span class="rows">
        {#if at === "mid"}
          {#each keys as c, i (i)}{@render row(c)}{/each}
          {#if foot}<span class="fold">{foot}</span>{/if}
        {:else}
          {#each shown as c, i (i)}{@render row(c)}{/each}
          {#if folded.length}<span class="fold bookkeeping" title={folded.map((c) => c.name).join(", ")}>+{folded.length} <span class="names">{folded.map((c) => c.name).join(", ")}</span></span>{/if}
          {#if table.translations?.length}<span class="i18n"><i class="tag i18n">i18n</i><span class="names">{table.translations.join(", ")}</span></span>{/if}
          {#each rows?.removed ?? [] as c, i (i)}
            <span class="col removed" title="dropped by this change">
              <span class="keys">{#if c.pk}<i class="pk">PK</i>{/if}{#if c.references}<i class="fk">FK</i>{/if}</span>
              <span class="cname"><span class="sign">{SIGN.removed}</span><s>{c.name}</s></span>
              <span class="type"><s>{c.type}</s></span>
            </span>
          {/each}
        {/if}
        {#if !table.columns.length && !rows?.removed.length}<span class="none">no columns read</span>{/if}
      </span>
    {/if}
  </span>
</button>
{#if onfocus && at !== "far"}
  <button class="focus" class:picked type="button" aria-label="Focus on {table.name}" title="Focus — Enter" style:top="{19 * m}px" style:right="{10 * m}px" style:scale={m === 1 ? null : m} onclick={onfocus}>Focus</button>
{/if}

<style>
  .table { font: inherit; text-align: left; cursor: pointer; width: 100%; height: 100%; box-sizing: border-box; padding: 0; position: relative; display: block;
    border: 1.5px solid var(--map-line); border-radius: 8px; background: var(--map-surface); color: var(--map-fg); overflow: hidden; }
  .table:focus-visible { outline: 3px solid var(--map-blue); outline-offset: 3px; }
  .card { position: absolute; left: 0; top: 0; width: 100%; height: 100%; transform-origin: 0 0; display: flex; flex-direction: column; padding-bottom: 8px; box-sizing: border-box; }
  .head { flex: none; height: 56px; box-sizing: border-box; padding: 8px 12px; display: flex; flex-direction: column; justify-content: center;
    background: var(--map-sunken); color: var(--map-fg); border-bottom: 1px solid var(--map-line); }
  .picked .head, .picked .chip { background: var(--map-trace); color: var(--map-surface); }
  .stereo { font: italic 10px/12px var(--font-sans); opacity: 0.75; }
  .new { font: 700 9px/12px var(--font-sans); text-transform: uppercase; background: var(--map-green); color: var(--map-surface); border-radius: 999px; padding: 1px 6px; margin-left: 4px; font-style: normal; }
  .name { font: 600 15px/20px var(--font-sans); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .chip { flex: 1; min-height: 0; margin-bottom: -8px; padding: 10px 12px; box-sizing: border-box; display: flex; flex-direction: column; justify-content: center; gap: 4px; }
  .chip .name { font-size: 18px; line-height: 22px; white-space: normal; overflow-wrap: anywhere; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; }
  .count { font: 12px/16px var(--font-sans); color: var(--map-muted); }
  .picked .count { color: inherit; opacity: 0.8; }
  .tags { display: flex; flex-wrap: wrap; gap: 4px; }
  .tag { font: 700 10px/16px var(--font-sans); font-style: normal; padding: 0 6px; border-radius: 999px; border: 1px solid var(--map-line); color: var(--map-fg); background: var(--map-surface); }
  .tag.hub { border-color: var(--map-gold); color: var(--map-gold); }
  .tag.diff { background: var(--map-warn-bg); color: var(--map-warn-text); border-color: transparent; }
  .rows { display: flex; flex-direction: column; min-height: 0; }
  .col { flex: none; height: 26px; box-sizing: border-box; padding: 0 12px; display: grid; grid-template-columns: 46px minmax(0, 1fr) auto; align-items: center; gap: 8px;
    font: 12.5px/26px var(--font-mono); }
  .keys { display: flex; gap: 3px; }
  .keys i { font: 700 9px/12px var(--font-sans); font-style: normal; padding: 0 4px; border-radius: 3px; border: 1px solid transparent; }
  .keys .pk { background: var(--map-gold); border-color: var(--map-gold); color: var(--map-surface); }
  .keys .fk { border-color: var(--map-fg); color: var(--map-fg); }
  .cname { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .type { color: var(--map-muted); white-space: nowrap; max-width: 120px; overflow: hidden; text-overflow: ellipsis; }
  .type.to { color: var(--map-gold); font-family: var(--font-sans); }
  .sign { font-weight: 700; margin-right: 6px; }
  .fold, .i18n { flex: none; height: 26px; box-sizing: border-box; padding: 0 12px; display: flex; align-items: center; gap: 6px; overflow: hidden; white-space: nowrap;
    font: 12px/26px var(--font-sans); color: var(--map-muted); }
  .i18n { border-top: 1px dashed var(--map-line); }
  .names { overflow: hidden; text-overflow: ellipsis; font-family: var(--font-mono); }
  .col.added { background: var(--map-tint-green); }
  .col.added .sign { color: var(--map-green); }
  .col.changed { background: var(--map-warn-bg); color: var(--map-warn-text); }
  .col.removed { background: var(--map-bad-bg); color: var(--map-bad-text); }
  .col.removed .type { color: inherit; }
  .focus { position: absolute; transform-origin: top right; font: 600 11px/18px var(--font-sans); padding: 0 8px; cursor: pointer;
    border: 1px solid var(--map-line); border-radius: 999px; background: var(--map-surface); color: var(--map-fg); }
  .focus:hover { border-color: var(--map-trace); }
  .focus:focus-visible { outline: 2px solid var(--map-blue); outline-offset: 2px; }
  .none { padding: 4px 12px; font-size: 12px; color: var(--map-faint); }
</style>
