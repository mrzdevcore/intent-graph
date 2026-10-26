<!-- Renames an element in place: a native input over its rect, so IME, selection and spell
     checking are the system's. Enter commits (never while composing), Esc cancels, blur commits.
     Its keys never reach the board. -->
<script lang="ts">
  import { onMount } from "svelte";

  let { rect, text, oncommit, oncancel }: {
    rect: [number, number, number, number];
    text: string;
    oncommit: (name: string) => void;
    oncancel: () => void;
  } = $props();

  let input: HTMLInputElement;
  let composing = false;
  let done = false;

  onMount(() => {
    input.focus();
    input.select();
  });

  function finish(commit: boolean) {
    if (done) return;
    done = true;
    if (commit) oncommit(input.value);
    else oncancel();
  }

  function onkeydown(e: KeyboardEvent) {
    e.stopPropagation();
    // WebKit sends the Enter that ends a composition after compositionend, as keyCode 229
    if (composing || e.isComposing || e.keyCode === 229) return;
    if (e.key === "Enter") {
      e.preventDefault();
      finish(true);
    } else if (e.key === "Escape") {
      e.preventDefault();
      finish(false);
    }
  }
</script>

<input bind:this={input} class="edit" value={text} aria-label="Name" {onkeydown}
  oncompositionstart={() => (composing = true)} oncompositionend={() => (composing = false)}
  onblur={() => finish(true)}
  style="left:{rect[0]}px;top:{rect[1]}px;width:{rect[2]}px;height:{rect[3]}px">

<style>
  .edit { position: absolute; box-sizing: border-box; margin: 0; padding: 0 6px; text-align: center;
    font: inherit; font-size: 12px; color: var(--fg); background: var(--panel);
    border: 2px solid var(--note); border-radius: 8px; outline: none; }
</style>
