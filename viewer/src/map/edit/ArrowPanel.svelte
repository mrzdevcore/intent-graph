<script lang="ts">
  import { onMount } from "svelte";
  import type { ArrowKind, MapArrow } from "../types";
  import { arrowKdl, specKind, type Editing } from "./arrows";
  import Field from "./Field.svelte";
  import type { EditSession } from "./session.svelte";

  let { a, how, kinds, s, titleOf, focus = null, onclose }: {
    a: MapArrow; how: Editing; kinds: ArrowKind[]; s: EditSession; titleOf: (id: string) => string; focus?: "context" | null; onclose: () => void;
  } = $props();
  const can = $derived(how === "edit" || how === "drift");
  const kind = $derived(specKind(a));
  const save = (key: "label" | "note" | "context") => (v: string | null, failed: () => void) => s.arrowSet(key, v, failed);
  let panel: HTMLElement;
  onMount(() => {
    const f = focus === "context" ? panel.querySelector<HTMLElement>('[data-field="context"]') : null;
    (f ?? panel).focus({ preventScroll: true });
  });
  function keydown(e: KeyboardEvent) {
    if (e.key !== "Escape") return;
    e.preventDefault();
    e.stopPropagation();
    (document.activeElement as HTMLElement | null)?.blur();
    onclose();
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<section class="panel" data-panel="arrow" aria-label="Edit arrow {titleOf(a.from)} → {titleOf(a.to)}" tabindex="-1" bind:this={panel} onkeydown={keydown}>
  <div class="head">
    <b>{titleOf(a.from)} → {titleOf(a.to)}</b>
    <button type="button" class="x" aria-label="Close" onclick={onclose}>×</button>
  </div>
  {#if how === "drift"}<p class="muted">Seen in the code, not in ig/ yet: a change writes it there.</p>{/if}
  <div class="field" role="group" aria-label="Kind">
    <span class="lab">Kind</span>
    <span class="chips">{#each kinds as k}<button type="button" data-arrow-kind={k} aria-pressed={k === kind} disabled={!can} onclick={() => s.arrowSet("kind", k)}>{k}</button>{/each}</span>
  </div>
  <Field name="label" label="Label" value={s.text(a, "label")} disabled={!can} onsave={save("label")} />
  <Field name="note" label="On the arrow" value={a.note ?? null} max={80} disabled={!can} help="One short line, shown under the label." onsave={save("note")} />
  <Field name="context" label="Context for whoever builds it" value={a.context ?? null} lines disabled={!can}
    help="As long as it needs: kept here, never on the map. Shift+Enter for a new line." onsave={save("context")} />
  {#if how !== "bundle"}
    <h3>In ig/</h3>
    <pre class="kdl" data-kdl>{arrowKdl(a)}</pre>
  {/if}
</section>

<style>
  .panel { position: absolute; top: 3.5rem; right: var(--space-4); width: 21.25rem; max-height: calc(100% - 7.5rem); overflow: auto; box-sizing: border-box; z-index: 4;
    color: var(--map-fg); background: var(--map-surface); border: 1px solid var(--map-line); border-radius: var(--radius-xl); padding: var(--space-3) var(--space-3-5);
    font-size: var(--text-md); box-shadow: 0 6px 20px var(--map-shadow); overflow-wrap: anywhere; }
  .panel:focus { outline: none; }
  .head { display: flex; align-items: start; gap: var(--space-2); }
  .head b { flex: 1; font-family: var(--font-hand); font-size: var(--text-xl); }
  .x { font: inherit; border: 0; background: none; cursor: pointer; font-size: var(--text-3xl); line-height: 1; color: var(--map-muted); }
  .muted { margin: var(--space-1) 0; font-size: var(--text-sm); color: var(--map-muted); }
  .lab { font-size: var(--text-xs); text-transform: uppercase; letter-spacing: 0.06em; color: var(--map-muted); }
  .field { display: flex; flex-direction: column; gap: var(--space-0-5); margin: var(--space-2) 0; }
  .chips { display: flex; flex-wrap: wrap; gap: var(--space-1); }
  .chips button { font: inherit; font-size: var(--text-sm); cursor: pointer; padding: var(--space-0-5) var(--space-2); border-radius: var(--radius-pill); border: 1px solid var(--map-line);
    background: none; color: var(--map-fg-2); }
  .chips button[aria-pressed="true"] { background: var(--map-tint-gold); border-color: var(--map-trace); color: var(--map-fg); }
  .chips button:disabled { cursor: default; color: var(--map-faint); }
  button:focus-visible { outline: 2px solid var(--map-trace); outline-offset: 1px; }
  h3 { margin: var(--space-3) 0 var(--space-1); font-size: var(--text-xs); text-transform: uppercase; letter-spacing: 0.06em; color: var(--map-muted); }
  .kdl { margin: 0; padding: var(--space-2); background: var(--map-sunken); border-radius: var(--radius-md); font: var(--text-sm)/1.4 var(--font-mono); white-space: pre-wrap; overflow-wrap: anywhere; }
  @media (max-width: 47.5rem) { .panel { left: var(--space-4); right: var(--space-4); width: auto; top: auto; bottom: 3.5rem; max-height: 50%; } }
</style>
