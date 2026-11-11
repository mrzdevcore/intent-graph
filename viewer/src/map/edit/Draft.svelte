<!-- A box being drawn, before it is sent: a dashed outline with "no kind yet" in its band and its name typed in place.
     Enter confirms, Tab confirms and asks for the kind, Esc confirms a typed name and drops an empty draft; a click away in the map confirms (the window losing focus does not). -->
<script lang="ts">
  let { x, y, w, h, editing, title, ontitle, onfinish }: {
    x: number; y: number; w: number; h: number; editing: boolean; title: string;
    ontitle: (v: string) => void; onfinish: (then: "kinds" | null) => void;
  } = $props();
  let field = $state<HTMLInputElement>();
  $effect(() => {
    if (editing) field?.focus({ preventScroll: true });
  });
  function keydown(e: KeyboardEvent) {
    if (e.key !== "Enter" && e.key !== "Tab" && e.key !== "Escape") return;
    e.preventDefault();
    e.stopPropagation();
    onfinish(e.key === "Tab" ? "kinds" : null);
  }
</script>

<div class="draft" data-draft data-popover style:left="{x}px" style:top="{y}px" style:width="{w}px" style:height="{h}px">
  <span class="band">no kind yet</span>
  {#if editing}
    <input bind:this={field} value={title} oninput={(e) => ontitle(e.currentTarget.value)} onkeydown={keydown}
      aria-label="Name the box" placeholder="Name" autocomplete="off" spellcheck="false" />
  {/if}
</div>

<style>
  .draft { position: absolute; z-index: 2; box-sizing: border-box; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 6px;
    padding: 28px 12px 12px; pointer-events: auto; cursor: text; background: var(--map-surface); border: 1.5px dashed var(--map-edge); border-radius: 10px; }
  .band { position: absolute; left: 0; right: 0; top: 0; padding: 4px 10px; font: 600 11px/14px var(--font-sans); color: var(--map-faint);
    border-bottom: 1px dashed var(--map-line-soft); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  input { width: 100%; min-width: 0; box-sizing: border-box; border: 0; padding: 0; background: none; outline: none; text-align: center;
    font: 19px/24px var(--font-hand); color: var(--map-fg); caret-color: var(--map-trace); }
  input::placeholder { color: var(--map-faint); }
</style>
