<!-- The architecture board's inspector: a new element's fields, a link's kind, an existing box's
     marks, or what the tools do. Every change is one `ops` call (one undo step). -->
<script lang="ts">
  import type { Design, LinkKind, MarkKind } from "../api";
  import type { Index } from "../model/graph";

  let { ix, doc, selection, boxes, reasons, ops, cmd }: {
    ix: Index;
    doc: Design | null;
    selection: string[];
    /** The context boxes by id. */
    boxes: Map<string, { id: string; label: string; kind: string }>;
    reasons: Record<string, string>;
    ops: (list: object[]) => unknown;
    /** "Cmd" or "Ctrl". */
    cmd: string;
  } = $props();

  const LINK_KINDS: LinkKind[] = ["calls", "imports", "renders", "http", "uses"];
  const METHODS = ["GET", "POST", "PUT", "PATCH", "DELETE"];

  const sel = $derived(selection.length === 1 ? selection[0] : null);
  const element = $derived(sel ? doc?.elements.find((e) => e.id === sel) : undefined);
  const link = $derived(sel ? doc?.links.find((l) => `${l.from}|${l.kind}|${l.to}` === sel) : undefined);
  const box = $derived(sel ? boxes.get(sel) : undefined);
  const mark = $derived(sel ? doc?.marks.find((m) => m.node === sel)?.mark : undefined);

  const update = (id: string, set: Record<string, unknown>) => ops([{ op: "update", id, set }]);
  const setMark = (node: string, m: MarkKind | null) => ops([{ op: "mark", node, mark: m }]);
</script>

<aside class="inspector" aria-label="Inspector">
  {#if element}
    {#key element.id}
      <h3>{element.kind === "system" ? "External system" : element.kind[0].toUpperCase() + element.kind.slice(1)} <span class="new">new</span></h3>
      <label>Id <input readonly value={element.id}></label>
      <label>Name <input value={element.name} onchange={(e) => update(element.id, { name: e.currentTarget.value })}></label>
      {#if element.kind === "route"}
        <label>Method <select value={element.method ?? ""} onchange={(e) => update(element.id, { method: e.currentTarget.value })}>
          <option value="">—</option>{#each METHODS as m (m)}<option value={m}>{m}</option>{/each}</select></label>
        <label>Path <input value={element.path ?? ""} placeholder="/api/…" onchange={(e) => update(element.id, { path: e.currentTarget.value })}></label>
      {/if}
      {#if element.kind !== "note" && element.kind !== "frame" && element.kind !== "system"}
        <label>File <input value={element.file ?? ""} placeholder="proposed from the layer" title="Proposed from the layer's files and the name; edit it freely"
          onchange={(e) => update(element.id, { file: e.currentTarget.value })}></label>
      {/if}
      <p class="muted">Layer: {element.layer ?? "none (drop it in a row)"}</p>
      {#if element.kind === "note"}
        <label class="row"><input type="checkbox" checked={!!element.question} onchange={(e) => update(element.id, { question: e.currentTarget.checked })}> A question</label>
        {#if element.question}<label>Answer <textarea value={element.answer ?? ""} onchange={(e) => update(element.id, { answer: e.currentTarget.value })}></textarea></label>{/if}
      {/if}
      {#if reasons[element.id]}<p class="problem">{reasons[element.id]}</p>{/if}
    {/key}
  {:else if link}
    <h3>Link</h3>
    <p><code>{link.from}</code> → <code>{link.to}</code></p>
    <label>Kind <select value={link.kind} onchange={(e) => ops([{ op: "unlink", key: sel }, { op: "link", from: link.from, to: link.to, kind: e.currentTarget.value }])}>
      {#each LINK_KINDS as k (k)}<option value={k}>{k}</option>{/each}</select></label>
    {#if reasons[sel!]}<p class="problem">{reasons[sel!]}</p>{/if}
  {:else if box}
    <h3>{box.label} <span class="muted">existing</span></h3>
    <p class="muted"><code>{box.id}</code></p>
    {#if ix.byId[box.id]}
      <div class="marks" role="group" aria-label="Mark">
        <button aria-pressed={mark === "changed" ? "true" : "false"} onclick={() => setMark(box.id, mark === "changed" ? null : "changed")}>Changed</button>
        <button aria-pressed={mark === "remove" ? "true" : "false"} onclick={() => setMark(box.id, mark === "remove" ? null : "remove")}>To remove</button>
      </div>
      <p class="muted">Link to it with L; it stays where it is.</p>
    {:else}
      <p class="muted">A group: pick its layer or folder as the context to mark what is inside.</p>
    {/if}
    {#if reasons[box.id]}<p class="problem">{reasons[box.id]}</p>{/if}
  {:else if selection.length > 1}
    <h3>{selection.length} selected</h3>
    <p class="muted">Arrows nudge (Shift: 40 px), Delete removes, {cmd}+D duplicates.</p>
  {:else}
    <h3>Inspector</h3>
    <p class="muted">Pick a tool, then click in a row: the box joins that layer. L, then drag from one box to another, links them. Double-click or Enter renames.</p>
  {/if}
</aside>

<style>
  .inspector { border-left: 1px solid var(--line); background: var(--panel); padding: 10px; overflow: auto; font-size: 13px; }
  .inspector h3 { margin: 0 0 8px; font-size: 14px; }
  .inspector label { display: flex; flex-direction: column; gap: 2px; margin-bottom: 8px; color: var(--muted); }
  .inspector label.row { flex-direction: row; align-items: center; gap: 6px; color: var(--fg); }
  .inspector input:not([type]), .inspector select, .inspector textarea { color: var(--fg); background: var(--bg); border: 1px solid var(--line);
    border-radius: 4px; padding: 3px 6px; font: inherit; }
  .inspector input[readonly] { color: var(--muted); font-family: ui-monospace, monospace; }
  .new { color: var(--added); font-size: 11px; font-weight: normal; border: 1px dashed var(--added); border-radius: 4px; padding: 0 4px; }
  .marks { display: flex; gap: 6px; margin-bottom: 8px; }
  .marks button[aria-pressed="true"] { background: var(--changed); color: var(--on-accent); }
  .muted { color: var(--muted); }
  .problem { color: var(--error); }
</style>
