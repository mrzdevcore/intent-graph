<script lang="ts">
  import type { ArrowKind, MapArrow } from "../types";
  import { arrowKdl, specKind, type Editing } from "./arrows";
  import Field from "./Field.svelte";
  import Panel from "./Panel.svelte";
  import type { EditSession } from "./session.svelte";

  let { a, how, kinds, s, titleOf, focus = null, onclose }: {
    a: MapArrow; how: Editing; kinds: ArrowKind[]; s: EditSession; titleOf: (id: string) => string; focus?: "context" | null; onclose: () => void;
  } = $props();
  const can = $derived(how === "edit" || how === "drift");
  const kind = $derived(specKind(a));
  const file = $derived(a.written?.replace(/:\d+$/, "") ?? "ig/");
  const save = (key: "label" | "note" | "context") => (v: string | null, failed: () => void) => s.arrowSet(key, v, failed);
</script>

<Panel what="arrow" label="Edit arrow {titleOf(a.from)} → {titleOf(a.to)}" focus={focus === "context" ? '[data-field="context"]' : undefined} {onclose}>
  {#snippet head()}<b class="title">{titleOf(a.from)} → {titleOf(a.to)}</b>{/snippet}
  {#if how === "drift"}<p class="muted">Seen in the code, not in ig/ yet: a change writes it there.</p>{/if}
  <div class="group" role="group" aria-label="Kind">
    <span class="lab">Kind</span>
    <span class="chips">{#each kinds as k}<button type="button" data-arrow-kind={k} aria-pressed={k === kind} disabled={!can} onclick={() => s.arrowSet("kind", k)}>{k}</button>{/each}</span>
  </div>
  <Field name="label" label="Label" value={s.text(a, "label")} disabled={!can} onsave={save("label")} />
  <Field name="note" label="On the arrow" value={a.note ?? null} max={80} disabled={!can} help="One short line, shown under the label." onsave={save("note")} />
  <Field name="context" label="Context for whoever builds it" value={a.context ?? null} lines disabled={!can}
    help="As long as it needs: kept here, never on the map. Shift+Enter for a new line." onsave={save("context")} />
  {#if how !== "bundle"}
    <h3 data-written title={a.written}>In {file}</h3>
    <pre class="kdl" data-kdl aria-label="The arrow in KDL">{arrowKdl(a)}</pre>
  {/if}
  {#if a.seen?.length}
    <h3>In the code</h3>
    <ul class="seen" data-seen>{#each a.seen as at}<li>{at}</li>{/each}</ul>
  {/if}
</Panel>

<style>
  .title { flex: 1; font-family: var(--font-hand); font-size: var(--text-xl); }
  .muted { margin: var(--space-1) 0; font-size: var(--text-sm); color: var(--map-muted); }
  .kdl { margin: 0; padding: var(--space-2); background: var(--map-sunken); border-radius: var(--radius-md); font: var(--text-sm)/1.4 var(--font-mono); white-space: pre-wrap; overflow-wrap: anywhere; }
  .seen { margin: 0; padding: 0; list-style: none; font: var(--text-sm)/1.6 var(--font-mono); }
</style>
