<!-- A linked piece's call chain: one lifeline per layer, the piece's screen first; planned steps
     dashed, steps into changed code marked. A step opens its code, or says it is planned. Below,
     what Claude reads from the link. -->
<script lang="ts">
  import type { CallChain, Step } from "../model/callchain";

  let { chain, to, requirement, open, onunlink }: {
    /** null: the target is not in the graph (nor the design). */
    chain: CallChain | null;
    /** The bridge's target id. */
    to: string;
    requirement: string;
    /** Opens a file at a line (the Code view); absent: the location is only shown. */
    open?: (file: string, line: number) => void;
    onunlink: () => void;
  } = $props();

  /** The planned step's message (or a location without a Code view), until the chain changes. */
  let planned = $state<string | null>(null);
  $effect(() => {
    void chain;
    planned = null;
  });

  function click(s: Step) {
    if (s.planned || !s.file) planned = `Planned: ${s.file || s.target}. Click to open the code once it exists.`;
    else if (open) open(s.file, s.line);
    else planned = `${s.file}:${s.line}`;
  }
  const span = (s: Step) => `${Math.min(s.from, s.to) + 1} / ${Math.max(s.from, s.to) + 2}`;
</script>

<section class="chain" aria-label="Call chain of the link">
  <header><strong>Linked to</strong> <code>{to}</code> <button class="unlink" onclick={onunlink}>Unlink</button></header>
  {#if !chain}
    <p class="muted">Linked to {to}, which is not in the graph yet.</p>
  {:else}
    <div class="seq" style="grid-template-columns: repeat({chain.lifelines.length}, minmax(84px, 1fr)); grid-template-rows: auto repeat({chain.steps.length}, auto)">
      {#each chain.lifelines as l, i (l.key)}
        <div class="life" style="grid-column: {i + 1}" aria-hidden="true"></div>
        <div class="head" class:system={l.system} style="grid-column: {i + 1}; grid-row: 1">{l.label}</div>
      {/each}
      {#each chain.steps as s, i (s.n)}
        <button class="step" class:planned={s.planned} class:changed={s.changed} class:back={s.to < s.from}
          style="grid-column: {span(s)}; grid-row: {i + 2}; --k: {Math.abs(s.to - s.from) + 1}" onclick={() => click(s)}
          title={s.planned ? "Planned" : s.file ? `${s.file}:${s.line}` : s.target}>
          <span>{s.n}. {s.label}{#if s.changed} <em>changed</em>{/if}{#if s.planned} <em>planned</em>{/if}</span>
        </button>
      {/each}
    </div>
    {#if chain.truncated}<p class="muted">{chain.truncated} more calls not shown.</p>{/if}
    {#if planned}<p class="muted" role="status">{planned}</p>{/if}
  {/if}
  <h4>What Claude reads from this link</h4>
  <p class="req">{requirement}</p>
</section>

<style>
  /* its own row under the canvas: the canvas shrinks, never covered */
  .chain { grid-row: 3; grid-column: 2; min-width: 0; max-height: 40vh; overflow: auto; padding: 8px 10px;
    background: var(--panel); border-top: 1px solid var(--line); font-size: 12px; color: var(--fg); }
  header { display: flex; gap: 6px; align-items: center; margin-bottom: 6px; }
  .unlink { margin-left: auto; font-size: 12px; }
  .seq { display: grid; column-gap: 0; row-gap: 4px; position: relative; }
  .life { grid-row: 1 / -1; justify-self: center; width: 0; border-left: 1px dashed var(--line); }
  .head { justify-self: center; padding: 2px 6px; border: 1px solid var(--line); border-radius: 4px; background: var(--bg); text-align: center;
    max-width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .head.system { border-style: double; border-width: 3px; }
  /* an arrow from the middle of one lifeline to the middle of another */
  .step { position: relative; margin: 0 calc(50% / var(--k)); padding: 0 0 4px; border: 0; background: none; color: inherit;
    font: inherit; cursor: pointer; text-align: center; border-bottom: 2px solid var(--fg); border-radius: 0; z-index: 1; }
  .step::after { content: ""; position: absolute; right: -2px; bottom: -6px; border: 5px solid transparent; border-left-color: var(--fg); }
  .step.back::after { right: auto; left: -2px; border-left-color: transparent; border-right-color: var(--fg); }
  .step.planned { border-bottom-style: dashed; color: var(--added); }
  .step.changed span { box-shadow: inset 0 -2px 0 var(--changed); }
  .step em { font-style: normal; font-size: 10px; opacity: 0.75; }
  .step:hover span, .step:focus-visible span { text-decoration: underline; }
  h4 { margin: 8px 0 2px; font-size: 12px; color: var(--muted); font-weight: 600; }
  .req { margin: 0; }
  .muted { color: var(--muted); margin: 4px 0; }
</style>
