<!-- The plan being applied or checked: its title, steps, volume, findings and notes, collapsible. -->
<script lang="ts">
  import type { Note, ViewData } from "../data";
  import { selectOnBoard } from "../model/boardselect";
  import { noteLine } from "../model/notes";
  import { findingItem } from "./shared.svelte";

  let { data, known, notes, focus }: {
    data: ViewData;
    /** Ids of the nodes in the graph, to pin plan findings to. */
    known: Record<string, unknown>;
    notes: Note[];
    focus: (id: string) => void;
  } = $props();

  const pairs = (ps: [string, string][]) => (ps && ps.length ? ps.map((p) => p[0] + " → " + p[1]).join(", ") : "none");
  const counts = (o: Record<string, number>) => {
    const ks = Object.keys(o || {});
    return ks.length ? ks.map((k) => k + " " + o[k]).join(", ") : "none";
  };
</script>

{#if data.plan}
  {@const p = data.plan}
  {@const v = p.volume}
  <details open>
    <summary><h2>Plan{p.done ? " (done)" : ""}</h2></summary>
    <p><strong>{p.title}</strong></p>
    {#if p.steps.length}<ol>{#each p.steps as t}<li>{t}</li>{/each}</ol>{/if}
    {#if v}
      <ul>
        {#each [
          "new files: " + v.newFiles, "new nodes: " + counts(v.newNodes), "changed nodes: " + v.changedNodes,
          "removed nodes: " + v.removedNodes, "new systems: " + (v.newSystems.length ? v.newSystems.join(", ") : "none"),
          "new cross-layer links: " + pairs(v.newCrossLayer), "new cross-lane links: " + pairs(v.newCrossLane),
          "blast radius: " + v.blastRadius.total + " (" + counts(v.blastRadius.byLayer) + ")",
        ] as t}<li class="loc">{t}</li>{/each}
      </ul>
    {/if}
    {#if p.design}
      {@const d = p.design}
      {@const gapIds = p.findings.filter((f) => f.ruleId === "plan/design-gap" && f.nodeId).map((f) => f.nodeId!)}
      {#each [
        { title: "Not in the plan", n: d.gaps, rows: gapIds.map((id) => ({ id, why: "" })) },
        { title: "Left out", n: d.omitted.length, rows: d.omitted.map((o) => ({ id: o.id, why: o.reason })) },
        { title: "Added by the agent", n: d.added.length, rows: d.added.map((id) => ({ id, why: "" })) },
      ] as l}
        <h2>{l.title} ({l.n})</h2>
        {#if l.rows.length}
          <ul class="design-list">{#each l.rows as r}<li><button class="loc" onclick={() => selectOnBoard(r.id) || focus(r.id)}>{r.id}</button>{r.why ? " " + r.why : ""}</li>{/each}</ul>
        {:else}<p class="loc">None.</p>{/if}
      {/each}
    {/if}
    <h2>Plan findings ({p.findings.length})</h2>
    {#if p.findings.length}
      <ul>{#each p.findings as f}{@render findingItem(data, { ...f, pinned: f.nodeId && known[f.nodeId] ? f.nodeId : null }, focus)}{/each}</ul>
    {:else}
      <p class="loc">None.</p>
    {/if}
    {#if notes.length}
      <h2>Notes ({notes.filter((n) => !n.done).length} open)</h2>
      <ul class="notes">
        {#each notes as n}<li class={n.done ? "done" : ""}>
          <strong>{n.id}</strong> {n.done ? "✓ " : ""}{noteLine(n)}{#if n.reply}<div class="loc">↳ {n.reply}</div>{/if}</li>{/each}
      </ul>
    {/if}
  </details>
{/if}
