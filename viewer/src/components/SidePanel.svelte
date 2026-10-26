<!-- The side panel: the overview (notices, plan, intent changes, findings, intents), a group, a node
     (code diff, calls down, findings, edges) or, in Blueprint, the journey list. -->
<script module lang="ts">
  export type SideView = { kind: "overview" } | { kind: "journeys" } | { kind: "group"; id: string } | { kind: "node"; id: string };
  /** What the panel asks <App> to do. */
  export type SideActions = {
    focus: (id: string) => void;
    overview: () => void;
    depth: (id: string) => void;
    arch: (id: string) => void;
    journey: (id: string) => void;
    uiOnly: (on: boolean) => void;
    /** The before/after merge view of a changed node. */
    compare: (id: string) => void;
    /** Desktop app: the node's file in the code view. */
    openFile?: (id: string) => void;
    /** Desktop app: the Design view, its context around this node. */
    design?: (id: string) => void;
  };
</script>

<script lang="ts">
  import type { Note, ViewEdge, ViewFinding, ViewNode } from "../data";
  import { ancestors, type Index } from "../model/graph";
  import { journeyName, journeys } from "../model/blueprint";
  import { callChain, chainToMermaid, chainToPlantUml, stepRows } from "../model/callchain";
  import { codeBlock, openOverlay, stepList, type CodeRow } from "./Overlay.svelte";
  import PlanPanel from "./PlanPanel.svelte";
  import { copyButton, editorLink, findingItem } from "./shared.svelte";

  let { ix, view, mode, focusId, focusDepth, journey, uiOnly, notes, on }: {
    ix: Index;
    view: SideView;
    mode: string;
    focusId: string | null;
    focusDepth: number;
    journey: string | null;
    uiOnly: boolean;
    /** Plan notes from `ig serve`; empty when not live. */
    notes: Note[];
    on: SideActions;
  } = $props();

  const D = $derived(ix.D);
  const mark = { added: "+", changed: "~", removed: "-", unchanged: "" };
  let chainDepth = $state(8);

  const back = (e: Event) => {
    e.preventDefault();
    on.overview();
  };

  function groupFindings(gid: string) {
    const g = ix.groups[gid];
    return ix.D.findings.filter((f) => {
      const n = f.pinned && ix.byId[f.pinned];
      return n && ancestors(ix, ix.treeOf[n.file]).indexOf(g) >= 0;
    });
  }

  /** The function's code: its diff when it changed, its text when it is new or removed. */
  function codeRows(n: ViewNode): CodeRow[] {
    const rows: CodeRow[] = [];
    if (n.diff) n.diff.forEach((d) => rows.push([d[0], d[0] === "-" ? d[1] : d[2], d[3]]));
    else if (n.code) n.code.text.split("\n").forEach((t, i) => rows.push([n.code!.op, n.code!.line + i, t]));
    return rows;
  }

  function setDepth(d: number) {
    chainDepth = Math.min(10, Math.max(1, chainDepth + d));
  }

  /** Journeys with the app heading before the first of each app. */
  function journeyRows() {
    let app: string | null = null;
    return journeys(ix).map((n) => {
      const jn = journeyName(n.file);
      const head = jn.app !== app ? jn.app || "pages" : null;
      app = jn.app;
      return { n, jn, head };
    });
  }
</script>

{#snippet findings(fs: ViewFinding[])}
  <ul>{#each fs as f}{@render findingItem(D, f, on.focus)}{/each}</ul>
{/snippet}

{#snippet backLink()}
  <!-- svelte-ignore a11y_invalid_attribute -->
  <p><a href="#" onclick={back}>← overview</a></p>
{/snippet}

{#snippet overview()}
  {#each D.notices as n}<p class="notice">{n}</p>{/each}
  {#if D.plan}<PlanPanel data={D} known={ix.byId} {notes} focus={on.focus} />{/if}
  {#if D.intentChanges.length}
    <h2>Intent changes</h2><ul>{#each D.intentChanges as c}<li class={c.status}>{mark[c.status] + " " + c.id + ": " + c.text}</li>{/each}</ul>
  {/if}
  <h2>Findings ({D.findings.length})</h2>
  {#if D.findings.length}{@render findings(D.findings)}{:else}<p class="loc">None.</p>{/if}
  {#if D.intents.length}
    <h2>Intents</h2>
    <ul>
      {#each D.intents as i}
        <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
        <li class="f" onclick={() => { const t = i.implementedBy[0] || i.testedBy[0]; if (t) on.focus(t); }}><strong>{i.id + " "}</strong><span>{i.text}</span>
          <div class="loc">{(i.implementedBy.length ? "implemented" : "not implemented") + " · " + (i.testedBy.length ? "tested" : "untested")}</div></li>
      {/each}
    </ul>
  {/if}
{/snippet}

{#snippet group(gid: string)}
  {@const g = ix.groups[gid]}
  {#if g}
    {@const fs = groupFindings(gid)}
    {@render backLink()}<h2>{g.layer ? "layer" : "directory"}</h2>
    <p><strong>{g.label}</strong> <span class="loc">{g.count} modules</span></p>
    {#if fs.length}<h2>Findings</h2>{@render findings(fs)}{/if}
  {/if}
{/snippet}

{#snippet code(n: ViewNode)}
  {@const rows = codeRows(n)}
  {#if rows.length}
    <h2>{n.diff ? "Code (before → after)" : n.code!.op === "+" ? "Code (new)" : "Code (removed)"}</h2>
    <div class="btns"><button onclick={() => openOverlay({ heading: n.name + " · " + n.file + ":" + n.line, code: rows })}>Open wide</button>{#if n.diff}<button onclick={() => on.compare(n.id)}>Merge view</button>{/if}{#if on.openFile}<button onclick={() => on.openFile!(n.id)}>Open file</button>{/if}</div>
    {@render codeBlock(rows)}
  {/if}
{/snippet}

{#snippet callsDown(id: string)}
  {@const c = callChain(ix, id, { depth: chainDepth })}
  {#snippet stepper()}
    <span class="loc">depth {chainDepth}</span>
    <button disabled={chainDepth <= 1} onclick={() => setDepth(-1)}>−</button>
    <button disabled={chainDepth >= 10} onclick={() => setDepth(1)}>+</button>
  {/snippet}
  {#if !c.steps.length}
    <h2>Calls down</h2><p class="loc">No calls from here</p><div class="btns">{@render stepper()}</div>
  {:else}
    {@const mm = chainToMermaid(c)}
    {@const pu = chainToPlantUml(c)}
    {@const rows = stepRows(c)}
    <h2>Calls down ({c.steps.length} steps)</h2>
    <div class="btns">
      <button onclick={() => openOverlay({ heading: "Calls down from " + ix.byId[id].name, mermaid: mm, plantUml: pu, steps: rows, truncated: c.truncated, onPick: on.focus })}>Show diagram</button>{@render copyButton("Copy Mermaid", mm)}{@render copyButton("Copy PlantUML", pu)}{@render stepper()}
    </div>
    {@render stepList(rows, on.focus, c.truncated)}
  {/if}
{/snippet}

{#snippet edgeList(title: string, list: ViewEdge[], key: "from" | "to")}
  {#if list.length}
    <h2>{title} ({list.length})</h2>
    <ul>
      {#each list.slice(0, 50) as e}
        {@const o = ix.byId[e[key]] || { name: e[key], file: "" }}
        <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
        <li class="f" onclick={() => on.focus(e[key])}><span>{e.kind + " "}</span><strong>{o.name}</strong>{#if e.violation}<span class="sev error">{" violation"}</span>{/if}{#if e.planned}<span class="tag">planned</span>{/if}<div class="loc">{o.file}</div></li>
      {/each}
    </ul>
  {/if}
{/snippet}

{#snippet node(id: string)}
  {@const n = ix.byId[id]}
  {#if !n}
    {@render overview()}
  {:else}
    {@const fs = n.module ? D.findings.filter((f) => f.file === n.file) : ix.findingsByNode[id] || []}
    {@render backLink()}
    <h2>{n.kind + (n.layer ? " · " + n.layer : "")}</h2>
    <p><strong>{n.name}</strong> <span class={n.status}>{n.status}</span>{#if n.planned}<span class="tag">planned</span>{/if}</p>
    {#if !n.planned}
      <div class="loc">{@render editorLink(D, n.file, n.line)}</div><div class="loc">complexity {n.complexity} · {n.loc} lines</div>
    {/if}
    <div class="btns">
      <button onclick={() => on.focus(id)}>Focus</button>
      {#if mode === "focus" && focusId === id}<button onclick={() => on.depth(id)}>{focusDepth === 1 ? "Depth 2" : "Depth 1"}</button>{/if}
      {#if mode !== "architecture"}<button onclick={() => on.arch(id)}>Show in architecture</button>{/if}
      {#if __BOARDS__ && on.design}<button title="Open the Design view with the modules around this one as context" onclick={() => on.design!(id)}>Design around this</button>{/if}
    </div>
    {#if n.intents.length}<p>{#each n.intents as t}<span class="tag">{t}</span>{/each}</p>{/if}
    {#if n.planned}<p class="loc">planned, not written yet</p>{:else}{@render code(n)}{/if}
    {@render callsDown(id)}
    {#if fs.length}<h2>Findings</h2>{@render findings(fs)}{/if}
    {@render edgeList("Used by", D.edges.filter((e) => e.to === id && e.from !== id), "from")}
    {@render edgeList("Uses", D.edges.filter((e) => e.from === id && e.to !== id), "to")}
  {/if}
{/snippet}

{#snippet journeyList()}
  {#if !D.lanes.length}
    <h2>Journeys</h2><p class="loc">Add a blueprint section to ig.yaml (lanes, http, systems) to see journeys.</p>
  {:else}
    {@const js = journeyRows()}
    {#if !js.length}
      <h2>Journeys</h2><p class="loc">No module is in the first lane ({D.lanes[0]}).</p>
    {:else}
      <h2>Journeys</h2>
      <label class="loc"><input type="checkbox" checked={uiOnly} onchange={(ev) => on.uiOnly(ev.currentTarget.checked)}> show UI-only components</label>
      {#each js as { n, jn, head }}
        {#if head !== null}<h2>{head}</h2>{/if}
        <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
        <ul><li class="f{n.id === journey ? ' current' : ''}" onclick={() => on.journey(n.id)}><strong>{jn.route}</strong></li></ul>
      {/each}
    {/if}
  {/if}
{/snippet}

<!-- A fresh DOM per view, so buttons ("Copied") start over. -->
{#key view.kind + ":" + ("id" in view ? view.id : "")}
  {#if view.kind === "journeys"}{@render journeyList()}
  {:else if view.kind === "group"}{@render group(view.id)}
  {:else if view.kind === "node"}{@render node(view.id)}
  {:else}{@render overview()}{/if}
{/key}
