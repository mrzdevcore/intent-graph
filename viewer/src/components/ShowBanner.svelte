<!-- An answer shown by Claude (`ig_show`) over the graph: the banner with Clear and Back, and the
     numbered step list. A step's first click selects its node, the second opens its code. -->
<script lang="ts">
  let { title, missing, steps, onclear, onback, onstep }: {
    title: string;
    /** How many of the show's ids the graph no longer has. */
    missing: number;
    steps: { n: number; target: string; text: string }[];
    onclear: () => void;
    onback: () => void;
    onstep: (target: string, again: boolean) => void;
  } = $props();

  let picked = $state<number | null>(null);
  let open = $state(true);

  function pick(n: number, target: string) {
    onstep(target, picked === n);
    picked = n;
  }
</script>

<div class="show-banner" role="region" aria-label="Shown by Claude">
  <div class="show-bar">
    <strong>Shown by Claude: {title}</strong>
    {#if missing}<span class="show-missing">{missing} {missing === 1 ? "item is" : "items are"} no longer in the graph</span>{/if}
    <span class="show-actions">
      {#if steps.length}<button aria-expanded={open} onclick={() => (open = !open)}>Steps</button>{/if}
      <button title="Remove the highlight, keep this view" onclick={onclear}>Clear</button>
      <button title="Back to the view before Claude showed this" onclick={onback}>Back</button>
    </span>
  </div>
  {#if open && steps.length}
    <ol class="show-steps">
      {#each steps as s (s.n)}
        <li><button class:picked={picked === s.n} title={picked === s.n ? "Click again to open the code" : "Select in the graph"}
          onclick={() => pick(s.n, s.target)}><span class="show-n">{s.n}</span>{s.text}</button></li>
      {/each}
    </ol>
  {/if}
</div>
