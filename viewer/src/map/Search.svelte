<!-- The map's search field: "/" focuses it. A word lists the boxes it names (Enter or a click opens
     one); "A to B" lists the chain of arrows from A to B, and Enter lights it on the map. Esc clears. -->
<script lang="ts">
  import type { View } from "./types";
  import { ends, find, trace, type Trace } from "./search";

  let { view, onopen, ontrace }: {
    view: View; onopen: (id: string) => void; ontrace: (t: Trace | null) => void;
  } = $props();

  let q = $state("");
  let focused = $state(false);
  let active = $state(0);
  let input: HTMLInputElement;

  const pair = $derived(ends(q));
  const hits = $derived(pair ? [] : find(view, q));
  const tips = $derived(pair ? pair.map((e) => find(view, e, 1)[0]) : []);
  const chain = $derived(tips[0] && tips[1] ? trace(view, tips[0].box.id, tips[1].box.id) : null);
  const title = (id: string) => view.boxes.find((b) => b.id === id)?.title.value ?? id;
  const missing = $derived(pair?.find((_, i) => !tips[i]));
  $effect(() => {
    void q;
    active = 0;
  });

  function open(id: string) {
    onopen(id);
    q = "";
    input.blur();
  }
  function clear() {
    q = "";
    ontrace(null);
    input.blur();
  }
  function keydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.stopPropagation();
      clear();
    } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      if (hits.length) active = (active + (e.key === "ArrowDown" ? 1 : hits.length - 1)) % hits.length;
    } else if (e.key === "Enter") {
      if (pair) ontrace(chain);
      else if (hits[active]) open(hits[active].box.id);
    }
  }
  // "/" anywhere but in a field
  function slash(e: KeyboardEvent) {
    const t = e.target as HTMLElement | null;
    if (e.key !== "/" || t?.closest?.("input, textarea, select, [contenteditable]")) return;
    e.preventDefault();
    input.focus();
  }
</script>

<svelte:window onkeydown={slash} />

<div class="search" onfocusin={() => (focused = true)}
  onfocusout={(e) => { if (!e.currentTarget.contains(e.relatedTarget as Node | null)) focused = false; }}>
  <label class="field">
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true"><circle cx="11" cy="11" r="7" /><path d="M20 20l-4-4" /></svg>
    <input bind:this={input} bind:value={q} onkeydown={keydown} placeholder="Search, or A to B" aria-label="Search the map" autocomplete="off" spellcheck="false" />
    <kbd>{q ? "Esc" : "/"}</kbd>
  </label>
  {#if focused && q.trim()}
    <div class="drop">
      {#if pair}
        {#if missing}
          <p class="note">No box matches “{missing}”.</p>
        {:else if !chain}
          <p class="note">Nothing joins {tips[0]!.box.title.value} and {tips[1]!.box.title.value}.</p>
        {:else}
          <p class="note">{chain.reversed ? "The other way round: " : ""}{chain.arrows.length} {chain.arrows.length === 1 ? "step" : "steps"} · Enter shows it</p>
          <ol>
            {#each chain.boxes as id, i}
              {@const a = chain.arrows[i]}
              <li><button type="button" onclick={() => open(id)}>{title(id)}</button>{#if a}<span class="muted"> {a.label ?? a.kind}{a.count > 1 ? ` ×${a.count}` : ""}</span>{/if}</li>
            {/each}
          </ol>
        {/if}
      {:else if hits.length}
        <ul>
          {#each hits as h, i (h.box.id)}
            <li><button type="button" class:active={i === active} onclick={() => open(h.box.id)} onpointerenter={() => (active = i)}>
              <span><b>{h.box.title.value}</b> <span class="muted">{[h.box.kind.value, h.box.tech?.value].filter(Boolean).join(" · ")}</span></span>
              {#if h.where || h.path}<span class="muted">{[h.where, h.path].filter(Boolean).join(" · ")}</span>{/if}
            </button></li>
          {/each}
        </ul>
        <p class="note">Enter opens it · <b>A to B</b> shows a path</p>
      {:else}
        <p class="note">No box matches “{q.trim()}”.</p>
      {/if}
    </div>
  {/if}
</div>

<style>
  .search { position: relative; width: 17rem; }
  .field { display: flex; align-items: center; gap: var(--space-2); border: 1px solid var(--map-line); background: var(--map-surface);
    color: var(--map-muted); border-radius: var(--radius-lg); padding: 0.1875rem var(--space-2-5); }
  .field:focus-within { border-color: var(--map-trace); box-shadow: 0 0 0 3px color-mix(in srgb, var(--map-trace) 25%, transparent); }
  input { flex: 1; min-width: 0; border: 0; outline: 0; background: none; font: inherit; font-size: var(--text-md); color: var(--map-fg); }
  input::placeholder { color: var(--map-muted); }
  kbd { font: var(--text-xs) var(--font-sans); border: 1px solid var(--map-line); border-radius: var(--radius-sm); padding: 0 var(--space-1); }
  .drop { position: absolute; top: calc(100% + var(--space-1)); left: 0; right: 0; z-index: 2; max-height: 60vh; overflow: auto;
    background: var(--map-surface); color: var(--map-fg); border: 1px solid var(--map-line); border-radius: var(--radius-lg);
    padding: var(--space-1); box-shadow: 0 6px 20px var(--map-shadow); font-size: var(--text-md); overflow-wrap: anywhere; }
  ul, ol { margin: 0; padding: 0; }
  ul { list-style: none; }
  ol { padding-left: var(--space-6); margin-bottom: var(--space-1); }
  button { font: inherit; color: inherit; text-align: left; border: 0; background: none; cursor: pointer; border-radius: var(--radius-md); }
  ul button { display: flex; flex-direction: column; width: 100%; padding: var(--space-1-5) var(--space-2-5); }
  ul button.active, ul button:hover { background: var(--map-sunken); }
  ol button { padding: var(--space-0-5) var(--space-1); text-decoration: underline; text-decoration-color: var(--map-line); }
  .muted { color: var(--map-muted); font-size: var(--text-sm); }
  .note { margin: var(--space-1) var(--space-2-5) var(--space-1-5); color: var(--map-muted); font-size: var(--text-sm); }
</style>
