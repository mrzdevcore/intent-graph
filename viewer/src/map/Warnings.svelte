<!-- What reads but looks wrong in ig/ (`view.warnings`), bottom left: an amber word for what it is, then ig's
     sentence, ids in code. Never a reason the map fails. Text only. -->
<script lang="ts">
  let { warnings }: { warnings: string[] } = $props();
  // the three warnings of crates/ig-app/src/map/warn.rs
  const word = (w: string) => (/^`[^`]+` holds \d+ boxes/.test(w) ? "Crowded" : /^`[^`]+`: a .+ sits /.test(w) ? "Out of place" : "Arrow");
</script>

{#if warnings.length}
  <ul aria-label="Warnings">
    {#each warnings as w, i (i)}
      <li><span class="word">{word(w)}</span> {#each w.split("`") as part, j (j)}{#if j % 2}<code>{part}</code>{:else}{part}{/if}{/each}</li>
    {/each}
  </ul>
{/if}

<style>
  ul { position: absolute; z-index: 2; left: var(--space-4); bottom: var(--space-4); max-width: min(26rem, calc(100% - 2rem)); max-height: 30%; overflow: auto; margin: 0; padding: var(--space-1-5) var(--space-3);
    list-style: none; display: flex; flex-direction: column; gap: var(--space-1); background: var(--map-surface); border: 1px solid var(--map-line); border-radius: var(--radius-xl);
    font-size: var(--text-sm); color: var(--map-fg-2); box-shadow: 0 0.5rem 1.5rem var(--map-shadow); }
  .word { color: var(--map-warn); font-weight: 600; }
  code { background: var(--map-sunken); }
</style>
