<script lang="ts">
  let { name, label, value, help, lines = false, max, hand = false, disabled = false, placeholder, norm = lines ? (v: string) => v.replace(/^\s*\n|\s+$/g, "") || null : (v: string) => v.trim() || null, onsave }: {
    name: string; label: string; value: string | null; help?: string; lines?: boolean; max?: number; hand?: boolean; disabled?: boolean; placeholder?: string;
    norm?: (v: string) => string | null; onsave: (v: string | null, failed: () => void) => boolean;
  } = $props();
  let draft = $state(""), last: string | null = null;
  $effect(() => {
    draft = value ?? "";
    last = value;
  });
  function save() {
    const v = norm(draft), was = last;
    if (disabled || v === last) return;
    if (onsave(v, () => { if (last === v) last = was; })) last = v;
  }
  function keydown(e: KeyboardEvent) {
    if (e.key !== "Enter" || (lines && e.shiftKey) || e.isComposing) return;
    e.preventDefault();
    save();
  }
  const id = $props.id();
</script>

<div class="field">
  <label for={id}>{label}</label>
  {#if lines}
    <textarea {id} data-field={name} bind:value={draft} {disabled} {placeholder} rows="3" spellcheck="true" onblur={save} onkeydown={keydown}></textarea>
  {:else}
    <input {id} data-field={name} class:hand bind:value={draft} {disabled} {placeholder} maxlength={max} autocomplete="off" onblur={save} onkeydown={keydown} />
  {/if}
  {#if help}<p class="help">{help}</p>{/if}
</div>

<style>
  .field { display: flex; flex-direction: column; gap: var(--space-0-5); margin: var(--space-2) 0; }
  label { font-size: var(--text-xs); text-transform: uppercase; letter-spacing: 0.06em; color: var(--map-muted); }
  input, textarea { font: inherit; color: inherit; background: var(--map-sunken); border: 1px solid var(--map-line); border-radius: var(--radius-md);
    padding: var(--space-1) var(--space-2); box-sizing: border-box; width: 100%; }
  textarea { resize: vertical; min-height: 4.5rem; line-height: 1.4; }
  input:focus-visible, textarea:focus-visible { outline: 2px solid var(--map-trace); outline-offset: 1px; }
  input:disabled, textarea:disabled { color: var(--map-muted); }
  .hand { font-family: var(--font-hand); font-size: var(--text-xl); }
  .help { margin: 0; font-size: var(--text-xs); color: var(--map-muted); }
</style>
