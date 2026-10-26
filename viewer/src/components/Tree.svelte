<!-- The file tree, synced with the graph: a filter, "only what is on screen", folders, files and members.
     Click selects (`onselect`), double-click focuses (`onfocus`), a folder click highlights it (`onfolder`). -->
<script lang="ts">
  import type { Index } from "../model/graph";
  import { fileTree, onScreenFiles, revealKeys, treeRows, type Row } from "../model/tree";

  let { ix, mode, screenIds, onselect, onfolder, onfocus }: {
    ix: Index;
    mode: string;
    /** The ids of the graph's nodes, after each render. */
    screenIds: string[];
    onselect: (id: string) => void;
    onfolder: (key: string) => void;
    onfocus: (id: string) => void;
  } = $props();

  let filter = $state("");
  // Off in Architecture (the boxes are whole directories), on elsewhere; the checkbox overrides it until the mode changes.
  let inView = $derived(mode !== "architecture");
  let sel = $state<string | null>(null);
  const expanded: Record<string, boolean> = $state({});
  let body: HTMLElement;

  const tree = $derived(fileTree(ix));
  const onScreen = $derived(inView ? onScreenFiles(ix, screenIds) : null);
  const rows = $derived(treeRows(ix, tree, { filter, expanded, onScreen }));

  /** Graph → tree: opens the rows down to `id` and selects it. */
  export function reveal(id: string) {
    const n = ix.byId[id];
    if (!n || n.kind === "system") return;
    revealKeys(ix, id).forEach((k) => (expanded[k] = true));
    sel = id;
  }

  $effect(() => {
    void [rows, sel];
    body.querySelector(".sel")?.scrollIntoView({ block: "nearest" });
  });

  function onRow(r: Row) {
    if (r.dir) {
      expanded[r.key] = !r.open;
      onfolder(r.key);
      return;
    }
    if (r.parent) expanded[r.key] = !r.open;
    sel = r.key;
    onselect(r.key);
  }
</script>

<input id="tree-filter" type="search" placeholder="Filter files and functions…" aria-label="Filter files and functions"
  autocomplete="off" bind:value={filter}>
<label class="loc"><input id="tree-inview" type="checkbox" checked={inView} onchange={(e) => (inView = e.currentTarget.checked)}> only what is on screen</label>
<div id="tree-body" role="tree" bind:this={body}>
  {#each rows as r}
    {@const s = !r.dir && r.key === sel}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_interactive_supports_focus -->
    <div class="row{r.dir ? ' dir' : ''}{s ? ' sel' : ''}" role="treeitem" aria-selected={s ? "true" : "false"}
        style="padding-left:{8 + r.depth * 12}px" title={r.title}
        onclick={() => onRow(r)} ondblclick={() => !r.dir && onfocus(r.key)}>
      <span class="caret">{r.parent ? (r.open ? "▾" : "▸") : ""}</span>
      {#if r.status !== "unchanged"}<span class="dot {r.status}"></span>{/if}
      <span class="name">{r.name}</span>
      {#if r.kind}<span class="kind">{r.kind}</span>{/if}
      {#if r.findings}<span class="nf">● {r.findings}</span>{/if}
    </div>
  {:else}
    <p class="loc">  Nothing matches.</p>
  {/each}
</div>
