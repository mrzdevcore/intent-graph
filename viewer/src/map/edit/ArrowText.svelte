<!-- The arrow's label or note, typed in place at its middle. Enter or leaving the field keeps, Esc cancels. -->
<script lang="ts">
  import { onMount } from "svelte";
  let { value, what, x, y, max, onsubmit, onclose }: {
    value: string; what: "label" | "note"; x: number; y: number; max?: number; onsubmit: (v: string | null) => void; onclose: () => void;
  } = $props();
  let v = $state(""), done = false, field: HTMLInputElement;
  onMount(() => { v = value; field.focus(); field.select(); });
  const finish = (fn: () => void) => { if (!done) { done = true; fn(); } };
  const keep = () => finish(() => onsubmit(v.trim() || null));
  function keydown(e: KeyboardEvent) {
    if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); finish(onclose); }
    else if (e.key === "Enter") { e.preventDefault(); keep(); }
  }
</script>

<input data-popover data-arrow-text={what} class={what} bind:this={field} bind:value={v} maxlength={max} aria-label={what === "label" ? "Arrow label" : "Note on the arrow"}
  placeholder={what === "label" ? "Label" : "A short note"} autocomplete="off" spellcheck="false" style:left="{x}px" style:top="{y}px" onkeydown={keydown} onblur={keep} />

<style>
  input { position: absolute; z-index: 6; translate: -50% -50%; box-sizing: border-box; width: 14rem; font-family: var(--font-hand); font-size: var(--text-md); text-align: center;
    color: var(--map-fg); background: var(--map-surface); border: 1px solid var(--map-trace); border-radius: var(--radius-md); padding: var(--space-0-5) var(--space-2); }
  input.note { translate: -50% 0; margin-top: var(--space-3); font-size: var(--text-sm); color: var(--map-muted); }
  input:focus-visible { outline: 2px solid var(--map-trace); outline-offset: 2px; }
</style>
