<!-- The add-box form (EditAdd artboard), anchored at the click. Enter adds, Esc closes; the inputs handle Esc
     themselves because the map's key handler ignores keys while typing. Text only. -->
<script lang="ts">
  import { onMount } from "svelte";
  import type { BoxKind } from "../types";
  import { CONTAINER_KINDS, NO_CODE, SYSTEM_KINDS, prefill } from "./add";

  let { parentTitle, system, globs, x, y, onadd, onclose }: {
    parentTitle: string; system: boolean; globs: string[]; x: number; y: number;
    onadd: (a: { kind: BoxKind; name: string; at: string[] }) => void; onclose: () => void;
  } = $props();

  const kinds = $derived(system ? SYSTEM_KINDS : CONTAINER_KINDS);
  let kind = $state<BoxKind>("app");
  $effect.pre(() => { if (!kinds.includes(kind)) kind = kinds[0]; });
  let name = $state("");
  let typed = $state<string | null>(null); // the code path once edited by hand
  const at = $derived(typed ?? prefill(globs, name));
  const code = $derived(!NO_CODE.includes(kind));
  let field: HTMLInputElement;
  onMount(() => field.focus());

  function submit(e?: Event) {
    e?.preventDefault();
    const n = name.trim();
    if (!n) return field.focus();
    const a = at.trim();
    onadd({ kind, name: n, at: code && a ? [a] : [] });
  }
  function keydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      onclose();
    } else if (e.key === "Enter" && (e.target as HTMLElement).tagName === "SELECT") submit(e);
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<form aria-label="New box in {parentTitle}" style:left="{x}px" style:top="{y}px" onsubmit={submit} onkeydown={keydown}>
  <b>New box in {parentTitle}</b>
  <label>Kind
    <select value={kind} onchange={(e) => (kind = e.currentTarget.value as BoxKind)}>{#each kinds as k (k)}<option value={k}>{k}</option>{/each}</select>
  </label>
  <label>Name
    <input bind:this={field} bind:value={name} required autocomplete="off" spellcheck="false" />
  </label>
  {#if code}
    <label>Code lives at
      <input value={at} oninput={(e) => (typed = e.currentTarget.value)} autocomplete="off" spellcheck="false" />
    </label>
    <small>Filled from the name; change it if the code will live elsewhere.</small>
  {/if}
  <div class="row">
    <button type="button" onclick={onclose}>Cancel <kbd>Esc</kbd></button>
    <button type="submit" class="go">Add <kbd>↵</kbd></button>
  </div>
</form>

<style>
  form { position: absolute; z-index: 5; width: 18.75rem; max-width: calc(100% - 1rem); box-sizing: border-box; display: flex; flex-direction: column; gap: var(--space-2-5);
    padding: var(--space-3); background: var(--map-surface); border: 1px solid var(--map-line); border-radius: var(--radius-xl); box-shadow: 0 0.5rem 1.5rem var(--map-shadow); color: var(--map-fg); font-size: var(--text-md); }
  label { display: flex; flex-direction: column; gap: var(--space-1); font-size: var(--text-sm); color: var(--map-muted); }
  input, select { font: inherit; font-size: var(--text-md); color: var(--map-fg); background: var(--map-sunken); border: 1px solid var(--map-line); border-radius: var(--radius-md); padding: var(--space-1-5) var(--space-2); }
  small { font-size: var(--text-sm); color: var(--map-faint); }
  .row { display: flex; justify-content: flex-end; gap: var(--space-1-5); }
  button { font: inherit; font-size: var(--text-md); border: 1px solid var(--map-line); background: var(--map-surface); color: var(--map-fg); border-radius: var(--radius-lg); padding: var(--space-1) var(--space-2-5); min-height: 2rem; cursor: pointer; }
  .go { border-color: var(--map-gold); background: var(--map-tint-gold); }
  kbd { font: inherit; font-size: var(--text-sm); color: var(--map-faint); }
  input:focus-visible, select:focus-visible, button:focus-visible { outline: 2px solid var(--map-trace); outline-offset: 2px; }
</style>
