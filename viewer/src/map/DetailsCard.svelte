<!-- The selected box: health, what needs attention, what it uses and what uses it, what it is made of and
     where each annotated field came from. All text, never HTML. -->
<script lang="ts">
  import type { EditApi } from "../api";
  import { tag } from "./looks";
  import type { Field, MapBox, View } from "./types";
  import Code from "./Code.svelte";

  let { view, box, onclose, onopen, onmenu, edit, run }: {
    view: View; box: MapBox; onclose: () => void; onopen: (id: string) => void; onmenu?: (e: MouseEvent) => void;
    edit?: EditApi; run?: (p: Promise<View>) => void;
  } = $props();
  // the edits Claude or the reading proposed on this box, to keep or drop here
  const proposals = $derived((view.proposals ?? []).filter((p) => p.ids.includes(box.id)));

  const titleOf = (id: string) => view.boxes.find((b) => b.id === id)?.title.value ?? id;
  // a part named like a component on the map (a tag it renders): a link to it
  const components = $derived(new Map(view.boxes.filter((b) => b.kind.value === "component").map((b) => [b.title.value, b.id])));
  const mine = $derived(view.arrows.filter((a) => a.from === box.id || a.to === box.id));
  const touched = $derived(new Set([box.id, ...mine.map((a) => a.id)]));
  const health = $derived(view.marks.filter((m) => m.layer === "health" && m.target === box.id));
  const attention = $derived(view.marks.filter((m) => (m.layer === "rules" || m.layer === "risks") && m.state !== "ok" && touched.has(m.target)));
  const source = $derived(view.sources?.[box.id]);
  const parts = $derived(view.parts?.[box.id]);
  // an arrow from → to is from's own calls and imports: from uses to
  const byCount = (a: { count: number }, b: { count: number }) => b.count - a.count;
  const usedBy = $derived(mine.filter((a) => a.to === box.id).sort(byCount));
  const uses = $derived(mine.filter((a) => a.from === box.id).sort(byCount));
  const groups = $derived([
    { name: "Uses", list: uses, end: "to" as const },
    { name: "Used by", list: usedBy, end: "from" as const },
  ]);
  const top = $derived(Math.max(1, ...mine.map((a) => a.count)));
  const sum = (as: { count: number }[]) => as.reduce((n, a) => n + a.count, 0);
  const inside = $derived(view.boxes.filter((b) => b.parent === box.id).length);
  const chips = $derived([box.kind.value, ...(box.tech?.value.split("·").map((t) => t.trim()).filter(Boolean) ?? [])]);
  const SHOWN = 5;
  let allPaths = $state(false);
  const paths = $derived(allPaths ? box.madeOf : box.madeOf.slice(0, SHOWN));
  const fields = $derived(
    (["kind", "title", "tech", "description", "logo", "badges", "pattern", "slot"] as const)
      .map((k) => [k, box[k] as Field<unknown> | undefined] as const)
      .filter((e): e is readonly [typeof e[0], Field<unknown>] => !!e[1]),
  );
  let card: HTMLElement;
  // focus follows the selection into the card
  $effect(() => {
    void box.id;
    allPaths = false;
    // a menu opened by right-click keeps the focus it took
    if (!document.activeElement?.closest?.("[data-popover]")) card.focus({ preventScroll: true });
  });
</script>

<section class="details" class:wide={!!source} aria-label="Details" tabindex="-1" bind:this={card}>
  <div class="head">
    <b>{box.title.value}</b>
    {#if onmenu}<button type="button" aria-label="Box menu" aria-haspopup="menu" onclick={onmenu}>⋯</button>{/if}
    <button type="button" aria-label="Close" onclick={onclose}>×</button>
  </div>
  <ul class="chips">{#each chips as c}<li>{c}</li>{/each}</ul>
  {#if box.origin === "build"}<p class="muted">To build: no code under <code>{box.madeOf[0] ?? box.id}</code> yet</p>
  {:else if box.origin === "drift"}<p class="muted">Drift: this code is in no box of ig/</p>{/if}
  {#if edit && run}
    {#each proposals as p (p.n)}
      <div class="prop"><span class="text"><span class="ptag">{tag(p.by)}</span> {p.summary}</span>
        <button type="button" onclick={() => run(edit.drop(p.n))}>Drop</button>
        <button type="button" class="go" onclick={() => run(edit.keep(p.n))}>Keep</button></div>
    {/each}
  {/if}
  {#if box.description}<p>{box.description.value}</p>{/if}
  {#if inside || mine.length}
    <dl class="stats">
      <div><dt>parts inside</dt><dd>{inside.toLocaleString("en")}</dd></div>
      <div><dt>links out</dt><dd>{sum(uses).toLocaleString("en")}</dd></div>
      <div><dt>links in</dt><dd>{sum(usedBy).toLocaleString("en")}</dd></div>
    </dl>
  {/if}

  {#if health.length}
    <h3>Health</h3>
    <ul>{#each health as m}<li class={m.state}>{m.value ?? m.state}{m.note ? ` — ${m.note}` : ""}</li>{/each}</ul>
  {/if}
  {#if attention.length}
    <h3>Needs attention</h3>
    <ul>{#each attention as m}<li class={m.state}>{m.badge ?? m.value ?? m.state}{m.note ? ` — ${m.note}` : ""}</li>{/each}</ul>
  {/if}
  {#each groups as { name, list, end }}
    {#if list.length}
      <h3>{name} · {list.length}</h3>
      <ul class="links {end}">
        {#each list as a}
          <li><button type="button" onclick={() => onopen(a[end])}>
            <span class="row"><span class="name">{titleOf(a[end])}</span><b>×{a.count.toLocaleString("en")}</b></span>
            <span class="bar"><span style:width="{Math.max(2, (a.count / top) * 100)}%"></span></span>
            <span class="muted">{a.label ?? a.kind}</span>
          </button></li>
        {/each}
      </ul>
    {/if}
  {/each}
  {#if parts?.length}
    <h3>{box.title.value}</h3>
    <ul>{#each parts as p}<li><span class="muted">{p.kind}</span> {#if components.get(p.name) && components.get(p.name) !== box.parent}<button type="button" class="tag" onclick={() => onopen(components.get(p.name)!)}><code>{p.name}</code></button>{:else}<code>{p.name}</code>{/if}{p.detail ? ` ${p.detail}` : ""} <span class="muted">:{p.line}</span></li>{/each}</ul>
  {/if}
  {#if source}
    <h3>Code</h3>
    <p class="muted"><code>{source.file}:{source.line}–{source.line + source.text.split("\n").length - 1 + (source.cut ?? 0)}</code> · complexity {source.complexity}</p>
    <pre class="code"><Code {source} /></pre>
  {/if}
  {#if box.madeOf.length}
    <h3>Made of · {box.madeOf.length} {box.madeOf.length === 1 ? "path" : "paths"}{box.madeOfSource ? ` (${box.madeOfSource})` : ""}</h3>
    <!-- no editor-link helper exists in the viewer: the paths, as text -->
    <ul class="paths">{#each paths as p}<li>{p}</li>{/each}</ul>
    {#if box.madeOf.length > SHOWN}
      <button type="button" class="more" onclick={() => (allPaths = !allPaths)}>{allPaths ? "Show fewer" : `Show all ${box.madeOf.length}`}</button>
    {/if}
  {/if}
  {#if box.serviceName || box.package}
    <p class="muted">
      {#if box.serviceName}service <code>{box.serviceName}</code>{/if}
      {#if box.package}package <code>{box.package}</code>{/if}
    </p>
  {/if}
  <details class="muted">
    <summary>Where these facts come from</summary>
    <ul>{#each fields as [k, f]}<li>{k}: {f.source}{f.confidence ? `, ${f.confidence}` : ""}</li>{/each}</ul>
  </details>
</section>

<style>
  .details { position: absolute; top: 3.5rem; right: var(--space-4); width: 21.25rem; color: var(--map-fg); max-height: calc(100% - 7.5rem); overflow: auto;
    background: var(--map-surface); border: 1px solid var(--map-line); border-radius: var(--radius-xl); padding: var(--space-3) var(--space-3-5); font-size: var(--text-md);
    box-shadow: 0 6px 20px var(--map-shadow); overflow-wrap: anywhere; }
  .details.wide { width: 30rem; }
  .code { margin: var(--space-1) 0; padding: var(--space-2); max-height: 22.5rem; overflow: auto; background: var(--map-sunken); border-radius: var(--radius-md);
    font: 0.71875rem/1.3 var(--font-mono); white-space: pre; overflow-wrap: normal; }
  .details:focus { outline: none; }
  .details:focus-visible { outline: 3px solid var(--map-blue); outline-offset: 2px; }
  .head { display: flex; align-items: start; gap: var(--space-2); }
  .head b { flex: 1; font-size: var(--text-xl); }
  .head button { font: inherit; border: 0; background: none; cursor: pointer; font-size: var(--text-3xl); line-height: 1; color: var(--map-muted); }
  h3 { margin: var(--space-3) 0 var(--space-1); font-size: var(--text-xs); text-transform: uppercase; letter-spacing: 0.06em; color: var(--map-muted); }
  p { margin: var(--space-1) 0; }
  ul { margin: 0; padding: 0; list-style: none; }
  li { padding: 1px 0; }
  .muted { color: var(--map-muted); }
  /* app.css's code takes the app's colours; the card takes the map's */
  code { background: var(--map-sunken); color: inherit; }
  .chips { display: flex; flex-wrap: wrap; gap: var(--space-1); margin: var(--space-1-5) 0; }
  .chips li { font-size: var(--text-xs); padding: var(--space-0-5) 0.4375rem; border-radius: var(--radius-pill); background: var(--map-sunken); color: var(--map-fg-2); }
  .stats { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: var(--space-2); margin: var(--space-2-5) 0 0; }
  .stats div { display: flex; flex-direction: column-reverse; background: var(--map-sunken); border-radius: var(--radius-lg); padding: var(--space-1-5) var(--space-2-5); }
  .stats dt { font-size: var(--text-xs); color: var(--map-muted); }
  .stats dd { margin: 0; font-size: var(--text-2xl); font-weight: 600; font-variant-numeric: tabular-nums; }
  .links button { display: flex; flex-direction: column; gap: 0.1875rem; width: calc(100% + 0.75rem); margin: 0 calc(-1 * var(--space-1-5)); padding: var(--space-1) var(--space-1-5);
    font: inherit; color: inherit; text-align: left; border: 0; border-radius: var(--radius-md); background: none; cursor: pointer; }
  .links button:hover { background: var(--map-sunken); }
  .links .row { display: flex; gap: var(--space-2); align-items: baseline; }
  .links .name { flex: 1; font-weight: 500; }
  .links b { font-variant-numeric: tabular-nums; }
  .bar { display: block; height: 0.25rem; border-radius: 0.125rem; background: var(--map-track); }
  .bar span { display: block; height: 100%; border-radius: 0.125rem; background: var(--map-bar-out); }
  .from .bar span { background: var(--map-bar-in); }
  .muted, .links .muted { font-size: var(--text-sm); }
  .paths { font: var(--text-sm)/1.6 var(--font-mono); }
  .more { margin-top: var(--space-1); font: inherit; font-size: var(--text-sm); border: 1px solid var(--map-line); background: var(--map-surface); color: inherit; border-radius: var(--radius-md); padding: var(--space-1) var(--space-2-5); cursor: pointer; }
  details { margin-top: var(--space-3); padding-top: var(--space-2); border-top: 1px solid var(--map-line-soft); }
  summary { cursor: pointer; }
  .prop { display: flex; align-items: center; gap: var(--space-1-5); margin: var(--space-1-5) 0; padding: var(--space-1-5) var(--space-2); border: 1px dashed var(--map-trace); border-radius: var(--radius-lg); }
  .prop .text { flex: 1; }
  .ptag { font-size: var(--text-xs); padding: var(--space-0-5) 0.4375rem; border-radius: var(--radius-pill); background: var(--map-sunken); color: var(--map-trace); font-weight: 600; }
  .prop button { font: inherit; font-size: var(--text-sm); border: 1px solid var(--map-line); background: var(--map-surface); color: var(--map-fg); border-radius: var(--radius-md); padding: var(--space-1) var(--space-2); cursor: pointer; }
  .prop button.go { border-color: var(--map-trace); background: var(--map-trace); color: var(--map-ground); font-weight: 600; }
  .prop button:focus-visible { outline: 2px solid var(--map-trace); outline-offset: 2px; }
  .tag { font: inherit; color: inherit; border: 0; padding: 0; background: none; cursor: pointer; text-decoration: underline; text-decoration-color: var(--map-blue); }
  .warn { color: var(--map-warn-text); } .bad { color: var(--map-bad-text); } .down { color: var(--map-fg-2); } .ok { color: var(--map-green); }
  @media (max-width: 47.5rem) { .details, .details.wide { left: var(--space-4); right: var(--space-4); width: auto; top: auto; bottom: 3.5rem; max-height: 50%; } }
</style>
