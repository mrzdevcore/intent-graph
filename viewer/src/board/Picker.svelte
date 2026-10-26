<!-- Link mode's search, on top of the docked architecture: type to filter (names starting with
     it first, then the design's elements), arrows move, Enter links, Esc closes. -->
<script lang="ts">
  let { items, piece, onpick, onclose }: {
    items: (q: string) => { id: string; label: string; kind: string }[];
    /** The piece being linked, in words. */
    piece: string;
    onpick: (id: string) => void;
    onclose: () => void;
  } = $props();

  let q = $state("");
  let at = $state(0);
  const list = $derived(items(q));
  const active = $derived(list[Math.min(at, list.length - 1)]);

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Escape") onclose();
    else if (e.key === "Enter" && active) onpick(active.id);
    else if (e.key === "ArrowDown") at = Math.min(at + 1, list.length - 1);
    else if (e.key === "ArrowUp") at = Math.max(at - 1, 0);
    else return;
    e.preventDefault();
  }
</script>

<div class="picker" role="dialog" aria-label="Link {piece} to the architecture">
  <!-- svelte-ignore a11y_autofocus -->
  <input bind:value={q} autofocus placeholder="Link {piece} to…" aria-label="Filter link targets" role="combobox"
    aria-expanded="true" aria-controls="bridge-targets" aria-activedescendant={active ? "bridge-" + list.indexOf(active) : undefined}
    oninput={() => (at = 0)} {onkeydown}>
  <ul id="bridge-targets" role="listbox">
    {#each list as it, i (it.id)}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <li id="bridge-{i}" role="option" aria-selected={it === active} onmousedown={(e) => (e.preventDefault(), onpick(it.id))}>
        <span class="kind">{it.kind}</span> {it.label} {#if it.label !== it.id}<code>{it.id}</code>{/if}
      </li>
    {:else}
      <li class="none">Nothing matches.</li>
    {/each}
  </ul>
</div>

<style>
  .picker { position: absolute; top: 8px; left: 50%; transform: translateX(-50%); z-index: 5; width: min(420px, 92%);
    background: var(--panel); border: 1px solid var(--line); border-radius: 6px; box-shadow: 0 4px 16px rgb(0 0 0 / 0.2); font-size: 13px; }
  input { width: 100%; box-sizing: border-box; padding: 8px 10px; border: 0; border-bottom: 1px solid var(--line); background: var(--bg);
    color: var(--fg); font: inherit; border-radius: 6px 6px 0 0; }
  ul { list-style: none; margin: 0; padding: 4px; max-height: 320px; overflow: auto; }
  li { padding: 4px 8px; border-radius: 4px; cursor: pointer; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  li[aria-selected="true"] { background: var(--accent); color: var(--on-accent); }
  .kind { display: inline-block; min-width: 5em; color: inherit; opacity: 0.7; }
  code { opacity: 0.7; font-size: 11px; }
  .none { color: var(--muted); cursor: default; }
</style>
