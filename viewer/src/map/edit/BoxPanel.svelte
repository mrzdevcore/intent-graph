<script lang="ts">
  import { onMount } from "svelte";
  import type { KindView, MapArrow, MapBox, View } from "../types";
  import { iconMask, kindOf } from "./add";
  import { kindName } from "./kinds";
  import KindPicker from "./KindPicker.svelte";
  import Field from "./Field.svelte";
  import type { ArrowPick, EditSession } from "./session.svelte";

  let { view, box, s, context, onclose, onbox, onadd, onarrow }: {
    view: View; box: MapBox; s: EditSession; context: { parentKinds: string[]; inRun: boolean };
    onclose: () => void; onbox: (id: string) => void; onadd: (id: string) => void; onarrow: (p: ArrowPick) => void;
  } = $props();
  const id = $derived(box.id);
  const kind = $derived(kindOf(box, view.kinds) ?? null);
  const current = $derived(kindName(box));
  let picking = $state(false);
  const titleOf = (b: string) => view.boxes.find((x) => x.id === b)?.title.value ?? b;
  // set keys of their own: a kind's property of that name cannot be set from here
  const TAKEN = new Set(["kind", "at", "context", "title", "description", "tech"]);
  const own = $derived((kind?.props ?? []) as KindView["props"]);
  const arrows = $derived(view.arrows.filter((a) => !a.ghost).flatMap((a): { a: MapArrow; pick: ArrowPick }[] =>
    a.from === id || a.to === id ? [{ a, pick: { id: a.id } }]
      : (a.parts ?? []).filter((p) => p.from === id || p.to === id).map((p) => ({ a: p, pick: { id: a.id, part: { from: p.from, to: p.to } } }))));
  const inside = $derived(view.boxes.filter((b) => b.parent === id && b.level !== "code"));
  const save = (key: string) => (v: string | null, failed: () => void) => s.setBox(id, key, v, failed);
  const globs = (v: string) => v.split("\n").map((g) => g.trim()).filter(Boolean).join("\n") || null;
  function chip(name: string, v: string, required: boolean) {
    const was = box.props?.[name] ?? null;
    s.setBox(id, name, was === v && !required ? null : v);
  }
  function pick(k: string) {
    picking = false;
    s.setKind([id], k);
  }
  let panel: HTMLElement;
  onMount(() => panel.focus({ preventScroll: true }));
  function keydown(e: KeyboardEvent) {
    if (e.key !== "Escape" || picking) return;
    e.preventDefault();
    e.stopPropagation();
    (document.activeElement as HTMLElement | null)?.blur();
    onclose();
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<section class="panel" data-panel="box" aria-label="Edit {box.title.value}" tabindex="-1" bind:this={panel} onkeydown={keydown}>
  <div class="head">
    <Field name="title" label="Name" value={box.title.value} hand norm={(v) => v.trim() || box.title.value} onsave={save("title")} />
    <button type="button" class="x" aria-label="Close" onclick={onclose}>×</button>
  </div>
  <div class="kind-at">
    <span class="lab">Kind</span>
    <button type="button" class="kind" class:none={current === "box"} data-kind-button aria-haspopup="menu" aria-expanded={picking} onclick={() => (picking = !picking)}>
      {#if kind}<i style:mask-image={iconMask(kind.icon)} aria-hidden="true"></i>{/if}<span>{kind?.title ?? (current === "box" ? "No kind yet" : current)}</span><b aria-hidden="true">▾</b>
    </button>
    {#if picking}<KindPicker kinds={view.kinds} {current} {context} onpick={pick} onclose={() => (picking = false)} />{/if}
  </div>
  <Field name="description" label="Description" value={box.description?.value ?? null} help="One line, shown on hover and in lists." onsave={save("description")} />
  <Field name="tech" label="Tech" value={box.tech?.value ?? null} onsave={save("tech")} />
  {#each own as p (p.name)}
    {#if TAKEN.has(p.name)}
      <div class="field"><span class="lab">{p.name}</span><span class="ro" title="Change it in ig/">{box.props?.[p.name] ?? "—"}</span></div>
    {:else if p.oneOf.length}
      <div class="field" role="group" aria-label={p.name}>
        <span class="lab" title={p.means}>{p.name}</span>
        <span class="chips">{#each p.oneOf as o}<button type="button" data-prop={p.name} aria-pressed={box.props?.[p.name] === o} onclick={() => chip(p.name, o, p.required)}>{o}</button>{/each}</span>
      </div>
    {:else}
      <Field name="prop-{p.name}" label={p.name} value={box.props?.[p.name] ?? null} placeholder={p.means} onsave={save(p.name)} />
    {/if}
  {/each}
  <Field name="context" label="Context for whoever builds it" value={box.context?.value ?? null} lines
    help="As long as it needs: kept here, never on the map. Shift+Enter for a new line." onsave={save("context")} />
  {#if kind?.code !== false}
    <Field name="at" label="Made of" value={box.madeOf.join("\n") || null} lines norm={globs} placeholder="src/orders/**" help="The code it is made of, a glob per line." onsave={save("at")} />
  {/if}
  {#if arrows.length}
    <h3>Arrows · {arrows.length}</h3>
    <ul class="list">
      {#each arrows as { a, pick: p } (a.id + (p.part ? `:${a.from}->${a.to}` : ""))}
        <li><button type="button" data-arrow-row={a.id} onclick={() => onarrow(p)}>
          <span class="k">{a.kind}</span><span class="n">{a.from === id ? "→" : "←"} {titleOf(a.from === id ? a.to : a.from)}</span>
          {#if a.origin === "build"}<span class="planned">planned</span>{/if}
        </button></li>
      {/each}
    </ul>
  {/if}
  <h3>Inside{inside.length ? ` · ${inside.length}` : ""}</h3>
  <ul class="list">
    {#each inside as b (b.id)}<li><button type="button" data-inside={b.id} onclick={() => onbox(b.id)}><span class="n">{b.title.value}</span><span class="k">{kindName(b) === "box" ? "no kind" : kindName(b)}</span></button></li>{/each}
    <li><button type="button" class="add" data-add-inside onclick={() => onadd(id)}>+ Add a box</button></li>
  </ul>
</section>

<style>
  .panel { position: absolute; top: 3.5rem; right: var(--space-4); width: 21.25rem; max-height: calc(100% - 7.5rem); overflow: auto; box-sizing: border-box; z-index: 4;
    color: var(--map-fg); background: var(--map-surface); border: 1px solid var(--map-line); border-radius: var(--radius-xl); padding: var(--space-3) var(--space-3-5);
    font-size: var(--text-md); box-shadow: 0 6px 20px var(--map-shadow); overflow-wrap: anywhere; }
  .panel:focus { outline: none; }
  .head { display: flex; align-items: start; gap: var(--space-2); }
  .head > :global(.field) { flex: 1; margin-top: 0; }
  .x { font: inherit; border: 0; background: none; cursor: pointer; font-size: var(--text-3xl); line-height: 1; color: var(--map-muted); }
  .lab { font-size: var(--text-xs); text-transform: uppercase; letter-spacing: 0.06em; color: var(--map-muted); }
  .field { display: flex; flex-direction: column; gap: var(--space-0-5); margin: var(--space-2) 0; }
  .ro { color: var(--map-fg-2); }
  .kind-at { position: relative; display: flex; flex-direction: column; align-items: start; gap: var(--space-0-5); margin: var(--space-2) 0; }
  button { font: inherit; color: inherit; cursor: pointer; }
  .kind { display: inline-flex; align-items: center; gap: var(--space-1); background: none; border: 1px solid var(--map-line); border-radius: var(--radius-lg); padding: var(--space-1) var(--space-2); }
  .kind.none { border-style: dashed; color: var(--map-muted); }
  .kind b { font-weight: 400; color: var(--map-faint); }
  .kind i { width: 0.875rem; height: 0.875rem; background: currentColor; mask-repeat: no-repeat; mask-position: center; mask-size: contain; }
  .chips { display: flex; flex-wrap: wrap; gap: var(--space-1); }
  .chips button { font-size: var(--text-sm); padding: var(--space-0-5) var(--space-2); border-radius: var(--radius-pill); border: 1px solid var(--map-line); background: none; color: var(--map-fg-2); }
  .chips button[aria-pressed="true"] { background: var(--map-tint-gold); border-color: var(--map-trace); color: var(--map-fg); }
  h3 { margin: var(--space-3) 0 var(--space-1); font-size: var(--text-xs); text-transform: uppercase; letter-spacing: 0.06em; color: var(--map-muted); }
  .list { margin: 0; padding: 0; list-style: none; }
  .list button { display: flex; align-items: baseline; gap: var(--space-2); width: 100%; text-align: left; background: none; border: 0; border-radius: var(--radius-md); padding: var(--space-1) var(--space-1-5); }
  .list button:hover { background: var(--map-sunken); }
  .list .n { flex: 1; min-width: 0; }
  .list .k { font-size: var(--text-xs); padding: var(--space-0-5) var(--space-1-5); border-radius: var(--radius-pill); background: var(--map-sunken); color: var(--map-fg-2); }
  .planned { font-size: var(--text-xs); color: var(--map-trace); }
  .add { color: var(--map-muted); }
  button:focus-visible { outline: 2px solid var(--map-trace); outline-offset: 1px; }
  @media (max-width: 47.5rem) { .panel { left: var(--space-4); right: var(--space-4); width: auto; top: auto; bottom: 3.5rem; max-height: 50%; } }
</style>
