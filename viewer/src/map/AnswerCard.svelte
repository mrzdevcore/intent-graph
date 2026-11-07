<script lang="ts">
  import type { Answer } from "./answer";

  let { answer, step, away, onstep, onback, onclear, label = "The answer", sub, note, empty }: {
    answer: Answer; step: number; away: boolean; onstep: (i: number) => void; onback: () => void; onclear: () => void;
    label?: string; sub?: string; note?: string; empty?: string;
  } = $props();
  const GLYPH = { changed: "~", added: "+", removed: "−", problem: "!" };
  let folded = $state(false);
  const count = $derived(answer.steps.length);
</script>

<section class="answer" class:folded aria-label={label}>
  <div class="head">
    {#if folded}<span class="n now">{step + 1}</span>{/if}
    <b>{answer.title}</b>
    <button type="button" class="icon" aria-label={folded ? "Expand" : "Collapse"} aria-expanded={!folded} onclick={() => (folded = !folded)}>{folded ? "⌄" : "⌃"}</button>
    <button type="button" class="icon" aria-label="Clear the answer" onclick={onclear}>×</button>
  </div>
  {#if !folded}
    {#if sub}<p class="sub">{sub}</p>{/if}
    {#if note}<p class="note">{note}</p>{/if}
    {#if empty && !count}<p class="none">{empty}</p>{/if}
    <ol>
      {#each answer.steps as s, i (i)}
        {#if s.group && s.group !== answer.steps[i - 1]?.group}<li class="grp" role="presentation">{s.group}</li>{/if}
        <li><button type="button" class="step" class:on={i === step} aria-current={i === step ? "step" : undefined} onclick={() => onstep(i)}>
          <span class="n" class:now={i === step}>{i + 1}</span>
          {#if s.kind}<span class="k {s.kind}" aria-hidden="true">{GLYPH[s.kind]}</span>{/if}
          <span class="words">
            <span class="title" class:gone={s.gone}>{s.title}{#if s.meta}<span class="meta">{" · " + s.meta}</span>{/if}</span>
            {#if s.gone}<span class="lost">No longer on the map</span>
            {:else if i === step && s.text}<span class="text">{s.text}</span>{/if}
          </span>
        </button></li>
      {/each}
    </ol>
    <div class="foot">
      {#if away && count}<button type="button" class="back" onclick={onback}>{label === "The answer" ? "Back to the answer" : "Back to the change"}</button>
      {:else if count > 1}
        <span class="walk">
          <button type="button" aria-label="Previous step" disabled={step === 0} onclick={() => onstep(step - 1)}>‹</button>
          <span class="at">{step + 1} / {count}</span>
          <button type="button" aria-label="Next step" disabled={step === count - 1} onclick={() => onstep(step + 1)}>›</button>
        </span>
      {:else}<span></span>{/if}
      <span class="hint">{count > 1 ? "← → step · " : ""}Esc clears</span>
    </div>
  {/if}
</section>

<style>
  .answer { position: absolute; z-index: 3; left: var(--space-4); top: 5.75rem; width: 19rem; box-sizing: border-box; max-height: calc(100% - 7.5rem); overflow: auto; display: flex; flex-direction: column; gap: var(--space-2);
    padding: var(--space-3); background: var(--map-surface); color: var(--map-fg); border: 1px solid var(--map-line); border-radius: var(--radius-xl); box-shadow: 0 6px 20px var(--map-shadow); font-size: var(--text-md); overflow-wrap: anywhere; }
  .answer.folded { padding: var(--space-1-5) var(--space-3); }
  .head { display: flex; align-items: center; gap: var(--space-2); }
  .head b { flex: 1; min-width: 0; font-size: var(--text-lg); font-weight: 600; }
  .folded .head b { font-size: var(--text-md); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  button { font: inherit; color: inherit; cursor: pointer; }
  .icon { flex: none; border: 0; background: none; padding: 0 var(--space-0-5); color: var(--map-muted); font-size: var(--text-3xl); line-height: 1; }
  ol { margin: 0; padding: 0; list-style: none; display: flex; flex-direction: column; gap: var(--space-0-5); }
  .step { display: flex; align-items: flex-start; gap: var(--space-2-5); width: 100%; box-sizing: border-box; padding: var(--space-2); border: 0; border-radius: var(--radius-md); background: none; text-align: left; }
  .step:hover, .step.on { background: var(--map-sunken); }
  .n { flex: none; width: 1.375rem; height: 1.375rem; line-height: 1.375rem; border-radius: 50%; text-align: center; font: 700 var(--text-sm)/1.375rem var(--font-mono); background: var(--map-sunken); color: var(--map-muted); }
  .step.on .n, .step:hover .n { background: var(--map-track); }
  .n.now, .step.on .n.now { background: var(--map-trace); color: var(--color-on-accent); }
  .words { min-width: 0; display: flex; flex-direction: column; gap: var(--space-0-5); }
  .title { line-height: 1.25rem; }
  .step.on .title { font-weight: 600; }
  .gone { text-decoration: line-through; color: var(--map-muted); }
  .text { color: var(--map-fg-2); line-height: 1.125rem; }
  .lost { font-size: var(--text-sm); color: var(--map-warn-text); }
  .sub { margin: 0; font: var(--text-sm)/1.4 var(--font-mono); color: var(--map-fg-2); }
  .note, .none { margin: 0; font-size: var(--text-sm); color: var(--map-muted); }
  .none { font-size: var(--text-md); color: var(--map-fg); }
  .grp { padding: var(--space-1-5) var(--space-2) 0; font-size: var(--text-xs); font-weight: 600; letter-spacing: 0.06em; text-transform: uppercase; color: var(--map-muted); }
  .k { flex: none; width: 0.75rem; text-align: center; font: 700 var(--text-md)/1.375rem var(--font-mono); }
  .k.changed { color: var(--color-changed); } .k.added { color: var(--color-added); } .k.removed, .k.problem { color: var(--color-removed); }
  .meta { color: var(--map-muted); font-weight: 400; }
  .foot { display: flex; justify-content: space-between; align-items: center; gap: var(--space-2); padding-top: var(--space-2); border-top: 1px solid var(--map-line-soft); }
  .walk { display: flex; align-items: center; gap: var(--space-1); }
  .walk button { border: 1px solid var(--map-line); background: var(--map-surface); border-radius: var(--radius-md); padding: var(--space-0-5) var(--space-2-5); }
  .walk button:disabled { opacity: 0.4; cursor: default; }
  .at { padding: 0 var(--space-1-5); font: var(--text-sm) var(--font-mono); color: var(--map-muted); }
  .hint { font-size: var(--text-xs); color: var(--map-faint); }
  .back { border: 1px solid var(--map-trace); background: var(--map-trace); color: var(--color-on-accent); font-weight: 600; border-radius: var(--radius-md); padding: var(--space-1) var(--space-3); }
  button:focus-visible { outline: 2px solid var(--map-trace); outline-offset: 2px; }
  @media (max-width: 47.5rem) { .answer { left: var(--space-4); right: var(--space-4); width: auto; top: auto; bottom: 3.5rem; max-height: 40%; } }
</style>
